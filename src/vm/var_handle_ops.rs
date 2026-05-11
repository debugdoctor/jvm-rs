//! VarHandle access-mode classifier and per-slot RMW operations. Kept apart
//! from `mod.rs` because the tables are large and self-contained.

use super::heap::HeapValue;
use super::types::{Reference, Value, VmError};

/// All VarHandle access modes collapse to one of these primitive operations.
/// Memory ordering (volatile / acquire / release / opaque / plain) does not
/// affect the operation here — the heap mutex provides SeqCst semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VarHandleAccess {
    Get,
    Set,
    CompareAndSet,
    CompareAndExchange,
    GetAndSet,
    GetAndAdd,
    GetAndBitwiseOr,
    GetAndBitwiseAnd,
    GetAndBitwiseXor,
}

pub(crate) fn classify_var_handle_access(name: &str) -> Option<VarHandleAccess> {
    match name {
        "get" | "getVolatile" | "getAcquire" | "getOpaque" => Some(VarHandleAccess::Get),
        "set" | "setVolatile" | "setRelease" | "setOpaque" => Some(VarHandleAccess::Set),
        "compareAndSet"
        | "weakCompareAndSet"
        | "weakCompareAndSetPlain"
        | "weakCompareAndSetAcquire"
        | "weakCompareAndSetRelease" => Some(VarHandleAccess::CompareAndSet),
        "compareAndExchange"
        | "compareAndExchangeAcquire"
        | "compareAndExchangeRelease" => Some(VarHandleAccess::CompareAndExchange),
        "getAndSet" | "getAndSetAcquire" | "getAndSetRelease" => Some(VarHandleAccess::GetAndSet),
        "getAndAdd" | "getAndAddAcquire" | "getAndAddRelease" => Some(VarHandleAccess::GetAndAdd),
        "getAndBitwiseOr" | "getAndBitwiseOrAcquire" | "getAndBitwiseOrRelease" => {
            Some(VarHandleAccess::GetAndBitwiseOr)
        }
        "getAndBitwiseAnd" | "getAndBitwiseAndAcquire" | "getAndBitwiseAndRelease" => {
            Some(VarHandleAccess::GetAndBitwiseAnd)
        }
        "getAndBitwiseXor" | "getAndBitwiseXorAcquire" | "getAndBitwiseXorRelease" => {
            Some(VarHandleAccess::GetAndBitwiseXor)
        }
        _ => None,
    }
}

/// Apply a `VarHandleAccess` op to a single mutable `Value` slot (used for
/// instance and static field access).
pub(crate) fn apply_var_handle_op(
    mode: VarHandleAccess,
    descriptor: &str,
    slot: &mut Value,
    payload: &[Value],
) -> Result<Option<Value>, VmError> {
    use VarHandleAccess::*;
    let coerce = |value: Value| coerce_to_descriptor(value, descriptor);
    match mode {
        Get => Ok(Some(*slot)),
        Set => {
            let new = coerce(*payload.first().ok_or(VmError::StackUnderflow)?)?;
            *slot = new;
            Ok(None)
        }
        CompareAndSet => {
            let expected = coerce(*payload.first().ok_or(VmError::StackUnderflow)?)?;
            let new = coerce(*payload.get(1).ok_or(VmError::StackUnderflow)?)?;
            let ok = value_equal(*slot, expected);
            if ok {
                *slot = new;
            }
            Ok(Some(Value::Int(if ok { 1 } else { 0 })))
        }
        CompareAndExchange => {
            let expected = coerce(*payload.first().ok_or(VmError::StackUnderflow)?)?;
            let new = coerce(*payload.get(1).ok_or(VmError::StackUnderflow)?)?;
            let prev = *slot;
            if value_equal(prev, expected) {
                *slot = new;
            }
            Ok(Some(prev))
        }
        GetAndSet => {
            let new = coerce(*payload.first().ok_or(VmError::StackUnderflow)?)?;
            let prev = *slot;
            *slot = new;
            Ok(Some(prev))
        }
        GetAndAdd | GetAndBitwiseOr | GetAndBitwiseAnd | GetAndBitwiseXor => {
            let operand = *payload.first().ok_or(VmError::StackUnderflow)?;
            let prev = *slot;
            let new = combine_rmw(mode, descriptor, prev, operand)?;
            *slot = new;
            Ok(Some(prev))
        }
    }
}

/// Apply a `VarHandleAccess` op to an array element. Uses the heap value
/// directly to avoid cloning the underlying `Vec`.
pub(crate) fn apply_var_handle_array_op(
    mode: VarHandleAccess,
    descriptor: &str,
    heap_value: &mut HeapValue,
    index: usize,
    payload: &[Value],
) -> Result<Option<Value>, VmError> {
    use VarHandleAccess::*;
    let bounds_err = |len: usize| VmError::ArrayIndexOutOfBounds {
        index: index as i32,
        len,
    };
    // Specialise per array kind.
    match heap_value {
        HeapValue::IntArray { values } => {
            if index >= values.len() {
                return Err(bounds_err(values.len()));
            }
            match mode {
                Get => Ok(Some(Value::Int(values[index]))),
                Set => {
                    let v = coerce_to_descriptor(*payload.first().ok_or(VmError::StackUnderflow)?, descriptor)?;
                    values[index] = v.as_int().unwrap_or(0);
                    Ok(None)
                }
                CompareAndSet => {
                    let exp = payload.first().ok_or(VmError::StackUnderflow)?.as_int()?;
                    let new = payload.get(1).ok_or(VmError::StackUnderflow)?.as_int()?;
                    let ok = values[index] == exp;
                    if ok {
                        values[index] = new;
                    }
                    Ok(Some(Value::Int(if ok { 1 } else { 0 })))
                }
                CompareAndExchange => {
                    let exp = payload.first().ok_or(VmError::StackUnderflow)?.as_int()?;
                    let new = payload.get(1).ok_or(VmError::StackUnderflow)?.as_int()?;
                    let prev = values[index];
                    if prev == exp {
                        values[index] = new;
                    }
                    Ok(Some(Value::Int(prev)))
                }
                GetAndSet => {
                    let new = payload.first().ok_or(VmError::StackUnderflow)?.as_int()?;
                    let prev = values[index];
                    values[index] = new;
                    Ok(Some(Value::Int(prev)))
                }
                GetAndAdd => {
                    let delta = payload.first().ok_or(VmError::StackUnderflow)?.as_int()?;
                    let prev = values[index];
                    values[index] = prev.wrapping_add(delta);
                    Ok(Some(Value::Int(prev)))
                }
                GetAndBitwiseOr => {
                    let v = payload.first().ok_or(VmError::StackUnderflow)?.as_int()?;
                    let prev = values[index];
                    values[index] = prev | v;
                    Ok(Some(Value::Int(prev)))
                }
                GetAndBitwiseAnd => {
                    let v = payload.first().ok_or(VmError::StackUnderflow)?.as_int()?;
                    let prev = values[index];
                    values[index] = prev & v;
                    Ok(Some(Value::Int(prev)))
                }
                GetAndBitwiseXor => {
                    let v = payload.first().ok_or(VmError::StackUnderflow)?.as_int()?;
                    let prev = values[index];
                    values[index] = prev ^ v;
                    Ok(Some(Value::Int(prev)))
                }
            }
        }
        HeapValue::LongArray { values } => {
            if index >= values.len() {
                return Err(bounds_err(values.len()));
            }
            match mode {
                Get => Ok(Some(Value::Long(values[index]))),
                Set => {
                    let v = payload.first().ok_or(VmError::StackUnderflow)?.as_long()?;
                    values[index] = v;
                    Ok(None)
                }
                CompareAndSet => {
                    let exp = payload.first().ok_or(VmError::StackUnderflow)?.as_long()?;
                    let new = payload.get(1).ok_or(VmError::StackUnderflow)?.as_long()?;
                    let ok = values[index] == exp;
                    if ok {
                        values[index] = new;
                    }
                    Ok(Some(Value::Int(if ok { 1 } else { 0 })))
                }
                CompareAndExchange => {
                    let exp = payload.first().ok_or(VmError::StackUnderflow)?.as_long()?;
                    let new = payload.get(1).ok_or(VmError::StackUnderflow)?.as_long()?;
                    let prev = values[index];
                    if prev == exp {
                        values[index] = new;
                    }
                    Ok(Some(Value::Long(prev)))
                }
                GetAndSet => {
                    let new = payload.first().ok_or(VmError::StackUnderflow)?.as_long()?;
                    let prev = values[index];
                    values[index] = new;
                    Ok(Some(Value::Long(prev)))
                }
                GetAndAdd => {
                    let delta = payload.first().ok_or(VmError::StackUnderflow)?.as_long()?;
                    let prev = values[index];
                    values[index] = prev.wrapping_add(delta);
                    Ok(Some(Value::Long(prev)))
                }
                GetAndBitwiseOr => {
                    let v = payload.first().ok_or(VmError::StackUnderflow)?.as_long()?;
                    let prev = values[index];
                    values[index] = prev | v;
                    Ok(Some(Value::Long(prev)))
                }
                GetAndBitwiseAnd => {
                    let v = payload.first().ok_or(VmError::StackUnderflow)?.as_long()?;
                    let prev = values[index];
                    values[index] = prev & v;
                    Ok(Some(Value::Long(prev)))
                }
                GetAndBitwiseXor => {
                    let v = payload.first().ok_or(VmError::StackUnderflow)?.as_long()?;
                    let prev = values[index];
                    values[index] = prev ^ v;
                    Ok(Some(Value::Long(prev)))
                }
            }
        }
        HeapValue::ReferenceArray { values, .. } => {
            if index >= values.len() {
                return Err(bounds_err(values.len()));
            }
            match mode {
                Get => Ok(Some(Value::Reference(values[index]))),
                Set => {
                    let new = payload.first().ok_or(VmError::StackUnderflow)?.as_reference()?;
                    values[index] = new;
                    Ok(None)
                }
                CompareAndSet => {
                    let exp = payload.first().ok_or(VmError::StackUnderflow)?.as_reference()?;
                    let new = payload.get(1).ok_or(VmError::StackUnderflow)?.as_reference()?;
                    let ok = values[index] == exp;
                    if ok {
                        values[index] = new;
                    }
                    Ok(Some(Value::Int(if ok { 1 } else { 0 })))
                }
                CompareAndExchange => {
                    let exp = payload.first().ok_or(VmError::StackUnderflow)?.as_reference()?;
                    let new = payload.get(1).ok_or(VmError::StackUnderflow)?.as_reference()?;
                    let prev = values[index];
                    if prev == exp {
                        values[index] = new;
                    }
                    Ok(Some(Value::Reference(prev)))
                }
                GetAndSet => {
                    let new = payload.first().ok_or(VmError::StackUnderflow)?.as_reference()?;
                    let prev = values[index];
                    values[index] = new;
                    Ok(Some(Value::Reference(prev)))
                }
                _ => Err(VmError::UnsupportedNativeMethod {
                    class_name: "java/lang/invoke/VarHandle".to_string(),
                    method_name: "<arithmetic>".to_string(),
                    descriptor: descriptor.to_string(),
                }),
            }
        }
        HeapValue::FloatArray { values } => {
            if index >= values.len() {
                return Err(bounds_err(values.len()));
            }
            match mode {
                Get => Ok(Some(Value::Float(values[index]))),
                Set => {
                    values[index] = payload.first().ok_or(VmError::StackUnderflow)?.as_float()?;
                    Ok(None)
                }
                _ => Err(VmError::UnsupportedNativeMethod {
                    class_name: "java/lang/invoke/VarHandle".to_string(),
                    method_name: "<float-rmw>".to_string(),
                    descriptor: descriptor.to_string(),
                }),
            }
        }
        HeapValue::DoubleArray { values } => {
            if index >= values.len() {
                return Err(bounds_err(values.len()));
            }
            match mode {
                Get => Ok(Some(Value::Double(values[index]))),
                Set => {
                    values[index] = payload.first().ok_or(VmError::StackUnderflow)?.as_double()?;
                    Ok(None)
                }
                _ => Err(VmError::UnsupportedNativeMethod {
                    class_name: "java/lang/invoke/VarHandle".to_string(),
                    method_name: "<double-rmw>".to_string(),
                    descriptor: descriptor.to_string(),
                }),
            }
        }
        other => Err(VmError::InvalidHeapValue {
            expected: "array",
            actual: other.kind_name(),
        }),
    }
}

fn value_equal(a: Value, b: Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Long(x), Value::Long(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x.to_bits() == y.to_bits(),
        (Value::Double(x), Value::Double(y)) => x.to_bits() == y.to_bits(),
        (Value::Reference(x), Value::Reference(y)) => x == y,
        // Mixed: compare numerically where reasonable (Int slots holding
        // boolean/byte/char/short widths).
        (Value::Int(x), Value::Long(y)) | (Value::Long(y), Value::Int(x)) => (x as i64) == y,
        _ => false,
    }
}

fn combine_rmw(
    mode: VarHandleAccess,
    descriptor: &str,
    prev: Value,
    operand: Value,
) -> Result<Value, VmError> {
    use VarHandleAccess::*;
    let byte = descriptor.as_bytes().first().copied().unwrap_or(b'?');
    match byte {
        b'I' | b'B' | b'C' | b'S' | b'Z' => {
            let p = prev.as_int().unwrap_or(0);
            let o = operand.as_int().unwrap_or(0);
            Ok(Value::Int(match mode {
                GetAndAdd => p.wrapping_add(o),
                GetAndBitwiseOr => p | o,
                GetAndBitwiseAnd => p & o,
                GetAndBitwiseXor => p ^ o,
                _ => p,
            }))
        }
        b'J' => {
            let p = prev.as_long().unwrap_or(0);
            let o = operand.as_long().unwrap_or(0);
            Ok(Value::Long(match mode {
                GetAndAdd => p.wrapping_add(o),
                GetAndBitwiseOr => p | o,
                GetAndBitwiseAnd => p & o,
                GetAndBitwiseXor => p ^ o,
                _ => p,
            }))
        }
        _ => Err(VmError::UnsupportedNativeMethod {
            class_name: "java/lang/invoke/VarHandle".to_string(),
            method_name: format!("rmw-{mode:?}"),
            descriptor: descriptor.to_string(),
        }),
    }
}

/// Coerce a Value into a slot of the given descriptor. Mostly a no-op since
/// the interpreter already stores values widely; only int-shaped target
/// descriptors get a `Value::Int` re-wrap to maintain JVMS storage rules.
pub(crate) fn coerce_to_descriptor(value: Value, descriptor: &str) -> Result<Value, VmError> {
    let byte = descriptor.as_bytes().first().copied().unwrap_or(b'?');
    Ok(match (byte, value) {
        (b'I' | b'B' | b'C' | b'S' | b'Z', Value::Int(_)) => value,
        (b'J', Value::Long(_)) => value,
        (b'F', Value::Float(_)) => value,
        (b'D', Value::Double(_)) => value,
        (b'L' | b'[', Value::Reference(_)) => value,
        _ => value,
    })
}
