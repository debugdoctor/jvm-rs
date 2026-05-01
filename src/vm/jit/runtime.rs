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
static JIT_FIELD_REFS: OnceLock<Mutex<Vec<FieldRef>>> = OnceLock::new();
static JIT_METHOD_REFS: OnceLock<Mutex<Vec<MethodRef>>> = OnceLock::new();
static JIT_INVOKE_DYNAMIC_SITES: OnceLock<Mutex<Vec<InvokeDynamicSite>>> = OnceLock::new();
const INLINE_ARG_MARKER: u64 = 1u64 << 63;

const ARRAY_KIND_PRIMITIVE: u64 = 1;
const ARRAY_KIND_REFERENCE: u64 = 2;
const ARRAY_KIND_MULTI: u64 = 3;

pub extern "C" fn jit_helper_load_reference_array_element(
    _ctx: u64,
    array_ref: u64,
    index: u64,
    _: u64,
    _: u64,
    _: u64,
) -> u64 {
    let array_ref = Reference::Heap(array_ref as usize);
    let index = index as i32;
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
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
            Ok(Reference::Heap(idx)) => idx as u64,
            Ok(Reference::Null) => 0,
            Err(e) => {
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
    _ctx: u64,
    array_ref: u64,
    index: u64,
    value: u64,
    _: u64,
    _: u64,
) -> u64 {
    let array_ref = Reference::Heap(array_ref as usize);
    let index = index as i32;
    let value = if value == 0 {
        Reference::Null
    } else {
        Reference::Heap(value as usize)
    };
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
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
    _ctx: u64,
    array_ref: u64,
    _: u64,
    _: u64,
    _: u64,
    _: u64,
) -> u64 {
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 || array_ref == 0 {
        println!("JIT helper: array_length - missing VM context or null array");
        return 0;
    }
    unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        let result = vm
            .heap
            .lock()
            .unwrap()
            .array_length(Reference::Heap(array_ref as usize));
        set_current_vm(vm_ptr);
        match result {
            Ok(len) => len as u64,
            Err(e) => {
                println!("JIT helper: array_length failed: {:?}", e);
                0
            }
        }
    }
}

pub fn get_array_length_ptr() -> u64 {
    jit_helper_array_length as u64
}

pub extern "C" fn jit_helper_load_typed_array_element(
    _ctx: u64,
    array_ref: u64,
    index: u64,
    _type_marker: u64,
    _: u64,
    _: u64,
) -> u64 {
    let array_ref = Reference::Heap(array_ref as usize);
    let index = index as i32;
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        println!("JIT helper: load_typed_array_element - no VM context, deoptimizing");
        return 0;
    }
    unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        let heap = vm.heap.lock().unwrap();
        let heap_val = heap.get(array_ref);
        match heap_val {
            Ok(HeapValue::DoubleArray { values }) => {
                let i = match Heap::check_array_index(index, values.len()) {
                    Ok(i) => i,
                    Err(e) => {
                        println!(
                            "JIT helper: load_typed_array_element - index error: {:?}",
                            e
                        );
                        return 0;
                    }
                };
                let val = values[i];
                drop(heap);
                set_current_vm(vm_ptr);
                val.to_bits() as u64
            }
            Ok(HeapValue::LongArray { values }) => {
                let i = match Heap::check_array_index(index, values.len()) {
                    Ok(i) => i,
                    Err(_) => return 0,
                };
                let val = values[i] as u64;
                drop(heap);
                set_current_vm(vm_ptr);
                val
            }
            Ok(HeapValue::IntArray { values }) => {
                let i = match Heap::check_array_index(index, values.len()) {
                    Ok(i) => i,
                    Err(_) => return 0,
                };
                let val = values[i] as u32 as u64;
                drop(heap);
                set_current_vm(vm_ptr);
                val
            }
            Ok(HeapValue::FloatArray { values }) => {
                let i = match Heap::check_array_index(index, values.len()) {
                    Ok(i) => i,
                    Err(_) => return 0,
                };
                let val = values[i];
                drop(heap);
                set_current_vm(vm_ptr);
                val.to_bits() as u64
            }
            Err(e) => {
                println!(
                    "JIT helper: load_typed_array_element - array not found: {:?}",
                    e
                );
                0
            }
            _ => {
                println!("JIT helper: load_typed_array_element - invalid array type");
                0
            }
        }
    }
}

pub fn get_load_typed_array_element_ptr() -> u64 {
    jit_helper_load_typed_array_element as u64
}

pub extern "C" fn jit_helper_store_typed_array_element(
    _ctx: u64,
    array_ref: u64,
    index: u64,
    value: u64,
    _: u64,
    _: u64,
) -> u64 {
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 || array_ref == 0 {
        println!("JIT helper: store_typed_array_element - missing VM context or null array");
        return 0;
    }

    unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        let mut heap = vm.heap.lock().unwrap();
        let reference = Reference::Heap(array_ref as usize);
        let index = index as i32;
        let result = match heap.get_mut(reference) {
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
        };
        set_current_vm(vm_ptr);
        match result {
            Ok(()) => 1,
            Err(e) => {
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
    descriptors.push(descriptor.into());
    (descriptors.len() - 1) as u64
}

fn get_registered_array_descriptor(index: usize) -> Option<String> {
    JIT_ARRAY_DESCRIPTORS
        .get()
        .and_then(|descriptors| descriptors.lock().ok()?.get(index).cloned())
}

pub fn register_field_ref(field_ref: FieldRef) -> u64 {
    let refs = JIT_FIELD_REFS.get_or_init(|| Mutex::new(Vec::new()));
    let mut refs = refs.lock().unwrap();
    refs.push(field_ref);
    (refs.len() - 1) as u64
}

fn get_registered_field_ref(index: usize) -> Option<FieldRef> {
    JIT_FIELD_REFS
        .get()
        .and_then(|refs| refs.lock().ok()?.get(index).cloned())
}

pub fn register_method_ref(method_ref: MethodRef) -> u64 {
    let refs = JIT_METHOD_REFS.get_or_init(|| Mutex::new(Vec::new()));
    let mut refs = refs.lock().unwrap();
    refs.push(method_ref);
    (refs.len() - 1) as u64
}

fn get_registered_method_ref(index: usize) -> Option<MethodRef> {
    JIT_METHOD_REFS
        .get()
        .and_then(|refs| refs.lock().ok()?.get(index).cloned())
}

pub fn register_invoke_dynamic_site(site: InvokeDynamicSite) -> u64 {
    let sites = JIT_INVOKE_DYNAMIC_SITES.get_or_init(|| Mutex::new(Vec::new()));
    let mut sites = sites.lock().unwrap();
    sites.push(site);
    (sites.len() - 1) as u64
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
        Some(Value::Reference(Reference::Null)) | None => 0,
        Some(Value::Reference(Reference::Heap(addr))) => addr as u64,
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
    _ctx: u64,
    _class_ptr: u64,
    _size: u64,
    _: u64,
    _: u64,
    _: u64,
) -> u64 {
    println!("JIT helper: allocate_object (stub - deoptimizing)");
    0
}

extern "C" fn jit_helper_allocate_array(
    _ctx: u64,
    kind: u64,
    descriptor_or_atype: u64,
    argc: u64,
    arg0: u64,
    arg1: u64,
) -> u64 {
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
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

    value_to_u64(result.map(Value::Reference))
}

extern "C" fn jit_helper_get_static_field(
    _ctx: u64,
    field_ref_id: u64,
    _: u64,
    _: u64,
    _: u64,
    _: u64,
) -> u64 {
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        println!("JIT helper: get_static_field - no VM context, deoptimizing");
        return 0;
    }

    let Some(field_ref) = get_registered_field_ref(field_ref_id as usize) else {
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

    value_to_u64(result)
}

extern "C" fn jit_helper_put_static_field(
    _ctx: u64,
    field_ref_id: u64,
    value: u64,
    _: u64,
    _: u64,
    _: u64,
) -> u64 {
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        println!("JIT helper: put_static_field - no VM context, deoptimizing");
        return 0;
    }

    let Some(field_ref) = get_registered_field_ref(field_ref_id as usize) else {
        println!(
            "JIT helper: put_static_field - missing field ref {}",
            field_ref_id
        );
        return 0;
    };

    unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        vm.invoke_jit_put_static_field_ref(&field_ref, value);
        set_current_vm(vm_ptr);
    }

    0
}

extern "C" fn jit_helper_get_instance_field(
    _ctx: u64,
    obj: u64,
    field_ref_id: u64,
    _: u64,
    _: u64,
    _: u64,
) -> u64 {
    if obj == 0 {
        println!("JIT helper: get_instance_field - null receiver, deoptimizing");
        return 0;
    }

    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        println!("JIT helper: get_instance_field - no VM context, deoptimizing");
        return 0;
    }

    let Some(field_ref) = get_registered_field_ref(field_ref_id as usize) else {
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

    value_to_u64(result)
}

extern "C" fn jit_helper_put_instance_field(
    _ctx: u64,
    obj: u64,
    field_ref_id: u64,
    value: u64,
    _: u64,
    _: u64,
) -> u64 {
    if obj == 0 {
        println!("JIT helper: put_instance_field - null receiver, deoptimizing");
        return 0;
    }

    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        println!("JIT helper: put_instance_field - no VM context, deoptimizing");
        return 0;
    }

    let Some(field_ref) = get_registered_field_ref(field_ref_id as usize) else {
        println!(
            "JIT helper: put_instance_field - missing field ref {}",
            field_ref_id
        );
        return 0;
    };

    unsafe {
        let vm = &mut *(vm_ptr as *mut Vm);
        vm.invoke_jit_put_instance_field_ref(&field_ref, obj, value);
        set_current_vm(vm_ptr);
    }

    0
}

extern "C" fn jit_helper_invoke_virtual(
    _ctx: u64,
    obj: u64,
    method_ref_id: u64,
    argc: u64,
    arg0: u64,
    arg1: u64,
) -> u64 {
    if obj == 0 {
        println!("JIT helper: invoke_virtual - null receiver, deoptimizing");
        return 0;
    }

    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        println!("JIT helper: invoke_virtual - no VM context, deoptimizing");
        return 0;
    }

    let Some(method_ref) = get_registered_method_ref(method_ref_id as usize) else {
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

    value_to_u64(result)
}

extern "C" fn jit_helper_invoke_special(
    _ctx: u64,
    obj: u64,
    method_ref_id: u64,
    argc: u64,
    arg0: u64,
    arg1: u64,
) -> u64 {
    if obj == 0 {
        println!("JIT helper: invoke_special - null receiver, deoptimizing");
        return 0;
    }

    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        println!("JIT helper: invoke_special - no VM context, deoptimizing");
        return 0;
    }

    let Some(method_ref) = get_registered_method_ref(method_ref_id as usize) else {
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

    value_to_u64(result)
}

extern "C" fn jit_helper_invoke_static(
    _ctx: u64,
    method_ref_id: u64,
    argc: u64,
    arg0: u64,
    arg1: u64,
    arg2: u64,
) -> u64 {
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        println!("JIT helper: invoke_static - no VM context, deoptimizing");
        return 0;
    }

    let Some(method_ref) = get_registered_method_ref(method_ref_id as usize) else {
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

    value_to_u64(result)
}

extern "C" fn jit_helper_invoke_interface(
    _ctx: u64,
    obj: u64,
    method_ref_id: u64,
    argc: u64,
    arg0: u64,
    arg1: u64,
) -> u64 {
    if obj == 0 {
        println!("JIT helper: invoke_interface - null receiver, deoptimizing");
        return 0;
    }

    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        println!("JIT helper: invoke_interface - no VM context, deoptimizing");
        return 0;
    }

    let Some(method_ref) = get_registered_method_ref(method_ref_id as usize) else {
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

    value_to_u64(result)
}

extern "C" fn jit_helper_invoke_dynamic(
    _ctx: u64,
    site_id: u64,
    argc: u64,
    arg0: u64,
    arg1: u64,
    arg2: u64,
) -> u64 {
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        println!("JIT helper: invoke_dynamic - no VM context, deoptimizing");
        return 0;
    }

    let Some(site) = get_registered_invoke_dynamic_site(site_id as usize) else {
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

    value_to_u64(result)
}

extern "C" fn jit_helper_invoke_native(
    _ctx: u64,
    method_ref_id: u64,
    argc: u64,
    arg0: u64,
    arg1: u64,
    arg2: u64,
) -> u64 {
    let vm_ptr = get_current_vm_ptr();
    if vm_ptr == 0 {
        println!("JIT helper: invoke_native - no VM context, deoptimizing");
        return 0;
    }

    let Some(method_ref) = get_registered_method_ref(method_ref_id as usize) else {
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

    value_to_u64(result)
}

extern "C" fn jit_helper_checkcast(
    _ctx: u64,
    _obj: u64,
    _class_ptr: u64,
    _: u64,
    _: u64,
    _: u64,
) -> u64 {
    println!("JIT helper: checkcast (stub - deoptimizing)");
    0
}

extern "C" fn jit_helper_instanceof(
    _ctx: u64,
    _obj: u64,
    _class_ptr: u64,
    _: u64,
    _: u64,
    _: u64,
) -> u64 {
    println!("JIT helper: instanceof (stub - deoptimizing)");
    0
}

extern "C" fn jit_helper_athrow(_ctx: u64, _exception: u64, _: u64, _: u64, _: u64, _: u64) -> u64 {
    println!("JIT helper: athrow (stub - deoptimizing)");
    0
}

extern "C" fn jit_helper_monitor_enter(
    _ctx: u64,
    _obj: u64,
    _: u64,
    _: u64,
    _: u64,
    _: u64,
) -> u64 {
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
        let mut int_count = 1;

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
