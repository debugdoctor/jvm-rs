use crate::vm::{HeapValue, Reference, Value, Vm, VmError};

pub(super) fn invoke_text(
    vm: &mut Vm,
    class_name: &str,
    method_name: &str,
    descriptor: &str,
    args: &[Value],
) -> Result<Option<Value>, VmError> {
    match (class_name, method_name, descriptor) {
        ("java/text/NumberFormat", "format", "(I)Ljava/lang/String;") => {
            let val = args[0].as_int()?;
            Ok(Some(vm.new_string(val.to_string())))
        }
        ("java/text/NumberFormat", "format", "(J)Ljava/lang/String;") => {
            let val = args[0].as_long()?;
            Ok(Some(vm.new_string(val.to_string())))
        }
        ("java/text/NumberFormat", "format", "(F)Ljava/lang/String;") => {
            let val = args[0].as_float()?;
            Ok(Some(vm.new_string(crate::vm::builtin::format::format_float(val as f64))))
        }
        ("java/text/NumberFormat", "format", "(D)Ljava/lang/String;") => {
            let val = args[0].as_double()?;
            Ok(Some(vm.new_string(crate::vm::builtin::format::format_float(val))))
        }
        ("java/text/NumberFormat", "format", "(Ljava/lang/Object;)Ljava/lang/String;") => {
            let r = args[0].as_reference()?;
            let s = vm.stringify_heap(r)?;
            Ok(Some(vm.new_string(s)))
        }
        ("java/text/DecimalFormat", "<init>", "(Ljava/lang/String;)V") => {
            let pattern_ref = args[0].as_reference()?;
            let this_ref = args.last().unwrap().as_reference()?;
            let pattern = crate::vm::builtin::helpers::stringify_reference(vm, pattern_ref)?;
            let heap = &mut vm.heap.lock().unwrap();
            if let Ok(HeapValue::Object { fields, .. }) = heap.get_mut(this_ref) {
                fields.insert("__pattern".to_string(), Value::Reference(pattern_ref));
            }
            Ok(None)
        }
        ("java/text/DecimalFormat", "applyPattern", "(Ljava/lang/String;)V") => {
            let pattern_ref = args[0].as_reference()?;
            let this_ref = args[1].as_reference()?;
            let heap = &mut vm.heap.lock().unwrap();
            if let Ok(HeapValue::Object { fields, .. }) = heap.get_mut(this_ref) {
                fields.insert("__pattern".to_string(), Value::Reference(pattern_ref));
            }
            Ok(None)
        }
        ("java/text/DecimalFormat", "format", "(D)Ljava/lang/String;") => {
            let val = args[0].as_double()?;
            let this_ref = args[1].as_reference()?;
            let pattern = {
                let heap = vm.heap.lock().unwrap();
                if let Ok(HeapValue::Object { fields, .. }) = heap.get(this_ref) {
                    fields.get("__pattern").and_then(|v| {
                        if let Value::Reference(r) = v {
                            match heap.get(*r) {
                                Ok(HeapValue::String(s)) => Some(s.clone()),
                                _ => None,
                            }
                        } else {
                            None
                        }
                    })
                } else {
                    None
                }
            };
            let formatted = if let Some(p) = pattern {
                simple_decimal_format(val, &p)
            } else {
                crate::vm::builtin::format::format_float(val)
            };
            Ok(Some(vm.new_string(formatted)))
        }
        ("java/text/DecimalFormat", "format", "(I)Ljava/lang/String;") => {
            let val = args[0].as_int()?;
            Ok(Some(vm.new_string(val.to_string())))
        }
        ("java/text/DecimalFormat", "format", "(J)Ljava/lang/String;") => {
            let val = args[0].as_long()?;
            Ok(Some(vm.new_string(val.to_string())))
        }
        ("java/text/DecimalFormat", "setMaximumFractionDigits", "(I)V") => Ok(None),
        ("java/text/DecimalFormat", "setMinimumFractionDigits", "(I)V") => Ok(None),
        ("java/text/DecimalFormat", "setMaximumIntegerDigits", "(I)V") => Ok(None),
        ("java/text/DecimalFormat", "setMinimumIntegerDigits", "(I)V") => Ok(None),
        ("java/text/MessageFormat", "<init>", "(Ljava/lang/String;)V") => {
            let pattern_ref = args[0].as_reference()?;
            let this_ref = args[1].as_reference()?;
            let heap = &mut vm.heap.lock().unwrap();
            if let Ok(HeapValue::Object { fields, .. }) = heap.get_mut(this_ref) {
                fields.insert("__pattern".to_string(), Value::Reference(pattern_ref));
            }
            Ok(None)
        }
        ("java/text/MessageFormat", "format", "(Ljava/lang/String;[Ljava/lang/Object;)Ljava/lang/String;") => {
            let pattern_ref = args[0].as_reference()?;
            let args_ref = args[1].as_reference()?;
            let pattern = crate::vm::builtin::helpers::stringify_reference(vm, pattern_ref)?;
            let arg_count = get_array_length(vm, args_ref)?;
            let mut arg_strings = Vec::new();
            for i in 0..arg_count {
                let elem_ref = get_array_element(vm, args_ref, i)?;
                let s = if elem_ref != Reference::Null {
                    vm.stringify_heap(elem_ref)?
                } else {
                    "null".to_string()
                };
                arg_strings.push(s);
            }
            let formatted = simple_message_format(&pattern, &arg_strings);
            Ok(Some(vm.new_string(formatted)))
        }
        ("java/text/MessageFormat", "format", "(Ljava/lang/Object;)Ljava/lang/String;") => {
            let r = args[0].as_reference()?;
            let s = vm.stringify_heap(r)?;
            Ok(Some(vm.new_string(s)))
        }
        _ => Err(VmError::UnhandledException {
            class_name: "".to_string(),
        }),
    }
}

fn get_array_length(vm: &Vm, arr_ref: Reference) -> Result<i32, VmError> {
    match vm.heap.lock().unwrap().get(arr_ref)? {
        HeapValue::ReferenceArray { values, .. } => Ok(values.len() as i32),
        HeapValue::IntArray { values, .. } => Ok(values.len() as i32),
        HeapValue::LongArray { values, .. } => Ok(values.len() as i32),
        HeapValue::DoubleArray { values, .. } => Ok(values.len() as i32),
        HeapValue::FloatArray { values, .. } => Ok(values.len() as i32),
        _ => Ok(0),
    }
}

fn get_array_element(vm: &Vm, arr_ref: Reference, index: i32) -> Result<Reference, VmError> {
    let heap = vm.heap.lock().unwrap();
    match heap.get(arr_ref)? {
        HeapValue::ReferenceArray { values, .. } => {
            let idx = index as usize;
            if idx < values.len() {
                Ok(values[idx])
            } else {
                Ok(Reference::Null)
            }
        }
        _ => Ok(Reference::Null),
    }
}

fn simple_decimal_format(val: f64, pattern: &str) -> String {
    let has_decimal = pattern.contains('.');
    let parts: Vec<&str> = pattern.split('.').collect();
    let int_part = parts.first().unwrap_or(&"#,##0");
    let frac_part = parts.get(1).unwrap_or(&"#.##");

    let int_digits = int_part.matches('#').count().max(1);
    let frac_digits = frac_part.matches('#').count().max(2);

    let abs_val = val.abs();
    let int_val = abs_val.trunc() as u64;
    let frac_val = ((abs_val - int_val as f64) * 10_f64.powi(frac_digits as i32)).round() as u64;

    let sign = if val < 0.0 { "-" } else { "" };

    let int_str = if int_val == 0 {
        "0".to_string()
    } else {
        let mut s = format!("{}", int_val);
        if s.len() > int_digits {
            s
        } else {
            let zeros_needed = int_digits.saturating_sub(s.len());
            format!("{}{}", "0".repeat(zeros_needed), s)
        }
    };

    let frac_str = if frac_digits > 0 {
        let f = format!("{:0>width$}", frac_val, width = frac_digits);
        if has_decimal {
            format!(".{}", f.trim_end_matches('0'))
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    format!("{}{}{}", sign, int_str, frac_str)
}

fn simple_message_format(pattern: &str, args: &[String]) -> String {
    let mut result = String::new();
    let mut chars = pattern.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' {
            let mut idx_str = String::new();
            while let Some(&next) = chars.peek() {
                if next.is_digit(10) {
                    idx_str.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            if let Some(&'{') = chars.peek() {
                chars.next();
                result.push('{');
                result.push_str(&idx_str);
            } else if chars.next() == Some('}') {
                if let Ok(idx) = idx_str.parse::<usize>() {
                    if idx < args.len() {
                        result.push_str(&args[idx]);
                    }
                }
            } else {
                result.push('{');
                result.push_str(&idx_str);
            }
        } else {
            result.push(c);
        }
    }

    result
}