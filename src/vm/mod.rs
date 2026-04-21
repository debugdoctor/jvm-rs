mod builtin;
mod classloader;
mod frame;
mod heap;
mod thread;
mod types;
pub mod verify;

pub use crate::classfile::ClassFile;
pub use heap::GcStats;
pub use thread::JvmThread;
pub use types::{
    ClassMethod, ExceptionHandler, ExecutionResult, FieldRef, InvokeDynamicKind, InvokeDynamicSite,
    Method, MethodRef, Reference, RuntimeClass, Value, VmError,
};
use frame::Frame;
use heap::{Heap, HeapValue};
use thread::{
    ClassInitializationState, JavaThreadState, RuntimeState, SharedMonitors,
    SharedThreads, Thread,
};
use types::{default_value_for_descriptor, format_vm_float, parse_arg_count, parse_arg_types};

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::bytecode::Opcode;
use classloader::{ClassLoader, LazyClassLoader, BootstrapClassLoader};

static NEXT_THREAD_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub struct Vm {
    heap: Arc<Mutex<Heap>>,
    runtime: Arc<Mutex<RuntimeState>>,
    /// Object monitors keyed by heap index.
    monitors: Arc<SharedMonitors>,
    threads: Arc<SharedThreads>,
    class_path: Vec<PathBuf>,
    class_loader: Option<LazyClassLoader<BootstrapClassLoader>>,
    trace: bool,
    thread_id: u64,
    output: Arc<Mutex<Vec<String>>>,
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
            thread_id: NEXT_THREAD_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            output: self.output.clone(),
        }
    }
}

impl Vm {
    pub fn new() -> Self {
        let mut vm = Self {
            heap: Arc::new(Mutex::new(Heap::default())),
            runtime: Arc::new(Mutex::new(RuntimeState::default())),
            monitors: Arc::new(SharedMonitors::default()),
            threads: Arc::new(SharedThreads::default()),
            class_path: Vec::new(),
            class_loader: Some(classloader::create_bootstrap_loader()),
            trace: false,
            thread_id: NEXT_THREAD_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            output: Arc::new(Mutex::new(Vec::new())),
        };
        vm.bootstrap();
        vm
    }

    /// Enable or disable execution tracing (prints pc, opcode, stack to stderr).
    /// Spawn a new thread that executes the given method.
    ///
    /// The new thread shares heap/monitor/output state with the parent VM,
    /// while method-local execution state remains isolated per thread.
    pub fn spawn(&self, method: Method) -> JvmThread {
        let mut child_vm = self.clone();
        child_vm.thread_id =
            NEXT_THREAD_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
        args: Vec<Value>,
    ) -> Result<JvmThread, VmError> {
        let mut child_vm = self.clone();
        child_vm.thread_id =
            NEXT_THREAD_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let start_class = start_class.to_string();
        let method_name = method_name.to_string();
        let descriptor = descriptor.to_string();

        let handle = std::thread::spawn(move || {
            let (resolved_class, class_method) =
                child_vm.resolve_method(&start_class, &method_name, &descriptor)?;
            match class_method {
                ClassMethod::Native => {
                    let result =
                        child_vm.invoke_native(&resolved_class, &method_name, &descriptor, &args)?;
                    Ok(result.map_or(ExecutionResult::Void, ExecutionResult::Value))
                }
                ClassMethod::Bytecode(method) => {
                    let callee = method.with_initial_locals(args.into_iter().map(Some).collect::<Vec<_>>());
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
            // Constants may hold string references.
            for constant in &frame.constants {
                if let Some(Value::Reference(r @ Reference::Heap(_))) = constant {
                    roots.push(*r);
                }
            }
        }

        // Roots from static fields of all loaded classes.
        let runtime = self.runtime.lock().unwrap();
        for class in runtime.classes.values() {
            for value in class.static_fields.values() {
                if let Value::Reference(r @ Reference::Heap(_)) = value {
                    roots.push(*r);
                }
            }
        }

        self.heap.lock().unwrap().gc(&roots);
    }

    pub fn set_trace(&mut self, enabled: bool) {
        self.trace = enabled;
    }

    /// Set the number of allocations between automatic GC passes. Use
    /// [`Self::disable_gc`] to switch automatic collection off entirely.
    pub fn set_gc_threshold(&mut self, allocations: usize) {
        self.heap.lock().unwrap().gc_threshold = allocations.max(1);
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
            frames: Vec::new(),
        };
        self.collect_garbage(&thread);
    }

    /// Snapshot current GC counters.
    pub fn gc_stats(&self) -> GcStats {
        self.heap.lock().unwrap().stats
    }

    /// Set the classpath entries used for on-demand class loading.
    pub fn set_class_path(&mut self, paths: Vec<PathBuf>) {
        self.class_path = paths;
    }

    /// Register a class loaded from a `.class` file.
    pub fn register_class(&mut self, class: RuntimeClass) {
        if class.name == "java/util/Objects" {
            eprintln!("DEBUG register_class: java/util/Objects being registered with {} methods", class.methods.len());
            for (k, v) in &class.methods {
                eprintln!("DEBUG register_class:   method: {:?} => {:?}", k, v);
            }
        }
        self.runtime
            .lock()
            .unwrap()
            .classes
            .insert(class.name.clone(), class);
    }

    /// Register a class from a parsed `ClassFile`, extracting all runtime
    /// metadata (constant pool entries, method/field refs, exception handlers,
    /// line numbers, stack map frames, invoke dynamic sites).
    pub fn register_classfile(&mut self, class_name: &str, class_file: &ClassFile) {
        eprintln!("DEBUG register_classfile: called for {}", class_name);
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
            if class_name == "java/util/Objects" {
                eprintln!("DEBUG ensure_class_loaded: {} already in classes map!", class_name);
            }
            return Ok(());
        }

        if let Some(ref mut loader) = self.class_loader {
            if let Ok(Some(class_file)) = ClassLoader::load_classfile(loader, class_name) {
                eprintln!("DEBUG ensure_class_loaded: {} from bootstrap loader, registering", class_name);
                self.register_classfile(class_name, &class_file);
                return Ok(());
            }
        }

        if !self.class_path.is_empty() {
            eprintln!("DEBUG ensure_class_loaded: {} from classpath", class_name);
            let class_path = self.class_path.clone();
            let source = crate::launcher::resolve_class_path(&class_path, class_name).ok_or_else(
                || VmError::ClassNotFound {
                    class_name: class_name.to_string(),
                },
            )?;
            crate::launcher::load_and_register_class_from(&source, class_name, self).map_err(|_| {
                VmError::ClassNotFound {
                    class_name: class_name.to_string(),
                }
            })
        } else {
            Err(VmError::ClassNotFound {
                class_name: class_name.to_string(),
            })
        }
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

    fn get_static_field(&self, class_name: &str, field_name: &str) -> Result<Value, VmError> {
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

    fn get_object_field(&self, reference: Reference, field_name: &str) -> Result<Value, VmError> {
        let heap = self.heap.lock().unwrap();
        match heap.get(reference)? {
            HeapValue::Object { fields, .. } => {
                Ok(*fields.get(field_name).unwrap_or(&Value::Reference(Reference::Null)))
            }
            value => Err(VmError::InvalidHeapValue {
                expected: "object",
                actual: value.kind_name(),
            }),
        }
    }

    fn set_object_field(
        &mut self,
        reference: Reference,
        field_name: &str,
        value: Value,
    ) -> Result<(), VmError> {
        let mut heap = self.heap.lock().unwrap();
        match heap.get_mut(reference)? {
            HeapValue::Object { fields, .. } => {
                fields.insert(field_name.to_string(), value);
                Ok(())
            }
            value => Err(VmError::InvalidHeapValue {
                expected: "object",
                actual: value.kind_name(),
            }),
        }
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

        let handle = self.spawn_invocation(start_class, method_name, descriptor, args)?;
        self.threads.states.lock().unwrap().insert(
            index,
            JavaThreadState {
                started: true,
                handle: Some(handle),
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
                        "java/lang/Integer" => match fields.get("value") {
                            Some(Value::Int(i)) => i.to_string(),
                            _ => "0".to_string(),
                        },
                        "java/lang/Long" => match fields.get("value") {
                            Some(Value::Long(i)) => i.to_string(),
                            _ => "0".to_string(),
                        },
                        "java/lang/Boolean" => match fields.get("value") {
                            Some(Value::Int(i)) if *i != 0 => "true".to_string(),
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
            b'Z' => Ok(if value.as_int()? != 0 { "true" } else { "false" }.to_string()),
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
                                descriptor: format!("missing invokedynamic concat arg at {arg_index}"),
                            }
                        })?;
                        result.push_str(
                            &self.stringify_concat_arg(type_for(arg_index), value)?,
                        );
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
            monitor.waiting_threads.saturating_sub(monitor.pending_notifies)
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

    pub fn new_string(&mut self, value: impl Into<String>) -> Value {
        Value::Reference(self.heap.lock().unwrap().allocate_string(value))
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
            HeapValue::ReferenceArray { component_type, .. } => {
                Ok(format!("[L{component_type};"))
            }
        }
    }

    /// Verify a method's bytecode structure before execution.
    pub fn verify_method(method: &Method) -> Result<(), VmError> {
        verify::verify_method(method)
    }

    pub fn execute(&mut self, method: Method) -> Result<ExecutionResult, VmError> {
        let mut thread = Thread::new(method);

        loop {
            // Trigger GC when allocation pressure crosses the configured threshold.
            if self.heap.lock().unwrap().should_collect() {
                self.collect_garbage(&thread);
            }

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
                let stack_repr: Vec<_> = thread
                    .current_frame()
                    .stack
                    .iter()
                    .map(|v| format!("{v}"))
                    .collect();
                eprintln!(
                    "  pc={opcode_pc:<4} {opcode:?}  stack=[{}]  depth={}",
                    stack_repr.join(", "),
                    thread.depth(),
                );
            }

            match self.execute_opcode(&mut thread, opcode, opcode_pc) {
                Ok(Some(result)) => return Ok(result),
                Ok(None) => {}
                Err(VmError::NullReference) => {
                    self.throw_new_exception(
                        &mut thread,
                        "java/lang/NullPointerException",
                    )?;
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
                    self.throw_new_exception(
                        &mut thread,
                        "java/lang/ClassCastException",
                    )?;
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
                Opcode::AconstNull => thread
                    .current_frame_mut()
                    .push(Value::Reference(Reference::Null))?,
                Opcode::IconstM1 => thread.current_frame_mut().push(Value::Int(-1))?,
                Opcode::Iconst0 => thread.current_frame_mut().push(Value::Int(0))?,
                Opcode::Iconst1 => thread.current_frame_mut().push(Value::Int(1))?,
                Opcode::Iconst2 => thread.current_frame_mut().push(Value::Int(2))?,
                Opcode::Iconst3 => thread.current_frame_mut().push(Value::Int(3))?,
                Opcode::Iconst4 => thread.current_frame_mut().push(Value::Int(4))?,
                Opcode::Iconst5 => thread.current_frame_mut().push(Value::Int(5))?,
                Opcode::Bipush => {
                    let value = thread.current_frame_mut().read_u8()? as i8 as i32;
                    thread.current_frame_mut().push(Value::Int(value))?;
                }
                Opcode::Sipush => {
                    let value = thread.current_frame_mut().read_i16()? as i32;
                    thread.current_frame_mut().push(Value::Int(value))?;
                }
                Opcode::Ldc => {
                    let index = thread.current_frame_mut().read_u8()? as usize;
                    let value = thread.current_frame().load_constant(index)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::LdcW => {
                    let index = thread.current_frame_mut().read_u16()? as usize;
                    let value = thread.current_frame().load_constant(index)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Ldc2W => {
                    let index = thread.current_frame_mut().read_u16()? as usize;
                    let value = thread.current_frame().load_constant(index)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Lconst0 => thread.current_frame_mut().push(Value::Long(0))?,
                Opcode::Lconst1 => thread.current_frame_mut().push(Value::Long(1))?,
                Opcode::Fconst0 => thread.current_frame_mut().push(Value::Float(0.0))?,
                Opcode::Fconst1 => thread.current_frame_mut().push(Value::Float(1.0))?,
                Opcode::Fconst2 => thread.current_frame_mut().push(Value::Float(2.0))?,
                Opcode::Dconst0 => thread.current_frame_mut().push(Value::Double(0.0))?,
                Opcode::Dconst1 => thread.current_frame_mut().push(Value::Double(1.0))?,
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
                        6 => self
                            .heap
                            .lock()
                            .unwrap()
                            .allocate(HeapValue::FloatArray { values: vec![0.0; n] }),
                        7 => self
                            .heap
                            .lock()
                            .unwrap()
                            .allocate(HeapValue::DoubleArray { values: vec![0.0; n] }),
                        11 => self
                            .heap
                            .lock()
                            .unwrap()
                            .allocate(HeapValue::LongArray { values: vec![0; n] }),
                        _ => return Err(VmError::UnsupportedNewArrayType { atype }),
                    };
                    thread.current_frame_mut().push(Value::Reference(reference))?;
                }
                Opcode::Anewarray => {
                    let index = thread.current_frame_mut().read_u16()? as usize;
                    let component_type =
                        thread.current_frame().load_reference_class(index)?.to_string();
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
                    thread.current_frame_mut().push(Value::Reference(reference))?;
                }
                Opcode::Aload => {
                    let index = thread.current_frame_mut().read_u8()? as usize;
                    let value = thread.current_frame().load_local(index)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Iload | Opcode::Lload | Opcode::Fload | Opcode::Dload => {
                    let index = thread.current_frame_mut().read_u8()? as usize;
                    let value = thread.current_frame().load_local(index)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Iload0 | Opcode::Lload0 | Opcode::Fload0 | Opcode::Dload0 => {
                    let value = thread.current_frame().load_local(0)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Iload1 | Opcode::Lload1 | Opcode::Fload1 | Opcode::Dload1 => {
                    let value = thread.current_frame().load_local(1)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Iload2 | Opcode::Lload2 | Opcode::Fload2 | Opcode::Dload2 => {
                    let value = thread.current_frame().load_local(2)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Iload3 | Opcode::Lload3 | Opcode::Fload3 | Opcode::Dload3 => {
                    let value = thread.current_frame().load_local(3)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Aload0 => {
                    let value = thread.current_frame().load_local(0)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Aload1 => {
                    let value = thread.current_frame().load_local(1)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Aload2 => {
                    let value = thread.current_frame().load_local(2)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Aload3 => {
                    let value = thread.current_frame().load_local(3)?;
                    thread.current_frame_mut().push(value)?;
                }
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
                    thread.current_frame_mut().push(Value::Reference(reference))?;
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
                    let value = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().store_local(index, value)?;
                }
                Opcode::Istore | Opcode::Lstore | Opcode::Fstore | Opcode::Dstore => {
                    let index = thread.current_frame_mut().read_u8()? as usize;
                    let value = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().store_local(index, value)?;
                }
                Opcode::Istore0 | Opcode::Lstore0 | Opcode::Fstore0 | Opcode::Dstore0 => {
                    let value = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().store_local(0, value)?;
                }
                Opcode::Istore1 | Opcode::Lstore1 | Opcode::Fstore1 | Opcode::Dstore1 => {
                    let value = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().store_local(1, value)?;
                }
                Opcode::Istore2 | Opcode::Lstore2 | Opcode::Fstore2 | Opcode::Dstore2 => {
                    let value = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().store_local(2, value)?;
                }
                Opcode::Istore3 | Opcode::Lstore3 | Opcode::Fstore3 | Opcode::Dstore3 => {
                    let value = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().store_local(3, value)?;
                }
                Opcode::Astore0 => {
                    let value = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().store_local(0, value)?;
                }
                Opcode::Astore1 => {
                    let value = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().store_local(1, value)?;
                }
                Opcode::Astore2 => {
                    let value = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().store_local(2, value)?;
                }
                Opcode::Astore3 => {
                    let value = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().store_local(3, value)?;
                }
                Opcode::Iastore => {
                    let value = thread.current_frame_mut().pop()?.as_int()?;
                    let index = thread.current_frame_mut().pop()?.as_int()?;
                    let array_ref = thread.current_frame_mut().pop()?.as_reference()?;
                    self.heap
                        .lock()
                        .unwrap()
                        .store_int_array_element(array_ref, index, value)?;
                }
                Opcode::Pop => {
                    let _ = thread.current_frame_mut().pop()?;
                }
                Opcode::Pop2 => {
                    let _ = thread.current_frame_mut().pop()?;
                    let _ = thread.current_frame_mut().pop()?;
                }
                Opcode::Dup => {
                    let value = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().push(value)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::DupX1 => {
                    let top = thread.current_frame_mut().pop()?;
                    let below = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().push(top)?;
                    thread.current_frame_mut().push(below)?;
                    thread.current_frame_mut().push(top)?;
                }
                Opcode::Dup2 => {
                    let top = thread.current_frame_mut().pop()?;
                    let below = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().push(below)?;
                    thread.current_frame_mut().push(top)?;
                    thread.current_frame_mut().push(below)?;
                    thread.current_frame_mut().push(top)?;
                }
                Opcode::DupX2 => {
                    let v1 = thread.current_frame_mut().pop()?;
                    let v2 = thread.current_frame_mut().pop()?;
                    let v3 = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().push(v1)?;
                    thread.current_frame_mut().push(v3)?;
                    thread.current_frame_mut().push(v2)?;
                    thread.current_frame_mut().push(v1)?;
                }
                Opcode::Dup2X1 => {
                    let v1 = thread.current_frame_mut().pop()?;
                    let v2 = thread.current_frame_mut().pop()?;
                    let v3 = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().push(v2)?;
                    thread.current_frame_mut().push(v1)?;
                    thread.current_frame_mut().push(v3)?;
                    thread.current_frame_mut().push(v2)?;
                    thread.current_frame_mut().push(v1)?;
                }
                Opcode::Dup2X2 => {
                    let v1 = thread.current_frame_mut().pop()?;
                    let v2 = thread.current_frame_mut().pop()?;
                    let v3 = thread.current_frame_mut().pop()?;
                    let v4 = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().push(v2)?;
                    thread.current_frame_mut().push(v1)?;
                    thread.current_frame_mut().push(v4)?;
                    thread.current_frame_mut().push(v3)?;
                    thread.current_frame_mut().push(v2)?;
                    thread.current_frame_mut().push(v1)?;
                }
                Opcode::Swap => {
                    let top = thread.current_frame_mut().pop()?;
                    let below = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().push(top)?;
                    thread.current_frame_mut().push(below)?;
                }
                Opcode::Iadd => {
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    thread.current_frame_mut().push(Value::Int(lhs + rhs))?;
                }
                Opcode::Isub => {
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    thread.current_frame_mut().push(Value::Int(lhs - rhs))?;
                }
                Opcode::Imul => {
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    thread.current_frame_mut().push(Value::Int(lhs * rhs))?;
                }
                Opcode::Idiv => {
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    if rhs == 0 {
                        self.throw_new_exception(
                            &mut thread,
                            "java/lang/ArithmeticException",
                        )?;
                        return Ok(None);
                    }
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    thread.current_frame_mut().push(Value::Int(lhs / rhs))?;
                }
                Opcode::Irem => {
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    if rhs == 0 {
                        self.throw_new_exception(
                            &mut thread,
                            "java/lang/ArithmeticException",
                        )?;
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
                    thread.current_frame_mut().push(Value::Long(lhs.wrapping_add(rhs)))?;
                }
                Opcode::Lsub => {
                    let rhs = thread.current_frame_mut().pop()?.as_long()?;
                    let lhs = thread.current_frame_mut().pop()?.as_long()?;
                    thread.current_frame_mut().push(Value::Long(lhs.wrapping_sub(rhs)))?;
                }
                Opcode::Lmul => {
                    let rhs = thread.current_frame_mut().pop()?.as_long()?;
                    let lhs = thread.current_frame_mut().pop()?.as_long()?;
                    thread.current_frame_mut().push(Value::Long(lhs.wrapping_mul(rhs)))?;
                }
                Opcode::Ldiv => {
                    let rhs = thread.current_frame_mut().pop()?.as_long()?;
                    if rhs == 0 {
                        self.throw_new_exception(
                            &mut thread,
                            "java/lang/ArithmeticException",
                        )?;
                        return Ok(None);
                    }
                    let lhs = thread.current_frame_mut().pop()?.as_long()?;
                    thread.current_frame_mut().push(Value::Long(lhs / rhs))?;
                }
                Opcode::Lrem => {
                    let rhs = thread.current_frame_mut().pop()?.as_long()?;
                    if rhs == 0 {
                        self.throw_new_exception(
                            &mut thread,
                            "java/lang/ArithmeticException",
                        )?;
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
                    thread.current_frame_mut().push(Value::Long(((lhs as u64) >> rhs) as i64))?;
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
                    let result = if lhs > rhs { 1 } else if lhs == rhs { 0 } else { -1 };
                    thread.current_frame_mut().push(Value::Int(result))?;
                }
                Opcode::Fcmpl => {
                    let rhs = thread.current_frame_mut().pop()?.as_float()?;
                    let lhs = thread.current_frame_mut().pop()?.as_float()?;
                    let result = if lhs > rhs { 1 } else if lhs == rhs { 0 } else { -1 };
                    thread.current_frame_mut().push(Value::Int(result))?;
                }
                Opcode::Fcmpg => {
                    let rhs = thread.current_frame_mut().pop()?.as_float()?;
                    let lhs = thread.current_frame_mut().pop()?.as_float()?;
                    let result = if lhs < rhs { -1 } else if lhs == rhs { 0 } else { 1 };
                    thread.current_frame_mut().push(Value::Int(result))?;
                }
                Opcode::Dcmpl => {
                    let rhs = thread.current_frame_mut().pop()?.as_double()?;
                    let lhs = thread.current_frame_mut().pop()?.as_double()?;
                    let result = if lhs > rhs { 1 } else if lhs == rhs { 0 } else { -1 };
                    thread.current_frame_mut().push(Value::Int(result))?;
                }
                Opcode::Dcmpg => {
                    let rhs = thread.current_frame_mut().pop()?.as_double()?;
                    let lhs = thread.current_frame_mut().pop()?.as_double()?;
                    let result = if lhs < rhs { -1 } else if lhs == rhs { 0 } else { 1 };
                    thread.current_frame_mut().push(Value::Int(result))?;
                }
                Opcode::Ifeq => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let value = thread.current_frame_mut().pop()?.as_int()?;
                    if value == 0 {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::Ifne => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let value = thread.current_frame_mut().pop()?.as_int()?;
                    if value != 0 {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::Iflt => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let value = thread.current_frame_mut().pop()?.as_int()?;
                    if value < 0 {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::Ifge => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let value = thread.current_frame_mut().pop()?.as_int()?;
                    if value >= 0 {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::Ifgt => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let value = thread.current_frame_mut().pop()?.as_int()?;
                    if value > 0 {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::Ifle => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let value = thread.current_frame_mut().pop()?.as_int()?;
                    if value <= 0 {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::IfIcmpeq => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    if lhs == rhs {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::IfIcmpne => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    if lhs != rhs {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::IfIcmplt => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    if lhs < rhs {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::IfIcmpge => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    if lhs >= rhs {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::IfIcmpgt => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    if lhs > rhs {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::IfIcmple => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    if lhs <= rhs {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::IfAcmpeq => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let rhs = thread.current_frame_mut().pop()?.as_reference()?;
                    let lhs = thread.current_frame_mut().pop()?.as_reference()?;
                    if lhs == rhs {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::IfAcmpne => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let rhs = thread.current_frame_mut().pop()?.as_reference()?;
                    let lhs = thread.current_frame_mut().pop()?.as_reference()?;
                    if lhs != rhs {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
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
                    thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                }
                Opcode::Jsr => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let return_pc = thread.current_frame().pc;
                    thread
                        .current_frame_mut()
                        .push(Value::ReturnAddress(return_pc))?;
                    thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                }
                Opcode::Ret => {
                    let index = thread.current_frame_mut().read_u8()? as usize;
                    let target = thread.current_frame().load_local(index)?.as_return_address()?;
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
                    let value =
                        self.get_static_field(&field_ref.class_name, &field_ref.field_name)?;
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
                    let value = match self.heap.lock().unwrap().get(object_ref)? {
                        HeapValue::Object { fields, .. } => fields
                            .get(&field_ref.field_name)
                            .copied()
                            .ok_or_else(|| VmError::FieldNotFound {
                                class_name: field_ref.class_name.clone(),
                                field_name: field_ref.field_name.clone(),
                            })?,
                        value => {
                            return Err(VmError::InvalidHeapValue {
                                expected: "object",
                                actual: value.kind_name(),
                            })
                        }
                    };
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Putfield => {
                    let index = thread.current_frame_mut().read_u16()? as usize;
                    let field_ref = thread.current_frame().load_field_ref(index)?.clone();
                    let value = thread.current_frame_mut().pop()?;
                    let object_ref = thread.current_frame_mut().pop()?.as_reference()?;
                    match self.heap.lock().unwrap().get_mut(object_ref)? {
                        HeapValue::Object { fields, .. } => {
                            fields.insert(field_ref.field_name, value);
                        }
                        value => {
                            return Err(VmError::InvalidHeapValue {
                                expected: "object",
                                actual: value.kind_name(),
                            })
                        }
                    };
                }

                // --- References: method invocation ---

                Opcode::Invokevirtual => {
                    let index = thread.current_frame_mut().read_u16()? as usize;
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
                Opcode::Invokespecial => {
                    let index = thread.current_frame_mut().read_u16()? as usize;
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
                    let class = self.get_class(class_name)?;
                    if class_name == "java/util/Objects" {
                        eprintln!("DEBUG Objects methods: {:?}", class.methods.keys().collect::<Vec<_>>());
                    }
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
                            let initial_locals: Vec<Option<Value>> =
                                args.into_iter().map(Some).collect();
                            let callee = method.with_initial_locals(initial_locals);
                            thread.push_frame(Frame::new(callee));
                        }
                    }
                }

                Opcode::Invokeinterface => {
                    let index = thread.current_frame_mut().read_u16()? as usize;
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
                            // LambdaMetafactory: create a lambda proxy object that stores the
                            // target method reference and any captured arguments.
                            let mut fields = HashMap::new();
                            fields.insert(
                                "__target_class".to_string(),
                                Value::Reference(
                                    self.heap
                                        .lock()
                                        .unwrap()
                                        .allocate_string(target_class.clone()),
                                ),
                            );
                            fields.insert(
                                "__target_method".to_string(),
                                Value::Reference(
                                    self.heap
                                        .lock()
                                        .unwrap()
                                        .allocate_string(target_method.clone()),
                                ),
                            );
                            fields.insert(
                                "__target_desc".to_string(),
                                Value::Reference(
                                    self.heap
                                        .lock()
                                        .unwrap()
                                        .allocate_string(target_descriptor.clone()),
                                ),
                            );
                            for (i, val) in args.into_iter().enumerate() {
                                fields.insert(format!("__capture_{i}"), val);
                            }

                            let proxy = self.heap.lock().unwrap().allocate(HeapValue::Object {
                                class_name: format!("__lambda_proxy_{}", site.name),
                                fields,
                            });
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
                    let class_name =
                        thread.current_frame().load_reference_class(index)?.to_string();
                    self.ensure_class_loaded(&class_name)?;
                    self.ensure_class_initialized(&class_name)?;
                    let mut all_instance_fields = Vec::new();
                    let mut current_class = class_name.clone();
                    loop {
                        self.ensure_class_loaded(&current_class)?;
                        let class = self.get_class(&current_class)?;
                        for (name, desc) in &class.instance_fields {
                            if !all_instance_fields.iter().any(|(n, _)| n == name) {
                                all_instance_fields.push((name.clone(), desc.clone()));
                            }
                        }
                        match &class.super_class {
                            Some(parent) => current_class = parent.clone(),
                            None => break,
                        }
                    }
                    let mut fields = HashMap::new();
                    for (name, descriptor) in all_instance_fields {
                        fields.insert(name, default_value_for_descriptor(&descriptor));
                    }
                    let reference = self.heap.lock().unwrap().allocate(HeapValue::Object {
                        class_name,
                        fields,
                    });
                    thread.current_frame_mut().push(Value::Reference(reference))?;
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
                    let _class_name =
                        thread.current_frame().load_reference_class(index)?.to_string();
                    let dimensions = thread.current_frame_mut().read_u8()? as usize;
                    let mut counts = Vec::with_capacity(dimensions);
                    for _ in 0..dimensions {
                        counts.push(thread.current_frame_mut().pop()?.as_int()?);
                    }
                    counts.reverse();
                    let reference =
                        self.allocate_multi_array(&counts, 0)?;
                    thread.current_frame_mut().push(Value::Reference(reference))?;
                }
                Opcode::Wide => {
                    let inner_byte = thread.current_frame_mut().read_u8()?;
                    let inner = Opcode::from_byte(inner_byte).ok_or(VmError::InvalidOpcode {
                        opcode: inner_byte,
                        pc: opcode_pc,
                    })?;
                    let index = thread.current_frame_mut().read_u16()? as usize;
                    match inner {
                        Opcode::Iload | Opcode::Lload | Opcode::Fload
                        | Opcode::Dload | Opcode::Aload => {
                            let value = thread.current_frame().load_local(index)?;
                            thread.current_frame_mut().push(value)?;
                        }
                        Opcode::Istore | Opcode::Lstore | Opcode::Fstore
                        | Opcode::Dstore | Opcode::Astore => {
                            let value = thread.current_frame_mut().pop()?;
                            thread.current_frame_mut().store_local(index, value)?;
                        }
                        Opcode::Iinc => {
                            let delta = thread.current_frame_mut().read_i16()? as i32;
                            let value =
                                thread.current_frame().load_local(index)?.as_int()?;
                            thread
                                .current_frame_mut()
                                .store_local(index, Value::Int(value + delta))?;
                        }
                        Opcode::Ret => {
                            let target =
                                thread.current_frame().load_local(index)?.as_return_address()?;
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
                    let target =
                        thread.current_frame().load_reference_class(index)?.to_string();
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
                    let target =
                        thread.current_frame().load_reference_class(index)?.to_string();
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
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::Ifnonnull => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let reference = thread.current_frame_mut().pop()?.as_reference()?;
                    if reference != Reference::Null {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }

                // --- Control: returns ---

                Opcode::Areturn | Opcode::Ireturn | Opcode::Lreturn
                | Opcode::Freturn | Opcode::Dreturn => {
                    let value = thread.current_frame_mut().pop()?;
                    if thread.depth() == 1 {
                        return Ok(Some(ExecutionResult::Value(value)));
                    }
                    thread.pop_frame();
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Return => {
                    if thread.depth() == 1 {
                        return Ok(Some(ExecutionResult::Void));
                    }
                    thread.pop_frame();
                }

                Opcode::Arraylength => {
                    let reference = thread.current_frame_mut().pop()?.as_reference()?;
                    let length = self.heap.lock().unwrap().array_length(reference)?;
                    thread.current_frame_mut().push(Value::Int(length as i32))?;
                }
            }
            Ok(None)
    }

    /// Recursively allocate a multi-dimensional array.
    ///
    /// `depth` must be `< counts.len()` — guaranteed by the caller which reads
    /// `dimensions` (≥ 1 per JVMS) from bytecode and builds `counts` with
    /// exactly that many entries.  The recursion terminates at
    /// `depth + 1 == counts.len()`.
    fn allocate_multi_array(
        &mut self,
        counts: &[i32],
        depth: usize,
    ) -> Result<Reference, VmError> {
        let count = counts[depth];
        if count < 0 {
            return Err(VmError::NegativeArraySize { size: count });
        }
        let n = count as usize;
        if depth + 1 == counts.len() {
            // Innermost dimension — allocate an int array (common case for int[][])
            Ok(self.heap.lock().unwrap().allocate_int_array(vec![0; n]))
        } else {
            // Allocate sub-arrays recursively
            let mut elements = Vec::with_capacity(n);
            for _ in 0..n {
                let sub = self.allocate_multi_array(counts, depth + 1)?;
                elements.push(sub);
            }
            Ok(self
                .heap
                .lock()
                .unwrap()
                .allocate_reference_array("array", elements))
        }
    }

    /// Create a new exception object and throw it.
    fn throw_new_exception(
        &mut self,
        thread: &mut Thread,
        class_name: &str,
    ) -> Result<(), VmError> {
        let reference = self.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: class_name.to_string(),
            fields: HashMap::new(),
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
        if class_name.starts_with("__lambda_proxy_") {
            return self.dispatch_lambda_proxy(thread, receiver, args);
        }

        let (resolved_class, class_method) =
            self.resolve_method(class_name, &method_ref.method_name, &method_ref.descriptor)?;

        match class_method {
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
                let mut initial_locals = vec![Some(Value::Reference(receiver))];
                initial_locals.extend(args.into_iter().map(Some));
                let callee = method.with_initial_locals(initial_locals);
                thread.push_frame(Frame::new(callee));
            }
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
            let fields = match self.heap.lock().unwrap().get(receiver)? {
                HeapValue::Object { fields, .. } => fields.clone(),
                _ => return Err(VmError::NullReference),
            };

            let get_str = |key: &str| -> Result<std::string::String, VmError> {
                match fields.get(key) {
                    Some(Value::Reference(r)) => self.stringify_reference(*r),
                    _ => Ok(std::string::String::new()),
                }
            };

            let tc = get_str("__target_class")?;
            let tm = get_str("__target_method")?;
            let td = get_str("__target_desc")?;

            let mut captures = Vec::new();
            let mut i = 0;
            while let Some(val) = fields.get(&format!("__capture_{i}")) {
                captures.push(*val);
                i += 1;
            }

            (tc, tm, td, captures)
        };

        let mut all_args = captures;
        all_args.extend(args);

        self.ensure_class_loaded(&target_class)?;

        let (_, class_method) =
            self.resolve_method(&target_class, &target_method, &target_desc)?;

        match class_method {
            ClassMethod::Native => {
                let result =
                    self.invoke_native(&target_class, &target_method, &target_desc, &all_args)?;
                if let Some(value) = result {
                    thread.current_frame_mut().push(value)?;
                }
            }
            ClassMethod::Bytecode(method) => {
                let initial_locals: Vec<Option<Value>> =
                    all_args.into_iter().map(Some).collect();
                let callee = method.with_initial_locals(initial_locals);
                thread.push_frame(Frame::new(callee));
            }
        }
        Ok(())
    }
}

/// Count the number of arguments in a JVM method descriptor.
///
/// Parses the parameter section of a descriptor like `(ILjava/lang/String;)V`
/// and returns the number of parameters (2 in that example).
/// Return the JVM default zero-value for a field descriptor.
#[cfg(test)]
mod tests {
use std::collections::HashMap;
    use std::sync::atomic::Ordering;
    use std::sync::mpsc;
    use std::time::Duration;

    use super::{
        ExecutionResult, FieldRef, HeapValue, Method, MethodRef, Reference, RuntimeClass, Value,
        Vm, VmError, NEXT_THREAD_ID,
    };

    #[test]
    fn executes_basic_integer_bytecode() {
        let method = Method::new(
            [
                0x05, // iconst_2
                0x06, // iconst_3
                0x60, // iadd
                0x3b, // istore_0
                0x1a, // iload_0
                0x08, // iconst_5
                0x68, // imul
                0xac, // ireturn
            ],
            1,
            2,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(25)));
    }

    #[test]
    fn supports_explicit_local_indexes_and_dup() {
        let method = Method::new(
            [
                0x10, 0x07, // bipush 7
                0x59, // dup
                0x36, 0x01, // istore 1
                0x15, 0x01, // iload 1
                0x60, // iadd
                0xac, // ireturn
            ],
            2,
            3,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(14)));
    }

    #[test]
    fn supports_dup_x1() {
        let method = Method::new(
            [
                0x04, // iconst_1
                0x05, // iconst_2
                0x5a, // dup_x1 => [2, 1, 2]
                0x60, // iadd => [2, 3]
                0x60, // iadd => [5]
                0xac, // ireturn
            ],
            0,
            3,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(5)));
    }

    #[test]
    fn supports_dup2() {
        let method = Method::new(
            [
                0x04, // iconst_1
                0x05, // iconst_2
                0x5c, // dup2 => [1, 2, 1, 2]
                0x60, // iadd => [1, 2, 3]
                0x60, // iadd => [1, 5]
                0x60, // iadd => [6]
                0xac, // ireturn
            ],
            0,
            4,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(6)));
    }

    #[test]
    fn supports_swap() {
        let method = Method::new(
            [
                0x10, 0x05, // bipush 5
                0x10, 0x03, // bipush 3
                0x5f, // swap => [3, 5]
                0x64, // isub => 3 - 5 = -2
                0xac, // ireturn
            ],
            0,
            2,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(-2)));
    }

    #[test]
    fn supports_reference_locals_and_arraylength() {
        let mut vm = Vm::new();
        let args = vm.new_string_array(&["a".to_string(), "b".to_string()]);
        let method = Method::new(
            [
                0x2a, // aload_0
                0xbe, // arraylength
                0xac, // ireturn
            ],
            1,
            1,
        )
        .with_initial_locals([Some(args)]);

        let result = vm.execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(2)));
    }

    #[test]
    fn supports_aconst_null_and_astore() {
        let method = Method::new(
            [
                0x01, // aconst_null
                0x4b, // astore_0
                0x2a, // aload_0
                0x57, // pop
                0xb1, // return
            ],
            1,
            1,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Void);
    }

    #[test]
    fn reports_null_reference_on_arraylength() {
        let method = Method::new(
            [
                0x01, // aconst_null
                0xbe, // arraylength
                0xac, // unreachable
            ],
            0,
            1,
        );

        let error = Vm::new().execute(method).unwrap_err();
        assert_eq!(
            error,
            VmError::UnhandledException {
                class_name: "java/lang/NullPointerException".to_string()
            }
        );
    }

    #[test]
    fn supports_aaload_and_areturn() {
        let mut vm = Vm::new();
        let args = vm.new_string_array(&["x".to_string(), "y".to_string()]);
        let method = Method::new(
            [
                0x2a, // aload_0
                0x04, // iconst_1
                0x32, // aaload
                0xb0, // areturn
            ],
            1,
            2,
        )
        .with_initial_locals([Some(args)]);

        let result = vm.execute(method).unwrap();
        match result {
            ExecutionResult::Value(Value::Reference(Reference::Heap(_))) => {}
            other => panic!("expected heap reference, got {other:?}"),
        }
    }

    #[test]
    fn supports_aastore() {
        let mut vm = Vm::new();
        let array = vm.new_string_array(&["x".to_string(), "y".to_string()]);
        let value = vm.new_string("z");
        let method = Method::new(
            [
                0x2a, // aload_0
                0x04, // iconst_1
                0x2b, // aload_1
                0x53, // aastore
                0x2a, // aload_0
                0x04, // iconst_1
                0x32, // aaload
                0xb0, // areturn
            ],
            2,
            3,
        )
        .with_initial_locals([Some(array), Some(value)]);

        let result = vm.execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(value));
    }

    #[test]
    fn supports_newarray_iaload_iastore_and_arraylength() {
        let method = Method::new(
            [
                0x06, // iconst_3
                0xbc, 0x0a, // newarray int
                0x4b, // astore_0
                0x2a, // aload_0
                0x04, // iconst_1
                0x10, 0x2a, // bipush 42
                0x4f, // iastore
                0x2a, // aload_0
                0x04, // iconst_1
                0x2e, // iaload
                0x2a, // aload_0
                0xbe, // arraylength
                0x68, // imul
                0xac, // ireturn
            ],
            1,
            3,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(126)));
    }

    #[test]
    fn supports_builtin_println_for_ints_and_strings() {
        let mut vm = Vm::new();
        let hello = vm.new_string("hello");
        let method = Method::with_constant_pool(
            [
                0xb2, 0x00, 0x01, // getstatic #1
                0x10, 0x2a, // bipush 42
                0xb6, 0x00, 0x01, // invokevirtual #1 println(int)
                0xb2, 0x00, 0x01, // getstatic #1
                0x12, 0x01, // ldc #1
                0xb6, 0x00, 0x02, // invokevirtual #2 println(String)
                0xb1, // return
            ],
            0,
            2,
            vec![None, Some(hello)],
        )
        .with_field_refs(vec![
            None,
            Some(FieldRef {
                class_name: "java/lang/System".to_string(),
                field_name: "out".to_string(),
                descriptor: "Ljava/io/PrintStream;".to_string(),
            }),
        ])
        .with_method_refs(vec![
            None,
            Some(MethodRef {
                class_name: "java/io/PrintStream".to_string(),
                method_name: "println".to_string(),
                descriptor: "(I)V".to_string(),
            }),
            Some(MethodRef {
                class_name: "java/io/PrintStream".to_string(),
                method_name: "println".to_string(),
                descriptor: "(Ljava/lang/String;)V".to_string(),
            }),
        ]);

        let result = vm.execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Void);
        assert_eq!(vm.take_output(), vec!["42".to_string(), "hello".to_string()]);
    }

    #[test]
    fn supports_ifnull_and_ifnonnull() {
        let method = Method::new(
            [
                0x01, // aconst_null
                0xc6, 0x00, 0x06, // ifnull +6
                0x10, 0x63, // bipush 99
                0xac, // ireturn
                0x10, 0x2a, // bipush 42
                0xac, // ireturn
            ],
            0,
            1,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(42)));

        let mut vm = Vm::new();
        let arg = vm.new_string("hello");
        let method = Method::new(
            [
                0x2a, // aload_0
                0xc7, 0x00, 0x06, // ifnonnull +6
                0x10, 0x0b, // bipush 11
                0xac, // ireturn
                0x10, 0x16, // bipush 22
                0xac, // ireturn
            ],
            1,
            1,
        )
        .with_initial_locals([Some(arg)]);

        let result = vm.execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(22)));
    }

    #[test]
    fn supports_if_acmpeq_and_if_acmpne() {
        let mut vm = Vm::new();
        let same = vm.new_string("same");
        let other = vm.new_string("other");

        let method = Method::new(
            [
                0x2a, // aload_0
                0x2b, // aload_1
                0xa5, 0x00, 0x06, // if_acmpeq +6
                0x10, 0x09, // bipush 9
                0xac, // ireturn
                0x10, 0x15, // bipush 21
                0xac, // ireturn
            ],
            2,
            2,
        )
        .with_initial_locals([Some(same), Some(same)]);

        let result = vm.execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(21)));

        let method = Method::new(
            [
                0x2a, // aload_0
                0x2b, // aload_1
                0xa6, 0x00, 0x06, // if_acmpne +6
                0x10, 0x0d, // bipush 13
                0xac, // ireturn
                0x10, 0x22, // bipush 34
                0xac, // ireturn
            ],
            2,
            2,
        )
        .with_initial_locals([Some(same), Some(other)]);

        let result = vm.execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(34)));
    }

    #[test]
    fn reports_array_index_out_of_bounds() {
        let mut vm = Vm::new();
        let args = vm.new_string_array(&["x".to_string()]);
        let method = Method::new(
            [
                0x2a, // aload_0
                0x04, // iconst_1
                0x32, // aaload
                0xb0, // areturn
            ],
            1,
            2,
        )
        .with_initial_locals([Some(args)]);

        let error = vm.execute(method).unwrap_err();
        assert_eq!(
            error,
            VmError::UnhandledException {
                class_name: "java/lang/ArrayIndexOutOfBoundsException".to_string()
            }
        );
    }

    #[test]
    fn supports_anewarray() {
        let method = Method::new(
            [
                0x05, // iconst_2
                0xbd, 0x00, 0x01, // anewarray #1
                0xbe, // arraylength
                0xac, // ireturn
            ],
            0,
            1,
        )
        .with_reference_classes(vec![None, Some("java/lang/String".to_string())]);

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(2)));
    }

    #[test]
    fn reports_negative_array_size_for_anewarray() {
        let method = Method::new(
            [
                0x02, // iconst_m1
                0xbd, 0x00, 0x01, // anewarray #1
                0xb0, // unreachable
            ],
            0,
            1,
        )
        .with_reference_classes(vec![None, Some("java/lang/String".to_string())]);

        let error = Vm::new().execute(method).unwrap_err();
        assert_eq!(
            error,
            VmError::UnhandledException {
                class_name: "java/lang/NegativeArraySizeException".to_string()
            }
        );
    }

    #[test]
    fn reports_invalid_class_constant_for_anewarray() {
        let method = Method::new(
            [
                0x04, // iconst_1
                0xbd, 0x00, 0x02, // anewarray #2
                0xb0, // unreachable
            ],
            0,
            1,
        )
        .with_reference_classes(vec![None, Some("java/lang/String".to_string())]);

        let error = Vm::new().execute(method).unwrap_err();
        assert_eq!(
            error,
            VmError::InvalidClassConstantIndex {
                index: 2,
                constant_count: 1,
            }
        );
    }

    #[test]
    fn reports_unsupported_newarray_type() {
        let method = Method::new(
            [
                0x04, // iconst_1
                0xbc, 0x03, // newarray with invalid atype 3
                0xb0, // unreachable
            ],
            0,
            1,
        );

        let error = Vm::new().execute(method).unwrap_err();
        assert_eq!(error, VmError::UnsupportedNewArrayType { atype: 3 });
    }

    #[test]
    fn reports_division_by_zero() {
        let method = Method::new(
            [
                0x08, // iconst_5
                0x03, // iconst_0
                0x6c, // idiv
                0xac, // ireturn
            ],
            0,
            2,
        );

        let error = Vm::new().execute(method).unwrap_err();
        assert_eq!(
            error,
            VmError::UnhandledException {
                class_name: "java/lang/ArithmeticException".to_string()
            }
        );
    }

    #[test]
    fn supports_sipush_ldc_and_ineg() {
        let method = Method::with_constants(
            [
                0x11, 0x01, 0x2c, // sipush 300
                0x12, 0x01, // ldc #1
                0x60, // iadd
                0x74, // ineg
                0xac, // ireturn
            ],
            0,
            2,
            [Value::Int(7)],
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(-307)));
    }

    #[test]
    fn supports_irem() {
        let method = Method::new(
            [
                0x10, 0x11, // bipush 17
                0x10, 0x05, // bipush 5
                0x70, // irem
                0xac, // ireturn
            ],
            0,
            2,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(2)));
    }

    #[test]
    fn supports_goto_and_ifeq() {
        let method = Method::new(
            [
                0x03, // iconst_0
                0x99, 0x00, 0x08, // ifeq +8
                0x10, 0x63, // bipush 99
                0xa7, 0x00, 0x05, // goto +5
                0x10, 0x2a, // bipush 42
                0xac, // ireturn
            ],
            0,
            2,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(42)));
    }

    #[test]
    fn supports_jsr_and_ret() {
        let method = Method::new(
            [
                0x08, // iconst_5
                0x3b, // istore_0
                0xa8, 0x00, 0x05, // jsr +5 -> pc 7
                0x1a, // iload_0
                0xac, // ireturn
                0x4c, // astore_1
                0x84, 0x00, 0x01, // iinc 0 by 1
                0xa9, 0x01, // ret 1
            ],
            2,
            1,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(6)));
    }

    #[test]
    fn shares_static_fields_across_spawned_threads() {
        let mut vm = Vm::new();
        vm.register_class(RuntimeClass {
            name: "demo/Counter".to_string(),
            super_class: Some("java/lang/Object".to_string()),
            methods: HashMap::new(),
            static_fields: HashMap::from([("value".to_string(), Value::Int(0))]),
            instance_fields: vec![],
            interfaces: vec![],
        });

        let child_method = Method::new(
            [
                0x10, 0x2a, // bipush 42
                0xb3, 0x00, 0x01, // putstatic #1
                0xb1, // return
            ],
            0,
            1,
        )
        .with_field_refs(vec![
            None,
            Some(FieldRef {
                class_name: "demo/Counter".to_string(),
                field_name: "value".to_string(),
                descriptor: "I".to_string(),
            }),
        ]);

        vm.spawn(child_method).join().unwrap();

        let read_method = Method::new(
            [
                0xb2, 0x00, 0x01, // getstatic #1
                0xac, // ireturn
            ],
            0,
            1,
        )
        .with_field_refs(vec![
            None,
            Some(FieldRef {
                class_name: "demo/Counter".to_string(),
                field_name: "value".to_string(),
                descriptor: "I".to_string(),
            }),
        ]);

        let result = vm.execute(read_method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(42)));
    }

    #[test]
    fn blocks_monitorenter_until_owner_releases_monitor() {
        let vm = Vm::new();
        let monitor_ref = vm.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: "java/lang/Object".to_string(),
            fields: HashMap::new(),
        });
        vm.enter_monitor(monitor_ref).unwrap();

        let mut child_vm = vm.clone();
        child_vm.thread_id = NEXT_THREAD_ID.fetch_add(1, Ordering::Relaxed);

        let (started_tx, started_rx) = mpsc::channel();
        let (acquired_tx, acquired_rx) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            child_vm.enter_monitor(monitor_ref).unwrap();
            acquired_tx.send(()).unwrap();
            child_vm.exit_monitor(monitor_ref).unwrap();
        });

        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(acquired_rx.recv_timeout(Duration::from_millis(50)).is_err());

        vm.exit_monitor(monitor_ref).unwrap();

        acquired_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        handle.join().unwrap();
    }

    #[test]
    fn supports_iinc_with_positive_and_negative_deltas() {
        let method = Method::new(
            [
                0x10, 0x0a, // bipush 10
                0x3b, // istore_0
                0x84, 0x00, 0x05, // iinc 0 by 5
                0x84, 0x00, 0xfd, // iinc 0 by -3
                0x1a, // iload_0
                0xac, // ireturn
            ],
            1,
            1,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(12)));
    }

    #[test]
    fn supports_ifne_and_if_icmpne() {
        let method = Method::new(
            [
                0x04, // iconst_1
                0x9a, 0x00, 0x06, // ifne +6
                0x10, 0x64, // bipush 100
                0xac, // ireturn
                0x05, // iconst_2
                0x06, // iconst_3
                0xa0, 0x00, 0x06, // if_icmpne +6
                0x10, 0x37, // bipush 55
                0xac, // ireturn
                0x10, 0x58, // bipush 88
                0xac, // ireturn
            ],
            0,
            2,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(88)));
    }

    #[test]
    fn supports_iflt_ifge_ifgt_and_ifle() {
        let method = Method::new(
            [
                0x02, // iconst_m1
                0x9b, 0x00, 0x08, // iflt +8
                0x10, 0x63, // bipush 99
                0xa7, 0x00, 0x29, // goto +41
                0x03, // iconst_0
                0x9c, 0x00, 0x08, // ifge +8
                0x10, 0x62, // bipush 98
                0xa7, 0x00, 0x20, // goto +32
                0x04, // iconst_1
                0x9d, 0x00, 0x08, // ifgt +8
                0x10, 0x61, // bipush 97
                0xa7, 0x00, 0x17, // goto +23
                0x03, // iconst_0
                0x9e, 0x00, 0x08, // ifle +8
                0x10, 0x60, // bipush 96
                0xa7, 0x00, 0x0e, // goto +14
                0x10, 0x2c, // bipush 44
                0xac, // ireturn
                0x10, 0x0b, // bipush 11
                0xac, // ireturn
            ],
            0,
            1,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(44)));
    }

    #[test]
    fn supports_if_icmpeq() {
        let method = Method::new(
            [
                0x08, // iconst_5
                0x10, 0x05, // bipush 5
                0x9f, 0x00, 0x06, // if_icmpeq +6
                0x10, 0x09, // bipush 9
                0xac, // ireturn
                0x10, 0x21, // bipush 33
                0xac, // ireturn
            ],
            0,
            2,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(33)));
    }

    #[test]
    fn supports_if_icmplt_if_icmpge_if_icmpgt_and_if_icmple() {
        let method = Method::new(
            [
                0x04, // iconst_1
                0x05, // iconst_2
                0xa1, 0x00, 0x08, // if_icmplt +8
                0x10, 0x63, // bipush 99
                0xa7, 0x00, 0x32, // goto +50
                0x05, // iconst_2
                0x05, // iconst_2
                0xa2, 0x00, 0x08, // if_icmpge +8
                0x10, 0x62, // bipush 98
                0xa7, 0x00, 0x28, // goto +40
                0x06, // iconst_3
                0x05, // iconst_2
                0xa3, 0x00, 0x08, // if_icmpgt +8
                0x10, 0x61, // bipush 97
                0xa7, 0x00, 0x1e, // goto +30
                0x04, // iconst_1
                0x04, // iconst_1
                0xa4, 0x00, 0x08, // if_icmple +8
                0x10, 0x60, // bipush 96
                0xa7, 0x00, 0x14, // goto +20
                0x10, 0x4d, // bipush 77
                0xac, // ireturn
                0x10, 0x0c, // bipush 12
                0xac, // ireturn
            ],
            0,
            2,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(77)));
    }

    #[test]
    fn reports_invalid_constant_index() {
        let method = Method::with_constants(
            [
                0x12, 0x02, // ldc #2
                0xac, // ireturn
            ],
            0,
            1,
            [Value::Int(1)],
        );

        let error = Vm::new().execute(method).unwrap_err();
        assert_eq!(
            error,
            VmError::InvalidConstantIndex {
                index: 2,
                constant_count: 1,
            }
        );
    }

    #[test]
    fn reports_invalid_branch_target() {
        let method = Method::new(
            [
                0xa7, 0x7f, 0xff, // goto far away
            ],
            0,
            0,
        );

        let error = Vm::new().execute(method).unwrap_err();
        assert_eq!(
            error,
            VmError::InvalidBranchTarget {
                target: 32767,
                code_len: 3,
            }
        );
    }

    #[test]
    fn gc_threshold_and_stats_tracked() {
        let mut vm = Vm::new();
        vm.set_gc_threshold(1);

        // Force a known number of string allocations. Each `new_string`
        // bumps `total_allocations`; since the threshold is 1 and the
        // strings are unreachable from any rooted frame, each one should
        // trigger a collection that frees the prior string.
        let _ = vm.new_string("one".to_string());
        let _ = vm.new_string("two".to_string());
        let _ = vm.new_string("three".to_string());

        // Do one final manual pass to clean up whatever remains.
        vm.request_gc();

        let stats = vm.gc_stats();
        assert!(stats.total_allocations >= 3, "stats: {stats:?}");
        assert!(stats.collections >= 1, "stats: {stats:?}");
    }

    #[test]
    fn disable_gc_stops_automatic_collections() {
        let mut vm = Vm::new();
        vm.disable_gc();
        for i in 0..64 {
            let _ = vm.new_string(format!("s{i}"));
        }
        // No automatic collection should have run.
        assert_eq!(vm.gc_stats().collections, 0);
        // But a manual request still works.
        vm.request_gc();
        assert_eq!(vm.gc_stats().collections, 1);
    }
}
