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
                    HeapValue::Object { fields, .. } => {
                        fields.get("__capacity").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0)
                    }
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
                    HeapValue::Object { fields, .. } => {
                        fields.get("__position").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0)
                    }
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
                    HeapValue::Object { fields, .. } => {
                        fields.get("__limit").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0)
                    }
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
        ("java/nio/Buffer", "mark", "()Ljava/nio/Buffer;") => Ok(Some(Value::Reference(args[0].as_reference()?))),
        ("java/nio/Buffer", "reset", "()Ljava/nio/Buffer;") => Ok(Some(Value::Reference(args[0].as_reference()?))),
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
                        let pos = fields.get("__position").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
                        let lim = fields.get("__limit").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
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
                        let pos = fields.get("__position").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
                        let lim = fields.get("__limit").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
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
            let length = if args.len() > 3 { args[2].as_int()? } else {
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
                    HeapValue::Object { fields, .. } => {
                        fields.get("__capacity").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0)
                    }
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
                    HeapValue::Object { fields, .. } => {
                        fields.get("__position").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0)
                    }
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
                    HeapValue::Object { fields, .. } => {
                        fields.get("__limit").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0)
                    }
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
        ("java/nio/ByteBuffer", "mark", "()Ljava/nio/Buffer;") => Ok(Some(Value::Reference(args[0].as_reference()?))),
        ("java/nio/ByteBuffer", "reset", "()Ljava/nio/Buffer;") => Ok(Some(Value::Reference(args[0].as_reference()?))),
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
                        let pos = fields.get("__position").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
                        let lim = fields.get("__limit").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
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
                        let pos = fields.get("__position").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
                        let lim = fields.get("__limit").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
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
                        let offset = fields.get("__offset").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
                        let pos = fields.get("__position").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
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
                    let pos = fields.get("__position").and_then(|v| match v {
                        Value::Int(i) => Some(*i),
                        _ => None,
                    }).unwrap_or(0);
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
                        let offset = fields.get("__offset").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
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
                        let offset = fields.get("__offset").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
                        let pos = fields.get("__position").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
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
                    let offset = fields.get("__offset").and_then(|v| match v {
                        Value::Int(i) => Some(*i),
                        _ => None,
                    }).unwrap_or(0);
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
            Ok(Some(backing.map(Value::Reference).unwrap_or(Value::Reference(Reference::Null))))
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
            let length = if args.len() > 3 { args[2].as_int()? } else {
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
                    HeapValue::Object { fields, .. } => {
                        fields.get("__capacity").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0)
                    }
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
                    HeapValue::Object { fields, .. } => {
                        fields.get("__position").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0)
                    }
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
                    HeapValue::Object { fields, .. } => {
                        fields.get("__limit").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0)
                    }
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
        ("java/nio/CharBuffer", "mark", "()Ljava/nio/Buffer;") => Ok(Some(Value::Reference(args[0].as_reference()?))),
        ("java/nio/CharBuffer", "reset", "()Ljava/nio/Buffer;") => Ok(Some(Value::Reference(args[0].as_reference()?))),
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
                        let pos = fields.get("__position").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
                        let lim = fields.get("__limit").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
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
                        let pos = fields.get("__position").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
                        let lim = fields.get("__limit").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
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
                        let offset = fields.get("__offset").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
                        let pos = fields.get("__position").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
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
                    let pos = fields.get("__position").and_then(|v| match v {
                        Value::Int(i) => Some(*i),
                        _ => None,
                    }).unwrap_or(0);
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
                        let offset = fields.get("__offset").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
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
                        let offset = fields.get("__offset").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
                        let pos = fields.get("__position").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
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
                    let offset = fields.get("__offset").and_then(|v| match v {
                        Value::Int(i) => Some(*i),
                        _ => None,
                    }).unwrap_or(0);
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
            Ok(Some(backing.map(Value::Reference).unwrap_or(Value::Reference(Reference::Null))))
        }
        ("java/nio/CharBuffer", "length", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let len = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        fields.get("__capacity").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0)
                    }
                    _ => 0,
                }
            };
            Ok(Some(Value::Int(len)))
        }
        _ => Err(VmError::UnhandledException {
            class_name: "".to_string(),
        }),
    }
}
