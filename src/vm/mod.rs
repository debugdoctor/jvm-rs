mod builtin;
mod classloader;
mod frame;
mod heap;
pub mod interpreter;
pub mod jit;
mod thread;
mod types;
pub mod verify;

pub use crate::classfile::ClassFile;
use frame::Frame;
pub use heap::GcStats;
use heap::{Heap, HeapValue};
use interpreter::{
    execute_aconst_null, execute_aload, execute_areturn_full, execute_astore, execute_bipush,
    execute_dconst, execute_dload, execute_dstore, execute_dup, execute_fconst, execute_fload,
    execute_fstore, execute_iadd, execute_iconst, execute_iload, execute_imul,
    execute_ireturn_full, execute_istore, execute_isub, execute_lconst, execute_ldc, execute_ldc_w,
    execute_lload, execute_lreturn_full, execute_lstore, execute_pop, execute_return_full,
    execute_sipush,
};
use smallvec::SmallVec;
pub use thread::JvmThread;
use thread::{
    ClassInitializationState, JavaThreadState, RuntimeState, SharedMonitors, SharedThreads, Thread,
};
pub use types::{
    ClassMethod, ExceptionHandler, ExecutionResult, FieldRef, InvokeDynamicKind, InvokeDynamicSite,
    Method, MethodRef, Reference, RuntimeClass, Value, VmError,
};
use types::{default_value_for_descriptor, format_vm_float, parse_arg_count, parse_arg_types};

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::bytecode::Opcode;
use crate::vm::jit::JitCompiler;
use crate::vm::jit::runtime::JitContext;
use classloader::{BootstrapClassLoader, ClassLoader, LazyClassLoader};

use crate::vm::jit::runtime::{clear_current_vm, set_current_vm, take_pending_jit_exception};

static NEXT_THREAD_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

enum JitInvocationResult {
    Returned(Option<Value>),
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
    thread_id: u64,
    output: Arc<Mutex<Vec<String>>>,
    jit: Option<JitCompiler>,
    jit_context: Option<JitContext>,
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
            thread_id: NEXT_THREAD_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            output: self.output.clone(),
            jit: None,
            jit_context: None,
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
            thread_id: NEXT_THREAD_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            output: Arc::new(Mutex::new(Vec::new())),
            jit,
            jit_context,
        };
        vm.bootstrap();
        Ok(vm)
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
        args: Vec<Value>,
    ) -> Result<JvmThread, VmError> {
        let mut child_vm = self.clone();
        child_vm.thread_id = NEXT_THREAD_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

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

    /// Test hook: how many times execution reached the JIT tier. Methods that
    /// the backend cannot lower yet are counted before deoptimizing to the
    /// interpreter so threshold bugs do not look like normal interpreter runs.
    pub fn jit_executions(&self) -> u64 {
        self.runtime.lock().unwrap().jit_executions
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

    /// Set the classpath entries used for on-demand class loading.
    pub fn set_class_path(&mut self, paths: Vec<PathBuf>) {
        self.class_path = paths;
    }

    /// Register a class loaded from a `.class` file.
    pub fn register_class(&mut self, class: RuntimeClass) {
        self.runtime
            .lock()
            .unwrap()
            .classes
            .insert(class.name.clone(), class);
    }

    /// Project a value list into JVM-slot-indexed locals: longs and doubles
    /// occupy two slots per JVMS §2.6, so subsequent parameters land at the
    /// index the bytecode expects. Without this padding, methods like
    /// `ArraysSupport.vectorizedMismatch(Object,J,Object,J,I,I)` read local
    /// 7 (the second int) from an uninitialized slot.
    pub(super) fn collect_jit_args_static(method: &Method, frame: &Frame) -> Vec<Value> {
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

    fn try_execute_jit_method(
        &mut self,
        method: &Method,
        args: &[Value],
    ) -> Option<JitInvocationResult> {
        let method_key = format!("{}.{}{}", method.class_name, method.name, method.descriptor);
        let code = self.jit.as_ref()?.get_or_compile(method)?;
        let vm_ptr = self as *mut Vm as u64;
        let jit_context = self.jit_context.as_mut()?;

        if jit_context.get_entry(&method_key).is_none()
            && !jit_context.add_method(method_key.clone(), code)
        {
            return None;
        }

        let ret = crate::vm::jit::runtime::JitReturn::from_descriptor(&method.descriptor);
        let result = jit_context.execute_typed(vm_ptr, &method_key, args, ret)?;
        self.runtime.lock().unwrap().jit_executions += 1;

        if let Some(exception_ref) = take_pending_jit_exception() {
            return Some(JitInvocationResult::Threw(exception_ref));
        }

        if matches!(ret, crate::vm::jit::runtime::JitReturn::Void) {
            Some(JitInvocationResult::Returned(None))
        } else {
            Some(JitInvocationResult::Returned(Some(result)))
        }
    }

    pub(crate) fn invoke_jit_static_method_ref(
        &mut self,
        method_ref: &MethodRef,
        args_ptr: u64,
        argc: usize,
    ) -> Option<Value> {
        let args = unsafe { Vm::jit_raw_args_to_values(&method_ref.descriptor, args_ptr, argc) }?;
        self.ensure_class_loaded(&method_ref.class_name).ok()?;
        self.ensure_class_initialized(&method_ref.class_name).ok()?;

        if self.has_native_override(
            &method_ref.class_name,
            &method_ref.method_name,
            &method_ref.descriptor,
        ) {
            return self
                .invoke_native(
                    &method_ref.class_name,
                    &method_ref.method_name,
                    &method_ref.descriptor,
                    &args,
                )
                .ok()
                .flatten();
        }

        let class = self.get_class(&method_ref.class_name).ok()?;
        let class_method = class
            .methods
            .get(&(
                method_ref.method_name.clone(),
                method_ref.descriptor.clone(),
            ))
            .cloned()?;

        match class_method {
            ClassMethod::Native => self
                .invoke_native(
                    &method_ref.class_name,
                    &method_ref.method_name,
                    &method_ref.descriptor,
                    &args,
                )
                .ok()
                .flatten(),
            ClassMethod::Bytecode(method) => {
                let callee = method.with_initial_locals(Vm::args_to_locals(args));
                let saved_jit = self.jit.take();
                let result = self.execute(callee);
                self.jit = saved_jit;
                match result.ok()? {
                    ExecutionResult::Value(value) => Some(value),
                    ExecutionResult::Void => None,
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
    ) -> Option<Value> {
        let receiver = Vm::jit_raw_reference(receiver_raw)?;
        let class_name = self.get_object_class(receiver).ok()?;
        self.invoke_jit_instance_method_ref(&class_name, method_ref, receiver, args_ptr, argc)
    }

    pub(crate) fn invoke_jit_special_method_ref(
        &mut self,
        method_ref: &MethodRef,
        receiver_raw: u64,
        args_ptr: u64,
        argc: usize,
    ) -> Option<Value> {
        let receiver = Vm::jit_raw_reference(receiver_raw)?;
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
    ) -> Option<Value> {
        let receiver = Vm::jit_raw_reference(receiver_raw)?;
        let class_name = self.get_object_class(receiver).ok()?;
        self.invoke_jit_instance_method_ref(&class_name, method_ref, receiver, args_ptr, argc)
    }

    pub(crate) fn invoke_jit_native_method_ref(
        &mut self,
        method_ref: &MethodRef,
        args_ptr: u64,
        argc: usize,
    ) -> Option<Value> {
        let args = unsafe { Vm::jit_raw_args_to_values(&method_ref.descriptor, args_ptr, argc) }?;
        self.invoke_native(
            &method_ref.class_name,
            &method_ref.method_name,
            &method_ref.descriptor,
            &args,
        )
        .ok()
        .flatten()
    }

    pub(crate) fn invoke_jit_dynamic_site(
        &mut self,
        site: &InvokeDynamicSite,
        args_ptr: u64,
        argc: usize,
    ) -> Option<Value> {
        let args = unsafe { Vm::jit_raw_args_to_values(&site.descriptor, args_ptr, argc) }?;
        match &site.kind {
            InvokeDynamicKind::LambdaProxy {
                target_class,
                target_method,
                target_descriptor,
            } => {
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
                Some(Value::Reference(proxy))
            }
            InvokeDynamicKind::StringConcat { recipe, constants } => self
                .build_string_concat(recipe.as_deref(), constants, &args, &site.descriptor)
                .ok()
                .map(|concat| self.new_string(concat)),
            InvokeDynamicKind::Unknown => Some(Value::Reference(Reference::Null)),
        }
    }

    pub(crate) fn invoke_jit_get_static_field_ref(
        &mut self,
        field_ref: &FieldRef,
    ) -> Option<Value> {
        self.ensure_class_loaded(&field_ref.class_name).ok()?;
        self.ensure_class_initialized(&field_ref.class_name).ok()?;
        self.get_static_field(&field_ref.class_name, &field_ref.field_name)
            .ok()
    }

    pub(crate) fn invoke_jit_put_static_field_ref(
        &mut self,
        field_ref: &FieldRef,
        raw_value: u64,
    ) -> bool {
        let Some(value) = Vm::jit_raw_field_value_to_value(&field_ref.descriptor, raw_value) else {
            return false;
        };
        self.ensure_class_loaded(&field_ref.class_name).is_ok()
            && self.ensure_class_initialized(&field_ref.class_name).is_ok()
            && self
                .put_static_field(&field_ref.class_name, &field_ref.field_name, value)
                .is_ok()
    }

    pub(crate) fn invoke_jit_get_instance_field_ref(
        &mut self,
        field_ref: &FieldRef,
        receiver_raw: u64,
    ) -> Option<Value> {
        let receiver = Vm::jit_raw_reference(receiver_raw)?;
        match self.heap.lock().unwrap().get(receiver).ok()? {
            HeapValue::Object { fields, .. } => fields.get(&field_ref.field_name).copied(),
            _ => None,
        }
    }

    pub(crate) fn invoke_jit_put_instance_field_ref(
        &mut self,
        field_ref: &FieldRef,
        receiver_raw: u64,
        raw_value: u64,
    ) -> bool {
        let Some(receiver) = Vm::jit_raw_reference(receiver_raw) else {
            return false;
        };
        let Some(value) = Vm::jit_raw_field_value_to_value(&field_ref.descriptor, raw_value) else {
            return false;
        };
        self.set_object_field(receiver, &field_ref.field_name, value)
            .is_ok()
    }

    pub(crate) fn invoke_jit_allocate_object(&mut self, class_name: &str) -> Option<Reference> {
        self.ensure_class_loaded(class_name).ok()?;
        self.ensure_class_initialized(class_name).ok()?;

        let mut all_instance_fields = Vec::new();
        let mut current_class = class_name.to_string();
        loop {
            self.ensure_class_loaded(&current_class).ok()?;
            let class = self.get_class(&current_class).ok()?;
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
    ) -> Option<Value> {
        if class_name.starts_with("__lambda_proxy_") {
            return None;
        }

        let args = unsafe { Vm::jit_raw_args_to_values(&method_ref.descriptor, args_ptr, argc) }?;
        let mut all_args = vec![Value::Reference(receiver)];
        all_args.extend(args);

        if self.has_native_override(class_name, &method_ref.method_name, &method_ref.descriptor) {
            return self
                .invoke_native(
                    class_name,
                    &method_ref.method_name,
                    &method_ref.descriptor,
                    &all_args,
                )
                .ok()
                .flatten();
        }

        let (resolved_class, class_method) = self
            .resolve_method(class_name, &method_ref.method_name, &method_ref.descriptor)
            .ok()?;

        match class_method {
            ClassMethod::Native => self
                .invoke_native(
                    &resolved_class,
                    &method_ref.method_name,
                    &method_ref.descriptor,
                    &all_args,
                )
                .ok()
                .flatten(),
            ClassMethod::Bytecode(method) => {
                let callee = method.with_initial_locals(Vm::args_to_locals(all_args));
                let saved_jit = self.jit.take();
                let result = self.execute(callee);
                self.jit = saved_jit;
                match result.ok()? {
                    ExecutionResult::Value(value) => Some(value),
                    ExecutionResult::Void => None,
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
        let mut fields = std::collections::HashMap::new();
        if let Value::Reference(r) = name_ref {
            fields.insert("__name".to_string(), Value::Reference(r));
        }
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

        // Array classes (e.g., [I, [Ljava/lang/Object;) are synthesized at runtime
        if class_name.starts_with('[') {
            return self.register_synthetic_array_class(class_name);
        }

        if let Some(ref mut loader) = self.class_loader {
            if let Ok(Some(class_file)) = ClassLoader::load_classfile(loader, class_name) {
                self.register_classfile(class_name, &class_file);
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
            })
        } else {
            Err(VmError::ClassNotFound {
                class_name: class_name.to_string(),
            })
        }
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
            HeapValue::Object { fields, .. } => Ok(*fields
                .get(field_name)
                .unwrap_or(&Value::Reference(Reference::Null))),
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
            HeapValue::ReferenceArray { component_type, .. } => Ok(format!("[L{component_type};")),
        }
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
                                if let Some(exception_ref) = take_pending_jit_exception() {
                                    let class_name = self.get_object_class(exception_ref)?;
                                    return Err(VmError::UnhandledException { class_name });
                                }
                                if matches!(ret, crate::vm::jit::runtime::JitReturn::Void) {
                                    return Ok(ExecutionResult::Void);
                                }
                                return Ok(ExecutionResult::Value(result));
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

                match self.execute_opcode(&mut thread, opcode, opcode_pc) {
                    Ok(Some(result)) => return Ok(result),
                    Ok(None) => {}
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
            Opcode::Ldc => execute_ldc(thread)?,
            Opcode::LdcW => execute_ldc_w(thread)?,
            Opcode::Ldc2W => {
                let index = thread.current_frame_mut().read_u16()? as usize;
                let value = thread.current_frame().load_constant(index)?;
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
            Opcode::Iload | Opcode::Lload | Opcode::Fload | Opcode::Dload => {
                let index = thread.current_frame_mut().read_u8()? as usize;
                execute_iload(thread, index)?;
            }
            Opcode::Iload0 | Opcode::Lload0 | Opcode::Fload0 | Opcode::Dload0 => {
                execute_iload(thread, 0)?;
            }
            Opcode::Iload1 | Opcode::Lload1 | Opcode::Fload1 | Opcode::Dload1 => {
                execute_iload(thread, 1)?;
            }
            Opcode::Iload2 | Opcode::Lload2 | Opcode::Fload2 | Opcode::Dload2 => {
                execute_iload(thread, 2)?;
            }
            Opcode::Iload3 | Opcode::Lload3 | Opcode::Fload3 | Opcode::Dload3 => {
                execute_iload(thread, 3)?;
            }
            Opcode::Aload0 => execute_iload(thread, 0)?,
            Opcode::Aload1 => execute_iload(thread, 1)?,
            Opcode::Aload2 => execute_iload(thread, 2)?,
            Opcode::Aload3 => execute_iload(thread, 3)?,
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
            Opcode::Istore | Opcode::Lstore | Opcode::Fstore | Opcode::Dstore => {
                let index = thread.current_frame_mut().read_u8()? as usize;
                execute_istore(thread, index)?;
            }
            Opcode::Istore0 | Opcode::Lstore0 | Opcode::Fstore0 | Opcode::Dstore0 => {
                execute_istore(thread, 0)?;
            }
            Opcode::Istore1 | Opcode::Lstore1 | Opcode::Fstore1 | Opcode::Dstore1 => {
                execute_istore(thread, 1)?;
            }
            Opcode::Istore2 | Opcode::Lstore2 | Opcode::Fstore2 | Opcode::Dstore2 => {
                execute_istore(thread, 2)?;
            }
            Opcode::Istore3 | Opcode::Lstore3 | Opcode::Fstore3 | Opcode::Dstore3 => {
                execute_istore(thread, 3)?;
            }
            Opcode::Astore0 => execute_istore(thread, 0)?,
            Opcode::Astore1 => execute_istore(thread, 1)?,
            Opcode::Astore2 => execute_istore(thread, 2)?,
            Opcode::Astore3 => execute_istore(thread, 3)?,
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
                let _ = thread.current_frame_mut().pop()?;
                let _ = thread.current_frame_mut().pop()?;
            }
            Opcode::Dup => execute_dup(thread)?,
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
                        });
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
                        });
                    }
                };
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

                let should_return_false = receiver == Reference::Null
                    && method_ref.class_name == "java/lang/Class"
                    && method_ref.method_name == "desiredAssertionStatus"
                    && method_ref.descriptor == "()Z";

                if should_return_false {
                    thread.current_frame_mut().push(Value::Int(0))?;
                } else {
                    let receiver_class = self.get_object_class(receiver)?;

                    if let Some(cached_method) = thread
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
                        let (resolved_class, class_method) = self.resolve_method(
                            &receiver_class,
                            &method_ref.method_name,
                            &method_ref.descriptor,
                        )?;
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
                let class_name = thread
                    .current_frame()
                    .load_reference_class(index)?
                    .to_string();
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
            Opcode::Areturn
            | Opcode::Ireturn
            | Opcode::Lreturn
            | Opcode::Freturn
            | Opcode::Dreturn => {
                return execute_ireturn_full(thread);
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

    fn array_component_name(component_descriptor: &str) -> String {
        if component_descriptor.starts_with('L') && component_descriptor.ends_with(';') {
            component_descriptor[1..component_descriptor.len() - 1].to_string()
        } else {
            component_descriptor.to_string()
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
                let mut all_args = vec![Value::Reference(receiver)];
                all_args.extend(args);
                let callee = method.with_initial_locals(Vm::args_to_locals(all_args));
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
#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::Ordering;
    use std::sync::mpsc;
    use std::time::Duration;

    use super::{
        ExecutionResult, FieldRef, HeapValue, Method, MethodRef, NEXT_THREAD_ID, Reference,
        RuntimeClass, Value, Vm, VmError,
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

        let result = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap();
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

        let result = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap();
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

        let result = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap();
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

        let result = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap();
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

        let result = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(-2)));
    }

    #[test]
    fn supports_reference_locals_and_arraylength() {
        let mut vm = Vm::new().expect("failed to create VM");
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
    fn preserves_local_slot_spacing_after_wide_arguments() {
        let method = Method::new(
            [
                0x1d, // iload_3
                0xac, // ireturn
            ],
            4,
            1,
        )
        .with_metadata("Main", "f", "(IDZ)I", 0x0009)
        .with_initial_locals(Vm::args_to_locals(vec![
            Value::Int(7),
            Value::Double(3.14),
            Value::Int(1),
        ]));

        let result = Vm::new().expect("failed to create VM").execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(1)));
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

        let result = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap();
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

        let error = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap_err();
        assert_eq!(
            error,
            VmError::UnhandledException {
                class_name: "java/lang/NullPointerException".to_string()
            }
        );
    }

    #[test]
    fn supports_aaload_and_areturn() {
        let mut vm = Vm::new().expect("failed to create VM");
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
        let mut vm = Vm::new().expect("failed to create VM");
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

        let result = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(126)));
    }

    #[test]
    fn supports_builtin_println_for_ints_and_strings() {
        let mut vm = Vm::new().expect("failed to create VM");
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
        assert_eq!(
            vm.take_output(),
            vec!["42".to_string(), "hello".to_string()]
        );
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

        let result = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(42)));

        let mut vm = Vm::new().expect("failed to create VM");
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
        let mut vm = Vm::new().expect("failed to create VM");
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
        let mut vm = Vm::new().expect("failed to create VM");
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

        let result = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap();
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

        let error = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap_err();
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

        let error = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap_err();
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

        let error = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap_err();
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

        let error = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap_err();
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

        let result = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap();
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

        let result = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap();
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

        let result = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap();
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

        let result = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(6)));
    }

    #[test]
    fn shares_static_fields_across_spawned_threads() {
        let mut vm = Vm::new().expect("failed to create VM");
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
        let vm = Vm::new().expect("failed to create VM");
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

        let result = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap();
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

        let result = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap();
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

        let result = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap();
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

        let result = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap();
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

        let result = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap();
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

        let error = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap_err();
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

        let error = Vm::new()
            .expect("failed to create VM")
            .execute(method)
            .unwrap_err();
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
        let mut vm = Vm::new().expect("failed to create VM");
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
        let mut vm = Vm::new().expect("failed to create VM");
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
