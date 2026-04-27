use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

use crate::vm::{HeapValue, Reference, Value, Vm, VmError};

static COMPILED_PATTERNS: LazyLock<std::sync::RwLock<HashMap<usize, Regex>>, fn() -> std::sync::RwLock<HashMap<usize, Regex>>> =
    LazyLock::new(|| std::sync::RwLock::new(HashMap::new()));

pub(super) fn invoke_regex(
    vm: &mut Vm,
    class_name: &str,
    method_name: &str,
    descriptor: &str,
    args: &[Value],
) -> Result<Option<Value>, VmError> {
    match (class_name, method_name, descriptor) {
        ("java/util/regex/Pattern", "compile", "(Ljava/lang/String;)Ljava/util/regex/Pattern;") => {
            let pattern_ref = args[0].as_reference()?;
            let pattern_str = crate::vm::builtin::helpers::stringify_reference(vm, pattern_ref)?;
            match Regex::new(&pattern_str) {
                Ok(re) => {
                    let regex_id = {
                        let mut patterns = COMPILED_PATTERNS.write().unwrap();
                        let id = patterns.len() + 1;
                        patterns.insert(id, re);
                        id
                    };
                    let mut fields = std::collections::HashMap::new();
                    fields.insert("__regex".to_string(), Value::Reference(pattern_ref));
                    fields.insert("__flags".to_string(), Value::Int(0));
                    fields.insert("__regex_id".to_string(), Value::Int(regex_id as i32));
                    let heap = &mut vm.heap.lock().unwrap();
                    let obj_ref = heap.allocate(HeapValue::Object {
                        class_name: "java/util/regex/Pattern".to_string(),
                        fields,
                    });
                    Ok(Some(Value::Reference(obj_ref)))
                }
                Err(e) => Err(VmError::UnhandledException {
                    class_name: "java/util/PatternSyntaxException".to_string(),
                }),
            }
        }
        ("java/util/regex/Pattern", "compile", "(Ljava/lang/String;I)Ljava/util/regex/Pattern;") => {
            let pattern_ref = args[0].as_reference()?;
            let flags = args[1].as_int()?;
            let pattern_str = crate::vm::builtin::helpers::stringify_reference(vm, pattern_ref)?;
            match Regex::new(&pattern_str) {
                Ok(re) => {
                    let regex_id = {
                        let mut patterns = COMPILED_PATTERNS.write().unwrap();
                        let id = patterns.len() + 1;
                        patterns.insert(id, re);
                        id
                    };
                    let mut fields = std::collections::HashMap::new();
                    fields.insert("__regex".to_string(), Value::Reference(pattern_ref));
                    fields.insert("__flags".to_string(), Value::Int(flags));
                    fields.insert("__regex_id".to_string(), Value::Int(regex_id as i32));
                    let heap = &mut vm.heap.lock().unwrap();
                    let obj_ref = heap.allocate(HeapValue::Object {
                        class_name: "java/util/regex/Pattern".to_string(),
                        fields,
                    });
                    Ok(Some(Value::Reference(obj_ref)))
                }
                Err(_) => Err(VmError::UnhandledException {
                    class_name: "java/util/PatternSyntaxException".to_string(),
                }),
            }
        }
        ("java/util/regex/Pattern", "matches", "(Ljava/lang/String;Ljava/lang/CharSequence;)Z") => {
            let pattern_ref = args[0].as_reference()?;
            let input_ref = args[1].as_reference()?;
            let pattern_str = crate::vm::builtin::helpers::stringify_reference(vm, pattern_ref)?;
            let input_str = char_sequence_to_string(vm, input_ref)?;
            let is_match = Regex::new(&pattern_str)
                .map(|re| re.is_match(&input_str))
                .unwrap_or(false);
            Ok(Some(Value::Int(if is_match { 1 } else { 0 })))
        }
        ("java/util/regex/Pattern", "pattern", "()Ljava/lang/String;") => {
            let this_ref = args[0].as_reference()?;
            let heap = vm.heap.lock().unwrap();
            let result: Option<String> = if let Ok(HeapValue::Object { fields, .. }) = heap.get(this_ref) {
                if let Some(Value::Reference(r)) = fields.get("__regex") {
                    if let Ok(HeapValue::String(s)) = heap.get(*r) {
                        Some(s.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            drop(heap);
            match result {
                Some(s) => Ok(Some(vm.new_string(s))),
                None => Ok(Some(Value::Reference(Reference::Null))),
            }
        }
        ("java/util/regex/Pattern", "matcher", "(Ljava/lang/CharSequence;)Ljava/util/regex/Matcher;") => {
            let this_ref = args[0].as_reference()?;
            let input_ref = args[1].as_reference()?;
            let mut fields = std::collections::HashMap::new();
            fields.insert("__pattern".to_string(), Value::Reference(this_ref));
            fields.insert("__input".to_string(), Value::Reference(input_ref));
            fields.insert("__match_start".to_string(), Value::Int(-1));
            fields.insert("__match_end".to_string(), Value::Int(-1));
            fields.insert("__last_match_start".to_string(), Value::Int(-1));
            fields.insert("__group_count".to_string(), Value::Int(0));
            let heap = &mut vm.heap.lock().unwrap();
            let obj_ref = heap.allocate(HeapValue::Object {
                class_name: "java/util/regex/Matcher".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/util/regex/Pattern", "split", "(Ljava/lang/CharSequence;I)[Ljava/lang/String;") => {
            let this_ref = args[0].as_reference()?;
            let input_ref = args[1].as_reference()?;
            let limit = args[2].as_int()?;
            let pattern_str = get_pattern_regex(vm, this_ref)?;
            let input_str = char_sequence_to_string(vm, input_ref)?;
            let re = match Regex::new(&pattern_str) {
                Ok(r) => r,
                Err(_) => return Ok(Some(Value::Reference(Reference::Null))),
            };
            let parts: Vec<&str> = if limit > 0 {
                re.splitn(&input_str, limit as usize).collect()
            } else {
                re.split(&input_str).collect()
            };
            let arr_ref = create_string_array(vm, &parts)?;
            Ok(Some(Value::Reference(arr_ref)))
        }
        ("java/util/regex/Pattern", "split", "(Ljava/lang/CharSequence;)[Ljava/lang/String;") => {
            let this_ref = args[0].as_reference()?;
            let input_ref = args[1].as_reference()?;
            let pattern_str = get_pattern_regex(vm, this_ref)?;
            let input_str = char_sequence_to_string(vm, input_ref)?;
            let re = match Regex::new(&pattern_str) {
                Ok(r) => r,
                Err(_) => return Ok(Some(Value::Reference(Reference::Null))),
            };
            let parts: Vec<&str> = re.split(&input_str).collect();
            let arr_ref = create_string_array(vm, &parts)?;
            Ok(Some(Value::Reference(arr_ref)))
        }
        ("java/util/regex/Matcher", "<init>", "(Ljava/util/regex/Pattern;Ljava/lang/CharSequence;)V") => {
            let pattern_ref = args[0].as_reference()?;
            let input_ref = args[1].as_reference()?;
            let this_ref = args[2].as_reference()?;
            let heap = &mut vm.heap.lock().unwrap();
            if let Ok(HeapValue::Object { fields, .. }) = heap.get_mut(this_ref) {
                fields.insert("__pattern".to_string(), Value::Reference(pattern_ref));
                fields.insert("__input".to_string(), Value::Reference(input_ref));
            }
            Ok(None)
        }
        ("java/util/regex/Matcher", "matches", "()Z") => {
            let this_ref = args[0].as_reference()?;
            let (pattern_str, input_str) = get_matcher_pattern_and_input(vm, this_ref)?;
            let re = match Regex::new(&pattern_str) {
                Ok(r) => r,
                Err(_) => return Ok(Some(Value::Int(0))),
            };
            let is_match = re.is_match(&input_str);
            let heap = &mut vm.heap.lock().unwrap();
            if let Ok(HeapValue::Object { fields, .. }) = heap.get_mut(this_ref) {
                if is_match {
                    fields.insert("__match_start".to_string(), Value::Int(0));
                    fields.insert("__match_end".to_string(), Value::Int(input_str.len() as i32));
                } else {
                    fields.insert("__match_start".to_string(), Value::Int(-1));
                    fields.insert("__match_end".to_string(), Value::Int(-1));
                }
            }
            Ok(Some(Value::Int(if is_match { 1 } else { 0 })))
        }
        ("java/util/regex/Matcher", "find", "()Z") => {
            let this_ref = args[0].as_reference()?;
            let (pattern_str, input_str) = get_matcher_pattern_and_input(vm, this_ref)?;
            let last_end = get_last_match_end(vm, this_ref)?;
            let re = match Regex::new(&pattern_str) {
                Ok(r) => r,
                Err(_) => return Ok(Some(Value::Int(0))),
            };
            let mut found = false;
            let search_from = if last_end >= 0 { last_end as usize } else { 0 };
            if search_from <= input_str.len() {
                let remaining = &input_str[search_from..];
                if let Some(m) = re.find(remaining) {
                    found = true;
                    let start = search_from as i32 + m.start() as i32;
                    let end = search_from as i32 + m.end() as i32;
                    let heap = &mut vm.heap.lock().unwrap();
                    if let Ok(HeapValue::Object { fields, .. }) = heap.get_mut(this_ref) {
                        fields.insert("__match_start".to_string(), Value::Int(start));
                        fields.insert("__match_end".to_string(), Value::Int(end));
                        fields.insert("__last_match_start".to_string(), Value::Int(start));
                    }
                }
            }
            Ok(Some(Value::Int(if found { 1 } else { 0 })))
        }
        ("java/util/regex/Matcher", "find", "(I)Z") => {
            let start_idx = args[0].as_int()?;
            let this_ref = args[1].as_reference()?;
            let (pattern_str, input_str) = get_matcher_pattern_and_input(vm, this_ref)?;
            let re = match Regex::new(&pattern_str) {
                Ok(r) => r,
                Err(_) => return Ok(Some(Value::Int(0))),
            };
            let mut found = false;
            if start_idx >= 0 && (start_idx as usize) <= input_str.len() {
                let remaining = &input_str[start_idx as usize..];
                if let Some(m) = re.find(remaining) {
                    found = true;
                    let start = start_idx + m.start() as i32;
                    let end = start_idx + m.end() as i32;
                    let heap = &mut vm.heap.lock().unwrap();
                    if let Ok(HeapValue::Object { fields, .. }) = heap.get_mut(this_ref) {
                        fields.insert("__match_start".to_string(), Value::Int(start));
                        fields.insert("__match_end".to_string(), Value::Int(end));
                        fields.insert("__last_match_start".to_string(), Value::Int(start));
                    }
                }
            }
            Ok(Some(Value::Int(if found { 1 } else { 0 })))
        }
        ("java/util/regex/Matcher", "lookingAt", "()Z") => {
            let this_ref = args[0].as_reference()?;
            let (pattern_str, input_str) = get_matcher_pattern_and_input(vm, this_ref)?;
            let re = match Regex::new(&pattern_str) {
                Ok(r) => r,
                Err(_) => return Ok(Some(Value::Int(0))),
            };
            let is_match = re.is_match(&input_str);
            if is_match {
                let heap = &mut vm.heap.lock().unwrap();
                if let Ok(HeapValue::Object { fields, .. }) = heap.get_mut(this_ref) {
                    fields.insert("__match_start".to_string(), Value::Int(0));
                    fields.insert("__match_end".to_string(), Value::Int(re.find(&input_str).unwrap().end() as i32));
                }
            }
            Ok(Some(Value::Int(if is_match { 1 } else { 0 })))
        }
        ("java/util/regex/Matcher", "reset", "()Ljava/util/regex/Matcher;") => {
            let this_ref = args[0].as_reference()?;
            let heap = &mut vm.heap.lock().unwrap();
            if let Ok(HeapValue::Object { fields, .. }) = heap.get_mut(this_ref) {
                fields.insert("__match_start".to_string(), Value::Int(-1));
                fields.insert("__match_end".to_string(), Value::Int(-1));
                fields.insert("__last_match_start".to_string(), Value::Int(-1));
            }
            Ok(Some(Value::Reference(this_ref)))
        }
        ("java/util/regex/Matcher", "reset", "(Ljava/lang/CharSequence;)Ljava/util/regex/Matcher;") => {
            let input_ref = args[0].as_reference()?;
            let this_ref = args[1].as_reference()?;
            let heap = &mut vm.heap.lock().unwrap();
            if let Ok(HeapValue::Object { fields, .. }) = heap.get_mut(this_ref) {
                fields.insert("__input".to_string(), Value::Reference(input_ref));
                fields.insert("__match_start".to_string(), Value::Int(-1));
                fields.insert("__match_end".to_string(), Value::Int(-1));
                fields.insert("__last_match_start".to_string(), Value::Int(-1));
            }
            Ok(Some(Value::Reference(this_ref)))
        }
        ("java/util/regex/Matcher", "group", "(I)Ljava/lang/String;") => {
            let group_idx = args[0].as_int()?;
            let this_ref = args[1].as_reference()?;
            if group_idx == 0 {
                let start = get_match_start(vm, this_ref)?;
                let end = get_match_end(vm, this_ref)?;
                let input_str = get_matcher_input(vm, this_ref)?;
                if start >= 0 && end >= 0 && (end as usize) <= input_str.len() && start < end {
                    return Ok(Some(vm.new_string(input_str[start as usize..end as usize].to_string())));
                }
            }
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/regex/Matcher", "group", "()Ljava/lang/String;") => {
            let this_ref = args[0].as_reference()?;
            let start = get_match_start(vm, this_ref)?;
            let end = get_match_end(vm, this_ref)?;
            let input_str = get_matcher_input(vm, this_ref)?;
            if start >= 0 && end >= 0 && (end as usize) <= input_str.len() && start < end {
                return Ok(Some(vm.new_string(input_str[start as usize..end as usize].to_string())));
            }
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/regex/Matcher", "groupCount", "()I") => {
            let this_ref = args[0].as_reference()?;
            let heap = vm.heap.lock().unwrap();
            if let Ok(HeapValue::Object { fields, .. }) = heap.get(this_ref) {
                if let Some(Value::Int(c)) = fields.get("__group_count") {
                    return Ok(Some(Value::Int(*c)));
                }
            }
            Ok(Some(Value::Int(0)))
        }
        ("java/util/regex/Matcher", "start", "()I") => {
            let this_ref = args[0].as_reference()?;
            Ok(Some(Value::Int(get_match_start(vm, this_ref)?)))
        }
        ("java/util/regex/Matcher", "start", "(I)I") => {
            let _group_idx = args[0].as_int()?;
            let this_ref = args[1].as_reference()?;
            Ok(Some(Value::Int(get_match_start(vm, this_ref)?)))
        }
        ("java/util/regex/Matcher", "end", "()I") => {
            let this_ref = args[0].as_reference()?;
            Ok(Some(Value::Int(get_match_end(vm, this_ref)?)))
        }
        ("java/util/regex/Matcher", "end", "(I)I") => {
            let _group_idx = args[0].as_int()?;
            let this_ref = args[1].as_reference()?;
            Ok(Some(Value::Int(get_match_end(vm, this_ref)?)))
        }
        ("java/util/regex/Matcher", "replaceAll", "(Ljava/lang/String;)Ljava/lang/String;") => {
            let replacement_ref = args[0].as_reference()?;
            let this_ref = args[1].as_reference()?;
            let (pattern_str, input_str) = get_matcher_pattern_and_input(vm, this_ref)?;
            let replacement_str = crate::vm::builtin::helpers::stringify_reference(vm, replacement_ref)?;
            let re = match Regex::new(&pattern_str) {
                Ok(r) => r,
                Err(_) => return Ok(Some(Value::Reference(Reference::Null))),
            };
            let result = re.replace_all(&input_str, replacement_str.as_str());
            Ok(Some(vm.new_string(result.to_string())))
        }
        ("java/util/regex/Matcher", "replaceFirst", "(Ljava/lang/String;)Ljava/lang/String;") => {
            let replacement_ref = args[0].as_reference()?;
            let this_ref = args[1].as_reference()?;
            let (pattern_str, input_str) = get_matcher_pattern_and_input(vm, this_ref)?;
            let replacement_str = crate::vm::builtin::helpers::stringify_reference(vm, replacement_ref)?;
            let re = match Regex::new(&pattern_str) {
                Ok(r) => r,
                Err(_) => return Ok(Some(Value::Reference(Reference::Null))),
            };
            let result = re.replace(&input_str, replacement_str.as_str());
            Ok(Some(vm.new_string(result.to_string())))
        }
        _ => Err(VmError::UnhandledException {
            class_name: "".to_string(),
        }),
    }
}

fn char_sequence_to_string(vm: &Vm, cs_ref: Reference) -> Result<String, VmError> {
    if cs_ref == Reference::Null {
        return Ok(String::new());
    }
    vm.stringify_heap(cs_ref)
}

fn get_pattern_regex(vm: &Vm, pattern_ref: Reference) -> Result<String, VmError> {
    let heap = vm.heap.lock().unwrap();
    if let Ok(HeapValue::Object { fields, .. }) = heap.get(pattern_ref) {
        if let Some(Value::Reference(r)) = fields.get("__regex") {
            if let Ok(HeapValue::String(s)) = heap.get(*r) {
                return Ok(s.clone());
            }
        }
    }
    Err(VmError::UnhandledException {
        class_name: "".to_string(),
    })
}

fn get_matcher_pattern_and_input(vm: &Vm, matcher_ref: Reference) -> Result<(String, String), VmError> {
    let heap = vm.heap.lock().unwrap();
    if let Ok(HeapValue::Object { fields, .. }) = heap.get(matcher_ref) {
        let pattern_ref = fields.get("__pattern").and_then(|v| {
            if let Value::Reference(r) = v { Some(*r) } else { None }
        }).unwrap_or(Reference::Null);
        let input_ref = fields.get("__input").and_then(|v| {
            if let Value::Reference(r) = v { Some(*r) } else { None }
        }).unwrap_or(Reference::Null);
        drop(heap);
        let pattern_str = get_pattern_regex(vm, pattern_ref)?;
        let input_str = char_sequence_to_string(vm, input_ref)?;
        Ok((pattern_str, input_str))
    } else {
        Err(VmError::UnhandledException {
            class_name: "".to_string(),
        })
    }
}

fn get_matcher_input(vm: &Vm, matcher_ref: Reference) -> Result<String, VmError> {
    let heap = vm.heap.lock().unwrap();
    let input_ref = if let Ok(HeapValue::Object { fields, .. }) = heap.get(matcher_ref) {
        fields.get("__input").and_then(|v| {
            if let Value::Reference(r) = v { Some(*r) } else { None }
        })
    } else {
        None
    };
    drop(heap);
    if let Some(r) = input_ref {
        char_sequence_to_string(vm, r)
    } else {
        Ok(String::new())
    }
}

fn get_match_start(vm: &Vm, matcher_ref: Reference) -> Result<i32, VmError> {
    let heap = vm.heap.lock().unwrap();
    if let Ok(HeapValue::Object { fields, .. }) = heap.get(matcher_ref) {
        if let Some(Value::Int(start)) = fields.get("__match_start") {
            return Ok(*start);
        }
    }
    Ok(-1)
}

fn get_match_end(vm: &Vm, matcher_ref: Reference) -> Result<i32, VmError> {
    let heap = vm.heap.lock().unwrap();
    if let Ok(HeapValue::Object { fields, .. }) = heap.get(matcher_ref) {
        if let Some(Value::Int(end)) = fields.get("__match_end") {
            return Ok(*end);
        }
    }
    Ok(-1)
}

fn get_last_match_end(vm: &Vm, matcher_ref: Reference) -> Result<i32, VmError> {
    let heap = vm.heap.lock().unwrap();
    if let Ok(HeapValue::Object { fields, .. }) = heap.get(matcher_ref) {
        if let Some(Value::Int(end)) = fields.get("__match_end") {
            return Ok(*end);
        }
    }
    Ok(-1)
}

fn create_string_array(vm: &mut Vm, parts: &[&str]) -> Result<Reference, VmError> {
    let arr_ref = {
        let heap = &mut vm.heap.lock().unwrap();
        heap.allocate_reference_array("Ljava/lang/String;", vec![Reference::Null; parts.len()])
    };
    for (i, part) in parts.iter().enumerate() {
        let str_ref = vm.new_string(part.to_string());
        let str_ref_value = match str_ref {
            Value::Reference(r) => r,
            _ => Reference::Null,
        };
        set_array_element(vm, arr_ref, i as i32, str_ref_value)?;
    }
    Ok(arr_ref)
}

fn set_array_element(vm: &Vm, arr_ref: Reference, index: i32, value: Reference) -> Result<(), VmError> {
    let mut heap = vm.heap.lock().unwrap();
    if let Ok(HeapValue::ReferenceArray { values, .. }) = heap.get_mut(arr_ref) {
        let idx = index as usize;
        if idx < values.len() {
            values[idx] = value;
        }
    }
    Ok(())
}

