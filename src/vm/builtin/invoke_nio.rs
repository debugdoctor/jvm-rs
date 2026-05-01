use crate::vm::{HeapValue, Reference, Value, Vm, VmError};

pub(super) fn invoke_nio(
    vm: &mut Vm,
    class_name: &str,
    method_name: &str,
    descriptor: &str,
    args: &[Value],
) -> Result<Option<Value>, VmError> {
    match (class_name, method_name, descriptor) {
        // --- Buffer stubs ---
        ("java/nio/Buffer", "capacity", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let cap = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => fields
                        .get("__capacity")
                        .and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        })
                        .unwrap_or(0),
                    _ => 0,
                }
            };
            Ok(Some(Value::Int(cap)))
        }
        ("java/nio/Buffer", "position", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let pos = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => fields
                        .get("__position")
                        .and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        })
                        .unwrap_or(0),
                    _ => 0,
                }
            };
            Ok(Some(Value::Int(pos)))
        }
        ("java/nio/Buffer", "position", "(I)Ljava/nio/Buffer;") => {
            let obj_ref = args[0].as_reference()?;
            let new_pos = args[1].as_int()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields.insert("__position".to_string(), Value::Int(new_pos));
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/Buffer", "limit", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let lim = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => fields
                        .get("__limit")
                        .and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        })
                        .unwrap_or(0),
                    _ => 0,
                }
            };
            Ok(Some(Value::Int(lim)))
        }
        ("java/nio/Buffer", "limit", "(I)Ljava/nio/Buffer;") => {
            let obj_ref = args[0].as_reference()?;
            let new_limit = args[1].as_int()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields.insert("__limit".to_string(), Value::Int(new_limit));
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/Buffer", "mark", "()Ljava/nio/Buffer;") => {
            Ok(Some(Value::Reference(args[0].as_reference()?)))
        }
        ("java/nio/Buffer", "reset", "()Ljava/nio/Buffer;") => {
            Ok(Some(Value::Reference(args[0].as_reference()?)))
        }
        ("java/nio/Buffer", "clear", "()Ljava/nio/Buffer;") => {
            let obj_ref = args[0].as_reference()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    let cap = fields.get("__capacity").copied().unwrap_or(Value::Int(0));
                    fields.insert("__position".to_string(), Value::Int(0));
                    fields.insert("__limit".to_string(), cap);
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/Buffer", "flip", "()Ljava/nio/Buffer;") => {
            let obj_ref = args[0].as_reference()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    let lim = fields.get("__position").copied().unwrap_or(Value::Int(0));
                    fields.insert("__limit".to_string(), lim);
                    fields.insert("__position".to_string(), Value::Int(0));
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/Buffer", "rewind", "()Ljava/nio/Buffer;") => {
            let obj_ref = args[0].as_reference()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields.insert("__position".to_string(), Value::Int(0));
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/Buffer", "remaining", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let (pos, lim) = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let pos = fields
                            .get("__position")
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        let lim = fields
                            .get("__limit")
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        (pos, lim)
                    }
                    _ => (0, 0),
                }
            };
            Ok(Some(Value::Int((lim - pos).max(0))))
        }
        ("java/nio/Buffer", "hasRemaining", "()Z") => {
            let obj_ref = args[0].as_reference()?;
            let has_more = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let pos = fields
                            .get("__position")
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        let lim = fields
                            .get("__limit")
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        pos < lim
                    }
                    _ => false,
                }
            };
            Ok(Some(Value::Int(if has_more { 1 } else { 0 })))
        }
        // --- ByteBuffer stubs ---
        ("java/nio/ByteBuffer", "allocate", "(I)Ljava/nio/ByteBuffer;") => {
            let capacity = args[0].as_int()? as usize;
            let backing = vm.heap.lock().unwrap().allocate(HeapValue::IntArray {
                values: vec![0; capacity],
            });
            let buf_ref = vm.heap.lock().unwrap().allocate(HeapValue::Object {
                class_name: "java/nio/ByteBuffer".to_string(),
                fields: std::collections::HashMap::new(),
            });
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(buf_ref)? {
                    fields.insert("__backing".to_string(), Value::Reference(backing));
                    fields.insert("__offset".to_string(), Value::Int(0));
                    fields.insert("__capacity".to_string(), Value::Int(capacity as i32));
                    fields.insert("__position".to_string(), Value::Int(0));
                    fields.insert("__limit".to_string(), Value::Int(capacity as i32));
                }
            }
            Ok(Some(Value::Reference(buf_ref)))
        }
        ("java/nio/ByteBuffer", "wrap", "([B)Ljava/nio/ByteBuffer;")
        | ("java/nio/ByteBuffer", "wrap", "([BII)Ljava/nio/ByteBuffer;") => {
            let array_ref = args[0].as_reference()?;
            let offset = if args.len() > 2 { args[1].as_int()? } else { 0 };
            let length = if args.len() > 3 {
                args[2].as_int()?
            } else {
                let heap = vm.heap.lock().unwrap();
                match heap.get(array_ref)? {
                    HeapValue::IntArray { values } => values.len() as i32,
                    _ => 0,
                }
            };
            let buf_ref = vm.heap.lock().unwrap().allocate(HeapValue::Object {
                class_name: "java/nio/ByteBuffer".to_string(),
                fields: std::collections::HashMap::new(),
            });
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(buf_ref)? {
                    fields.insert("__backing".to_string(), Value::Reference(array_ref));
                    fields.insert("__offset".to_string(), Value::Int(offset));
                    fields.insert("__capacity".to_string(), Value::Int(length));
                    fields.insert("__position".to_string(), Value::Int(0));
                    fields.insert("__limit".to_string(), Value::Int(length));
                }
            }
            Ok(Some(Value::Reference(buf_ref)))
        }
        ("java/nio/ByteBuffer", "capacity", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let cap = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => fields
                        .get("__capacity")
                        .and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        })
                        .unwrap_or(0),
                    _ => 0,
                }
            };
            Ok(Some(Value::Int(cap)))
        }
        ("java/nio/ByteBuffer", "position", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let pos = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => fields
                        .get("__position")
                        .and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        })
                        .unwrap_or(0),
                    _ => 0,
                }
            };
            Ok(Some(Value::Int(pos)))
        }
        ("java/nio/ByteBuffer", "position", "(I)Ljava/nio/Buffer;") => {
            let obj_ref = args[0].as_reference()?;
            let new_pos = args[1].as_int()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields.insert("__position".to_string(), Value::Int(new_pos));
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/ByteBuffer", "limit", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let lim = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => fields
                        .get("__limit")
                        .and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        })
                        .unwrap_or(0),
                    _ => 0,
                }
            };
            Ok(Some(Value::Int(lim)))
        }
        ("java/nio/ByteBuffer", "limit", "(I)Ljava/nio/Buffer;") => {
            let obj_ref = args[0].as_reference()?;
            let new_limit = args[1].as_int()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields.insert("__limit".to_string(), Value::Int(new_limit));
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/ByteBuffer", "mark", "()Ljava/nio/Buffer;") => {
            Ok(Some(Value::Reference(args[0].as_reference()?)))
        }
        ("java/nio/ByteBuffer", "reset", "()Ljava/nio/Buffer;") => {
            Ok(Some(Value::Reference(args[0].as_reference()?)))
        }
        ("java/nio/ByteBuffer", "clear", "()Ljava/nio/Buffer;") => {
            let obj_ref = args[0].as_reference()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    let cap = fields.get("__capacity").copied().unwrap_or(Value::Int(0));
                    fields.insert("__position".to_string(), Value::Int(0));
                    fields.insert("__limit".to_string(), cap);
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/ByteBuffer", "flip", "()Ljava/nio/Buffer;") => {
            let obj_ref = args[0].as_reference()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    let lim = fields.get("__position").copied().unwrap_or(Value::Int(0));
                    fields.insert("__limit".to_string(), lim);
                    fields.insert("__position".to_string(), Value::Int(0));
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/ByteBuffer", "rewind", "()Ljava/nio/Buffer;") => {
            let obj_ref = args[0].as_reference()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields.insert("__position".to_string(), Value::Int(0));
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/ByteBuffer", "remaining", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let (pos, lim) = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let pos = fields
                            .get("__position")
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        let lim = fields
                            .get("__limit")
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        (pos, lim)
                    }
                    _ => (0, 0),
                }
            };
            Ok(Some(Value::Int((lim - pos).max(0))))
        }
        ("java/nio/ByteBuffer", "hasRemaining", "()Z") => {
            let obj_ref = args[0].as_reference()?;
            let has_more = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let pos = fields
                            .get("__position")
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        let lim = fields
                            .get("__limit")
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        pos < lim
                    }
                    _ => false,
                }
            };
            Ok(Some(Value::Int(if has_more { 1 } else { 0 })))
        }
        ("java/nio/ByteBuffer", "get", "()B") => {
            let obj_ref = args[0].as_reference()?;
            let (backing, offset, pos) = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let backing = fields.get("__backing").and_then(|v| match v {
                            Value::Reference(r) => Some(*r),
                            _ => None,
                        });
                        let offset = fields
                            .get("__offset")
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        let pos = fields
                            .get("__position")
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        (backing, offset, pos)
                    }
                    _ => (None, 0, 0),
                }
            };
            let byte_val = if let Some(backing) = backing {
                let heap = vm.heap.lock().unwrap();
                match heap.get(backing)? {
                    HeapValue::IntArray { values } => {
                        let idx = (offset + pos) as usize;
                        values.get(idx).copied().unwrap_or(0) as i8 as i32
                    }
                    _ => 0,
                }
            } else {
                0
            };
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    let pos = fields
                        .get("__position")
                        .and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        })
                        .unwrap_or(0);
                    fields.insert("__position".to_string(), Value::Int(pos + 1));
                }
            }
            Ok(Some(Value::Int(byte_val)))
        }
        ("java/nio/ByteBuffer", "get", "(I)B") => {
            let obj_ref = args[0].as_reference()?;
            let index = args[1].as_int()?;
            let byte_val = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let backing = fields.get("__backing").and_then(|v| match v {
                            Value::Reference(r) => Some(*r),
                            _ => None,
                        });
                        let offset = fields
                            .get("__offset")
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        if let Some(backing) = backing {
                            if let HeapValue::IntArray { values } = heap.get(backing)? {
                                let idx = (offset + index) as usize;
                                values.get(idx).copied().unwrap_or(0) as i8 as i32
                            } else {
                                0
                            }
                        } else {
                            0
                        }
                    }
                    _ => 0,
                }
            };
            Ok(Some(Value::Int(byte_val)))
        }
        ("java/nio/ByteBuffer", "put", "(B)Ljava/nio/ByteBuffer;") => {
            let obj_ref = args[0].as_reference()?;
            let byte_val = args[1].as_int()?;
            let (backing, offset, pos) = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let backing = fields.get("__backing").and_then(|v| match v {
                            Value::Reference(r) => Some(*r),
                            _ => None,
                        });
                        let offset = fields
                            .get("__offset")
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        let pos = fields
                            .get("__position")
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        (backing, offset, pos)
                    }
                    _ => (None, 0, 0),
                }
            };
            if let Some(backing) = backing {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::IntArray { values } = heap.get_mut(backing)? {
                    let idx = (offset + pos) as usize;
                    if idx < values.len() {
                        values[idx] = byte_val as i32;
                    }
                }
            }
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields.insert("__position".to_string(), Value::Int(pos + 1));
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/ByteBuffer", "put", "(IB)Ljava/nio/ByteBuffer;") => {
            let obj_ref = args[0].as_reference()?;
            let index = args[1].as_int()?;
            let byte_val = args[2].as_int()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    let backing = fields.get("__backing").and_then(|v| match v {
                        Value::Reference(r) => Some(*r),
                        _ => None,
                    });
                    let offset = fields
                        .get("__offset")
                        .and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        })
                        .unwrap_or(0);
                    if let Some(backing) = backing {
                        if let HeapValue::IntArray { values } = heap.get_mut(backing)? {
                            let idx = (offset + index) as usize;
                            if idx < values.len() {
                                values[idx] = byte_val as i32;
                            }
                        }
                    }
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/ByteBuffer", "array", "()[B") => {
            let obj_ref = args[0].as_reference()?;
            let backing = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        fields.get("__backing").and_then(|v| match v {
                            Value::Reference(r) => Some(*r),
                            _ => None,
                        })
                    }
                    _ => None,
                }
            };
            Ok(Some(
                backing
                    .map(Value::Reference)
                    .unwrap_or(Value::Reference(Reference::Null)),
            ))
        }
        ("java/nio/ByteBuffer", "isDirect", "()Z") => Ok(Some(Value::Int(0))),
        // --- CharBuffer stubs ---
        ("java/nio/CharBuffer", "allocate", "(I)Ljava/nio/CharBuffer;") => {
            let capacity = args[0].as_int()? as usize;
            let backing = vm.heap.lock().unwrap().allocate(HeapValue::IntArray {
                values: vec![0; capacity],
            });
            let buf_ref = vm.heap.lock().unwrap().allocate(HeapValue::Object {
                class_name: "java/nio/CharBuffer".to_string(),
                fields: std::collections::HashMap::new(),
            });
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(buf_ref)? {
                    fields.insert("__backing".to_string(), Value::Reference(backing));
                    fields.insert("__offset".to_string(), Value::Int(0));
                    fields.insert("__capacity".to_string(), Value::Int(capacity as i32));
                    fields.insert("__position".to_string(), Value::Int(0));
                    fields.insert("__limit".to_string(), Value::Int(capacity as i32));
                }
            }
            Ok(Some(Value::Reference(buf_ref)))
        }
        ("java/nio/CharBuffer", "wrap", "([C)Ljava/nio/CharBuffer;")
        | ("java/nio/CharBuffer", "wrap", "([CII)Ljava/nio/CharBuffer;") => {
            let array_ref = args[0].as_reference()?;
            let offset = if args.len() > 2 { args[1].as_int()? } else { 0 };
            let length = if args.len() > 3 {
                args[2].as_int()?
            } else {
                let heap = vm.heap.lock().unwrap();
                match heap.get(array_ref)? {
                    HeapValue::IntArray { values } => values.len() as i32,
                    _ => 0,
                }
            };
            let buf_ref = vm.heap.lock().unwrap().allocate(HeapValue::Object {
                class_name: "java/nio/CharBuffer".to_string(),
                fields: std::collections::HashMap::new(),
            });
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(buf_ref)? {
                    fields.insert("__backing".to_string(), Value::Reference(array_ref));
                    fields.insert("__offset".to_string(), Value::Int(offset));
                    fields.insert("__capacity".to_string(), Value::Int(length));
                    fields.insert("__position".to_string(), Value::Int(0));
                    fields.insert("__limit".to_string(), Value::Int(length));
                }
            }
            Ok(Some(Value::Reference(buf_ref)))
        }
        ("java/nio/CharBuffer", "capacity", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let cap = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => fields
                        .get("__capacity")
                        .and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        })
                        .unwrap_or(0),
                    _ => 0,
                }
            };
            Ok(Some(Value::Int(cap)))
        }
        ("java/nio/CharBuffer", "position", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let pos = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => fields
                        .get("__position")
                        .and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        })
                        .unwrap_or(0),
                    _ => 0,
                }
            };
            Ok(Some(Value::Int(pos)))
        }
        ("java/nio/CharBuffer", "position", "(I)Ljava/nio/Buffer;") => {
            let obj_ref = args[0].as_reference()?;
            let new_pos = args[1].as_int()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields.insert("__position".to_string(), Value::Int(new_pos));
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/CharBuffer", "limit", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let lim = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => fields
                        .get("__limit")
                        .and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        })
                        .unwrap_or(0),
                    _ => 0,
                }
            };
            Ok(Some(Value::Int(lim)))
        }
        ("java/nio/CharBuffer", "limit", "(I)Ljava/nio/Buffer;") => {
            let obj_ref = args[0].as_reference()?;
            let new_limit = args[1].as_int()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields.insert("__limit".to_string(), Value::Int(new_limit));
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/CharBuffer", "mark", "()Ljava/nio/Buffer;") => {
            Ok(Some(Value::Reference(args[0].as_reference()?)))
        }
        ("java/nio/CharBuffer", "reset", "()Ljava/nio/Buffer;") => {
            Ok(Some(Value::Reference(args[0].as_reference()?)))
        }
        ("java/nio/CharBuffer", "clear", "()Ljava/nio/Buffer;") => {
            let obj_ref = args[0].as_reference()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    let cap = fields.get("__capacity").copied().unwrap_or(Value::Int(0));
                    fields.insert("__position".to_string(), Value::Int(0));
                    fields.insert("__limit".to_string(), cap);
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/CharBuffer", "flip", "()Ljava/nio/Buffer;") => {
            let obj_ref = args[0].as_reference()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    let lim = fields.get("__position").copied().unwrap_or(Value::Int(0));
                    fields.insert("__limit".to_string(), lim);
                    fields.insert("__position".to_string(), Value::Int(0));
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/CharBuffer", "rewind", "()Ljava/nio/Buffer;") => {
            let obj_ref = args[0].as_reference()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields.insert("__position".to_string(), Value::Int(0));
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/CharBuffer", "remaining", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let (pos, lim) = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let pos = fields
                            .get("__position")
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        let lim = fields
                            .get("__limit")
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        (pos, lim)
                    }
                    _ => (0, 0),
                }
            };
            Ok(Some(Value::Int((lim - pos).max(0))))
        }
        ("java/nio/CharBuffer", "hasRemaining", "()Z") => {
            let obj_ref = args[0].as_reference()?;
            let has_more = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let pos = fields
                            .get("__position")
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        let lim = fields
                            .get("__limit")
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        pos < lim
                    }
                    _ => false,
                }
            };
            Ok(Some(Value::Int(if has_more { 1 } else { 0 })))
        }
        ("java/nio/CharBuffer", "get", "()C") => {
            let obj_ref = args[0].as_reference()?;
            let (backing, offset, pos) = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let backing = fields.get("__backing").and_then(|v| match v {
                            Value::Reference(r) => Some(*r),
                            _ => None,
                        });
                        let offset = fields
                            .get("__offset")
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        let pos = fields
                            .get("__position")
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        (backing, offset, pos)
                    }
                    _ => (None, 0, 0),
                }
            };
            let char_val = if let Some(backing) = backing {
                let heap = vm.heap.lock().unwrap();
                match heap.get(backing)? {
                    HeapValue::IntArray { values } => {
                        let idx = (offset + pos) as usize;
                        values.get(idx).copied().unwrap_or(0) as u8 as char as i32
                    }
                    _ => 0,
                }
            } else {
                0
            };
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    let pos = fields
                        .get("__position")
                        .and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        })
                        .unwrap_or(0);
                    fields.insert("__position".to_string(), Value::Int(pos + 1));
                }
            }
            Ok(Some(Value::Int(char_val)))
        }
        ("java/nio/CharBuffer", "get", "(I)C") => {
            let obj_ref = args[0].as_reference()?;
            let index = args[1].as_int()?;
            let char_val = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let backing = fields.get("__backing").and_then(|v| match v {
                            Value::Reference(r) => Some(*r),
                            _ => None,
                        });
                        let offset = fields
                            .get("__offset")
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        if let Some(backing) = backing {
                            if let HeapValue::IntArray { values } = heap.get(backing)? {
                                let idx = (offset + index) as usize;
                                values.get(idx).copied().unwrap_or(0) as u8 as char as i32
                            } else {
                                0
                            }
                        } else {
                            0
                        }
                    }
                    _ => 0,
                }
            };
            Ok(Some(Value::Int(char_val)))
        }
        ("java/nio/CharBuffer", "put", "(C)Ljava/nio/CharBuffer;") => {
            let obj_ref = args[0].as_reference()?;
            let char_val = args[1].as_int()?;
            let (backing, offset, pos) = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let backing = fields.get("__backing").and_then(|v| match v {
                            Value::Reference(r) => Some(*r),
                            _ => None,
                        });
                        let offset = fields
                            .get("__offset")
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        let pos = fields
                            .get("__position")
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        (backing, offset, pos)
                    }
                    _ => (None, 0, 0),
                }
            };
            if let Some(backing) = backing {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::IntArray { values } = heap.get_mut(backing)? {
                    let idx = (offset + pos) as usize;
                    if idx < values.len() {
                        values[idx] = char_val;
                    }
                }
            }
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields.insert("__position".to_string(), Value::Int(pos + 1));
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/CharBuffer", "put", "(IC)Ljava/nio/CharBuffer;") => {
            let obj_ref = args[0].as_reference()?;
            let index = args[1].as_int()?;
            let char_val = args[2].as_int()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    let backing = fields.get("__backing").and_then(|v| match v {
                        Value::Reference(r) => Some(*r),
                        _ => None,
                    });
                    let offset = fields
                        .get("__offset")
                        .and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        })
                        .unwrap_or(0);
                    if let Some(backing) = backing {
                        if let HeapValue::IntArray { values } = heap.get_mut(backing)? {
                            let idx = (offset + index) as usize;
                            if idx < values.len() {
                                values[idx] = char_val;
                            }
                        }
                    }
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/CharBuffer", "array", "()[C") => {
            let obj_ref = args[0].as_reference()?;
            let backing = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        fields.get("__backing").and_then(|v| match v {
                            Value::Reference(r) => Some(*r),
                            _ => None,
                        })
                    }
                    _ => None,
                }
            };
            Ok(Some(
                backing
                    .map(Value::Reference)
                    .unwrap_or(Value::Reference(Reference::Null)),
            ))
        }
        ("java/nio/CharBuffer", "length", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let len = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => fields
                        .get("__capacity")
                        .and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        })
                        .unwrap_or(0),
                    _ => 0,
                }
            };
            Ok(Some(Value::Int(len)))
        }
        // --- Path stubs ---
        ("java/nio/file/Path", "getFileName", "()Ljava/lang/String;") => {
            let obj_ref = args[0].as_reference()?;
            let name = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let path_str = fields.get("__path").and_then(|v| match v {
                            Value::Reference(r) => Some(*r),
                            _ => None,
                        });
                        if let Some(path_str) = path_str {
                            if let HeapValue::String(s) = heap.get(path_str)? {
                                Some(s.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            };
            let name = name
                .and_then(|s| s.rsplit('/').next().map(|s| s.to_string()))
                .unwrap_or_default();
            Ok(Some(vm.new_string(name)))
        }
        ("java/nio/file/Path", "getParent", "()Ljava/nio/file/Path;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/nio/file/Path", "getRoot", "()Ljava/nio/file/Path;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/nio/file/Path", "isAbsolute", "()Z") => Ok(Some(Value::Int(0))),
        ("java/nio/file/Path", "getNameCount", "()I") => Ok(Some(Value::Int(0))),
        ("java/nio/file/Path", "getName", "(I)Ljava/lang/String;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/nio/file/Path", "subpath", "(II)Ljava/nio/file/Path;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/nio/file/Path", "toString", "()Ljava/lang/String;") => {
            let obj_ref = args[0].as_reference()?;
            let path_str = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let path_ref = fields.get("__path").and_then(|v| match v {
                            Value::Reference(r) => Some(*r),
                            _ => None,
                        });
                        if let Some(path_ref) = path_ref {
                            if let HeapValue::String(s) = heap.get(path_ref)? {
                                Some(s.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            };
            Ok(match path_str {
                Some(s) => Some(vm.new_string(s)),
                None => Some(Value::Reference(Reference::Null)),
            })
        }
        ("java/nio/file/Path", "toUri", "()Ljava/net/URI;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/nio/file/Path", "toAbsolutePath", "()Ljava/nio/file/Path;") => {
            Ok(Some(Value::Reference(args[0].as_reference()?)))
        }
        ("java/nio/file/Path", "normalize", "()Ljava/nio/file/Path;") => {
            Ok(Some(Value::Reference(args[0].as_reference()?)))
        }
        ("java/nio/file/Path", "resolve", "(Ljava/lang/String;)Ljava/nio/file/Path;") => {
            Ok(Some(Value::Reference(args[0].as_reference()?)))
        }
        ("java/nio/file/Path", "startsWith", "(Ljava/lang/String;)Z") => Ok(Some(Value::Int(0))),
        ("java/nio/file/Path", "endsWith", "(Ljava/lang/String;)Z") => Ok(Some(Value::Int(0))),
        // --- Paths stubs ---
        (
            "java/nio/file/Paths",
            "get",
            "(Ljava/lang/String;[Ljava/lang/String;)Ljava/nio/file/Path;",
        ) => {
            let path_str = args[0].as_reference()?;
            let path_ref = vm.heap.lock().unwrap().allocate(HeapValue::Object {
                class_name: "java/nio/file/Path".to_string(),
                fields: std::collections::HashMap::new(),
            });
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(path_ref)? {
                    fields.insert("__path".to_string(), Value::Reference(path_str));
                }
            }
            Ok(Some(Value::Reference(path_ref)))
        }
        // --- Files stubs ---
        (
            "java/nio/file/Files",
            "exists",
            "(Ljava/nio/file/Path;[Ljava/nio/file/attribute/FileAttribute;)Z",
        ) => Ok(Some(Value::Int(0))),
        ("java/nio/file/Files", "isRegularFile", "(Ljava/nio/file/Path;)Z") => {
            Ok(Some(Value::Int(0)))
        }
        ("java/nio/file/Files", "isDirectory", "(Ljava/nio/file/Path;)Z") => {
            Ok(Some(Value::Int(0)))
        }
        (
            "java/nio/file/Files",
            "createFile",
            "(Ljava/nio/file/Path;[Ljava/nio/file/attribute/FileAttribute;)Ljava/nio/file/Path;",
        ) => Ok(Some(Value::Reference(args[0].as_reference()?))),
        ("java/nio/file/Files", "delete", "(Ljava/nio/file/Path;)V") => Ok(None),
        (
            "java/nio/file/Files",
            "copy",
            "(Ljava/nio/file/Path;Ljava/nio/file/Path;[Ljava/nio/file/CopyOption;)Ljava/nio/file/Path;",
        ) => Ok(Some(Value::Reference(Reference::Null))),
        (
            "java/nio/file/Files",
            "move",
            "(Ljava/nio/file/Path;Ljava/nio/file/Path;[Ljava/nio/file/CopyOption;)Ljava/nio/file/Path;",
        ) => Ok(Some(Value::Reference(Reference::Null))),
        ("java/nio/file/Files", "readString", "(Ljava/nio/file/Path;)Ljava/lang/String;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        (
            "java/nio/file/Files",
            "writeString",
            "(Ljava/nio/file/Path;Ljava/lang/CharSequence;[Ljava/nio/file/OpenOption;[Ljava/nio/file/attribute/FileAttribute;)Ljava/nio/file/Path;",
        ) => Ok(Some(Value::Reference(args[0].as_reference()?))),
        ("java/nio/file/Files", "size", "(Ljava/nio/file/Path;)J") => Ok(Some(Value::Long(0))),
        ("java/nio/file/Files", "isHidden", "(Ljava/nio/file/Path;)Z") => Ok(Some(Value::Int(0))),
        (
            "java/nio/file/Files",
            "getFileStore",
            "(Ljava/nio/file/Path;)Ljava/nio/file/FileStore;",
        ) => Ok(Some(Value::Reference(Reference::Null))),
        (
            "java/nio/file/Files",
            "newInputStream",
            "(Ljava/nio/file/Path;[Ljava/nio/file/OpenOption;)Ljava/io/InputStream;",
        ) => Ok(Some(Value::Reference(Reference::Null))),
        (
            "java/nio/file/Files",
            "newOutputStream",
            "(Ljava/nio/file/Path;[Ljava/nio/file/OpenOption;)Ljava/io/OutputStream;",
        ) => Ok(Some(Value::Reference(Reference::Null))),
        (
            "java/nio/file/Files",
            "newBufferedReader",
            "(Ljava/nio/file/Path;)Ljava/io/BufferedReader;",
        ) => Ok(Some(Value::Reference(Reference::Null))),
        (
            "java/nio/file/Files",
            "newBufferedWriter",
            "(Ljava/nio/file/Path;[Ljava/nio/file/OpenOption;)Ljava/io/BufferedWriter;",
        ) => Ok(Some(Value::Reference(Reference::Null))),
        // --- FileStore stubs ---
        ("java/nio/file/FileStore", "name", "()Ljava/lang/String;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/nio/file/FileStore", "type", "()Ljava/lang/String;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/nio/file/FileStore", "getTotalSpace", "()J") => Ok(Some(Value::Long(0))),
        ("java/nio/file/FileStore", "getUsableSpace", "()J") => Ok(Some(Value::Long(0))),
        ("java/nio/file/FileStore", "getUnallocatedSpace", "()J") => Ok(Some(Value::Long(0))),
        ("java/nio/file/FileStore", "isReadOnly", "()Z") => Ok(Some(Value::Int(0))),
        // --- Channels stubs ---
        (
            "java/nio/channels/Channels",
            "newInputStream",
            "(Ljava/nio/channels/ReadableByteChannel;)Ljava/io/InputStream;",
        ) => Ok(Some(Value::Reference(Reference::Null))),
        (
            "java/nio/channels/Channels",
            "newOutputStream",
            "(Ljava/nio/channels/WritableByteChannel;)Ljava/io/OutputStream;",
        ) => Ok(Some(Value::Reference(Reference::Null))),
        (
            "java/nio/channels/Channels",
            "newChannel",
            "(Ljava/io/InputStream;)Ljava/nio/channels/ReadableByteChannel;",
        ) => Ok(Some(Value::Reference(Reference::Null))),
        (
            "java/nio/channels/Channels",
            "newChannel",
            "(Ljava/io/OutputStream;)Ljava/nio/channels/WritableByteChannel;",
        ) => Ok(Some(Value::Reference(Reference::Null))),
        // --- Console stubs ---
        ("java/io/Console", "readLine", "()Ljava/lang/String;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        (
            "java/io/Console",
            "readLine",
            "(Ljava/lang/String;;[Ljava/lang/Object;)Ljava/lang/String;",
        ) => Ok(Some(Value::Reference(Reference::Null))),
        (
            "java/io/Console",
            "printf",
            "(Ljava/lang/String;[Ljava/lang/Object;)Ljava/io/Console;",
        ) => Ok(Some(Value::Reference(Reference::Null))),
        (
            "java/io/Console",
            "format",
            "(Ljava/lang/String;[Ljava/lang/Object;)Ljava/io/Console;",
        ) => Ok(Some(Value::Reference(Reference::Null))),
        ("java/io/Console", "flush", "()V") => Ok(None),
        // --- StandardOpenOption stubs ---
        ("java/nio/file/StandardOpenOption", "name", "()Ljava/lang/String;") => {
            let obj_ref = args[0].as_reference()?;
            let name = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => fields.get("name").and_then(|v| match v {
                        Value::Reference(r) => Some(*r),
                        _ => None,
                    }),
                    _ => None,
                }
            };
            Ok(Some(Value::Reference(name.unwrap_or(Reference::Null))))
        }
        ("java/nio/file/StandardOpenOption", "ordinal", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let ordinal = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => fields
                        .get("ordinal")
                        .and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        })
                        .unwrap_or(0),
                    _ => 0,
                }
            };
            Ok(Some(Value::Int(ordinal)))
        }
        _ => Err(VmError::UnhandledException {
            class_name: "".to_string(),
        }),
    }
}
