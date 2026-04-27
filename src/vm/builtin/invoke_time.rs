use crate::vm::{HeapValue, Reference, Value, Vm, VmError};

pub(super) fn invoke_time(
    vm: &mut Vm,
    class_name: &str,
    method_name: &str,
    descriptor: &str,
    args: &[Value],
) -> Result<Option<Value>, VmError> {
    match (class_name, method_name, descriptor) {
        ("java/time/Instant", "now", "()Ljava/time/Instant;") => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap();
            let mut fields = std::collections::HashMap::new();
            fields.insert("__epoch_second".to_string(), Value::Long(now.as_secs() as i64));
            fields.insert("__nano".to_string(), Value::Int(now.subsec_nanos() as i32));
            let heap = &mut vm.heap.lock().unwrap();
            let obj_ref = heap.allocate(HeapValue::Object {
                class_name: "java/time/Instant".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/time/Instant", "ofEpochSecond", "(J)Ljava/time/Instant;") => {
            let epoch_second = args[0].as_long()?;
            let mut fields = std::collections::HashMap::new();
            fields.insert("__epoch_second".to_string(), Value::Long(epoch_second));
            fields.insert("__nano".to_string(), Value::Int(0));
            let heap = &mut vm.heap.lock().unwrap();
            let obj_ref = heap.allocate(HeapValue::Object {
                class_name: "java/time/Instant".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/time/Instant", "ofEpochSecond", "(JJ)Ljava/time/Instant;") => {
            let epoch_second = args[0].as_long()?;
            let nano_adj = args[1].as_long()?;
            let mut fields = std::collections::HashMap::new();
            fields.insert("__epoch_second".to_string(), Value::Long(epoch_second + nano_adj / 1_000_000_000));
            fields.insert("__nano".to_string(), Value::Int((nano_adj % 1_000_000_000) as i32));
            let heap = &mut vm.heap.lock().unwrap();
            let obj_ref = heap.allocate(HeapValue::Object {
                class_name: "java/time/Instant".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/time/Instant", "ofEpochMilli", "(J)Ljava/time/Instant;") => {
            let epoch_milli = args[0].as_long()?;
            let mut fields = std::collections::HashMap::new();
            fields.insert("__epoch_second".to_string(), Value::Long(epoch_milli / 1000));
            fields.insert("__nano".to_string(), Value::Int(((epoch_milli % 1000) * 1_000_000) as i32));
            let heap = &mut vm.heap.lock().unwrap();
            let obj_ref = heap.allocate(HeapValue::Object {
                class_name: "java/time/Instant".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/time/Instant", "getEpochSecond", "()J") => {
            let this_ref = args[0].as_reference()?;
            get_instant_epoch_second(vm, this_ref)
        }
        ("java/time/Instant", "getNano", "()I") => {
            let this_ref = args[0].as_reference()?;
            get_instant_nano(vm, this_ref)
        }
        ("java/time/Instant", "toEpochMilli", "()J") => {
            let epoch_second = args[0].as_reference().and_then(|r| get_instant_epoch_second(vm, r))?.unwrap_or(Value::Long(0)).as_long()?;
            let nano = args[0].as_reference().and_then(|r| get_instant_nano(vm, r))?.unwrap_or(Value::Int(0)).as_int()?;
            Ok(Some(Value::Long(epoch_second * 1000 + (nano / 1_000_000) as i64)))
        }
        ("java/time/Instant", "isAfter", "(Ljava/time/Instant;)Z") => {
            let other_ref = args[0].as_reference()?;
            let this_ref = args[1].as_reference()?;
            let this_epoch = get_instant_epoch_second(vm, this_ref)?.unwrap_or(Value::Long(0)).as_long()?;
            let other_epoch = get_instant_epoch_second(vm, other_ref)?.unwrap_or(Value::Long(0)).as_long()?;
            Ok(Some(Value::Int(if this_epoch > other_epoch { 1 } else { 0 })))
        }
        ("java/time/Instant", "isBefore", "(Ljava/time/Instant;)Z") => {
            let other_ref = args[0].as_reference()?;
            let this_ref = args[1].as_reference()?;
            let this_epoch = get_instant_epoch_second(vm, this_ref)?.unwrap_or(Value::Long(0)).as_long()?;
            let other_epoch = get_instant_epoch_second(vm, other_ref)?.unwrap_or(Value::Long(0)).as_long()?;
            Ok(Some(Value::Int(if this_epoch < other_epoch { 1 } else { 0 })))
        }
        ("java/time/Instant", "plusSeconds", "(J)Ljava/time/Instant;") => {
            let seconds = args[0].as_long()?;
            let this_ref = args[1].as_reference()?;
            let epoch_second = get_instant_epoch_second(vm, this_ref)?.unwrap_or(Value::Long(0)).as_long()?;
            let nano = get_instant_nano(vm, this_ref)?.unwrap_or(Value::Int(0)).as_int()?;
            let mut fields = std::collections::HashMap::new();
            fields.insert("__epoch_second".to_string(), Value::Long(epoch_second + seconds));
            fields.insert("__nano".to_string(), Value::Int(nano));
            let heap = &mut vm.heap.lock().unwrap();
            let obj_ref = heap.allocate(HeapValue::Object {
                class_name: "java/time/Instant".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/time/Instant", "plusMillis", "(J)Ljava/time/Instant;") => {
            let millis = args[0].as_long()?;
            let this_ref = args[1].as_reference()?;
            let epoch_second = get_instant_epoch_second(vm, this_ref)?.unwrap_or(Value::Long(0)).as_long()?;
            let nano = get_instant_nano(vm, this_ref)?.unwrap_or(Value::Int(0)).as_int()?;
            let total_millis = epoch_second * 1000 + millis + (nano / 1_000_000) as i64;
            let mut fields = std::collections::HashMap::new();
            fields.insert("__epoch_second".to_string(), Value::Long(total_millis / 1000));
            fields.insert("__nano".to_string(), Value::Int(((total_millis % 1000) * 1_000_000) as i32));
            let heap = &mut vm.heap.lock().unwrap();
            let obj_ref = heap.allocate(HeapValue::Object {
                class_name: "java/time/Instant".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/time/Instant", "plusNanos", "(J)Ljava/time/Instant;") => {
            let nanos = args[0].as_long()?;
            let this_ref = args[1].as_reference()?;
            let epoch_second = get_instant_epoch_second(vm, this_ref)?.unwrap_or(Value::Long(0)).as_long()?;
            let nano = get_instant_nano(vm, this_ref)?.unwrap_or(Value::Int(0)).as_int()?;
            let total_nanos = (epoch_second * 1_000_000_000 + nanos as i64) + nano as i64;
            let mut fields = std::collections::HashMap::new();
            fields.insert("__epoch_second".to_string(), Value::Long(total_nanos / 1_000_000_000));
            fields.insert("__nano".to_string(), Value::Int((total_nanos % 1_000_000_000) as i32));
            let heap = &mut vm.heap.lock().unwrap();
            let obj_ref = heap.allocate(HeapValue::Object {
                class_name: "java/time/Instant".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/time/Instant", "minusSeconds", "(J)Ljava/time/Instant;") => {
            let seconds = args[0].as_long()?;
            let this_ref = args[1].as_reference()?;
            let epoch_second = get_instant_epoch_second(vm, this_ref)?.unwrap_or(Value::Long(0)).as_long()?;
            let nano = get_instant_nano(vm, this_ref)?.unwrap_or(Value::Int(0)).as_int()?;
            let mut fields = std::collections::HashMap::new();
            fields.insert("__epoch_second".to_string(), Value::Long(epoch_second - seconds));
            fields.insert("__nano".to_string(), Value::Int(nano));
            let heap = &mut vm.heap.lock().unwrap();
            let obj_ref = heap.allocate(HeapValue::Object {
                class_name: "java/time/Instant".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/time/Instant", "minusMillis", "(J)Ljava/time/Instant;") => {
            let millis = args[0].as_long()?;
            let this_ref = args[1].as_reference()?;
            let epoch_second = get_instant_epoch_second(vm, this_ref)?.unwrap_or(Value::Long(0)).as_long()?;
            let nano = get_instant_nano(vm, this_ref)?.unwrap_or(Value::Int(0)).as_int()?;
            let total_millis = epoch_second * 1000 - millis + (nano / 1_000_000) as i64;
            let mut fields = std::collections::HashMap::new();
            fields.insert("__epoch_second".to_string(), Value::Long(total_millis / 1000));
            fields.insert("__nano".to_string(), Value::Int(((total_millis % 1000) * 1_000_000) as i32));
            let heap = &mut vm.heap.lock().unwrap();
            let obj_ref = heap.allocate(HeapValue::Object {
                class_name: "java/time/Instant".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/time/Instant", "compareTo", "(Ljava/time/Instant;)I") => {
            let other_ref = args[0].as_reference()?;
            let this_ref = args[1].as_reference()?;
            let this_epoch = get_instant_epoch_second(vm, this_ref)?.unwrap_or(Value::Long(0)).as_long()?;
            let this_nano = get_instant_nano(vm, this_ref)?.unwrap_or(Value::Int(0)).as_int()?;
            let other_epoch = get_instant_epoch_second(vm, other_ref)?.unwrap_or(Value::Long(0)).as_long()?;
            let other_nano = get_instant_nano(vm, other_ref)?.unwrap_or(Value::Int(0)).as_int()?;
            let cmp = if this_epoch != other_epoch {
                (this_epoch - other_epoch).cmp(&0)
            } else {
                (this_nano - other_nano).cmp(&0)
            };
            Ok(Some(Value::Int(match cmp {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            })))
        }
        ("java/time/Instant", "equals", "(Ljava/lang/Object;)Z") => {
            let other_ref = args[0].as_reference()?;
            let this_ref = args[1].as_reference()?;
            if other_ref == Reference::Null {
                return Ok(Some(Value::Int(0)));
            }
            let this_epoch = get_instant_epoch_second(vm, this_ref)?.unwrap_or(Value::Long(0)).as_long()?;
            let this_nano = get_instant_nano(vm, this_ref)?.unwrap_or(Value::Int(0)).as_int()?;
            let other_epoch = get_instant_epoch_second(vm, other_ref)?.unwrap_or(Value::Long(0)).as_long()?;
            let other_nano = get_instant_nano(vm, other_ref)?.unwrap_or(Value::Int(0)).as_int()?;
            Ok(Some(Value::Int(if this_epoch == other_epoch && this_nano == other_nano { 1 } else { 0 })))
        }
        ("java/time/Instant", "hashCode", "()I") => {
            let this_ref = args[0].as_reference()?;
            let epoch_second = get_instant_epoch_second(vm, this_ref)?.unwrap_or(Value::Long(0)).as_long()?;
            let nano = get_instant_nano(vm, this_ref)?.unwrap_or(Value::Int(0)).as_int()?;
            Ok(Some(Value::Int((epoch_second ^ (epoch_second >> 32)) as i32 ^ nano)))
        }
        ("java/time/Instant", "toString", "()Ljava/lang/String;") => {
            let this_ref = args[0].as_reference()?;
            let epoch_second = get_instant_epoch_second(vm, this_ref)?.unwrap_or(Value::Long(0)).as_long()?;
            let nano = get_instant_nano(vm, this_ref)?.unwrap_or(Value::Int(0)).as_int()?;
            Ok(Some(vm.new_string(format!("{}.{:09}", epoch_second, nano))))
        }
        ("java/time/Duration", "ofSeconds", "(J)Ljava/time/Duration;") => {
            let seconds = args[0].as_long()?;
            let mut fields = std::collections::HashMap::new();
            fields.insert("__seconds".to_string(), Value::Long(seconds));
            fields.insert("__nano".to_string(), Value::Int(0));
            let heap = &mut vm.heap.lock().unwrap();
            let obj_ref = heap.allocate(HeapValue::Object {
                class_name: "java/time/Duration".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/time/Duration", "ofMillis", "(J)Ljava/time/Duration;") => {
            let millis = args[0].as_long()?;
            let mut fields = std::collections::HashMap::new();
            fields.insert("__seconds".to_string(), Value::Long(millis / 1000));
            fields.insert("__nano".to_string(), Value::Int(((millis % 1000) * 1_000_000) as i32));
            let heap = &mut vm.heap.lock().unwrap();
            let obj_ref = heap.allocate(HeapValue::Object {
                class_name: "java/time/Duration".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/time/Duration", "ofNanos", "(J)Ljava/time/Duration;") => {
            let nanos = args[0].as_long()?;
            let mut fields = std::collections::HashMap::new();
            fields.insert("__seconds".to_string(), Value::Long(nanos / 1_000_000_000));
            fields.insert("__nano".to_string(), Value::Int((nanos % 1_000_000_000) as i32));
            let heap = &mut vm.heap.lock().unwrap();
            let obj_ref = heap.allocate(HeapValue::Object {
                class_name: "java/time/Duration".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/time/Duration", "getSeconds", "()J") => {
            let this_ref = args[0].as_reference()?;
            let heap = vm.heap.lock().unwrap();
            if let Ok(HeapValue::Object { fields, .. }) = heap.get(this_ref) {
                if let Some(Value::Long(s)) = fields.get("__seconds") {
                    return Ok(Some(Value::Long(*s)));
                }
            }
            Ok(Some(Value::Long(0)))
        }
        ("java/time/Duration", "getNano", "()I") => {
            let this_ref = args[0].as_reference()?;
            let heap = vm.heap.lock().unwrap();
            if let Ok(HeapValue::Object { fields, .. }) = heap.get(this_ref) {
                if let Some(Value::Int(n)) = fields.get("__nano") {
                    return Ok(Some(Value::Int(*n)));
                }
            }
            Ok(Some(Value::Int(0)))
        }
        ("java/time/Duration", "toMillis", "()J") => {
            let this_ref = args[0].as_reference()?;
            let heap = vm.heap.lock().unwrap();
            if let Ok(HeapValue::Object { fields, .. }) = heap.get(this_ref) {
                let seconds = fields.get("__seconds").and_then(|v| if let Value::Long(s) = v { Some(*s) } else { None }).unwrap_or(0);
                let nano = fields.get("__nano").and_then(|v| if let Value::Int(n) = v { Some(*n) } else { None }).unwrap_or(0);
                return Ok(Some(Value::Long(seconds * 1000 + (nano / 1_000_000) as i64)));
            }
            Ok(Some(Value::Long(0)))
        }
        ("java/time/Duration", "toMicros", "()J") => {
            let this_ref = args[0].as_reference()?;
            let heap = vm.heap.lock().unwrap();
            if let Ok(HeapValue::Object { fields, .. }) = heap.get(this_ref) {
                let seconds = fields.get("__seconds").and_then(|v| if let Value::Long(s) = v { Some(*s) } else { None }).unwrap_or(0);
                let nano = fields.get("__nano").and_then(|v| if let Value::Int(n) = v { Some(*n) } else { None }).unwrap_or(0);
                return Ok(Some(Value::Long(seconds * 1_000_000 + (nano / 1000) as i64)));
            }
            Ok(Some(Value::Long(0)))
        }
        ("java/time/Duration", "isNegative", "()Z") => {
            let this_ref = args[0].as_reference()?;
            let heap = vm.heap.lock().unwrap();
            if let Ok(HeapValue::Object { fields, .. }) = heap.get(this_ref) {
                let seconds = fields.get("__seconds").and_then(|v| if let Value::Long(s) = v { Some(*s) } else { None }).unwrap_or(0);
                return Ok(Some(Value::Int(if seconds < 0 { 1 } else { 0 })));
            }
            Ok(Some(Value::Int(0)))
        }
        ("java/time/Duration", "isZero", "()Z") => {
            let this_ref = args[0].as_reference()?;
            let heap = vm.heap.lock().unwrap();
            if let Ok(HeapValue::Object { fields, .. }) = heap.get(this_ref) {
                let seconds = fields.get("__seconds").and_then(|v| if let Value::Long(s) = v { Some(*s) } else { None }).unwrap_or(0);
                let nano = fields.get("__nano").and_then(|v| if let Value::Int(n) = v { Some(*n) } else { None }).unwrap_or(0);
                return Ok(Some(Value::Int(if seconds == 0 && nano == 0 { 1 } else { 0 })));
            }
            Ok(Some(Value::Int(0)))
        }
        ("java/time/Duration", "compareTo", "(Ljava/time/Duration;)I") => {
            let other_ref = args[0].as_reference()?;
            let this_ref = args[1].as_reference()?;
            let heap = vm.heap.lock().unwrap();
            let this_seconds = if let Ok(HeapValue::Object { fields, .. }) = heap.get(this_ref) {
                fields.get("__seconds").and_then(|v| if let Value::Long(s) = v { Some(*s) } else { None }).unwrap_or(0)
            } else { 0 };
            let this_nano = if let Ok(HeapValue::Object { fields, .. }) = heap.get(this_ref) {
                fields.get("__nano").and_then(|v| if let Value::Int(n) = v { Some(*n) } else { None }).unwrap_or(0)
            } else { 0 };
            let other_seconds = if let Ok(HeapValue::Object { fields, .. }) = heap.get(other_ref) {
                fields.get("__seconds").and_then(|v| if let Value::Long(s) = v { Some(*s) } else { None }).unwrap_or(0)
            } else { 0 };
            let other_nano = if let Ok(HeapValue::Object { fields, .. }) = heap.get(other_ref) {
                fields.get("__nano").and_then(|v| if let Value::Int(n) = v { Some(*n) } else { None }).unwrap_or(0)
            } else { 0 };
            let cmp = if this_seconds != other_seconds {
                (this_seconds - other_seconds).cmp(&0)
            } else {
                (this_nano - other_nano).cmp(&0)
            };
            Ok(Some(Value::Int(match cmp {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            })))
        }
        ("java/time/Duration", "toString", "()Ljava/lang/String;") => {
            let this_ref = args[0].as_reference()?;
            let heap = vm.heap.lock().unwrap();
            let (seconds, nano) = if let Ok(HeapValue::Object { fields, .. }) = heap.get(this_ref) {
                let s = fields.get("__seconds").and_then(|v| if let Value::Long(s) = v { Some(*s) } else { None }).unwrap_or(0);
                let n = fields.get("__nano").and_then(|v| if let Value::Int(n) = v { Some(*n) } else { None }).unwrap_or(0);
                (s, n)
            } else {
                (0, 0)
            };
            drop(heap);
            let sign = if seconds < 0 || (seconds == 0 && nano < 0) { "-" } else { "" };
            Ok(Some(vm.new_string(format!("{}PT{}S", sign, seconds.abs()))))
        }
        _ => Err(VmError::UnhandledException {
            class_name: "".to_string(),
        }),
    }
}

fn get_instant_epoch_second(vm: &Vm, reference: Reference) -> Result<Option<Value>, VmError> {
    let heap = vm.heap.lock().unwrap();
    if let Ok(HeapValue::Object { fields, .. }) = heap.get(reference) {
        if let Some(Value::Long(s)) = fields.get("__epoch_second") {
            return Ok(Some(Value::Long(*s)));
        }
    }
    Ok(None)
}

fn get_instant_nano(vm: &Vm, reference: Reference) -> Result<Option<Value>, VmError> {
    let heap = vm.heap.lock().unwrap();
    if let Ok(HeapValue::Object { fields, .. }) = heap.get(reference) {
        if let Some(Value::Int(n)) = fields.get("__nano") {
            return Ok(Some(Value::Int(*n)));
        }
    }
    Ok(None)
}