use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::RwLock;

#[cfg(unix)]
use libc;

use super::CompiledCode;
use crate::vm::heap::Heap;
use crate::vm::{FieldRef, HeapValue, InvokeDynamicSite, MethodRef, Reference, Value, Vm};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
unsafe extern "C" {
    fn sys_icache_invalidate(start: *mut libc::c_void, len: libc::size_t);
}

thread_local! {
    static CURRENT_VM: std::cell::UnsafeCell<u64> = std::cell::UnsafeCell::new(0);
    static PENDING_JIT_EXCEPTION: std::cell::UnsafeCell<u64> = std::cell::UnsafeCell::new(0);
    static LAST_DEOPT_SNAPSHOT: std::cell::RefCell<Option<DeoptSnapshot>> = const { std::cell::RefCell::new(None) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DeoptReason {
    GuardFailure,
    NullCheck,
    ClassCast,
    MonitorFailure,
    AllocationFailure,
    HelperUnsupported,
    Exception,
    SiteFallback,
}

#[derive(Clone)]
pub struct DeoptSnapshot {
    pub reason: Option<DeoptReason>,
    pub pc: usize,
    pub locals: Vec<u64>,
    pub stack: Vec<u64>,
}

pub fn set_current_vm(vm_ptr: u64) {
    CURRENT_VM.with(|cell| unsafe {
        *cell.get() = vm_ptr;
    });
}

pub fn clear_current_vm() {
    CURRENT_VM.with(|cell| unsafe {
        *cell.get() = 0;
    });
}

pub fn get_current_vm_ptr() -> u64 {
    CURRENT_VM.with(|cell| unsafe { *cell.get() })
}

pub fn clear_pending_jit_exception() {
    PENDING_JIT_EXCEPTION.with(|cell| unsafe {
        *cell.get() = 0;
    });
}

pub fn clear_last_deopt_snapshot() {
    LAST_DEOPT_SNAPSHOT.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

pub fn take_last_deopt_snapshot() -> Option<DeoptSnapshot> {
    LAST_DEOPT_SNAPSHOT.with(|cell| cell.borrow_mut().take())
}

fn set_last_deopt_snapshot(snapshot: DeoptSnapshot) {
    LAST_DEOPT_SNAPSHOT.with(|cell| {
        *cell.borrow_mut() = Some(snapshot);
    });
}

pub fn take_pending_jit_exception() -> Option<Reference> {
    PENDING_JIT_EXCEPTION.with(|cell| unsafe {
        let raw = *cell.get();
        *cell.get() = 0;
        decode_optional_reference(raw)
    })
}

fn set_pending_jit_exception(exception: u64) {
    PENDING_JIT_EXCEPTION.with(|cell| unsafe {
        *cell.get() = exception;
    });
}

fn raise_pending_exception(vm: &mut Vm, class_name: &str) -> u64 {
    let exception = vm.heap.lock().unwrap().allocate(HeapValue::Object {
        class_name: class_name.to_string(),
        fields: HashMap::new(),
    });
    let raw = encode_reference(exception);
    set_pending_jit_exception(raw);
    raw
}

fn record_pending_deopt_pc(ctx: u64, pc: u64) {
    if ctx == 0 {
        return;
    }
    unsafe {
        *(ctx as *mut u64) = (*(ctx as *mut u64) & DEOPT_REASON_MASK) | DEOPT_PENDING_MARKER | pc;
    }
}

fn record_deopt_request(ctx: u64, reason: DeoptReason) {
    if ctx == 0 {
        return;
    }
    unsafe {
        let slot = ctx as *mut u64;
        *slot = (*slot & !DEOPT_REASON_MASK) | DEOPT_PENDING_MARKER | encode_deopt_reason(reason);
    }
}

fn encode_deopt_reason(reason: DeoptReason) -> u64 {
    (match reason {
        DeoptReason::GuardFailure => 1,
        DeoptReason::NullCheck => 2,
        DeoptReason::ClassCast => 3,
        DeoptReason::MonitorFailure => 4,
        DeoptReason::AllocationFailure => 5,
        DeoptReason::HelperUnsupported => 6,
        DeoptReason::Exception => 7,
        DeoptReason::SiteFallback => 8,
    } as u64)
        << DEOPT_REASON_SHIFT
}

fn decode_deopt_reason(raw: u64) -> Option<DeoptReason> {
    match (raw & DEOPT_REASON_MASK) >> DEOPT_REASON_SHIFT {
        1 => Some(DeoptReason::GuardFailure),
        2 => Some(DeoptReason::NullCheck),
        3 => Some(DeoptReason::ClassCast),
        4 => Some(DeoptReason::MonitorFailure),
        5 => Some(DeoptReason::AllocationFailure),
        6 => Some(DeoptReason::HelperUnsupported),
        7 => Some(DeoptReason::Exception),
        8 => Some(DeoptReason::SiteFallback),
        _ => None,
    }
}

fn encode_reference(reference: Reference) -> u64 {
    match reference {
        Reference::Null => 0,
        Reference::Heap(index) => index as u64 + HEAP_REF_BIAS,
    }
}

fn decode_optional_reference(raw: u64) -> Option<Reference> {
    if raw == 0 {
        None
    } else {
        Some(Reference::Heap((raw - HEAP_REF_BIAS) as usize))
    }
}

fn decode_reference(raw: u64) -> Reference {
    decode_optional_reference(raw).unwrap_or(Reference::Null)
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
static JIT_ARRAY_DESCRIPTORS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
static JIT_CLASS_NAMES: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
static JIT_FIELD_REFS: OnceLock<Mutex<Vec<FieldRef>>> = OnceLock::new();
static JIT_METHOD_REFS: OnceLock<Mutex<Vec<MethodRef>>> = OnceLock::new();
static JIT_INVOKE_DYNAMIC_SITES: OnceLock<Mutex<Vec<InvokeDynamicSite>>> = OnceLock::new();
const INLINE_ARG_MARKER: u64 = 1u64 << 63;
const DEOPT_PENDING_MARKER: u64 = 1u64 << 63;
const DEOPT_REASON_SHIFT: u64 = 56;
const DEOPT_REASON_MASK: u64 = 0x7f << DEOPT_REASON_SHIFT;
const HEAP_REF_BIAS: u64 = 1;

const ARRAY_KIND_PRIMITIVE: u64 = 1;
const ARRAY_KIND_REFERENCE: u64 = 2;
const ARRAY_KIND_MULTI: u64 = 3;

pub extern "C" fn jit_helper_load_reference_array_element(
    ctx: u64,
    array_ref: u64,
    index: u64,
    pc: u64,
    _: u64,
    _: u64,
) -> u64 {
    let Some(array_ref) = decode_optional_reference(array_ref) else {
        record_deopt_request(ctx, DeoptReason::Exception);
        let vm_ptr = get_current_vm_ptr();
        if vm_ptr != 0 {
            unsafe {
                let vm = &mut *(vm_ptr as *mut Vm);
                raise_pending_exception(vm, "java/lang/NullPointerException");
                set_current_vm(vm_ptr);
            }
        }
        println!("JIT helper: load_reference_array_element - null array, pending NPE");
        return 0;
    };
    let index = index as i32;
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!("JIT helper: load_reference_array_element - no VM context, deoptimizing");
        return 0;
    }
    let result = unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        let result = vm
            .heap
            .lock()
            .unwrap()
            .load_reference_array_element(array_ref, index);
        set_current_vm(vm_ptr);
        match result {
            Ok(reference) => encode_reference(reference),
            Err(e) => {
                record_pending_deopt_pc(ctx, pc);
                record_deopt_request(ctx, DeoptReason::Exception);
                let class_name = match e {
                    crate::vm::VmError::ArrayIndexOutOfBounds { .. } => {
                        "java/lang/ArrayIndexOutOfBoundsException"
                    }
                    crate::vm::VmError::NullReference => "java/lang/NullPointerException",
                    _ => "java/lang/ArrayIndexOutOfBoundsException",
                };
                raise_pending_exception(vm, class_name);
                println!("JIT helper: load_reference_array_element failed: {:?}", e);
                0
            }
        }
    };
    result
}

pub fn get_load_reference_array_element_ptr() -> u64 {
    jit_helper_load_reference_array_element as u64
}

pub extern "C" fn jit_helper_store_reference_array_element(
    ctx: u64,
    array_ref: u64,
    index: u64,
    value: u64,
    pc: u64,
    _: u64,
) -> u64 {
    let Some(array_ref) = decode_optional_reference(array_ref) else {
        record_deopt_request(ctx, DeoptReason::Exception);
        let vm_ptr = get_current_vm_ptr();
        if vm_ptr != 0 {
            unsafe {
                let vm = &mut *(vm_ptr as *mut Vm);
                raise_pending_exception(vm, "java/lang/NullPointerException");
                set_current_vm(vm_ptr);
            }
        }
        println!("JIT helper: store_reference_array_element - null array, pending NPE");
        return 0;
    };
    let index = index as i32;
    let value = decode_reference(value);
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!("JIT helper: store_reference_array_element - no VM context, deoptimizing");
        return 0;
    }
    unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        let result = vm
            .heap
            .lock()
            .unwrap()
            .store_reference_array_element(array_ref, index, value);
        set_current_vm(vm_ptr);
        match result {
            Ok(()) => 1,
            Err(e) => {
                record_pending_deopt_pc(ctx, pc);
                record_deopt_request(ctx, DeoptReason::Exception);
                let class_name = match e {
                    crate::vm::VmError::ArrayIndexOutOfBounds { .. } => {
                        "java/lang/ArrayIndexOutOfBoundsException"
                    }
                    crate::vm::VmError::NullReference => "java/lang/NullPointerException",
                    _ => "java/lang/ArrayIndexOutOfBoundsException",
                };
                raise_pending_exception(vm, class_name);
                println!("JIT helper: store_reference_array_element failed: {:?}", e);
                0
            }
        }
    }
}

pub fn get_store_reference_array_element_ptr() -> u64 {
    jit_helper_store_reference_array_element as u64
}

pub extern "C" fn jit_helper_array_length(
    ctx: u64,
    array_ref: u64,
    pc: u64,
    _: u64,
    _: u64,
    _: u64,
) -> u64 {
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 || array_ref == 0 {
        record_deopt_request(ctx, DeoptReason::Exception);
        if vm_ptr != 0 {
            unsafe {
                let vm = &mut *(vm_ptr as *mut Vm);
                raise_pending_exception(vm, "java/lang/NullPointerException");
                set_current_vm(vm_ptr);
            }
        }
        println!("JIT helper: array_length - missing VM context or null array");
        return 0;
    }
    unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        let result = vm
            .heap
            .lock()
            .unwrap()
            .array_length(Reference::Heap((array_ref - HEAP_REF_BIAS) as usize));
        set_current_vm(vm_ptr);
        match result {
            Ok(len) => len as u64,
            Err(e) => {
                record_pending_deopt_pc(ctx, pc);
                record_deopt_request(ctx, DeoptReason::Exception);
                let class_name = match e {
                    crate::vm::VmError::ArrayIndexOutOfBounds { .. } => {
                        "java/lang/ArrayIndexOutOfBoundsException"
                    }
                    crate::vm::VmError::NullReference => "java/lang/NullPointerException",
                    _ => "java/lang/NullPointerException",
                };
                raise_pending_exception(vm, class_name);
                println!("JIT helper: array_length failed: {:?}", e);
                0
            }
        }
    }
}

pub fn get_array_length_ptr() -> u64 {
    jit_helper_array_length as u64
}

extern "C" fn jit_helper_raise_exception_class(
    _ctx: u64,
    class_id: u64,
    _: u64,
    _: u64,
    _: u64,
    _: u64,
) -> u64 {
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        record_deopt_request(_ctx, DeoptReason::HelperUnsupported);
        println!("JIT helper: raise_exception_class - no VM context, deoptimizing");
        return 0;
    }
    let Some(class_name) = get_registered_class_name(class_id as usize) else {
        record_deopt_request(_ctx, DeoptReason::HelperUnsupported);
        println!(
            "JIT helper: raise_exception_class - missing class id {}",
            class_id
        );
        return 0;
    };
    unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        let exception = raise_pending_exception(vm, &class_name);
        set_current_vm(vm_ptr);
        exception
    }
}

pub fn get_raise_exception_class_ptr() -> u64 {
    jit_helper_raise_exception_class as u64
}

pub extern "C" fn jit_helper_load_typed_array_element(
    ctx: u64,
    array_ref: u64,
    index: u64,
    _type_marker: u64,
    pc: u64,
    _: u64,
) -> u64 {
    let Some(array_ref) = decode_optional_reference(array_ref) else {
        record_deopt_request(ctx, DeoptReason::Exception);
        let vm_ptr = get_current_vm_ptr();
        if vm_ptr != 0 {
            unsafe {
                let vm = &mut *(vm_ptr as *mut Vm);
                raise_pending_exception(vm, "java/lang/NullPointerException");
                set_current_vm(vm_ptr);
            }
        }
        println!("JIT helper: load_typed_array_element - null array, pending NPE");
        return 0;
    };
    let index = index as i32;
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!("JIT helper: load_typed_array_element - no VM context, deoptimizing");
        return 0;
    }
    unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        let outcome = {
            let heap = vm.heap.lock().unwrap();
            match heap.get(array_ref) {
                Ok(HeapValue::DoubleArray { values }) => match Heap::check_array_index(index, values.len()) {
                    Ok(i) => Ok(values[i].to_bits() as u64),
                    Err(_) => Err("java/lang/ArrayIndexOutOfBoundsException"),
                },
                Ok(HeapValue::LongArray { values }) => match Heap::check_array_index(index, values.len()) {
                    Ok(i) => Ok(values[i] as u64),
                    Err(_) => Err("java/lang/ArrayIndexOutOfBoundsException"),
                },
                Ok(HeapValue::IntArray { values }) => match Heap::check_array_index(index, values.len()) {
                    Ok(i) => Ok(values[i] as u32 as u64),
                    Err(_) => Err("java/lang/ArrayIndexOutOfBoundsException"),
                },
                Ok(HeapValue::FloatArray { values }) => match Heap::check_array_index(index, values.len()) {
                    Ok(i) => Ok(values[i].to_bits() as u64),
                    Err(_) => Err("java/lang/ArrayIndexOutOfBoundsException"),
                },
                Err(_) => Err("java/lang/NullPointerException"),
                _ => Err("java/lang/ArrayIndexOutOfBoundsException"),
            }
        };
        set_current_vm(vm_ptr);
        match outcome {
            Ok(value) => value,
            Err(class_name) => {
                record_pending_deopt_pc(ctx, pc);
                record_deopt_request(ctx, DeoptReason::Exception);
                raise_pending_exception(vm, class_name);
                println!(
                    "JIT helper: load_typed_array_element raised pending exception {}",
                    class_name
                );
                0
            }
        }
    }
}

pub fn get_load_typed_array_element_ptr() -> u64 {
    jit_helper_load_typed_array_element as u64
}

pub extern "C" fn jit_helper_store_typed_array_element(
    ctx: u64,
    array_ref: u64,
    index: u64,
    value: u64,
    pc: u64,
    _: u64,
) -> u64 {
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 || array_ref == 0 {
        if vm_ptr != 0 {
            unsafe {
                let vm = &mut *(vm_ptr as *mut Vm);
                raise_pending_exception(vm, "java/lang/NullPointerException");
                set_current_vm(vm_ptr);
            }
        }
        println!("JIT helper: store_typed_array_element - missing VM context or null array");
        return 0;
    }

    unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        let reference = Reference::Heap((array_ref - HEAP_REF_BIAS) as usize);
        let index = index as i32;
        let result = {
            let mut heap = vm.heap.lock().unwrap();
            match heap.get_mut(reference) {
                Ok(HeapValue::IntArray { values }) => {
                    let i = Heap::check_array_index(index, values.len());
                    i.map(|i| values[i] = value as i32).map_err(|e| e)
                }
                Ok(HeapValue::LongArray { values }) => {
                    let i = Heap::check_array_index(index, values.len());
                    i.map(|i| values[i] = value as i64).map_err(|e| e)
                }
                Ok(HeapValue::FloatArray { values }) => {
                    let i = Heap::check_array_index(index, values.len());
                    i.map(|i| values[i] = f32::from_bits(value as u32))
                        .map_err(|e| e)
                }
                Ok(HeapValue::DoubleArray { values }) => {
                    let i = Heap::check_array_index(index, values.len());
                    i.map(|i| values[i] = f64::from_bits(value)).map_err(|e| e)
                }
                Ok(value) => Err(crate::vm::VmError::InvalidHeapValue {
                    expected: "primitive-array",
                    actual: value.kind_name(),
                }),
                Err(e) => Err(e),
            }
        };
        set_current_vm(vm_ptr);
        match result {
            Ok(()) => 1,
            Err(e) => {
                record_pending_deopt_pc(ctx, pc);
                record_deopt_request(ctx, DeoptReason::Exception);
                let class_name = match e {
                    crate::vm::VmError::ArrayIndexOutOfBounds { .. } => {
                        "java/lang/ArrayIndexOutOfBoundsException"
                    }
                    crate::vm::VmError::NullReference => "java/lang/NullPointerException",
                    _ => "java/lang/ArrayIndexOutOfBoundsException",
                };
                raise_pending_exception(vm, class_name);
                println!("JIT helper: store_typed_array_element failed: {:?}", e);
                0
            }
        }
    }
}

pub fn get_store_typed_array_element_ptr() -> u64 {
    jit_helper_store_typed_array_element as u64
}

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

pub fn get_allocate_array_ptr() -> u64 {
    jit_helper_allocate_array as u64
}

pub fn get_allocate_object_ptr() -> u64 {
    jit_helper_allocate_object as u64
}

pub fn get_checkcast_ptr() -> u64 {
    jit_helper_checkcast as u64
}

pub fn get_force_deopt_ptr() -> u64 {
    jit_helper_force_deopt as u64
}

pub fn get_instanceof_ptr() -> u64 {
    jit_helper_instanceof as u64
}

pub fn get_athrow_ptr() -> u64 {
    jit_helper_athrow as u64
}

pub fn get_monitor_enter_ptr() -> u64 {
    jit_helper_monitor_enter as u64
}

pub fn get_monitor_exit_ptr() -> u64 {
    jit_helper_monitor_exit as u64
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

pub fn get_invoke_dynamic_ptr() -> u64 {
    jit_helper_invoke_dynamic as u64
}

pub fn get_invoke_native_ptr() -> u64 {
    jit_helper_invoke_native as u64
}

pub fn get_get_static_field_ptr() -> u64 {
    jit_helper_get_static_field as u64
}

pub fn get_put_static_field_ptr() -> u64 {
    jit_helper_put_static_field as u64
}

pub fn get_get_instance_field_ptr() -> u64 {
    jit_helper_get_instance_field as u64
}

pub fn get_put_instance_field_ptr() -> u64 {
    jit_helper_put_instance_field as u64
}

pub fn register_array_descriptor(descriptor: impl Into<String>) -> u64 {
    let descriptors = JIT_ARRAY_DESCRIPTORS.get_or_init(|| Mutex::new(Vec::new()));
    let mut descriptors = descriptors.lock().unwrap();
    register_unique(&mut descriptors, descriptor.into())
}

fn get_registered_array_descriptor(index: usize) -> Option<String> {
    JIT_ARRAY_DESCRIPTORS
        .get()
        .and_then(|descriptors| descriptors.lock().ok()?.get(index).cloned())
}

pub fn register_class_name(class_name: impl Into<String>) -> u64 {
    let class_names = JIT_CLASS_NAMES.get_or_init(|| Mutex::new(Vec::new()));
    let mut class_names = class_names.lock().unwrap();
    register_unique(&mut class_names, class_name.into())
}

fn get_registered_class_name(index: usize) -> Option<String> {
    JIT_CLASS_NAMES
        .get()
        .and_then(|class_names| class_names.lock().ok()?.get(index).cloned())
}

pub fn register_field_ref(field_ref: FieldRef) -> u64 {
    let refs = JIT_FIELD_REFS.get_or_init(|| Mutex::new(Vec::new()));
    let mut refs = refs.lock().unwrap();
    register_unique(&mut refs, field_ref)
}

fn get_registered_field_ref(index: usize) -> Option<FieldRef> {
    JIT_FIELD_REFS
        .get()
        .and_then(|refs| refs.lock().ok()?.get(index).cloned())
}

pub fn register_method_ref(method_ref: MethodRef) -> u64 {
    let refs = JIT_METHOD_REFS.get_or_init(|| Mutex::new(Vec::new()));
    let mut refs = refs.lock().unwrap();
    register_unique(&mut refs, method_ref)
}

fn get_registered_method_ref(index: usize) -> Option<MethodRef> {
    JIT_METHOD_REFS
        .get()
        .and_then(|refs| refs.lock().ok()?.get(index).cloned())
}

pub fn register_invoke_dynamic_site(site: InvokeDynamicSite) -> u64 {
    let sites = JIT_INVOKE_DYNAMIC_SITES.get_or_init(|| Mutex::new(Vec::new()));
    let mut sites = sites.lock().unwrap();
    register_unique(&mut sites, site)
}

fn register_unique<T: PartialEq>(items: &mut Vec<T>, item: T) -> u64 {
    if let Some(index) = items.iter().position(|existing| *existing == item) {
        index as u64
    } else {
        items.push(item);
        (items.len() - 1) as u64
    }
}

fn get_registered_invoke_dynamic_site(index: usize) -> Option<InvokeDynamicSite> {
    JIT_INVOKE_DYNAMIC_SITES
        .get()
        .and_then(|sites| sites.lock().ok()?.get(index).cloned())
}

fn value_to_u64(value: Option<Value>) -> u64 {
    match value {
        Some(Value::Int(v)) => v as u32 as u64,
        Some(Value::Long(v)) => v as u64,
        Some(Value::Float(v)) => v.to_bits() as u64,
        Some(Value::Double(v)) => v.to_bits(),
        Some(Value::Reference(reference)) => encode_reference(reference),
        None => 0,
        Some(Value::ReturnAddress(pc)) => pc as u64,
    }
}

fn decode_helper_args(argc: u64, arg0: u64, arg1: u64, arg2: u64) -> (usize, [u64; 3], bool) {
    if argc & INLINE_ARG_MARKER != 0 {
        let real_argc = (argc & !INLINE_ARG_MARKER) as usize;
        (real_argc, [arg0, arg1, arg2], true)
    } else {
        (argc as usize, [arg0, 0, 0], false)
    }
}

fn decode_array_counts(argc: u64, arg0: u64, arg1: u64) -> (usize, [u64; 2], bool) {
    if argc & INLINE_ARG_MARKER != 0 {
        let real_argc = (argc & !INLINE_ARG_MARKER) as usize;
        (real_argc, [arg0, arg1], true)
    } else {
        (argc as usize, [arg0, 0], false)
    }
}

extern "C" fn jit_helper_allocate_object(
    ctx: u64,
    class_id: u64,
    _size: u64,
    _: u64,
    _: u64,
    _: u64,
) -> u64 {
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        record_deopt_request(ctx, DeoptReason::AllocationFailure);
        println!("JIT helper: allocate_object - no VM context, deoptimizing");
        return 0;
    }

    let Some(class_name) = get_registered_class_name(class_id as usize) else {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!(
            "JIT helper: allocate_object - missing class id {}",
            class_id
        );
        return 0;
    };

    let result = unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        let result = vm.invoke_jit_allocate_object(&class_name);
        set_current_vm(vm_ptr);
        result
    };

    if result.is_none() {
        record_deopt_request(ctx, DeoptReason::AllocationFailure);
    }
    value_to_u64(result.map(Value::Reference))
}

extern "C" fn jit_helper_allocate_array(
    ctx: u64,
    kind: u64,
    descriptor_or_atype: u64,
    argc: u64,
    arg0: u64,
    arg1: u64,
) -> u64 {
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        record_deopt_request(ctx, DeoptReason::AllocationFailure);
        println!("JIT helper: allocate_array - no VM context, deoptimizing");
        return 0;
    }

    let (argc, inline_args, inline) = decode_array_counts(argc, arg0, arg1);
    let counts = if inline {
        inline_args[..argc].to_vec()
    } else {
        unsafe {
            let ptr = inline_args[0] as *const u64;
            (0..argc)
                .map(|index| std::ptr::read_unaligned(ptr.add(index)))
                .collect()
        }
    };

    let result = unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        let result = match kind {
            ARRAY_KIND_PRIMITIVE => vm.invoke_jit_allocate_primitive_array(
                descriptor_or_atype as u8,
                counts.first().copied().unwrap_or(0),
            ),
            ARRAY_KIND_REFERENCE => {
                let Some(component_type) =
                    get_registered_array_descriptor(descriptor_or_atype as usize)
                else {
                    record_deopt_request(ctx, DeoptReason::HelperUnsupported);
                    println!(
                        "JIT helper: allocate_array - missing component descriptor {}",
                        descriptor_or_atype
                    );
                    return 0;
                };
                vm.invoke_jit_allocate_reference_array(
                    &component_type,
                    counts.first().copied().unwrap_or(0),
                )
            }
            ARRAY_KIND_MULTI => {
                let Some(descriptor) =
                    get_registered_array_descriptor(descriptor_or_atype as usize)
                else {
                    record_deopt_request(ctx, DeoptReason::HelperUnsupported);
                    println!(
                        "JIT helper: allocate_array - missing array descriptor {}",
                        descriptor_or_atype
                    );
                    return 0;
                };
                vm.invoke_jit_allocate_multi_array(&descriptor, &counts)
            }
            _ => None,
        };
        set_current_vm(vm_ptr);
        result
    };

    if result.is_none() {
        record_deopt_request(ctx, DeoptReason::AllocationFailure);
    }
    value_to_u64(result.map(Value::Reference))
}

extern "C" fn jit_helper_get_static_field(
    ctx: u64,
    field_ref_id: u64,
    _: u64,
    _: u64,
    _: u64,
    _: u64,
) -> u64 {
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!("JIT helper: get_static_field - no VM context, deoptimizing");
        return 0;
    }

    let Some(field_ref) = get_registered_field_ref(field_ref_id as usize) else {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!(
            "JIT helper: get_static_field - missing field ref {}",
            field_ref_id
        );
        return 0;
    };

    let result = unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        let result = vm.invoke_jit_get_static_field_ref(&field_ref);
        set_current_vm(vm_ptr);
        result
    };

    if result.is_none() {
        record_deopt_request(ctx, DeoptReason::GuardFailure);
    }
    value_to_u64(result)
}

extern "C" fn jit_helper_put_static_field(
    ctx: u64,
    field_ref_id: u64,
    value: u64,
    _: u64,
    _: u64,
    _: u64,
) -> u64 {
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!("JIT helper: put_static_field - no VM context, deoptimizing");
        return 0;
    }

    let Some(field_ref) = get_registered_field_ref(field_ref_id as usize) else {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!(
            "JIT helper: put_static_field - missing field ref {}",
            field_ref_id
        );
        return 0;
    };

    unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        if !vm.invoke_jit_put_static_field_ref(&field_ref, value) {
            record_deopt_request(ctx, DeoptReason::GuardFailure);
        }
        set_current_vm(vm_ptr);
    }

    0
}

extern "C" fn jit_helper_get_instance_field(
    ctx: u64,
    obj: u64,
    field_ref_id: u64,
    _: u64,
    _: u64,
    _: u64,
) -> u64 {
    if obj == 0 {
        record_deopt_request(ctx, DeoptReason::NullCheck);
        println!("JIT helper: get_instance_field - null receiver, deoptimizing");
        return 0;
    }

    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!("JIT helper: get_instance_field - no VM context, deoptimizing");
        return 0;
    }

    let Some(field_ref) = get_registered_field_ref(field_ref_id as usize) else {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!(
            "JIT helper: get_instance_field - missing field ref {}",
            field_ref_id
        );
        return 0;
    };

    let result = unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        let result = vm.invoke_jit_get_instance_field_ref(&field_ref, obj);
        set_current_vm(vm_ptr);
        result
    };

    if result.is_none() {
        record_deopt_request(ctx, DeoptReason::GuardFailure);
    }
    value_to_u64(result)
}

extern "C" fn jit_helper_put_instance_field(
    ctx: u64,
    obj: u64,
    field_ref_id: u64,
    value: u64,
    _: u64,
    _: u64,
) -> u64 {
    if obj == 0 {
        record_deopt_request(ctx, DeoptReason::NullCheck);
        println!("JIT helper: put_instance_field - null receiver, deoptimizing");
        return 0;
    }

    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!("JIT helper: put_instance_field - no VM context, deoptimizing");
        return 0;
    }

    let Some(field_ref) = get_registered_field_ref(field_ref_id as usize) else {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!(
            "JIT helper: put_instance_field - missing field ref {}",
            field_ref_id
        );
        return 0;
    };

    unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        if !vm.invoke_jit_put_instance_field_ref(&field_ref, obj, value) {
            record_deopt_request(ctx, DeoptReason::GuardFailure);
        }
        set_current_vm(vm_ptr);
    }

    0
}

extern "C" fn jit_helper_invoke_virtual(
    ctx: u64,
    obj: u64,
    method_ref_id: u64,
    argc: u64,
    arg0: u64,
    arg1: u64,
) -> u64 {
    if obj == 0 {
        record_deopt_request(ctx, DeoptReason::NullCheck);
        println!("JIT helper: invoke_virtual - null receiver, deoptimizing");
        return 0;
    }

    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!("JIT helper: invoke_virtual - no VM context, deoptimizing");
        return 0;
    }

    let Some(method_ref) = get_registered_method_ref(method_ref_id as usize) else {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!(
            "JIT helper: invoke_virtual - missing method ref {}",
            method_ref_id
        );
        return 0;
    };

    let result = unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        let (argc, inline_args, inline) = decode_helper_args(argc, arg0, arg1, 0);
        let args_ptr = if inline {
            inline_args.as_ptr() as u64
        } else {
            inline_args[0]
        };
        let result = vm.invoke_jit_virtual_method_ref(&method_ref, obj, args_ptr, argc);
        set_current_vm(vm_ptr);
        result
    };

    if result.is_none() && !method_ref.descriptor.ends_with('V') {
        record_deopt_request(ctx, DeoptReason::GuardFailure);
    }
    value_to_u64(result)
}

extern "C" fn jit_helper_invoke_special(
    ctx: u64,
    obj: u64,
    method_ref_id: u64,
    argc: u64,
    arg0: u64,
    arg1: u64,
) -> u64 {
    if obj == 0 {
        record_deopt_request(ctx, DeoptReason::NullCheck);
        println!("JIT helper: invoke_special - null receiver, deoptimizing");
        return 0;
    }

    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!("JIT helper: invoke_special - no VM context, deoptimizing");
        return 0;
    }

    let Some(method_ref) = get_registered_method_ref(method_ref_id as usize) else {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!(
            "JIT helper: invoke_special - missing method ref {}",
            method_ref_id
        );
        return 0;
    };

    let result = unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        let (argc, inline_args, inline) = decode_helper_args(argc, arg0, arg1, 0);
        let args_ptr = if inline {
            inline_args.as_ptr() as u64
        } else {
            inline_args[0]
        };
        let result = vm.invoke_jit_special_method_ref(&method_ref, obj, args_ptr, argc);
        set_current_vm(vm_ptr);
        result
    };

    if result.is_none() && !method_ref.descriptor.ends_with('V') {
        record_deopt_request(ctx, DeoptReason::GuardFailure);
    }
    value_to_u64(result)
}

extern "C" fn jit_helper_invoke_static(
    ctx: u64,
    method_ref_id: u64,
    argc: u64,
    arg0: u64,
    arg1: u64,
    arg2: u64,
) -> u64 {
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!("JIT helper: invoke_static - no VM context, deoptimizing");
        return 0;
    }

    let Some(method_ref) = get_registered_method_ref(method_ref_id as usize) else {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!(
            "JIT helper: invoke_static - missing method ref {}",
            method_ref_id
        );
        return 0;
    };

    let result = unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        let (argc, inline_args, inline) = decode_helper_args(argc, arg0, arg1, arg2);
        let args_ptr = if inline {
            inline_args.as_ptr() as u64
        } else {
            inline_args[0]
        };
        let result = vm.invoke_jit_static_method_ref(&method_ref, args_ptr, argc);
        set_current_vm(vm_ptr);
        result
    };

    if result.is_none() && !method_ref.descriptor.ends_with('V') {
        record_deopt_request(ctx, DeoptReason::GuardFailure);
    }
    value_to_u64(result)
}

extern "C" fn jit_helper_invoke_interface(
    ctx: u64,
    obj: u64,
    method_ref_id: u64,
    argc: u64,
    arg0: u64,
    arg1: u64,
) -> u64 {
    if obj == 0 {
        record_deopt_request(ctx, DeoptReason::NullCheck);
        println!("JIT helper: invoke_interface - null receiver, deoptimizing");
        return 0;
    }

    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!("JIT helper: invoke_interface - no VM context, deoptimizing");
        return 0;
    }

    let Some(method_ref) = get_registered_method_ref(method_ref_id as usize) else {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!(
            "JIT helper: invoke_interface - missing method ref {}",
            method_ref_id
        );
        return 0;
    };

    let result = unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        let (argc, inline_args, inline) = decode_helper_args(argc, arg0, arg1, 0);
        let args_ptr = if inline {
            inline_args.as_ptr() as u64
        } else {
            inline_args[0]
        };
        let result = vm.invoke_jit_interface_method_ref(&method_ref, obj, args_ptr, argc);
        set_current_vm(vm_ptr);
        result
    };

    if result.is_none() && !method_ref.descriptor.ends_with('V') {
        record_deopt_request(ctx, DeoptReason::GuardFailure);
    }
    value_to_u64(result)
}

extern "C" fn jit_helper_invoke_dynamic(
    ctx: u64,
    site_id: u64,
    argc: u64,
    arg0: u64,
    arg1: u64,
    arg2: u64,
) -> u64 {
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!("JIT helper: invoke_dynamic - no VM context, deoptimizing");
        return 0;
    }

    let Some(site) = get_registered_invoke_dynamic_site(site_id as usize) else {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!("JIT helper: invoke_dynamic - missing site {}", site_id);
        return 0;
    };

    let result = unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        let (argc, inline_args, inline) = decode_helper_args(argc, arg0, arg1, arg2);
        let args_ptr = if inline {
            inline_args.as_ptr() as u64
        } else {
            inline_args[0]
        };
        let result = vm.invoke_jit_dynamic_site(&site, args_ptr, argc);
        set_current_vm(vm_ptr);
        result
    };

    if result.is_none() && !site.descriptor.ends_with('V') {
        record_deopt_request(ctx, DeoptReason::GuardFailure);
    }
    value_to_u64(result)
}

extern "C" fn jit_helper_invoke_native(
    ctx: u64,
    method_ref_id: u64,
    argc: u64,
    arg0: u64,
    arg1: u64,
    arg2: u64,
) -> u64 {
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!("JIT helper: invoke_native - no VM context, deoptimizing");
        return 0;
    }

    let Some(method_ref) = get_registered_method_ref(method_ref_id as usize) else {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!(
            "JIT helper: invoke_native - missing method ref {}",
            method_ref_id
        );
        return 0;
    };

    let result = unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        let (argc, inline_args, inline) = decode_helper_args(argc, arg0, arg1, arg2);
        let args_ptr = if inline {
            inline_args.as_ptr() as u64
        } else {
            inline_args[0]
        };
        let result = vm.invoke_jit_native_method_ref(&method_ref, args_ptr, argc);
        set_current_vm(vm_ptr);
        result
    };

    if result.is_none() && !method_ref.descriptor.ends_with('V') {
        record_deopt_request(ctx, DeoptReason::GuardFailure);
    }
    value_to_u64(result)
}

extern "C" fn jit_helper_checkcast(
    ctx: u64,
    obj: u64,
    class_id: u64,
    _: u64,
    _: u64,
    _: u64,
) -> u64 {
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!("JIT helper: checkcast - no VM context, deoptimizing");
        return 0;
    }

    let Some(class_name) = get_registered_class_name(class_id as usize) else {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!("JIT helper: checkcast - missing class id {}", class_id);
        return 0;
    };

    unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        let ok = vm.invoke_jit_checkcast(obj, &class_name);
        set_current_vm(vm_ptr);
        if !ok {
            record_deopt_request(ctx, DeoptReason::ClassCast);
        }
        if ok { 1 } else { 0 }
    }
}

extern "C" fn jit_helper_force_deopt(
    ctx: u64,
    raw_reason: u64,
    passthrough: u64,
    _: u64,
    _: u64,
    _: u64,
) -> u64 {
    let reason = match raw_reason {
        1 => DeoptReason::GuardFailure,
        2 => DeoptReason::NullCheck,
        3 => DeoptReason::ClassCast,
        4 => DeoptReason::MonitorFailure,
        5 => DeoptReason::AllocationFailure,
        6 => DeoptReason::HelperUnsupported,
        7 => DeoptReason::Exception,
        8 => DeoptReason::SiteFallback,
        _ => DeoptReason::SiteFallback,
    };
    record_deopt_request(ctx, reason);
    passthrough
}

extern "C" fn jit_helper_instanceof(
    ctx: u64,
    obj: u64,
    class_id: u64,
    _: u64,
    _: u64,
    _: u64,
) -> u64 {
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!("JIT helper: instanceof - no VM context, deoptimizing");
        return 0;
    }

    let Some(class_name) = get_registered_class_name(class_id as usize) else {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!("JIT helper: instanceof - missing class id {}", class_id);
        return 0;
    };

    unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        let ok = vm.invoke_jit_instanceof(obj, &class_name);
        set_current_vm(vm_ptr);
        if ok { 1 } else { 0 }
    }
}

extern "C" fn jit_helper_athrow(ctx: u64, exception: u64, _: u64, _: u64, _: u64, _: u64) -> u64 {
    record_deopt_request(ctx, DeoptReason::Exception);
    if exception == 0 {
        let vm_ptr = get_current_vm_ptr();
        if vm_ptr != 0 {
            unsafe {
                let vm = &mut *(vm_ptr as *mut Vm);
                raise_pending_exception(vm, "java/lang/NullPointerException");
                set_current_vm(vm_ptr);
            }
        }
        return 0;
    }
    set_pending_jit_exception(exception);
    0
}

extern "C" fn jit_helper_monitor_enter(ctx: u64, obj: u64, _: u64, _: u64, _: u64, _: u64) -> u64 {
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!("JIT helper: monitor_enter - no VM context, deoptimizing");
        return 0;
    }

    unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        let ok = vm.invoke_jit_monitor_enter(obj);
        set_current_vm(vm_ptr);
        if !ok {
            record_deopt_request(ctx, DeoptReason::MonitorFailure);
        }
        if ok { 1 } else { 0 }
    }
}

extern "C" fn jit_helper_monitor_exit(ctx: u64, obj: u64, _: u64, _: u64, _: u64, _: u64) -> u64 {
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        record_deopt_request(ctx, DeoptReason::HelperUnsupported);
        println!("JIT helper: monitor_exit - no VM context, deoptimizing");
        return 0;
    }

    unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        let ok = vm.invoke_jit_monitor_exit(obj);
        set_current_vm(vm_ptr);
        if !ok {
            record_deopt_request(ctx, DeoptReason::MonitorFailure);
        }
        if ok { 1 } else { 0 }
    }
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
    deopt_local_count: usize,
    deopt_stack_count: usize,
}

impl JitEntry {
    pub fn new(
        code: Vec<u8>,
        frame_size: usize,
        num_slots: usize,
        deopt_local_count: usize,
        deopt_stack_count: usize,
    ) -> Option<Self> {
        let (code_ptr, alloc_size) = Self::make_executable(&code)?;
        Some(JitEntry {
            code_ptr,
            alloc_size,
            frame_size,
            num_slots,
            deopt_local_count,
            deopt_stack_count,
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

    pub fn deopt_local_count(&self) -> usize {
        self.deopt_local_count
    }

    pub fn deopt_stack_count(&self) -> usize {
        self.deopt_stack_count
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
        let deopt_local_count = code.deopt_info.local_kinds.len();
        let deopt_stack_count = code.deopt_info.max_stack_depth;

        match JitEntry::new(
            code.code_buffer,
            frame_size,
            num_slots,
            deopt_local_count,
            deopt_stack_count,
        ) {
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

    pub fn remove_method(&mut self, key: &str) {
        self.entries.remove(key);
    }

    pub fn execute(
        &self,
        vm_ptr: u64,
        key: &str,
        args: &[crate::vm::types::Value],
    ) -> Option<crate::vm::types::Value> {
        self.execute_typed(vm_ptr, key, args, JitReturn::Int)
    }

    pub fn execute_typed(
        &self,
        vm_ptr: u64,
        key: &str,
        args: &[crate::vm::types::Value],
        ret: JitReturn,
    ) -> Option<crate::vm::types::Value> {
        let entry = self.entries.get(key)?;
        let fn_ptr = entry.code_ptr();
        let _ = entry.frame_size();
        clear_pending_jit_exception();
        clear_last_deopt_snapshot();
        set_current_vm(vm_ptr);
        let deopt_stack_depth_index = 1 + entry.deopt_local_count();
        let deopt_stack_base = deopt_stack_depth_index + 1;
        let mut deopt_buffer =
            vec![0u64; deopt_stack_base + entry.deopt_stack_count()];

        let mut int_args: [u64; 6] = [0; 6];
        let mut int_count = 1;

        int_args[0] = deopt_buffer.as_mut_ptr() as u64;

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
                    if int_count < 6 {
                        int_args[int_count] = v.to_bits() as u64;
                    }
                    int_count += 1;
                }
                crate::vm::types::Value::Double(v) => {
                    if int_count < 6 {
                        int_args[int_count] = v.to_bits();
                    }
                    int_count += 1;
                }
                crate::vm::types::Value::Reference(r) => {
                    if int_count < 6 {
                        int_args[int_count] = encode_reference(*r);
                    }
                    int_count += 1;
                }
                crate::vm::types::Value::ReturnAddress(_) => {}
            }
        }

        let result = unsafe {
            match ret {
                JitReturn::Void => {
                    let f: extern "C" fn(u64, u64, u64, u64, u64, u64) =
                        std::mem::transmute(fn_ptr);
                    f(
                        int_args[0],
                        int_args[1],
                        int_args[2],
                        int_args[3],
                        int_args[4],
                        int_args[5],
                    );
                    Some(crate::vm::types::Value::Int(0))
                }
                JitReturn::Int => {
                    let f: extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64 =
                        std::mem::transmute(fn_ptr);
                    let r = f(
                        int_args[0],
                        int_args[1],
                        int_args[2],
                        int_args[3],
                        int_args[4],
                        int_args[5],
                    );
                    Some(crate::vm::types::Value::Int(r as i32))
                }
                JitReturn::Long => {
                    let f: extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64 =
                        std::mem::transmute(fn_ptr);
                    let r = f(
                        int_args[0],
                        int_args[1],
                        int_args[2],
                        int_args[3],
                        int_args[4],
                        int_args[5],
                    );
                    Some(crate::vm::types::Value::Long(r as i64))
                }
                JitReturn::Float => {
                    let f: extern "C" fn(u64, u64, u64, u64, u64, u64) -> f32 =
                        std::mem::transmute(fn_ptr);
                    let r = f(
                        int_args[0],
                        int_args[1],
                        int_args[2],
                        int_args[3],
                        int_args[4],
                        int_args[5],
                    );
                    Some(crate::vm::types::Value::Float(r))
                }
                JitReturn::Double => {
                    let f: extern "C" fn(u64, u64, u64, u64, u64, u64) -> f64 =
                        std::mem::transmute(fn_ptr);
                    let r = f(
                        int_args[0],
                        int_args[1],
                        int_args[2],
                        int_args[3],
                        int_args[4],
                        int_args[5],
                    );
                    Some(crate::vm::types::Value::Double(r))
                }
                JitReturn::Reference => {
                    let f: extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64 =
                        std::mem::transmute(fn_ptr);
                    let r = f(
                        int_args[0],
                        int_args[1],
                        int_args[2],
                        int_args[3],
                        int_args[4],
                        int_args[5],
                    );
                    let r_ref = decode_reference(r);
                    Some(crate::vm::types::Value::Reference(r_ref))
                }
            }
        };
        let stack_depth = deopt_buffer
            .get(deopt_stack_depth_index)
            .copied()
            .unwrap_or(0) as usize;
        let stack_end = deopt_stack_base + stack_depth.min(entry.deopt_stack_count());
        let deopt_flags = deopt_buffer[0];
        set_last_deopt_snapshot(DeoptSnapshot {
            reason: if deopt_flags & DEOPT_PENDING_MARKER != 0 {
                decode_deopt_reason(deopt_flags)
            } else {
                None
            },
            pc: (deopt_flags & !(DEOPT_PENDING_MARKER | DEOPT_REASON_MASK)) as usize,
            locals: deopt_buffer[1..deopt_stack_depth_index].to_vec(),
            stack: deopt_buffer[deopt_stack_base..stack_end].to_vec(),
        });
        clear_current_vm();
        result
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
