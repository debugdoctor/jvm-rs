//! Thread state: the per-VM-instance call stack (`Thread`), the user-facing
//! join handle (`JvmThread`), monitor bookkeeping, and the shared runtime
//! state that's parked behind a `Mutex` on `Vm`.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Condvar, Mutex};

use super::frame::Frame;
use super::types::{ExecutionResult, Method, RuntimeClass, VmError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ClassInitializationState {
    Initializing(u64),
    Initialized,
}

#[derive(Debug, Default)]
pub(super) struct RuntimeState {
    pub(super) classes: BTreeMap<String, RuntimeClass>,
    pub(super) initialized_classes: BTreeMap<String, ClassInitializationState>,
}

#[derive(Debug, Default)]
pub(super) struct SharedMonitors {
    pub(super) states: Mutex<BTreeMap<usize, MonitorState>>,
    pub(super) changed: Condvar,
}

#[derive(Default)]
pub(super) struct SharedThreads {
    pub(super) states: Mutex<BTreeMap<usize, JavaThreadState>>,
}

pub(super) struct JavaThreadState {
    pub(super) started: bool,
    pub(super) handle: Option<JvmThread>,
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
pub(super) struct Thread {
    pub(super) frames: Vec<Frame>,
}

impl Thread {
    pub(super) fn new(method: Method) -> Self {
        Self {
            frames: vec![Frame::new(method)],
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
