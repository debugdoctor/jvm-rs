use std::collections::HashMap;
use std::sync::RwLock;
use std::sync::OnceLock;

#[cfg(unix)]
use libc;

use super::CompiledCode;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
unsafe extern "C" {
    fn sys_icache_invalidate(start: *mut libc::c_void, len: libc::size_t);
}

thread_local! {
    static CURRENT_VM: std::cell::UnsafeCell<u64> = std::cell::UnsafeCell::new(0);
}

pub fn set_current_vm(vm_ptr: u64) {
    CURRENT_VM.with(|cell| {
        unsafe { *cell.get() = vm_ptr; }
    });
}

pub fn clear_current_vm() {
    CURRENT_VM.with(|cell| {
        unsafe { *cell.get() = 0; }
    });
}

pub fn get_current_vm_ptr() -> u64 {
    CURRENT_VM.with(|cell| unsafe { *cell.get() })
}

pub type JitHelperFn = extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64;

pub struct JitRuntimeHelpers {
    pub allocate_object: JitHelperFn,
    pub allocate_array: JitHelperFn,
    pub get_static_field: JitHelperFn,
    pub put_static_field: JitHelperFn,
    pub get_instance_field: JitHelperFn,
    pub put_instance_field: JitHelperFn,
    pub invoke_virtual: JitHelperFn,
    pub invoke_special: JitHelperFn,
    pub invoke_static: JitHelperFn,
    pub invoke_interface: JitHelperFn,
    pub checkcast: JitHelperFn,
    pub instanceof: JitHelperFn,
    pub athrow: JitHelperFn,
    pub monitor_enter: JitHelperFn,
    pub monitor_exit: JitHelperFn,
}

static JIT_HELPERS: OnceLock<JitRuntimeHelpers> = OnceLock::new();

pub static mut INVOKE_VIRTUAL_fn: JitHelperFn = jit_helper_invoke_virtual;
pub static mut INVOKE_SPECIAL_fn: JitHelperFn = jit_helper_invoke_special;
pub static mut INVOKE_STATIC_fn: JitHelperFn = jit_helper_invoke_static;
pub static mut INVOKE_INTERFACE_fn: JitHelperFn = jit_helper_invoke_interface;

pub fn initialize_jit_helpers() {
    let _ = JIT_HELPERS.get_or_init(|| JitRuntimeHelpers {
        allocate_object: jit_helper_allocate_object,
        allocate_array: jit_helper_allocate_array,
        get_static_field: jit_helper_get_static_field,
        put_static_field: jit_helper_put_static_field,
        get_instance_field: jit_helper_get_instance_field,
        put_instance_field: jit_helper_put_instance_field,
        invoke_virtual: jit_helper_invoke_virtual,
        invoke_special: jit_helper_invoke_special,
        invoke_static: jit_helper_invoke_static,
        invoke_interface: jit_helper_invoke_interface,
        checkcast: jit_helper_checkcast,
        instanceof: jit_helper_instanceof,
        athrow: jit_helper_athrow,
        monitor_enter: jit_helper_monitor_enter,
        monitor_exit: jit_helper_monitor_exit,
    });
}

pub fn get_jit_helpers_ptr() -> u64 {
    let helpers = JIT_HELPERS.get().expect("JIT helpers not initialized");
    helpers as *const JitRuntimeHelpers as u64
}

pub fn get_invoke_virtual_ptr() -> u64 {
    jit_helper_invoke_virtual as u64
}

pub fn get_invoke_special_ptr() -> u64 {
    jit_helper_invoke_special as u64
}

pub fn get_invoke_static_ptr() -> u64 {
    jit_helper_invoke_static as u64
}

pub fn get_invoke_interface_ptr() -> u64 {
    jit_helper_invoke_interface as u64
}

extern "C" fn jit_helper_allocate_object(_ctx: u64, _class_ptr: u64, _size: u64, _: u64, _: u64, _: u64) -> u64 {
    println!("JIT helper: allocate_object (stub - deoptimizing)");
    0
}

extern "C" fn jit_helper_allocate_array(_ctx: u64, _class_ptr: u64, _length: u64, _: u64, _: u64, _: u64) -> u64 {
    println!("JIT helper: allocate_array (stub - deoptimizing)");
    0
}

extern "C" fn jit_helper_get_static_field(_ctx: u64, _class_ptr: u64, _field_ptr: u64, _: u64, _: u64, _: u64) -> u64 {
    println!("JIT helper: get_static_field (stub - deoptimizing)");
    0
}

extern "C" fn jit_helper_put_static_field(_ctx: u64, _class_ptr: u64, _field_ptr: u64, _value: u64, _: u64, _: u64) -> u64 {
    println!("JIT helper: put_static_field (stub - deoptimizing)");
    0
}

extern "C" fn jit_helper_get_instance_field(_ctx: u64, _obj: u64, _field_ptr: u64, _: u64, _: u64, _: u64) -> u64 {
    println!("JIT helper: get_instance_field (stub - deoptimizing)");
    0
}

extern "C" fn jit_helper_put_instance_field(_ctx: u64, _obj: u64, _field_ptr: u64, _value: u64, _: u64, _: u64) -> u64 {
    println!("JIT helper: put_instance_field (stub - deoptimizing)");
    0
}

extern "C" fn jit_helper_invoke_virtual(_ctx: u64, obj: u64, method_ptr: u64, argc: u64, _: u64, _: u64) -> u64 {
    if obj == 0 {
        println!("JIT helper: invoke_virtual - null receiver, deoptimizing");
        return 0;
    }

    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        println!("JIT helper: invoke_virtual - no VM context, deoptimizing");
        return 0;
    }

    let cp_index = method_ptr as usize;
    println!("JIT helper: invoke_virtual called (obj={}, cp_index={}, argc={}) - deoptimizing for now", obj, cp_index, argc);
    0
}

extern "C" fn jit_helper_invoke_special(_ctx: u64, obj: u64, method_ptr: u64, argc: u64, _: u64, _: u64) -> u64 {
    if obj == 0 {
        println!("JIT helper: invoke_special - null receiver, deoptimizing");
        return 0;
    }

    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        println!("JIT helper: invoke_special - no VM context, deoptimizing");
        return 0;
    }

    let cp_index = method_ptr as usize;
    println!("JIT helper: invoke_special called (obj={}, cp_index={}, argc={}) - deoptimizing for now", obj, cp_index, argc);
    0
}

extern "C" fn jit_helper_invoke_static(_ctx: u64, class_ptr: u64, method_ptr: u64, argc: u64, _: u64, _: u64) -> u64 {
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        println!("JIT helper: invoke_static - no VM context, deoptimizing");
        return 0;
    }

    let cp_index = method_ptr as usize;
    println!("JIT helper: invoke_static called (class={}, cp_index={}, argc={}) - deoptimizing for now", class_ptr, cp_index, argc);
    0
}

extern "C" fn jit_helper_invoke_interface(_ctx: u64, obj: u64, method_ptr: u64, argc: u64, _: u64, _: u64) -> u64 {
    if obj == 0 {
        println!("JIT helper: invoke_interface - null receiver, deoptimizing");
        return 0;
    }

    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        println!("JIT helper: invoke_interface - no VM context, deoptimizing");
        return 0;
    }

    let cp_index = method_ptr as usize;
    println!("JIT helper: invoke_interface called (obj={}, cp_index={}, argc={}) - deoptimizing for now", obj, cp_index, argc);
    0
}

extern "C" fn jit_helper_checkcast(_ctx: u64, _obj: u64, _class_ptr: u64, _: u64, _: u64, _: u64) -> u64 {
    println!("JIT helper: checkcast (stub - deoptimizing)");
    0
}

extern "C" fn jit_helper_instanceof(_ctx: u64, _obj: u64, _class_ptr: u64, _: u64, _: u64, _: u64) -> u64 {
    println!("JIT helper: instanceof (stub - deoptimizing)");
    0
}

extern "C" fn jit_helper_athrow(_ctx: u64, _exception: u64, _: u64, _: u64, _: u64, _: u64) -> u64 {
    println!("JIT helper: athrow (stub - deoptimizing)");
    0
}

extern "C" fn jit_helper_monitor_enter(_ctx: u64, _obj: u64, _: u64, _: u64, _: u64, _: u64) -> u64 {
    println!("JIT helper: monitor_enter (stub - deoptimizing)");
    0
}

extern "C" fn jit_helper_monitor_exit(_ctx: u64, _obj: u64, _: u64, _: u64, _: u64, _: u64) -> u64 {
    println!("JIT helper: monitor_exit (stub - deoptimizing)");
    0
}

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
        self.compiled_methods
            .write()
            .unwrap()
            .insert(method_key, code);
    }

    pub fn get_compiled_code(&self, method_key: &str) -> Option<CompiledCode> {
        self.compiled_methods
            .read()
            .unwrap()
            .get(method_key)
            .cloned()
    }

    pub fn increment_invocation_count(&mut self, method_key: &str) {
        *self
            .invocation_counts
            .entry(method_key.to_string())
            .or_insert(0) += 1;
    }

    pub fn increment_backedge_count(&mut self, method_key: &str) {
        *self
            .backedge_counts
            .entry(method_key.to_string())
            .or_insert(0) += 1;
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
    alloc_size: usize,
    frame_size: usize,
    num_slots: usize,
}

impl JitEntry {
    pub fn new(code: Vec<u8>, frame_size: usize, num_slots: usize) -> Option<Self> {
        let (code_ptr, alloc_size) = Self::make_executable(&code)?;
        Some(JitEntry {
            code_ptr,
            alloc_size,
            frame_size,
            num_slots,
        })
    }

    fn make_executable(code: &[u8]) -> Option<(usize, usize)> {
        let size = code.len();
        if size == 0 {
            return None;
        }

        let page_size = 4096;
        let alloc_size = ((size + page_size - 1) / page_size) * page_size;

        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        unsafe {
            use std::ptr;

            let ptr = libc::mmap(
                ptr::null_mut(),
                alloc_size,
                libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_JIT,
                -1,
                0,
            );
            if ptr == libc::MAP_FAILED {
                return None;
            }

            libc::pthread_jit_write_protect_np(0);
            ptr::copy_nonoverlapping(code.as_ptr(), ptr as *mut u8, size);
            libc::pthread_jit_write_protect_np(1);
            sys_icache_invalidate(ptr, alloc_size);

            return Some((ptr as usize, alloc_size));
        }

        #[cfg(all(unix, not(all(target_os = "macos", target_arch = "aarch64"))))]
        unsafe {
            use std::ptr;

            let ptr = libc::mmap(
                ptr::null_mut(),
                alloc_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            );
            if ptr == libc::MAP_FAILED {
                return None;
            }

            ptr::copy_nonoverlapping(code.as_ptr(), ptr as *mut u8, size);

            if libc::mprotect(ptr, alloc_size, libc::PROT_READ | libc::PROT_EXEC) != 0 {
                libc::munmap(ptr, alloc_size);
                return None;
            }

            #[cfg(target_arch = "aarch64")]
            {
                let start = ptr as usize;
                let end = start + alloc_size;
                std::arch::asm!(
                    "0: dc cvau, {addr}",
                    "   add {addr}, {addr}, {line}",
                    "   cmp {addr}, {end}",
                    "   b.lo 0b",
                    "   dsb ish",
                    "1: ic ivau, {start}",
                    "   add {start}, {start}, {line}",
                    "   cmp {start}, {end}",
                    "   b.lo 1b",
                    "   dsb ish",
                    "   isb",
                    addr = inout(reg) start as u64 => _,
                    start = inout(reg) start as u64 => _,
                    end = in(reg) end as u64,
                    line = in(reg) 64u64,
                );
            }

            return Some((ptr as usize, alloc_size));
        }

        #[cfg(not(unix))]
        {
            let _ = (size, alloc_size);
            None
        }
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
        unsafe {
            libc::munmap(self.code_ptr as *mut libc::c_void, self.alloc_size);
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
        initialize_jit_helpers();
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

    pub fn execute(
        &self,
        key: &str,
        args: &[crate::vm::types::Value],
    ) -> Option<crate::vm::types::Value> {
        self.execute_typed(key, args, JitReturn::Int)
    }

    pub fn execute_typed(
        &self,
        key: &str,
        args: &[crate::vm::types::Value],
        ret: JitReturn,
    ) -> Option<crate::vm::types::Value> {
        let entry = self.entries.get(key)?;
        let fn_ptr = entry.code_ptr();
        let _ = entry.frame_size();

        let mut int_args: [u64; 6] = [0; 6];
        let mut float_args: [f64; 8] = [0.0; 8];
        let mut int_count = 1;
        let mut float_count = 0;

        // First int slot is the JIT context pointer (currently 0 — runtime helpers
        // read the live VM through the thread-local in `set_current_vm`).
        int_args[0] = 0;

        for arg in args {
            match arg {
                crate::vm::types::Value::Int(v) => {
                    if int_count < 6 {
                        int_args[int_count] = *v as u32 as u64;
                    }
                    int_count += 1;
                }
                crate::vm::types::Value::Long(v) => {
                    if int_count < 6 {
                        int_args[int_count] = *v as u64;
                    }
                    int_count += 1;
                }
                crate::vm::types::Value::Float(v) => {
                    if float_count < 8 {
                        float_args[float_count] = *v as f64;
                    }
                    float_count += 1;
                }
                crate::vm::types::Value::Double(v) => {
                    if float_count < 8 {
                        float_args[float_count] = *v;
                    }
                    float_count += 1;
                }
                crate::vm::types::Value::Reference(r) => {
                    let ptr = match r {
                        crate::vm::types::Reference::Null => 0usize,
                        crate::vm::types::Reference::Heap(addr) => *addr,
                    };
                    if int_count < 6 {
                        int_args[int_count] = ptr as u64;
                    }
                    int_count += 1;
                }
                crate::vm::types::Value::ReturnAddress(_) => {}
            }
        }

        unsafe {
            match ret {
                JitReturn::Void => {
                    let f: extern "C" fn(u64, u64, u64, u64, u64, u64) =
                        std::mem::transmute(fn_ptr);
                    f(int_args[0], int_args[1], int_args[2], int_args[3], int_args[4], int_args[5]);
                    Some(crate::vm::types::Value::Int(0))
                }
                JitReturn::Int => {
                    let f: extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64 =
                        std::mem::transmute(fn_ptr);
                    let r = f(int_args[0], int_args[1], int_args[2], int_args[3], int_args[4], int_args[5]);
                    Some(crate::vm::types::Value::Int(r as i32))
                }
                JitReturn::Long => {
                    let f: extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64 =
                        std::mem::transmute(fn_ptr);
                    let r = f(int_args[0], int_args[1], int_args[2], int_args[3], int_args[4], int_args[5]);
                    Some(crate::vm::types::Value::Long(r as i64))
                }
                JitReturn::Float => {
                    let f: extern "C" fn(u64, u64, u64, u64, u64, u64) -> f32 =
                        std::mem::transmute(fn_ptr);
                    let r = f(int_args[0], int_args[1], int_args[2], int_args[3], int_args[4], int_args[5]);
                    Some(crate::vm::types::Value::Float(r))
                }
                JitReturn::Double => {
                    let f: extern "C" fn(u64, u64, u64, u64, u64, u64) -> f64 =
                        std::mem::transmute(fn_ptr);
                    let r = f(int_args[0], int_args[1], int_args[2], int_args[3], int_args[4], int_args[5]);
                    Some(crate::vm::types::Value::Double(r))
                }
                JitReturn::Reference => {
                    let f: extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64 =
                        std::mem::transmute(fn_ptr);
                    let r = f(int_args[0], int_args[1], int_args[2], int_args[3], int_args[4], int_args[5]);
                    let r_ref = if r == 0 {
                        crate::vm::types::Reference::Null
                    } else {
                        crate::vm::types::Reference::Heap(r as usize)
                    };
                    Some(crate::vm::types::Value::Reference(r_ref))
                }
            }
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum JitReturn {
    Void,
    Int,
    Long,
    Float,
    Double,
    Reference,
}

impl JitReturn {
    pub fn from_descriptor(descriptor: &str) -> Self {
        let ret_idx = descriptor.rfind(')').map(|i| i + 1).unwrap_or(0);
        match descriptor.as_bytes().get(ret_idx).copied() {
            Some(b'V') => JitReturn::Void,
            Some(b'J') => JitReturn::Long,
            Some(b'F') => JitReturn::Float,
            Some(b'D') => JitReturn::Double,
            Some(b'L') | Some(b'[') => JitReturn::Reference,
            _ => JitReturn::Int,
        }
    }
}

impl Default for JitContext {
    fn default() -> Self {
        Self::new()
    }
}
