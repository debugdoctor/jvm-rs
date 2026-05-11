use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

use crate::vm::{HeapValue, Reference, Value, Vm, VmError};

fn path_string_from_object(
    vm: &Vm,
    object_ref: Reference,
) -> Result<Option<String>, VmError> {
    let heap = vm.heap.lock().unwrap();
    match heap.get(object_ref)? {
        HeapValue::Object { fields, .. } => {
            let path_ref = fields.get(0).and_then(|value| match value {
                Value::Reference(reference) => Some(*reference),
                _ => None,
            });
            match path_ref {
                Some(Reference::Null) | None => Ok(None),
                Some(path_ref) => match heap.get(path_ref)? {
                    HeapValue::String(path) => Ok(Some(path.clone())),
                    value => Err(VmError::InvalidHeapValue {
                        expected: "string",
                        actual: value.kind_name(),
                    }),
                },
            }
        }
        value => Err(VmError::InvalidHeapValue {
            expected: "object",
            actual: value.kind_name(),
        }),
    }
}

fn new_path_object(vm: &mut Vm, path: impl Into<String>) -> Reference {
    let path_value = vm.new_string(path.into());
    vm.heap.lock().unwrap().allocate(HeapValue::Object {
        class_name: "java/nio/file/Path".to_string(),
        fields: vec![path_value],
    })
}

fn java_string_from_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    let mut stack: Vec<OsString> = Vec::new();
    let mut absolute = false;

    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => {
                absolute = true;
                normalized.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if let Some(last) = stack.last() {
                    if last != ".." {
                        stack.pop();
                        continue;
                    }
                }
                if !absolute {
                    stack.push(OsString::from(".."));
                }
            }
            Component::Normal(part) => stack.push(part.to_os_string()),
        }
    }

    for part in stack {
        normalized.push(part);
    }

    normalized
}

fn path_name_components(path: &Path) -> Vec<OsString> {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_os_string()),
            Component::ParentDir => Some(OsString::from("..")),
            Component::CurDir => Some(OsString::from(".")),
            _ => None,
        })
        .collect()
}

fn root_path(path: &Path) -> Option<PathBuf> {
    let mut root = PathBuf::new();
    let mut saw_root = false;
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => root.push(prefix.as_os_str()),
            Component::RootDir => {
                saw_root = true;
                root.push(component.as_os_str());
                break;
            }
            _ => break,
        }
    }

    if saw_root || !root.as_os_str().is_empty() {
        Some(root)
    } else {
        None
    }
}

fn path_reference_to_pathbuf(vm: &Vm, reference: Reference) -> Result<PathBuf, VmError> {
    let path = path_string_from_object(vm, reference)?.ok_or(VmError::NullReference)?;
    Ok(PathBuf::from(path))
}

fn io_exception_for_error(error: &std::io::Error) -> &'static str {
    match error.kind() {
        std::io::ErrorKind::NotFound => "java/nio/file/NoSuchFileException",
        std::io::ErrorKind::AlreadyExists => "java/nio/file/FileAlreadyExistsException",
        std::io::ErrorKind::PermissionDenied => "java/nio/file/AccessDeniedException",
        std::io::ErrorKind::DirectoryNotEmpty => "java/nio/file/DirectoryNotEmptyException",
        _ => "java/io/IOException",
    }
}

fn read_open_options(vm: &Vm, options_ref: Reference) -> Result<(bool, bool, bool, bool), VmError> {
    let mut append = false;
    let mut truncate = false;
    let mut create = false;
    let mut create_new = false;

    if options_ref == Reference::Null {
        return Ok((append, truncate, create, create_new));
    }

    let option_refs = {
        let heap = vm.heap.lock().unwrap();
        match heap.get(options_ref)? {
            HeapValue::ReferenceArray { values, .. } => values.clone(),
            value => {
                return Err(VmError::InvalidHeapValue {
                    expected: "reference-array",
                    actual: value.kind_name(),
                });
            }
        }
    };

    for option_ref in option_refs {
        if option_ref == Reference::Null {
            continue;
        }
        let name = {
            let heap = vm.heap.lock().unwrap();
            match heap.get(option_ref)? {
                HeapValue::Object { fields, .. } => {
                    let name_ref = fields.get(0).and_then(|value| match value {
                        Value::Reference(reference) => Some(*reference),
                        _ => None,
                    });
                    match name_ref {
                        Some(name_ref) => match heap.get(name_ref)? {
                            HeapValue::String(name) => name.clone(),
                            value => {
                                return Err(VmError::InvalidHeapValue {
                                    expected: "string",
                                    actual: value.kind_name(),
                                });
                            }
                        },
                        None => continue,
                    }
                }
                value => {
                    return Err(VmError::InvalidHeapValue {
                        expected: "object",
                        actual: value.kind_name(),
                    });
                }
            }
        };

        match name.as_str() {
            "APPEND" => append = true,
            "TRUNCATE_EXISTING" => truncate = true,
            "CREATE" => create = true,
            "CREATE_NEW" => create_new = true,
            _ => {}
        }
    }

    Ok((append, truncate, create, create_new))
}

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
                        .get(0)
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
                        .get(1)
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
                    fields[1] = Value::Int(new_pos);
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
                        .get(2)
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
                    fields[2] = Value::Int(new_limit);
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
                    let cap = fields[0].clone();
                    fields[1] = Value::Int(0);
                    fields[2] = cap;
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/Buffer", "flip", "()Ljava/nio/Buffer;") => {
            let obj_ref = args[0].as_reference()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    let lim = fields[1].clone();
                    fields[2] = lim;
                    fields[1] = Value::Int(0);
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/Buffer", "rewind", "()Ljava/nio/Buffer;") => {
            let obj_ref = args[0].as_reference()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields[1] = Value::Int(0);
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
                            .get(1)
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        let lim = fields
                            .get(2)
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
                            .get(1)
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        let lim = fields
                            .get(2)
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
                fields: vec![
                    Value::Reference(backing),
                    Value::Int(0),
                    Value::Int(capacity as i32),
                    Value::Int(0),
                    Value::Int(capacity as i32),
                ],
            });
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
                fields: vec![
                    Value::Reference(array_ref),
                    Value::Int(offset),
                    Value::Int(length),
                    Value::Int(0),
                    Value::Int(length),
                ],
            });
            Ok(Some(Value::Reference(buf_ref)))
        }
        ("java/nio/ByteBuffer", "capacity", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let cap = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => fields
                        .get(2)
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
                        .get(3)
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
                    fields[3] = Value::Int(new_pos);
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
                        .get(4)
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
                    fields[4] = Value::Int(new_limit);
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
                    let cap = fields[2].clone();
                    fields[3] = Value::Int(0);
                    fields[4] = cap;
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/ByteBuffer", "flip", "()Ljava/nio/Buffer;") => {
            let obj_ref = args[0].as_reference()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    let lim = fields[3].clone();
                    fields[4] = lim;
                    fields[3] = Value::Int(0);
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/ByteBuffer", "rewind", "()Ljava/nio/Buffer;") => {
            let obj_ref = args[0].as_reference()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields[3] = Value::Int(0);
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
                            .get(3)
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        let lim = fields
                            .get(4)
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
                            .get(3)
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        let lim = fields
                            .get(4)
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
                        let backing = fields.get(0).and_then(|v| match v {
                            Value::Reference(r) => Some(*r),
                            _ => None,
                        });
                        let offset = fields
                            .get(1)
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        let pos = fields
                            .get(3)
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
                        .get(3)
                        .and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        })
                        .unwrap_or(0);
                    fields[3] = Value::Int(pos + 1);
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
                        let backing = fields.get(0).and_then(|v| match v {
                            Value::Reference(r) => Some(*r),
                            _ => None,
                        });
                        let offset = fields
                            .get(1)
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
                        let backing = fields.get(0).and_then(|v| match v {
                            Value::Reference(r) => Some(*r),
                            _ => None,
                        });
                        let offset = fields
                            .get(1)
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        let pos = fields
                            .get(3)
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
                    fields[3] = Value::Int(pos + 1);
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
                    let backing = fields.get(0).and_then(|v| match v {
                        Value::Reference(r) => Some(*r),
                        _ => None,
                    });
                    let offset = fields
                        .get(1)
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
                        fields.get(0).and_then(|v| match v {
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
                fields: vec![
                    Value::Reference(backing),
                    Value::Int(0),
                    Value::Int(capacity as i32),
                    Value::Int(0),
                    Value::Int(capacity as i32),
                ],
            });
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
                fields: vec![
                    Value::Reference(array_ref),
                    Value::Int(offset),
                    Value::Int(length),
                    Value::Int(0),
                    Value::Int(length),
                ],
            });
            Ok(Some(Value::Reference(buf_ref)))
        }
        ("java/nio/CharBuffer", "capacity", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let cap = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => fields
                        .get(2)
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
                        .get(3)
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
                    fields[3] = Value::Int(new_pos);
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
                        .get(4)
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
                    fields[4] = Value::Int(new_limit);
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
                    let cap = fields[2].clone();
                    fields[3] = Value::Int(0);
                    fields[4] = cap;
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/CharBuffer", "flip", "()Ljava/nio/Buffer;") => {
            let obj_ref = args[0].as_reference()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    let lim = fields[3].clone();
                    fields[4] = lim;
                    fields[3] = Value::Int(0);
                }
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/nio/CharBuffer", "rewind", "()Ljava/nio/Buffer;") => {
            let obj_ref = args[0].as_reference()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields[3] = Value::Int(0);
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
                            .get(3)
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        let lim = fields
                            .get(4)
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
                            .get(3)
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        let lim = fields
                            .get(4)
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
                        let backing = fields.get(0).and_then(|v| match v {
                            Value::Reference(r) => Some(*r),
                            _ => None,
                        });
                        let offset = fields
                            .get(1)
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        let pos = fields
                            .get(3)
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
                        .get(3)
                        .and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        })
                        .unwrap_or(0);
                    fields[3] = Value::Int(pos + 1);
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
                        let backing = fields.get(0).and_then(|v| match v {
                            Value::Reference(r) => Some(*r),
                            _ => None,
                        });
                        let offset = fields
                            .get(1)
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
                        let backing = fields.get(0).and_then(|v| match v {
                            Value::Reference(r) => Some(*r),
                            _ => None,
                        });
                        let offset = fields
                            .get(1)
                            .and_then(|v| match v {
                                Value::Int(i) => Some(*i),
                                _ => None,
                            })
                            .unwrap_or(0);
                        let pos = fields
                            .get(3)
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
                    fields[3] = Value::Int(pos + 1);
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
                    let backing = fields.get(0).and_then(|v| match v {
                        Value::Reference(r) => Some(*r),
                        _ => None,
                    });
                    let offset = fields
                        .get(1)
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
                        fields.get(0).and_then(|v| match v {
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
                        .get(2)
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
        // --- Path ---
        ("java/nio/file/Path", "getFileName", "()Ljava/nio/file/Path;") => {
            let obj_ref = args[0].as_reference()?;
            let path = path_reference_to_pathbuf(vm, obj_ref)?;
            let result = path
                .file_name()
                .map(|name| {
                    Value::Reference(new_path_object(vm, name.to_string_lossy().into_owned()))
                })
                .unwrap_or(Value::Reference(Reference::Null));
            Ok(Some(result))
        }
        ("java/nio/file/Path", "getFileName", "()Ljava/lang/String;") => {
            let obj_ref = args[0].as_reference()?;
            let path = path_reference_to_pathbuf(vm, obj_ref)?;
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            Ok(Some(vm.new_string(name)))
        }
        ("java/nio/file/Path", "getParent", "()Ljava/nio/file/Path;") => {
            let obj_ref = args[0].as_reference()?;
            let path = path_reference_to_pathbuf(vm, obj_ref)?;
            let result = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .map(|parent| Value::Reference(new_path_object(vm, java_string_from_path(parent))))
                .unwrap_or(Value::Reference(Reference::Null));
            Ok(Some(result))
        }
        ("java/nio/file/Path", "getRoot", "()Ljava/nio/file/Path;") => {
            let obj_ref = args[0].as_reference()?;
            let path = path_reference_to_pathbuf(vm, obj_ref)?;
            let result = root_path(&path)
                .map(|root| Value::Reference(new_path_object(vm, java_string_from_path(&root))))
                .unwrap_or(Value::Reference(Reference::Null));
            Ok(Some(result))
        }
        ("java/nio/file/Path", "isAbsolute", "()Z") => {
            let obj_ref = args[0].as_reference()?;
            let path = path_reference_to_pathbuf(vm, obj_ref)?;
            Ok(Some(Value::Int(path.is_absolute() as i32)))
        }
        ("java/nio/file/Path", "getNameCount", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let path = path_reference_to_pathbuf(vm, obj_ref)?;
            Ok(Some(Value::Int(path_name_components(&path).len() as i32)))
        }
        ("java/nio/file/Path", "getName", "(I)Ljava/nio/file/Path;") => {
            let obj_ref = args[0].as_reference()?;
            let index = args[1].as_int()?;
            let path = path_reference_to_pathbuf(vm, obj_ref)?;
            let parts = path_name_components(&path);
            if index < 0 || index as usize >= parts.len() {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/IllegalArgumentException".to_string(),
                });
            }
            let part = parts[index as usize].to_string_lossy().into_owned();
            Ok(Some(Value::Reference(new_path_object(vm, part))))
        }
        ("java/nio/file/Path", "getName", "(I)Ljava/lang/String;") => {
            let obj_ref = args[0].as_reference()?;
            let index = args[1].as_int()?;
            let path = path_reference_to_pathbuf(vm, obj_ref)?;
            let parts = path_name_components(&path);
            if index < 0 || index as usize >= parts.len() {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/IllegalArgumentException".to_string(),
                });
            }
            Ok(Some(vm.new_string(
                parts[index as usize].to_string_lossy().into_owned(),
            )))
        }
        ("java/nio/file/Path", "subpath", "(II)Ljava/nio/file/Path;") => {
            let obj_ref = args[0].as_reference()?;
            let begin = args[1].as_int()?;
            let end = args[2].as_int()?;
            let path = path_reference_to_pathbuf(vm, obj_ref)?;
            let parts = path_name_components(&path);
            if begin < 0 || end < 0 || begin >= end || end as usize > parts.len() {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/IllegalArgumentException".to_string(),
                });
            }
            let mut subpath = PathBuf::new();
            for part in &parts[begin as usize..end as usize] {
                subpath.push(part);
            }
            Ok(Some(Value::Reference(new_path_object(
                vm,
                java_string_from_path(&subpath),
            ))))
        }
        ("java/nio/file/Path", "toString", "()Ljava/lang/String;") => {
            let obj_ref = args[0].as_reference()?;
            let path_str = path_string_from_object(vm, obj_ref)?;
            Ok(match path_str {
                Some(s) => Some(vm.new_string(s)),
                None => Some(Value::Reference(Reference::Null)),
            })
        }
        ("java/nio/file/Path", "toUri", "()Ljava/net/URI;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/nio/file/Path", "toAbsolutePath", "()Ljava/nio/file/Path;") => {
            let obj_ref = args[0].as_reference()?;
            let path = path_reference_to_pathbuf(vm, obj_ref)?;
            let absolute = if path.is_absolute() {
                path
            } else {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(path)
            };
            Ok(Some(Value::Reference(new_path_object(
                vm,
                java_string_from_path(&normalize_path(&absolute)),
            ))))
        }
        ("java/nio/file/Path", "normalize", "()Ljava/nio/file/Path;") => {
            let obj_ref = args[0].as_reference()?;
            let path = path_reference_to_pathbuf(vm, obj_ref)?;
            Ok(Some(Value::Reference(new_path_object(
                vm,
                java_string_from_path(&normalize_path(&path)),
            ))))
        }
        ("java/nio/file/Path", "resolve", "(Ljava/lang/String;)Ljava/nio/file/Path;") => {
            let obj_ref = args[0].as_reference()?;
            let other_ref = args[1].as_reference()?;
            let base = path_reference_to_pathbuf(vm, obj_ref)?;
            let other = match other_ref {
                Reference::Null => {
                    return Err(VmError::UnhandledException {
                        class_name: "java/lang/NullPointerException".to_string(),
                    });
                }
                _ => crate::vm::builtin::helpers::stringify_reference(vm, other_ref)?,
            };
            let other_path = PathBuf::from(other);
            let resolved = if other_path.is_absolute() {
                other_path
            } else {
                base.join(other_path)
            };
            Ok(Some(Value::Reference(new_path_object(
                vm,
                java_string_from_path(&resolved),
            ))))
        }
        ("java/nio/file/Path", "startsWith", "(Ljava/lang/String;)Z") => {
            let obj_ref = args[0].as_reference()?;
            let prefix_ref = args[1].as_reference()?;
            let path = normalize_path(&path_reference_to_pathbuf(vm, obj_ref)?);
            let prefix = normalize_path(&PathBuf::from(
                crate::vm::builtin::helpers::stringify_reference(vm, prefix_ref)?,
            ));
            Ok(Some(Value::Int(path.starts_with(&prefix) as i32)))
        }
        ("java/nio/file/Path", "endsWith", "(Ljava/lang/String;)Z") => {
            let obj_ref = args[0].as_reference()?;
            let suffix_ref = args[1].as_reference()?;
            let path = normalize_path(&path_reference_to_pathbuf(vm, obj_ref)?);
            let suffix = normalize_path(&PathBuf::from(
                crate::vm::builtin::helpers::stringify_reference(vm, suffix_ref)?,
            ));
            Ok(Some(Value::Int(path.ends_with(&suffix) as i32)))
        }
        // --- Paths ---
        (
            "java/nio/file/Paths",
            "get",
            "(Ljava/lang/String;[Ljava/lang/String;)Ljava/nio/file/Path;",
        ) => {
            let first =
                crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            let more_ref = args[1].as_reference()?;
            let mut path = PathBuf::from(first);
            if more_ref != Reference::Null {
                let parts = {
                    let heap = vm.heap.lock().unwrap();
                    match heap.get(more_ref)? {
                        HeapValue::ReferenceArray { values, .. } => values.clone(),
                        value => {
                            return Err(VmError::InvalidHeapValue {
                                expected: "reference-array",
                                actual: value.kind_name(),
                            });
                        }
                    }
                };
                for part_ref in parts {
                    if part_ref != Reference::Null {
                        path.push(crate::vm::builtin::helpers::stringify_reference(
                            vm, part_ref,
                        )?);
                    }
                }
            }
            Ok(Some(Value::Reference(new_path_object(
                vm,
                java_string_from_path(&path),
            ))))
        }
        // --- Files ---
        ("java/nio/file/Files", "exists", "(Ljava/nio/file/Path;[Ljava/nio/file/LinkOption;)Z")
        | (
            "java/nio/file/Files",
            "exists",
            "(Ljava/nio/file/Path;[Ljava/nio/file/attribute/FileAttribute;)Z",
        ) => {
            let path = path_reference_to_pathbuf(vm, args[0].as_reference()?)?;
            Ok(Some(Value::Int(path.exists() as i32)))
        }
        ("java/nio/file/Files", "isRegularFile", "(Ljava/nio/file/Path;)Z") => {
            let path = path_reference_to_pathbuf(vm, args[0].as_reference()?)?;
            Ok(Some(Value::Int(path.is_file() as i32)))
        }
        ("java/nio/file/Files", "isDirectory", "(Ljava/nio/file/Path;)Z") => {
            let path = path_reference_to_pathbuf(vm, args[0].as_reference()?)?;
            Ok(Some(Value::Int(path.is_dir() as i32)))
        }
        (
            "java/nio/file/Files",
            "createFile",
            "(Ljava/nio/file/Path;[Ljava/nio/file/attribute/FileAttribute;)Ljava/nio/file/Path;",
        ) => {
            let path_ref = args[0].as_reference()?;
            let path = path_reference_to_pathbuf(vm, path_ref)?;
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|error| VmError::UnhandledException {
                    class_name: io_exception_for_error(&error).to_string(),
                })?;
            Ok(Some(Value::Reference(path_ref)))
        }
        ("java/nio/file/Files", "delete", "(Ljava/nio/file/Path;)V") => {
            let path = path_reference_to_pathbuf(vm, args[0].as_reference()?)?;
            let metadata = fs::metadata(&path).map_err(|error| VmError::UnhandledException {
                class_name: io_exception_for_error(&error).to_string(),
            })?;
            let result = if metadata.is_dir() {
                fs::remove_dir(&path)
            } else {
                fs::remove_file(&path)
            };
            result.map_err(|error| VmError::UnhandledException {
                class_name: io_exception_for_error(&error).to_string(),
            })?;
            Ok(None)
        }
        (
            "java/nio/file/Files",
            "copy",
            "(Ljava/nio/file/Path;Ljava/nio/file/Path;[Ljava/nio/file/CopyOption;)Ljava/nio/file/Path;",
        ) => {
            let source = path_reference_to_pathbuf(vm, args[0].as_reference()?)?;
            let target_ref = args[1].as_reference()?;
            let target = path_reference_to_pathbuf(vm, target_ref)?;
            let metadata = fs::metadata(&source).map_err(|error| VmError::UnhandledException {
                class_name: io_exception_for_error(&error).to_string(),
            })?;
            if metadata.is_dir() {
                return Err(VmError::UnhandledException {
                    class_name: "java/nio/file/FileSystemException".to_string(),
                });
            }
            if target.exists() {
                return Err(VmError::UnhandledException {
                    class_name: "java/nio/file/FileAlreadyExistsException".to_string(),
                });
            }
            fs::copy(&source, &target).map_err(|error| VmError::UnhandledException {
                class_name: io_exception_for_error(&error).to_string(),
            })?;
            Ok(Some(Value::Reference(target_ref)))
        }
        (
            "java/nio/file/Files",
            "move",
            "(Ljava/nio/file/Path;Ljava/nio/file/Path;[Ljava/nio/file/CopyOption;)Ljava/nio/file/Path;",
        ) => {
            let source = path_reference_to_pathbuf(vm, args[0].as_reference()?)?;
            let target_ref = args[1].as_reference()?;
            let target = path_reference_to_pathbuf(vm, target_ref)?;
            if target.exists() {
                return Err(VmError::UnhandledException {
                    class_name: "java/nio/file/FileAlreadyExistsException".to_string(),
                });
            }
            fs::rename(&source, &target).map_err(|error| VmError::UnhandledException {
                class_name: io_exception_for_error(&error).to_string(),
            })?;
            Ok(Some(Value::Reference(target_ref)))
        }
        ("java/nio/file/Files", "readString", "(Ljava/nio/file/Path;)Ljava/lang/String;") => {
            let path = path_reference_to_pathbuf(vm, args[0].as_reference()?)?;
            let content =
                fs::read_to_string(&path).map_err(|error| VmError::UnhandledException {
                    class_name: io_exception_for_error(&error).to_string(),
                })?;
            Ok(Some(vm.new_string(content)))
        }
        (
            "java/nio/file/Files",
            "writeString",
            "(Ljava/nio/file/Path;Ljava/lang/CharSequence;[Ljava/nio/file/OpenOption;[Ljava/nio/file/attribute/FileAttribute;)Ljava/nio/file/Path;",
        ) => {
            let path_ref = args[0].as_reference()?;
            let path = path_reference_to_pathbuf(vm, path_ref)?;
            let char_sequence_ref = args[1].as_reference()?;
            if char_sequence_ref == Reference::Null {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/NullPointerException".to_string(),
                });
            }
            let content = vm.stringify_heap(char_sequence_ref)?;
            let (append, truncate, create, create_new) =
                read_open_options(vm, args[2].as_reference()?)?;
            let mut options = OpenOptions::new();
            options.write(true);
            if append {
                options.append(true).create(true);
            } else {
                options.truncate(truncate || (!create_new && !append));
            }
            if create || (!append && !create_new) {
                options.create(true);
            }
            if create_new {
                options.create_new(true);
            }
            let mut file = options
                .open(&path)
                .map_err(|error| VmError::UnhandledException {
                    class_name: io_exception_for_error(&error).to_string(),
                })?;
            if !append {
                file.seek(SeekFrom::Start(0))
                    .map_err(|error| VmError::UnhandledException {
                        class_name: io_exception_for_error(&error).to_string(),
                    })?;
            }
            file.write_all(content.as_bytes())
                .map_err(|error| VmError::UnhandledException {
                    class_name: io_exception_for_error(&error).to_string(),
                })?;
            Ok(Some(Value::Reference(path_ref)))
        }
        ("java/nio/file/Files", "size", "(Ljava/nio/file/Path;)J") => {
            let path = path_reference_to_pathbuf(vm, args[0].as_reference()?)?;
            let size = fs::metadata(&path)
                .map_err(|error| VmError::UnhandledException {
                    class_name: io_exception_for_error(&error).to_string(),
                })?
                .len();
            Ok(Some(Value::Long(size as i64)))
        }
        ("java/nio/file/Files", "isHidden", "(Ljava/nio/file/Path;)Z") => {
            let path = path_reference_to_pathbuf(vm, args[0].as_reference()?)?;
            let hidden = path
                .file_name()
                .map(|name| name.to_string_lossy().starts_with('.'))
                .unwrap_or(false);
            Ok(Some(Value::Int(hidden as i32)))
        }
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
                    HeapValue::Object { fields, .. } => fields.get(0).and_then(|v| match v {
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
                        .get(1)
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
