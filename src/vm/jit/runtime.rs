use std::collections::HashMap;
use std::sync::RwLock;

use super::CompiledCode;

pub struct JitRuntime {
    compiled_methods: RwLock<HashMap<String, CompiledCode>>,
    invocation_counts: HashMap<String, u64>,
    backedge_counts: HashMap<String, u64>,
    compilation_threshold: u64,
    deopt_threshold: u64,
}

impl JitRuntime {
    pub fn new() -> Self {
        Self {
            compiled_methods: RwLock::new(HashMap::new()),
            invocation_counts: HashMap::new(),
            backedge_counts: HashMap::new(),
            compilation_threshold: 1000,
            deopt_threshold: 2000,
        }
    }

    pub fn install_compiled_code(&self, method_key: String, code: CompiledCode) {
        self.compiled_methods.write().unwrap().insert(method_key, code);
    }

    pub fn get_compiled_code(&self, method_key: &str) -> Option<CompiledCode> {
        self.compiled_methods.read().unwrap().get(method_key).cloned()
    }

    pub fn increment_invocation_count(&mut self, method_key: &str) {
        *self.invocation_counts.entry(method_key.to_string()).or_insert(0) += 1;
    }

    pub fn increment_backedge_count(&mut self, method_key: &str) {
        *self.backedge_counts.entry(method_key.to_string()).or_insert(0) += 1;
    }

    pub fn should_compile(&self, method_key: &str) -> bool {
        let invocation_count = self.invocation_counts.get(method_key).copied().unwrap_or(0);
        invocation_count >= self.compilation_threshold
    }

    pub fn should_deoptimize(&self, method_key: &str) -> bool {
        let deopt_count = self.get_deopt_count(method_key);
        deopt_count >= self.deopt_threshold
    }

    fn get_deopt_count(&self, _method_key: &str) -> u64 {
        0
    }

    pub fn deoptimize(&self, method_key: &str) {
        self.compiled_methods.write().unwrap().remove(method_key);
    }
}

impl Default for JitRuntime {
    fn default() -> Self {
        Self::new()
    }
}

pub struct JITEntry {
    pub code: *const u8,
    pub frame_size: usize,
    pub num_slots: usize,
}

impl JITEntry {
    pub fn new(code: Vec<u8>, frame_size: usize, num_slots: usize) -> Self {
        let code_box = code.into_boxed_slice();
        let code_ptr = Box::into_raw(code_box);
        Self {
            code: code_ptr as *const u8,
            frame_size,
            num_slots,
        }
    }
}

impl Drop for JITEntry {
    fn drop(&mut self) {
        unsafe {
            let _ = Box::from_raw(self.code as *mut u8);
        }
    }
}

pub trait JitCall {
    fn call(&self, args: &[crate::vm::types::Value]) -> Result<crate::vm::types::Value, ()>;
}

pub struct NativeCall {
    pub fn_ptr: *const u8,
}

impl NativeCall {
    pub fn new(fn_ptr: *const u8) -> Self {
        Self { fn_ptr }
    }
}

impl JitCall for NativeCall {
    fn call(&self, _args: &[crate::vm::types::Value]) -> Result<crate::vm::types::Value, ()> {
        Ok(crate::vm::types::Value::Int(0))
    }
}