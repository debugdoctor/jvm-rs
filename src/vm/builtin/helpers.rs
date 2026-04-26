use crate::vm::{
    HeapValue, Reference, Value, Vm, VmError,
};
use crate::vm::types::ExecutionResult;

pub(super) fn stringify_reference(vm: &Vm, reference: Reference) -> Result<String, VmError> {
    match reference {
        Reference::Null => Ok("null".to_string()),
        _ => match vm.heap.lock().unwrap().get(reference)? {
            HeapValue::String(value) => Ok(value.clone()),
            value => Err(VmError::InvalidHeapValue {
                expected: "string",
                actual: value.kind_name(),
            }),
        },
    }
}

pub(super) fn format_value_for_append(
    vm: &Vm,
    descriptor: &str,
    args: &[Value],
) -> Result<std::string::String, VmError> {
    match descriptor {
        "(Ljava/lang/String;)Ljava/lang/StringBuilder;" => {
            stringify_reference(vm, args[0].as_reference()?)
        }
        "(I)Ljava/lang/StringBuilder;" => Ok(args[0].as_int()?.to_string()),
        "(J)Ljava/lang/StringBuilder;" => Ok(args[0].as_long()?.to_string()),
        "(C)Ljava/lang/StringBuilder;" => {
            Ok((args[0].as_int()? as u16 as u32)
                .try_into()
                .map(|c: char| c.to_string())
                .unwrap_or_default())
        }
        "(Z)Ljava/lang/StringBuilder;" => {
            Ok(if args[0].as_int()? != 0 { "true" } else { "false" }.to_string())
        }
        "(F)Ljava/lang/StringBuilder;" => Ok(crate::vm::builtin::format::format_float(args[0].as_float()? as f64)),
        "(D)Ljava/lang/StringBuilder;" => Ok(crate::vm::builtin::format::format_float(args[0].as_double()?)),
        "(Ljava/lang/Object;)Ljava/lang/StringBuilder;" => {
            let r = args[0].as_reference()?;
            vm.stringify_heap(r)
        }
        _ => Ok("?".to_string()),
    }
}

pub(super) fn native_int_stream_array(vm: &Vm, stream_ref: Reference) -> Result<Reference, VmError> {
    match vm.heap.lock().unwrap().get(stream_ref)? {
        HeapValue::Object { fields, .. } => match fields.get("__array") {
            Some(Value::Reference(r)) => Ok(*r),
            _ => Err(VmError::NullReference),
        },
        value => Err(VmError::InvalidHeapValue {
            expected: "object",
            actual: value.kind_name(),
        }),
    }
}

pub(super) fn native_long_stream_array(vm: &Vm, stream_ref: Reference) -> Result<Reference, VmError> {
    match vm.heap.lock().unwrap().get(stream_ref)? {
        HeapValue::Object { fields, .. } => match fields.get("__array") {
            Some(Value::Reference(r)) => Ok(*r),
            _ => Err(VmError::NullReference),
        },
        value => Err(VmError::InvalidHeapValue {
            expected: "object",
            actual: value.kind_name(),
        }),
    }
}

pub(super) fn native_double_stream_array(vm: &Vm, stream_ref: Reference) -> Result<Reference, VmError> {
    match vm.heap.lock().unwrap().get(stream_ref)? {
        HeapValue::Object { fields, .. } => match fields.get("__array") {
            Some(Value::Reference(r)) => Ok(*r),
            _ => Err(VmError::NullReference),
        },
        value => Err(VmError::InvalidHeapValue {
            expected: "object",
            actual: value.kind_name(),
        }),
    }
}

pub(super) fn native_collector_mode(vm: &Vm, collector_ref: Reference) -> Result<i32, VmError> {
    match vm.heap.lock().unwrap().get(collector_ref)? {
        HeapValue::Object { fields, .. } => match fields.get("__mode") {
            Some(Value::Int(mode)) => Ok(*mode),
            _ => Ok(0),
        },
        _ => Ok(0),
    }
}

pub(super) fn native_collector_array(vm: &Vm, collector_ref: Reference) -> Result<Reference, VmError> {
    match vm.heap.lock().unwrap().get(collector_ref)? {
        HeapValue::Object { fields, .. } => match fields.get("__array") {
            Some(Value::Reference(r)) => Ok(*r),
            _ => Err(VmError::NullReference),
        },
        value => Err(VmError::InvalidHeapValue {
            expected: "object",
            actual: value.kind_name(),
        }),
    }
}

pub(super) fn collect_with_mode(vm: &mut Vm, elements: Vec<Reference>, mode: i32, collector_ref: Reference) -> Result<Option<Value>, VmError> {
    match mode {
        1 => {
            let list_ref = vm.heap.lock().unwrap().allocate(HeapValue::Object {
                class_name: "java/util/ArrayList".to_string(),
                fields: std::collections::HashMap::new(),
            });
            for elem_ref in elements {
                vm.call_virtual(list_ref, "add", "(Ljava/lang/Object;)Z", vec![Value::Reference(elem_ref)])?;
            }
            Ok(Some(Value::Reference(list_ref)))
        }
        2 => {
            let set_ref = vm.heap.lock().unwrap().allocate(HeapValue::Object {
                class_name: "java/util/HashSet".to_string(),
                fields: std::collections::HashMap::new(),
            });
            for elem_ref in elements {
                vm.call_virtual(set_ref, "add", "(Ljava/lang/Object;)Z", vec![Value::Reference(elem_ref)])?;
            }
            Ok(Some(Value::Reference(set_ref)))
        }
        3 => {
            let count = elements.len() as i64;
            let mut fields = std::collections::HashMap::new();
            fields.insert("value".to_string(), Value::Long(count));
            let result = vm.heap.lock().unwrap().allocate(HeapValue::Object {
                class_name: "java/lang/Long".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(result)))
        }
        4 => {
            let mut strs = Vec::new();
            for elem_ref in elements {
                if elem_ref != Reference::Null {
                    if let Ok(s) = vm.stringify_heap(elem_ref) {
                        strs.push(s);
                    }
                }
            }
            let result = vm.new_string(strs.join(""));
            Ok(Some(result))
        }
        5 => {
            let delimiter_ref = native_collector_array(vm, collector_ref)?;
            let delimiter = if delimiter_ref != Reference::Null {
                vm.stringify_heap(delimiter_ref)?
            } else {
                String::new()
            };
            let mut strs = Vec::new();
            for elem_ref in elements {
                if elem_ref != Reference::Null {
                    if let Ok(s) = vm.stringify_heap(elem_ref) {
                        strs.push(s);
                    }
                }
            }
            let result = vm.new_string(strs.join(&delimiter));
            Ok(Some(result))
        }
        _ => Ok(None),
    }
}

pub(super) fn list_snapshot(vm: &mut Vm, list: Reference) -> Result<Vec<Reference>, VmError> {
    let size_res = vm.call_virtual(list, "size", "()I", vec![])?;
    let size = match size_res {
        ExecutionResult::Value(Value::Int(n)) => n,
        _ => return Err(VmError::TypeMismatch {
            expected: "int",
            actual: "non-int from List.size()",
        }),
    };
    let mut out = Vec::with_capacity(size.max(0) as usize);
    for i in 0..size {
        let res = vm.call_virtual(
            list,
            "get",
            "(I)Ljava/lang/Object;",
            vec![Value::Int(i)],
        )?;
        let r = match res {
            ExecutionResult::Value(Value::Reference(r)) => r,
            _ => return Err(VmError::TypeMismatch {
                expected: "reference",
                actual: "non-reference from List.get(I)",
            }),
        };
        out.push(r);
    }
    Ok(out)
}

pub(super) fn list_overwrite(vm: &mut Vm, list: Reference, values: &[Reference]) -> Result<(), VmError> {
    for (i, v) in values.iter().enumerate() {
        vm.call_virtual(
            list,
            "set",
            "(ILjava/lang/Object;)Ljava/lang/Object;",
            vec![Value::Int(i as i32), Value::Reference(*v)],
        )?;
    }
    Ok(())
}

pub(super) fn compare_natural(vm: &mut Vm, a: Reference, b: Reference) -> Result<i32, VmError> {
    let res = vm.call_virtual(
        a,
        "compareTo",
        "(Ljava/lang/Object;)I",
        vec![Value::Reference(b)],
    )?;
    match res {
        ExecutionResult::Value(Value::Int(n)) => Ok(n),
        _ => Err(VmError::TypeMismatch {
            expected: "int",
            actual: "non-int from compareTo",
        }),
    }
}

pub(super) fn compare_with(vm: &mut Vm, cmp: Reference, a: Reference, b: Reference) -> Result<i32, VmError> {
    let res = vm.call_virtual(
        cmp,
        "compare",
        "(Ljava/lang/Object;Ljava/lang/Object;)I",
        vec![Value::Reference(a), Value::Reference(b)],
    )?;
    match res {
        ExecutionResult::Value(Value::Int(n)) => Ok(n),
        _ => Err(VmError::TypeMismatch {
            expected: "int",
            actual: "non-int from Comparator.compare",
        }),
    }
}

pub(super) fn class_internal_name(vm: &Vm, reference: Reference) -> Result<String, VmError> {
    match vm.heap.lock().unwrap().get(reference)? {
        HeapValue::Object { fields, class_name } => {
            if let Some(Value::Reference(name_ref)) = fields.get("__name") {
                if let HeapValue::String(s) = vm.heap.lock().unwrap().get(*name_ref)? {
                    return Ok(s.clone());
                }
            }
            Ok(class_name.clone())
        }
        value => Err(VmError::InvalidHeapValue {
            expected: "object",
            actual: value.kind_name(),
        }),
    }
}

pub(super) fn is_throwable_class(vm: &mut Vm, class_name: &str) -> Result<bool, VmError> {
    vm.is_instance_of(class_name, "java/lang/Throwable")
}

pub(super) fn integer_value(vm: &Vm, reference: Reference) -> Result<i32, VmError> {
    match vm.heap.lock().unwrap().get(reference)? {
        HeapValue::Object { fields, .. } => Ok(fields
            .get("value")
            .and_then(|v| if let Value::Int(i) = v { Some(*i) } else { None })
            .unwrap_or(0)),
        _ => Ok(0),
    }
}

pub(super) fn hash_object(vm: &Vm, reference: Reference) -> i32 {
    match reference {
        Reference::Null => 0,
        Reference::Heap(idx) => {
            let base = idx as i64;
            ((base >> 32) ^ base) as i32
        }
    }
}

pub(super) fn hash_array_elements(vm: &Vm, arr_ref: Reference) -> Result<i32, VmError> {
    let mut hash: i32 = 0;
    match vm.heap.lock().unwrap().get(arr_ref)? {
        HeapValue::ReferenceArray { values, .. } => {
            for r in values {
                let elem_hash = match r {
                    Reference::Null => 0,
                    _ => hash_object(vm, *r),
                };
                hash = hash.wrapping_mul(31).wrapping_add(elem_hash);
            }
        }
        HeapValue::IntArray { values } => {
            for v in values {
                hash = hash.wrapping_mul(31).wrapping_add(*v);
            }
        }
        HeapValue::LongArray { values } => {
            for v in values {
                let elem_hash = ((*v as u64) ^ ((*v as u64) >> 32)) as i32;
                hash = hash.wrapping_mul(31).wrapping_add(elem_hash);
            }
        }
        HeapValue::FloatArray { values } => {
            for v in values {
                hash = hash.wrapping_mul(31).wrapping_add((*v as u32) as i32);
            }
        }
        HeapValue::DoubleArray { values } => {
            for v in values {
                let bits = v.to_bits();
                let elem_hash = ((bits as u64) ^ ((bits as u64) >> 32)) as i32;
                hash = hash.wrapping_mul(31).wrapping_add(elem_hash);
            }
        }
        _ => {
            let base = match arr_ref {
                Reference::Heap(idx) => idx as i64,
                Reference::Null => 0,
            };
            hash = ((base >> 32) ^ base) as i32;
        }
    }
    Ok(hash)
}

pub(super) fn arraycopy(
    vm: &mut Vm,
    src: Reference,
    src_pos: i32,
    dst: Reference,
    dst_pos: i32,
    length: i32,
) -> Result<(), VmError> {
    if length < 0 || src_pos < 0 || dst_pos < 0 {
        return Err(VmError::UnhandledException {
            class_name: "java/lang/ArrayIndexOutOfBoundsException".to_string(),
        });
    }
    let src_pos = src_pos as usize;
    let dst_pos = dst_pos as usize;
    let length = length as usize;

    let src_kind;
    let src_slice_int;
    let src_slice_long;
    let src_slice_float;
    let src_slice_double;
    let src_slice_ref;
    {
        let mut heap = vm.heap.lock().unwrap();
        let value = heap.get(src)?;
        match value {
            HeapValue::IntArray { values } => {
                if src_pos + length > values.len() {
                    return Err(VmError::UnhandledException {
                        class_name: "java/lang/ArrayIndexOutOfBoundsException"
                            .to_string(),
                    });
                }
                src_kind = "I";
                src_slice_int = values[src_pos..src_pos + length].to_vec();
                src_slice_long = Vec::new();
                src_slice_float = Vec::new();
                src_slice_double = Vec::new();
                src_slice_ref = Vec::new();
            }
            HeapValue::LongArray { values } => {
                if src_pos + length > values.len() {
                    return Err(VmError::UnhandledException {
                        class_name: "java/lang/ArrayIndexOutOfBoundsException"
                            .to_string(),
                    });
                }
                src_kind = "J";
                src_slice_int = Vec::new();
                src_slice_long = values[src_pos..src_pos + length].to_vec();
                src_slice_float = Vec::new();
                src_slice_double = Vec::new();
                src_slice_ref = Vec::new();
            }
            HeapValue::FloatArray { values } => {
                if src_pos + length > values.len() {
                    return Err(VmError::UnhandledException {
                        class_name: "java/lang/ArrayIndexOutOfBoundsException"
                            .to_string(),
                    });
                }
                src_kind = "F";
                src_slice_int = Vec::new();
                src_slice_long = Vec::new();
                src_slice_float = values[src_pos..src_pos + length].to_vec();
                src_slice_double = Vec::new();
                src_slice_ref = Vec::new();
            }
            HeapValue::DoubleArray { values } => {
                if src_pos + length > values.len() {
                    return Err(VmError::UnhandledException {
                        class_name: "java/lang/ArrayIndexOutOfBoundsException"
                            .to_string(),
                    });
                }
                src_kind = "D";
                src_slice_int = Vec::new();
                src_slice_long = Vec::new();
                src_slice_float = Vec::new();
                src_slice_double = values[src_pos..src_pos + length].to_vec();
                src_slice_ref = Vec::new();
            }
            HeapValue::ReferenceArray { values, .. } => {
                if src_pos + length > values.len() {
                    return Err(VmError::UnhandledException {
                        class_name: "java/lang/ArrayIndexOutOfBoundsException"
                            .to_string(),
                    });
                }
                src_kind = "L";
                src_slice_int = Vec::new();
                src_slice_long = Vec::new();
                src_slice_float = Vec::new();
                src_slice_double = Vec::new();
                src_slice_ref = values[src_pos..src_pos + length].to_vec();
            }
            other => {
                return Err(VmError::InvalidHeapValue {
                    expected: "array",
                    actual: other.kind_name(),
                });
            }
        }
    }

    let mut heap = vm.heap.lock().unwrap();
    match (src_kind, heap.get_mut(dst)?) {
        ("I", HeapValue::IntArray { values }) => {
            if dst_pos + length > values.len() {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/ArrayIndexOutOfBoundsException".to_string(),
                });
            }
            values[dst_pos..dst_pos + length].copy_from_slice(&src_slice_int);
        }
        ("J", HeapValue::LongArray { values }) => {
            if dst_pos + length > values.len() {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/ArrayIndexOutOfBoundsException".to_string(),
                });
            }
            values[dst_pos..dst_pos + length].copy_from_slice(&src_slice_long);
        }
        ("F", HeapValue::FloatArray { values }) => {
            if dst_pos + length > values.len() {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/ArrayIndexOutOfBoundsException".to_string(),
                });
            }
            values[dst_pos..dst_pos + length].copy_from_slice(&src_slice_float);
        }
        ("D", HeapValue::DoubleArray { values }) => {
            if dst_pos + length > values.len() {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/ArrayIndexOutOfBoundsException".to_string(),
                });
            }
            values[dst_pos..dst_pos + length].copy_from_slice(&src_slice_double);
        }
        ("L", HeapValue::ReferenceArray { values, .. }) => {
            if dst_pos + length > values.len() {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/ArrayIndexOutOfBoundsException".to_string(),
                });
            }
            values[dst_pos..dst_pos + length].copy_from_slice(&src_slice_ref);
        }
        _ => {
            return Err(VmError::UnhandledException {
                class_name: "java/lang/ArrayStoreException".to_string(),
            });
        }
    }
    Ok(())
}

pub(super) fn native_arrays_equals_int(vm: &Vm, a: Reference, b: Reference) -> Result<bool, VmError> {
    if a == b {
        return Ok(true);
    }
    if a == Reference::Null || b == Reference::Null {
        return Ok(false);
    }
    let mut heap = vm.heap.lock().unwrap();
    match (heap.get(a)?, heap.get(b)?) {
        (HeapValue::IntArray { values: x }, HeapValue::IntArray { values: y }) => Ok(x == y),
        _ => Ok(false),
    }
}

pub(super) fn native_arrays_equals_long(vm: &Vm, a: Reference, b: Reference) -> Result<bool, VmError> {
    if a == b {
        return Ok(true);
    }
    if a == Reference::Null || b == Reference::Null {
        return Ok(false);
    }
    let mut heap = vm.heap.lock().unwrap();
    match (heap.get(a)?, heap.get(b)?) {
        (HeapValue::LongArray { values: x }, HeapValue::LongArray { values: y }) => {
            Ok(x == y)
        }
        _ => Ok(false),
    }
}

pub(super) fn native_arrays_equals_float(vm: &Vm, a: Reference, b: Reference) -> Result<bool, VmError> {
    if a == b {
        return Ok(true);
    }
    if a == Reference::Null || b == Reference::Null {
        return Ok(false);
    }
    let mut heap = vm.heap.lock().unwrap();
    match (heap.get(a)?, heap.get(b)?) {
        (HeapValue::FloatArray { values: x }, HeapValue::FloatArray { values: y }) => {
            Ok(x.len() == y.len()
                && x.iter().zip(y.iter()).all(|(a, b)| a.to_bits() == b.to_bits()))
        }
        _ => Ok(false),
    }
}

pub(super) fn native_arrays_equals_double(vm: &Vm, a: Reference, b: Reference) -> Result<bool, VmError> {
    if a == b {
        return Ok(true);
    }
    if a == Reference::Null || b == Reference::Null {
        return Ok(false);
    }
    let mut heap = vm.heap.lock().unwrap();
    match (heap.get(a)?, heap.get(b)?) {
        (HeapValue::DoubleArray { values: x }, HeapValue::DoubleArray { values: y }) => {
            Ok(x.len() == y.len()
                && x.iter().zip(y.iter()).all(|(a, b)| a.to_bits() == b.to_bits()))
        }
        _ => Ok(false),
    }
}

pub(super) fn native_arrays_equals_ref(vm: &mut Vm, a: Reference, b: Reference) -> Result<bool, VmError> {
    if a == b {
        return Ok(true);
    }
    if a == Reference::Null || b == Reference::Null {
        return Ok(false);
    }
    let (xs, ys): (Vec<Reference>, Vec<Reference>) = {
        let mut heap = vm.heap.lock().unwrap();
        match (heap.get(a)?, heap.get(b)?) {
            (
                HeapValue::ReferenceArray { values: x, .. },
                HeapValue::ReferenceArray { values: y, .. },
            ) => (x.clone(), y.clone()),
            _ => return Ok(false),
        }
    };
    if xs.len() != ys.len() {
        return Ok(false);
    }
    for (x, y) in xs.iter().zip(ys.iter()) {
        if x == y {
            continue;
        }
        if *x == Reference::Null || *y == Reference::Null {
            return Ok(false);
        }
        let res = vm.call_virtual(
            *x,
            "equals",
            "(Ljava/lang/Object;)Z",
            vec![Value::Reference(*y)],
        )?;
        match res {
            ExecutionResult::Value(Value::Int(0)) => return Ok(false),
            ExecutionResult::Value(Value::Int(_)) => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}

pub(super) fn native_collections_sort(
    vm: &mut Vm,
    list: Reference,
    cmp: Option<Reference>,
) -> Result<(), VmError> {
    if list == Reference::Null {
        return Err(VmError::NullReference);
    }
    let mut values = list_snapshot(vm, list)?;
    for i in 1..values.len() {
        let mut j = i;
        while j > 0 {
            let cmp_result = match cmp {
                Some(c) => compare_with(vm, c, values[j - 1], values[j])?,
                None => compare_natural(vm, values[j - 1], values[j])?,
            };
            if cmp_result > 0 {
                values.swap(j - 1, j);
                j -= 1;
            } else {
                break;
            }
        }
    }
    list_overwrite(vm, list, &values)?;
    Ok(())
}

pub(super) fn native_collections_reverse(vm: &mut Vm, list: Reference) -> Result<(), VmError> {
    if list == Reference::Null {
        return Err(VmError::NullReference);
    }
    let mut values = list_snapshot(vm, list)?;
    values.reverse();
    list_overwrite(vm, list, &values)?;
    Ok(())
}
