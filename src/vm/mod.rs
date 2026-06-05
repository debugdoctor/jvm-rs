pub mod arena;
mod builtin;
mod classloader;
mod frame;
mod heap;
pub mod intern;
pub mod interpreter;
pub mod jit;
mod thread;
mod types;
pub mod verify;

pub use crate::classfile::ClassFile;
use frame::Frame;
pub use heap::GcStats;

/// A snapshot of runtime metrics collected from the VM at a point in time.
#[derive(Debug, Default, Clone)]
pub struct RuntimeCounters {
    /// Total classes currently registered (bootstrap + classpath combined).
    pub classes_loaded: u64,
    /// Total bytecode-interpreter invocations recorded (currently 0 — no global
    /// interpreter-invocation counter exists; per-method counts live in JIT profiles).
    pub interpreter_invocations: u64,
    /// Total JIT compilations performed (compiled method count in the code cache).
    pub jit_compilations: u64,
    /// Total times execution reached the JIT tier.
    pub jit_executions: u64,
    /// GC collections performed.
    pub gc_collections: u64,
    /// GC total pause time in nanoseconds.
    pub gc_pause_ns: u64,
    /// Current live heap objects.
    pub heap_live_objects: usize,
    /// Total heap allocations since start.
    pub total_allocations: u64,
}
pub use intern::{get_interner, Interner};
use heap::{Heap, HeapValue};
use interpreter::{
    execute_aconst_null, execute_aload, execute_areturn_full, execute_astore, execute_bipush,
    execute_dconst, execute_dload, execute_dstore, execute_dup, execute_fconst, execute_fload,
    execute_fstore, execute_iadd, execute_iconst, execute_iload, execute_imul,
    execute_ireturn_full, execute_istore, execute_isub, execute_lconst,
    execute_lload, execute_lreturn_full, execute_lstore, execute_pop, execute_return_full,
    execute_sipush,
};
use smallvec::SmallVec;
pub use thread::JvmThread;
use thread::{
    ClassInitializationState, JavaThreadState, RuntimeState, SharedMonitors, SharedThreads, Thread,
};
pub use types::{
    BootstrapArgument, ClassMethod, CondySite, ExceptionHandler, ExecutionResult, FieldRef,
    InvokeDynamicKind, InvokeDynamicSite, Method, MethodRef, Reference, RuntimeClass, Value,
    VmError,
};
use types::{
    default_value_for_descriptor, format_vm_float, parse_arg_count, parse_arg_types,
    parse_return_type,
};

use std::collections::HashMap;
use std::fmt;
use std::fs::File as FsFile;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use crate::bytecode::Opcode;
use crate::vm::jit::JitCompiler;
use crate::vm::jit::runtime::JitContext;
use classloader::{BootstrapClassLoader, ClassLoader, LazyClassLoader};

use crate::vm::jit::DeoptLocalKind;
use crate::vm::jit::runtime::{
    DeoptReason, DeoptSnapshot, clear_current_vm, set_current_vm, take_last_deopt_snapshot,
    take_pending_jit_exception,
};

static NEXT_THREAD_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

const LOOKUP_PUBLIC: i32 = 0x01;
const LOOKUP_PRIVATE: i32 = 0x02;
const LOOKUP_PROTECTED: i32 = 0x04;
const LOOKUP_PACKAGE: i32 = 0x08;
const LOOKUP_MODULE: i32 = 0x10;
const LOOKUP_ORIGINAL: i32 = 0x40;
const LOOKUP_FULL_POWER_MODES: i32 = LOOKUP_PUBLIC
    | LOOKUP_PRIVATE
    | LOOKUP_PROTECTED
    | LOOKUP_PACKAGE
    | LOOKUP_MODULE
    | LOOKUP_ORIGINAL;

// Derived (combinator) MethodHandle kinds. JVMS reference kinds occupy 1..=9.
pub(crate) const MH_KIND_BIND_TO: i32 = 10;
pub(crate) const MH_KIND_INSERT_ARGUMENTS: i32 = 11;
pub(crate) const MH_KIND_DROP_ARGUMENTS: i32 = 12;
pub(crate) const MH_KIND_PERMUTE_ARGUMENTS: i32 = 13;
pub(crate) const MH_KIND_AS_TYPE: i32 = 14;
pub(crate) const MH_KIND_AS_COLLECTOR: i32 = 15;
pub(crate) const MH_KIND_AS_SPREADER: i32 = 16;
pub(crate) const MH_KIND_FILTER_ARGUMENTS: i32 = 17;
pub(crate) const MH_KIND_FILTER_RETURN: i32 = 18;
pub(crate) const MH_KIND_FOLD_ARGUMENTS: i32 = 19;
pub(crate) const MH_KIND_GUARD_WITH_TEST: i32 = 20;
pub(crate) const MH_KIND_CATCH_EXCEPTION: i32 = 21;
pub(crate) const MH_KIND_INVOKER: i32 = 22;

// Sub-flavours of the kind-22 invoker so dispatch knows what to do.
pub(crate) const MH_INVOKER_EXACT: i32 = 0;
pub(crate) const MH_INVOKER_GENERIC: i32 = 1;
pub(crate) const MH_INVOKER_SPREAD: i32 = 2;
pub(crate) const MH_INVOKER_CALLSITE: i32 = 3;
pub(crate) const MH_INVOKER_IDENTITY: i32 = 4;

#[derive(Default)]
struct IoResources {
    next_id: AtomicU64,
    files: Mutex<Vec<Option<FsFile>>>,
}

impl IoResources {
    fn alloc(&self, file: FsFile) -> u64 {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.files.lock().unwrap().push(Some(file));
        id
    }

    fn with_file<F, R>(&self, id: u64, f: F) -> Result<R, VmError>
    where
        F: FnOnce(&mut FsFile) -> Result<R, VmError>,
    {
        let mut files = self.files.lock().unwrap();
        let idx = id as usize;
        match files.get_mut(idx).and_then(|slot| slot.as_mut()) {
            Some(file) => f(file),
            None => Err(VmError::UnhandledException {
                class_name: "java/io/IOException".to_string(),
            }),
        }
    }

    fn close(&self, id: u64) -> Option<FsFile> {
        let mut files = self.files.lock().unwrap();
        let idx = id as usize;
        if idx < files.len() {
            files[idx].take()
        } else {
            None
        }
    }

    fn is_open(&self, id: u64) -> bool {
        let files = self.files.lock().unwrap();
        let idx = id as usize;
        idx < files.len() && files[idx].is_some()
    }
}

enum JitInvocationResult {
    Returned(Option<Value>),
    Threw(Reference),
}

enum InterpreterFallbackResult {
    Returned(ExecutionResult),
    Threw(Reference),
}

pub struct Vm {
    heap: Arc<Mutex<Heap>>,
    runtime: Arc<Mutex<RuntimeState>>,
    monitors: Arc<SharedMonitors>,
    threads: Arc<SharedThreads>,
    class_path: Vec<PathBuf>,
    class_loader: Option<LazyClassLoader<BootstrapClassLoader>>,
    trace: bool,
    fail_fast: bool,
    verify_mode: crate::launcher::VerifyMode,
    xlog: crate::launcher::XlogFlags,
    jit_dump: bool,
    thread_id: u64,
    /// Heap index of this thread's `java/lang/Thread` object, set when a new
    /// Java thread is started via `start_java_thread`. Used by LockSupport
    /// park/unpark to locate the parking permit for this thread.
    java_thread_ref: Option<usize>,
    output: Arc<Mutex<Vec<String>>>,
    jit: Option<JitCompiler>,
    jit_context: Option<JitContext>,
    string_pool: Arc<Mutex<HashMap<String, Reference>>>,
    io_resources: Arc<IoResources>,
    /// Cache of resolved constant-pool values keyed by (class_name, cp_index).
    /// Infrastructure for quickened bytecode; populated lazily on LDC resolution.
    quickened_cp: Arc<Mutex<HashMap<(String, u16), Value>>>,
    /// Pre-parsed class data cache. Classes stored here skip re-parsing on
    /// repeated loads within a session. Foundation for mmap-backed CDS.
    class_data_cache: Arc<Mutex<HashMap<String, Arc<RuntimeClass>>>>,
}

impl fmt::Debug for Vm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vm")
            .field("heap", &self.heap)
            .field("runtime", &self.runtime)
            .field("monitors", &self.monitors)
            .field("threads", &self.threads)
            .field("class_path", &self.class_path)
            .field("trace", &self.trace)
            .field("thread_id", &self.thread_id)
            .field("output", &self.output)
            .field("jit", &self.jit)
            .finish()
    }
}

impl Clone for Vm {
    fn clone(&self) -> Self {
        Self {
            heap: self.heap.clone(),
            runtime: self.runtime.clone(),
            monitors: self.monitors.clone(),
            threads: self.threads.clone(),
            class_path: self.class_path.clone(),
            class_loader: None,
            trace: self.trace,
            fail_fast: self.fail_fast,
            verify_mode: self.verify_mode,
            xlog: self.xlog,
            jit_dump: self.jit_dump,
            thread_id: NEXT_THREAD_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            java_thread_ref: None,
            output: self.output.clone(),
            jit: None,
            jit_context: None,
            string_pool: self.string_pool.clone(),
            io_resources: self.io_resources.clone(),
            quickened_cp: self.quickened_cp.clone(),
            class_data_cache: self.class_data_cache.clone(),
        }
    }
}

impl Vm {
    pub fn new() -> Result<Self, String> {
        let jit = match JitCompiler::new() {
            Ok(j) => Some(j),
            Err(e) => {
                eprintln!("Warning: Failed to initialize JIT compiler: {}", e);
                None
            }
        };
        let jit_context = if jit.is_some() {
            Some(JitContext::new())
        } else {
            None
        };
        let mut vm = Self {
            heap: Arc::new(Mutex::new(Heap::default())),
            runtime: Arc::new(Mutex::new(RuntimeState::default())),
            monitors: Arc::new(SharedMonitors::default()),
            threads: Arc::new(SharedThreads::default()),
            class_path: Vec::new(),
            class_loader: Some(classloader::create_bootstrap_loader()),
            trace: false,
            fail_fast: false,
            verify_mode: crate::launcher::VerifyMode::default(),
            xlog: crate::launcher::XlogFlags::default(),
            jit_dump: false,
            thread_id: NEXT_THREAD_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            java_thread_ref: None,
            output: Arc::new(Mutex::new(Vec::new())),
            jit,
            jit_context,
            string_pool: Arc::new(Mutex::new(HashMap::new())),
            io_resources: Arc::new(IoResources::default()),
            quickened_cp: Arc::new(Mutex::new(HashMap::new())),
            class_data_cache: Arc::new(Mutex::new(HashMap::new())),
        };
        vm.bootstrap();
        Ok(vm)
    }

    pub fn set_fail_fast(&mut self, enabled: bool) {
        self.fail_fast = enabled;
    }

    pub fn set_verify_mode(&mut self, mode: crate::launcher::VerifyMode) {
        self.verify_mode = mode;
    }

    pub fn verify_mode(&self) -> crate::launcher::VerifyMode {
        self.verify_mode
    }

    pub fn set_xlog(&mut self, flags: crate::launcher::XlogFlags) {
        self.xlog = flags;
    }

    pub fn xlog(&self) -> crate::launcher::XlogFlags {
        self.xlog
    }

    pub fn set_jit_dump(&mut self, enabled: bool) {
        self.jit_dump = enabled;
    }

    #[inline(always)]
    fn log_class_load(&self, class_name: &str, source: &str) {
        if self.xlog.class_load {
            eprintln!("[class+load] Loaded {} from {}", class_name, source);
        }
    }

    #[inline(always)]
    fn log_gc(&self, pause_ms: f64, freed_bytes: usize, live_bytes: usize) {
        if self.xlog.gc {
            eprintln!("[gc] Pause {:.2}ms freed {}KB live {}KB", pause_ms, freed_bytes / 1024, live_bytes / 1024);
        }
    }

    #[inline(always)]
    fn log_jit(&self, class_name: &str, method_name: &str, code_bytes: usize) {
        if self.xlog.jit || self.jit_dump {
            eprintln!("[jit] Compiled {}.{} {} bytes", class_name, method_name, code_bytes);
        }
    }

    pub fn thread_dump(&self) -> Vec<(usize, String, String)> {
        let states = self.threads.states.lock().unwrap();
        let mut result = Vec::new();
        for (idx, state) in states.iter() {
            let name = format!("Thread-{}", idx);
            let status = format!("{:?}", state.status);
            result.push((*idx, name, status));
        }
        result
    }

    pub fn get_stub_stats(&self) -> (usize, usize, usize) {
        crate::vm::types::STUB_STATS.get_and_reset()
    }

    /// Enable or disable execution tracing (prints pc, opcode, stack to stderr).
    /// Spawn a new thread that executes the given method.
    ///
    /// The new thread shares heap/monitor/output state with the parent VM,
    /// while method-local execution state remains isolated per thread.
    pub fn spawn(&self, method: Method) -> JvmThread {
        let mut child_vm = self.clone();
        child_vm.thread_id = NEXT_THREAD_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let handle = std::thread::spawn(move || child_vm.execute(method));
        JvmThread {
            handle: Some(handle),
        }
    }

    fn spawn_invocation(
        &self,
        start_class: &str,
        method_name: &str,
        descriptor: &str,
        java_thread_ref: Option<usize>,
        args: Vec<Value>,
    ) -> Result<JvmThread, VmError> {
        let mut child_vm = self.clone();
        child_vm.thread_id = NEXT_THREAD_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        child_vm.java_thread_ref = java_thread_ref;

        let start_class = start_class.to_string();
        let method_name = method_name.to_string();
        let descriptor = descriptor.to_string();

        let handle = std::thread::spawn(move || {
            let (resolved_class, class_method) =
                child_vm.resolve_method(&start_class, &method_name, &descriptor)?;
            match class_method {
                ClassMethod::Native => {
                    let result = child_vm.invoke_native(
                        &resolved_class,
                        &method_name,
                        &descriptor,
                        &args,
                    )?;
                    Ok(result.map_or(ExecutionResult::Void, ExecutionResult::Value))
                }
                ClassMethod::Bytecode(method) => {
                    let callee = method.with_initial_locals(Vm::args_to_locals(args));
                    child_vm.execute(callee)
                }
            }
        });

        Ok(JvmThread {
            handle: Some(handle),
        })
    }

    /// Run garbage collection, freeing unreachable heap objects.
    fn collect_garbage(&mut self, thread: &Thread) {
        let mut roots = Vec::new();

        // Roots from thread frames: stack + locals.
        for frame in &thread.frames {
            for value in &frame.stack {
                if let Value::Reference(r @ Reference::Heap(_)) = value {
                    roots.push(*r);
                }
            }
            for local in &frame.locals {
                if let Some(Value::Reference(r @ Reference::Heap(_))) = local {
                    roots.push(*r);
                }
            }
            for constant in &frame.constants {
                if let Some(Value::Reference(r @ Reference::Heap(_))) = constant {
                    roots.push(*r);
                }
            }
        }

        // Roots from static fields of all loaded classes and from the
        // `java.lang.Class` cache.
        let runtime = self.runtime.lock().unwrap();
        for class in runtime.classes.values() {
            for value in class.static_fields.values() {
                if let Value::Reference(r @ Reference::Heap(_)) = value {
                    roots.push(*r);
                }
            }
        }
        for r in runtime.class_objects.values() {
            if let Reference::Heap(_) = r {
                roots.push(*r);
            }
        }
        for r in runtime.linked_dynamic_sites.values() {
            if let Reference::Heap(_) = r {
                roots.push(*r);
            }
        }

        // Roots from interned strings.
        for &r in self.string_pool.lock().unwrap().values() {
            if let Reference::Heap(_) = r {
                roots.push(r);
            }
        }

        drop(runtime);

        // Roots from any in-progress JIT-compiled frame on this thread. The
        // helper reads each frame's deopt buffer, which compiled code keeps
        // in sync with current locals via `emit_deopt_local_store`.
        for r in crate::vm::jit::runtime::jit_active_frame_roots() {
            roots.push(r);
        }

        let t0 = std::time::Instant::now();
        self.heap.lock().unwrap().gc(&roots);
        if self.xlog.gc {
            let stats = self.gc_stats();
            let pause_ms = t0.elapsed().as_secs_f64() * 1000.0;
            self.log_gc(pause_ms, stats.last_collection_freed * 32, stats.total_heap_bytes);
        }
    }

    pub fn set_trace(&mut self, enabled: bool) {
        self.trace = enabled;
    }

    /// Set the number of allocations between automatic GC passes. Use
    /// [`Self::disable_gc`] to switch automatic collection off entirely.
    pub fn set_gc_threshold(&mut self, allocations: usize) {
        self.heap.lock().unwrap().gc_threshold = allocations.max(1);
    }

    /// Test hook: set how many invocations are required before a method is
    /// JIT-compiled. Production threshold is 1000; tests can drop this to 1
    /// so JIT fires on the very first call.
    pub fn set_jit_thresholds(&mut self, invocation: u32, backedge: u32) {
        if let Some(jit) = self.jit.as_mut() {
            jit.set_thresholds(invocation, backedge);
        }
    }

    /// Whether a real JIT compiler is available (false if `JitCompiler::new`
    /// failed to build a host ISA).
    pub fn has_jit(&self) -> bool {
        self.jit.is_some() && self.jit_context.is_some()
    }

    /// Returns `true` if the JIT considers the named method a candidate for
    /// inlining at call sites (small body, no exception handlers, static/private/final).
    /// Always returns `false` when no JIT is available.
    pub fn can_inline_method(&self, class: &str, method: &str, desc: &str) -> bool {
        let rt = self.runtime.lock().unwrap();
        self.jit
            .as_ref()
            .map(|j| j.can_inline(&rt.classes, class, method, desc))
            .unwrap_or(false)
    }

    /// Returns JIT code-cache statistics: `(compiled_methods, total_code_bytes, interpreter_only_methods)`.
    pub fn jit_code_cache_stats(&self) -> (usize, usize, usize) {
        self.jit
            .as_ref()
            .map(|j| j.code_cache_stats())
            .unwrap_or((0, 0, 0))
    }

    /// Test hook: how many times execution reached the JIT tier. Methods that
    /// the backend cannot lower yet are counted before deoptimizing to the
    /// interpreter so threshold bugs do not look like normal interpreter runs.
    pub fn jit_executions(&self) -> u64 {
        self.runtime.lock().unwrap().jit_executions
    }

    /// Test hook: how many times a specific compiled method deoptimized for a
    /// given reason.
    pub fn jit_deopt_count(
        &self,
        class_name: &str,
        method_name: &str,
        descriptor: &str,
        reason: DeoptReason,
    ) -> u64 {
        let method_key = format!("{}.{}{}", class_name, method_name, descriptor);
        self.jit
            .as_ref()
            .map(|jit| jit.deopt_count(&method_key, reason))
            .unwrap_or(0)
    }

    /// Test hook: total deoptimizations observed for a specific compiled
    /// method across all reasons.
    pub fn jit_total_deopt_count(
        &self,
        class_name: &str,
        method_name: &str,
        descriptor: &str,
    ) -> u64 {
        let method_key = format!("{}.{}{}", class_name, method_name, descriptor);
        self.jit
            .as_ref()
            .map(|jit| jit.total_deopt_count(&method_key))
            .unwrap_or(0)
    }

    /// Test hook: how many times a specific bytecode pc deoptimized for a
    /// given reason within a compiled method.
    pub fn jit_deopt_site_count(
        &self,
        class_name: &str,
        method_name: &str,
        descriptor: &str,
        pc: usize,
        reason: DeoptReason,
    ) -> u64 {
        let method_key = format!("{}.{}{}", class_name, method_name, descriptor);
        self.jit
            .as_ref()
            .map(|jit| jit.deopt_site_count(&method_key, pc, reason))
            .unwrap_or(0)
    }

    /// Test hook: hottest deoptimization bytecode site for a specific method.
    pub fn jit_hottest_deopt_site(
        &self,
        class_name: &str,
        method_name: &str,
        descriptor: &str,
    ) -> Option<(usize, u64)> {
        let method_key = format!("{}.{}{}", class_name, method_name, descriptor);
        self.jit
            .as_ref()
            .and_then(|jit| jit.hottest_deopt_site(&method_key))
    }

    /// Test hook: whether the method has been forced back to interpreter-only
    /// execution after repeated JIT deoptimizations.
    pub fn jit_interpreter_only_reason(
        &self,
        class_name: &str,
        method_name: &str,
        descriptor: &str,
    ) -> Option<DeoptReason> {
        let method_key = format!("{}.{}{}", class_name, method_name, descriptor);
        self.jit
            .as_ref()
            .and_then(|jit| jit.interpreter_only_reason(&method_key))
    }

    /// Turn off automatic GC. Programs can still call [`Self::request_gc`]
    /// explicitly (for example after a workload that produces transient garbage).
    pub fn disable_gc(&mut self) {
        self.heap.lock().unwrap().gc_threshold = usize::MAX;
    }

    /// Force a GC pass using the current thread's root set. Intended for tests
    /// and tools that want deterministic heap shape; production code should let
    /// the VM trigger collections on its own.
    pub fn request_gc(&mut self) {
        let thread = Thread {
            frames: SmallVec::new(),
        };
        self.collect_garbage(&thread);
    }

    /// Snapshot current GC counters.
    pub fn gc_stats(&self) -> GcStats {
        self.heap.lock().unwrap().stats
    }

    /// Number of classes currently held in the class-data-sharing cache.
    pub fn class_data_cache_len(&self) -> usize {
        self.class_data_cache.lock().unwrap().len()
    }

    /// Emit a human-readable heap snapshot (jvm-rs native text format).
    pub fn heap_dump(&self) -> String {
        use std::fmt::Write as _;
        let heap = self.heap.lock().unwrap();
        let stats = heap.stats;
        let mut out = String::new();
        let _ = writeln!(out, "=== jvm-rs heap dump ===");
        let _ = writeln!(out, "live objects: {}", stats.live);
        let _ = writeln!(out, "total allocations: {}", stats.total_allocations);
        let _ = writeln!(out, "gc collections: {}", stats.collections);
        let _ = writeln!(out, "heap bytes: {}", stats.total_heap_bytes);

        // Build histogram: count per class/kind name
        let mut histogram: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for slot in &heap.values {
            if let Some(obj) = slot {
                let key = match obj {
                    HeapValue::Object { class_name, .. } => class_name.clone(),
                    other => other.kind_name().to_string(),
                };
                *histogram.entry(key).or_insert(0) += 1;
            }
        }

        let _ = writeln!(out, "--- class histogram (top 10) ---");
        let mut entries: Vec<(String, usize)> = histogram.into_iter().collect();
        entries.sort_by(|a, b| b.1.cmp(&a.1));
        for (name, count) in entries.iter().take(10) {
            let _ = writeln!(out, "  {:>6}  {}", count, name);
        }
        let _ = writeln!(out, "--- end heap dump ---");
        out
    }

    /// Snapshot all runtime counters in a single consistent read.
    pub fn counters(&self) -> RuntimeCounters {
        let rt = self.runtime.lock().unwrap();
        let gc = self.heap.lock().unwrap().stats;
        let (jit_compilations, _, _) = self
            .jit
            .as_ref()
            .map(|j| j.code_cache_stats())
            .unwrap_or((0, 0, 0));
        RuntimeCounters {
            classes_loaded: rt.classes.len() as u64,
            interpreter_invocations: 0, // no single global counter; profiled per-method via JIT
            jit_compilations: jit_compilations as u64,
            jit_executions: rt.jit_executions,
            gc_collections: gc.collections,
            gc_pause_ns: gc.pause_time_ns,
            heap_live_objects: gc.live,
            total_allocations: gc.total_allocations,
        }
    }

    /// Resident set size of the current process in bytes, or `None` if
    /// the platform does not provide a simple `/proc/self/status` interface.
    /// Used by benchmarks to record memory footprint alongside `RuntimeCounters`.
    pub fn rss_bytes() -> Option<usize> {
        #[cfg(target_os = "linux")]
        {
            let text = std::fs::read_to_string("/proc/self/status").ok()?;
            for line in text.lines() {
                if let Some(rest) = line.strip_prefix("VmRSS:") {
                    let kb: usize = rest.split_whitespace().next()?.parse().ok()?;
                    return Some(kb * 1024);
                }
            }
            None
        }
        #[cfg(target_os = "macos")]
        {
            // getrusage ru_maxrss is bytes on macOS
            let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
            if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } == 0 {
                Some(usage.ru_maxrss as usize)
            } else {
                None
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            None
        }
    }

    /// Set the classpath entries used for on-demand class loading.
    pub fn set_class_path(&mut self, paths: Vec<PathBuf>) {
        self.class_path = paths;
    }

    /// Find a resource from the bootstrap classpath.
    pub fn find_resource(&mut self, resource_path: &str) -> Option<Vec<u8>> {
        if let Some(ref mut loader) = self.class_loader {
            if let Some(bytes) = ClassLoader::find_resource_bytes(loader, resource_path) {
                return Some(bytes);
            }
        }
        None
    }

    /// Register a class loaded from a `.class` file.
    pub fn register_class(&mut self, class: RuntimeClass) {
        self.runtime
            .lock()
            .unwrap()
            .classes
            .insert(class.name.clone(), class);
    }

    pub(crate) fn register_field_access_flags(
        &mut self,
        class_name: &str,
        flags: impl IntoIterator<Item = (String, u16)>,
    ) {
        let mut runtime = self.runtime.lock().unwrap();
        for (field_name, access_flags) in flags {
            runtime
                .field_access_flags
                .insert((class_name.to_string(), field_name), access_flags);
        }
    }

    pub(crate) fn register_field_descriptors(
        &mut self,
        class_name: &str,
        descriptors: impl IntoIterator<Item = (String, String)>,
    ) {
        let mut runtime = self.runtime.lock().unwrap();
        for (field_name, descriptor) in descriptors {
            runtime
                .field_descriptors
                .insert((class_name.to_string(), field_name), descriptor);
        }
    }

    /// Project a value list into JVM-slot-indexed locals: longs and doubles
    /// occupy two slots per JVMS §2.6, so subsequent parameters land at the
    /// index the bytecode expects. Without this padding, methods like
    /// `ArraysSupport.vectorizedMismatch(Object,J,Object,J,I,I)` read local
    /// 7 (the second int) from an uninitialized slot.
    fn collect_jit_args_static(method: &Method, frame: &Frame) -> Vec<Value> {
        // JIT signature is built from the descriptor; for non-static methods the
        // JIT does not include `this`, so we skip locals[0] in that case.
        let arg_count = parse_arg_types(&method.descriptor)
            .map(|v| v.len())
            .unwrap_or(0);
        let is_static = method.access_flags & 0x0008 != 0;
        let mut out = Vec::with_capacity(arg_count);
        let mut local_idx = if is_static { 0 } else { 1 };
        for _ in 0..arg_count {
            let v = frame
                .locals
                .get(local_idx)
                .and_then(|o| o.clone())
                .unwrap_or(Value::Int(0));
            let wide = matches!(v, Value::Long(_) | Value::Double(_));
            out.push(v);
            local_idx += if wide { 2 } else { 1 };
        }
        out
    }

    pub(super) fn args_to_locals(args: Vec<Value>) -> Vec<Option<Value>> {
        let mut locals = Vec::with_capacity(args.len());
        for value in args {
            let wide = matches!(value, Value::Long(_) | Value::Double(_));
            locals.push(Some(value));
            if wide {
                locals.push(None);
            }
        }
        locals
    }

    fn execute_interpreter_fallback(
        &mut self,
        method: Method,
        locals: Vec<Option<Value>>,
    ) -> Option<InterpreterFallbackResult> {
        let mut fallback_vm = self.clone();
        let method = method.with_initial_locals(locals);
        match fallback_vm.execute(method) {
            Ok(result) => Some(InterpreterFallbackResult::Returned(result)),
            Err(VmError::UnhandledException { class_name }) => {
                let exception_ref = fallback_vm
                    .heap
                    .lock()
                    .unwrap()
                    .allocate(HeapValue::Object {
                        class_name,
                        fields: vec![],
                    });
                Some(InterpreterFallbackResult::Threw(exception_ref))
            }
            Err(err) => {
                println!(
                    "interpreter fallback failed for JIT exception path: {:?}",
                    err
                );
                None
            }
        }
    }

    fn decode_deopt_locals(
        &self,
        local_kinds: &[DeoptLocalKind],
        raw_locals: &[u64],
    ) -> Vec<Option<Value>> {
        local_kinds
            .iter()
            .enumerate()
            .map(|(index, kind)| match kind {
                DeoptLocalKind::Int => Some(Value::Int(
                    raw_locals.get(index).copied().unwrap_or(0) as i32,
                )),
                DeoptLocalKind::Long => Some(Value::Long(
                    raw_locals.get(index).copied().unwrap_or(0) as i64,
                )),
                DeoptLocalKind::Float => Some(Value::Float(f32::from_bits(
                    raw_locals.get(index).copied().unwrap_or(0) as u32,
                ))),
                DeoptLocalKind::Double => Some(Value::Double(f64::from_bits(
                    raw_locals.get(index).copied().unwrap_or(0),
                ))),
                DeoptLocalKind::Reference => Some(Value::Reference(
                    Vm::jit_raw_reference(raw_locals.get(index).copied().unwrap_or(0))
                        .unwrap_or(Reference::Null),
                )),
                DeoptLocalKind::Top => None,
            })
            .collect()
    }

    fn decode_deopt_stack(&self, stack_kinds: &[DeoptLocalKind], raw_stack: &[u64]) -> Vec<Value> {
        stack_kinds
            .iter()
            .enumerate()
            .filter_map(|(index, kind)| match kind {
                DeoptLocalKind::Int => {
                    Some(Value::Int(raw_stack.get(index).copied().unwrap_or(0) as i32))
                }
                DeoptLocalKind::Long => Some(Value::Long(
                    raw_stack.get(index).copied().unwrap_or(0) as i64,
                )),
                DeoptLocalKind::Float => Some(Value::Float(f32::from_bits(
                    raw_stack.get(index).copied().unwrap_or(0) as u32,
                ))),
                DeoptLocalKind::Double => Some(Value::Double(f64::from_bits(
                    raw_stack.get(index).copied().unwrap_or(0),
                ))),
                DeoptLocalKind::Reference => Some(Value::Reference(
                    Vm::jit_raw_reference(raw_stack.get(index).copied().unwrap_or(0))
                        .unwrap_or(Reference::Null),
                )),
                DeoptLocalKind::Top => None,
            })
            .collect()
    }

    fn run_interpreter_thread(&mut self, thread: &mut Thread) -> Result<ExecutionResult, VmError> {
        loop {
            let opcode_pc = thread.current_frame().pc;
            if opcode_pc >= thread.current_frame().code.len() {
                return Err(VmError::MissingReturn);
            }
            let opcode_byte = thread.current_frame_mut().read_u8()?;
            let opcode = Opcode::from_byte(opcode_byte).ok_or(VmError::InvalidOpcode {
                opcode: opcode_byte,
                pc: opcode_pc,
            })?;

            match self.execute_opcode(thread, opcode, opcode_pc) {
                Ok(Some(result)) => return Ok(result),
                Ok(None) => {}
                Err(VmError::NullReference) => {
                    self.throw_new_exception(thread, "java/lang/NullPointerException")?;
                }
                Err(VmError::ArrayIndexOutOfBounds { .. }) => {
                    self.throw_new_exception(thread, "java/lang/ArrayIndexOutOfBoundsException")?;
                }
                Err(VmError::NegativeArraySize { .. }) => {
                    self.throw_new_exception(thread, "java/lang/NegativeArraySizeException")?;
                }
                Err(VmError::ClassCastError { .. }) => {
                    self.throw_new_exception(thread, "java/lang/ClassCastException")?;
                }
                Err(VmError::UnhandledException { class_name }) => {
                    self.throw_new_exception(thread, &class_name)?;
                }
                Err(err) => return Err(err),
            }
        }
    }

    fn resume_interpreter_from_deopt(
        &mut self,
        method: Method,
        local_kinds: &[DeoptLocalKind],
        stack_kinds_by_pc: &HashMap<usize, Vec<DeoptLocalKind>>,
        snapshot: &DeoptSnapshot,
        exception_ref: Option<Reference>,
    ) -> Option<InterpreterFallbackResult> {
        let locals = self.decode_deopt_locals(local_kinds, &snapshot.locals);
        let stack_kinds = stack_kinds_by_pc
            .get(&snapshot.pc)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let stack = self.decode_deopt_stack(stack_kinds, &snapshot.stack);
        let mut fallback_vm = self.clone();
        let method = method.with_initial_locals(locals);
        let mut thread = Thread::new(method);
        {
            let frame = thread.current_frame_mut();
            frame.stack = stack;
        }

        match exception_ref {
            Some(exception_ref) => {
                thread.current_frame_mut().pc = snapshot.pc.saturating_add(1);
                match fallback_vm.throw_exception(&mut thread, exception_ref) {
                    Ok(()) => match fallback_vm.run_interpreter_thread(&mut thread) {
                        Ok(result) => Some(InterpreterFallbackResult::Returned(result)),
                        Err(VmError::UnhandledException { .. }) => {
                            Some(InterpreterFallbackResult::Threw(exception_ref))
                        }
                        Err(err) => {
                            println!("deopt resume failed while interpreting handler: {:?}", err);
                            None
                        }
                    },
                    Err(VmError::UnhandledException { .. }) => {
                        Some(InterpreterFallbackResult::Threw(exception_ref))
                    }
                    Err(err) => {
                        println!("deopt resume failed while installing exception: {:?}", err);
                        None
                    }
                }
            }
            None => {
                thread.current_frame_mut().pc = snapshot.pc;
                match fallback_vm.run_interpreter_thread(&mut thread) {
                    Ok(result) => Some(InterpreterFallbackResult::Returned(result)),
                    Err(VmError::UnhandledException { class_name }) => {
                        let exception_ref =
                            fallback_vm
                                .heap
                                .lock()
                                .unwrap()
                                .allocate(HeapValue::Object {
                                    class_name,
                                    fields: vec![],
                                });
                        Some(InterpreterFallbackResult::Threw(exception_ref))
                    }
                    Err(err) => {
                        println!(
                            "deopt resume failed while continuing in interpreter: {:?}",
                            err
                        );
                        None
                    }
                }
            }
        }
    }

    fn apply_jit_deopt_policy(&mut self, method_key: &str, snapshot: Option<&DeoptSnapshot>) {
        let Some(reason) = snapshot.and_then(|snapshot| snapshot.reason) else {
            return;
        };

        if let Some(jit) = self.jit.as_ref() {
            jit.record_deopt(method_key, reason);
            let should_abandon = if let Some(snapshot) = snapshot {
                jit.record_deopt_site(method_key, snapshot.pc, reason);
                if jit.should_recompile_with_site_fallback(method_key, snapshot.pc, reason) {
                    jit.invalidate_compiled_method(method_key);
                    if let Some(jit_context) = self.jit_context.as_mut() {
                        jit_context.remove_method(method_key);
                    }
                }
                jit.should_abandon_jit_at_site(method_key, snapshot.pc, reason)
            } else {
                jit.should_abandon_jit(method_key, reason)
            };
            if should_abandon {
                if let Some(jit_context) = self.jit_context.as_mut() {
                    jit_context.remove_method(method_key);
                }
                jit.mark_interpreter_only(method_key.to_string(), reason);
            }
        }
    }

    fn complete_jit_execution(
        &mut self,
        method_key: &str,
        method: Method,
        deopt_local_kinds: &[DeoptLocalKind],
        deopt_stack_kinds_by_pc: &HashMap<usize, Vec<DeoptLocalKind>>,
        snapshot: Option<DeoptSnapshot>,
        exception_ref: Option<Reference>,
        interpreter_locals: Option<Vec<Option<Value>>>,
        jit_value: Value,
        ret: crate::vm::jit::runtime::JitReturn,
    ) -> Option<JitInvocationResult> {
        let snapshot_ref = snapshot.as_ref();
        self.apply_jit_deopt_policy(method_key, snapshot_ref);

        if let Some(exception_ref) = exception_ref {
            if !method.exception_handlers.is_empty() {
                if let Some(snapshot) = snapshot_ref {
                    match self.resume_interpreter_from_deopt(
                        method.clone(),
                        deopt_local_kinds,
                        deopt_stack_kinds_by_pc,
                        snapshot,
                        Some(exception_ref),
                    ) {
                        Some(InterpreterFallbackResult::Returned(ExecutionResult::Void)) => {
                            return Some(JitInvocationResult::Returned(None));
                        }
                        Some(InterpreterFallbackResult::Returned(ExecutionResult::Value(
                            value,
                        ))) => {
                            return Some(JitInvocationResult::Returned(Some(value)));
                        }
                        Some(InterpreterFallbackResult::Threw(exception_ref)) => {
                            return Some(JitInvocationResult::Threw(exception_ref));
                        }
                        None => {}
                    }
                } else if let Some(locals) = interpreter_locals {
                    match self.execute_interpreter_fallback(method.clone(), locals) {
                        Some(InterpreterFallbackResult::Returned(ExecutionResult::Void)) => {
                            return Some(JitInvocationResult::Returned(None));
                        }
                        Some(InterpreterFallbackResult::Returned(ExecutionResult::Value(
                            value,
                        ))) => {
                            return Some(JitInvocationResult::Returned(Some(value)));
                        }
                        Some(InterpreterFallbackResult::Threw(exception_ref)) => {
                            return Some(JitInvocationResult::Threw(exception_ref));
                        }
                        None => {}
                    }
                }
            }
            return Some(JitInvocationResult::Threw(exception_ref));
        }

        if let Some(snapshot) = snapshot_ref.filter(|snapshot| snapshot.reason.is_some()) {
            match self.resume_interpreter_from_deopt(
                method,
                deopt_local_kinds,
                deopt_stack_kinds_by_pc,
                snapshot,
                None,
            ) {
                Some(InterpreterFallbackResult::Returned(ExecutionResult::Void)) => {
                    return Some(JitInvocationResult::Returned(None));
                }
                Some(InterpreterFallbackResult::Returned(ExecutionResult::Value(value))) => {
                    return Some(JitInvocationResult::Returned(Some(value)));
                }
                Some(InterpreterFallbackResult::Threw(exception_ref)) => {
                    return Some(JitInvocationResult::Threw(exception_ref));
                }
                None => {}
            }
        }

        if matches!(ret, crate::vm::jit::runtime::JitReturn::Void) {
            Some(JitInvocationResult::Returned(None))
        } else {
            Some(JitInvocationResult::Returned(Some(jit_value)))
        }
    }

    fn try_execute_jit_method(
        &mut self,
        method: &Method,
        args: &[Value],
    ) -> Option<JitInvocationResult> {
        let method_key = format!("{}.{}{}", method.class_name, method.name, method.descriptor);
        let code = self.jit.as_ref()?.get_or_compile(method)?;
        self.log_jit(&method.class_name, &method.name, code.code_buffer.len());
        let vm_ptr = self as *mut Vm as u64;
        let jit_context = self.jit_context.as_mut()?;
        let deopt_local_kinds = code.deopt_info.local_kinds.clone();
        let deopt_stack_kinds_by_pc = code.deopt_info.stack_kinds_by_pc.clone();

        if jit_context.get_entry(&method_key).is_none()
            && !jit_context.add_method(method_key.clone(), code)
        {
            return None;
        }

        let ret = crate::vm::jit::runtime::JitReturn::from_descriptor(&method.descriptor);
        let result = jit_context.execute_typed(vm_ptr, &method_key, args, ret)?;
        self.runtime.lock().unwrap().jit_executions += 1;
        let snapshot = take_last_deopt_snapshot();
        self.complete_jit_execution(
            &method_key,
            method.clone(),
            &deopt_local_kinds,
            &deopt_stack_kinds_by_pc,
            snapshot,
            take_pending_jit_exception(),
            Some(Vm::args_to_locals(args.to_vec())),
            result,
            ret,
        )
    }

    fn try_execute_osr_method(
        &mut self,
        method: Method,
        locals: Vec<Option<Value>>,
        entry_pc: usize,
    ) -> Option<JitInvocationResult> {
        if !locals
            .iter()
            .all(|value| !matches!(value, Some(Value::ReturnAddress(_))))
        {
            return None;
        }

        let method_key = JitCompiler::osr_method_key(&method, entry_pc);
        let code = self.jit.as_ref()?.get_or_compile_osr(&method, entry_pc)?;
        let vm_ptr = self as *mut Vm as u64;
        let jit_context = self.jit_context.as_mut()?;
        let deopt_local_kinds = code.deopt_info.local_kinds.clone();
        let deopt_stack_kinds_by_pc = code.deopt_info.stack_kinds_by_pc.clone();

        if jit_context.get_entry(&method_key).is_none()
            && !jit_context.add_method(method_key.clone(), code)
        {
            return None;
        }

        let osr_locals = Self::osr_locals_to_buffer(&locals, method.max_locals);
        let ret = crate::vm::jit::runtime::JitReturn::from_descriptor(&method.descriptor);
        let result =
            jit_context.execute_osr_typed(vm_ptr, &method_key, &osr_locals, ret)?;
        self.runtime.lock().unwrap().jit_executions += 1;
        let snapshot = take_last_deopt_snapshot();
        self.complete_jit_execution(
            &method_key,
            method,
            &deopt_local_kinds,
            &deopt_stack_kinds_by_pc,
            snapshot,
            take_pending_jit_exception(),
            Some(Vm::args_to_locals(osr_locals)),
            result,
            ret,
        )
    }

    /// Pack an interpreter local table into a contiguous, fixed-length buffer
    /// that matches the OSR-compiled function's `locals_ptr` ABI: one i64 slot
    /// per declared local (uninitialized slots become zero ints).
    fn osr_locals_to_buffer(locals: &[Option<Value>], max_locals: usize) -> Vec<Value> {
        let mut out = Vec::with_capacity(max_locals);
        for i in 0..max_locals {
            out.push(locals.get(i).and_then(|v| v.clone()).unwrap_or(Value::Int(0)));
        }
        out
    }

    pub(crate) fn invoke_jit_static_method_ref(
        &mut self,
        method_ref: &MethodRef,
        args_ptr: u64,
        argc: usize,
    ) -> Result<Option<Value>, VmError> {
        let args = unsafe { Vm::jit_raw_args_to_values(&method_ref.descriptor, args_ptr, argc) }
            .ok_or_else(|| VmError::InvalidDescriptor {
                descriptor: method_ref.descriptor.clone(),
            })?;
        self.ensure_class_loaded(&method_ref.class_name)?;
        self.ensure_class_initialized(&method_ref.class_name)?;

        if self.has_native_override(
            &method_ref.class_name,
            &method_ref.method_name,
            &method_ref.descriptor,
        ) {
            return self.invoke_native(
                &method_ref.class_name,
                &method_ref.method_name,
                &method_ref.descriptor,
                &args,
            );
        }

        let class = self.get_class(&method_ref.class_name)?;
        let class_method = class
            .methods
            .get(&(
                method_ref.method_name.clone(),
                method_ref.descriptor.clone(),
            ))
            .cloned()
            .ok_or_else(|| VmError::MethodNotFound {
                class_name: method_ref.class_name.clone(),
                method_name: method_ref.method_name.clone(),
                descriptor: method_ref.descriptor.clone(),
            })?;

        match class_method {
            ClassMethod::Native => self.invoke_native(
                &method_ref.class_name,
                &method_ref.method_name,
                &method_ref.descriptor,
                &args,
            ),
            ClassMethod::Bytecode(method) => {
                let callee = method.with_initial_locals(Vm::args_to_locals(args));
                let saved_jit = self.jit.take();
                let result = self.execute(callee);
                self.jit = saved_jit;
                match result? {
                    ExecutionResult::Value(value) => Ok(Some(value)),
                    ExecutionResult::Void => Ok(None),
                }
            }
        }
    }

    pub(crate) fn invoke_jit_virtual_method_ref(
        &mut self,
        method_ref: &MethodRef,
        receiver_raw: u64,
        args_ptr: u64,
        argc: usize,
    ) -> Result<Option<Value>, VmError> {
        let receiver = Vm::jit_raw_reference(receiver_raw).ok_or(VmError::NullReference)?;
        if Self::is_signature_polymorphic(&method_ref.class_name, &method_ref.method_name) {
            let args =
                unsafe { Vm::jit_raw_args_to_values(&method_ref.descriptor, args_ptr, argc) }
                    .ok_or_else(|| VmError::InvalidDescriptor {
                        descriptor: method_ref.descriptor.clone(),
                    })?;
            return self.invoke_signature_polymorphic(receiver, method_ref, args);
        }
        let class_name = self.get_object_class(receiver)?;
        self.invoke_jit_instance_method_ref(&class_name, method_ref, receiver, args_ptr, argc)
    }

    pub(crate) fn invoke_jit_special_method_ref(
        &mut self,
        method_ref: &MethodRef,
        receiver_raw: u64,
        args_ptr: u64,
        argc: usize,
    ) -> Result<Option<Value>, VmError> {
        let receiver = Vm::jit_raw_reference(receiver_raw).ok_or(VmError::NullReference)?;
        self.invoke_jit_instance_method_ref(
            &method_ref.class_name,
            method_ref,
            receiver,
            args_ptr,
            argc,
        )
    }

    pub(crate) fn invoke_jit_interface_method_ref(
        &mut self,
        method_ref: &MethodRef,
        receiver_raw: u64,
        args_ptr: u64,
        argc: usize,
    ) -> Result<Option<Value>, VmError> {
        let receiver = Vm::jit_raw_reference(receiver_raw).ok_or(VmError::NullReference)?;
        let class_name = self.get_object_class(receiver)?;
        self.invoke_jit_instance_method_ref(&class_name, method_ref, receiver, args_ptr, argc)
    }

    pub(crate) fn invoke_jit_native_method_ref(
        &mut self,
        method_ref: &MethodRef,
        args_ptr: u64,
        argc: usize,
    ) -> Result<Option<Value>, VmError> {
        let args = unsafe { Vm::jit_raw_args_to_values(&method_ref.descriptor, args_ptr, argc) }
            .ok_or_else(|| VmError::InvalidDescriptor {
                descriptor: method_ref.descriptor.clone(),
            })?;
        self.invoke_native(
            &method_ref.class_name,
            &method_ref.method_name,
            &method_ref.descriptor,
            &args,
        )
    }

    pub(crate) fn invoke_jit_dynamic_site(
        &mut self,
        site: &InvokeDynamicSite,
        args_ptr: u64,
        argc: usize,
    ) -> Result<Option<Value>, VmError> {
        let args = unsafe { Vm::jit_raw_args_to_values(&site.descriptor, args_ptr, argc) }
            .ok_or_else(|| VmError::InvalidDescriptor {
                descriptor: site.descriptor.clone(),
            })?;
        match &site.kind {
            InvokeDynamicKind::LambdaProxy {
                target_class,
                target_method,
                target_descriptor,
            } => self
                .allocate_lambda_proxy(site, target_class, target_method, target_descriptor, args)
                .map(|proxy| Some(Value::Reference(proxy))),
            InvokeDynamicKind::StringConcat { recipe, constants } => self
                .build_string_concat(recipe.as_deref(), constants, &args, &site.descriptor)
                .map(|concat| Some(self.new_string(concat)))
                .map_err(|_| VmError::InvalidDescriptor {
                    descriptor: site.descriptor.clone(),
                }),
            InvokeDynamicKind::Unknown => Ok(Some(Value::Reference(Reference::Null))),
            InvokeDynamicKind::BootstrapMethodHandle { .. } => {
                self.invoke_dynamic_via_method_handle(site, args)
            }
        }
    }

    fn invoke_dynamic_via_method_handle(
        &mut self,
        site: &InvokeDynamicSite,
        args: Vec<Value>,
    ) -> Result<Option<Value>, VmError> {
        if let InvokeDynamicKind::BootstrapMethodHandle {
            bootstrap_class,
            bootstrap_name,
            bootstrap_descriptor,
            arguments,
        } = &site.kind
        {
            let linked = if let Some(linked) = self.get_linked_dynamic_site(site) {
                linked
            } else {
                let mut bootstrap_args = self.bootstrap_method_leading_args(site, None)?;
                for arg in arguments {
                    if let Some(val) = self.resolve_bootstrap_argument_value(arg)? {
                        bootstrap_args.push(val);
                    }
                }
                let result = self.reflect_invoke_method(
                    bootstrap_class,
                    bootstrap_name,
                    bootstrap_descriptor,
                    None,
                    bootstrap_args,
                )?;
                let linked = self.extract_linked_dynamic_target(result)?;
                self.set_linked_dynamic_site(site, linked);
                linked
            };
            let target = self.resolve_dynamic_target(linked)?;
            self.invoke_method_handle(target, args)
        } else {
            Ok(Some(Value::Reference(Reference::Null)))
        }
    }

    fn invoke_interp_dynamic_via_method_handle(
        &mut self,
        thread: &mut Thread,
        site: &InvokeDynamicSite,
        args: Vec<Value>,
    ) -> Result<(), VmError> {
        if let InvokeDynamicKind::BootstrapMethodHandle {
            bootstrap_class,
            bootstrap_name,
            bootstrap_descriptor,
            arguments,
        } = &site.kind
        {
            let linked = if let Some(linked) = self.get_linked_dynamic_site(site) {
                linked
            } else {
                let caller_class_name = thread.current_frame().class_name.clone();
                let mut bootstrap_args =
                    self.bootstrap_method_leading_args(site, Some(&caller_class_name))?;
                for arg in arguments {
                    if let Some(val) = self.resolve_bootstrap_argument_value(arg)? {
                        bootstrap_args.push(val);
                    }
                }
                let result = self.reflect_invoke_method(
                    bootstrap_class,
                    bootstrap_name,
                    bootstrap_descriptor,
                    None,
                    bootstrap_args,
                )?;
                let linked = self.extract_linked_dynamic_target(result)?;
                self.set_linked_dynamic_site(site, linked);
                linked
            };
            let target = self.resolve_dynamic_target(linked)?;
            if let Some(value) = self.invoke_method_handle(target, args)? {
                thread.current_frame_mut().push(value)?;
            }
            Ok(())
        } else {
            thread
                .current_frame_mut()
                .push(Value::Reference(Reference::Null))?;
            Ok(())
        }
    }

    fn bootstrap_method_leading_args(
        &mut self,
        site: &InvokeDynamicSite,
        caller_class_name: Option<&str>,
    ) -> Result<Vec<Value>, VmError> {
        let lookup =
            self.allocate_bootstrap_lookup(caller_class_name.unwrap_or("java/lang/Object"))?;
        let invoked_name = self.new_string(site.name.clone());
        let invoked_type = self.allocate_bootstrap_method_type(&site.descriptor)?;
        Ok(vec![
            Value::Reference(lookup),
            invoked_name,
            Value::Reference(invoked_type),
        ])
    }

    pub(crate) fn allocate_bootstrap_lookup(
        &mut self,
        caller_class_name: &str,
    ) -> Result<Reference, VmError> {
        self.allocate_bootstrap_lookup_with_modes(caller_class_name, LOOKUP_FULL_POWER_MODES)
    }

    pub(crate) fn allocate_bootstrap_lookup_with_modes(
        &mut self,
        caller_class_name: &str,
        modes: i32,
    ) -> Result<Reference, VmError> {
        self.allocate_bootstrap_lookup_full(caller_class_name, modes, None)
    }

    /// Allocate a Lookup with full coordinates including a previous lookup class
    /// (set by `Lookup.in(Class)` teleporting).
    pub(crate) fn allocate_bootstrap_lookup_full(
        &mut self,
        caller_class_name: &str,
        modes: i32,
        previous_class: Option<&str>,
    ) -> Result<Reference, VmError> {
        self.ensure_lookup_class();
        let class = self.get_class("java/lang/invoke/MethodHandles$Lookup")?;
        let mut fields = vec![Value::Reference(Reference::Null); class.instance_fields.len()];
        if let Some(offset) = class.field_offsets.get("__lookupClass").copied() {
            fields[offset] = Value::Reference(self.class_object(caller_class_name));
        }
        if let Some(offset) = class.field_offsets.get("__modes").copied() {
            fields[offset] = Value::Int(modes);
        }
        if let (Some(offset), Some(prev)) = (
            class.field_offsets.get("__previousLookupClass").copied(),
            previous_class,
        ) {
            fields[offset] = Value::Reference(self.class_object(prev));
        }
        Ok(self.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: "java/lang/invoke/MethodHandles$Lookup".to_string(),
            fields,
        }))
    }

    /// Register the Lookup placeholder class and lazily add fields used by M4
    /// teleporting (`__previousLookupClass`).
    pub(crate) fn ensure_lookup_class(&mut self) {
        self.ensure_bootstrap_placeholder_class(
            "java/lang/invoke/MethodHandles$Lookup",
            vec![
                ("__lookupClass".to_string(), "Ljava/lang/Class;".to_string()),
                ("__modes".to_string(), "I".to_string()),
            ],
        );
        let mut runtime = self.runtime.lock().unwrap();
        if let Some(class) = runtime
            .classes
            .get_mut("java/lang/invoke/MethodHandles$Lookup")
        {
            for (name, descriptor) in [
                ("__modes", "I"),
                ("__previousLookupClass", "Ljava/lang/Class;"),
            ] {
                if !class.field_offsets.contains_key(name) {
                    let offset = class.instance_fields.len();
                    class
                        .instance_fields
                        .push((name.to_string(), descriptor.to_string()));
                    class.field_offsets.insert(name.to_string(), offset);
                }
            }
        }
    }

    fn allocate_bootstrap_method_type(&mut self, descriptor: &str) -> Result<Reference, VmError> {
        self.ensure_bootstrap_placeholder_class(
            "java/lang/invoke/MethodType",
            vec![("__descriptor".to_string(), "Ljava/lang/String;".to_string())],
        );

        let class = self.get_class("java/lang/invoke/MethodType")?;
        let mut fields = vec![Value::Reference(Reference::Null); class.instance_fields.len()];
        if let Some(offset) = class.field_offsets.get("__descriptor").copied() {
            fields[offset] = self.new_string(descriptor.to_string());
        }
        Ok(self.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: "java/lang/invoke/MethodType".to_string(),
            fields,
        }))
    }

    fn ensure_bootstrap_placeholder_class(
        &mut self,
        class_name: &str,
        instance_fields: Vec<(String, String)>,
    ) {
        if self
            .runtime
            .lock()
            .unwrap()
            .classes
            .contains_key(class_name)
        {
            return;
        }

        let field_offsets = instance_fields
            .iter()
            .enumerate()
            .map(|(i, (name, _))| (name.clone(), i))
            .collect();
        self.register_class(RuntimeClass {
            name: class_name.to_string(),
            super_class: Some("java/lang/Object".to_string()),
            methods: HashMap::new(),
            static_fields: HashMap::new(),
            instance_fields,
            field_offsets,
            interfaces: vec![],
        });
    }

    fn linked_dynamic_site_key(site: &InvokeDynamicSite) -> String {
        format!("{}#{}", site.owner_class, site.constant_pool_index)
    }

    /// True for `invokevirtual` targets whose call-site descriptor is treated
    /// as polymorphic (JVMS §5.4.3.5): every site of `MethodHandle.invoke*` or
    /// `VarHandle.<accessMode>` is linked by the descriptor *of the site*, not
    /// of any declared overload.
    pub(crate) fn is_signature_polymorphic(class_name: &str, method_name: &str) -> bool {
        match class_name {
            "java/lang/invoke/MethodHandle" => matches!(
                method_name,
                "invoke" | "invokeExact" | "invokeBasic" | "invokeWithArguments"
            ),
            "java/lang/invoke/VarHandle" => matches!(
                method_name,
                "get"
                    | "set"
                    | "getVolatile"
                    | "setVolatile"
                    | "getAcquire"
                    | "setRelease"
                    | "getOpaque"
                    | "setOpaque"
                    | "compareAndSet"
                    | "compareAndExchange"
                    | "compareAndExchangeAcquire"
                    | "compareAndExchangeRelease"
                    | "weakCompareAndSet"
                    | "weakCompareAndSetPlain"
                    | "weakCompareAndSetAcquire"
                    | "weakCompareAndSetRelease"
                    | "getAndSet"
                    | "getAndSetAcquire"
                    | "getAndSetRelease"
                    | "getAndAdd"
                    | "getAndAddAcquire"
                    | "getAndAddRelease"
                    | "getAndBitwiseOr"
                    | "getAndBitwiseOrAcquire"
                    | "getAndBitwiseOrRelease"
                    | "getAndBitwiseAnd"
                    | "getAndBitwiseAndAcquire"
                    | "getAndBitwiseAndRelease"
                    | "getAndBitwiseXor"
                    | "getAndBitwiseXorAcquire"
                    | "getAndBitwiseXorRelease"
            ),
            _ => false,
        }
    }

    /// Dispatch a signature-polymorphic `invokevirtual`. The receiver is the
    /// `MethodHandle` (or `VarHandle`); `args` is the call-site arguments.
    fn invoke_signature_polymorphic(
        &mut self,
        receiver: Reference,
        method_ref: &MethodRef,
        args: Vec<Value>,
    ) -> Result<Option<Value>, VmError> {
        if receiver == Reference::Null {
            return Err(VmError::UnhandledException {
                class_name: "java/lang/NullPointerException".to_string(),
            });
        }
        match method_ref.class_name.as_str() {
            "java/lang/invoke/MethodHandle" => self.invoke_method_handle(receiver, args),
            "java/lang/invoke/VarHandle" => self.invoke_var_handle_access(
                receiver,
                &method_ref.method_name,
                &method_ref.descriptor,
                args,
            ),
            _ => Err(VmError::MethodNotFound {
                class_name: method_ref.class_name.clone(),
                method_name: method_ref.method_name.clone(),
                descriptor: method_ref.descriptor.clone(),
            }),
        }
    }

    /// Dispatch a signature-polymorphic `VarHandle.<accessMode>` call.
    /// Memory-ordering modes (`*Volatile`, `*Acquire`, `*Release`, `*Opaque`,
    /// plain) all funnel through the heap mutex which gives SeqCst semantics —
    /// safe for `ConcurrentHashMap`, `AtomicInteger`, etc.
    fn invoke_var_handle_access(
        &mut self,
        handle: Reference,
        method_name: &str,
        descriptor: &str,
        args: Vec<Value>,
    ) -> Result<Option<Value>, VmError> {
        let (var_kind, coord_class, coord_name, coord_desc, field_offset) =
            self.read_var_handle_coordinates(handle)?;
        let mode = classify_var_handle_access(method_name).ok_or_else(|| {
            VmError::UnsupportedNativeMethod {
                class_name: "java/lang/invoke/VarHandle".to_string(),
                method_name: method_name.to_string(),
                descriptor: descriptor.to_string(),
            }
        })?;
        match var_kind {
            0 => self.dispatch_instance_field_access(
                mode,
                &coord_class,
                &coord_name,
                &coord_desc,
                field_offset,
                args,
            ),
            1 => self.dispatch_static_field_access(mode, &coord_class, &coord_name, &coord_desc, args),
            2 => self.dispatch_array_access(mode, &coord_desc, args),
            _ => Err(VmError::UnsupportedNativeMethod {
                class_name: "java/lang/invoke/VarHandle".to_string(),
                method_name: method_name.to_string(),
                descriptor: descriptor.to_string(),
            }),
        }
    }

    fn read_var_handle_coordinates(
        &mut self,
        handle: Reference,
    ) -> Result<(i32, String, String, String, i32), VmError> {
        let var_kind = self.get_object_field(handle, "__var_kind")?.as_int()?;
        let coord_class_ref = self
            .get_object_field(handle, "__coord_class")?
            .as_reference()?;
        let coord_name_ref = self
            .get_object_field(handle, "__coord_name")?
            .as_reference()?;
        let coord_desc_ref = self
            .get_object_field(handle, "__coord_desc")?
            .as_reference()?;
        let field_offset = self
            .get_object_field(handle, "__field_offset")?
            .as_int()
            .unwrap_or(-1);
        let coord_class = if coord_class_ref == Reference::Null {
            String::new()
        } else {
            crate::vm::builtin::helpers::class_internal_name(self, coord_class_ref)?
        };
        let coord_name = if coord_name_ref == Reference::Null {
            String::new()
        } else {
            self.stringify_reference(coord_name_ref)?
        };
        let coord_desc = if coord_desc_ref == Reference::Null {
            String::new()
        } else {
            self.stringify_reference(coord_desc_ref)?
        };
        Ok((var_kind, coord_class, coord_name, coord_desc, field_offset))
    }

    fn dispatch_instance_field_access(
        &mut self,
        mode: VarHandleAccess,
        coord_class: &str,
        coord_name: &str,
        coord_desc: &str,
        field_offset: i32,
        args: Vec<Value>,
    ) -> Result<Option<Value>, VmError> {
        let receiver = args
            .first()
            .copied()
            .ok_or(VmError::StackUnderflow)?
            .as_reference()?;
        if receiver == Reference::Null {
            return Err(VmError::UnhandledException {
                class_name: "java/lang/NullPointerException".to_string(),
            });
        }
        let offset = if field_offset >= 0 {
            field_offset as usize
        } else {
            self.get_class(coord_class)?
                .field_offsets
                .get(coord_name)
                .copied()
                .ok_or_else(|| VmError::FieldNotFound {
                    class_name: coord_class.to_string(),
                    field_name: coord_name.to_string(),
                })?
        };
        let payload = &args[1..];
        self.perform_object_field_access(mode, receiver, offset, coord_desc, payload)
    }

    fn dispatch_static_field_access(
        &mut self,
        mode: VarHandleAccess,
        coord_class: &str,
        coord_name: &str,
        coord_desc: &str,
        args: Vec<Value>,
    ) -> Result<Option<Value>, VmError> {
        // Static fields take only the operation values (no receiver).
        self.ensure_class_loaded(coord_class)?;
        self.ensure_class_initialized(coord_class)?;
        self.perform_static_field_access(mode, coord_class, coord_name, coord_desc, &args)
    }

    fn dispatch_array_access(
        &mut self,
        mode: VarHandleAccess,
        coord_desc: &str,
        args: Vec<Value>,
    ) -> Result<Option<Value>, VmError> {
        let array = args
            .first()
            .copied()
            .ok_or(VmError::StackUnderflow)?
            .as_reference()?;
        if array == Reference::Null {
            return Err(VmError::UnhandledException {
                class_name: "java/lang/NullPointerException".to_string(),
            });
        }
        let index = args
            .get(1)
            .copied()
            .ok_or(VmError::StackUnderflow)?
            .as_int()? as usize;
        let payload = &args[2..];
        self.perform_array_element_access(mode, array, index, coord_desc, payload)
    }

    /// Perform a VarHandle access mode on an object instance field. The heap
    /// mutex is held across read-compare-write for CAS / RMW ops.
    fn perform_object_field_access(
        &mut self,
        mode: VarHandleAccess,
        receiver: Reference,
        offset: usize,
        descriptor: &str,
        payload: &[Value],
    ) -> Result<Option<Value>, VmError> {
        let mut heap = self.heap.lock().unwrap();
        let value = heap.get_mut(receiver)?;
        let HeapValue::Object { fields, .. } = value else {
            return Err(VmError::InvalidHeapValue {
                expected: "object",
                actual: value.kind_name(),
            });
        };
        if offset >= fields.len() {
            return Err(VmError::FieldNotFound {
                class_name: String::new(),
                field_name: format!("offset {offset}"),
            });
        }
        let slot = &mut fields[offset];
        apply_var_handle_op(mode, descriptor, slot, payload)
    }

    fn perform_array_element_access(
        &mut self,
        mode: VarHandleAccess,
        array: Reference,
        index: usize,
        element_descriptor: &str,
        payload: &[Value],
    ) -> Result<Option<Value>, VmError> {
        let mut heap = self.heap.lock().unwrap();
        let value = heap.get_mut(array)?;
        apply_var_handle_array_op(mode, element_descriptor, value, index, payload)
    }

    fn perform_static_field_access(
        &mut self,
        mode: VarHandleAccess,
        coord_class: &str,
        coord_name: &str,
        descriptor: &str,
        payload: &[Value],
    ) -> Result<Option<Value>, VmError> {
        let mut runtime = self.runtime.lock().unwrap();
        let class = runtime
            .classes
            .get_mut(coord_class)
            .ok_or_else(|| VmError::ClassNotFound {
                class_name: coord_class.to_string(),
            })?;
        let entry = class
            .static_fields
            .entry(coord_name.to_string())
            .or_insert_with(|| default_value_for_descriptor(descriptor));
        apply_var_handle_op(mode, descriptor, entry, payload)
    }

    /// Resolves a constant-pool entry for `ldc`/`ldc_w`/`ldc2_w`, falling back
    /// to `CONSTANT_Dynamic` bootstrap resolution when the static slot is empty.
    fn load_constant_or_condy(
        &mut self,
        thread: &Thread,
        index: usize,
    ) -> Result<Value, VmError> {
        let condy = thread
            .current_frame()
            .condy_sites
            .get(index)
            .and_then(|slot| slot.as_ref())
            .cloned();
        if let Some(site) = condy {
            return self.resolve_condy_site(&site);
        }
        thread.current_frame().load_constant(index)
    }

    fn get_linked_dynamic_site(&self, site: &InvokeDynamicSite) -> Option<Reference> {
        self.runtime
            .lock()
            .unwrap()
            .linked_dynamic_sites
            .get(&Self::linked_dynamic_site_key(site))
            .copied()
    }

    fn set_linked_dynamic_site(&mut self, site: &InvokeDynamicSite, target: Reference) {
        self.runtime
            .lock()
            .unwrap()
            .linked_dynamic_sites
            .insert(Self::linked_dynamic_site_key(site), target);
    }

    /// Registers the CallSite hierarchy as placeholder classes with a single
    /// `__target: MethodHandle` field. ConstantCallSite is immutable; Mutable
    /// and Volatile are kept distinct so the dispatch path can tell whether to
    /// re-read the target on every invocation.
    pub(crate) fn ensure_callsite_classes(&mut self) {
        let fields = vec![("__target".to_string(), "Ljava/lang/invoke/MethodHandle;".to_string())];
        for class_name in [
            "java/lang/invoke/CallSite",
            "java/lang/invoke/ConstantCallSite",
            "java/lang/invoke/MutableCallSite",
            "java/lang/invoke/VolatileCallSite",
        ] {
            self.ensure_bootstrap_placeholder_class(class_name, fields.clone());
        }
    }

    /// Parse the provided classfile bytes, register the class under a unique
    /// synthetic name derived from `caller_class`, and optionally initialize it.
    /// Returns the synthetic class name.
    pub(crate) fn define_hidden_class(
        &mut self,
        caller_class: &str,
        bytes_ref: Reference,
        initialize: bool,
    ) -> Result<String, VmError> {
        // Pull the bytes out of the byte[] heap value.
        let bytes: Vec<u8> = {
            let heap = self.heap.lock().unwrap();
            match heap.get(bytes_ref)? {
                HeapValue::IntArray { values } => values.iter().map(|v| *v as u8).collect(),
                other => {
                    return Err(VmError::InvalidHeapValue {
                        expected: "byte-array",
                        actual: other.kind_name(),
                    });
                }
            }
        };
        let class_file = crate::classfile::ClassFile::parse(&bytes).map_err(|_| {
            VmError::InvalidDescriptor {
                descriptor: "hidden classfile parse failed".to_string(),
            }
        })?;
        // Allocate a unique synthetic name. Hidden class names use the JDK
        // convention `Owner+suffix` but jvm-rs has no notion of slash/dot
        // packaging, so we just append a counter.
        static HIDDEN_COUNTER: std::sync::atomic::AtomicUsize =
            std::sync::atomic::AtomicUsize::new(0);
        let id = HIDDEN_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let synthetic_name = format!("{}$$HIDDEN$${}", caller_class, id);
        crate::launcher::register_class(&synthetic_name, &class_file, self).map_err(|_| {
            VmError::InvalidDescriptor {
                descriptor: format!("hidden class registration failed for {synthetic_name}"),
            }
        })?;
        if initialize {
            self.ensure_class_initialized(&synthetic_name)?;
        }
        Ok(synthetic_name)
    }

    /// Register `java/lang/invoke/MethodHandleInfo` as a placeholder class and
    /// populate one from an existing direct MethodHandle.
    pub(crate) fn allocate_method_handle_info(
        &mut self,
        handle: Reference,
    ) -> Result<Reference, VmError> {
        self.ensure_bootstrap_placeholder_class(
            "java/lang/invoke/MethodHandleInfo",
            vec![
                ("__referenceKind".to_string(), "I".to_string()),
                ("__declaringClass".to_string(), "Ljava/lang/Class;".to_string()),
                ("__name".to_string(), "Ljava/lang/String;".to_string()),
                (
                    "__methodType".to_string(),
                    "Ljava/lang/invoke/MethodType;".to_string(),
                ),
            ],
        );
        let kind = self.get_object_field(handle, "__kind")?;
        let declaring = self.get_object_field(handle, "__targetClass")?;
        let name = self.get_object_field(handle, "__targetName")?;
        let descriptor_obj = self.get_object_field(handle, "__targetDesc")?;
        let class = self.get_class("java/lang/invoke/MethodHandleInfo")?;
        let mut fields = vec![Value::Reference(Reference::Null); class.instance_fields.len()];
        if let Some(o) = class.field_offsets.get("__referenceKind").copied() {
            fields[o] = kind;
        }
        if let Some(o) = class.field_offsets.get("__declaringClass").copied() {
            fields[o] = declaring;
        }
        if let Some(o) = class.field_offsets.get("__name").copied() {
            fields[o] = name;
        }
        if let Some(o) = class.field_offsets.get("__methodType").copied() {
            fields[o] = descriptor_obj;
        }
        Ok(self.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: "java/lang/invoke/MethodHandleInfo".to_string(),
            fields,
        }))
    }

    /// Registers `java/lang/invoke/VarHandle` as a placeholder class with the
    /// coordinates needed to dispatch field / array access modes.
    pub(crate) fn ensure_var_handle_class(&mut self) {
        let fields = vec![
            ("__var_kind".to_string(), "I".to_string()),
            ("__coord_class".to_string(), "Ljava/lang/Class;".to_string()),
            ("__coord_name".to_string(), "Ljava/lang/String;".to_string()),
            ("__coord_desc".to_string(), "Ljava/lang/String;".to_string()),
            ("__field_offset".to_string(), "I".to_string()),
        ];
        self.ensure_bootstrap_placeholder_class("java/lang/invoke/VarHandle", fields);
    }

    /// Allocate a new VarHandle. `var_kind` is 0 (instance field), 1 (static
    /// field), or 2 (array element). For instance/static VHs `coord_class` is
    /// the declaring class, `coord_name` the field name, and `coord_desc` the
    /// field's type descriptor. For array VHs `coord_class` is the array class
    /// (e.g. `[I`), `coord_name` is empty, and `coord_desc` is the element
    /// descriptor.
    pub(crate) fn allocate_var_handle(
        &mut self,
        var_kind: i32,
        coord_class: &str,
        coord_name: &str,
        coord_desc: &str,
    ) -> Result<Reference, VmError> {
        self.ensure_var_handle_class();
        let coord_class_ref = self.class_object(coord_class);
        let coord_name_value = self.new_string(coord_name.to_string());
        let coord_desc_value = self.new_string(coord_desc.to_string());
        let field_offset = if var_kind == 0 {
            let class = self.get_class(coord_class)?;
            class
                .field_offsets
                .get(coord_name)
                .copied()
                .map(|v| v as i32)
                .unwrap_or(-1)
        } else {
            -1
        };
        let class = self.get_class("java/lang/invoke/VarHandle")?;
        let mut fields = vec![Value::Reference(Reference::Null); class.instance_fields.len()];
        if let Some(o) = class.field_offsets.get("__var_kind").copied() {
            fields[o] = Value::Int(var_kind);
        }
        if let Some(o) = class.field_offsets.get("__coord_class").copied() {
            fields[o] = Value::Reference(coord_class_ref);
        }
        if let Some(o) = class.field_offsets.get("__coord_name").copied() {
            fields[o] = coord_name_value;
        }
        if let Some(o) = class.field_offsets.get("__coord_desc").copied() {
            fields[o] = coord_desc_value;
        }
        if let Some(o) = class.field_offsets.get("__field_offset").copied() {
            fields[o] = Value::Int(field_offset);
        }
        Ok(self.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: "java/lang/invoke/VarHandle".to_string(),
            fields,
        }))
    }

    fn ensure_method_handle_class(&mut self) {
        let fields = vec![
            ("__kind".to_string(), "I".to_string()),
            ("__targetClass".to_string(), "Ljava/lang/Class;".to_string()),
            ("__targetName".to_string(), "Ljava/lang/String;".to_string()),
            (
                "__targetDesc".to_string(),
                "Ljava/lang/invoke/MethodType;".to_string(),
            ),
            (
                "__constantValue".to_string(),
                "Ljava/lang/Object;".to_string(),
            ),
            ("__lookupClass".to_string(), "Ljava/lang/Class;".to_string()),
        ];
        self.ensure_bootstrap_placeholder_class("java/lang/invoke/MethodHandle", fields);
        self.ensure_method_handle_extra_fields(&[(
            "__lookupClass",
            "Ljava/lang/Class;",
        )]);
    }

    /// Lazily adds extra instance fields to `java/lang/invoke/MethodHandle`.
    /// Derived (combinator) kinds use this to store their per-adapter state
    /// without rewriting the existing layout for direct handles.
    pub(crate) fn ensure_method_handle_extra_fields(&mut self, fields: &[(&str, &str)]) {
        let mut runtime = self.runtime.lock().unwrap();
        let Some(class) = runtime.classes.get_mut("java/lang/invoke/MethodHandle") else {
            return;
        };
        for (name, descriptor) in fields {
            if class.field_offsets.contains_key(*name) {
                continue;
            }
            let offset = class.instance_fields.len();
            class
                .instance_fields
                .push(((*name).to_string(), (*descriptor).to_string()));
            class.field_offsets.insert((*name).to_string(), offset);
        }
    }

    /// Public alias of `split_method_descriptor` for use by the combinator
    /// helper module without exposing the underlying private utility broadly.
    pub(crate) fn split_method_descriptor_public(
        descriptor: &str,
    ) -> Option<(Vec<String>, String)> {
        Self::split_method_descriptor(descriptor)
    }

    /// Splits a method descriptor `(...)R` into per-argument descriptor strings
    /// plus the return descriptor. Reference types keep their full `L..;` and
    /// array types keep their full `[..` shape, unlike `parse_arg_types` which
    /// returns only first-byte type codes.
    fn split_method_descriptor(descriptor: &str) -> Option<(Vec<String>, String)> {
        let bytes = descriptor.as_bytes();
        if bytes.first() != Some(&b'(') {
            return None;
        }
        let mut args = Vec::new();
        let mut i = 1;
        while i < bytes.len() && bytes[i] != b')' {
            let start = i;
            match bytes[i] {
                b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z' | b'V' => {
                    i += 1;
                }
                b'L' => {
                    i += 1;
                    while i < bytes.len() && bytes[i] != b';' {
                        i += 1;
                    }
                    if i >= bytes.len() {
                        return None;
                    }
                    i += 1;
                }
                b'[' => {
                    while i < bytes.len() && bytes[i] == b'[' {
                        i += 1;
                    }
                    if i >= bytes.len() {
                        return None;
                    }
                    if bytes[i] == b'L' {
                        i += 1;
                        while i < bytes.len() && bytes[i] != b';' {
                            i += 1;
                        }
                        if i >= bytes.len() {
                            return None;
                        }
                        i += 1;
                    } else {
                        i += 1;
                    }
                }
                _ => return None,
            }
            args.push(String::from_utf8_lossy(&bytes[start..i]).into_owned());
        }
        if i >= bytes.len() || bytes[i] != b')' {
            return None;
        }
        i += 1;
        if i >= bytes.len() {
            return None;
        }
        let ret = String::from_utf8_lossy(&bytes[i..]).into_owned();
        Some((args, ret))
    }

    /// Returns the wrapper class internal name for a primitive descriptor
    /// (e.g. `I` → `java/lang/Integer`). Returns `None` for non-primitive
    /// descriptors.
    fn primitive_wrapper_class(descriptor: &str) -> Option<&'static str> {
        match descriptor {
            "I" => Some("java/lang/Integer"),
            "J" => Some("java/lang/Long"),
            "F" => Some("java/lang/Float"),
            "D" => Some("java/lang/Double"),
            "Z" => Some("java/lang/Boolean"),
            "B" => Some("java/lang/Byte"),
            "C" => Some("java/lang/Character"),
            "S" => Some("java/lang/Short"),
            "V" => Some("java/lang/Void"),
            _ => None,
        }
    }

    /// Ensures `java/lang/Integer` / `java/lang/Long` etc. exist as placeholder
    /// classes with a single `value:<descriptor>` field. Used for direct
    /// allocation of wrapper objects in the boxing helper when the JDK natives
    /// aren't registered (`Float`/`Double`/`Byte`/`Character`/`Short`).
    fn ensure_wrapper_class(&mut self, wrapper: &str, value_descriptor: &str) {
        let already = self
            .runtime
            .lock()
            .unwrap()
            .classes
            .contains_key(wrapper);
        if already {
            // Make sure the `value` field exists in case the placeholder was
            // registered without it.
            let mut runtime = self.runtime.lock().unwrap();
            if let Some(class) = runtime.classes.get_mut(wrapper) {
                if !class.field_offsets.contains_key("value") {
                    let offset = class.instance_fields.len();
                    class
                        .instance_fields
                        .push(("value".to_string(), value_descriptor.to_string()));
                    class.field_offsets.insert("value".to_string(), offset);
                }
            }
            return;
        }
        self.ensure_bootstrap_placeholder_class(
            wrapper,
            vec![("value".to_string(), value_descriptor.to_string())],
        );
    }

    /// Box a primitive `Value` into the JVM wrapper object. Uses existing
    /// `*.valueOf` natives where present so the resulting object matches the
    /// shape other VM code expects; falls back to direct allocation for
    /// wrappers without a native (`Float`/`Double`/`Byte`/`Character`/`Short`).
    pub(crate) fn box_primitive_value(
        &mut self,
        value: Value,
        primitive_descriptor: &str,
    ) -> Result<Reference, VmError> {
        match primitive_descriptor {
            "I" => self
                .reflect_invoke_method(
                    "java/lang/Integer",
                    "valueOf",
                    "(I)Ljava/lang/Integer;",
                    None,
                    vec![value],
                )?
                .unwrap_or(Value::Reference(Reference::Null))
                .as_reference(),
            "J" => self
                .reflect_invoke_method(
                    "java/lang/Long",
                    "valueOf",
                    "(J)Ljava/lang/Long;",
                    None,
                    vec![value],
                )?
                .unwrap_or(Value::Reference(Reference::Null))
                .as_reference(),
            "Z" => self
                .reflect_invoke_method(
                    "java/lang/Boolean",
                    "valueOf",
                    "(Z)Ljava/lang/Boolean;",
                    None,
                    vec![value],
                )?
                .unwrap_or(Value::Reference(Reference::Null))
                .as_reference(),
            "F" | "D" | "B" | "C" | "S" => {
                let wrapper = Self::primitive_wrapper_class(primitive_descriptor).ok_or(
                    VmError::TypeMismatch {
                        expected: "primitive descriptor",
                        actual: "non-primitive",
                    },
                )?;
                self.ensure_wrapper_class(wrapper, primitive_descriptor);
                let class = self.get_class(wrapper)?;
                let mut fields = vec![Value::Reference(Reference::Null); class.instance_fields.len()];
                if let Some(offset) = class.field_offsets.get("value").copied() {
                    fields[offset] = value;
                }
                Ok(self.heap.lock().unwrap().allocate(HeapValue::Object {
                    class_name: wrapper.to_string(),
                    fields,
                }))
            }
            _ => Err(VmError::TypeMismatch {
                expected: "primitive descriptor",
                actual: "non-primitive",
            }),
        }
    }

    /// Unbox a wrapper-object reference into a primitive `Value`.
    pub(crate) fn unbox_primitive_value(
        &mut self,
        reference: Reference,
        primitive_descriptor: &str,
    ) -> Result<Value, VmError> {
        if reference == Reference::Null {
            return Err(VmError::UnhandledException {
                class_name: "java/lang/NullPointerException".to_string(),
            });
        }
        let class_name = self.get_object_class(reference)?;
        let expected = Self::primitive_wrapper_class(primitive_descriptor).ok_or(
            VmError::TypeMismatch {
                expected: "primitive descriptor",
                actual: "non-primitive",
            },
        )?;
        if class_name != expected {
            return Err(VmError::UnhandledException {
                class_name: "java/lang/ClassCastException".to_string(),
            });
        }
        let value = self.get_object_field(reference, "value")?;
        // Coerce stored value to the requested primitive width so unboxing a
        // wrapper that secretly holds a narrower type still yields the right
        // JVMS type.
        Ok(match primitive_descriptor {
            "I" | "B" | "C" | "S" | "Z" => Value::Int(value.as_int().unwrap_or(0)),
            "J" => Value::Long(value.as_long().unwrap_or(0)),
            "F" => Value::Float(value.as_float().unwrap_or(0.0)),
            "D" => Value::Double(value.as_double().unwrap_or(0.0)),
            _ => value,
        })
    }

    /// Returns whether `value` is assignable to the reference type `target`.
    /// `target` is a JVMS descriptor in either `Lclass;` or `[..` form. `null`
    /// is assignable to any reference type. Primitives never satisfy a
    /// reference type and vice versa.
    fn reference_assignable(&mut self, value: Reference, target: &str) -> Result<bool, VmError> {
        if value == Reference::Null {
            return Ok(true);
        }
        if target == "Ljava/lang/Object;" {
            return Ok(true);
        }
        let actual_class = self.get_object_class(value)?;
        let target_class = if target.starts_with('L') && target.ends_with(';') {
            &target[1..target.len() - 1]
        } else {
            target
        };
        self.is_instance_of(&actual_class, target_class)
    }

    /// JVMS §5.4.3.5 / MethodHandle.asType coercion of a single value from
    /// `from` descriptor to `to` descriptor. Mismatched primitive pairs
    /// (e.g. `J → I`) and unsupported reference casts raise the appropriate
    /// runtime exception via `VmError::UnhandledException`.
    pub(crate) fn coerce_method_handle_value(
        &mut self,
        value: Value,
        from: &str,
        to: &str,
    ) -> Result<Value, VmError> {
        if from == to {
            return Ok(value);
        }
        let from_byte = from.as_bytes().first().copied().unwrap_or(b'?');
        let to_byte = to.as_bytes().first().copied().unwrap_or(b'?');
        let from_is_prim = matches!(
            from_byte,
            b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z' | b'V'
        );
        let to_is_prim = matches!(
            to_byte,
            b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z' | b'V'
        );

        // Reference -> reference: identity, with optional runtime cast check.
        if !from_is_prim && !to_is_prim {
            if let Value::Reference(r) = value {
                if !self.reference_assignable(r, to)? {
                    return Err(VmError::UnhandledException {
                        class_name: "java/lang/ClassCastException".to_string(),
                    });
                }
            }
            return Ok(value);
        }

        // Primitive -> primitive: widening only.
        if from_is_prim && to_is_prim {
            return self.widen_primitive(value, from_byte, to_byte);
        }

        // Primitive -> reference: box (or widen first when target is Object/Number/wrapper).
        if from_is_prim && !to_is_prim {
            let wrapper = Self::primitive_wrapper_class(from).ok_or(VmError::TypeMismatch {
                expected: "primitive",
                actual: "unknown",
            })?;
            let boxed = self.box_primitive_value(value, from)?;
            // Verify the boxed object is assignable to the target reference type.
            if to != "Ljava/lang/Object;" {
                let to_class = if to.starts_with('L') && to.ends_with(';') {
                    &to[1..to.len() - 1]
                } else {
                    wrapper
                };
                if !self.is_instance_of(wrapper, to_class)? {
                    return Err(VmError::UnhandledException {
                        class_name: "java/lang/ClassCastException".to_string(),
                    });
                }
            }
            return Ok(Value::Reference(boxed));
        }

        // Reference -> primitive: unbox the matching wrapper, then widen.
        if !from_is_prim && to_is_prim {
            let reference = value.as_reference()?;
            // Pick the most specific wrapper type to unbox from. We try the
            // target primitive's matching wrapper first; if the object isn't
            // that wrapper, fall through to a ClassCastException.
            let wrapper = Self::primitive_wrapper_class(to).ok_or(VmError::TypeMismatch {
                expected: "primitive",
                actual: "unknown",
            })?;
            if reference == Reference::Null {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/NullPointerException".to_string(),
                });
            }
            let actual_class = self.get_object_class(reference)?;
            if actual_class != wrapper {
                // Allow widening across wrappers (e.g. Integer -> long): unbox
                // using the actual class then widen.
                if let Some(actual_prim) = match actual_class.as_str() {
                    "java/lang/Integer" => Some("I"),
                    "java/lang/Long" => Some("J"),
                    "java/lang/Float" => Some("F"),
                    "java/lang/Double" => Some("D"),
                    "java/lang/Byte" => Some("B"),
                    "java/lang/Character" => Some("C"),
                    "java/lang/Short" => Some("S"),
                    "java/lang/Boolean" => Some("Z"),
                    _ => None,
                } {
                    let primitive = self.unbox_primitive_value(reference, actual_prim)?;
                    return self.widen_primitive(
                        primitive,
                        actual_prim.as_bytes()[0],
                        to_byte,
                    );
                }
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/ClassCastException".to_string(),
                });
            }
            return self.unbox_primitive_value(reference, to);
        }

        Ok(value)
    }

    /// JVMS widening of a primitive value to a wider primitive type. Identity
    /// when `from == to`. Returns `WrongMethodTypeException` for any narrowing.
    fn widen_primitive(
        &mut self,
        value: Value,
        from: u8,
        to: u8,
    ) -> Result<Value, VmError> {
        if from == to {
            return Ok(value);
        }
        let as_int = || -> Result<i32, VmError> {
            match value {
                Value::Int(v) => Ok(v),
                _ => Err(VmError::TypeMismatch {
                    expected: "int",
                    actual: value.type_name(),
                }),
            }
        };
        let as_long = || -> Result<i64, VmError> {
            match value {
                Value::Long(v) => Ok(v),
                _ => Err(VmError::TypeMismatch {
                    expected: "long",
                    actual: value.type_name(),
                }),
            }
        };
        let as_float = || -> Result<f32, VmError> {
            match value {
                Value::Float(v) => Ok(v),
                _ => Err(VmError::TypeMismatch {
                    expected: "float",
                    actual: value.type_name(),
                }),
            }
        };
        let result = match (from, to) {
            // int-shaped widening (byte/char/short/bool all map to Int storage).
            (b'B' | b'C' | b'S' | b'Z' | b'I', b'I') => Some(Value::Int(as_int()?)),
            (b'B' | b'C' | b'S' | b'Z' | b'I', b'J') => Some(Value::Long(as_int()? as i64)),
            (b'B' | b'C' | b'S' | b'Z' | b'I', b'F') => Some(Value::Float(as_int()? as f32)),
            (b'B' | b'C' | b'S' | b'Z' | b'I', b'D') => Some(Value::Double(as_int()? as f64)),
            (b'J', b'F') => Some(Value::Float(as_long()? as f32)),
            (b'J', b'D') => Some(Value::Double(as_long()? as f64)),
            (b'F', b'D') => Some(Value::Double(as_float()? as f64)),
            _ => None,
        };
        result.ok_or(VmError::UnhandledException {
            class_name: "java/lang/invoke/WrongMethodTypeException".to_string(),
        })
    }

    /// Allocates a derived MethodHandle (combinator) of the given kind. The
    /// caller passes the post-transformation descriptor and the adapter-
    /// specific field values; any missing fields stay at the default `null` /
    /// `0` value. Adapter fields are lazily registered onto the MethodHandle
    /// placeholder class so existing direct-handle layouts stay intact.
    pub(crate) fn allocate_derived_method_handle(
        &mut self,
        kind: i32,
        target_descriptor: &str,
        extras: Vec<(&str, Value)>,
    ) -> Result<Reference, VmError> {
        self.ensure_method_handle_class();
        // Register any combinator field this allocation needs.
        let to_add: Vec<(&str, &str)> = extras
            .iter()
            .map(|(name, value)| (*name, Self::adapter_field_descriptor(name, value)))
            .collect();
        self.ensure_method_handle_extra_fields(&to_add);
        let target_desc_ref =
            Value::Reference(self.allocate_bootstrap_method_type(target_descriptor)?);
        let class = self.get_class("java/lang/invoke/MethodHandle")?;
        let mut fields = vec![Value::Reference(Reference::Null); class.instance_fields.len()];
        if let Some(offset) = class.field_offsets.get("__kind").copied() {
            fields[offset] = Value::Int(kind);
        }
        if let Some(offset) = class.field_offsets.get("__targetDesc").copied() {
            fields[offset] = target_desc_ref;
        }
        for (name, value) in extras {
            if let Some(offset) = class.field_offsets.get(name).copied() {
                fields[offset] = value;
            }
        }
        Ok(self.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: "java/lang/invoke/MethodHandle".to_string(),
            fields,
        }))
    }

    /// Guess the descriptor for a combinator field given its name and stored
    /// value kind. Integer slots get `I`, everything else gets `Ljava/lang/Object;`
    /// (we don't read these via reflection — the descriptor is just for the
    /// placeholder class metadata).
    fn adapter_field_descriptor(_name: &str, value: &Value) -> &'static str {
        match value {
            Value::Int(_) => "I",
            Value::Long(_) => "J",
            Value::Float(_) => "F",
            Value::Double(_) => "D",
            _ => "Ljava/lang/Object;",
        }
    }

    fn resolve_bootstrap_argument_value(
        &mut self,
        argument: &BootstrapArgument,
    ) -> Result<Option<Value>, VmError> {
        match argument {
            BootstrapArgument::Int(value) => Ok(Some(Value::Int(*value))),
            BootstrapArgument::Long(value) => Ok(Some(Value::Long(*value))),
            BootstrapArgument::Float(value) => Ok(Some(Value::Float(*value))),
            BootstrapArgument::Double(value) => Ok(Some(Value::Double(*value))),
            BootstrapArgument::String(value) => Ok(Some(self.new_string(value.clone()))),
            BootstrapArgument::Class(class_name) => {
                Ok(Some(Value::Reference(self.class_object(class_name))))
            }
            BootstrapArgument::MethodType(descriptor) => Ok(Some(Value::Reference(
                self.allocate_bootstrap_method_type(descriptor)?,
            ))),
            BootstrapArgument::MethodHandle {
                reference_kind,
                target_class,
                target_method,
                target_descriptor,
            } => Ok(Some(Value::Reference(
                self.allocate_bootstrap_method_handle(
                    *reference_kind,
                    target_class,
                    target_method,
                    target_descriptor,
                    None,
                )?,
            ))),
            BootstrapArgument::Dynamic {
                name,
                descriptor,
                bootstrap_class,
                bootstrap_name,
                bootstrap_descriptor,
                arguments,
            } => Ok(Some(self.resolve_condy_nested(
                name,
                descriptor,
                bootstrap_class,
                bootstrap_name,
                bootstrap_descriptor,
                arguments,
            )?)),
        }
    }

    /// Resolves a `CONSTANT_Dynamic` constant given its parsed bootstrap info.
    /// Tries fast paths in `java/lang/invoke/ConstantBootstraps` before
    /// dispatching the bootstrap method like any other invokedynamic.
    fn resolve_condy_nested(
        &mut self,
        name: &str,
        descriptor: &str,
        bootstrap_class: &str,
        bootstrap_name: &str,
        bootstrap_descriptor: &str,
        arguments: &[BootstrapArgument],
    ) -> Result<Value, VmError> {
        if let Some(value) =
            self.try_constant_bootstraps_fast_path(bootstrap_class, bootstrap_name, name, descriptor, arguments)?
        {
            return Ok(value);
        }
        let lookup = self.allocate_bootstrap_lookup("java/lang/Object")?;
        let invoked_name = self.new_string(name.to_string());
        let invoked_type = self.condy_type_object(descriptor)?;
        let mut bootstrap_args = vec![
            Value::Reference(lookup),
            invoked_name,
            invoked_type,
        ];
        for arg in arguments {
            if let Some(val) = self.resolve_bootstrap_argument_value(arg)? {
                bootstrap_args.push(val);
            }
        }
        let result = self.reflect_invoke_method(
            bootstrap_class,
            bootstrap_name,
            bootstrap_descriptor,
            None,
            bootstrap_args,
        )?;
        Ok(result.unwrap_or(Value::Reference(Reference::Null)))
    }

    /// Resolves a `CondySite` and caches the result keyed by `(owner_class, cp_index)`.
    pub(crate) fn resolve_condy_site(
        &mut self,
        site: &CondySite,
    ) -> Result<Value, VmError> {
        let key = format!("{}#{}", site.owner_class, site.constant_pool_index);
        if let Some(value) = self
            .runtime
            .lock()
            .unwrap()
            .linked_condy_constants
            .get(&key)
            .copied()
        {
            return Ok(value);
        }
        let value = self.resolve_condy_nested(
            &site.name,
            &site.descriptor,
            &site.bootstrap_class,
            &site.bootstrap_name,
            &site.bootstrap_descriptor,
            &site.arguments,
        )?;
        self.runtime
            .lock()
            .unwrap()
            .linked_condy_constants
            .insert(key, value);
        Ok(value)
    }

    /// For condy, the third bootstrap arg is `Class<?>` (the constant type) for
    /// `ConstantBootstraps`-style bootstraps; for arbitrary user bootstraps it
    /// is a `MethodType`. Detect by leading `(` to decide; otherwise treat as a
    /// field descriptor and pass a `Class` object.
    fn condy_type_object(&mut self, descriptor: &str) -> Result<Value, VmError> {
        if descriptor.starts_with('(') {
            Ok(Value::Reference(
                self.allocate_bootstrap_method_type(descriptor)?,
            ))
        } else {
            let class_name = match descriptor {
                "I" => "java/lang/Integer",
                "J" => "java/lang/Long",
                "F" => "java/lang/Float",
                "D" => "java/lang/Double",
                "Z" => "java/lang/Boolean",
                "B" => "java/lang/Byte",
                "C" => "java/lang/Character",
                "S" => "java/lang/Short",
                "V" => "java/lang/Void",
                other if other.starts_with('L') && other.ends_with(';') => {
                    &other[1..other.len() - 1]
                }
                other => other,
            };
            Ok(Value::Reference(self.class_object(class_name)))
        }
    }

    /// Fast-path implementations of standard `java.lang.invoke.ConstantBootstraps`
    /// entries so we don't need real JDK bytecode for the common condy cases.
    fn try_constant_bootstraps_fast_path(
        &mut self,
        bootstrap_class: &str,
        bootstrap_name: &str,
        name: &str,
        descriptor: &str,
        arguments: &[BootstrapArgument],
    ) -> Result<Option<Value>, VmError> {
        if bootstrap_class != "java/lang/invoke/ConstantBootstraps" {
            return Ok(None);
        }
        match bootstrap_name {
            "nullConstant" => Ok(Some(Value::Reference(Reference::Null))),
            "primitiveClass" => {
                let class_name = match name {
                    "I" => "I",
                    "J" => "J",
                    "F" => "F",
                    "D" => "D",
                    "Z" => "Z",
                    "B" => "B",
                    "C" => "C",
                    "S" => "S",
                    "V" => "V",
                    other => other,
                };
                Ok(Some(Value::Reference(self.class_object(class_name))))
            }
            "getStaticFinal" => {
                let owner = match arguments.first() {
                    Some(BootstrapArgument::Class(c)) => c.clone(),
                    _ => {
                        let desc = descriptor.trim_start_matches('L').trim_end_matches(';');
                        desc.to_string()
                    }
                };
                self.ensure_class_loaded(&owner)?;
                self.ensure_class_initialized(&owner)?;
                Ok(Some(self.get_static_field(&owner, name)?))
            }
            "enumConstant" => {
                let owner = descriptor.trim_start_matches('L').trim_end_matches(';');
                self.ensure_class_loaded(owner)?;
                self.ensure_class_initialized(owner)?;
                Ok(Some(self.get_static_field(owner, name)?))
            }
            "invoke" => {
                // arg 0 is a MethodHandle, remaining args are passed to it.
                let mut iter = arguments.iter();
                let mh_arg = iter.next().ok_or(VmError::StackUnderflow)?;
                let handle_value = self
                    .resolve_bootstrap_argument_value(mh_arg)?
                    .unwrap_or(Value::Reference(Reference::Null));
                let handle_ref = handle_value.as_reference()?;
                let mut args = Vec::new();
                for arg in iter {
                    if let Some(val) = self.resolve_bootstrap_argument_value(arg)? {
                        args.push(val);
                    }
                }
                Ok(Some(
                    self.invoke_method_handle(handle_ref, args)?
                        .unwrap_or(Value::Reference(Reference::Null)),
                ))
            }
            _ => Ok(None),
        }
    }

    fn allocate_bootstrap_method_handle(
        &mut self,
        reference_kind: u8,
        target_class: &str,
        target_method: &str,
        target_descriptor: &str,
        constant_value: Option<Value>,
    ) -> Result<Reference, VmError> {
        self.allocate_bootstrap_method_handle_with_lookup(
            reference_kind,
            target_class,
            target_method,
            target_descriptor,
            constant_value,
            None,
        )
    }

    pub(crate) fn allocate_bootstrap_method_handle_with_lookup(
        &mut self,
        reference_kind: u8,
        target_class: &str,
        target_method: &str,
        target_descriptor: &str,
        constant_value: Option<Value>,
        lookup_class: Option<&str>,
    ) -> Result<Reference, VmError> {
        self.ensure_method_handle_class();
        let class = self.get_class("java/lang/invoke/MethodHandle")?;
        let mut fields = vec![Value::Reference(Reference::Null); class.instance_fields.len()];
        if let Some(offset) = class.field_offsets.get("__kind").copied() {
            fields[offset] = Value::Int(reference_kind as i32);
        }
        if let Some(offset) = class.field_offsets.get("__targetClass").copied() {
            fields[offset] = Value::Reference(self.class_object(target_class));
        }
        if let Some(offset) = class.field_offsets.get("__targetName").copied() {
            fields[offset] = self.new_string(target_method.to_string());
        }
        if let Some(offset) = class.field_offsets.get("__targetDesc").copied() {
            fields[offset] =
                Value::Reference(self.allocate_bootstrap_method_type(target_descriptor)?);
        }
        if let Some(offset) = class.field_offsets.get("__constantValue").copied() {
            fields[offset] = constant_value.unwrap_or(Value::Reference(Reference::Null));
        }
        if let (Some(offset), Some(lookup_class)) = (
            class.field_offsets.get("__lookupClass").copied(),
            lookup_class,
        ) {
            fields[offset] = Value::Reference(self.class_object(lookup_class));
        }
        Ok(self.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: "java/lang/invoke/MethodHandle".to_string(),
            fields,
        }))
    }

    fn extract_linked_dynamic_target(
        &mut self,
        result: Option<Value>,
    ) -> Result<Reference, VmError> {
        let value = result.unwrap_or(Value::Reference(Reference::Null));
        let reference = value.as_reference()?;
        if reference == Reference::Null {
            return Err(VmError::UnhandledException {
                class_name: "java/lang/NullPointerException".to_string(),
            });
        }
        let class_name = self.get_object_class(reference)?;
        match class_name.as_str() {
            "java/lang/invoke/MethodHandle" => Ok(reference),
            // Eagerly unwrap immutable call sites; mutable / volatile sites are
            // kept as the call-site reference so subsequent `setTarget` calls
            // are observed by later invocations.
            "java/lang/invoke/ConstantCallSite" | "java/lang/invoke/CallSite" => {
                self.get_object_field(reference, "__target")?.as_reference()
            }
            "java/lang/invoke/MutableCallSite" | "java/lang/invoke/VolatileCallSite" => {
                Ok(reference)
            }
            _ => Err(VmError::TypeMismatch {
                expected: "java/lang/invoke/CallSite or MethodHandle",
                actual: "unexpected bootstrap result",
            }),
        }
    }

    /// Translate a cached indy binding (either a MethodHandle or a CallSite
    /// reference) into the MethodHandle to invoke. For Mutable/VolatileCallSite
    /// the current `__target` is re-read on every invocation so `setTarget`
    /// changes take effect immediately.
    fn resolve_dynamic_target(&mut self, linked: Reference) -> Result<Reference, VmError> {
        if linked == Reference::Null {
            return Err(VmError::UnhandledException {
                class_name: "java/lang/NullPointerException".to_string(),
            });
        }
        let class_name = self.get_object_class(linked)?;
        match class_name.as_str() {
            "java/lang/invoke/MethodHandle" => Ok(linked),
            "java/lang/invoke/MutableCallSite" | "java/lang/invoke/VolatileCallSite" => {
                self.get_object_field(linked, "__target")?.as_reference()
            }
            _ => Ok(linked),
        }
    }

    fn invoke_method_handle(
        &mut self,
        handle_ref: Reference,
        args: Vec<Value>,
    ) -> Result<Option<Value>, VmError> {
        let raw_kind = self.get_object_field(handle_ref, "__kind")?.as_int()?;
        if raw_kind >= MH_KIND_BIND_TO {
            return self.invoke_derived_method_handle(handle_ref, raw_kind, args);
        }
        let (kind, target_class, target_name, target_descriptor, constant_value, lookup_class) = {
            let kind = raw_kind;
            let target_class_ref = self
                .get_object_field(handle_ref, "__targetClass")?
                .as_reference()?;
            let target_name_ref = self
                .get_object_field(handle_ref, "__targetName")?
                .as_reference()?;
            let target_desc_ref = self
                .get_object_field(handle_ref, "__targetDesc")?
                .as_reference()?;
            let constant_value = self.get_object_field(handle_ref, "__constantValue")?;
            let lookup_class_ref = self
                .get_object_field(handle_ref, "__lookupClass")
                .ok()
                .and_then(|value| value.as_reference().ok())
                .unwrap_or(Reference::Null);
            let target_class = if target_class_ref == Reference::Null {
                String::new()
            } else {
                crate::vm::builtin::helpers::class_internal_name(self, target_class_ref)?
            };
            let target_name = if target_name_ref == Reference::Null {
                String::new()
            } else {
                self.stringify_reference(target_name_ref)?
            };
            let target_descriptor = if target_desc_ref == Reference::Null {
                String::new()
            } else {
                let desc_ref = self
                    .get_object_field(target_desc_ref, "__descriptor")?
                    .as_reference()?;
                self.stringify_reference(desc_ref)?
            };
            let lookup_class = if lookup_class_ref == Reference::Null {
                None
            } else {
                Some(crate::vm::builtin::helpers::class_internal_name(
                    self,
                    lookup_class_ref,
                )?)
            };
            (
                kind,
                target_class,
                target_name,
                target_descriptor,
                constant_value,
                lookup_class,
            )
        };

        match kind {
            1 => {
                let receiver = args
                    .first()
                    .copied()
                    .ok_or(VmError::StackUnderflow)?
                    .as_reference()?;
                self.validate_method_handle_receiver_access(
                    lookup_class.as_deref(),
                    &target_class,
                    &target_name,
                    &target_descriptor,
                    kind,
                    receiver,
                )?;
                self.get_instance_field_from_declaring(receiver, &target_class, &target_name)
                    .map(Some)
            }
            2 => self.get_static_field(&target_class, &target_name).map(Some),
            3 => {
                let receiver = args
                    .first()
                    .copied()
                    .ok_or(VmError::StackUnderflow)?
                    .as_reference()?;
                let value = args.get(1).copied().ok_or(VmError::StackUnderflow)?;
                self.validate_method_handle_receiver_access(
                    lookup_class.as_deref(),
                    &target_class,
                    &target_name,
                    &target_descriptor,
                    kind,
                    receiver,
                )?;
                self.set_object_field_from_declaring(receiver, &target_class, &target_name, value)?;
                Ok(None)
            }
            4 => {
                let value = args.first().copied().ok_or(VmError::StackUnderflow)?;
                self.put_static_field(&target_class, &target_name, value)?;
                Ok(None)
            }
            5 => {
                let receiver = args
                    .first()
                    .copied()
                    .ok_or(VmError::StackUnderflow)?
                    .as_reference()?;
                self.validate_method_handle_receiver_access(
                    lookup_class.as_deref(),
                    &target_class,
                    &target_name,
                    &target_descriptor,
                    kind,
                    receiver,
                )?;
                self.reflect_invoke_method(
                    &target_class,
                    &target_name,
                    &target_descriptor,
                    Some(receiver),
                    args[1..].to_vec(),
                )
            }
            6 => self.reflect_invoke_method(
                &target_class,
                &target_name,
                &target_descriptor,
                None,
                args,
            ),
            7 => {
                let receiver = args
                    .first()
                    .copied()
                    .ok_or(VmError::StackUnderflow)?
                    .as_reference()?;
                self.validate_method_handle_receiver_access(
                    lookup_class.as_deref(),
                    &target_class,
                    &target_name,
                    &target_descriptor,
                    kind,
                    receiver,
                )?;
                self.invoke_exact_instance_method(
                    &target_class,
                    &target_name,
                    &target_descriptor,
                    receiver,
                    args[1..].to_vec(),
                )
            }
            8 => self
                .reflect_new_instance(&target_class, &target_descriptor, args)
                .map(|reference| Some(Value::Reference(reference))),
            9 => {
                let receiver = args
                    .first()
                    .copied()
                    .ok_or(VmError::StackUnderflow)?
                    .as_reference()?;
                self.validate_method_handle_receiver_access(
                    lookup_class.as_deref(),
                    &target_class,
                    &target_name,
                    &target_descriptor,
                    kind,
                    receiver,
                )?;
                self.reflect_invoke_method(
                    &target_class,
                    &target_name,
                    &target_descriptor,
                    Some(receiver),
                    args[1..].to_vec(),
                )
            }
            0 => Ok(Some(constant_value)),
            _ => Err(VmError::UnsupportedNativeMethod {
                class_name: "java/lang/invoke/MethodHandle".to_string(),
                method_name: format!("invoke-kind-{kind}"),
                descriptor: target_descriptor,
            }),
        }
    }

    /// Read the post-transformation descriptor stored on an adapter MH.
    fn method_handle_descriptor(&mut self, handle_ref: Reference) -> Result<String, VmError> {
        let desc_ref = self
            .get_object_field(handle_ref, "__targetDesc")?
            .as_reference()?;
        if desc_ref == Reference::Null {
            return Ok(String::new());
        }
        let s = self.get_object_field(desc_ref, "__descriptor")?.as_reference()?;
        if s == Reference::Null {
            return Ok(String::new());
        }
        self.stringify_reference(s)
    }

    /// Dispatch for derived (combinator) MethodHandle kinds 10..=22.
    fn invoke_derived_method_handle(
        &mut self,
        handle_ref: Reference,
        kind: i32,
        args: Vec<Value>,
    ) -> Result<Option<Value>, VmError> {
        match kind {
            MH_KIND_BIND_TO => {
                let inner = self
                    .get_object_field(handle_ref, "__inner")?
                    .as_reference()?;
                let bind = self.get_object_field(handle_ref, "__bindArg")?;
                let mut new_args = Vec::with_capacity(args.len() + 1);
                new_args.push(bind);
                new_args.extend(args);
                self.invoke_method_handle(inner, new_args)
            }
            MH_KIND_INSERT_ARGUMENTS => {
                let inner = self
                    .get_object_field(handle_ref, "__inner")?
                    .as_reference()?;
                let pos = self
                    .get_object_field(handle_ref, "__insertPos")?
                    .as_int()? as usize;
                let inserts_ref = self
                    .get_object_field(handle_ref, "__insertArgs")?
                    .as_reference()?;
                let inserts = self.read_reference_array_values(inserts_ref)?;
                let mut new_args = Vec::with_capacity(args.len() + inserts.len());
                new_args.extend(args.iter().take(pos).copied());
                new_args.extend(inserts);
                new_args.extend(args.iter().skip(pos).copied());
                self.invoke_method_handle(inner, new_args)
            }
            MH_KIND_DROP_ARGUMENTS => {
                let inner = self
                    .get_object_field(handle_ref, "__inner")?
                    .as_reference()?;
                let pos = self
                    .get_object_field(handle_ref, "__dropPos")?
                    .as_int()? as usize;
                let count = self
                    .get_object_field(handle_ref, "__dropCount")?
                    .as_int()? as usize;
                let mut new_args = Vec::with_capacity(args.len().saturating_sub(count));
                new_args.extend(args.iter().take(pos).copied());
                new_args.extend(args.iter().skip(pos + count).copied());
                self.invoke_method_handle(inner, new_args)
            }
            MH_KIND_PERMUTE_ARGUMENTS => {
                let inner = self
                    .get_object_field(handle_ref, "__inner")?
                    .as_reference()?;
                let perm_ref = self
                    .get_object_field(handle_ref, "__permute")?
                    .as_reference()?;
                let perm = self.read_int_array_values(perm_ref)?;
                let mut new_args = Vec::with_capacity(perm.len());
                for i in perm {
                    let idx = i as usize;
                    let value = args.get(idx).copied().ok_or(VmError::StackUnderflow)?;
                    new_args.push(value);
                }
                self.invoke_method_handle(inner, new_args)
            }
            MH_KIND_AS_TYPE => {
                let inner = self
                    .get_object_field(handle_ref, "__inner")?
                    .as_reference()?;
                let outer_desc = self.method_handle_descriptor(handle_ref)?;
                let inner_desc = self.method_handle_descriptor(inner)?;
                let (outer_args, outer_ret) = Self::split_method_descriptor(&outer_desc)
                    .ok_or(VmError::InvalidDescriptor { descriptor: outer_desc.clone() })?;
                let (inner_args, inner_ret) = Self::split_method_descriptor(&inner_desc)
                    .ok_or(VmError::InvalidDescriptor { descriptor: inner_desc.clone() })?;
                if outer_args.len() != inner_args.len() {
                    return Err(VmError::UnhandledException {
                        class_name: "java/lang/invoke/WrongMethodTypeException".to_string(),
                    });
                }
                let mut coerced = Vec::with_capacity(args.len());
                for (i, arg) in args.into_iter().enumerate() {
                    let from = outer_args.get(i).map(String::as_str).unwrap_or("L;");
                    let to = inner_args.get(i).map(String::as_str).unwrap_or("L;");
                    coerced.push(self.coerce_method_handle_value(arg, from, to)?);
                }
                let inner_result = self.invoke_method_handle(inner, coerced)?;
                if outer_ret == "V" {
                    return Ok(None);
                }
                let value = inner_result.unwrap_or(Value::Reference(Reference::Null));
                Ok(Some(self.coerce_method_handle_value(
                    value, &inner_ret, &outer_ret,
                )?))
            }
            MH_KIND_AS_COLLECTOR => {
                let inner = self
                    .get_object_field(handle_ref, "__inner")?
                    .as_reference()?;
                let pos = self
                    .get_object_field(handle_ref, "__collectPos")?
                    .as_int()? as usize;
                let count = self
                    .get_object_field(handle_ref, "__collectCount")?
                    .as_int()? as usize;
                let comp_ref = self
                    .get_object_field(handle_ref, "__collectComponent")?
                    .as_reference()?;
                let comp = if comp_ref == Reference::Null {
                    "Ljava/lang/Object;".to_string()
                } else {
                    // __collectComponent stores the array class (e.g. `[I`);
                    // strip the leading `[` to get the element descriptor.
                    let name =
                        crate::vm::builtin::helpers::class_internal_name(self, comp_ref)?;
                    name.strip_prefix('[').map(|s| s.to_string()).unwrap_or(name)
                };
                let varargs = self
                    .get_object_field(handle_ref, "__isVarargs")
                    .ok()
                    .and_then(|v| v.as_int().ok())
                    .unwrap_or(0)
                    != 0;
                let mut new_args = Vec::with_capacity(args.len().saturating_sub(count) + 1);
                new_args.extend(args.iter().take(pos).copied());
                // For varargs: if the trailing slot is already an array, pass through.
                let mut collected = if varargs
                    && args.len() == pos + 1
                    && matches!(args.get(pos), Some(Value::Reference(r)) if *r != Reference::Null)
                {
                    args[pos]
                } else {
                    let tail: Vec<Value> = args.iter().skip(pos).copied().collect();
                    Value::Reference(self.allocate_collected_array(&comp, tail)?)
                };
                // Sanity: if we're not in varargs and arg count doesn't match,
                // still try to collect whatever is present.
                if !varargs && args.len() >= pos + count {
                    let tail: Vec<Value> = args.iter().skip(pos).take(count).copied().collect();
                    collected = Value::Reference(self.allocate_collected_array(&comp, tail)?);
                }
                new_args.push(collected);
                // Remaining args after the collected window:
                if !varargs && args.len() > pos + count {
                    new_args.extend(args.iter().skip(pos + count).copied());
                }
                self.invoke_method_handle(inner, new_args)
            }
            MH_KIND_AS_SPREADER => {
                let inner = self
                    .get_object_field(handle_ref, "__inner")?
                    .as_reference()?;
                let pos = self
                    .get_object_field(handle_ref, "__spreadPos")?
                    .as_int()? as usize;
                let count = self
                    .get_object_field(handle_ref, "__spreadCount")?
                    .as_int()? as usize;
                let array_ref = args
                    .get(pos)
                    .and_then(|v| v.as_reference().ok())
                    .ok_or(VmError::StackUnderflow)?;
                let spread = self.read_reference_array_values(array_ref)?;
                if spread.len() != count {
                    return Err(VmError::UnhandledException {
                        class_name: "java/lang/invoke/WrongMethodTypeException".to_string(),
                    });
                }
                let mut new_args = Vec::with_capacity(args.len() + count - 1);
                new_args.extend(args.iter().take(pos).copied());
                new_args.extend(spread);
                new_args.extend(args.iter().skip(pos + 1).copied());
                self.invoke_method_handle(inner, new_args)
            }
            MH_KIND_FILTER_ARGUMENTS => {
                let inner = self
                    .get_object_field(handle_ref, "__inner")?
                    .as_reference()?;
                let pos = self
                    .get_object_field(handle_ref, "__filterPos")?
                    .as_int()? as usize;
                let filters_ref = self
                    .get_object_field(handle_ref, "__filterHandles")?
                    .as_reference()?;
                let filters = self.read_reference_array_values(filters_ref)?;
                let mut new_args = args.clone();
                for (i, filter_value) in filters.iter().enumerate() {
                    let filter_ref = filter_value.as_reference()?;
                    if filter_ref == Reference::Null {
                        continue;
                    }
                    let idx = pos + i;
                    let arg = new_args.get(idx).copied().ok_or(VmError::StackUnderflow)?;
                    let result = self
                        .invoke_method_handle(filter_ref, vec![arg])?
                        .unwrap_or(Value::Reference(Reference::Null));
                    new_args[idx] = result;
                }
                self.invoke_method_handle(inner, new_args)
            }
            MH_KIND_FILTER_RETURN => {
                let inner = self
                    .get_object_field(handle_ref, "__inner")?
                    .as_reference()?;
                let filter = self
                    .get_object_field(handle_ref, "__retFilter")?
                    .as_reference()?;
                let inner_result = self.invoke_method_handle(inner, args)?;
                let v = inner_result.unwrap_or(Value::Reference(Reference::Null));
                self.invoke_method_handle(filter, vec![v])
            }
            MH_KIND_FOLD_ARGUMENTS => {
                let inner = self
                    .get_object_field(handle_ref, "__inner")?
                    .as_reference()?;
                let combiner = self
                    .get_object_field(handle_ref, "__foldCombiner")?
                    .as_reference()?;
                let pos = self
                    .get_object_field(handle_ref, "__foldPos")?
                    .as_int()? as usize;
                let combiner_desc = self.method_handle_descriptor(combiner)?;
                let combiner_arity = parse_arg_count(&combiner_desc)?;
                let combiner_ret = parse_return_type(&combiner_desc)?;
                let combiner_args: Vec<Value> =
                    args.iter().skip(pos).take(combiner_arity).copied().collect();
                let fold_result = self.invoke_method_handle(combiner, combiner_args)?;
                let mut new_args = Vec::with_capacity(args.len() + 1);
                new_args.extend(args.iter().take(pos).copied());
                if combiner_ret.is_some() {
                    new_args.push(fold_result.unwrap_or(Value::Reference(Reference::Null)));
                }
                new_args.extend(args.iter().skip(pos).copied());
                self.invoke_method_handle(inner, new_args)
            }
            MH_KIND_GUARD_WITH_TEST => {
                let test = self
                    .get_object_field(handle_ref, "__guardTest")?
                    .as_reference()?;
                let target = self
                    .get_object_field(handle_ref, "__guardTarget")?
                    .as_reference()?;
                let fallback = self
                    .get_object_field(handle_ref, "__guardFallback")?
                    .as_reference()?;
                let test_desc = self.method_handle_descriptor(test)?;
                let test_arity = parse_arg_count(&test_desc)?;
                let test_args: Vec<Value> = args.iter().take(test_arity).copied().collect();
                let cond = self
                    .invoke_method_handle(test, test_args)?
                    .unwrap_or(Value::Int(0))
                    .as_int()?;
                let chosen = if cond != 0 { target } else { fallback };
                self.invoke_method_handle(chosen, args)
            }
            MH_KIND_CATCH_EXCEPTION => {
                let target = self
                    .get_object_field(handle_ref, "__guardTarget")?
                    .as_reference()?;
                let catch_type_ref = self
                    .get_object_field(handle_ref, "__catchType")?
                    .as_reference()?;
                let handler = self
                    .get_object_field(handle_ref, "__catchHandler")?
                    .as_reference()?;
                let catch_class = if catch_type_ref == Reference::Null {
                    "java/lang/Throwable".to_string()
                } else {
                    crate::vm::builtin::helpers::class_internal_name(self, catch_type_ref)?
                };
                match self.invoke_method_handle(target, args.clone()) {
                    Ok(v) => Ok(v),
                    Err(VmError::UnhandledException { class_name })
                        if self.is_instance_of(&class_name, &catch_class).unwrap_or(false) =>
                    {
                        // Build a synthetic exception object for the handler.
                        let exc_ref = self.heap.lock().unwrap().allocate(HeapValue::Object {
                            class_name: class_name.clone(),
                            fields: Vec::new(),
                        });
                        let mut handler_args = Vec::with_capacity(args.len() + 1);
                        handler_args.push(Value::Reference(exc_ref));
                        handler_args.extend(args);
                        self.invoke_method_handle(handler, handler_args)
                    }
                    Err(e) => Err(e),
                }
            }
            MH_KIND_INVOKER => {
                let flavour = self
                    .get_object_field(handle_ref, "__invokerKind")
                    .ok()
                    .and_then(|v| v.as_int().ok())
                    .unwrap_or(MH_INVOKER_GENERIC);
                let outer_desc = self.method_handle_descriptor(handle_ref)?;
                let (outer_args, _) = Self::split_method_descriptor(&outer_desc)
                    .ok_or(VmError::InvalidDescriptor { descriptor: outer_desc.clone() })?;
                if flavour == MH_INVOKER_CALLSITE {
                    // Read __target from the wrapped call site each time so
                    // MutableCallSite re-targeting takes effect.
                    let callsite = self
                        .get_object_field(handle_ref, "__invokerCallsite")?
                        .as_reference()?;
                    let target = self.resolve_dynamic_target(callsite)?;
                    return self.invoke_method_handle(target, args);
                }
                if flavour == MH_INVOKER_IDENTITY {
                    // identity(T): pass through the single argument.
                    return Ok(args.into_iter().next());
                }
                // For exact/generic/spread invokers the first arg is the MH.
                let mh_ref = args.first().copied().ok_or(VmError::StackUnderflow)?.as_reference()?;
                let rest: Vec<Value> = args.into_iter().skip(1).collect();
                if flavour == MH_INVOKER_SPREAD {
                    // Last MH-type arg is an array to spread.
                    let spread_count = self
                        .get_object_field(handle_ref, "__spreadArrayCount")
                        .ok()
                        .and_then(|v| v.as_int().ok())
                        .unwrap_or(0) as usize;
                    if rest.is_empty() {
                        return self.invoke_method_handle(mh_ref, rest);
                    }
                    let leading_count = rest.len() - 1;
                    let array_ref = rest[leading_count].as_reference()?;
                    let spread = self.read_reference_array_values(array_ref)?;
                    if spread.len() != spread_count {
                        return Err(VmError::UnhandledException {
                            class_name: "java/lang/invoke/WrongMethodTypeException".to_string(),
                        });
                    }
                    let mut new_args = Vec::with_capacity(leading_count + spread.len());
                    new_args.extend(rest.iter().take(leading_count).copied());
                    new_args.extend(spread);
                    return self.invoke_method_handle(mh_ref, new_args);
                }
                // Exact / generic invoker: for generic, coerce args to MH's type.
                if flavour == MH_INVOKER_GENERIC {
                    let inner_desc = self.method_handle_descriptor(mh_ref)?;
                    if let Some((inner_args, _)) = Self::split_method_descriptor(&inner_desc) {
                        if inner_args.len() == rest.len() && outer_args.len() == rest.len() + 1 {
                            let mut coerced = Vec::with_capacity(rest.len());
                            for (i, arg) in rest.into_iter().enumerate() {
                                let from = outer_args.get(i + 1).map(String::as_str).unwrap_or("L;");
                                let to = inner_args.get(i).map(String::as_str).unwrap_or("L;");
                                coerced.push(self.coerce_method_handle_value(arg, from, to)?);
                            }
                            return self.invoke_method_handle(mh_ref, coerced);
                        }
                    }
                }
                self.invoke_method_handle(mh_ref, rest)
            }
            _ => Err(VmError::UnsupportedNativeMethod {
                class_name: "java/lang/invoke/MethodHandle".to_string(),
                method_name: format!("invoke-derived-{kind}"),
                descriptor: String::new(),
            }),
        }
    }

    /// Read `int[]` values from a heap reference array slot.
    fn read_int_array_values(&self, reference: Reference) -> Result<Vec<i32>, VmError> {
        if reference == Reference::Null {
            return Ok(Vec::new());
        }
        match self.heap.lock().unwrap().get(reference)? {
            HeapValue::IntArray { values } => Ok(values.clone()),
            _ => Err(VmError::InvalidHeapValue {
                expected: "int-array",
                actual: "other",
            }),
        }
    }

    /// Read a `Reference[]`-style array slot, regardless of element type — each
    /// slot is returned as a `Value` mirroring how primitives are widened.
    fn read_reference_array_values(&self, reference: Reference) -> Result<Vec<Value>, VmError> {
        if reference == Reference::Null {
            return Ok(Vec::new());
        }
        match self.heap.lock().unwrap().get(reference)? {
            HeapValue::ReferenceArray { values, .. } => Ok(values
                .iter()
                .map(|r| Value::Reference(*r))
                .collect()),
            HeapValue::IntArray { values } => Ok(values.iter().map(|v| Value::Int(*v)).collect()),
            HeapValue::LongArray { values } => Ok(values.iter().map(|v| Value::Long(*v)).collect()),
            HeapValue::FloatArray { values } => Ok(values.iter().map(|v| Value::Float(*v)).collect()),
            HeapValue::DoubleArray { values } => Ok(values.iter().map(|v| Value::Double(*v)).collect()),
            _ => Err(VmError::InvalidHeapValue {
                expected: "array",
                actual: "other",
            }),
        }
    }

    /// Allocate an array of `component` holding the given values, coercing each
    /// value to fit the component type. Used by `asCollector`.
    fn allocate_collected_array(
        &mut self,
        component: &str,
        values: Vec<Value>,
    ) -> Result<Reference, VmError> {
        match component {
            "I" => {
                let mut out = Vec::with_capacity(values.len());
                for v in values {
                    out.push(v.as_int().unwrap_or(0));
                }
                Ok(self.heap.lock().unwrap().allocate_int_array(out))
            }
            "J" => {
                let mut out = Vec::with_capacity(values.len());
                for v in values {
                    out.push(v.as_long().unwrap_or(0));
                }
                Ok(self.heap.lock().unwrap().allocate(HeapValue::LongArray { values: out }))
            }
            "F" => {
                let mut out = Vec::with_capacity(values.len());
                for v in values {
                    out.push(v.as_float().unwrap_or(0.0));
                }
                Ok(self.heap.lock().unwrap().allocate(HeapValue::FloatArray { values: out }))
            }
            "D" => {
                let mut out = Vec::with_capacity(values.len());
                for v in values {
                    out.push(v.as_double().unwrap_or(0.0));
                }
                Ok(self.heap.lock().unwrap().allocate(HeapValue::DoubleArray { values: out }))
            }
            other => {
                let mut out = Vec::with_capacity(values.len());
                for v in values {
                    out.push(v.as_reference().unwrap_or(Reference::Null));
                }
                Ok(self.heap.lock().unwrap().allocate(HeapValue::ReferenceArray {
                    component_type: other.to_string(),
                    values: out,
                }))
            }
        }
    }

    fn invoke_exact_instance_method(
        &mut self,
        declaring_class: &str,
        method_name: &str,
        descriptor: &str,
        receiver: Reference,
        args: Vec<Value>,
    ) -> Result<Option<Value>, VmError> {
        self.ensure_class_loaded(declaring_class)?;
        self.ensure_class_initialized(declaring_class)?;

        let (resolved_class, class_method) =
            self.resolve_method(declaring_class, method_name, descriptor)?;
        let mut all_args = vec![Value::Reference(receiver)];
        all_args.extend(args);
        match class_method {
            ClassMethod::Native => {
                self.invoke_native(&resolved_class, method_name, descriptor, &all_args)
            }
            ClassMethod::Bytecode(method) => {
                let callee = method.with_initial_locals(Vm::args_to_locals(all_args));
                let saved_jit = self.jit.take();
                let result = self.execute(callee);
                self.jit = saved_jit;
                match result? {
                    ExecutionResult::Value(value) => Ok(Some(value)),
                    ExecutionResult::Void => Ok(None),
                }
            }
        }
    }

    pub(crate) fn validate_method_handle_lookup(
        &mut self,
        lookup_class: &str,
        lookup_modes: i32,
        owner_class: &str,
        member_name: &str,
        descriptor: &str,
        reference_kind: u8,
    ) -> Result<(), VmError> {
        let (resolved_class, class_method) = match reference_kind {
            5 | 6 | 9 => self.resolve_method(owner_class, member_name, descriptor)?,
            7 | 8 => {
                self.ensure_class_loaded(owner_class)?;
                let class = self.get_class(owner_class)?;
                let class_method = class
                    .methods
                    .get(&(member_name.to_string(), descriptor.to_string()))
                    .cloned()
                    .ok_or_else(|| VmError::MethodNotFound {
                        class_name: owner_class.to_string(),
                        method_name: member_name.to_string(),
                        descriptor: descriptor.to_string(),
                    })?;
                (owner_class.to_string(), class_method)
            }
            _ => return Ok(()),
        };

        if member_name == "<init>" && reference_kind != 8 {
            return Err(VmError::UnhandledException {
                class_name: "java/lang/NoSuchMethodException".to_string(),
            });
        }

        let access_flags = match &class_method {
            ClassMethod::Bytecode(method) => method.access_flags,
            ClassMethod::Native => 0x0001,
        };
        let is_static = access_flags & 0x0008 != 0;
        match reference_kind {
            5 | 9 if is_static => {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/NoSuchMethodException".to_string(),
                });
            }
            6 if !is_static => {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/NoSuchMethodException".to_string(),
                });
            }
            7 if is_static => {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/NoSuchMethodException".to_string(),
                });
            }
            8 if member_name != "<init>" => {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/NoSuchMethodException".to_string(),
                });
            }
            _ => {}
        }

        if !self.lookup_access_permitted(
            lookup_class,
            lookup_modes,
            &resolved_class,
            access_flags,
        )? {
            return Err(VmError::UnhandledException {
                class_name: "java/lang/IllegalAccessException".to_string(),
            });
        }
        Ok(())
    }

    pub(crate) fn validate_field_method_handle_lookup(
        &mut self,
        lookup_class: &str,
        lookup_modes: i32,
        owner_class: &str,
        field_name: &str,
        descriptor: &str,
        reference_kind: u8,
    ) -> Result<String, VmError> {
        let is_static = matches!(reference_kind, 2 | 4);
        let resolved_class = self.resolve_field(owner_class, field_name, descriptor, is_static)?;
        let access_flags = self
            .field_access_flags(&resolved_class, field_name)
            .unwrap_or(0x0001);
        self.validate_lookup_member_access(
            lookup_class,
            lookup_modes,
            &resolved_class,
            access_flags,
        )?;
        Ok(resolved_class)
    }

    fn resolve_field(
        &mut self,
        owner_class: &str,
        field_name: &str,
        descriptor: &str,
        is_static: bool,
    ) -> Result<String, VmError> {
        let mut current = Some(owner_class.to_string());
        while let Some(class_name) = current {
            self.ensure_class_loaded(&class_name)?;
            let class = self.get_class(&class_name)?;
            let found = if is_static {
                class.static_fields.contains_key(field_name)
                    && self
                        .field_descriptor(&class_name, field_name)
                        .map(|field_descriptor| field_descriptor == descriptor)
                        .unwrap_or_else(|| {
                            class
                                .static_fields
                                .get(field_name)
                                .map(|value| Self::field_value_descriptor(*value) == descriptor)
                                .unwrap_or(false)
                        })
            } else {
                class
                    .instance_fields
                    .iter()
                    .any(|(name, desc)| name == field_name && desc == descriptor)
            };
            if found {
                return Ok(class_name);
            }
            if is_static {
                for interface in &class.interfaces {
                    if let Some(resolved) =
                        self.resolve_interface_field(interface, field_name, descriptor)?
                    {
                        return Ok(resolved);
                    }
                }
            }
            current = class.super_class.clone();
        }
        Err(VmError::FieldNotFound {
            class_name: owner_class.to_string(),
            field_name: field_name.to_string(),
        })
    }

    fn resolve_interface_field(
        &mut self,
        interface_name: &str,
        field_name: &str,
        descriptor: &str,
    ) -> Result<Option<String>, VmError> {
        self.ensure_class_loaded(interface_name)?;
        let interface = self.get_class(interface_name)?;
        let found = interface.static_fields.contains_key(field_name)
            && self
                .field_descriptor(interface_name, field_name)
                .map(|field_descriptor| field_descriptor == descriptor)
                .unwrap_or_else(|| {
                    interface
                        .static_fields
                        .get(field_name)
                        .map(|value| Self::field_value_descriptor(*value) == descriptor)
                        .unwrap_or(false)
                });
        if found {
            return Ok(Some(interface_name.to_string()));
        }
        for parent in &interface.interfaces {
            if let Some(resolved) = self.resolve_interface_field(parent, field_name, descriptor)? {
                return Ok(Some(resolved));
            }
        }
        Ok(None)
    }

    fn field_access_flags(&self, owner_class: &str, field_name: &str) -> Option<u16> {
        self.runtime
            .lock()
            .unwrap()
            .field_access_flags
            .get(&(owner_class.to_string(), field_name.to_string()))
            .copied()
    }

    pub(crate) fn field_descriptor(&self, owner_class: &str, field_name: &str) -> Option<String> {
        self.runtime
            .lock()
            .unwrap()
            .field_descriptors
            .get(&(owner_class.to_string(), field_name.to_string()))
            .cloned()
    }

    pub(crate) fn validate_lookup_member_access(
        &mut self,
        lookup_class: &str,
        lookup_modes: i32,
        declaring_class: &str,
        access_flags: u16,
    ) -> Result<(), VmError> {
        if self.lookup_access_permitted(
            lookup_class,
            lookup_modes,
            declaring_class,
            access_flags,
        )? {
            Ok(())
        } else {
            Err(VmError::UnhandledException {
                class_name: "java/lang/IllegalAccessException".to_string(),
            })
        }
    }

    fn lookup_access_permitted(
        &mut self,
        lookup_class: &str,
        lookup_modes: i32,
        declaring_class: &str,
        access_flags: u16,
    ) -> Result<bool, VmError> {
        if lookup_modes & LOOKUP_PUBLIC == 0 {
            return Ok(false);
        }
        if access_flags & 0x0001 != 0 {
            return Ok(true);
        }
        if access_flags & 0x0002 != 0 {
            return Ok(lookup_modes & LOOKUP_PRIVATE != 0 && lookup_class == declaring_class);
        }
        if access_flags & 0x0004 != 0 {
            return Ok(lookup_modes & LOOKUP_PROTECTED != 0
                && (lookup_class == declaring_class
                    || self.same_runtime_package(lookup_class, declaring_class)
                    || self.is_instance_of(lookup_class, declaring_class)?));
        }
        Ok(lookup_modes & LOOKUP_PACKAGE != 0
            && self.same_runtime_package(lookup_class, declaring_class))
    }

    fn validate_method_handle_receiver_access(
        &mut self,
        lookup_class: Option<&str>,
        declaring_class: &str,
        member_name: &str,
        descriptor: &str,
        reference_kind: i32,
        receiver: Reference,
    ) -> Result<(), VmError> {
        let Some(lookup_class) = lookup_class else {
            return Ok(());
        };
        let access_flags = match reference_kind {
            1 | 3 => self
                .field_access_flags(declaring_class, member_name)
                .unwrap_or(0x0001),
            5 | 7 | 9 => {
                let (_, class_method) =
                    self.resolve_method(declaring_class, member_name, descriptor)?;
                match class_method {
                    ClassMethod::Bytecode(method) => method.access_flags,
                    ClassMethod::Native => 0x0001,
                }
            }
            _ => return Ok(()),
        };
        if access_flags & 0x0004 == 0
            || lookup_class == declaring_class
            || self.same_runtime_package(lookup_class, declaring_class)
            || !self.is_instance_of(lookup_class, declaring_class)?
        {
            return Ok(());
        }

        let receiver_class = self.get_object_class(receiver)?;
        if self.is_instance_of(&receiver_class, lookup_class)? {
            Ok(())
        } else {
            Err(VmError::UnhandledException {
                class_name: "java/lang/IllegalAccessException".to_string(),
            })
        }
    }

    pub(crate) fn same_runtime_package(&self, lhs: &str, rhs: &str) -> bool {
        let lhs_pkg = lhs.rsplit_once('/').map(|(pkg, _)| pkg).unwrap_or("");
        let rhs_pkg = rhs.rsplit_once('/').map(|(pkg, _)| pkg).unwrap_or("");
        lhs_pkg == rhs_pkg
    }

    fn field_value_descriptor(value: Value) -> &'static str {
        match value {
            Value::Int(_) => "I",
            Value::Long(_) => "J",
            Value::Float(_) => "F",
            Value::Double(_) => "D",
            Value::Reference(_) => "Ljava/lang/Object;",
            Value::ReturnAddress(_) => "I",
        }
    }

    pub(crate) fn invoke_jit_get_static_field_ref(
        &mut self,
        field_ref: &FieldRef,
    ) -> Result<Value, VmError> {
        self.ensure_class_loaded(&field_ref.class_name)?;
        self.ensure_class_initialized(&field_ref.class_name)?;
        self.get_static_field(&field_ref.class_name, &field_ref.field_name)
    }

    pub(crate) fn invoke_jit_put_static_field_ref(
        &mut self,
        field_ref: &FieldRef,
        raw_value: u64,
    ) -> Result<(), VmError> {
        let value = Vm::jit_raw_field_value_to_value(&field_ref.descriptor, raw_value).ok_or_else(
            || VmError::InvalidDescriptor {
                descriptor: field_ref.descriptor.clone(),
            },
        )?;
        self.ensure_class_loaded(&field_ref.class_name)?;
        self.ensure_class_initialized(&field_ref.class_name)?;
        self.put_static_field(&field_ref.class_name, &field_ref.field_name, value)
    }

    pub(crate) fn invoke_jit_get_instance_field_ref(
        &mut self,
        field_ref: &FieldRef,
        receiver_raw: u64,
    ) -> Result<Value, VmError> {
        let receiver = Vm::jit_raw_reference(receiver_raw).ok_or(VmError::NullReference)?;
        match self.heap.lock().unwrap().get(receiver)? {
            HeapValue::Object {
                class_name, fields, ..
            } => {
                let offset = self.instance_field_offset(
                    class_name,
                    Some(&field_ref.class_name),
                    &field_ref.field_name,
                )?;
                Ok(fields[offset])
            }
            value => Err(VmError::InvalidHeapValue {
                expected: "object",
                actual: value.kind_name(),
            }),
        }
    }

    pub(crate) fn invoke_jit_put_instance_field_ref(
        &mut self,
        field_ref: &FieldRef,
        receiver_raw: u64,
        raw_value: u64,
    ) -> Result<(), VmError> {
        let receiver = Vm::jit_raw_reference(receiver_raw).ok_or(VmError::NullReference)?;
        let value = Vm::jit_raw_field_value_to_value(&field_ref.descriptor, raw_value).ok_or_else(
            || VmError::InvalidDescriptor {
                descriptor: field_ref.descriptor.clone(),
            },
        )?;
        self.set_object_field_from_declaring(
            receiver,
            &field_ref.class_name,
            &field_ref.field_name,
            value,
        )
    }

    pub(crate) fn invoke_jit_allocate_object(&mut self, class_name: &str) -> Option<Reference> {
        self.ensure_class_loaded(class_name).ok()?;
        self.ensure_class_initialized(class_name).ok()?;

        let all_instance_fields = self.collect_instance_fields(class_name).ok()?;
        let fields: Vec<Value> = all_instance_fields
            .iter()
            .map(|(_, descriptor)| default_value_for_descriptor(descriptor))
            .collect();
        Some(self.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: class_name.to_string(),
            fields,
        }))
    }

    pub(crate) fn invoke_jit_checkcast(&mut self, receiver_raw: u64, target: &str) -> bool {
        let Some(receiver) = Vm::jit_raw_reference(receiver_raw) else {
            return true;
        };
        let Ok(obj_class) = self.get_object_class(receiver) else {
            return false;
        };
        self.is_instance_of(&obj_class, target).unwrap_or(false)
    }

    pub(crate) fn invoke_jit_instanceof(&mut self, receiver_raw: u64, target: &str) -> bool {
        self.invoke_jit_checkcast(receiver_raw, target)
    }

    pub(crate) fn invoke_jit_monitor_enter(&mut self, receiver_raw: u64) -> bool {
        let Some(receiver) = Vm::jit_raw_reference(receiver_raw) else {
            return false;
        };
        self.enter_monitor(receiver).is_ok()
    }

    pub(crate) fn invoke_jit_monitor_exit(&mut self, receiver_raw: u64) -> bool {
        let Some(receiver) = Vm::jit_raw_reference(receiver_raw) else {
            return false;
        };
        self.exit_monitor(receiver).is_ok()
    }

    pub(crate) fn invoke_jit_allocate_primitive_array(
        &mut self,
        atype: u8,
        raw_count: u64,
    ) -> Option<Reference> {
        let count = Vm::jit_raw_count(raw_count)?;
        if count < 0 {
            return None;
        }
        let n = count as usize;
        let reference = match atype {
            4 | 5 | 8 | 9 | 10 => self.heap.lock().unwrap().allocate_int_array(vec![0; n]),
            6 => self.heap.lock().unwrap().allocate(HeapValue::FloatArray {
                values: vec![0.0; n],
            }),
            7 => self.heap.lock().unwrap().allocate(HeapValue::DoubleArray {
                values: vec![0.0; n],
            }),
            11 => self
                .heap
                .lock()
                .unwrap()
                .allocate(HeapValue::LongArray { values: vec![0; n] }),
            _ => return None,
        };
        Some(reference)
    }

    pub(crate) fn invoke_jit_allocate_reference_array(
        &mut self,
        component_type: &str,
        raw_count: u64,
    ) -> Option<Reference> {
        let count = Vm::jit_raw_count(raw_count)?;
        if count < 0 {
            return None;
        }
        let values = vec![Reference::Null; count as usize];
        Some(
            self.heap
                .lock()
                .unwrap()
                .allocate_reference_array(component_type.to_string(), values),
        )
    }

    pub(crate) fn invoke_jit_allocate_multi_array(
        &mut self,
        descriptor: &str,
        raw_counts: &[u64],
    ) -> Option<Reference> {
        let counts = raw_counts
            .iter()
            .map(|&raw| Vm::jit_raw_count(raw))
            .collect::<Option<Vec<_>>>()?;
        self.allocate_multi_array_descriptor(descriptor, &counts)
            .ok()
    }

    fn invoke_jit_instance_method_ref(
        &mut self,
        class_name: &str,
        method_ref: &MethodRef,
        receiver: Reference,
        args_ptr: u64,
        argc: usize,
    ) -> Result<Option<Value>, VmError> {
        if class_name.starts_with("__lambda_proxy_")
            && method_ref.method_name == class_name.trim_start_matches("__lambda_proxy_")
        {
            return Ok(None);
        }

        let args = unsafe { Vm::jit_raw_args_to_values(&method_ref.descriptor, args_ptr, argc) }
            .ok_or_else(|| VmError::InvalidDescriptor {
                descriptor: method_ref.descriptor.clone(),
            })?;
        let mut all_args = vec![Value::Reference(receiver)];
        all_args.extend(args);

        if self.has_native_override(class_name, &method_ref.method_name, &method_ref.descriptor) {
            return self.invoke_native(
                class_name,
                &method_ref.method_name,
                &method_ref.descriptor,
                &all_args,
            );
        }

        let (resolved_class, class_method) =
            self.resolve_method(class_name, &method_ref.method_name, &method_ref.descriptor)?;

        match class_method {
            ClassMethod::Native => self.invoke_native(
                &resolved_class,
                &method_ref.method_name,
                &method_ref.descriptor,
                &all_args,
            ),
            ClassMethod::Bytecode(method) => {
                let callee = method.with_initial_locals(Vm::args_to_locals(all_args));
                let saved_jit = self.jit.take();
                let result = self.execute(callee);
                self.jit = saved_jit;
                match result? {
                    ExecutionResult::Value(value) => Ok(Some(value)),
                    ExecutionResult::Void => Ok(None),
                }
            }
        }
    }

    fn jit_raw_reference(raw: u64) -> Option<Reference> {
        if raw == 0 {
            None
        } else {
            Some(Reference::Heap((raw - 1) as usize))
        }
    }

    fn jit_raw_count(raw: u64) -> Option<i32> {
        Some(raw as i64 as i32)
    }

    fn jit_raw_field_value_to_value(descriptor: &str, raw: u64) -> Option<Value> {
        match descriptor.as_bytes().first()? {
            b'B' | b'C' | b'I' | b'S' | b'Z' => Some(Value::Int(raw as i32)),
            b'J' => Some(Value::Long(raw as i64)),
            b'F' => Some(Value::Float(f32::from_bits(raw as u32))),
            b'D' => Some(Value::Double(f64::from_bits(raw))),
            b'L' | b'[' => Some(Value::Reference(if raw == 0 {
                Reference::Null
            } else {
                Reference::Heap((raw - 1) as usize)
            })),
            _ => None,
        }
    }

    unsafe fn jit_raw_args_to_values(
        descriptor: &str,
        args_ptr: u64,
        argc: usize,
    ) -> Option<Vec<Value>> {
        let arg_types = parse_arg_types(descriptor)?;
        if arg_types.len() != argc {
            return None;
        }

        let mut values = Vec::with_capacity(arg_types.len());
        for (index, arg_type) in arg_types.into_iter().enumerate() {
            let slot = unsafe { (args_ptr as *const u8).add(index * 8) };
            let value = match arg_type {
                b'B' | b'C' | b'I' | b'S' | b'Z' => {
                    Value::Int(unsafe { std::ptr::read_unaligned(slot as *const i64) } as i32)
                }
                b'J' => Value::Long(unsafe { std::ptr::read_unaligned(slot as *const i64) }),
                b'F' => Value::Float(unsafe { std::ptr::read_unaligned(slot as *const f32) }),
                b'D' => Value::Double(unsafe { std::ptr::read_unaligned(slot as *const f64) }),
                b'L' | b'[' => {
                    let raw = unsafe { std::ptr::read_unaligned(slot as *const u64) };
                    if raw == 0 {
                        Value::Reference(Reference::Null)
                    } else {
                        Value::Reference(Reference::Heap((raw - 1) as usize))
                    }
                }
                _ => return None,
            };
            values.push(value);
        }
        Some(values)
    }

    /// Invoke an instance method on the receiver, resolving dynamically from
    /// the receiver's runtime class (like `invokevirtual`), and return its
    /// value. For calling back into Java bytecode from native implementations
    /// (e.g., `Collections.sort` native reading/writing a List through
    /// `get`/`set`).
    pub(super) fn call_virtual(
        &mut self,
        receiver: Reference,
        method_name: &str,
        descriptor: &str,
        extra_args: Vec<Value>,
    ) -> Result<ExecutionResult, VmError> {
        let class_name = self.get_object_class(receiver)?;
        if class_name.starts_with("__lambda_proxy_")
            && method_name == class_name.trim_start_matches("__lambda_proxy_")
        {
            return self.call_lambda_proxy(receiver, extra_args);
        }
        let (resolved_class, class_method) =
            self.resolve_method(&class_name, method_name, descriptor)?;
        let mut all_args = vec![Value::Reference(receiver)];
        all_args.extend(extra_args);
        match class_method {
            ClassMethod::Native => {
                let result =
                    self.invoke_native(&resolved_class, method_name, descriptor, &all_args)?;
                Ok(match result {
                    Some(v) => ExecutionResult::Value(v),
                    None => ExecutionResult::Void,
                })
            }
            ClassMethod::Bytecode(method) => {
                let callee = method.with_initial_locals(Vm::args_to_locals(all_args));
                self.execute(callee)
            }
        }
    }

    /// Invoke a lambda proxy's single abstract method, returning the result
    /// synchronously. Mirrors `dispatch_lambda_proxy` but produces a value
    /// instead of pushing onto a thread frame.
    fn call_lambda_proxy(
        &mut self,
        receiver: Reference,
        extra_args: Vec<Value>,
    ) -> Result<ExecutionResult, VmError> {
        let (target_class, target_method, target_desc, captures) = {
            let class_name = self.get_object_class(receiver)?;
            let class = self.get_class(&class_name)?;
            let fields = match self.heap.lock().unwrap().get(receiver)? {
                HeapValue::Object { fields, .. } => fields.clone(),
                _ => return Err(VmError::NullReference),
            };

            let get_str = |key: &str| -> Result<std::string::String, VmError> {
                let Some(offset) = class.field_offsets.get(key).copied() else {
                    return Ok(std::string::String::new());
                };
                match fields.get(offset) {
                    Some(Value::Reference(r)) => self.stringify_reference(*r),
                    _ => Ok(std::string::String::new()),
                }
            };

            let tc = get_str("__target_class")?;
            let tm = get_str("__target_method")?;
            let td = get_str("__target_desc")?;

            let mut captures = Vec::new();
            let mut i = 0;
            while let Some(offset) = class.field_offsets.get(&format!("__capture_{i}")).copied() {
                let Some(Value::Reference(r)) = fields.get(offset) else {
                    break;
                };
                captures.push(*r);
                i += 1;
            }
            (tc, tm, td, captures)
        };

        let mut all_args: Vec<Value> = captures.into_iter().map(Value::Reference).collect();
        all_args.extend(extra_args);

        self.ensure_class_loaded(&target_class)?;
        let (resolved_class, class_method) =
            self.resolve_method(&target_class, &target_method, &target_desc)?;
        match class_method {
            ClassMethod::Native => {
                let result =
                    self.invoke_native(&resolved_class, &target_method, &target_desc, &all_args)?;
                Ok(match result {
                    Some(v) => ExecutionResult::Value(v),
                    None => ExecutionResult::Void,
                })
            }
            ClassMethod::Bytecode(method) => {
                let callee = method.with_initial_locals(Vm::args_to_locals(all_args));
                self.execute(callee)
            }
        }
    }

    /// Whether a `(class, method, descriptor)` has a Rust-native shadow that
    /// should win over any bytecode version loaded from the JDK. Used to
    /// short-circuit JDK implementations that transitively pull in machinery
    /// we don't support (reference handler threads, security, reflection).
    pub(super) fn has_native_override(
        &self,
        class_name: &str,
        method_name: &str,
        descriptor: &str,
    ) -> bool {
        // Every method on Unsafe is native-stubbed — the real Unsafe depends
        // on intrinsics we don't provide, and listing every method JDK code
        // might call up-front would be miles of boilerplate.
        if class_name == "jdk/internal/misc/Unsafe" {
            return true;
        }
        matches!(
            (class_name, method_name, descriptor),
            ("java/util/Collections", "sort", "(Ljava/util/List;)V")
                | (
                    "java/util/Collections",
                    "sort",
                    "(Ljava/util/List;Ljava/util/Comparator;)V",
                )
                | ("java/util/Collections", "reverse", "(Ljava/util/List;)V")
                | (
                    "java/util/Arrays",
                    "stream",
                    "([I)Ljava/util/stream/IntStream;"
                )
                | (
                    "java/util/Arrays",
                    "stream",
                    "([J)Ljava/util/stream/LongStream;"
                )
                | (
                    "java/util/Arrays",
                    "stream",
                    "([D)Ljava/util/stream/DoubleStream;"
                )
                | ("java/util/Arrays", "equals", "([I[I)Z")
                | ("java/util/Arrays", "equals", "([J[J)Z")
                | ("java/util/Arrays", "equals", "([B[B)Z")
                | ("java/util/Arrays", "equals", "([S[S)Z")
                | ("java/util/Arrays", "equals", "([C[C)Z")
                | ("java/util/Arrays", "equals", "([F[F)Z")
                | ("java/util/Arrays", "equals", "([D[D)Z")
                | ("java/util/Arrays", "equals", "([Z[Z)Z")
                | (
                    "java/util/Arrays",
                    "equals",
                    "([Ljava/lang/Object;[Ljava/lang/Object;)Z",
                )
                | (
                    "java/util/stream/Collectors",
                    "toList",
                    "()Ljava/util/stream/Collector;"
                )
                | (
                    "java/util/stream/Collectors",
                    "toSet",
                    "()Ljava/util/stream/Collector;"
                )
                | (
                    "java/util/stream/Collectors",
                    "counting",
                    "()Ljava/util/function/Supplier;"
                )
                | (
                    "java/util/stream/Collectors",
                    "joining",
                    "()Ljava/util/stream/Collector;"
                )
                | (
                    "java/util/stream/Collectors",
                    "joining",
                    "(Ljava/lang/CharSequence;)Ljava/util/stream/Collector;"
                )
                | (
                    "java/util/stream/Collectors",
                    "reducing",
                    "(Ljava/lang/Object;Ljava/util/function/BinaryOperator;)Ljava/util/stream/Collector;"
                )
                | (
                    "java/util/stream/Collectors",
                    "toMap",
                    "(Ljava/util/function/Function;Ljava/util/function/Function;)Ljava/util/stream/Collector;"
                )
                | (
                    "__jvm_rs/NativeIntStream",
                    "collect",
                    "(Ljava/util/stream/Collector;)Ljava/lang/Object;"
                )
                | (
                    "__jvm_rs/NativeLongStream",
                    "collect",
                    "(Ljava/util/stream/Collector;)Ljava/lang/Object;"
                )
                | (
                    "__jvm_rs/NativeDoubleStream",
                    "collect",
                    "(Ljava/util/stream/Collector;)Ljava/lang/Object;"
                )
        )
    }

    /// Return the `java/lang/Class` heap object for the given internal class
    /// name, allocating (and caching) it on first reference. `ldc` of a
    /// `CONSTANT_Class` entry resolves through here so that class literals
    /// round-trip as real heap references instead of null — which is what
    /// static initializers like `Reflection.<clinit>` rely on when they
    /// build `Map.of(SomeClass.class, ...)`.
    pub fn class_object(&mut self, internal_name: &str) -> Reference {
        if let Some(existing) = self
            .runtime
            .lock()
            .unwrap()
            .class_objects
            .get(internal_name)
            .copied()
        {
            return existing;
        }
        let name_ref = self.new_string(internal_name.to_string());
        let fields = if let Value::Reference(r) = name_ref {
            vec![Value::Reference(r)]
        } else {
            vec![]
        };
        let reference = self.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: "java/lang/Class".to_string(),
            fields,
        });
        self.runtime
            .lock()
            .unwrap()
            .class_objects
            .insert(internal_name.to_string(), reference);
        reference
    }

    /// Register a class from a parsed `ClassFile`, extracting all runtime
    /// metadata (constant pool entries, method/field refs, exception handlers,
    /// line numbers, stack map frames, invoke dynamic sites).
    pub fn register_classfile(&mut self, class_name: &str, class_file: &ClassFile) {
        crate::launcher::register_class(class_name, class_file, self)
            .expect("register_class should not fail for valid ClassFile data");
    }

    /// Ensure a class is loaded, loading it from the classpath on demand.
    /// Uses a parent-first delegation model: bootstrap classloader (loads java.*,
    /// jdk.*, sun.*) is consulted first; if not found and a user classpath is set,
    /// the user classpath is searched.
    fn ensure_class_loaded(&mut self, class_name: &str) -> Result<(), VmError> {
        if self
            .runtime
            .lock()
            .unwrap()
            .classes
            .contains_key(class_name)
        {
            return Ok(());
        }

        // CDS fast path: restore pre-parsed class from cache instead of re-parsing.
        if let Some(cached) = self.class_data_cache.lock().unwrap().get(class_name).cloned() {
            self.runtime.lock().unwrap().classes.insert(class_name.to_string(), (*cached).clone());
            self.log_class_load(class_name, "cds-cache");
            return Ok(());
        }

        // Array classes (e.g., [I, [Ljava/lang/Object;) are synthesized at runtime
        if class_name.starts_with('[') {
            return self.register_synthetic_array_class(class_name);
        }

        if let Some(ref mut loader) = self.class_loader {
            if let Ok(Some(class_file)) = ClassLoader::load_classfile(loader, class_name) {
                self.register_classfile(class_name, &class_file);
                self.log_class_load(class_name, "bootstrap");
                return Ok(());
            }
        }

        if !self.class_path.is_empty() {
            let class_path = self.class_path.clone();
            let source =
                crate::launcher::resolve_class_path(&class_path, class_name).ok_or_else(|| {
                    VmError::ClassNotFound {
                        class_name: class_name.to_string(),
                    }
                })?;
            crate::launcher::load_and_register_class_from(&source, class_name, self).map_err(|_| {
                VmError::ClassNotFound {
                    class_name: class_name.to_string(),
                }
            })?;
            self.log_class_load(class_name, "classpath");
            // Populate CDS cache so subsequent loads skip re-parsing.
            if let Some(class) = self.runtime.lock().unwrap().classes.get(class_name).cloned() {
                self.class_data_cache.lock().unwrap().insert(class_name.to_string(), Arc::new(class));
            }
            Ok(())
        } else {
            Err(VmError::ClassNotFound {
                class_name: class_name.to_string(),
            })
        }
    }

    pub(super) fn ensure_class(&mut self, class_name: &str) -> Result<(), VmError> {
        self.ensure_class_loaded(class_name)?;
        self.ensure_class_initialized(class_name)
    }

    /// Register a synthesized array class (e.g., [I, [Ljava/lang/String;)
    fn register_synthetic_array_class(&mut self, class_name: &str) -> Result<(), VmError> {
        // Determine element type and array dimensions
        let (element_type, dimensions) = Self::parse_array_descriptor(class_name);

        // For 1-dim primitive arrays like [I, [B, etc., create a simple runtime class
        // For object arrays like [Ljava/lang/String;, we need to know the element class
        let super_class = if dimensions == 1
            && !element_type.starts_with('[')
            && !element_type.starts_with('L')
        {
            // Primitive array's super is Object
            "java/lang/Object".to_string()
        } else if dimensions > 1 {
            // Multi-dim array: super is array of (dimensions-1)
            format!("[{}", &element_type[..element_type.len().saturating_sub(2)])
        } else {
            // Object array: super is Object
            "java/lang/Object".to_string()
        };

        let runtime_class = RuntimeClass {
            name: class_name.to_string(),
            super_class: Some(super_class),
            methods: std::collections::HashMap::new(),
            static_fields: std::collections::HashMap::new(),
            instance_fields: vec![],
            field_offsets: std::collections::HashMap::new(),
            interfaces: vec![],
        };

        self.register_class(runtime_class);
        Ok(())
    }

    /// Parse array descriptor to get element type and dimensions
    fn parse_array_descriptor(class_name: &str) -> (String, usize) {
        let mut dims = 0;
        let mut i = 0;
        while i < class_name.len() && class_name.chars().nth(i) == Some('[') {
            dims += 1;
            i += 1;
        }
        let element_type = class_name[i..].to_string();
        (element_type, dims)
    }

    /// Run `<clinit>` for a class if it hasn't been initialized yet.
    fn ensure_class_initialized(&mut self, class_name: &str) -> Result<(), VmError> {
        loop {
            enum InitializationAction {
                Wait,
                Run(Option<Method>),
                Done,
            }

            let action = {
                let mut runtime = self.runtime.lock().unwrap();
                match runtime.initialized_classes.get(class_name) {
                    Some(ClassInitializationState::Initialized) => InitializationAction::Done,
                    Some(ClassInitializationState::Initializing(owner))
                        if *owner == self.thread_id =>
                    {
                        InitializationAction::Done
                    }
                    Some(ClassInitializationState::Initializing(_)) => InitializationAction::Wait,
                    None => {
                        runtime.initialized_classes.insert(
                            class_name.to_string(),
                            ClassInitializationState::Initializing(self.thread_id),
                        );
                        let clinit = runtime.classes.get(class_name).and_then(|class| {
                            class
                                .methods
                                .get(&("<clinit>".to_string(), "()V".to_string()))
                                .cloned()
                        });
                        match clinit {
                            Some(ClassMethod::Bytecode(method)) => {
                                InitializationAction::Run(Some(method))
                            }
                            _ => InitializationAction::Run(None),
                        }
                    }
                }
            };

            match action {
                InitializationAction::Done => return Ok(()),
                InitializationAction::Wait => std::thread::yield_now(),
                InitializationAction::Run(clinit) => {
                    let result = if let Some(method) = clinit {
                        self.execute(method).map(|_| ())
                    } else {
                        Ok(())
                    };

                    let mut runtime = self.runtime.lock().unwrap();
                    match result {
                        Ok(()) => {
                            runtime.initialized_classes.insert(
                                class_name.to_string(),
                                ClassInitializationState::Initialized,
                            );
                            return Ok(());
                        }
                        Err(error) => {
                            runtime.initialized_classes.remove(class_name);
                            return Err(error);
                        }
                    }
                }
            }
        }
    }

    fn get_class(&self, class_name: &str) -> Result<RuntimeClass, VmError> {
        self.runtime
            .lock()
            .unwrap()
            .classes
            .get(class_name)
            .cloned()
            .ok_or_else(|| VmError::ClassNotFound {
                class_name: class_name.to_string(),
            })
    }

    pub(super) fn get_static_field(
        &self,
        class_name: &str,
        field_name: &str,
    ) -> Result<Value, VmError> {
        let runtime = self.runtime.lock().unwrap();
        let class = runtime
            .classes
            .get(class_name)
            .ok_or_else(|| VmError::ClassNotFound {
                class_name: class_name.to_string(),
            })?;
        class
            .static_fields
            .get(field_name)
            .copied()
            .ok_or_else(|| VmError::FieldNotFound {
                class_name: class_name.to_string(),
                field_name: field_name.to_string(),
            })
    }

    fn put_static_field(
        &mut self,
        class_name: &str,
        field_name: &str,
        value: Value,
    ) -> Result<(), VmError> {
        let mut runtime = self.runtime.lock().unwrap();
        let class = runtime
            .classes
            .get_mut(class_name)
            .ok_or_else(|| VmError::ClassNotFound {
                class_name: class_name.to_string(),
            })?;
        class.static_fields.insert(field_name.to_string(), value);
        Ok(())
    }

    pub(super) fn get_instance_field(
        &self,
        reference: Reference,
        field_name: &str,
    ) -> Result<Value, VmError> {
        self.get_instance_field_from_declaring(reference, "", field_name)
    }

    fn get_instance_field_from_declaring(
        &self,
        reference: Reference,
        declaring_class: &str,
        field_name: &str,
    ) -> Result<Value, VmError> {
        let heap = self.heap.lock().unwrap();
        match heap.get(reference)? {
            HeapValue::Object {
                class_name, fields, ..
            } => {
                let declaring_class = (!declaring_class.is_empty()).then_some(declaring_class);
                let offset = self.instance_field_offset(class_name, declaring_class, field_name)?;
                Ok(fields
                    .get(offset)
                    .copied()
                    .ok_or_else(|| VmError::FieldNotFound {
                        class_name: class_name.clone(),
                        field_name: field_name.to_string(),
                    })?)
            }
            value => Err(VmError::InvalidHeapValue {
                expected: "object",
                actual: value.kind_name(),
            }),
        }
    }

    fn get_object_field(&self, reference: Reference, field_name: &str) -> Result<Value, VmError> {
        self.get_instance_field(reference, field_name)
    }

    fn set_object_field(
        &mut self,
        reference: Reference,
        field_name: &str,
        value: Value,
    ) -> Result<(), VmError> {
        self.set_object_field_from_declaring(reference, "", field_name, value)
    }

    fn set_object_field_from_declaring(
        &mut self,
        reference: Reference,
        declaring_class: &str,
        field_name: &str,
        value: Value,
    ) -> Result<(), VmError> {
        let object_slot = match reference {
            Reference::Heap(i) => i,
            Reference::Null => return Err(VmError::NullReference),
        };

        let target_slot_for_barrier = if let Value::Reference(target_ref) = &value {
            if let Reference::Heap(slot) = target_ref {
                Some(*slot)
            } else {
                None
            }
        } else {
            None
        };

        let mut heap = self.heap.lock().unwrap();
        if let Some(target_slot) = target_slot_for_barrier {
            heap.write_barrier(object_slot, target_slot);
        }

        match heap.get_mut(reference)? {
            HeapValue::Object {
                class_name, fields, ..
            } => {
                let declaring_class = (!declaring_class.is_empty()).then_some(declaring_class);
                let offset = self.instance_field_offset(class_name, declaring_class, field_name)?;
                if offset >= fields.len() {
                    return Err(VmError::FieldNotFound {
                        class_name: class_name.clone(),
                        field_name: field_name.to_string(),
                    });
                }
                fields[offset] = value;
                Ok(())
            }
            value => Err(VmError::InvalidHeapValue {
                expected: "object",
                actual: value.kind_name(),
            }),
        }
    }

    fn instance_field_offset(
        &self,
        object_class: &str,
        declaring_class: Option<&str>,
        field_name: &str,
    ) -> Result<usize, VmError> {
        let mut offset = 0usize;
        let mut current = Some(object_class.to_string());
        while let Some(class_name) = current {
            let class = self.get_class(&class_name)?;
            for (name, _) in &class.instance_fields {
                if name == field_name && declaring_class.map_or(true, |decl| decl == class_name) {
                    return Ok(offset);
                }
                offset += 1;
            }
            current = class.super_class.clone();
        }
        Err(VmError::FieldNotFound {
            class_name: declaring_class.unwrap_or(object_class).to_string(),
            field_name: field_name.to_string(),
        })
    }

    fn collect_instance_fields(
        &mut self,
        class_name: &str,
    ) -> Result<Vec<(String, String)>, VmError> {
        let mut all_instance_fields = Vec::new();
        let mut current = Some(class_name.to_string());
        while let Some(current_class) = current {
            self.ensure_class_loaded(&current_class)?;
            let class = self.get_class(&current_class)?;
            for (name, desc) in &class.instance_fields {
                all_instance_fields.push((name.clone(), desc.clone()));
            }
            current = class.super_class.clone();
        }
        Ok(all_instance_fields)
    }

    fn start_java_thread(
        &mut self,
        thread_ref: Reference,
        start_class: &str,
        method_name: &str,
        descriptor: &str,
        args: Vec<Value>,
    ) -> Result<(), VmError> {
        let index = match thread_ref {
            Reference::Null => return Err(VmError::NullReference),
            Reference::Heap(index) => index,
        };

        {
            let threads = self.threads.states.lock().unwrap();
            if threads.get(&index).is_some_and(|state| state.started) {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/IllegalThreadStateException".to_string(),
                });
            }
        }

        // Create a parking permit for the new thread and register it.
        let parking = std::sync::Arc::new((
            std::sync::Mutex::new(false),
            std::sync::Condvar::new(),
        ));
        self.threads
            .parking
            .lock()
            .unwrap()
            .insert(index, std::sync::Arc::clone(&parking));

        let handle =
            self.spawn_invocation(start_class, method_name, descriptor, Some(index), args)?;
        self.threads.states.lock().unwrap().insert(
            index,
            JavaThreadState {
                started: true,
                interrupted: false,
                handle: Some(handle),
                status: thread::ThreadStatus::Runnable,
            },
        );
        Ok(())
    }

    fn join_java_thread(&mut self, thread_ref: Reference) -> Result<(), VmError> {
        let index = match thread_ref {
            Reference::Null => return Err(VmError::NullReference),
            Reference::Heap(index) => index,
        };
        let maybe_handle = self
            .threads
            .states
            .lock()
            .unwrap()
            .get_mut(&index)
            .and_then(|state| state.handle.take());
        if let Some(handle) = maybe_handle {
            let _ = handle.join()?;
        }
        Ok(())
    }

    fn stringify_value(&self, value: Value) -> Result<String, VmError> {
        match value {
            Value::Int(v) => Ok(v.to_string()),
            Value::Long(v) => Ok(v.to_string()),
            Value::Float(v) => Ok(format_vm_float(v as f64)),
            Value::Double(v) => Ok(format_vm_float(v)),
            Value::Reference(Reference::Null) => Ok("null".to_string()),
            Value::Reference(reference) => self.stringify_heap(reference),
            Value::ReturnAddress(pc) => Ok(format!("ret@{pc}")),
        }
    }

    /// Format a heap value for user-visible output (Object.toString equivalent
    /// for built-in wrapper classes). Falls back to `class@ref` for unknown
    /// object kinds so tracing still produces useful strings.
    pub(super) fn stringify_heap(&self, reference: Reference) -> Result<String, VmError> {
        match reference {
            Reference::Null => Ok("null".to_string()),
            _ => {
                let heap = self.heap.lock().unwrap();
                let value = heap.get(reference)?;
                Ok(match value {
                    HeapValue::String(s) => s.clone(),
                    HeapValue::StringBuilder(s) => s.clone(),
                    HeapValue::Object { class_name, fields } => match class_name.as_str() {
                        "java/lang/Integer" => match &fields[0] {
                            Value::Int(i) => i.to_string(),
                            _ => "0".to_string(),
                        },
                        "java/lang/Long" => match &fields[0] {
                            Value::Long(i) => i.to_string(),
                            _ => "0".to_string(),
                        },
                        "java/lang/Boolean" => match &fields[0] {
                            Value::Int(i) if *i != 0 => "true".to_string(),
                            _ => "false".to_string(),
                        },
                        other => format!("{other}@{reference:?}"),
                    },
                    other => format!("{}@{reference:?}", other.kind_name()),
                })
            }
        }
    }

    /// Format a value per the single descriptor character used by
    /// `StringConcatFactory.makeConcatWithConstants`. Promotes booleans to
    /// `"true"/"false"` and chars to their `char` code point instead of the
    /// raw int fallback.
    fn stringify_concat_arg(&self, ty: u8, value: Value) -> Result<String, VmError> {
        match ty {
            b'Z' => Ok(if value.as_int()? != 0 {
                "true"
            } else {
                "false"
            }
            .to_string()),
            b'C' => {
                let ch = char::from_u32(value.as_int()? as u32).unwrap_or('\0');
                Ok(ch.to_string())
            }
            _ => self.stringify_value(value),
        }
    }

    fn build_string_concat(
        &self,
        recipe: Option<&str>,
        constants: &[String],
        args: &[Value],
        descriptor: &str,
    ) -> Result<String, VmError> {
        let arg_types = parse_arg_types(descriptor).unwrap_or_default();
        let type_for = |index: usize| arg_types.get(index).copied().unwrap_or(b'L');

        if let Some(recipe) = recipe {
            let mut result = String::new();
            let mut arg_index = 0usize;
            let mut constant_index = 0usize;
            for ch in recipe.chars() {
                match ch {
                    '\u{0001}' => {
                        let value = args.get(arg_index).copied().ok_or_else(|| {
                            VmError::InvalidDescriptor {
                                descriptor: format!(
                                    "missing invokedynamic concat arg at {arg_index}"
                                ),
                            }
                        })?;
                        result.push_str(&self.stringify_concat_arg(type_for(arg_index), value)?);
                        arg_index += 1;
                    }
                    '\u{0002}' => {
                        let value = constants.get(constant_index).ok_or_else(|| {
                            VmError::InvalidDescriptor {
                                descriptor: format!(
                                    "missing invokedynamic concat constant at {constant_index}"
                                ),
                            }
                        })?;
                        result.push_str(value);
                        constant_index += 1;
                    }
                    other => result.push(other),
                }
            }
            return Ok(result);
        }

        let mut result = String::new();
        for (i, value) in args.iter().enumerate() {
            result.push_str(&self.stringify_concat_arg(type_for(i), *value)?);
        }
        Ok(result)
    }

    fn enter_monitor(&self, reference: Reference) -> Result<(), VmError> {
        let index = match reference {
            Reference::Null => return Err(VmError::NullReference),
            Reference::Heap(index) => index,
        };
        let tid = self.thread_id;
        let mut states = self.monitors.states.lock().unwrap();
        loop {
            let monitor = states.entry(index).or_default();
            if monitor.lock_count == 0 || monitor.owner_thread == tid {
                monitor.owner_thread = tid;
                monitor.lock_count += 1;
                return Ok(());
            }
            states = self.monitors.changed.wait(states).unwrap();
        }
    }

    fn exit_monitor(&self, reference: Reference) -> Result<(), VmError> {
        let index = match reference {
            Reference::Null => return Err(VmError::NullReference),
            Reference::Heap(index) => index,
        };
        let tid = self.thread_id;
        let mut states = self.monitors.states.lock().unwrap();
        let monitor = states.entry(index).or_default();
        if monitor.lock_count == 0 || monitor.owner_thread != tid {
            return Err(VmError::UnhandledException {
                class_name: "java/lang/IllegalMonitorStateException".to_string(),
            });
        }
        monitor.lock_count -= 1;
        if monitor.lock_count == 0 {
            monitor.owner_thread = 0;
            if monitor.waiting_threads == 0 && monitor.pending_notifies == 0 {
                states.remove(&index);
            }
            self.monitors.changed.notify_all();
        }
        Ok(())
    }

    fn wait_on_monitor(&self, reference: Reference) -> Result<(), VmError> {
        let index = match reference {
            Reference::Null => return Err(VmError::NullReference),
            Reference::Heap(index) => index,
        };
        let tid = self.thread_id;
        let mut states = self.monitors.states.lock().unwrap();
        let saved_lock_count = {
            let monitor = states.entry(index).or_default();
            if monitor.lock_count == 0 || monitor.owner_thread != tid {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/IllegalMonitorStateException".to_string(),
                });
            }
            let saved_lock_count = monitor.lock_count;
            monitor.lock_count = 0;
            monitor.owner_thread = 0;
            monitor.waiting_threads += 1;
            saved_lock_count
        };
        self.monitors.changed.notify_all();

        loop {
            states = self.monitors.changed.wait(states).unwrap();
            let monitor = states.entry(index).or_default();
            if monitor.pending_notifies > 0
                && (monitor.lock_count == 0 || monitor.owner_thread == tid)
            {
                monitor.pending_notifies -= 1;
                monitor.waiting_threads -= 1;
                monitor.owner_thread = tid;
                monitor.lock_count = saved_lock_count;
                return Ok(());
            }
        }
    }

    fn notify_monitor(&self, reference: Reference, notify_all: bool) -> Result<(), VmError> {
        let index = match reference {
            Reference::Null => return Err(VmError::NullReference),
            Reference::Heap(index) => index,
        };
        let tid = self.thread_id;
        let mut states = self.monitors.states.lock().unwrap();
        let monitor = states.entry(index).or_default();
        if monitor.lock_count == 0 || monitor.owner_thread != tid {
            return Err(VmError::UnhandledException {
                class_name: "java/lang/IllegalMonitorStateException".to_string(),
            });
        }
        let newly_available = if notify_all {
            monitor
                .waiting_threads
                .saturating_sub(monitor.pending_notifies)
        } else if monitor.waiting_threads > monitor.pending_notifies {
            1
        } else {
            0
        };
        monitor.pending_notifies += newly_available;
        if newly_available > 0 {
            self.monitors.changed.notify_all();
        }
        Ok(())
    }

    pub(crate) fn lock_support_park(&self, timeout: Option<std::time::Duration>) {
        let Some(idx) = self.java_thread_ref else {
            return;
        };
        let arc = {
            self.threads
                .parking
                .lock()
                .unwrap()
                .get(&idx)
                .map(std::sync::Arc::clone)
        };
        let Some(arc) = arc else { return };
        let (permit_mutex, cvar) = &*arc;
        let mut permit = permit_mutex.lock().unwrap();
        if *permit {
            *permit = false;
            return;
        }
        match timeout {
            Some(dur) => {
                let (mut g, _) =
                    cvar.wait_timeout_while(permit, dur, |p| !*p).unwrap();
                *g = false;
            }
            None => {
                let mut g = cvar.wait_while(permit, |p| !*p).unwrap();
                *g = false;
            }
        }
    }

    pub(crate) fn lock_support_unpark(&self, thread_ref: Reference) {
        let Reference::Heap(index) = thread_ref else {
            return;
        };
        let arc = {
            self.threads
                .parking
                .lock()
                .unwrap()
                .get(&index)
                .map(std::sync::Arc::clone)
        };
        let Some(arc) = arc else { return };
        let (permit_mutex, cvar) = &*arc;
        let mut permit = permit_mutex.lock().unwrap();
        *permit = true;
        cvar.notify_one();
    }

    pub fn new_string(&mut self, value: impl Into<String>) -> Value {
        Value::Reference(self.heap.lock().unwrap().allocate_string(value))
    }

    pub fn intern_string(&mut self, value: impl Into<String>) -> Value {
        let value = value.into();
        if let Some(existing) = self.string_pool.lock().unwrap().get(&value).copied() {
            return Value::Reference(existing);
        }

        let reference = self.heap.lock().unwrap().allocate_string(value.clone());
        self.string_pool.lock().unwrap().insert(value, reference);
        Value::Reference(reference)
    }

    pub fn new_string_array(&mut self, values: &[String]) -> Value {
        let references = values
            .iter()
            .map(|value| match self.new_string(value.clone()) {
                Value::Reference(reference) => reference,
                _ => unreachable!(),
            })
            .collect();
        Value::Reference(
            self.heap
                .lock()
                .unwrap()
                .allocate_reference_array("java/lang/String", references),
        )
    }

    pub fn take_output(&mut self) -> Vec<String> {
        std::mem::take(&mut self.output.lock().unwrap())
    }

    /// Get the class name of a heap value.
    fn get_object_class(&self, reference: Reference) -> Result<String, VmError> {
        match self.heap.lock().unwrap().get(reference)? {
            HeapValue::Object { class_name, .. } => Ok(class_name.clone()),
            HeapValue::String(_) => Ok("java/lang/String".to_string()),
            HeapValue::StringBuilder(_) => Ok("java/lang/StringBuilder".to_string()),
            HeapValue::IntArray { .. } => Ok("[I".to_string()),
            HeapValue::LongArray { .. } => Ok("[J".to_string()),
            HeapValue::FloatArray { .. } => Ok("[F".to_string()),
            HeapValue::DoubleArray { .. } => Ok("[D".to_string()),
            HeapValue::ReferenceArray { component_type, .. } => Ok(format!("[L{component_type};")),
        }
    }

    pub(super) fn reflect_invoke_method(
        &mut self,
        declaring_class: &str,
        method_name: &str,
        descriptor: &str,
        receiver: Option<Reference>,
        args: Vec<Value>,
    ) -> Result<Option<Value>, VmError> {
        self.ensure_class_loaded(declaring_class)?;
        self.ensure_class_initialized(declaring_class)?;

        if let Some(receiver) = receiver {
            let receiver_class = self.get_object_class(receiver)?;
            let (resolved_class, class_method) =
                self.resolve_method(&receiver_class, method_name, descriptor)?;
            let mut all_args = vec![Value::Reference(receiver)];
            all_args.extend(args);
            match class_method {
                ClassMethod::Native => {
                    self.invoke_native(&resolved_class, method_name, descriptor, &all_args)
                }
                ClassMethod::Bytecode(method) => {
                    let callee = method.with_initial_locals(Vm::args_to_locals(all_args));
                    let saved_jit = self.jit.take();
                    let result = self.execute(callee);
                    self.jit = saved_jit;
                    match result? {
                        ExecutionResult::Value(value) => Ok(Some(value)),
                        ExecutionResult::Void => Ok(None),
                    }
                }
            }
        } else {
            let (resolved_class, class_method) =
                self.resolve_method(declaring_class, method_name, descriptor)?;
            match class_method {
                ClassMethod::Native => {
                    self.invoke_native(&resolved_class, method_name, descriptor, &args)
                }
                ClassMethod::Bytecode(method) => {
                    let callee = method.with_initial_locals(Vm::args_to_locals(args));
                    let saved_jit = self.jit.take();
                    let result = self.execute(callee);
                    self.jit = saved_jit;
                    match result? {
                        ExecutionResult::Value(value) => Ok(Some(value)),
                        ExecutionResult::Void => Ok(None),
                    }
                }
            }
        }
    }

    pub(super) fn reflect_new_instance(
        &mut self,
        class_name: &str,
        constructor_descriptor: &str,
        args: Vec<Value>,
    ) -> Result<Reference, VmError> {
        let object =
            self.invoke_jit_allocate_object(class_name)
                .ok_or_else(|| VmError::ClassNotFound {
                    class_name: class_name.to_string(),
                })?;
        let mut ctor_args = vec![Value::Reference(object)];
        ctor_args.extend(args);
        self.reflect_invoke_method(
            class_name,
            "<init>",
            constructor_descriptor,
            None,
            ctor_args,
        )?;
        Ok(object)
    }

    /// Verify a method's bytecode structure before execution.
    pub fn verify_method(method: &Method) -> Result<(), VmError> {
        verify::verify_method(method)
    }

    pub fn execute(&mut self, method: Method) -> Result<ExecutionResult, VmError> {
        let class_name = method.class_name.clone();
        let method_name = method.name.clone();
        let descriptor = method.descriptor.clone();
        let method_key = format!("{}.{}{}", class_name, method_name, descriptor);
        let method_clone = method.clone();

        let mut thread = Thread::new(method);
        thread.current_frame_mut().increment_invocation_count();
        // Record per-method invocation in the JIT profiling subsystem.
        if let Some(ref jit) = self.jit {
            jit.record_invocation(&class_name, &method_name, &descriptor);
        }

        let vm_ptr = self as *mut Vm as u64;
        set_current_vm(vm_ptr);

        let result = (|| -> Result<ExecutionResult, VmError> {
            if self.jit.is_some() && self.jit_context.is_some() {
                let jit = self.jit.as_ref().unwrap();
                let jit_context = self.jit_context.as_mut().unwrap();
                let frame = thread.current_frame();
                if jit.should_compile(&frame, None) {
                    if let Some(code) = jit.get_or_compile(&method_clone) {
                        let installed = jit_context.add_method(method_key.clone(), code.clone());
                        if installed {
                            let jit_args = Vm::collect_jit_args_static(&method_clone, frame);
                            let ret =
                                crate::vm::jit::runtime::JitReturn::from_descriptor(&descriptor);
                            if let Some(result) =
                                jit_context.execute_typed(vm_ptr, &method_key, &jit_args, ret)
                            {
                                self.runtime.lock().unwrap().jit_executions += 1;
                                let snapshot = take_last_deopt_snapshot();
                                match self.complete_jit_execution(
                                    &method_key,
                                    method_clone.clone(),
                                    &code.deopt_info.local_kinds,
                                    &code.deopt_info.stack_kinds_by_pc,
                                    snapshot,
                                    take_pending_jit_exception(),
                                    Some(thread.current_frame().locals.clone()),
                                    result,
                                    ret,
                                ) {
                                    Some(JitInvocationResult::Returned(None)) => {
                                        return Ok(ExecutionResult::Void);
                                    }
                                    Some(JitInvocationResult::Returned(Some(value))) => {
                                        return Ok(ExecutionResult::Value(value));
                                    }
                                    Some(JitInvocationResult::Threw(exception_ref)) => {
                                        let class_name = self.get_object_class(exception_ref)?;
                                        return Err(VmError::UnhandledException { class_name });
                                    }
                                    None => {}
                                }
                            }
                        }
                    } else {
                        self.runtime.lock().unwrap().jit_executions += 1;
                    }
                }
            }

            loop {
                let opcode_pc = thread.current_frame().pc;
                if opcode_pc >= thread.current_frame().code.len() {
                    return Err(VmError::MissingReturn);
                }
                let opcode_byte = thread.current_frame_mut().read_u8()?;
                let opcode = Opcode::from_byte(opcode_byte).ok_or(VmError::InvalidOpcode {
                    opcode: opcode_byte,
                    pc: opcode_pc,
                })?;

                if self.trace {
                    let frame = thread.current_frame();
                    let stack_repr: Vec<_> = frame.stack.iter().map(|v| format!("{v}")).collect();
                    eprintln!(
                        "  [{}.{}{}] pc={opcode_pc:<4} {opcode:?}  stack=[{}]  depth={}",
                        frame.class_name,
                        frame.method_name,
                        frame.descriptor,
                        stack_repr.join(", "),
                        thread.depth(),
                    );
                }

                let frame_key_before_opcode = thread.current_frame().method_key();
                match self.execute_opcode(&mut thread, opcode, opcode_pc) {
                    Ok(Some(result)) => return Ok(result),
                    Ok(None) => {
                        if thread.current_frame().method_key() == frame_key_before_opcode {
                            let osr_entry_pc = thread.current_frame().pc;
                            if osr_entry_pc <= opcode_pc {
                                let osr_candidate = {
                                    let frame = thread.current_frame_mut();
                                    frame.increment_backedge_count(osr_entry_pc);
                                    if frame.stack.is_empty()
                                        && self
                                            .jit
                                            .as_ref()
                                            .is_some_and(|jit| jit.should_osr(frame, osr_entry_pc))
                                    {
                                        Some((
                                            frame.to_method(),
                                            frame.locals.clone(),
                                            osr_entry_pc,
                                        ))
                                    } else {
                                        None
                                    }
                                };

                                if let Some((method, locals, entry_pc)) = osr_candidate {
                                    let osr_result =
                                        self.try_execute_osr_method(method, locals, entry_pc);
                                    set_current_vm(vm_ptr);
                                    if let Some(result) = osr_result {
                                        match result {
                                            JitInvocationResult::Returned(Some(value)) => {
                                                if thread.depth() == 1 {
                                                    return Ok(ExecutionResult::Value(value));
                                                }
                                                thread.pop_frame();
                                                thread.current_frame_mut().push(value)?;
                                            }
                                            JitInvocationResult::Returned(None) => {
                                                if thread.depth() == 1 {
                                                    return Ok(ExecutionResult::Void);
                                                }
                                                thread.pop_frame();
                                            }
                                            JitInvocationResult::Threw(exception_ref) => {
                                                self.throw_exception(&mut thread, exception_ref)?;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(VmError::NullReference) => {
                        self.throw_new_exception(&mut thread, "java/lang/NullPointerException")?;
                    }
                    Err(VmError::ArrayIndexOutOfBounds { .. }) => {
                        self.throw_new_exception(
                            &mut thread,
                            "java/lang/ArrayIndexOutOfBoundsException",
                        )?;
                    }
                    Err(VmError::NegativeArraySize { .. }) => {
                        self.throw_new_exception(
                            &mut thread,
                            "java/lang/NegativeArraySizeException",
                        )?;
                    }
                    Err(VmError::ClassCastError { .. }) => {
                        self.throw_new_exception(&mut thread, "java/lang/ClassCastException")?;
                    }
                    Err(VmError::UnhandledException { class_name }) => {
                        // Native methods return `UnhandledException` to signal a Java-level
                        // throw. Try to deliver it to a matching handler. If no frame
                        // handles it, `throw_new_exception` re-returns `UnhandledException`
                        // and it propagates out of `execute`.
                        self.throw_new_exception(&mut thread, &class_name)?;
                    }
                    Err(err) => return Err(err),
                }
            }
        })();

        clear_current_vm();

        result
    }

    /// Execute a single opcode.
    ///
    /// Returns `Ok(Some(result))` when a return instruction terminates the
    /// entry-point method, `Ok(None)` to continue the loop.
    fn execute_opcode(
        &mut self,
        mut thread: &mut Thread,
        opcode: Opcode,
        opcode_pc: usize,
    ) -> Result<Option<ExecutionResult>, VmError> {
        match opcode {
            Opcode::AconstNull => execute_aconst_null(thread)?,
            Opcode::IconstM1 => execute_iconst(thread, -1)?,
            Opcode::Iconst0 => execute_iconst(thread, 0)?,
            Opcode::Iconst1 => execute_iconst(thread, 1)?,
            Opcode::Iconst2 => execute_iconst(thread, 2)?,
            Opcode::Iconst3 => execute_iconst(thread, 3)?,
            Opcode::Iconst4 => execute_iconst(thread, 4)?,
            Opcode::Iconst5 => execute_iconst(thread, 5)?,
            Opcode::Bipush => execute_bipush(thread)?,
            Opcode::Sipush => execute_sipush(thread)?,
            Opcode::Ldc => {
                let index = thread.current_frame_mut().read_u8()? as usize;
                let value = self.load_constant_or_condy(thread, index)?;
                thread.current_frame_mut().push(value)?;
            }
            Opcode::LdcW => {
                let index = thread.current_frame_mut().read_u16()? as usize;
                let value = self.load_constant_or_condy(thread, index)?;
                thread.current_frame_mut().push(value)?;
            }
            Opcode::Ldc2W => {
                let index = thread.current_frame_mut().read_u16()? as usize;
                let value = self.load_constant_or_condy(thread, index)?;
                thread.current_frame_mut().push(value)?;
            }
            Opcode::Lconst0 => execute_lconst(thread, 0)?,
            Opcode::Lconst1 => execute_lconst(thread, 1)?,
            Opcode::Fconst0 => execute_fconst(thread, 0.0)?,
            Opcode::Fconst1 => execute_fconst(thread, 1.0)?,
            Opcode::Fconst2 => execute_fconst(thread, 2.0)?,
            Opcode::Dconst0 => execute_dconst(thread, 0.0)?,
            Opcode::Dconst1 => execute_dconst(thread, 1.0)?,
            Opcode::Newarray => {
                let atype = thread.current_frame_mut().read_u8()?;
                let count = thread.current_frame_mut().pop()?.as_int()?;
                if count < 0 {
                    return Err(VmError::NegativeArraySize { size: count });
                }
                let n = count as usize;
                let reference = match atype {
                    4 | 5 | 8 | 9 | 10 => {
                        // boolean(4), char(5), byte(8), short(9), int(10)
                        self.heap.lock().unwrap().allocate_int_array(vec![0; n])
                    }
                    6 => self.heap.lock().unwrap().allocate(HeapValue::FloatArray {
                        values: vec![0.0; n],
                    }),
                    7 => self.heap.lock().unwrap().allocate(HeapValue::DoubleArray {
                        values: vec![0.0; n],
                    }),
                    11 => self
                        .heap
                        .lock()
                        .unwrap()
                        .allocate(HeapValue::LongArray { values: vec![0; n] }),
                    _ => return Err(VmError::UnsupportedNewArrayType { atype }),
                };
                thread
                    .current_frame_mut()
                    .push(Value::Reference(reference))?;
            }
            Opcode::Anewarray => {
                let index = thread.current_frame_mut().read_u16()? as usize;
                let component_type = thread
                    .current_frame()
                    .load_reference_class(index)?
                    .to_string();
                let count = thread.current_frame_mut().pop()?.as_int()?;
                if count < 0 {
                    return Err(VmError::NegativeArraySize { size: count });
                }
                let values = vec![Reference::Null; count as usize];
                let reference = self
                    .heap
                    .lock()
                    .unwrap()
                    .allocate_reference_array(component_type, values);
                thread
                    .current_frame_mut()
                    .push(Value::Reference(reference))?;
            }
            Opcode::Aload => {
                let index = thread.current_frame_mut().read_u8()? as usize;
                execute_aload(thread, index)?;
            }
            Opcode::Iload => {
                let index = thread.current_frame_mut().read_u8()? as usize;
                execute_iload(thread, index)?;
            }
            Opcode::Lload => {
                let index = thread.current_frame_mut().read_u8()? as usize;
                execute_lload(thread, index)?;
            }
            Opcode::Fload => {
                let index = thread.current_frame_mut().read_u8()? as usize;
                execute_fload(thread, index)?;
            }
            Opcode::Dload => {
                let index = thread.current_frame_mut().read_u8()? as usize;
                execute_dload(thread, index)?;
            }
            Opcode::Iload0 => execute_iload(thread, 0)?,
            Opcode::Lload0 => execute_lload(thread, 0)?,
            Opcode::Fload0 => execute_fload(thread, 0)?,
            Opcode::Dload0 => execute_dload(thread, 0)?,
            Opcode::Iload1 => execute_iload(thread, 1)?,
            Opcode::Lload1 => execute_lload(thread, 1)?,
            Opcode::Fload1 => execute_fload(thread, 1)?,
            Opcode::Dload1 => execute_dload(thread, 1)?,
            Opcode::Iload2 => execute_iload(thread, 2)?,
            Opcode::Lload2 => execute_lload(thread, 2)?,
            Opcode::Fload2 => execute_fload(thread, 2)?,
            Opcode::Dload2 => execute_dload(thread, 2)?,
            Opcode::Iload3 => execute_iload(thread, 3)?,
            Opcode::Lload3 => execute_lload(thread, 3)?,
            Opcode::Fload3 => execute_fload(thread, 3)?,
            Opcode::Dload3 => execute_dload(thread, 3)?,
            Opcode::Aload0 => execute_aload(thread, 0)?,
            Opcode::Aload1 => execute_aload(thread, 1)?,
            Opcode::Aload2 => execute_aload(thread, 2)?,
            Opcode::Aload3 => execute_aload(thread, 3)?,
            Opcode::Iaload => {
                let index = thread.current_frame_mut().pop()?.as_int()?;
                let array_ref = thread.current_frame_mut().pop()?.as_reference()?;
                let value = self
                    .heap
                    .lock()
                    .unwrap()
                    .load_int_array_element(array_ref, index)?;
                thread.current_frame_mut().push(Value::Int(value))?;
            }
            Opcode::Laload | Opcode::Faload | Opcode::Daload => {
                let index = thread.current_frame_mut().pop()?.as_int()?;
                let array_ref = thread.current_frame_mut().pop()?.as_reference()?;
                let value = self
                    .heap
                    .lock()
                    .unwrap()
                    .load_typed_array_element(array_ref, index)?;
                thread.current_frame_mut().push(value)?;
            }
            Opcode::Baload | Opcode::Caload | Opcode::Saload => {
                let index = thread.current_frame_mut().pop()?.as_int()?;
                let array_ref = thread.current_frame_mut().pop()?.as_reference()?;
                let value = self
                    .heap
                    .lock()
                    .unwrap()
                    .load_int_array_element(array_ref, index)?;
                thread.current_frame_mut().push(Value::Int(value))?;
            }
            Opcode::Aaload => {
                let index = thread.current_frame_mut().pop()?.as_int()?;
                let array_ref = thread.current_frame_mut().pop()?.as_reference()?;
                let reference = self
                    .heap
                    .lock()
                    .unwrap()
                    .load_reference_array_element(array_ref, index)?;
                thread
                    .current_frame_mut()
                    .push(Value::Reference(reference))?;
            }
            Opcode::Lastore | Opcode::Fastore | Opcode::Dastore => {
                let value = thread.current_frame_mut().pop()?;
                let index = thread.current_frame_mut().pop()?.as_int()?;
                let array_ref = thread.current_frame_mut().pop()?.as_reference()?;
                self.heap
                    .lock()
                    .unwrap()
                    .store_typed_array_element(array_ref, index, value)?;
            }
            Opcode::Bastore | Opcode::Castore | Opcode::Sastore => {
                let value = thread.current_frame_mut().pop()?.as_int()?;
                let index = thread.current_frame_mut().pop()?.as_int()?;
                let array_ref = thread.current_frame_mut().pop()?.as_reference()?;
                self.heap
                    .lock()
                    .unwrap()
                    .store_int_array_element(array_ref, index, value)?;
            }
            Opcode::Aastore => {
                let value = thread.current_frame_mut().pop()?.as_reference()?;
                let index = thread.current_frame_mut().pop()?.as_int()?;
                let array_ref = thread.current_frame_mut().pop()?.as_reference()?;
                self.heap
                    .lock()
                    .unwrap()
                    .store_reference_array_element(array_ref, index, value)?;
            }
            Opcode::Astore => {
                let index = thread.current_frame_mut().read_u8()? as usize;
                execute_astore(thread, index)?;
            }
            Opcode::Istore => {
                let index = thread.current_frame_mut().read_u8()? as usize;
                execute_istore(thread, index)?;
            }
            Opcode::Lstore => {
                let index = thread.current_frame_mut().read_u8()? as usize;
                execute_lstore(thread, index)?;
            }
            Opcode::Fstore => {
                let index = thread.current_frame_mut().read_u8()? as usize;
                execute_fstore(thread, index)?;
            }
            Opcode::Dstore => {
                let index = thread.current_frame_mut().read_u8()? as usize;
                execute_dstore(thread, index)?;
            }
            Opcode::Istore0 => execute_istore(thread, 0)?,
            Opcode::Lstore0 => execute_lstore(thread, 0)?,
            Opcode::Fstore0 => execute_fstore(thread, 0)?,
            Opcode::Dstore0 => execute_dstore(thread, 0)?,
            Opcode::Istore1 => execute_istore(thread, 1)?,
            Opcode::Lstore1 => execute_lstore(thread, 1)?,
            Opcode::Fstore1 => execute_fstore(thread, 1)?,
            Opcode::Dstore1 => execute_dstore(thread, 1)?,
            Opcode::Istore2 => execute_istore(thread, 2)?,
            Opcode::Lstore2 => execute_lstore(thread, 2)?,
            Opcode::Fstore2 => execute_fstore(thread, 2)?,
            Opcode::Dstore2 => execute_dstore(thread, 2)?,
            Opcode::Istore3 => execute_istore(thread, 3)?,
            Opcode::Lstore3 => execute_lstore(thread, 3)?,
            Opcode::Fstore3 => execute_fstore(thread, 3)?,
            Opcode::Dstore3 => execute_dstore(thread, 3)?,
            Opcode::Astore0 => execute_astore(thread, 0)?,
            Opcode::Astore1 => execute_astore(thread, 1)?,
            Opcode::Astore2 => execute_astore(thread, 2)?,
            Opcode::Astore3 => execute_astore(thread, 3)?,
            Opcode::Iastore => {
                let value = thread.current_frame_mut().pop()?.as_int()?;
                let index = thread.current_frame_mut().pop()?.as_int()?;
                let array_ref = thread.current_frame_mut().pop()?.as_reference()?;
                self.heap
                    .lock()
                    .unwrap()
                    .store_int_array_element(array_ref, index, value)?;
            }
            Opcode::Pop => execute_pop(thread)?,
            Opcode::Pop2 => {
                let top = thread.current_frame_mut().pop()?;
                if !matches!(top, Value::Long(_) | Value::Double(_)) {
                    let _ = thread.current_frame_mut().pop()?;
                }
            }
            Opcode::Dup => execute_dup(thread)?,
            Opcode::DupX1 => {
                let top = thread.current_frame_mut().pop()?;
                let below = thread.current_frame_mut().pop()?;
                if matches!(top, Value::Long(_) | Value::Double(_))
                    || matches!(below, Value::Long(_) | Value::Double(_))
                {
                    return Err(VmError::VerificationError {
                        pc: opcode_pc,
                        reason: "dup_x1 requires two category-1 values".to_string(),
                    });
                }
                thread.current_frame_mut().push(top)?;
                thread.current_frame_mut().push(below)?;
                thread.current_frame_mut().push(top)?;
            }
            Opcode::Dup2 => {
                let top = thread.current_frame_mut().pop()?;
                if matches!(top, Value::Long(_) | Value::Double(_)) {
                    thread.current_frame_mut().push(top)?;
                    thread.current_frame_mut().push(top)?;
                } else {
                    let below = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().push(below)?;
                    thread.current_frame_mut().push(top)?;
                    thread.current_frame_mut().push(below)?;
                    thread.current_frame_mut().push(top)?;
                }
            }
            Opcode::DupX2 => {
                let v1 = thread.current_frame_mut().pop()?;
                let v2 = thread.current_frame_mut().pop()?;
                if matches!(v1, Value::Long(_) | Value::Double(_)) {
                    return Err(VmError::VerificationError {
                        pc: opcode_pc,
                        reason: "dup_x2 requires top value to be category-1".to_string(),
                    });
                }
                if matches!(v2, Value::Long(_) | Value::Double(_)) {
                    thread.current_frame_mut().push(v1)?;
                    thread.current_frame_mut().push(v2)?;
                    thread.current_frame_mut().push(v1)?;
                } else {
                    let v3 = thread.current_frame_mut().pop()?;
                    if matches!(v3, Value::Long(_) | Value::Double(_)) {
                        return Err(VmError::VerificationError {
                            pc: opcode_pc,
                            reason: "dup_x2 requires either [cat1, cat2] or [cat1, cat1, cat1]"
                                .to_string(),
                        });
                    }
                    thread.current_frame_mut().push(v1)?;
                    thread.current_frame_mut().push(v3)?;
                    thread.current_frame_mut().push(v2)?;
                    thread.current_frame_mut().push(v1)?;
                }
            }
            Opcode::Dup2X1 => {
                let v1 = thread.current_frame_mut().pop()?;
                let v2 = thread.current_frame_mut().pop()?;
                if matches!(v1, Value::Long(_) | Value::Double(_)) {
                    thread.current_frame_mut().push(v1)?;
                    thread.current_frame_mut().push(v2)?;
                    thread.current_frame_mut().push(v1)?;
                } else {
                    let v3 = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().push(v2)?;
                    thread.current_frame_mut().push(v1)?;
                    thread.current_frame_mut().push(v3)?;
                    thread.current_frame_mut().push(v2)?;
                    thread.current_frame_mut().push(v1)?;
                }
            }
            Opcode::Dup2X2 => {
                let v1 = thread.current_frame_mut().pop()?;
                let v2 = thread.current_frame_mut().pop()?;
                if matches!(v1, Value::Long(_) | Value::Double(_)) {
                    if matches!(v2, Value::Long(_) | Value::Double(_)) {
                        thread.current_frame_mut().push(v1)?;
                        thread.current_frame_mut().push(v2)?;
                        thread.current_frame_mut().push(v1)?;
                    } else {
                        let v3 = thread.current_frame_mut().pop()?;
                        thread.current_frame_mut().push(v1)?;
                        thread.current_frame_mut().push(v3)?;
                        thread.current_frame_mut().push(v2)?;
                        thread.current_frame_mut().push(v1)?;
                    }
                } else if matches!(v2, Value::Long(_) | Value::Double(_)) {
                    let v3 = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().push(v2)?;
                    thread.current_frame_mut().push(v1)?;
                    thread.current_frame_mut().push(v3)?;
                    thread.current_frame_mut().push(v2)?;
                    thread.current_frame_mut().push(v1)?;
                } else {
                    let v3 = thread.current_frame_mut().pop()?;
                    let v4 = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().push(v2)?;
                    thread.current_frame_mut().push(v1)?;
                    thread.current_frame_mut().push(v4)?;
                    thread.current_frame_mut().push(v3)?;
                    thread.current_frame_mut().push(v2)?;
                    thread.current_frame_mut().push(v1)?;
                }
            }
            Opcode::Swap => {
                let top = thread.current_frame_mut().pop()?;
                let below = thread.current_frame_mut().pop()?;
                if matches!(top, Value::Long(_) | Value::Double(_))
                    || matches!(below, Value::Long(_) | Value::Double(_))
                {
                    return Err(VmError::VerificationError {
                        pc: opcode_pc,
                        reason: "swap requires two category-1 values".to_string(),
                    });
                }
                thread.current_frame_mut().push(top)?;
                thread.current_frame_mut().push(below)?;
            }
            Opcode::Iadd => execute_iadd(thread)?,
            Opcode::Isub => execute_isub(thread)?,
            Opcode::Imul => execute_imul(thread)?,
            Opcode::Idiv => {
                let rhs = thread.current_frame_mut().pop()?.as_int()?;
                if rhs == 0 {
                    self.throw_new_exception(&mut thread, "java/lang/ArithmeticException")?;
                    return Ok(None);
                }
                let lhs = thread.current_frame_mut().pop()?.as_int()?;
                thread.current_frame_mut().push(Value::Int(lhs / rhs))?;
            }
            Opcode::Irem => {
                let rhs = thread.current_frame_mut().pop()?.as_int()?;
                if rhs == 0 {
                    self.throw_new_exception(&mut thread, "java/lang/ArithmeticException")?;
                    return Ok(None);
                }
                let lhs = thread.current_frame_mut().pop()?.as_int()?;
                thread.current_frame_mut().push(Value::Int(lhs % rhs))?;
            }
            Opcode::Ineg => {
                let value = thread.current_frame_mut().pop()?.as_int()?;
                thread.current_frame_mut().push(Value::Int(-value))?;
            }
            // --- Long arithmetic ---
            Opcode::Ladd => {
                let rhs = thread.current_frame_mut().pop()?.as_long()?;
                let lhs = thread.current_frame_mut().pop()?.as_long()?;
                thread
                    .current_frame_mut()
                    .push(Value::Long(lhs.wrapping_add(rhs)))?;
            }
            Opcode::Lsub => {
                let rhs = thread.current_frame_mut().pop()?.as_long()?;
                let lhs = thread.current_frame_mut().pop()?.as_long()?;
                thread
                    .current_frame_mut()
                    .push(Value::Long(lhs.wrapping_sub(rhs)))?;
            }
            Opcode::Lmul => {
                let rhs = thread.current_frame_mut().pop()?.as_long()?;
                let lhs = thread.current_frame_mut().pop()?.as_long()?;
                thread
                    .current_frame_mut()
                    .push(Value::Long(lhs.wrapping_mul(rhs)))?;
            }
            Opcode::Ldiv => {
                let rhs = thread.current_frame_mut().pop()?.as_long()?;
                if rhs == 0 {
                    self.throw_new_exception(&mut thread, "java/lang/ArithmeticException")?;
                    return Ok(None);
                }
                let lhs = thread.current_frame_mut().pop()?.as_long()?;
                thread.current_frame_mut().push(Value::Long(lhs / rhs))?;
            }
            Opcode::Lrem => {
                let rhs = thread.current_frame_mut().pop()?.as_long()?;
                if rhs == 0 {
                    self.throw_new_exception(&mut thread, "java/lang/ArithmeticException")?;
                    return Ok(None);
                }
                let lhs = thread.current_frame_mut().pop()?.as_long()?;
                thread.current_frame_mut().push(Value::Long(lhs % rhs))?;
            }
            Opcode::Lneg => {
                let value = thread.current_frame_mut().pop()?.as_long()?;
                thread.current_frame_mut().push(Value::Long(-value))?;
            }
            // --- Float arithmetic ---
            Opcode::Fadd => {
                let rhs = thread.current_frame_mut().pop()?.as_float()?;
                let lhs = thread.current_frame_mut().pop()?.as_float()?;
                thread.current_frame_mut().push(Value::Float(lhs + rhs))?;
            }
            Opcode::Fsub => {
                let rhs = thread.current_frame_mut().pop()?.as_float()?;
                let lhs = thread.current_frame_mut().pop()?.as_float()?;
                thread.current_frame_mut().push(Value::Float(lhs - rhs))?;
            }
            Opcode::Fmul => {
                let rhs = thread.current_frame_mut().pop()?.as_float()?;
                let lhs = thread.current_frame_mut().pop()?.as_float()?;
                thread.current_frame_mut().push(Value::Float(lhs * rhs))?;
            }
            Opcode::Fdiv => {
                let rhs = thread.current_frame_mut().pop()?.as_float()?;
                let lhs = thread.current_frame_mut().pop()?.as_float()?;
                thread.current_frame_mut().push(Value::Float(lhs / rhs))?;
            }
            Opcode::Frem => {
                let rhs = thread.current_frame_mut().pop()?.as_float()?;
                let lhs = thread.current_frame_mut().pop()?.as_float()?;
                thread.current_frame_mut().push(Value::Float(lhs % rhs))?;
            }
            Opcode::Fneg => {
                let value = thread.current_frame_mut().pop()?.as_float()?;
                thread.current_frame_mut().push(Value::Float(-value))?;
            }
            // --- Double arithmetic ---
            Opcode::Dadd => {
                let rhs = thread.current_frame_mut().pop()?.as_double()?;
                let lhs = thread.current_frame_mut().pop()?.as_double()?;
                thread.current_frame_mut().push(Value::Double(lhs + rhs))?;
            }
            Opcode::Dsub => {
                let rhs = thread.current_frame_mut().pop()?.as_double()?;
                let lhs = thread.current_frame_mut().pop()?.as_double()?;
                thread.current_frame_mut().push(Value::Double(lhs - rhs))?;
            }
            Opcode::Dmul => {
                let rhs = thread.current_frame_mut().pop()?.as_double()?;
                let lhs = thread.current_frame_mut().pop()?.as_double()?;
                thread.current_frame_mut().push(Value::Double(lhs * rhs))?;
            }
            Opcode::Ddiv => {
                let rhs = thread.current_frame_mut().pop()?.as_double()?;
                let lhs = thread.current_frame_mut().pop()?.as_double()?;
                thread.current_frame_mut().push(Value::Double(lhs / rhs))?;
            }
            Opcode::Drem => {
                let rhs = thread.current_frame_mut().pop()?.as_double()?;
                let lhs = thread.current_frame_mut().pop()?.as_double()?;
                thread.current_frame_mut().push(Value::Double(lhs % rhs))?;
            }
            Opcode::Dneg => {
                let value = thread.current_frame_mut().pop()?.as_double()?;
                thread.current_frame_mut().push(Value::Double(-value))?;
            }
            Opcode::Ishl => {
                let rhs = thread.current_frame_mut().pop()?.as_int()? & 0x1f;
                let lhs = thread.current_frame_mut().pop()?.as_int()?;
                thread.current_frame_mut().push(Value::Int(lhs << rhs))?;
            }
            Opcode::Ishr => {
                let rhs = thread.current_frame_mut().pop()?.as_int()? & 0x1f;
                let lhs = thread.current_frame_mut().pop()?.as_int()?;
                thread.current_frame_mut().push(Value::Int(lhs >> rhs))?;
            }
            Opcode::Iushr => {
                let rhs = thread.current_frame_mut().pop()?.as_int()? & 0x1f;
                let lhs = thread.current_frame_mut().pop()?.as_int()?;
                thread
                    .current_frame_mut()
                    .push(Value::Int(((lhs as u32) >> rhs) as i32))?;
            }
            Opcode::Iand => {
                let rhs = thread.current_frame_mut().pop()?.as_int()?;
                let lhs = thread.current_frame_mut().pop()?.as_int()?;
                thread.current_frame_mut().push(Value::Int(lhs & rhs))?;
            }
            Opcode::Ior => {
                let rhs = thread.current_frame_mut().pop()?.as_int()?;
                let lhs = thread.current_frame_mut().pop()?.as_int()?;
                thread.current_frame_mut().push(Value::Int(lhs | rhs))?;
            }
            Opcode::Ixor => {
                let rhs = thread.current_frame_mut().pop()?.as_int()?;
                let lhs = thread.current_frame_mut().pop()?.as_int()?;
                thread.current_frame_mut().push(Value::Int(lhs ^ rhs))?;
            }
            Opcode::Lshl => {
                let rhs = thread.current_frame_mut().pop()?.as_int()? & 0x3f;
                let lhs = thread.current_frame_mut().pop()?.as_long()?;
                thread.current_frame_mut().push(Value::Long(lhs << rhs))?;
            }
            Opcode::Lshr => {
                let rhs = thread.current_frame_mut().pop()?.as_int()? & 0x3f;
                let lhs = thread.current_frame_mut().pop()?.as_long()?;
                thread.current_frame_mut().push(Value::Long(lhs >> rhs))?;
            }
            Opcode::Lushr => {
                let rhs = thread.current_frame_mut().pop()?.as_int()? & 0x3f;
                let lhs = thread.current_frame_mut().pop()?.as_long()?;
                thread
                    .current_frame_mut()
                    .push(Value::Long(((lhs as u64) >> rhs) as i64))?;
            }
            Opcode::Land => {
                let rhs = thread.current_frame_mut().pop()?.as_long()?;
                let lhs = thread.current_frame_mut().pop()?.as_long()?;
                thread.current_frame_mut().push(Value::Long(lhs & rhs))?;
            }
            Opcode::Lor => {
                let rhs = thread.current_frame_mut().pop()?.as_long()?;
                let lhs = thread.current_frame_mut().pop()?.as_long()?;
                thread.current_frame_mut().push(Value::Long(lhs | rhs))?;
            }
            Opcode::Lxor => {
                let rhs = thread.current_frame_mut().pop()?.as_long()?;
                let lhs = thread.current_frame_mut().pop()?.as_long()?;
                thread.current_frame_mut().push(Value::Long(lhs ^ rhs))?;
            }
            Opcode::Iinc => {
                let index = thread.current_frame_mut().read_u8()? as usize;
                let delta = thread.current_frame_mut().read_u8()? as i8 as i32;
                let value = thread.current_frame().load_local(index)?.as_int()?;
                thread
                    .current_frame_mut()
                    .store_local(index, Value::Int(value + delta))?;
            }
            Opcode::I2b => {
                let value = thread.current_frame_mut().pop()?.as_int()?;
                thread
                    .current_frame_mut()
                    .push(Value::Int(value as i8 as i32))?;
            }
            Opcode::I2c => {
                let value = thread.current_frame_mut().pop()?.as_int()?;
                thread
                    .current_frame_mut()
                    .push(Value::Int(value as u16 as i32))?;
            }
            Opcode::I2s => {
                let value = thread.current_frame_mut().pop()?.as_int()?;
                thread
                    .current_frame_mut()
                    .push(Value::Int(value as i16 as i32))?;
            }
            // --- Widening / narrowing conversions ---
            Opcode::I2l => {
                let v = thread.current_frame_mut().pop()?.as_int()?;
                thread.current_frame_mut().push(Value::Long(v as i64))?;
            }
            Opcode::I2f => {
                let v = thread.current_frame_mut().pop()?.as_int()?;
                thread.current_frame_mut().push(Value::Float(v as f32))?;
            }
            Opcode::I2d => {
                let v = thread.current_frame_mut().pop()?.as_int()?;
                thread.current_frame_mut().push(Value::Double(v as f64))?;
            }
            Opcode::L2i => {
                let v = thread.current_frame_mut().pop()?.as_long()?;
                thread.current_frame_mut().push(Value::Int(v as i32))?;
            }
            Opcode::L2f => {
                let v = thread.current_frame_mut().pop()?.as_long()?;
                thread.current_frame_mut().push(Value::Float(v as f32))?;
            }
            Opcode::L2d => {
                let v = thread.current_frame_mut().pop()?.as_long()?;
                thread.current_frame_mut().push(Value::Double(v as f64))?;
            }
            Opcode::F2i => {
                let v = thread.current_frame_mut().pop()?.as_float()?;
                thread.current_frame_mut().push(Value::Int(v as i32))?;
            }
            Opcode::F2l => {
                let v = thread.current_frame_mut().pop()?.as_float()?;
                thread.current_frame_mut().push(Value::Long(v as i64))?;
            }
            Opcode::F2d => {
                let v = thread.current_frame_mut().pop()?.as_float()?;
                thread.current_frame_mut().push(Value::Double(v as f64))?;
            }
            Opcode::D2i => {
                let v = thread.current_frame_mut().pop()?.as_double()?;
                thread.current_frame_mut().push(Value::Int(v as i32))?;
            }
            Opcode::D2l => {
                let v = thread.current_frame_mut().pop()?.as_double()?;
                thread.current_frame_mut().push(Value::Long(v as i64))?;
            }
            Opcode::D2f => {
                let v = thread.current_frame_mut().pop()?.as_double()?;
                thread.current_frame_mut().push(Value::Float(v as f32))?;
            }
            // --- Long / float / double comparisons ---
            Opcode::Lcmp => {
                let rhs = thread.current_frame_mut().pop()?.as_long()?;
                let lhs = thread.current_frame_mut().pop()?.as_long()?;
                let result = if lhs > rhs {
                    1
                } else if lhs == rhs {
                    0
                } else {
                    -1
                };
                thread.current_frame_mut().push(Value::Int(result))?;
            }
            Opcode::Fcmpl => {
                let rhs = thread.current_frame_mut().pop()?.as_float()?;
                let lhs = thread.current_frame_mut().pop()?.as_float()?;
                let result = if lhs > rhs {
                    1
                } else if lhs == rhs {
                    0
                } else {
                    -1
                };
                thread.current_frame_mut().push(Value::Int(result))?;
            }
            Opcode::Fcmpg => {
                let rhs = thread.current_frame_mut().pop()?.as_float()?;
                let lhs = thread.current_frame_mut().pop()?.as_float()?;
                let result = if lhs < rhs {
                    -1
                } else if lhs == rhs {
                    0
                } else {
                    1
                };
                thread.current_frame_mut().push(Value::Int(result))?;
            }
            Opcode::Dcmpl => {
                let rhs = thread.current_frame_mut().pop()?.as_double()?;
                let lhs = thread.current_frame_mut().pop()?.as_double()?;
                let result = if lhs > rhs {
                    1
                } else if lhs == rhs {
                    0
                } else {
                    -1
                };
                thread.current_frame_mut().push(Value::Int(result))?;
            }
            Opcode::Dcmpg => {
                let rhs = thread.current_frame_mut().pop()?.as_double()?;
                let lhs = thread.current_frame_mut().pop()?.as_double()?;
                let result = if lhs < rhs {
                    -1
                } else if lhs == rhs {
                    0
                } else {
                    1
                };
                thread.current_frame_mut().push(Value::Int(result))?;
            }
            Opcode::Ifeq => {
                let offset = thread.current_frame_mut().read_i16()?;
                let value = thread.current_frame_mut().pop()?.as_int()?;
                if value == 0 {
                    thread
                        .current_frame_mut()
                        .branch(opcode_pc, offset.into())?;
                }
            }
            Opcode::Ifne => {
                let offset = thread.current_frame_mut().read_i16()?;
                let value = thread.current_frame_mut().pop()?.as_int()?;
                if value != 0 {
                    thread
                        .current_frame_mut()
                        .branch(opcode_pc, offset.into())?;
                }
            }
            Opcode::Iflt => {
                let offset = thread.current_frame_mut().read_i16()?;
                let value = thread.current_frame_mut().pop()?.as_int()?;
                if value < 0 {
                    thread
                        .current_frame_mut()
                        .branch(opcode_pc, offset.into())?;
                }
            }
            Opcode::Ifge => {
                let offset = thread.current_frame_mut().read_i16()?;
                let value = thread.current_frame_mut().pop()?.as_int()?;
                if value >= 0 {
                    thread
                        .current_frame_mut()
                        .branch(opcode_pc, offset.into())?;
                }
            }
            Opcode::Ifgt => {
                let offset = thread.current_frame_mut().read_i16()?;
                let value = thread.current_frame_mut().pop()?.as_int()?;
                if value > 0 {
                    thread
                        .current_frame_mut()
                        .branch(opcode_pc, offset.into())?;
                }
            }
            Opcode::Ifle => {
                let offset = thread.current_frame_mut().read_i16()?;
                let value = thread.current_frame_mut().pop()?.as_int()?;
                if value <= 0 {
                    thread
                        .current_frame_mut()
                        .branch(opcode_pc, offset.into())?;
                }
            }
            Opcode::IfIcmpeq => {
                let offset = thread.current_frame_mut().read_i16()?;
                let rhs = thread.current_frame_mut().pop()?.as_int()?;
                let lhs = thread.current_frame_mut().pop()?.as_int()?;
                if lhs == rhs {
                    thread
                        .current_frame_mut()
                        .branch(opcode_pc, offset.into())?;
                }
            }
            Opcode::IfIcmpne => {
                let offset = thread.current_frame_mut().read_i16()?;
                let rhs = thread.current_frame_mut().pop()?.as_int()?;
                let lhs = thread.current_frame_mut().pop()?.as_int()?;
                if lhs != rhs {
                    thread
                        .current_frame_mut()
                        .branch(opcode_pc, offset.into())?;
                }
            }
            Opcode::IfIcmplt => {
                let offset = thread.current_frame_mut().read_i16()?;
                let rhs = thread.current_frame_mut().pop()?.as_int()?;
                let lhs = thread.current_frame_mut().pop()?.as_int()?;
                if lhs < rhs {
                    thread
                        .current_frame_mut()
                        .branch(opcode_pc, offset.into())?;
                }
            }
            Opcode::IfIcmpge => {
                let offset = thread.current_frame_mut().read_i16()?;
                let rhs = thread.current_frame_mut().pop()?.as_int()?;
                let lhs = thread.current_frame_mut().pop()?.as_int()?;
                if lhs >= rhs {
                    thread
                        .current_frame_mut()
                        .branch(opcode_pc, offset.into())?;
                }
            }
            Opcode::IfIcmpgt => {
                let offset = thread.current_frame_mut().read_i16()?;
                let rhs = thread.current_frame_mut().pop()?.as_int()?;
                let lhs = thread.current_frame_mut().pop()?.as_int()?;
                if lhs > rhs {
                    thread
                        .current_frame_mut()
                        .branch(opcode_pc, offset.into())?;
                }
            }
            Opcode::IfIcmple => {
                let offset = thread.current_frame_mut().read_i16()?;
                let rhs = thread.current_frame_mut().pop()?.as_int()?;
                let lhs = thread.current_frame_mut().pop()?.as_int()?;
                if lhs <= rhs {
                    thread
                        .current_frame_mut()
                        .branch(opcode_pc, offset.into())?;
                }
            }
            Opcode::IfAcmpeq => {
                let offset = thread.current_frame_mut().read_i16()?;
                let rhs = thread.current_frame_mut().pop()?.as_reference()?;
                let lhs = thread.current_frame_mut().pop()?.as_reference()?;
                if lhs == rhs {
                    thread
                        .current_frame_mut()
                        .branch(opcode_pc, offset.into())?;
                }
            }
            Opcode::IfAcmpne => {
                let offset = thread.current_frame_mut().read_i16()?;
                let rhs = thread.current_frame_mut().pop()?.as_reference()?;
                let lhs = thread.current_frame_mut().pop()?.as_reference()?;
                if lhs != rhs {
                    thread
                        .current_frame_mut()
                        .branch(opcode_pc, offset.into())?;
                }
            }
            Opcode::Tableswitch => {
                // Align pc to a 4-byte boundary (relative to method start).
                let padding = (4 - (thread.current_frame().pc % 4)) % 4;
                for _ in 0..padding {
                    thread.current_frame_mut().read_u8()?;
                }
                let default = thread.current_frame_mut().read_i32()?;
                let low = thread.current_frame_mut().read_i32()?;
                let high = thread.current_frame_mut().read_i32()?;
                let count = (high - low + 1) as usize;
                let mut offsets = Vec::with_capacity(count);
                for _ in 0..count {
                    offsets.push(thread.current_frame_mut().read_i32()?);
                }
                let index = thread.current_frame_mut().pop()?.as_int()?;
                let offset = if index >= low && index <= high {
                    offsets[(index - low) as usize]
                } else {
                    default
                };
                thread.current_frame_mut().branch(opcode_pc, offset)?;
            }
            Opcode::Lookupswitch => {
                let padding = (4 - (thread.current_frame().pc % 4)) % 4;
                for _ in 0..padding {
                    thread.current_frame_mut().read_u8()?;
                }
                let default = thread.current_frame_mut().read_i32()?;
                let npairs = thread.current_frame_mut().read_i32()? as usize;
                let mut pairs = Vec::with_capacity(npairs);
                for _ in 0..npairs {
                    let key = thread.current_frame_mut().read_i32()?;
                    let offset = thread.current_frame_mut().read_i32()?;
                    pairs.push((key, offset));
                }
                let key = thread.current_frame_mut().pop()?.as_int()?;
                let offset = pairs
                    .iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, o)| *o)
                    .unwrap_or(default);
                thread.current_frame_mut().branch(opcode_pc, offset)?;
            }
            Opcode::Goto => {
                let offset = thread.current_frame_mut().read_i16()?;
                thread
                    .current_frame_mut()
                    .branch(opcode_pc, offset.into())?;
            }
            Opcode::Jsr => {
                let offset = thread.current_frame_mut().read_i16()?;
                let return_pc = thread.current_frame().pc;
                thread
                    .current_frame_mut()
                    .push(Value::ReturnAddress(return_pc))?;
                thread
                    .current_frame_mut()
                    .branch(opcode_pc, offset.into())?;
            }
            Opcode::Ret => {
                let index = thread.current_frame_mut().read_u8()? as usize;
                let target = thread
                    .current_frame()
                    .load_local(index)?
                    .as_return_address()?;
                if target >= thread.current_frame().code.len() {
                    return Err(VmError::InvalidBranchTarget {
                        target: target as isize,
                        code_len: thread.current_frame().code.len(),
                    });
                }
                thread.current_frame_mut().pc = target;
            }
            Opcode::GotoW => {
                let offset = thread.current_frame_mut().read_i32()?;
                thread.current_frame_mut().branch(opcode_pc, offset)?;
            }
            Opcode::JsrW => {
                let offset = thread.current_frame_mut().read_i32()?;
                let return_pc = thread.current_frame().pc;
                thread
                    .current_frame_mut()
                    .push(Value::ReturnAddress(return_pc))?;
                thread.current_frame_mut().branch(opcode_pc, offset)?;
            }

            // --- References: field access ---
            Opcode::Getstatic => {
                let index = thread.current_frame_mut().read_u16()? as usize;
                let field_ref = thread.current_frame().load_field_ref(index)?.clone();
                self.ensure_class_loaded(&field_ref.class_name)?;
                self.ensure_class_initialized(&field_ref.class_name)?;
                let value = self.get_static_field(&field_ref.class_name, &field_ref.field_name)?;
                thread.current_frame_mut().push(value)?;
            }
            Opcode::Putstatic => {
                let index = thread.current_frame_mut().read_u16()? as usize;
                let field_ref = thread.current_frame().load_field_ref(index)?.clone();
                let value = thread.current_frame_mut().pop()?;
                self.ensure_class_loaded(&field_ref.class_name)?;
                self.ensure_class_initialized(&field_ref.class_name)?;
                self.put_static_field(&field_ref.class_name, &field_ref.field_name, value)?;
            }
            Opcode::Getfield => {
                let index = thread.current_frame_mut().read_u16()? as usize;
                let field_ref = thread.current_frame().load_field_ref(index)?.clone();
                let object_ref = thread.current_frame_mut().pop()?.as_reference()?;
                let value = self.get_instance_field_from_declaring(
                    object_ref,
                    &field_ref.class_name,
                    &field_ref.field_name,
                )?;
                thread.current_frame_mut().push(value)?;
            }
            Opcode::Putfield => {
                let index = thread.current_frame_mut().read_u16()? as usize;
                let field_ref = thread.current_frame().load_field_ref(index)?.clone();
                let value = thread.current_frame_mut().pop()?;
                let object_ref = thread.current_frame_mut().pop()?.as_reference()?;
                self.set_object_field_from_declaring(
                    object_ref,
                    &field_ref.class_name,
                    &field_ref.field_name,
                    value,
                )?;
            }

            // --- References: method invocation ---
            Opcode::Invokevirtual => {
                let index = thread.current_frame_mut().read_u16()? as usize;
                thread.current_frame_mut().increment_call_count(index);
                let method_ref = thread.current_frame().load_method_ref(index)?.clone();
                let arg_count = parse_arg_count(&method_ref.descriptor)?;

                let mut args = Vec::with_capacity(arg_count);
                for _ in 0..arg_count {
                    args.push(thread.current_frame_mut().pop()?);
                }
                args.reverse();
                let receiver = thread.current_frame_mut().pop()?.as_reference()?;

                if Self::is_signature_polymorphic(
                    &method_ref.class_name,
                    &method_ref.method_name,
                ) {
                    let result =
                        self.invoke_signature_polymorphic(receiver, &method_ref, args)?;
                    if let Some(value) = result {
                        thread.current_frame_mut().push(value)?;
                    }
                    return Ok(None);
                }

                let should_return_false = receiver == Reference::Null
                    && method_ref.class_name == "java/lang/Class"
                    && method_ref.method_name == "desiredAssertionStatus"
                    && method_ref.descriptor == "()Z";

                if should_return_false {
                    thread.current_frame_mut().push(Value::Int(0))?;
                } else {
                    let receiver_class = self.get_object_class(receiver)?;

                    // Native shadows on the receiver class take precedence over
                    // any inherited method table entry (used by e.g. Lookup,
                    // Unsafe, VarHandle whose placeholder classes don't
                    // register every JDK method explicitly).
                    if self.has_native_override(
                        &receiver_class,
                        &method_ref.method_name,
                        &method_ref.descriptor,
                    ) {
                        let mut all_args = vec![Value::Reference(receiver)];
                        all_args.extend(args);
                        let result = self.invoke_native(
                            &receiver_class,
                            &method_ref.method_name,
                            &method_ref.descriptor,
                            &all_args,
                        )?;
                        if let Some(value) = result {
                            thread.current_frame_mut().push(value)?;
                        }
                    } else if let Some(cached_method) = thread
                        .current_frame()
                        .get_cached_invoke(index, &receiver_class)
                    {
                        let mut all_args = vec![Value::Reference(receiver)];
                        all_args.extend(args);
                        match cached_method {
                            ClassMethod::Native => {
                                let result = self.invoke_native(
                                    &receiver_class,
                                    &method_ref.method_name,
                                    &method_ref.descriptor,
                                    &all_args,
                                )?;
                                if let Some(value) = result {
                                    thread.current_frame_mut().push(value)?;
                                }
                            }
                            ClassMethod::Bytecode(bytecode_method) => {
                                let callee = bytecode_method
                                    .clone()
                                    .with_initial_locals(Vm::args_to_locals(all_args));
                                thread.push_frame(Frame::new(callee));
                            }
                        }
                    } else {
                        let resolved =
                            self.resolve_method(
                                &receiver_class,
                                &method_ref.method_name,
                                &method_ref.descriptor,
                            );
                        match resolved {
                            Ok((resolved_class, class_method)) => {
                                thread.current_frame_mut().cache_invoke(
                                    index,
                                    resolved_class.clone(),
                                    class_method.clone(),
                                );
                                let mut all_args = vec![Value::Reference(receiver)];
                                all_args.extend(args);
                                match class_method {
                                    ClassMethod::Native => {
                                        let result = self.invoke_native(
                                            &resolved_class,
                                            &method_ref.method_name,
                                            &method_ref.descriptor,
                                            &all_args,
                                        )?;
                                        if let Some(value) = result {
                                            thread.current_frame_mut().push(value)?;
                                        }
                                    }
                                    ClassMethod::Bytecode(bytecode_method) => {
                                        let callee = bytecode_method
                                            .clone()
                                            .with_initial_locals(Vm::args_to_locals(all_args));
                                        thread.push_frame(Frame::new(callee));
                                    }
                                }
                            }
                            Err(VmError::MethodNotFound { .. }) => {
                                // Method tables didn't carry an entry, but the
                                // builtin native dispatcher may still have one
                                // (placeholder classes like MethodHandles$Lookup
                                // don't pre-populate every method).
                                let mut all_args = vec![Value::Reference(receiver)];
                                all_args.extend(args);
                                match self.invoke_native(
                                    &receiver_class,
                                    &method_ref.method_name,
                                    &method_ref.descriptor,
                                    &all_args,
                                ) {
                                    Ok(result) => {
                                        if let Some(value) = result {
                                            thread.current_frame_mut().push(value)?;
                                        }
                                    }
                                    Err(_) => {
                                        return Err(VmError::MethodNotFound {
                                            class_name: receiver_class,
                                            method_name: method_ref.method_name.clone(),
                                            descriptor: method_ref.descriptor.clone(),
                                        });
                                    }
                                }
                            }
                            Err(e) => return Err(e),
                        }
                    }
                }
            }
            Opcode::Invokespecial => {
                let index = thread.current_frame_mut().read_u16()? as usize;
                thread.current_frame_mut().increment_call_count(index);
                let method_ref = thread.current_frame().load_method_ref(index)?.clone();
                let arg_count = parse_arg_count(&method_ref.descriptor)?;

                let mut args = Vec::with_capacity(arg_count);
                for _ in 0..arg_count {
                    args.push(thread.current_frame_mut().pop()?);
                }
                args.reverse();
                let receiver = thread.current_frame_mut().pop()?.as_reference()?;

                // invokespecial uses the compile-time class, not the runtime class
                self.dispatch_instance_method(
                    &mut thread,
                    &method_ref.class_name,
                    &method_ref,
                    receiver,
                    args,
                )?;
            }
            Opcode::Invokestatic => {
                let index = thread.current_frame_mut().read_u16()? as usize;
                thread.current_frame_mut().increment_call_count(index);
                let method_ref = thread.current_frame().load_method_ref(index)?.clone();
                let arg_count = parse_arg_count(&method_ref.descriptor)?;

                let mut args = Vec::with_capacity(arg_count);
                for _ in 0..arg_count {
                    args.push(thread.current_frame_mut().pop()?);
                }
                args.reverse();

                let class_name = &method_ref.class_name;
                self.ensure_class_loaded(class_name)?;
                self.ensure_class_initialized(class_name)?;

                // Shortcut: some JDK static methods drag in heavy
                // machinery (Reference handler threads, security,
                // reflection). When a native shadow exists, dispatch
                // to it rather than running the JDK bytecode.
                if self.has_native_override(
                    class_name,
                    &method_ref.method_name,
                    &method_ref.descriptor,
                ) {
                    let result = self.invoke_native(
                        class_name,
                        &method_ref.method_name,
                        &method_ref.descriptor,
                        &args,
                    )?;
                    if let Some(value) = result {
                        thread.current_frame_mut().push(value)?;
                    }
                } else {
                    let class = self.get_class(class_name)?;
                    let class_method = class
                        .methods
                        .get(&(
                            method_ref.method_name.clone(),
                            method_ref.descriptor.clone(),
                        ))
                        .cloned()
                        .ok_or_else(|| VmError::MethodNotFound {
                            class_name: class_name.clone(),
                            method_name: method_ref.method_name.clone(),
                            descriptor: method_ref.descriptor.clone(),
                        })?;

                    match class_method {
                        ClassMethod::Native => {
                            let result = self.invoke_native(
                                class_name,
                                &method_ref.method_name,
                                &method_ref.descriptor,
                                &args,
                            )?;
                            if let Some(value) = result {
                                thread.current_frame_mut().push(value)?;
                            }
                        }
                        ClassMethod::Bytecode(method) => {
                            let should_jit = self.jit.as_ref().is_some_and(|jit| {
                                jit.should_compile(thread.current_frame(), Some(index))
                            });

                            let jit_result = if should_jit {
                                self.try_execute_jit_method(&method, &args)
                            } else {
                                None
                            };

                            if let Some(result) = jit_result {
                                match result {
                                    JitInvocationResult::Returned(Some(value)) => {
                                        thread.current_frame_mut().push(value)?;
                                    }
                                    JitInvocationResult::Returned(None) => {}
                                    JitInvocationResult::Threw(exception_ref) => {
                                        self.throw_exception(&mut thread, exception_ref)?;
                                    }
                                }
                            } else {
                                let callee = method.with_initial_locals(Vm::args_to_locals(args));
                                thread.push_frame(Frame::new(callee));
                            }
                        }
                    }
                }
            }

            Opcode::Invokeinterface => {
                let index = thread.current_frame_mut().read_u16()? as usize;
                thread.current_frame_mut().increment_call_count(index);
                let _count = thread.current_frame_mut().read_u8()?;
                let _zero = thread.current_frame_mut().read_u8()?;
                let method_ref = thread.current_frame().load_method_ref(index)?.clone();
                let arg_count = parse_arg_count(&method_ref.descriptor)?;

                let mut args = Vec::with_capacity(arg_count);
                for _ in 0..arg_count {
                    args.push(thread.current_frame_mut().pop()?);
                }
                args.reverse();
                let receiver = thread.current_frame_mut().pop()?.as_reference()?;

                let class_name = self.get_object_class(receiver)?;
                self.dispatch_instance_method(
                    &mut thread,
                    &class_name,
                    &method_ref,
                    receiver,
                    args,
                )?;
            }

            Opcode::Invokedynamic => {
                let index = thread.current_frame_mut().read_u16()? as usize;
                let _zero1 = thread.current_frame_mut().read_u8()?;
                let _zero2 = thread.current_frame_mut().read_u8()?;

                let site = thread
                    .current_frame()
                    .invoke_dynamic_sites
                    .get(index)
                    .and_then(|s| s.as_ref())
                    .cloned()
                    .ok_or_else(|| VmError::InvalidOpcode {
                        opcode: 0xba,
                        pc: opcode_pc,
                    })?;

                let arg_count = parse_arg_count(&site.descriptor)?;
                let mut args = Vec::with_capacity(arg_count);
                for _ in 0..arg_count {
                    args.push(thread.current_frame_mut().pop()?);
                }
                args.reverse();

                match &site.kind {
                    InvokeDynamicKind::LambdaProxy {
                        target_class,
                        target_method,
                        target_descriptor,
                    } => {
                        let proxy = self.allocate_lambda_proxy(
                            &site,
                            target_class,
                            target_method,
                            target_descriptor,
                            args,
                        )?;
                        thread.current_frame_mut().push(Value::Reference(proxy))?;
                    }
                    InvokeDynamicKind::StringConcat { recipe, constants } => {
                        let concat = self.build_string_concat(
                            recipe.as_deref(),
                            constants,
                            &args,
                            &site.descriptor,
                        )?;
                        thread.current_frame_mut().push(self.new_string(concat))?;
                    }
                    InvokeDynamicKind::Unknown => {
                        // Unknown bootstrap method — push null as placeholder.
                        thread
                            .current_frame_mut()
                            .push(Value::Reference(Reference::Null))?;
                    }
                    InvokeDynamicKind::BootstrapMethodHandle { .. } => {
                        self.invoke_interp_dynamic_via_method_handle(&mut thread, &site, args)?;
                    }
                }
            }

            // --- Monitors ---
            Opcode::Monitorenter => {
                let obj_ref = thread.current_frame_mut().pop()?.as_reference()?;
                self.enter_monitor(obj_ref)?;
            }
            Opcode::Monitorexit => {
                let obj_ref = thread.current_frame_mut().pop()?.as_reference()?;
                match self.exit_monitor(obj_ref) {
                    Ok(()) => {}
                    Err(VmError::UnhandledException { class_name }) => {
                        self.throw_new_exception(&mut thread, &class_name)?;
                        return Ok(None);
                    }
                    Err(error) => return Err(error),
                }
            }

            // --- References: object creation ---
            Opcode::New => {
                let index = thread.current_frame_mut().read_u16()? as usize;
                let class_name = thread
                    .current_frame()
                    .load_reference_class(index)?
                    .to_string();
                self.ensure_class_loaded(&class_name)?;
                self.ensure_class_initialized(&class_name)?;
                let all_instance_fields = self.collect_instance_fields(&class_name)?;
                let fields: Vec<Value> = all_instance_fields
                    .iter()
                    .map(|(_, descriptor)| default_value_for_descriptor(descriptor))
                    .collect();
                let reference = self
                    .heap
                    .lock()
                    .unwrap()
                    .allocate(HeapValue::Object { class_name, fields });
                thread
                    .current_frame_mut()
                    .push(Value::Reference(reference))?;
            }
            Opcode::Athrow => {
                let exception_ref = thread.current_frame_mut().pop()?.as_reference()?;
                if exception_ref == Reference::Null {
                    return Err(VmError::NullReference);
                }
                self.throw_exception(&mut thread, exception_ref)?;
            }
            Opcode::Multianewarray => {
                let index = thread.current_frame_mut().read_u16()? as usize;
                let class_name = thread
                    .current_frame()
                    .load_reference_class(index)?
                    .to_string();
                let dimensions = thread.current_frame_mut().read_u8()? as usize;
                let mut counts = Vec::with_capacity(dimensions);
                for _ in 0..dimensions {
                    counts.push(thread.current_frame_mut().pop()?.as_int()?);
                }
                counts.reverse();
                let reference = self.allocate_multi_array_descriptor(&class_name, &counts)?;
                thread
                    .current_frame_mut()
                    .push(Value::Reference(reference))?;
            }
            Opcode::Wide => {
                let inner_byte = thread.current_frame_mut().read_u8()?;
                let inner = Opcode::from_byte(inner_byte).ok_or(VmError::InvalidOpcode {
                    opcode: inner_byte,
                    pc: opcode_pc,
                })?;
                let index = thread.current_frame_mut().read_u16()? as usize;
                match inner {
                    Opcode::Iload
                    | Opcode::Lload
                    | Opcode::Fload
                    | Opcode::Dload
                    | Opcode::Aload => {
                        let value = thread.current_frame().load_local(index)?;
                        thread.current_frame_mut().push(value)?;
                    }
                    Opcode::Istore
                    | Opcode::Lstore
                    | Opcode::Fstore
                    | Opcode::Dstore
                    | Opcode::Astore => {
                        let value = thread.current_frame_mut().pop()?;
                        thread.current_frame_mut().store_local(index, value)?;
                    }
                    Opcode::Iinc => {
                        let delta = thread.current_frame_mut().read_i16()? as i32;
                        let value = thread.current_frame().load_local(index)?.as_int()?;
                        thread
                            .current_frame_mut()
                            .store_local(index, Value::Int(value + delta))?;
                    }
                    Opcode::Ret => {
                        let target = thread
                            .current_frame()
                            .load_local(index)?
                            .as_return_address()?;
                        if target >= thread.current_frame().code.len() {
                            return Err(VmError::InvalidBranchTarget {
                                target: target as isize,
                                code_len: thread.current_frame().code.len(),
                            });
                        }
                        thread.current_frame_mut().pc = target;
                    }
                    _ => {
                        return Err(VmError::InvalidOpcode {
                            opcode: inner_byte,
                            pc: opcode_pc,
                        });
                    }
                }
            }
            Opcode::Checkcast => {
                let index = thread.current_frame_mut().read_u16()? as usize;
                let target = thread
                    .current_frame()
                    .load_reference_class(index)?
                    .to_string();
                let value = thread.current_frame_mut().pop()?;
                let reference = value.as_reference()?;
                if reference != Reference::Null {
                    let obj_class = self.get_object_class(reference)?;
                    if !self.is_instance_of(&obj_class, &target)? {
                        return Err(VmError::ClassCastError {
                            from: obj_class,
                            to: target,
                        });
                    }
                }
                thread.current_frame_mut().push(value)?;
            }
            Opcode::Instanceof => {
                let index = thread.current_frame_mut().read_u16()? as usize;
                let target = thread
                    .current_frame()
                    .load_reference_class(index)?
                    .to_string();
                let reference = thread.current_frame_mut().pop()?.as_reference()?;
                let result = if reference == Reference::Null {
                    0
                } else {
                    let obj_class = self.get_object_class(reference)?;
                    if self.is_instance_of(&obj_class, &target)? {
                        1
                    } else {
                        0
                    }
                };
                thread.current_frame_mut().push(Value::Int(result))?;
            }

            // --- Control: null checks ---
            Opcode::Ifnull => {
                let offset = thread.current_frame_mut().read_i16()?;
                let reference = thread.current_frame_mut().pop()?.as_reference()?;
                if reference == Reference::Null {
                    thread
                        .current_frame_mut()
                        .branch(opcode_pc, offset.into())?;
                }
            }
            Opcode::Ifnonnull => {
                let offset = thread.current_frame_mut().read_i16()?;
                let reference = thread.current_frame_mut().pop()?.as_reference()?;
                if reference != Reference::Null {
                    thread
                        .current_frame_mut()
                        .branch(opcode_pc, offset.into())?;
                }
            }

            // --- Control: returns ---
            Opcode::Ireturn | Opcode::Freturn | Opcode::Dreturn => {
                return execute_ireturn_full(thread);
            }
            Opcode::Lreturn => {
                return execute_lreturn_full(thread);
            }
            Opcode::Areturn => {
                return execute_areturn_full(thread);
            }
            Opcode::Return => {
                return execute_return_full(thread);
            }

            Opcode::Arraylength => {
                let reference = thread.current_frame_mut().pop()?.as_reference()?;
                let length = self.heap.lock().unwrap().array_length(reference)?;
                thread.current_frame_mut().push(Value::Int(length as i32))?;
            }
        }
        Ok(None)
    }

    fn allocate_multi_array_descriptor(
        &mut self,
        descriptor: &str,
        counts: &[i32],
    ) -> Result<Reference, VmError> {
        if counts.is_empty() {
            return Err(VmError::InvalidDescriptor {
                descriptor: descriptor.to_string(),
            });
        }

        let count = counts[0];
        if count < 0 {
            return Err(VmError::NegativeArraySize { size: count });
        }
        let n = count as usize;

        let Some(component_descriptor) = descriptor.strip_prefix('[') else {
            return Err(VmError::InvalidDescriptor {
                descriptor: descriptor.to_string(),
            });
        };

        if counts.len() == 1 {
            return self.allocate_one_dimensional_array(descriptor, n);
        }

        let mut elements = Vec::with_capacity(n);
        for _ in 0..n {
            elements
                .push(self.allocate_multi_array_descriptor(component_descriptor, &counts[1..])?);
        }
        Ok(self
            .heap
            .lock()
            .unwrap()
            .allocate_reference_array(Self::array_component_name(component_descriptor), elements))
    }

    fn allocate_one_dimensional_array(
        &mut self,
        descriptor: &str,
        len: usize,
    ) -> Result<Reference, VmError> {
        let Some(component_descriptor) = descriptor.strip_prefix('[') else {
            return Err(VmError::InvalidDescriptor {
                descriptor: descriptor.to_string(),
            });
        };
        let reference = match component_descriptor.as_bytes().first() {
            Some(b'Z' | b'B' | b'C' | b'S' | b'I') => {
                self.heap.lock().unwrap().allocate_int_array(vec![0; len])
            }
            Some(b'J') => self.heap.lock().unwrap().allocate(HeapValue::LongArray {
                values: vec![0; len],
            }),
            Some(b'F') => self.heap.lock().unwrap().allocate(HeapValue::FloatArray {
                values: vec![0.0; len],
            }),
            Some(b'D') => self.heap.lock().unwrap().allocate(HeapValue::DoubleArray {
                values: vec![0.0; len],
            }),
            Some(b'L' | b'[') => self.heap.lock().unwrap().allocate_reference_array(
                Self::array_component_name(component_descriptor),
                vec![Reference::Null; len],
            ),
            _ => {
                return Err(VmError::InvalidDescriptor {
                    descriptor: descriptor.to_string(),
                });
            }
        };
        Ok(reference)
    }

    fn allocate_lambda_proxy(
        &mut self,
        site: &InvokeDynamicSite,
        target_class: &str,
        target_method: &str,
        target_descriptor: &str,
        captures: Vec<Value>,
    ) -> Result<Reference, VmError> {
        let class_name = format!("__lambda_proxy_{}", site.name);
        self.ensure_lambda_proxy_class(&class_name, &site.descriptor, captures.len())?;

        let class = self.get_class(&class_name)?;
        let field_count = class.field_offsets.len();
        let mut fields = vec![Value::Reference(Reference::Null); field_count];
        let mut set_field =
            |name: &str, value: Value| -> Result<(), VmError> {
                let offset = class.field_offsets.get(name).copied().ok_or_else(|| {
                    VmError::FieldNotFound {
                        class_name: class_name.clone(),
                        field_name: name.to_string(),
                    }
                })?;
                if offset >= fields.len() {
                    fields.resize(offset + 1, Value::Reference(Reference::Null));
                }
                fields[offset] = value;
                Ok(())
            };

        set_field(
            "__target_class",
            Value::Reference(
                self.heap
                    .lock()
                    .unwrap()
                    .allocate_string(target_class.to_string()),
            ),
        )?;
        set_field(
            "__target_method",
            Value::Reference(
                self.heap
                    .lock()
                    .unwrap()
                    .allocate_string(target_method.to_string()),
            ),
        )?;
        set_field(
            "__target_desc",
            Value::Reference(
                self.heap
                    .lock()
                    .unwrap()
                    .allocate_string(target_descriptor.to_string()),
            ),
        )?;
        for (i, cap) in captures.iter().enumerate() {
            set_field(&format!("__capture_{i}"), *cap)?;
        }

        Ok(self
            .heap
            .lock()
            .unwrap()
            .allocate(HeapValue::Object { class_name, fields }))
    }

    fn ensure_lambda_proxy_class(
        &mut self,
        class_name: &str,
        site_descriptor: &str,
        capture_count: usize,
    ) -> Result<(), VmError> {
        if self
            .runtime
            .lock()
            .unwrap()
            .classes
            .contains_key(class_name)
        {
            return Ok(());
        }

        let interfaces = Self::lambda_proxy_interfaces(site_descriptor)?;
        let mut instance_fields = vec![
            (
                "__target_class".to_string(),
                "Ljava/lang/String;".to_string(),
            ),
            (
                "__target_method".to_string(),
                "Ljava/lang/String;".to_string(),
            ),
            (
                "__target_desc".to_string(),
                "Ljava/lang/String;".to_string(),
            ),
        ];
        for i in 0..capture_count {
            instance_fields.push((format!("__capture_{i}"), "Ljava/lang/Object;".to_string()));
        }
        let field_offsets = instance_fields
            .iter()
            .enumerate()
            .map(|(i, (name, _))| (name.clone(), i))
            .collect();
        self.register_class(RuntimeClass {
            name: class_name.to_string(),
            super_class: Some("java/lang/Object".to_string()),
            methods: HashMap::new(),
            static_fields: HashMap::new(),
            instance_fields,
            field_offsets,
            interfaces,
        });
        Ok(())
    }

    fn lambda_proxy_interfaces(site_descriptor: &str) -> Result<Vec<String>, VmError> {
        let Some(end) = site_descriptor.find(')') else {
            return Err(VmError::InvalidDescriptor {
                descriptor: site_descriptor.to_string(),
            });
        };
        let return_descriptor = &site_descriptor[end + 1..];
        if return_descriptor.starts_with('L') && return_descriptor.ends_with(';') {
            return Ok(vec![
                return_descriptor[1..return_descriptor.len() - 1].to_string(),
            ]);
        }
        Ok(vec![])
    }

    fn array_component_name(component_descriptor: &str) -> String {
        if component_descriptor.starts_with('L') && component_descriptor.ends_with(';') {
            component_descriptor[1..component_descriptor.len() - 1].to_string()
        } else {
            component_descriptor.to_string()
        }
    }

    fn throw_new_exception(
        &mut self,
        thread: &mut Thread,
        class_name: &str,
    ) -> Result<(), VmError> {
        let reference = self.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: class_name.to_string(),
            fields: vec![],
        });
        self.throw_exception(thread, reference)
    }

    /// Propagate an exception through the call stack, searching for a matching handler.
    ///
    /// If a handler is found the current frame's stack is cleared, the exception
    /// reference is pushed, and `pc` jumps to the handler.  If no handler matches
    /// in any frame, an `UnhandledException` error is returned.
    fn throw_exception(
        &mut self,
        thread: &mut Thread,
        exception_ref: Reference,
    ) -> Result<(), VmError> {
        let exception_class = self.get_object_class(exception_ref)?;

        loop {
            let pc = thread.current_frame().pc.saturating_sub(1); // pc of the athrow / throwing opcode
            let handler = thread
                .current_frame()
                .exception_handlers
                .iter()
                .find(|h| {
                    if pc < h.start_pc as usize || pc >= h.end_pc as usize {
                        return false;
                    }
                    match &h.catch_class {
                        None => true, // finally / catch-all
                        Some(cls) => {
                            // Check class hierarchy (best-effort: ignore load errors)
                            self.is_instance_of(&exception_class, cls).unwrap_or(false)
                        }
                    }
                })
                .cloned();

            if let Some(h) = handler {
                let frame = thread.current_frame_mut();
                frame.stack.clear();
                frame.push(Value::Reference(exception_ref))?;
                frame.pc = h.handler_pc as usize;
                return Ok(());
            }

            // No handler in this frame — pop and try the caller.
            if thread.depth() == 1 {
                return Err(VmError::UnhandledException {
                    class_name: exception_class,
                });
            }
            thread.pop_frame();
        }
    }

    /// Resolve a method by walking the class hierarchy from `start_class` upward.
    ///
    /// If no match is found along the super-class chain, fall back to searching
    /// every interface implemented (directly or transitively) by any visited
    /// class. This lets `invokeinterface` / `invokevirtual` pick up `default`
    /// interface methods.
    ///
    /// Returns `(resolved_class_name, class_method)`.
    fn resolve_method(
        &mut self,
        start_class: &str,
        method_name: &str,
        descriptor: &str,
    ) -> Result<(String, ClassMethod), VmError> {
        let mut visited_interfaces: Vec<String> = Vec::new();
        let mut current = start_class.to_string();
        loop {
            self.ensure_class_loaded(&current)?;
            let class = self.get_class(&current)?;
            if let Some(m) = class
                .methods
                .get(&(method_name.to_string(), descriptor.to_string()))
            {
                return Ok((current, m.clone()));
            }
            for iface in &class.interfaces {
                if !visited_interfaces.contains(iface) {
                    visited_interfaces.push(iface.clone());
                }
            }
            match &class.super_class {
                Some(parent) => current = parent.clone(),
                None => break,
            }
        }

        // Expand with transitively-extended interfaces, then look for the method.
        let mut i = 0;
        while i < visited_interfaces.len() {
            let iface = visited_interfaces[i].clone();
            i += 1;
            if self.ensure_class_loaded(&iface).is_err() {
                continue;
            }
            let class = match self.get_class(&iface) {
                Ok(c) => c,
                Err(_) => continue,
            };
            if let Some(m) = class
                .methods
                .get(&(method_name.to_string(), descriptor.to_string()))
            {
                return Ok((iface, m.clone()));
            }
            for parent in &class.interfaces {
                if !visited_interfaces.contains(parent) {
                    visited_interfaces.push(parent.clone());
                }
            }
        }

        Err(VmError::MethodNotFound {
            class_name: start_class.to_string(),
            method_name: method_name.to_string(),
            descriptor: descriptor.to_string(),
        })
    }

    /// Check whether `class_name` is the same as, or a sub-class of, `target`.
    fn is_instance_of(&mut self, class_name: &str, target: &str) -> Result<bool, VmError> {
        // BFS over super-classes and all directly/transitively implemented interfaces.
        let mut queue: Vec<String> = vec![class_name.to_string()];
        let mut seen: Vec<String> = Vec::new();
        while let Some(current) = queue.pop() {
            if current == target {
                return Ok(true);
            }
            if seen.contains(&current) {
                continue;
            }
            seen.push(current.clone());
            if self.ensure_class_loaded(&current).is_err() {
                continue;
            }
            let class = match self.get_class(&current) {
                Ok(c) => c,
                Err(_) => continue,
            };
            if let Some(parent) = &class.super_class {
                queue.push(parent.clone());
            }
            for iface in &class.interfaces {
                queue.push(iface.clone());
            }
        }
        Ok(false)
    }

    /// Shared dispatch logic for `invokevirtual` and `invokespecial`.
    fn dispatch_instance_method(
        &mut self,
        thread: &mut Thread,
        class_name: &str,
        method_ref: &MethodRef,
        receiver: Reference,
        args: Vec<Value>,
    ) -> Result<(), VmError> {
        // Lambda proxy dispatch: redirect to the captured target method.
        if class_name.starts_with("__lambda_proxy_")
            && method_ref.method_name == class_name.trim_start_matches("__lambda_proxy_")
        {
            return self.dispatch_lambda_proxy(thread, receiver, args);
        }

        // Class-wide native shadows (e.g., every method on Unsafe) skip
        // method-table lookup so we don't have to enumerate every JDK
        // method name up front.
        if self.has_native_override(class_name, &method_ref.method_name, &method_ref.descriptor) {
            let mut all_args = vec![Value::Reference(receiver)];
            all_args.extend(args);
            let result = self.invoke_native(
                class_name,
                &method_ref.method_name,
                &method_ref.descriptor,
                &all_args,
            )?;
            if let Some(value) = result {
                thread.current_frame_mut().push(value)?;
            }
            return Ok(());
        }

        let resolved =
            self.resolve_method(class_name, &method_ref.method_name, &method_ref.descriptor);

        match resolved {
            Ok((resolved_class, class_method)) => match class_method {
                ClassMethod::Native => {
                    let mut all_args = vec![Value::Reference(receiver)];
                    all_args.extend(args);
                    let result = self.invoke_native(
                        &resolved_class,
                        &method_ref.method_name,
                        &method_ref.descriptor,
                        &all_args,
                    )?;
                    if let Some(value) = result {
                        thread.current_frame_mut().push(value)?;
                    }
                }
                ClassMethod::Bytecode(method) => {
                    let mut all_args = vec![Value::Reference(receiver)];
                    all_args.extend(args);
                    let callee = method.with_initial_locals(Vm::args_to_locals(all_args));
                    thread.push_frame(Frame::new(callee));
                }
            },
            Err(VmError::MethodNotFound { .. }) => {
                // No method table entry — fall back to the builtin native
                // dispatcher for the receiver class (Lookup, CallSite, etc.).
                let mut all_args = vec![Value::Reference(receiver)];
                all_args.extend(args);
                match self.invoke_native(
                    class_name,
                    &method_ref.method_name,
                    &method_ref.descriptor,
                    &all_args,
                ) {
                    Ok(result) => {
                        if let Some(value) = result {
                            thread.current_frame_mut().push(value)?;
                        }
                    }
                    Err(_) => {
                        return Err(VmError::MethodNotFound {
                            class_name: class_name.to_string(),
                            method_name: method_ref.method_name.clone(),
                            descriptor: method_ref.descriptor.clone(),
                        });
                    }
                }
            }
            Err(e) => return Err(e),
        }
        Ok(())
    }

    /// Dispatch a call on a lambda proxy object to its captured target method.
    fn dispatch_lambda_proxy(
        &mut self,
        thread: &mut Thread,
        receiver: Reference,
        args: Vec<Value>,
    ) -> Result<(), VmError> {
        let (target_class, target_method, target_desc, captures) = {
            let class_name = self.get_object_class(receiver)?;
            let class = self.get_class(&class_name)?;
            let fields = match self.heap.lock().unwrap().get(receiver)? {
                HeapValue::Object { fields, .. } => fields.clone(),
                _ => return Err(VmError::NullReference),
            };

            let get_str = |key: &str| -> Result<std::string::String, VmError> {
                let Some(offset) = class.field_offsets.get(key).copied() else {
                    return Ok(std::string::String::new());
                };
                match fields.get(offset) {
                    Some(Value::Reference(r)) => self.stringify_reference(*r),
                    _ => Ok(std::string::String::new()),
                }
            };

            let tc = get_str("__target_class")?;
            let tm = get_str("__target_method")?;
            let td = get_str("__target_desc")?;

            let mut captures = Vec::new();
            let mut i = 0;
            while let Some(offset) = class.field_offsets.get(&format!("__capture_{i}")).copied() {
                let Some(Value::Reference(r)) = fields.get(offset) else {
                    break;
                };
                captures.push(*r);
                i += 1;
            }

            (tc, tm, td, captures)
        };

        let mut all_args: Vec<Value> = captures.into_iter().map(Value::Reference).collect();
        all_args.extend(args);

        self.ensure_class_loaded(&target_class)?;

        let (_, class_method) = self.resolve_method(&target_class, &target_method, &target_desc)?;

        match class_method {
            ClassMethod::Native => {
                let result =
                    self.invoke_native(&target_class, &target_method, &target_desc, &all_args)?;
                if let Some(value) = result {
                    thread.current_frame_mut().push(value)?;
                }
            }
            ClassMethod::Bytecode(method) => {
                let callee = method.with_initial_locals(Vm::args_to_locals(all_args));
                thread.push_frame(Frame::new(callee));
            }
        }
        Ok(())
    }

    /// Resolve a heap string reference to its Rust `String` value.
    pub(super) fn stringify_reference(&self, reference: Reference) -> Result<String, VmError> {
        match reference {
            Reference::Null => Ok("null".to_string()),
            _ => match self.heap.lock().unwrap().get(reference)? {
                HeapValue::String(value) => Ok(value.clone()),
                value => Err(VmError::InvalidHeapValue {
                    expected: "string",
                    actual: value.kind_name(),
                }),
            },
        }
    }
}

/// Count the number of arguments in a JVM method descriptor.
///
/// Parses the parameter section of a descriptor like `(ILjava/lang/String;)V`
/// and returns the number of parameters (2 in that example).
/// Return the JVM default zero-value for a field descriptor.

mod var_handle_ops;
pub(crate) use var_handle_ops::{
    VarHandleAccess, apply_var_handle_array_op, apply_var_handle_op,
    classify_var_handle_access,
};

#[cfg(test)]
mod tests;
