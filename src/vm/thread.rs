//! Thread state: the per-VM-instance call stack (`Thread`), the user-facing
//! join handle (`JvmThread`), monitor bookkeeping, and the shared runtime
//! state that's parked behind a `Mutex` on `Vm`.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Condvar, Mutex};

use smallvec::SmallVec;

use super::frame::Frame;
use super::types::{ExecutionResult, Method, RuntimeClass, VmError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ClassInitializationState {
    Initializing(u64),
    Initialized,
}

#[derive(Debug, Default)]
pub(super) struct RuntimeState {
    pub(super) classes: HashMap<String, RuntimeClass>,
    pub(super) initialized_classes: HashMap<String, ClassInitializationState>,
    /// Cache of `java/lang/Class` heap references, keyed by the internal class
    /// name (e.g., `java/util/HashMap`, `I`, `[Ljava/lang/String;`). Populated
    /// on demand when `ldc` or native reflection produces a Class constant.
    pub(super) class_objects: HashMap<String, crate::vm::types::Reference>,
    /// Linked invokedynamic targets, keyed by `owner_class#constant_pool_index`.
    pub(super) linked_dynamic_sites: HashMap<String, crate::vm::types::Reference>,
    /// Cached `CONSTANT_Dynamic` (condy) resolution results, keyed the same way
    /// as `linked_dynamic_sites` but for `ldc`-resolved values.
    pub(super) linked_condy_constants: HashMap<String, crate::vm::types::Value>,
    /// Field access flags keyed by `(declaring_class, field_name)`.
    pub(super) field_access_flags: HashMap<(String, String), u16>,
    /// Field descriptors keyed by `(declaring_class, field_name)`.
    pub(super) field_descriptors: HashMap<(String, String), String>,
    /// Counter incremented each time `Vm::execute` reaches the JIT tier. If the
    /// backend cannot lower the method yet, the VM records the activation and
    /// deoptimizes back to the interpreter instead of silently ignoring the JIT
    /// threshold.
    pub(super) jit_executions: u64,
}

#[derive(Debug, Default)]
pub(super) struct SharedMonitors {
    pub(super) states: Mutex<HashMap<usize, MonitorState>>,
    pub(super) changed: Condvar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum ThreadStatus {
    #[default]
    New,
    Runnable,
    Waiting,
    TimedWaiting,
    Blocked,
    Terminated,
}

pub(super) struct SharedThreads {
    pub(super) states: Mutex<HashMap<usize, JavaThreadState>>,
    /// Per-thread parking permit: `(Mutex<bool>, Condvar)` where the bool is
    /// `true` when a permit has been pre-granted (via `unpark`).
    pub(super) parking: Mutex<HashMap<usize, Arc<(Mutex<bool>, Condvar)>>>,
}

impl Default for SharedThreads {
    fn default() -> Self {
        Self {
            states: Mutex::new(HashMap::new()),
            parking: Mutex::new(HashMap::new()),
        }
    }
}

pub(super) struct JavaThreadState {
    pub(super) started: bool,
    pub(super) interrupted: bool,
    pub(super) handle: Option<JvmThread>,
    pub(super) status: ThreadStatus,
}

impl fmt::Debug for SharedThreads {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.states.lock().unwrap().len();
        f.debug_struct("SharedThreads")
            .field("thread_count", &count)
            .finish()
    }
}

#[derive(Debug)]
pub(crate) struct Thread {
    pub(super) frames: SmallVec<[Frame; 8]>,
}

impl Thread {
    pub(super) fn new(method: Method) -> Self {
        let mut frames = SmallVec::<[Frame; 8]>::new();
        frames.push(Frame::new(method));
        Self { frames }
    }

    pub(super) fn dummy() -> Self {
        Self {
            frames: SmallVec::new(),
        }
    }

    pub(super) fn current_frame(&self) -> &Frame {
        self.frames.last().expect("call stack is empty")
    }

    pub(super) fn current_frame_mut(&mut self) -> &mut Frame {
        self.frames.last_mut().expect("call stack is empty")
    }

    pub(super) fn push_frame(&mut self, frame: Frame) {
        self.frames.push(frame);
    }

    pub(super) fn pop_frame(&mut self) -> Frame {
        self.frames.pop().expect("call stack is empty")
    }

    pub(super) fn depth(&self) -> usize {
        self.frames.len()
    }
}

/// Per-object monitor state for `monitorenter` / `monitorexit`.
#[derive(Debug, Clone, Default)]
pub(super) struct MonitorState {
    /// Number of times the owning thread has entered this monitor.
    /// Zero means the monitor is free.
    pub(super) lock_count: usize,
    /// Thread ID of the owner (0 = unowned).
    pub(super) owner_thread: u64,
    /// Number of threads waiting in `Object.wait()`.
    pub(super) waiting_threads: usize,
    /// Number of pending notifications that have not yet been consumed by a waiter.
    pub(super) pending_notifies: usize,
}

/// Handle to a spawned VM thread, allowing the caller to wait for completion.
pub struct JvmThread {
    pub(super) handle: Option<std::thread::JoinHandle<Result<ExecutionResult, VmError>>>,
}

impl JvmThread {
    /// Block until the thread finishes and return its result.
    pub fn join(mut self) -> Result<ExecutionResult, VmError> {
        self.handle
            .take()
            .expect("thread already joined")
            .join()
            .unwrap_or(Err(VmError::MissingReturn))
    }
}
