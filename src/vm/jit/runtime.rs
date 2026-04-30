use std::collections::HashMap;
use std::sync::RwLock;

#[cfg(unix)]
use libc;

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

pub struct JitEntry {
    code_ptr: usize,
    frame_size: usize,
    num_slots: usize,
}

impl JitEntry {
    pub fn new(code: Vec<u8>, frame_size: usize, num_slots: usize) -> Option<Self> {
        Self::make_executable(code).map(|(code_ptr, _, _)| JitEntry {
            code_ptr,
            frame_size,
            num_slots,
        })
    }

    fn make_executable(code: Vec<u8>) -> Option<(usize, usize, Box<[u8]>)> {
        let size = code.len();
        if size == 0 {
            return None;
        }

        let page_size = 4096;
        let alloc_size = ((size + page_size - 1) / page_size) * page_size;

        #[cfg(unix)]
        {
            use std::ptr;

            let ptr = unsafe {
                libc::mmap(
                    ptr::null_mut(),
                    alloc_size,
                    libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
                    libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                    -1,
                    0,
                )
            };

            if ptr == libc::MAP_FAILED {
                return None;
            }

            unsafe {
                ptr::copy_nonoverlapping(code.as_ptr(), ptr as *mut u8, size);
            }

            let boxed = code.into_boxed_slice();
            std::mem::forget(boxed);

            return Some((ptr as usize, alloc_size, unsafe {
                Box::from_raw(std::slice::from_raw_parts_mut(ptr as *mut u8, alloc_size))
            }));
        }

        #[cfg(not(unix))]
        None
    }

    pub fn code_ptr(&self) -> usize {
        self.code_ptr
    }

    pub fn frame_size(&self) -> usize {
        self.frame_size
    }

    pub fn num_slots(&self) -> usize {
        self.num_slots
    }
}

impl Drop for JitEntry {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::ptr;
            let size = self.frame_size.max(4096);
            unsafe {
                libc::munmap(self.code_ptr as *mut libc::c_void, size);
            }
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

pub struct JitContext {
    entries: HashMap<String, JitEntry>,
}

impl JitContext {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn add_method(&mut self, key: String, code: CompiledCode) -> bool {
        let frame_size = code.frame_size;
        let num_slots = code.stack_slots.len();

        match JitEntry::new(code.code_buffer, frame_size, num_slots) {
            Some(entry) => {
                self.entries.insert(key, entry);
                true
            }
            None => false,
        }
    }

    pub fn get_entry(&self, key: &str) -> Option<&JitEntry> {
        self.entries.get(key)
    }

    pub fn execute(&self, key: &str, args: &[crate::vm::types::Value]) -> Option<crate::vm::types::Value> {
        self.entries.get(key).map(|entry| {
            let fn_ptr = entry.code_ptr();
            let frame_size = entry.frame_size();

            #[cfg(target_arch = "x86_64")]
            {
                type JitFn = extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64;
                let fn_typed = unsafe { std::mem::transmute::<usize, JitFn>(fn_ptr) };

                let mut int_args: [u64; 6] = [0; 6];
                let mut float_args: [f64; 8] = [0.0; 8];
                let mut int_count = 0;
                let mut float_count = 0;

                int_args[0] = 0u64;

                for arg in args {
                    match arg {
                        crate::vm::types::Value::Int(v) => {
                            if int_count < 5 { int_args[int_count + 1] = *v as u64; }
                            int_count += 1;
                        }
                        crate::vm::types::Value::Long(v) => {
                            if int_count < 5 { int_args[int_count + 1] = *v as u64; }
                            int_count += 1;
                        }
                        crate::vm::types::Value::Float(v) => {
                            if float_count < 8 { float_args[float_count] = *v as f64; }
                            float_count += 1;
                        }
                        crate::vm::types::Value::Double(v) => {
                            if float_count < 8 { float_args[float_count] = *v; }
                            float_count += 1;
                        }
                        crate::vm::types::Value::Reference(r) => {
                            let ptr = match r {
                                crate::vm::types::Reference::Null => 0usize,
                                crate::vm::types::Reference::Heap(addr) => *addr,
                            };
                            if int_count < 5 { int_args[int_count + 1] = ptr as u64; }
                            int_count += 1;
                        }
                        crate::vm::types::Value::ReturnAddress(_) => {}
                    }
                }

                let result = unsafe {
                    fn_typed(int_args[0], int_args[1], int_args[2], int_args[3], int_args[4], int_args[5])
                };

                crate::vm::types::Value::Int(result as i32)
            }

            #[cfg(not(target_arch = "x86_64"))]
            {
                let _ = (args, frame_size);
                crate::vm::types::Value::Int(0)
            }
        })
    }
}

impl Default for JitContext {
    fn default() -> Self {
        Self::new()
    }
}