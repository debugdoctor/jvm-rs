use std::collections::HashMap;

use crate::vm::types::stub_return_value;
use crate::vm::{ClassMethod, HeapValue, Reference, RuntimeClass, Value, Vm, VmError};

pub(super) fn invoke_io(
    vm: &mut Vm,
    class_name: &str,
    method_name: &str,
    descriptor: &str,
    args: &[Value],
) -> Result<Option<Value>, VmError> {
    match (class_name, method_name, descriptor) {
        ("java/io/PrintStream", "println", "(I)V") => {
            let line = args[1].as_int()?.to_string();
            println!("{line}");
            vm.output.lock().unwrap().push(line);
            Ok(None)
        }
        ("java/io/PrintStream", "println", "(Z)V") => {
            let line = if args[1].as_int()? != 0 {
                "true"
            } else {
                "false"
            }
            .to_string();
            println!("{line}");
            vm.output.lock().unwrap().push(line);
            Ok(None)
        }
        ("java/io/PrintStream", "println", "(C)V") => {
            let ch = args[1].as_int()? as u8 as char;
            let line = ch.to_string();
            println!("{line}");
            vm.output.lock().unwrap().push(line);
            Ok(None)
        }
        ("java/io/PrintStream", "println", "(Ljava/lang/String;)V") => {
            let reference = args[1].as_reference()?;
            let line = crate::vm::builtin::helpers::stringify_reference(vm, reference)?;
            println!("{line}");
            vm.output.lock().unwrap().push(line);
            Ok(None)
        }
        ("java/io/PrintStream", "println", "(J)V") => {
            let line = args[1].as_long()?.to_string();
            println!("{line}");
            vm.output.lock().unwrap().push(line);
            Ok(None)
        }
        ("java/io/PrintStream", "println", "(F)V") => {
            let v = args[1].as_float()?;
            let line = crate::vm::builtin::format::format_float(v as f64);
            println!("{line}");
            vm.output.lock().unwrap().push(line);
            Ok(None)
        }
        ("java/io/PrintStream", "println", "(D)V") => {
            let v = args[1].as_double()?;
            let line = crate::vm::builtin::format::format_float(v);
            println!("{line}");
            vm.output.lock().unwrap().push(line);
            Ok(None)
        }
        ("java/io/PrintStream", "println", "()V") => {
            println!();
            vm.output.lock().unwrap().push(String::new());
            Ok(None)
        }
        ("java/io/PrintStream", "println", "(Ljava/lang/Object;)V") => {
            let reference = args[1].as_reference()?;
            let line = if reference == Reference::Null {
                "null".to_string()
            } else {
                vm.stringify_heap(reference)?
            };
            println!("{line}");
            vm.output.lock().unwrap().push(line);
            Ok(None)
        }
        ("java/io/PrintStream", "print", "(Ljava/lang/Object;)V") => {
            let reference = args[1].as_reference()?;
            let text = if reference == Reference::Null {
                "null".to_string()
            } else {
                vm.stringify_heap(reference)?
            };
            print!("{text}");
            Ok(None)
        }
        ("java/io/PrintStream", "print", "(I)V") => {
            let text = args[1].as_int()?.to_string();
            print!("{text}");
            Ok(None)
        }
        ("java/io/PrintStream", "print", "(Z)V") => {
            let text = if args[1].as_int()? != 0 {
                "true"
            } else {
                "false"
            };
            print!("{text}");
            Ok(None)
        }
        ("java/io/PrintStream", "print", "(C)V") => {
            let ch = args[1].as_int()? as u8 as char;
            print!("{ch}");
            Ok(None)
        }
        ("java/io/PrintStream", "print", "(Ljava/lang/String;)V") => {
            let reference = args[1].as_reference()?;
            let text = crate::vm::builtin::helpers::stringify_reference(vm, reference)?;
            print!("{text}");
            Ok(None)
        }
        ("java/io/PrintStream", "print", "(J)V") => {
            let text = args[1].as_long()?.to_string();
            print!("{text}");
            Ok(None)
        }
        ("java/io/PrintStream", "print", "(F)V") => {
            let text = crate::vm::builtin::format::format_float(args[1].as_float()? as f64);
            print!("{text}");
            Ok(None)
        }
        ("java/io/PrintStream", "print", "(D)V") => {
            let text = crate::vm::builtin::format::format_float(args[1].as_double()?);
            print!("{text}");
            Ok(None)
        }
        ("java/io/PrintStream", "print", "()V") => Ok(None),
        _ => Err(VmError::UnhandledException {
            class_name: "".to_string(),
        }),
    }
}

pub(super) fn invoke_lang(
    vm: &mut Vm,
    class_name: &str,
    method_name: &str,
    descriptor: &str,
    args: &[Value],
) -> Result<Option<Value>, VmError> {
    match (class_name, method_name, descriptor) {
        ("java/lang/Object", "wait", "()V") => {
            vm.wait_on_monitor(args[0].as_reference()?)?;
            Ok(None)
        }
        ("java/lang/Object", "notify", "()V") => {
            vm.notify_monitor(args[0].as_reference()?, false)?;
            Ok(None)
        }
        ("java/lang/Object", "notifyAll", "()V") => {
            vm.notify_monitor(args[0].as_reference()?, true)?;
            Ok(None)
        }
        ("java/lang/Object", "hashCode", "()I") => {
            let r = args[0].as_reference()?;
            Ok(Some(Value::Int(match r {
                Reference::Null => 0,
                Reference::Heap(i) => i as i32,
            })))
        }
        ("java/lang/Object", "equals", "(Ljava/lang/Object;)Z") => {
            Ok(Some(Value::Int(i32::from(
                args[0].as_reference()? == args[1].as_reference()?,
            ))))
        }
        ("java/lang/Object", "toString", "()Ljava/lang/String;") => {
            let r = args[0].as_reference()?;
            let (cls, id) = match r {
                Reference::Null => ("null".to_string(), 0usize),
                Reference::Heap(i) => {
                    let name = match vm.heap.lock().unwrap().get(r)? {
                        HeapValue::Object { class_name, .. } => class_name.clone(),
                        v => v.kind_name().to_string(),
                    };
                    (name, i)
                }
            };
            Ok(Some(vm.new_string(format!("{}@{:x}", cls.replace('/', "."), id))))
        }
        ("java/lang/Object", "getClass", "()Ljava/lang/Class;") => {
            let r = args[0].as_reference()?;
            let class_name = match r {
                Reference::Null => return Err(VmError::NullReference),
                Reference::Heap(_) => match vm.heap.lock().unwrap().get(r)? {
                    HeapValue::Object { class_name, .. } => class_name.clone(),
                    HeapValue::String(_) => "java/lang/String".to_string(),
                    HeapValue::StringBuilder(_) => "java/lang/StringBuilder".to_string(),
                    HeapValue::IntArray { .. } => "[I".to_string(),
                    HeapValue::LongArray { .. } => "[J".to_string(),
                    HeapValue::FloatArray { .. } => "[F".to_string(),
                    HeapValue::DoubleArray { .. } => "[D".to_string(),
                    HeapValue::ReferenceArray { component_type, .. } => {
                        format!("[{component_type}")
                    }
                },
            };
            Ok(Some(Value::Reference(vm.class_object(&class_name))))
        }
        ("java/lang/String", "length", "()I") => {
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            Ok(Some(Value::Int(s.len() as i32)))
        }
        ("java/lang/String", "charAt", "(I)C") => {
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            let index = args[1].as_int()?;
            let ch = s.chars().nth(index as usize).unwrap_or('\0');
            Ok(Some(Value::Int(ch as i32)))
        }
        ("java/lang/String", "equals", "(Ljava/lang/Object;)Z") => {
            let a = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            let b_ref = args[1].as_reference()?;
            let result = match b_ref {
                Reference::Null => 0,
                _ => {
                    if let Ok(b) = crate::vm::builtin::helpers::stringify_reference(vm, b_ref) {
                        if a == b { 1 } else { 0 }
                    } else {
                        0
                    }
                }
            };
            Ok(Some(Value::Int(result)))
        }
        ("java/lang/String", "hashCode", "()I") => {
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            let mut h: i32 = 0;
            for ch in s.chars() {
                h = h.wrapping_mul(31).wrapping_add(ch as i32);
            }
            Ok(Some(Value::Int(h)))
        }
        ("java/lang/String", "isEmpty", "()Z") => {
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            Ok(Some(Value::Int(if s.is_empty() { 1 } else { 0 })))
        }
        ("java/lang/String", "trim", "()Ljava/lang/String;") => {
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            Ok(Some(vm.new_string(s.trim().to_string())))
        }
        ("java/lang/String", "toLowerCase", "()Ljava/lang/String;") => {
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            Ok(Some(vm.new_string(s.to_lowercase())))
        }
        ("java/lang/String", "toUpperCase", "()Ljava/lang/String;") => {
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            Ok(Some(vm.new_string(s.to_uppercase())))
        }
        ("java/lang/String", "toString", "()Ljava/lang/String;") => {
            Ok(Some(Value::Reference(args[0].as_reference()?)))
        }
        ("java/lang/String", "concat", "(Ljava/lang/String;)Ljava/lang/String;") => {
            let mut a = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            let b = crate::vm::builtin::helpers::stringify_reference(vm, args[1].as_reference()?)?;
            a.push_str(&b);
            Ok(Some(vm.new_string(a)))
        }
        ("java/lang/String", "substring", "(I)Ljava/lang/String;") => {
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            let start = args[1].as_int()?;
            let chars: Vec<char> = s.chars().collect();
            let len = chars.len() as i32;
            if start < 0 || start > len {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/StringIndexOutOfBoundsException".to_string(),
                });
            }
            let sub: String = chars[start as usize..].iter().collect();
            Ok(Some(vm.new_string(sub)))
        }
        ("java/lang/String", "substring", "(II)Ljava/lang/String;") => {
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            let start = args[1].as_int()?;
            let end = args[2].as_int()?;
            let chars: Vec<char> = s.chars().collect();
            let len = chars.len() as i32;
            if start < 0 || end > len || start > end {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/StringIndexOutOfBoundsException".to_string(),
                });
            }
            let sub: String = chars[start as usize..end as usize].iter().collect();
            Ok(Some(vm.new_string(sub)))
        }
        ("java/lang/String", "indexOf", "(I)I") => {
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            let ch = args[1].as_int()? as u32;
            let needle = char::from_u32(ch).unwrap_or('\0');
            let pos = s.chars().position(|c| c == needle);
            Ok(Some(Value::Int(pos.map(|p| p as i32).unwrap_or(-1))))
        }
        ("java/lang/String", "indexOf", "(Ljava/lang/String;)I") => {
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            let needle = crate::vm::builtin::helpers::stringify_reference(vm, args[1].as_reference()?)?;
            let pos = match s.find(&needle) {
                Some(byte_pos) => s[..byte_pos].chars().count() as i32,
                None => -1,
            };
            Ok(Some(Value::Int(pos)))
        }
        ("java/lang/String", "startsWith", "(Ljava/lang/String;)Z") => {
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            let prefix = crate::vm::builtin::helpers::stringify_reference(vm, args[1].as_reference()?)?;
            Ok(Some(Value::Int(if s.starts_with(&prefix) { 1 } else { 0 })))
        }
        ("java/lang/String", "endsWith", "(Ljava/lang/String;)Z") => {
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            let suffix = crate::vm::builtin::helpers::stringify_reference(vm, args[1].as_reference()?)?;
            Ok(Some(Value::Int(if s.ends_with(&suffix) { 1 } else { 0 })))
        }
        ("java/lang/String", "contains", "(Ljava/lang/CharSequence;)Z") => {
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            let needle = crate::vm::builtin::helpers::stringify_reference(vm, args[1].as_reference()?)?;
            Ok(Some(Value::Int(if s.contains(&needle) { 1 } else { 0 })))
        }
        ("java/lang/String", "replace", "(CC)Ljava/lang/String;") => {
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            let from = char::from_u32(args[1].as_int()? as u32).unwrap_or('\0');
            let to = char::from_u32(args[2].as_int()? as u32).unwrap_or('\0');
            let result: String = s.chars().map(|c| if c == from { to } else { c }).collect();
            Ok(Some(vm.new_string(result)))
        }
        ("java/lang/String", "compareTo", "(Ljava/lang/String;)I") => {
            let a = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            let b = crate::vm::builtin::helpers::stringify_reference(vm, args[1].as_reference()?)?;
            let cmp = match a.cmp(&b) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            };
            Ok(Some(Value::Int(cmp)))
        }
        ("java/lang/String", "compareTo", "(Ljava/lang/Object;)I") => {
            let a_ref = args[0].as_reference()?;
            let b_ref = args[1].as_reference()?;
            let a = crate::vm::builtin::helpers::stringify_reference(vm, a_ref)?;
            let b = crate::vm::builtin::helpers::stringify_reference(vm, b_ref)?;
            let cmp = match a.cmp(&b) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            };
            Ok(Some(Value::Int(cmp)))
        }
        ("java/lang/String", "valueOf", "(I)Ljava/lang/String;") => {
            Ok(Some(vm.new_string(args[0].as_int()?.to_string())))
        }
        ("java/lang/String", "valueOf", "(J)Ljava/lang/String;") => {
            Ok(Some(vm.new_string(args[0].as_long()?.to_string())))
        }
        ("java/lang/String", "valueOf", "(Z)Ljava/lang/String;") => {
            let s = if args[0].as_int()? != 0 { "true" } else { "false" };
            Ok(Some(vm.new_string(s.to_string())))
        }
        ("java/lang/String", "valueOf", "(C)Ljava/lang/String;") => {
            let ch = char::from_u32(args[0].as_int()? as u32).unwrap_or('\0');
            Ok(Some(vm.new_string(ch.to_string())))
        }
        ("java/lang/String", "valueOf", "(D)Ljava/lang/String;") => {
            Ok(Some(vm.new_string(crate::vm::builtin::format::format_float(args[0].as_double()?))))
        }
        ("java/lang/String", "valueOf", "(F)Ljava/lang/String;") => {
            Ok(Some(vm.new_string(crate::vm::builtin::format::format_float(args[0].as_float()? as f64))))
        }
        ("java/lang/Integer", "numberOfLeadingZeros", "(I)I") => {
            Ok(Some(Value::Int(args[0].as_int()?.leading_zeros() as i32)))
        }
        ("java/lang/Integer", "numberOfTrailingZeros", "(I)I") => {
            Ok(Some(Value::Int(args[0].as_int()?.trailing_zeros() as i32)))
        }
        ("java/lang/Integer", "bitCount", "(I)I") => {
            Ok(Some(Value::Int(args[0].as_int()?.count_ones() as i32)))
        }
        ("java/lang/Integer", "reverse", "(I)I") => {
            Ok(Some(Value::Int(args[0].as_int()?.reverse_bits())))
        }
        ("java/lang/Integer", "reverseBytes", "(I)I") => {
            Ok(Some(Value::Int(args[0].as_int()?.swap_bytes())))
        }
        ("java/lang/Integer", "highestOneBit", "(I)I") => {
            let v = args[0].as_int()? as u32;
            Ok(Some(Value::Int(if v == 0 {
                0
            } else {
                (1u32 << (31 - v.leading_zeros())) as i32
            })))
        }
        ("java/lang/Integer", "lowestOneBit", "(I)I") => {
            let v = args[0].as_int()?;
            Ok(Some(Value::Int(v & v.wrapping_neg())))
        }
        ("java/lang/Integer", "signum", "(I)I") => {
            Ok(Some(Value::Int(args[0].as_int()?.signum())))
        }
        ("java/lang/Integer", "intValue", "()I") => {
            let obj_ref = args[0].as_reference()?;
            match vm.heap.lock().unwrap().get(obj_ref)? {
                HeapValue::Object { fields, .. } => {
                    let value = fields
                        .get("value")
                        .copied()
                        .unwrap_or(Value::Int(0));
                    Ok(Some(value))
                }
                _ => Ok(Some(Value::Int(0))),
            }
        }
        ("java/lang/Integer", "valueOf", "(I)Ljava/lang/Integer;") => {
            let value = args[0].as_int()?;
            let mut fields = HashMap::new();
            fields.insert("value".to_string(), Value::Int(value));
            let reference = vm.heap.lock().unwrap().allocate(HeapValue::Object {
                class_name: "java/lang/Integer".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(reference)))
        }
        ("java/lang/Integer", "parseInt", "(Ljava/lang/String;)I") => {
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            let value = s.parse::<i32>().map_err(|_| VmError::UnhandledException {
                class_name: "java/lang/NumberFormatException".to_string(),
            })?;
            Ok(Some(Value::Int(value)))
        }
        ("java/lang/Integer", "parseInt", "(Ljava/lang/String;I)I") => {
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            let radix = args[1].as_int()? as u32;
            let value = i32::from_str_radix(&s, radix).map_err(|_| {
                VmError::UnhandledException {
                    class_name: "java/lang/NumberFormatException".to_string(),
                }
            })?;
            Ok(Some(Value::Int(value)))
        }
        ("java/lang/Integer", "toString", "(I)Ljava/lang/String;") => {
            Ok(Some(vm.new_string(args[0].as_int()?.to_string())))
        }
        ("java/lang/Integer", "toString", "(II)Ljava/lang/String;") => {
            let value = args[0].as_int()?;
            let radix = args[1].as_int()? as u32;
            let s = match radix {
                2 => format!("{value:b}"),
                8 => format!("{value:o}"),
                16 => format!("{value:x}"),
                10 => value.to_string(),
                _ => value.to_string(),
            };
            let s = if value < 0 && radix != 10 {
                format!("-{}", crate::vm::builtin::format::format_unsigned_radix(value.unsigned_abs() as u64, radix))
            } else {
                s
            };
            Ok(Some(vm.new_string(s)))
        }
        ("java/lang/Integer", "toBinaryString", "(I)Ljava/lang/String;") => {
            Ok(Some(vm.new_string(format!("{:b}", args[0].as_int()? as u32))))
        }
        ("java/lang/Integer", "toHexString", "(I)Ljava/lang/String;") => {
            Ok(Some(vm.new_string(format!("{:x}", args[0].as_int()? as u32))))
        }
        ("java/lang/Integer", "toOctalString", "(I)Ljava/lang/String;") => {
            Ok(Some(vm.new_string(format!("{:o}", args[0].as_int()? as u32))))
        }
        ("java/lang/Integer", "compare", "(II)I") => {
            let a = args[0].as_int()?;
            let b = args[1].as_int()?;
            Ok(Some(Value::Int(a.cmp(&b) as i32)))
        }
        ("java/lang/Integer", "compareTo", "(Ljava/lang/Integer;)I")
        | ("java/lang/Integer", "compareTo", "(Ljava/lang/Object;)I") => {
            let a = crate::vm::builtin::helpers::integer_value(vm, args[0].as_reference()?)?;
            let b = crate::vm::builtin::helpers::integer_value(vm, args[1].as_reference()?)?;
            Ok(Some(Value::Int(a.cmp(&b) as i32)))
        }
        ("java/lang/Integer", "<init>", "(I)V") => {
            let obj_ref = args[0].as_reference()?;
            let value = args[1].as_int()?;
            if let Ok(HeapValue::Object { fields, .. }) = vm.heap.lock().unwrap().get_mut(obj_ref) {
                fields.insert("value".to_string(), Value::Int(value));
            }
            Ok(None)
        }
        ("java/lang/Long", "<init>", "(J)V") => {
            let obj_ref = args[0].as_reference()?;
            let value = args[1].as_long()?;
            if let Ok(HeapValue::Object { fields, .. }) = vm.heap.lock().unwrap().get_mut(obj_ref) {
                fields.insert("value".to_string(), Value::Long(value));
            }
            Ok(None)
        }
        ("java/lang/Long", "longValue", "()J") => {
            let obj_ref = args[0].as_reference()?;
            match vm.heap.lock().unwrap().get(obj_ref)? {
                HeapValue::Object { fields, .. } => Ok(Some(
                    fields.get("value").copied().unwrap_or(Value::Long(0)),
                )),
                _ => Ok(Some(Value::Long(0))),
            }
        }
        ("java/lang/Long", "valueOf", "(J)Ljava/lang/Long;") => {
            let value = args[0].as_long()?;
            let mut fields = HashMap::new();
            fields.insert("value".to_string(), Value::Long(value));
            let reference = vm.heap.lock().unwrap().allocate(HeapValue::Object {
                class_name: "java/lang/Long".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(reference)))
        }
        ("java/lang/Long", "parseLong", "(Ljava/lang/String;)J") => {
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            let value = s.parse::<i64>().map_err(|_| VmError::UnhandledException {
                class_name: "java/lang/NumberFormatException".to_string(),
            })?;
            Ok(Some(Value::Long(value)))
        }
        ("java/lang/Long", "toString", "(J)Ljava/lang/String;") => {
            Ok(Some(vm.new_string(args[0].as_long()?.to_string())))
        }
        ("java/lang/Long", "compare", "(JJ)I") => {
            let a = args[0].as_long()?;
            let b = args[1].as_long()?;
            Ok(Some(Value::Int(a.cmp(&b) as i32)))
        }
        ("java/lang/Character", "isDigit", "(C)Z") => {
            let ch = char::from_u32(args[0].as_int()? as u32).unwrap_or('\0');
            Ok(Some(Value::Int(if ch.is_ascii_digit() { 1 } else { 0 })))
        }
        ("java/lang/Character", "isLetter", "(C)Z") => {
            let ch = char::from_u32(args[0].as_int()? as u32).unwrap_or('\0');
            Ok(Some(Value::Int(if ch.is_alphabetic() { 1 } else { 0 })))
        }
        ("java/lang/Character", "isLetterOrDigit", "(C)Z") => {
            let ch = char::from_u32(args[0].as_int()? as u32).unwrap_or('\0');
            Ok(Some(Value::Int(if ch.is_alphanumeric() { 1 } else { 0 })))
        }
        ("java/lang/Character", "isWhitespace", "(C)Z") => {
            let ch = char::from_u32(args[0].as_int()? as u32).unwrap_or('\0');
            Ok(Some(Value::Int(if ch.is_whitespace() { 1 } else { 0 })))
        }
        ("java/lang/Character", "isUpperCase", "(C)Z") => {
            let ch = char::from_u32(args[0].as_int()? as u32).unwrap_or('\0');
            Ok(Some(Value::Int(if ch.is_uppercase() { 1 } else { 0 })))
        }
        ("java/lang/Character", "isLowerCase", "(C)Z") => {
            let ch = char::from_u32(args[0].as_int()? as u32).unwrap_or('\0');
            Ok(Some(Value::Int(if ch.is_lowercase() { 1 } else { 0 })))
        }
        ("java/lang/Character", "toLowerCase", "(C)C") => {
            let ch = char::from_u32(args[0].as_int()? as u32).unwrap_or('\0');
            let lower = ch.to_lowercase().next().unwrap_or(ch);
            Ok(Some(Value::Int(lower as i32)))
        }
        ("java/lang/Character", "toUpperCase", "(C)C") => {
            let ch = char::from_u32(args[0].as_int()? as u32).unwrap_or('\0');
            let upper = ch.to_uppercase().next().unwrap_or(ch);
            Ok(Some(Value::Int(upper as i32)))
        }
        ("java/lang/Character", "toString", "(C)Ljava/lang/String;") => {
            let ch = char::from_u32(args[0].as_int()? as u32).unwrap_or('\0');
            Ok(Some(vm.new_string(ch.to_string())))
        }
        ("java/lang/Boolean", "getBoolean", "(Ljava/lang/String;)Z") => {
            Ok(Some(Value::Int(0)))
        }
        ("java/lang/Boolean", "parseBoolean", "(Ljava/lang/String;)Z") => {
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            Ok(Some(Value::Int(if s.eq_ignore_ascii_case("true") { 1 } else { 0 })))
        }
        ("java/lang/Boolean", "toString", "(Z)Ljava/lang/String;") => {
            let s = if args[0].as_int()? != 0 { "true" } else { "false" };
            Ok(Some(vm.new_string(s.to_string())))
        }
        ("java/lang/Boolean", "valueOf", "(Z)Ljava/lang/Boolean;") => {
            let value = args[0].as_int()?;
            let mut fields = HashMap::new();
            fields.insert("value".to_string(), Value::Int(value));
            let reference = vm.heap.lock().unwrap().allocate(HeapValue::Object {
                class_name: "java/lang/Boolean".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(reference)))
        }
        ("java/lang/Boolean", "booleanValue", "()Z") => {
            let obj_ref = args[0].as_reference()?;
            match vm.heap.lock().unwrap().get(obj_ref)? {
                HeapValue::Object { fields, .. } => Ok(Some(
                    fields.get("value").copied().unwrap_or(Value::Int(0)),
                )),
                _ => Ok(Some(Value::Int(0))),
            }
        }
        ("java/lang/Math", "floor", "(D)D") => {
            Ok(Some(Value::Double(args[0].as_double()?.floor())))
        }
        ("java/lang/Math", "ceil", "(D)D") => {
            Ok(Some(Value::Double(args[0].as_double()?.ceil())))
        }
        ("java/lang/Math", "round", "(D)J") => {
            let v = args[0].as_double()?;
            let r = (v + 0.5).floor() as i64;
            Ok(Some(Value::Long(r)))
        }
        ("java/lang/Math", "round", "(F)I") => {
            let v = args[0].as_float()?;
            let r = (v + 0.5).floor() as i32;
            Ok(Some(Value::Int(r)))
        }
        ("java/lang/Math", "random", "()D") => {
            use std::sync::atomic::{AtomicU64, Ordering};
            static STATE: AtomicU64 = AtomicU64::new(0x9E3779B97F4A7C15);
            let mut x = STATE.load(Ordering::Relaxed);
            if x == 0 {
                x = 0x9E3779B97F4A7C15;
            }
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            STATE.store(x, Ordering::Relaxed);
            let bits = (x >> 11) & ((1u64 << 53) - 1);
            let v = bits as f64 / ((1u64 << 53) as f64);
            Ok(Some(Value::Double(v)))
        }
        ("java/lang/Math", "log", "(D)D") => {
            Ok(Some(Value::Double(args[0].as_double()?.ln())))
        }
        ("java/lang/Math", "log10", "(D)D") => {
            Ok(Some(Value::Double(args[0].as_double()?.log10())))
        }
        ("java/lang/Math", "exp", "(D)D") => {
            Ok(Some(Value::Double(args[0].as_double()?.exp())))
        }
        ("java/lang/Math", "sin", "(D)D") => {
            Ok(Some(Value::Double(args[0].as_double()?.sin())))
        }
        ("java/lang/Math", "cos", "(D)D") => {
            Ok(Some(Value::Double(args[0].as_double()?.cos())))
        }
        ("java/lang/Math", "tan", "(D)D") => {
            Ok(Some(Value::Double(args[0].as_double()?.tan())))
        }
        ("java/lang/Math", "floorDiv", "(II)I") => {
            let (x, y) = (args[0].as_int()?, args[1].as_int()?);
            if y == 0 {
                return Err(VmError::DivisionByZero);
            }
            Ok(Some(Value::Int(x.div_euclid(y).wrapping_add(
                if (x % y != 0) && ((x ^ y) < 0) { -1 + 1 } else { 0 },
            ))))
        }
        ("java/lang/Math", "floorDiv", "(JJ)J") => {
            let (x, y) = (args[0].as_long()?, args[1].as_long()?);
            if y == 0 {
                return Err(VmError::DivisionByZero);
            }
            let q = x / y;
            let q = if (x % y != 0) && ((x ^ y) < 0) { q - 1 } else { q };
            Ok(Some(Value::Long(q)))
        }
        ("java/lang/Math", "floorMod", "(II)I") => {
            let (x, y) = (args[0].as_int()?, args[1].as_int()?);
            if y == 0 {
                return Err(VmError::DivisionByZero);
            }
            let r = x % y;
            Ok(Some(Value::Int(if (r != 0) && ((r ^ y) < 0) { r + y } else { r })))
        }
        ("java/lang/Math", "floorMod", "(JJ)J") => {
            let (x, y) = (args[0].as_long()?, args[1].as_long()?);
            if y == 0 {
                return Err(VmError::DivisionByZero);
            }
            let r = x % y;
            Ok(Some(Value::Long(if (r != 0) && ((r ^ y) < 0) { r + y } else { r })))
        }
        ("java/lang/Math", "addExact", "(II)I") => {
            let (a, b) = (args[0].as_int()?, args[1].as_int()?);
            a.checked_add(b)
                .map(|v| Some(Value::Int(v)))
                .ok_or(VmError::DivisionByZero)
        }
        ("java/lang/Math", "addExact", "(JJ)J") => {
            let (a, b) = (args[0].as_long()?, args[1].as_long()?);
            a.checked_add(b)
                .map(|v| Some(Value::Long(v)))
                .ok_or(VmError::DivisionByZero)
        }
        ("java/lang/Math", "subtractExact", "(II)I") => {
            let (a, b) = (args[0].as_int()?, args[1].as_int()?);
            a.checked_sub(b)
                .map(|v| Some(Value::Int(v)))
                .ok_or(VmError::DivisionByZero)
        }
        ("java/lang/Math", "multiplyExact", "(II)I") => {
            let (a, b) = (args[0].as_int()?, args[1].as_int()?);
            a.checked_mul(b)
                .map(|v| Some(Value::Int(v)))
                .ok_or(VmError::DivisionByZero)
        }
        ("java/lang/Math", "multiplyExact", "(JJ)J") => {
            let (a, b) = (args[0].as_long()?, args[1].as_long()?);
            a.checked_mul(b)
                .map(|v| Some(Value::Long(v)))
                .ok_or(VmError::DivisionByZero)
        }
        ("java/lang/Math", "signum", "(I)I") => {
            Ok(Some(Value::Int(args[0].as_int()?.signum())))
        }
        ("java/lang/Math", "max", "(II)I") => {
            Ok(Some(Value::Int(args[0].as_int()?.max(args[1].as_int()?))))
        }
        ("java/lang/Math", "min", "(II)I") => {
            Ok(Some(Value::Int(args[0].as_int()?.min(args[1].as_int()?))))
        }
        ("java/lang/Math", "abs", "(I)I") => {
            Ok(Some(Value::Int(args[0].as_int()?.wrapping_abs())))
        }
        ("java/lang/Math", "max", "(JJ)J") => {
            Ok(Some(Value::Long(args[0].as_long()?.max(args[1].as_long()?))))
        }
        ("java/lang/Math", "min", "(JJ)J") => {
            Ok(Some(Value::Long(args[0].as_long()?.min(args[1].as_long()?))))
        }
        ("java/lang/Math", "abs", "(J)J") => {
            Ok(Some(Value::Long(args[0].as_long()?.wrapping_abs())))
        }
        ("java/lang/Math", "max", "(DD)D") => {
            Ok(Some(Value::Double(args[0].as_double()?.max(args[1].as_double()?))))
        }
        ("java/lang/Math", "min", "(DD)D") => {
            Ok(Some(Value::Double(args[0].as_double()?.min(args[1].as_double()?))))
        }
        ("java/lang/Math", "abs", "(D)D") => {
            Ok(Some(Value::Double(args[0].as_double()?.abs())))
        }
        ("java/lang/Math", "sqrt", "(D)D") => {
            Ok(Some(Value::Double(args[0].as_double()?.sqrt())))
        }
        ("java/lang/Math", "pow", "(DD)D") => {
            Ok(Some(Value::Double(args[0].as_double()?.powf(args[1].as_double()?))))
        }
        ("java/lang/StringBuilder", "<init>", "()V") => {
            let obj_ref = args[0].as_reference()?;
            *vm.heap.lock().unwrap().get_mut(obj_ref)? =
                HeapValue::StringBuilder(std::string::String::new());
            Ok(None)
        }
        ("java/lang/StringBuilder", "<init>", "(Ljava/lang/String;)V") => {
            let obj_ref = args[0].as_reference()?;
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[1].as_reference()?)?;
            *vm.heap.lock().unwrap().get_mut(obj_ref)? = HeapValue::StringBuilder(s);
            Ok(None)
        }
        ("java/lang/StringBuilder", "append", _) => {
            let obj_ref = args[0].as_reference()?;
            let text = crate::vm::builtin::helpers::format_value_for_append(vm, descriptor, &args[1..])?;
            if let HeapValue::StringBuilder(buf) = vm.heap.lock().unwrap().get_mut(obj_ref)? {
                buf.push_str(&text);
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/lang/StringBuilder", "toString", "()Ljava/lang/String;") => {
            let obj_ref = args[0].as_reference()?;
            let s = match vm.heap.lock().unwrap().get(obj_ref)? {
                HeapValue::StringBuilder(buf) => buf.clone(),
                _ => std::string::String::new(),
            };
            Ok(Some(vm.new_string(s)))
        }
        ("java/lang/StringBuilder", "length", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let len = match vm.heap.lock().unwrap().get(obj_ref)? {
                HeapValue::StringBuilder(buf) => buf.chars().count() as i32,
                _ => 0,
            };
            Ok(Some(Value::Int(len)))
        }
        ("java/lang/StringBuilder", "charAt", "(I)C") => {
            let obj_ref = args[0].as_reference()?;
            let index = args[1].as_int()?;
            let ch = match vm.heap.lock().unwrap().get(obj_ref)? {
                HeapValue::StringBuilder(buf) => {
                    buf.chars().nth(index as usize).ok_or_else(|| {
                        VmError::UnhandledException {
                            class_name: "java/lang/StringIndexOutOfBoundsException"
                                .to_string(),
                        }
                    })?
                }
                _ => '\0',
            };
            Ok(Some(Value::Int(ch as i32)))
        }
        ("java/lang/StringBuilder", "setLength", "(I)V") => {
            let obj_ref = args[0].as_reference()?;
            let new_len = args[1].as_int()?;
            if new_len < 0 {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/StringIndexOutOfBoundsException".to_string(),
                });
            }
            if let HeapValue::StringBuilder(buf) =
                vm.heap.lock().unwrap().get_mut(obj_ref)?
            {
                let current: Vec<char> = buf.chars().collect();
                let n = new_len as usize;
                if n <= current.len() {
                    *buf = current[..n].iter().collect();
                } else {
                    let mut s: String = current.into_iter().collect();
                    s.extend(std::iter::repeat('\0').take(n - s.chars().count()));
                    *buf = s;
                }
            }
            Ok(None)
        }
        ("java/lang/StringBuilder", "deleteCharAt", "(I)Ljava/lang/StringBuilder;") => {
            let obj_ref = args[0].as_reference()?;
            let index = args[1].as_int()?;
            if let HeapValue::StringBuilder(buf) =
                vm.heap.lock().unwrap().get_mut(obj_ref)?
            {
                let mut chars: Vec<char> = buf.chars().collect();
                if index < 0 || (index as usize) >= chars.len() {
                    return Err(VmError::UnhandledException {
                        class_name: "java/lang/StringIndexOutOfBoundsException"
                            .to_string(),
                    });
                }
                chars.remove(index as usize);
                *buf = chars.into_iter().collect();
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/lang/StringBuilder", "setCharAt", "(IC)V") => {
            let obj_ref = args[0].as_reference()?;
            let index = args[1].as_int()?;
            let ch = char::from_u32(args[2].as_int()? as u32).unwrap_or('\0');
            if let HeapValue::StringBuilder(buf) =
                vm.heap.lock().unwrap().get_mut(obj_ref)?
            {
                let mut chars: Vec<char> = buf.chars().collect();
                if index < 0 || (index as usize) >= chars.len() {
                    return Err(VmError::UnhandledException {
                        class_name: "java/lang/StringIndexOutOfBoundsException"
                            .to_string(),
                    });
                }
                chars[index as usize] = ch;
                *buf = chars.into_iter().collect();
            }
            Ok(None)
        }
        ("java/lang/StringBuilder", "reverse", "()Ljava/lang/StringBuilder;") => {
            let obj_ref = args[0].as_reference()?;
            if let HeapValue::StringBuilder(buf) =
                vm.heap.lock().unwrap().get_mut(obj_ref)?
            {
                *buf = buf.chars().rev().collect();
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/lang/StringBuilder", "insert", "(ILjava/lang/String;)Ljava/lang/StringBuilder;") => {
            let obj_ref = args[0].as_reference()?;
            let index = args[1].as_int()?;
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[2].as_reference()?)?;
            if let HeapValue::StringBuilder(buf) =
                vm.heap.lock().unwrap().get_mut(obj_ref)?
            {
                let mut chars: Vec<char> = buf.chars().collect();
                let n = chars.len() as i32;
                if index < 0 || index > n {
                    return Err(VmError::UnhandledException {
                        class_name: "java/lang/StringIndexOutOfBoundsException"
                            .to_string(),
                    });
                }
                let insert_chars: Vec<char> = s.chars().collect();
                let insert_at = index as usize;
                for (i, c) in insert_chars.into_iter().enumerate() {
                    chars.insert(insert_at + i, c);
                }
                *buf = chars.into_iter().collect();
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/lang/Thread", "currentThread", "()Ljava/lang/Thread;") => {
            const KEY: &str = "__current_thread";
            if let Some(r) = vm
                .runtime
                .lock()
                .unwrap()
                .class_objects
                .get(KEY)
                .copied()
            {
                return Ok(Some(Value::Reference(r)));
            }
            let reference = vm.heap.lock().unwrap().allocate(HeapValue::Object {
                class_name: "java/lang/Thread".to_string(),
                fields: HashMap::new(),
            });
            vm.runtime
                .lock()
                .unwrap()
                .class_objects
                .insert(KEY.to_string(), reference);
            Ok(Some(Value::Reference(reference)))
        }
        ("java/lang/Thread", "<init>", "()V") => {
            let obj_ref = args[0].as_reference()?;
            vm.set_object_field(obj_ref, "target", Value::Reference(Reference::Null))?;
            Ok(None)
        }
        ("java/lang/Thread", "<init>", "(Ljava/lang/Runnable;)V") => {
            let obj_ref = args[0].as_reference()?;
            vm.set_object_field(obj_ref, "target", args[1])?;
            Ok(None)
        }
        ("java/lang/Thread", "start", "()V") => {
            let thread_ref = args[0].as_reference()?;
            let target = vm.get_object_field(thread_ref, "target")?.as_reference()?;
            let receiver = if target == Reference::Null {
                thread_ref
            } else {
                target
            };
            let class_name = vm.get_object_class(receiver)?;
            vm.start_java_thread(
                thread_ref,
                &class_name,
                "run",
                "()V",
                vec![Value::Reference(receiver)],
            )?;
            Ok(None)
        }
        ("java/lang/Thread", "run", "()V") => {
            let thread_ref = args[0].as_reference()?;
            let target = vm.get_object_field(thread_ref, "target")?.as_reference()?;
            if target != Reference::Null {
                let class_name = vm.get_object_class(target)?;
                let (resolved_class, class_method) =
                    vm.resolve_method(&class_name, "run", "()V")?;
                match class_method {
                    ClassMethod::Native => {
                        vm.invoke_native(
                            &resolved_class,
                            "run",
                            "()V",
                            &[Value::Reference(target)],
                        )?;
                    }
                    ClassMethod::Bytecode(method) => {
                        let callee =
                            method.with_initial_locals(vec![Some(Value::Reference(target))]);
                        let _ = vm.execute(callee)?;
                    }
                }
            }
            Ok(None)
        }
        ("java/lang/Thread", "join", "()V") => {
            let thread_ref = args[0].as_reference()?;
            vm.join_java_thread(thread_ref)?;
            Ok(None)
        }
        ("java/lang/Thread", _, _) => {
            let _ = stub_return_value(descriptor);
            Ok(None)
        }
        ("java/lang/ThreadGroup", _, _) => {
            let _ = stub_return_value(descriptor);
            Ok(None)
        }
        ("java/lang/Class", "desiredAssertionStatus", "()Z") => Ok(Some(Value::Int(0))),
        ("java/lang/Class", "isArray", "()Z") => {
            let name = crate::vm::builtin::helpers::class_internal_name(vm, args[0].as_reference()?)?;
            Ok(Some(Value::Int(i32::from(name.starts_with('[')))))
        }
        ("java/lang/Class", "isPrimitive", "()Z") => {
            let name = crate::vm::builtin::helpers::class_internal_name(vm, args[0].as_reference()?)?;
            let primitive = matches!(
                name.as_str(),
                "boolean" | "byte" | "char" | "short" | "int" | "long" | "float" | "double" | "void"
            );
            Ok(Some(Value::Int(i32::from(primitive))))
        }
        ("java/lang/Class", "isInterface", "()Z") => Ok(Some(Value::Int(0))),
        ("java/lang/Class", "getName", "()Ljava/lang/String;")
        | ("java/lang/Class", "toString", "()Ljava/lang/String;") => {
            let internal = crate::vm::builtin::helpers::class_internal_name(vm, args[0].as_reference()?)?;
            let dotted = internal.replace('/', ".");
            Ok(Some(vm.new_string(dotted)))
        }
        ("java/lang/Class", "getSimpleName", "()Ljava/lang/String;") => {
            let internal = crate::vm::builtin::helpers::class_internal_name(vm, args[0].as_reference()?)?;
            let simple = internal
                .rsplit_once('/')
                .map(|(_, s)| s)
                .unwrap_or(internal.as_str())
                .rsplit_once('$')
                .map(|(_, s)| s.to_string())
                .unwrap_or_else(|| {
                    internal
                        .rsplit_once('/')
                        .map(|(_, s)| s.to_string())
                        .unwrap_or(internal.clone())
                });
            Ok(Some(vm.new_string(simple)))
        }
        ("java/lang/Runtime", "availableProcessors", "()I") => {
            let n = std::thread::available_parallelism()
                .map(|n| n.get() as i32)
                .unwrap_or(1);
            Ok(Some(Value::Int(n)))
        }
        ("java/lang/Runtime", "freeMemory", "()J")
        | ("java/lang/Runtime", "totalMemory", "()J")
        | ("java/lang/Runtime", "maxMemory", "()J") => {
            Ok(Some(Value::Long(256 * 1024 * 1024)))
        }
        ("java/lang/Runtime", "gc", "()V") => {
            vm.request_gc();
            Ok(None)
        }
        // --- InputStream stubs ---
        ("java/io/InputStream", "read", "()I") => Ok(Some(Value::Int(-1))),
        ("java/io/InputStream", "read", "([B)I") => Ok(Some(Value::Int(-1))),
        ("java/io/InputStream", "read", "([BII)I") => Ok(Some(Value::Int(-1))),
        ("java/io/InputStream", "skip", "(J)J") => Ok(Some(Value::Long(0))),
        ("java/io/InputStream", "available", "()I") => Ok(Some(Value::Int(0))),
        ("java/io/InputStream", "close", "()V") => Ok(None),
        ("java/io/InputStream", "reset", "()V") => Ok(None),
        ("java/io/InputStream", "mark", "(I)V") => Ok(None),
        ("java/io/InputStream", "markSupported", "()Z") => Ok(Some(Value::Int(0))),
        // --- OutputStream stubs ---
        ("java/io/OutputStream", "write", "(I)V") => Ok(None),
        ("java/io/OutputStream", "write", "([B)V") => Ok(None),
        ("java/io/OutputStream", "write", "([BII)V") => Ok(None),
        ("java/io/OutputStream", "flush", "()V") => Ok(None),
        ("java/io/OutputStream", "close", "()V") => Ok(None),
        // --- ByteArrayOutputStream native impl ---
        ("java/io/ByteArrayOutputStream", "<init>", "()V") => {
            let obj_ref = args[0].as_reference()?;
            let buf = vm.heap.lock().unwrap().allocate(HeapValue::IntArray {
                values: vec![0; 32],
            });
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields.insert("buf".to_string(), Value::Reference(buf));
                    fields.insert("count".to_string(), Value::Int(0));
                }
            }
            Ok(None)
        }
        ("java/io/ByteArrayOutputStream", "write", "(I)V") => {
            let obj_ref = args[0].as_reference()?;
            let b = args[1].as_int()? as i32;
            let (buf_ref, current_count) = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let buf_ref = fields.get("buf").and_then(|v| match v {
                            Value::Reference(r) => Some(*r),
                            _ => None,
                        });
                        let count = fields.get("count").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        });
                        (buf_ref, count)
                    }
                    _ => (None, None),
                }
            };
            if let (Some(buf_ref), Some(current_count)) = (buf_ref, current_count) {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::IntArray { values } = heap.get_mut(buf_ref)? {
                    if current_count as usize >= values.len() {
                        values.push(b);
                    } else {
                        values[current_count as usize] = b;
                    }
                    drop(values);
                    if let Ok(HeapValue::Object { fields, .. }) = heap.get_mut(obj_ref) {
                        fields.insert("count".to_string(), Value::Int(current_count + 1));
                    }
                }
            }
            Ok(None)
        }
        ("java/io/ByteArrayOutputStream", "write", "([B)V")
        | ("java/io/ByteArrayOutputStream", "write", "([BII)V") => {
            let obj_ref = args[0].as_reference()?;
            let buf_ref = args[1].as_reference()?;
            let (src_values, src_count) = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(buf_ref)? {
                    HeapValue::IntArray { values } => (values.clone(), values.len() as i32),
                    _ => (vec![], 0),
                }
            };
            let offset = if args.len() > 2 { args[2].as_int()? } else { 0 };
            let len = if args.len() > 3 { args[3].as_int()? } else { src_count };
            let offset = offset.max(0);
            let len = len.max(0).min(src_count.saturating_sub(offset));
            let (target_buf, current_count) = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let target_buf = fields.get("buf").and_then(|v| match v {
                            Value::Reference(r) => Some(*r),
                            _ => None,
                        });
                        let count = fields.get("count").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        });
                        (target_buf, count)
                    }
                    _ => (None, None),
                }
            };
            if let (Some(target_buf), Some(current_count)) = (target_buf, current_count) {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::IntArray { values: target } = heap.get_mut(target_buf)? {
                    for i in 0..len {
                        let idx = (offset + i) as usize;
                        if idx < src_values.len() {
                            target[(current_count + i) as usize] = src_values[idx];
                        }
                    }
                    drop(target);
                    if let Ok(HeapValue::Object { fields, .. }) = heap.get_mut(obj_ref) {
                        fields.insert("count".to_string(), Value::Int(current_count + len));
                    }
                }
            }
            Ok(None)
        }
        ("java/io/ByteArrayOutputStream", "toString", "()Ljava/lang/String;") => {
            let obj_ref = args[0].as_reference()?;
            let (buf_ref, count) = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let buf_ref = fields.get("buf").and_then(|v| match v {
                            Value::Reference(r) => Some(*r),
                            _ => None,
                        });
                        let count = fields.get("count").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        });
                        (buf_ref, count)
                    }
                    _ => (None, None),
                }
            };
            if let (Some(buf_ref), Some(count)) = (buf_ref, count) {
                let chars: String = {
                    let heap = vm.heap.lock().unwrap();
                    match heap.get(buf_ref)? {
                        HeapValue::IntArray { values } => {
                            values.iter()
                                .take(count as usize)
                                .map(|&v| v as u8 as char)
                                .collect()
                        }
                        _ => String::new(),
                    }
                };
                Ok(Some(vm.new_string(chars)))
            } else {
                Ok(Some(Value::Reference(Reference::Null)))
            }
        }
        ("java/io/ByteArrayOutputStream", "toByteArray", "()[B") => {
            let obj_ref = args[0].as_reference()?;
            let (buf_ref, count) = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let buf_ref = fields.get("buf").and_then(|v| match v {
                            Value::Reference(r) => Some(*r),
                            _ => None,
                        });
                        let count = fields.get("count").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        });
                        (buf_ref, count)
                    }
                    _ => (None, None),
                }
            };
            if let (Some(buf_ref), Some(count)) = (buf_ref, count) {
                let bytes: Vec<i32> = {
                    let heap = vm.heap.lock().unwrap();
                    match heap.get(buf_ref)? {
                        HeapValue::IntArray { values } => {
                            values.iter().take(count as usize).cloned().collect()
                        }
                        _ => vec![],
                    }
                };
                let arr_ref = vm.heap.lock().unwrap().allocate(HeapValue::IntArray { values: bytes });
                Ok(Some(Value::Reference(arr_ref)))
            } else {
                Ok(Some(Value::Reference(Reference::Null)))
            }
        }
        ("java/io/ByteArrayOutputStream", "size", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let count = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        fields.get("count").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        })
                    }
                    _ => None,
                }
            };
            Ok(Some(count.map(Value::Int).unwrap_or(Value::Int(0))))
        }
        ("java/io/ByteArrayOutputStream", "reset", "()V") => {
            let obj_ref = args[0].as_reference()?;
            let mut heap = vm.heap.lock().unwrap();
            if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                fields.insert("count".to_string(), Value::Int(0));
            }
            Ok(None)
        }
        ("java/io/ByteArrayOutputStream", "flush", "()V") => Ok(None),
        ("java/io/ByteArrayOutputStream", "close", "()V") => Ok(None),
        // --- Writer stubs ---
        ("java/io/Writer", "write", "(I)V") => Ok(None),
        ("java/io/Writer", "write", "([C)V") => Ok(None),
        ("java/io/Writer", "write", "([CII)V") => Ok(None),
        ("java/io/Writer", "write", "(Ljava/lang/String;)V") => Ok(None),
        ("java/io/Writer", "write", "(Ljava/lang/String;II)V") => Ok(None),
        ("java/io/Writer", "flush", "()V") => Ok(None),
        ("java/io/Writer", "close", "()V") => Ok(None),
        // --- BufferedWriter stubs ---
        ("java/io/BufferedWriter", "<init>", "(Ljava/io/Writer;)V") => Ok(None),
        ("java/io/BufferedWriter", "write", "(I)V") => Ok(None),
        ("java/io/BufferedWriter", "write", "([C)V") => Ok(None),
        ("java/io/BufferedWriter", "write", "([CII)V") => Ok(None),
        ("java/io/BufferedWriter", "flush", "()V") => Ok(None),
        ("java/io/BufferedWriter", "close", "()V") => Ok(None),
        // --- PrintWriter println/print ---
        ("java/io/PrintWriter", "println", "()V") => {
            println!("");
            vm.output.lock().unwrap().push(String::new());
            Ok(None)
        }
        ("java/io/PrintWriter", "println", "(Z)V") => {
            let line = if args[1].as_int()? != 0 { "true" } else { "false" }.to_string();
            println!("{line}");
            vm.output.lock().unwrap().push(line);
            Ok(None)
        }
        ("java/io/PrintWriter", "println", "(C)V") => {
            let ch = args[1].as_int()? as u8 as char;
            let line = ch.to_string();
            println!("{line}");
            vm.output.lock().unwrap().push(line);
            Ok(None)
        }
        ("java/io/PrintWriter", "println", "(I)V") => {
            let line = args[1].as_int()?.to_string();
            println!("{line}");
            vm.output.lock().unwrap().push(line);
            Ok(None)
        }
        ("java/io/PrintWriter", "println", "(J)V") => {
            let line = args[1].as_long()?.to_string();
            println!("{line}");
            vm.output.lock().unwrap().push(line);
            Ok(None)
        }
        ("java/io/PrintWriter", "println", "(F)V") => {
            let line = super::format::format_float(args[1].as_float()? as f64);
            println!("{line}");
            vm.output.lock().unwrap().push(line);
            Ok(None)
        }
        ("java/io/PrintWriter", "println", "(D)V") => {
            let line = super::format::format_float(args[1].as_double()?);
            println!("{line}");
            vm.output.lock().unwrap().push(line);
            Ok(None)
        }
        ("java/io/PrintWriter", "println", "(Ljava/lang/String;)V") => {
            let reference = args[1].as_reference()?;
            let line = crate::vm::builtin::helpers::stringify_reference(vm, reference)?;
            println!("{line}");
            vm.output.lock().unwrap().push(line);
            Ok(None)
        }
        ("java/io/PrintWriter", "println", "(Ljava/lang/Object;)V") => {
            let reference = args[1].as_reference()?;
            let line = if reference == Reference::Null {
                "null".to_string()
            } else {
                vm.stringify_heap(reference)?
            };
            println!("{line}");
            vm.output.lock().unwrap().push(line);
            Ok(None)
        }
        ("java/io/PrintWriter", "print", "(Z)V") => {
            let text = if args[1].as_int()? != 0 { "true" } else { "false" }.to_string();
            print!("{text}");
            Ok(None)
        }
        ("java/io/PrintWriter", "print", "(C)V") => {
            let ch = args[1].as_int()? as u8 as char;
            print!("{ch}");
            Ok(None)
        }
        ("java/io/PrintWriter", "print", "(I)V") => {
            let text = args[1].as_int()?.to_string();
            print!("{text}");
            Ok(None)
        }
        ("java/io/PrintWriter", "print", "(J)V") => {
            let text = args[1].as_long()?.to_string();
            print!("{text}");
            Ok(None)
        }
        ("java/io/PrintWriter", "print", "(F)V") => {
            let text = super::format::format_float(args[1].as_float()? as f64);
            print!("{text}");
            Ok(None)
        }
        ("java/io/PrintWriter", "print", "(D)V") => {
            let text = super::format::format_float(args[1].as_double()?);
            print!("{text}");
            Ok(None)
        }
        ("java/io/PrintWriter", "print", "(Ljava/lang/String;)V") => {
            let reference = args[1].as_reference()?;
            let text = crate::vm::builtin::helpers::stringify_reference(vm, reference)?;
            print!("{text}");
            Ok(None)
        }
        ("java/io/PrintWriter", "print", "(Ljava/lang/Object;)V") => {
            let reference = args[1].as_reference()?;
            let text = if reference == Reference::Null {
                "null".to_string()
            } else {
                vm.stringify_heap(reference)?
            };
            print!("{text}");
            Ok(None)
        }
        ("java/io/PrintWriter", "flush", "()V") => Ok(None),
        ("java/io/PrintWriter", "close", "()V") => Ok(None),
        ("java/io/PrintWriter", "append", "(C)Ljava/io/Writer;") => {
            Ok(Some(Value::Reference(args[0].as_reference()?)))
        }
        ("java/io/PrintWriter", "append", "(Ljava/lang/CharSequence;)Ljava/io/Writer;") => {
            Ok(Some(Value::Reference(args[0].as_reference()?)))
        }
        ("java/io/PrintWriter", "append", "(Ljava/lang/CharSequence;II)Ljava/io/Writer;") => {
            Ok(Some(Value::Reference(args[0].as_reference()?)))
        }
        ("java/io/PrintWriter", "<init>", "(Ljava/io/OutputStream;)V") => Ok(None),
        ("java/io/PrintWriter", "<init>", "(Ljava/io/Writer;)V") => Ok(None),
        ("java/io/PrintWriter", "<init>", "()V") => Ok(None),
        // --- Reader stubs ---
        ("java/io/Reader", "read", "()I") => Ok(Some(Value::Int(-1))),
        ("java/io/Reader", "read", "(I)I") => Ok(Some(Value::Int(-1))),
        ("java/io/Reader", "read", "([C)I") => Ok(Some(Value::Int(-1))),
        ("java/io/Reader", "read", "([CII)I") => Ok(Some(Value::Int(-1))),
        ("java/io/Reader", "skip", "(J)J") => Ok(Some(Value::Long(0))),
        ("java/io/Reader", "ready", "()Z") => Ok(Some(Value::Int(0))),
        ("java/io/Reader", "close", "()V") => Ok(None),
        ("java/io/Reader", "mark", "(I)V") => Ok(None),
        ("java/io/Reader", "reset", "()V") => Ok(None),
        ("java/io/Reader", "markSupported", "()Z") => Ok(Some(Value::Int(0))),
        // --- BufferedReader stubs ---
        ("java/io/BufferedReader", "<init>", "(Ljava/io/Reader;)V") => Ok(None),
        ("java/io/BufferedReader", "read", "()I") => Ok(Some(Value::Int(-1))),
        ("java/io/BufferedReader", "read", "(I)I") => Ok(Some(Value::Int(-1))),
        ("java/io/BufferedReader", "read", "([C)I") => Ok(Some(Value::Int(-1))),
        ("java/io/BufferedReader", "read", "([CII)I") => Ok(Some(Value::Int(-1))),
        ("java/io/BufferedReader", "skip", "(J)J") => Ok(Some(Value::Long(0))),
        ("java/io/BufferedReader", "ready", "()Z") => Ok(Some(Value::Int(0))),
        ("java/io/BufferedReader", "close", "()V") => Ok(None),
        ("java/io/BufferedReader", "readLine", "()Ljava/lang/String;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        // --- InputStreamReader stubs ---
        ("java/io/InputStreamReader", "<init>", "(Ljava/io/InputStream;)V") => Ok(None),
        ("java/io/InputStreamReader", "read", "()I") => Ok(Some(Value::Int(-1))),
        ("java/io/InputStreamReader", "read", "(I)I") => Ok(Some(Value::Int(-1))),
        ("java/io/InputStreamReader", "read", "([C)I") => Ok(Some(Value::Int(-1))),
        ("java/io/InputStreamReader", "read", "([CII)I") => Ok(Some(Value::Int(-1))),
        ("java/io/InputStreamReader", "close", "()V") => Ok(None),
        // --- OutputStreamWriter stubs ---
        ("java/io/OutputStreamWriter", "<init>", "(Ljava/io/OutputStream;)V") => Ok(None),
        ("java/io/OutputStreamWriter", "write", "(I)V") => Ok(None),
        ("java/io/OutputStreamWriter", "write", "([C)V") => Ok(None),
        ("java/io/OutputStreamWriter", "write", "([CII)V") => Ok(None),
        ("java/io/OutputStreamWriter", "write", "(Ljava/lang/String;)V") => Ok(None),
        ("java/io/OutputStreamWriter", "write", "(Ljava/lang/String;II)V") => Ok(None),
        ("java/io/OutputStreamWriter", "flush", "()V") => Ok(None),
        ("java/io/OutputStreamWriter", "close", "()V") => Ok(None),
        _ => Err(VmError::UnhandledException {
            class_name: "".to_string(),
        }),
    }
}
