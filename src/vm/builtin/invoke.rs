use std::collections::HashMap;
use std::fs::{self, File as FsFile};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::vm::builtin::helpers::stringify_reference;
use crate::vm::builtin::invoke_nio;
use crate::vm::types::stub_return_value_tracked;
use crate::vm::{ClassMethod, HeapValue, Reference, RuntimeClass, Value, Vm, VmError};

fn file_path_string(vm: &Vm, file_ref: Reference) -> Result<Option<String>, VmError> {
    let heap = vm.heap.lock().unwrap();
    match heap.get(file_ref)? {
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

fn new_file_object(vm: &mut Vm, path: impl Into<String>) -> Reference {
    let path_ref = vm.new_string(path.into());
    vm.heap.lock().unwrap().allocate(HeapValue::Object {
        class_name: "java/io/File".to_string(),
        fields: vec![path_ref],
    })
}

fn io_exception_for_error(error: &std::io::Error) -> &'static str {
    match error.kind() {
        std::io::ErrorKind::NotFound => "java/io/FileNotFoundException",
        std::io::ErrorKind::AlreadyExists => "java/nio/file/FileAlreadyExistsException",
        std::io::ErrorKind::PermissionDenied => "java/nio/file/AccessDeniedException",
        std::io::ErrorKind::DirectoryNotEmpty => "java/nio/file/DirectoryNotEmptyException",
        _ => "java/io/IOException",
    }
}

fn new_file_descriptor(vm: &mut Vm, resource_id: u64) -> Reference {
    vm.heap.lock().unwrap().allocate(HeapValue::Object {
        class_name: "java/io/FileDescriptor".to_string(),
        fields: vec![Value::Long(resource_id as i64)],
    })
}

fn fd_resource_id(vm: &Vm, fd_ref: Reference) -> Result<u64, VmError> {
    let heap = vm.heap.lock().unwrap();
    match heap.get(fd_ref)? {
        HeapValue::Object { fields, .. } => match fields.get(0) {
            Some(Value::Long(id)) => Ok(*id as u64),
            _ => Ok(0),
        },
        _ => Ok(0),
    }
}

fn fd_set_resource_id(vm: &mut Vm, fd_ref: Reference, id: u64) -> Result<(), VmError> {
    let mut heap = vm.heap.lock().unwrap();
    if let HeapValue::Object { fields, .. } = heap.get_mut(fd_ref)? {
        if !fields.is_empty() {
            fields[0] = Value::Long(id as i64);
        }
    }
    Ok(())
}

fn fis_get_fd_ref(vm: &Vm, obj_ref: Reference) -> Result<Reference, VmError> {
    let heap = vm.heap.lock().unwrap();
    match heap.get(obj_ref)? {
        HeapValue::Object { fields, .. } => match fields.get(0) {
            Some(Value::Reference(r)) => Ok(*r),
            _ => Ok(Reference::Null),
        },
        _ => Ok(Reference::Null),
    }
}

fn raf_get_rw(vm: &Vm, obj_ref: Reference) -> Result<bool, VmError> {
    let heap = vm.heap.lock().unwrap();
    match heap.get(obj_ref)? {
        HeapValue::Object { fields, .. } => match fields.get(2) {
            Some(Value::Int(i)) => Ok(*i != 0),
            _ => Ok(false),
        },
        _ => Ok(false),
    }
}

fn raf_set_rw(vm: &mut Vm, obj_ref: Reference, rw: bool) -> Result<(), VmError> {
    let mut heap = vm.heap.lock().unwrap();
    if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
        if fields.len() > 2 {
            fields[2] = Value::Int(if rw { 1 } else { 0 });
        }
    }
    Ok(())
}

fn open_file_stream(
    vm: &mut Vm,
    obj_ref: Reference,
    path_str: &str,
    mode: u8,
) -> Result<(), VmError> {
    let path = PathBuf::from(path_str);
    let file = match mode {
        0 => FsFile::open(&path).map_err(|e| VmError::UnhandledException {
            class_name: io_exception_for_error(&e).to_string(),
        })?,
        1 => fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)
            .map_err(|e| VmError::UnhandledException {
                class_name: io_exception_for_error(&e).to_string(),
            })?,
        2 => FsFile::create(&path).map_err(|e| VmError::UnhandledException {
            class_name: io_exception_for_error(&e).to_string(),
        })?,
        3 => fs::OpenOptions::new()
            .write(true)
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| VmError::UnhandledException {
                class_name: io_exception_for_error(&e).to_string(),
            })?,
        _ => {
            return Err(VmError::UnhandledException {
                class_name: "java/lang/IllegalArgumentException".to_string(),
            });
        }
    };
    let rid = vm.io_resources.alloc(file);
    let fd_ref = fis_get_fd_ref(vm, obj_ref)?;
    fd_set_resource_id(vm, fd_ref, rid)?;
    // Store path in fields[1]
    let path_value = vm.new_string(path_str.to_string());
    {
        let mut heap = vm.heap.lock().unwrap();
        if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
            if fields.len() > 1 {
                fields[1] = path_value;
            }
        }
    }
    Ok(())
}

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
        ("java/lang/Object", "equals", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(i32::from(
            args[0].as_reference()? == args[1].as_reference()?,
        )))),
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
            Ok(Some(vm.new_string(format!(
                "{}@{:x}",
                cls.replace('/', "."),
                id
            ))))
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
        ("java/lang/String", "<init>", "()V") => {
            let obj_ref = args[0].as_reference()?;
            *vm.heap.lock().unwrap().get_mut(obj_ref)? = HeapValue::String(String::new());
            Ok(None)
        }
        ("java/lang/String", "<init>", "(Ljava/lang/String;)V") => {
            let obj_ref = args[0].as_reference()?;
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[1].as_reference()?)?;
            *vm.heap.lock().unwrap().get_mut(obj_ref)? = HeapValue::String(s);
            Ok(None)
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
            let mut a =
                crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
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
            let needle =
                crate::vm::builtin::helpers::stringify_reference(vm, args[1].as_reference()?)?;
            let pos = match s.find(&needle) {
                Some(byte_pos) => s[..byte_pos].chars().count() as i32,
                None => -1,
            };
            Ok(Some(Value::Int(pos)))
        }
        ("java/lang/String", "startsWith", "(Ljava/lang/String;)Z") => {
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            let prefix =
                crate::vm::builtin::helpers::stringify_reference(vm, args[1].as_reference()?)?;
            Ok(Some(Value::Int(if s.starts_with(&prefix) { 1 } else { 0 })))
        }
        ("java/lang/String", "endsWith", "(Ljava/lang/String;)Z") => {
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            let suffix =
                crate::vm::builtin::helpers::stringify_reference(vm, args[1].as_reference()?)?;
            Ok(Some(Value::Int(if s.ends_with(&suffix) { 1 } else { 0 })))
        }
        ("java/lang/String", "contains", "(Ljava/lang/CharSequence;)Z") => {
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            let needle =
                crate::vm::builtin::helpers::stringify_reference(vm, args[1].as_reference()?)?;
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
            let s = if args[0].as_int()? != 0 {
                "true"
            } else {
                "false"
            };
            Ok(Some(vm.new_string(s.to_string())))
        }
        ("java/lang/String", "valueOf", "(C)Ljava/lang/String;") => {
            let ch = char::from_u32(args[0].as_int()? as u32).unwrap_or('\0');
            Ok(Some(vm.new_string(ch.to_string())))
        }
        ("java/lang/String", "valueOf", "(D)Ljava/lang/String;") => Ok(Some(vm.new_string(
            crate::vm::builtin::format::format_float(args[0].as_double()?),
        ))),
        ("java/lang/String", "valueOf", "(F)Ljava/lang/String;") => Ok(Some(vm.new_string(
            crate::vm::builtin::format::format_float(args[0].as_float()? as f64),
        ))),
        ("java/lang/String", "intern", "()Ljava/lang/String;") => {
            let s_ref = args[0].as_reference()?;
            let s_str = match vm.heap.lock().unwrap().get(s_ref)? {
                HeapValue::String(s) => s.clone(),
                _ => return Ok(Some(args[0].clone())),
            };
            let mut pool = vm.string_pool.lock().unwrap();
            if let Some(existing) = pool.get(&s_str) {
                Ok(Some(Value::Reference(*existing)))
            } else {
                pool.insert(s_str, s_ref);
                Ok(Some(Value::Reference(s_ref)))
            }
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
        ("java/lang/Integer", "signum", "(I)I") => Ok(Some(Value::Int(args[0].as_int()?.signum()))),
        ("java/lang/Integer", "intValue", "()I") => {
            let obj_ref = args[0].as_reference()?;
            match vm.heap.lock().unwrap().get(obj_ref)? {
                HeapValue::Object { fields, .. } => {
                    let value = fields.get(0).copied().unwrap_or(Value::Int(0));
                    Ok(Some(value))
                }
                _ => Ok(Some(Value::Int(0))),
            }
        }
        ("java/lang/Integer", "valueOf", "(I)Ljava/lang/Integer;") => {
            let value = args[0].as_int()?;
            let reference = vm.heap.lock().unwrap().allocate(HeapValue::Object {
                class_name: "java/lang/Integer".to_string(),
                fields: vec![Value::Int(value)],
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
            let value =
                i32::from_str_radix(&s, radix).map_err(|_| VmError::UnhandledException {
                    class_name: "java/lang/NumberFormatException".to_string(),
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
                format!(
                    "-{}",
                    crate::vm::builtin::format::format_unsigned_radix(
                        value.unsigned_abs() as u64,
                        radix
                    )
                )
            } else {
                s
            };
            Ok(Some(vm.new_string(s)))
        }
        ("java/lang/Integer", "toBinaryString", "(I)Ljava/lang/String;") => Ok(Some(
            vm.new_string(format!("{:b}", args[0].as_int()? as u32)),
        )),
        ("java/lang/Integer", "toHexString", "(I)Ljava/lang/String;") => Ok(Some(
            vm.new_string(format!("{:x}", args[0].as_int()? as u32)),
        )),
        ("java/lang/Integer", "toOctalString", "(I)Ljava/lang/String;") => Ok(Some(
            vm.new_string(format!("{:o}", args[0].as_int()? as u32)),
        )),
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
                fields[0] = Value::Int(value);
            }
            Ok(None)
        }
        ("java/lang/Long", "<init>", "(J)V") => {
            let obj_ref = args[0].as_reference()?;
            let value = args[1].as_long()?;
            if let Ok(HeapValue::Object { fields, .. }) = vm.heap.lock().unwrap().get_mut(obj_ref) {
                fields[0] = Value::Long(value);
            }
            Ok(None)
        }
        ("java/lang/Long", "longValue", "()J") => {
            let obj_ref = args[0].as_reference()?;
            match vm.heap.lock().unwrap().get(obj_ref)? {
                HeapValue::Object { fields, .. } => {
                    Ok(Some(fields.get(0).copied().unwrap_or(Value::Long(0))))
                }
                _ => Ok(Some(Value::Long(0))),
            }
        }
        ("java/lang/Long", "valueOf", "(J)Ljava/lang/Long;") => {
            let value = args[0].as_long()?;
            let reference = vm.heap.lock().unwrap().allocate(HeapValue::Object {
                class_name: "java/lang/Long".to_string(),
                fields: vec![Value::Long(value)],
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
        ("java/lang/Boolean", "getBoolean", "(Ljava/lang/String;)Z") => Ok(Some(Value::Int(0))),
        ("java/lang/Boolean", "parseBoolean", "(Ljava/lang/String;)Z") => {
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            Ok(Some(Value::Int(if s.eq_ignore_ascii_case("true") {
                1
            } else {
                0
            })))
        }
        ("java/lang/Boolean", "toString", "(Z)Ljava/lang/String;") => {
            let s = if args[0].as_int()? != 0 {
                "true"
            } else {
                "false"
            };
            Ok(Some(vm.new_string(s.to_string())))
        }
        ("java/lang/Boolean", "valueOf", "(Z)Ljava/lang/Boolean;") => {
            let value = args[0].as_int()?;
            let reference = vm.heap.lock().unwrap().allocate(HeapValue::Object {
                class_name: "java/lang/Boolean".to_string(),
                fields: vec![Value::Int(value)],
            });
            Ok(Some(Value::Reference(reference)))
        }
        ("java/lang/Boolean", "booleanValue", "()Z") => {
            let obj_ref = args[0].as_reference()?;
            match vm.heap.lock().unwrap().get(obj_ref)? {
                HeapValue::Object { fields, .. } => {
                    Ok(Some(fields.get(0).copied().unwrap_or(Value::Int(0))))
                }
                _ => Ok(Some(Value::Int(0))),
            }
        }
        ("java/lang/Math", "floor", "(D)D") => {
            Ok(Some(Value::Double(args[0].as_double()?.floor())))
        }
        ("java/lang/Math", "ceil", "(D)D") => Ok(Some(Value::Double(args[0].as_double()?.ceil()))),
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
        ("java/lang/Math", "log", "(D)D") => Ok(Some(Value::Double(args[0].as_double()?.ln()))),
        ("java/lang/Math", "log10", "(D)D") => {
            Ok(Some(Value::Double(args[0].as_double()?.log10())))
        }
        ("java/lang/Math", "exp", "(D)D") => Ok(Some(Value::Double(args[0].as_double()?.exp()))),
        ("java/lang/Math", "sin", "(D)D") => Ok(Some(Value::Double(args[0].as_double()?.sin()))),
        ("java/lang/Math", "cos", "(D)D") => Ok(Some(Value::Double(args[0].as_double()?.cos()))),
        ("java/lang/Math", "tan", "(D)D") => Ok(Some(Value::Double(args[0].as_double()?.tan()))),
        ("java/lang/Math", "floorDiv", "(II)I") => {
            let (x, y) = (args[0].as_int()?, args[1].as_int()?);
            if y == 0 {
                return Err(VmError::DivisionByZero);
            }
            Ok(Some(Value::Int(x.div_euclid(y).wrapping_add(
                if (x % y != 0) && ((x ^ y) < 0) {
                    -1 + 1
                } else {
                    0
                },
            ))))
        }
        ("java/lang/Math", "floorDiv", "(JJ)J") => {
            let (x, y) = (args[0].as_long()?, args[1].as_long()?);
            if y == 0 {
                return Err(VmError::DivisionByZero);
            }
            let q = x / y;
            let q = if (x % y != 0) && ((x ^ y) < 0) {
                q - 1
            } else {
                q
            };
            Ok(Some(Value::Long(q)))
        }
        ("java/lang/Math", "floorMod", "(II)I") => {
            let (x, y) = (args[0].as_int()?, args[1].as_int()?);
            if y == 0 {
                return Err(VmError::DivisionByZero);
            }
            let r = x % y;
            Ok(Some(Value::Int(if (r != 0) && ((r ^ y) < 0) {
                r + y
            } else {
                r
            })))
        }
        ("java/lang/Math", "floorMod", "(JJ)J") => {
            let (x, y) = (args[0].as_long()?, args[1].as_long()?);
            if y == 0 {
                return Err(VmError::DivisionByZero);
            }
            let r = x % y;
            Ok(Some(Value::Long(if (r != 0) && ((r ^ y) < 0) {
                r + y
            } else {
                r
            })))
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
        ("java/lang/Math", "signum", "(I)I") => Ok(Some(Value::Int(args[0].as_int()?.signum()))),
        ("java/lang/Math", "max", "(II)I") => {
            Ok(Some(Value::Int(args[0].as_int()?.max(args[1].as_int()?))))
        }
        ("java/lang/Math", "min", "(II)I") => {
            Ok(Some(Value::Int(args[0].as_int()?.min(args[1].as_int()?))))
        }
        ("java/lang/Math", "abs", "(I)I") => Ok(Some(Value::Int(args[0].as_int()?.wrapping_abs()))),
        ("java/lang/Math", "max", "(JJ)J") => Ok(Some(Value::Long(
            args[0].as_long()?.max(args[1].as_long()?),
        ))),
        ("java/lang/Math", "min", "(JJ)J") => Ok(Some(Value::Long(
            args[0].as_long()?.min(args[1].as_long()?),
        ))),
        ("java/lang/Math", "abs", "(J)J") => {
            Ok(Some(Value::Long(args[0].as_long()?.wrapping_abs())))
        }
        ("java/lang/Math", "max", "(DD)D") => Ok(Some(Value::Double(
            args[0].as_double()?.max(args[1].as_double()?),
        ))),
        ("java/lang/Math", "min", "(DD)D") => Ok(Some(Value::Double(
            args[0].as_double()?.min(args[1].as_double()?),
        ))),
        ("java/lang/Math", "abs", "(D)D") => Ok(Some(Value::Double(args[0].as_double()?.abs()))),
        ("java/lang/Math", "sqrt", "(D)D") => Ok(Some(Value::Double(args[0].as_double()?.sqrt()))),
        ("java/lang/Math", "pow", "(DD)D") => Ok(Some(Value::Double(
            args[0].as_double()?.powf(args[1].as_double()?),
        ))),
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
            let text =
                crate::vm::builtin::helpers::format_value_for_append(vm, descriptor, &args[1..])?;
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
                    buf.chars()
                        .nth(index as usize)
                        .ok_or_else(|| VmError::UnhandledException {
                            class_name: "java/lang/StringIndexOutOfBoundsException".to_string(),
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
            if let HeapValue::StringBuilder(buf) = vm.heap.lock().unwrap().get_mut(obj_ref)? {
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
            if let HeapValue::StringBuilder(buf) = vm.heap.lock().unwrap().get_mut(obj_ref)? {
                let mut chars: Vec<char> = buf.chars().collect();
                if index < 0 || (index as usize) >= chars.len() {
                    return Err(VmError::UnhandledException {
                        class_name: "java/lang/StringIndexOutOfBoundsException".to_string(),
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
            if let HeapValue::StringBuilder(buf) = vm.heap.lock().unwrap().get_mut(obj_ref)? {
                let mut chars: Vec<char> = buf.chars().collect();
                if index < 0 || (index as usize) >= chars.len() {
                    return Err(VmError::UnhandledException {
                        class_name: "java/lang/StringIndexOutOfBoundsException".to_string(),
                    });
                }
                chars[index as usize] = ch;
                *buf = chars.into_iter().collect();
            }
            Ok(None)
        }
        ("java/lang/StringBuilder", "reverse", "()Ljava/lang/StringBuilder;") => {
            let obj_ref = args[0].as_reference()?;
            if let HeapValue::StringBuilder(buf) = vm.heap.lock().unwrap().get_mut(obj_ref)? {
                *buf = buf.chars().rev().collect();
            }
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/lang/StringBuilder", "insert", "(ILjava/lang/String;)Ljava/lang/StringBuilder;") => {
            let obj_ref = args[0].as_reference()?;
            let index = args[1].as_int()?;
            let s = crate::vm::builtin::helpers::stringify_reference(vm, args[2].as_reference()?)?;
            if let HeapValue::StringBuilder(buf) = vm.heap.lock().unwrap().get_mut(obj_ref)? {
                let mut chars: Vec<char> = buf.chars().collect();
                let n = chars.len() as i32;
                if index < 0 || index > n {
                    return Err(VmError::UnhandledException {
                        class_name: "java/lang/StringIndexOutOfBoundsException".to_string(),
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
            if let Some(r) = vm.runtime.lock().unwrap().class_objects.get(KEY).copied() {
                return Ok(Some(Value::Reference(r)));
            }
            let reference = vm.heap.lock().unwrap().allocate(HeapValue::Object {
                class_name: "java/lang/Thread".to_string(),
                fields: vec![],
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
            let index = match obj_ref {
                Reference::Heap(idx) => idx,
                Reference::Null => return Err(VmError::NullReference),
            };
            let thread_name = vm.new_string(format!("Thread-{}", index));
            vm.set_object_field(obj_ref, "target", Value::Reference(Reference::Null))?;
            vm.set_object_field(obj_ref, "name", thread_name)?;
            vm.set_object_field(obj_ref, "priority", Value::Int(5))?;
            vm.set_object_field(obj_ref, "daemon", Value::Int(0))?;
            vm.set_object_field(obj_ref, "threadGroup", Value::Reference(Reference::Null))?;
            vm.set_object_field(obj_ref, "contextClassLoader", Value::Reference(Reference::Null))?;
            vm.set_object_field(obj_ref, "uncaughtExceptionHandler", Value::Reference(Reference::Null))?;
            Ok(None)
        }
        ("java/lang/Thread", "<init>", "(Ljava/lang/Runnable;)V") => {
            let obj_ref = args[0].as_reference()?;
            let index = match obj_ref {
                Reference::Heap(idx) => idx,
                Reference::Null => return Err(VmError::NullReference),
            };
            let thread_name = vm.new_string(format!("Thread-{}", index));
            vm.set_object_field(obj_ref, "target", args[1].clone())?;
            vm.set_object_field(obj_ref, "name", thread_name)?;
            vm.set_object_field(obj_ref, "priority", Value::Int(5))?;
            vm.set_object_field(obj_ref, "daemon", Value::Int(0))?;
            vm.set_object_field(obj_ref, "threadGroup", Value::Reference(Reference::Null))?;
            vm.set_object_field(obj_ref, "contextClassLoader", Value::Reference(Reference::Null))?;
            vm.set_object_field(obj_ref, "uncaughtExceptionHandler", Value::Reference(Reference::Null))?;
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
        ("java/lang/Thread", "getName", "()Ljava/lang/String;") => {
            let thread_ref = args[0].as_reference()?;
            let name = vm.get_object_field(thread_ref, "name")?;
            Ok(Some(name))
        }
        ("java/lang/Thread", "setName", "(Ljava/lang/String;)V") => {
            let thread_ref = args[0].as_reference()?;
            let name = args[1].clone();
            vm.set_object_field(thread_ref, "name", name)?;
            Ok(None)
        }
        ("java/lang/Thread", "getPriority", "()I") => {
            let thread_ref = args[0].as_reference()?;
            let priority = vm.get_object_field(thread_ref, "priority")?;
            Ok(Some(priority))
        }
        ("java/lang/Thread", "setPriority", "(I)V") => {
            let thread_ref = args[0].as_reference()?;
            let priority = args[1].clone();
            vm.set_object_field(thread_ref, "priority", priority)?;
            Ok(None)
        }
        ("java/lang/Thread", "isDaemon", "()Z") => {
            let thread_ref = args[0].as_reference()?;
            let daemon = vm.get_object_field(thread_ref, "daemon")?;
            Ok(Some(daemon))
        }
        ("java/lang/Thread", "setDaemon", "(Z)V") => {
            let thread_ref = args[0].as_reference()?;
            let daemon = args[1].clone();
            vm.set_object_field(thread_ref, "daemon", daemon)?;
            Ok(None)
        }
        ("java/lang/Thread", "getId", "()J") => {
            let thread_ref = args[0].as_reference()?;
            let index = match thread_ref {
                Reference::Heap(idx) => idx as i64,
                Reference::Null => return Err(VmError::NullReference),
            };
            Ok(Some(Value::Long(index)))
        }
        ("java/lang/Thread", "isAlive", "()Z") => {
            let thread_ref = args[0].as_reference()?;
            let index = match thread_ref {
                Reference::Heap(idx) => idx,
                Reference::Null => return Err(VmError::NullReference),
            };
            let started = vm.threads.states.lock().unwrap()
                .get(&index)
                .map(|s| s.started)
                .unwrap_or(false);
            Ok(Some(Value::Int(i32::from(started))))
        }
        ("java/lang/Thread", "isInterrupted", "()Z") => {
            let thread_ref = args[0].as_reference()?;
            let index = match thread_ref {
                Reference::Heap(idx) => idx,
                Reference::Null => return Err(VmError::NullReference),
            };
            let interrupted = vm.threads.states.lock().unwrap()
                .get(&index)
                .map(|s| s.interrupted)
                .unwrap_or(false);
            Ok(Some(Value::Int(i32::from(interrupted))))
        }
        ("java/lang/Thread", "interrupt", "()V") => {
            let thread_ref = args[0].as_reference()?;
            let index = match thread_ref {
                Reference::Heap(idx) => idx,
                Reference::Null => return Err(VmError::NullReference),
            };
            vm.threads.states.lock().unwrap()
                .get_mut(&index)
                .map(|s| s.interrupted = true);
            Ok(None)
        }
        ("java/lang/Thread", "getThreadGroup", "()Ljava/lang/ThreadGroup;") => {
            let thread_ref = args[0].as_reference()?;
            let group = vm.get_object_field(thread_ref, "threadGroup")?;
            Ok(Some(group))
        }
        ("java/lang/Thread", "getContextClassLoader", "()Ljava/lang/ClassLoader;") => {
            let thread_ref = args[0].as_reference()?;
            let ccl = vm.get_object_field(thread_ref, "contextClassLoader")?;
            Ok(Some(ccl))
        }
        ("java/lang/Thread", "setContextClassLoader", "(Ljava/lang/ClassLoader;)V") => {
            let thread_ref = args[0].as_reference()?;
            let ccl = args[1].clone();
            vm.set_object_field(thread_ref, "contextClassLoader", ccl)?;
            Ok(None)
        }
        ("java/lang/Thread", "getUncaughtExceptionHandler", "()Ljava/lang/Thread$UncaughtExceptionHandler;") => {
            let thread_ref = args[0].as_reference()?;
            let handler = vm.get_object_field(thread_ref, "uncaughtExceptionHandler")?;
            Ok(Some(handler))
        }
        ("java/lang/Thread", "setUncaughtExceptionHandler", "(Ljava/lang/Thread$UncaughtExceptionHandler;)V") => {
            let thread_ref = args[0].as_reference()?;
            let handler = args[1].clone();
            vm.set_object_field(thread_ref, "uncaughtExceptionHandler", handler)?;
            Ok(None)
        }
        ("java/lang/Thread", _, _) => {
            let _ = stub_return_value_tracked(class_name, method_name, descriptor);
            Ok(None)
        }
        ("java/lang/ThreadGroup", _, _) => {
            let _ = stub_return_value_tracked(class_name, method_name, descriptor);
            Ok(None)
        }
        ("java/lang/Class", "desiredAssertionStatus", "()Z") => Ok(Some(Value::Int(0))),
        ("java/lang/Class", "isArray", "()Z") => {
            let name =
                crate::vm::builtin::helpers::class_internal_name(vm, args[0].as_reference()?)?;
            Ok(Some(Value::Int(i32::from(name.starts_with('[')))))
        }
        ("java/lang/Class", "isPrimitive", "()Z") => {
            let name =
                crate::vm::builtin::helpers::class_internal_name(vm, args[0].as_reference()?)?;
            let primitive = matches!(
                name.as_str(),
                "boolean"
                    | "byte"
                    | "char"
                    | "short"
                    | "int"
                    | "long"
                    | "float"
                    | "double"
                    | "void"
            );
            Ok(Some(Value::Int(i32::from(primitive))))
        }
        ("java/lang/Class", "isInterface", "()Z") => Ok(Some(Value::Int(0))),
        ("java/lang/Class", "getName", "()Ljava/lang/String;")
        | ("java/lang/Class", "toString", "()Ljava/lang/String;") => {
            let internal =
                crate::vm::builtin::helpers::class_internal_name(vm, args[0].as_reference()?)?;
            let dotted = internal.replace('/', ".");
            Ok(Some(vm.new_string(dotted)))
        }
        ("java/lang/Class", "getSimpleName", "()Ljava/lang/String;") => {
            let internal =
                crate::vm::builtin::helpers::class_internal_name(vm, args[0].as_reference()?)?;
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
        | ("java/lang/Runtime", "maxMemory", "()J") => Ok(Some(Value::Long(256 * 1024 * 1024))),
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
        // --- FileDescriptor ---
        ("java/io/FileDescriptor", "valid", "()Z") => {
            let obj_ref = args[0].as_reference()?;
            let rid = fd_resource_id(vm, obj_ref)?;
            Ok(Some(Value::Int(if vm.io_resources.is_open(rid) {
                1
            } else {
                0
            })))
        }
        ("java/io/FileDescriptor", "sync", "()V") => {
            let obj_ref = args[0].as_reference()?;
            let rid = fd_resource_id(vm, obj_ref)?;
            let _ = vm.io_resources.with_file(rid, |file| {
                file.sync_all().map_err(|e| VmError::UnhandledException {
                    class_name: io_exception_for_error(&e).to_string(),
                })
            });
            Ok(None)
        }
        // --- FileInputStream ---
        ("java/io/FileInputStream", "<init>", "(Ljava/io/File;)V") => {
            let obj_ref = args[0].as_reference()?;
            let file_ref = args[1].as_reference()?;
            let path = file_path_string(vm, file_ref)?;
            if let Some(path) = path {
                let fd_ref = new_file_descriptor(vm, 0);
                {
                    let mut heap = vm.heap.lock().unwrap();
                    if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                        fields[0] = Value::Reference(fd_ref);
                    }
                }
                open_file_stream(vm, obj_ref, &path, 0)?;
            }
            Ok(None)
        }
        ("java/io/FileInputStream", "<init>", "(Ljava/io/FileDescriptor;)V") => {
            let obj_ref = args[0].as_reference()?;
            let fd_ref = args[1].as_reference()?;
            let mut heap = vm.heap.lock().unwrap();
            if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                fields[0] = Value::Reference(fd_ref);
            }
            Ok(None)
        }
        ("java/io/FileInputStream", "open0", "(Ljava/lang/String;)V") => {
            let obj_ref = args[0].as_reference()?;
            let path_ref = args[1].as_reference()?;
            let path = crate::vm::builtin::helpers::stringify_reference(vm, path_ref)?;
            open_file_stream(vm, obj_ref, &path, 0)?;
            Ok(None)
        }
        ("java/io/FileInputStream", "read0", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let fd_ref = fis_get_fd_ref(vm, obj_ref)?;
            let rid = fd_resource_id(vm, fd_ref)?;
            let byte = vm.io_resources.with_file(rid, |file| {
                let mut buf = [0u8; 1];
                match file.read(&mut buf) {
                    Ok(0) => Ok(-1),
                    Ok(_) => Ok(buf[0] as i32),
                    Err(e) => Err(VmError::UnhandledException {
                        class_name: io_exception_for_error(&e).to_string(),
                    }),
                }
            })?;
            Ok(Some(Value::Int(byte)))
        }
        ("java/io/FileInputStream", "readBytes", "([BII)I") => {
            let obj_ref = args[0].as_reference()?;
            let buf_ref = args[1].as_reference()?;
            let off = args[2].as_int()? as usize;
            let len = args[3].as_int()? as usize;
            let fd_ref = fis_get_fd_ref(vm, obj_ref)?;
            let rid = fd_resource_id(vm, fd_ref)?;
            if len == 0 {
                return Ok(Some(Value::Int(0)));
            }
            let total = vm.io_resources.with_file(rid, |file| {
                let mut buffer = vec![0u8; len];
                match file.read(&mut buffer) {
                    Ok(0) => Ok(-1i32),
                    Ok(n) => {
                        let mut heap = vm.heap.lock().unwrap();
                        if let HeapValue::IntArray { values } = heap.get_mut(buf_ref)? {
                            let end = off + n;
                            if end <= values.len() {
                                for (slot, byte) in values[off..end].iter_mut().zip(&buffer[..n]) {
                                    *slot = i32::from(*byte);
                                }
                            }
                        }
                        Ok(n as i32)
                    }
                    Err(e) => Err(VmError::UnhandledException {
                        class_name: io_exception_for_error(&e).to_string(),
                    }),
                }
            })?;
            Ok(Some(Value::Int(total)))
        }
        ("java/io/FileInputStream", "available0", "()J") => {
            let obj_ref = args[0].as_reference()?;
            let fd_ref = fis_get_fd_ref(vm, obj_ref)?;
            let rid = fd_resource_id(vm, fd_ref)?;
            let avail = vm.io_resources.with_file(rid, |file| {
                let metadata = file.metadata().map_err(|e| VmError::UnhandledException {
                    class_name: io_exception_for_error(&e).to_string(),
                })?;
                let current = file
                    .stream_position()
                    .map_err(|e| VmError::UnhandledException {
                        class_name: io_exception_for_error(&e).to_string(),
                    })?;
                Ok(metadata.len().saturating_sub(current) as i64)
            })?;
            Ok(Some(Value::Long(avail)))
        }
        ("java/io/FileInputStream", "skip0", "(J)J") => {
            let obj_ref = args[0].as_reference()?;
            let n = args[1].as_long()?;
            let fd_ref = fis_get_fd_ref(vm, obj_ref)?;
            let rid = fd_resource_id(vm, fd_ref)?;
            let skipped = vm.io_resources.with_file(rid, |file| {
                let before = file
                    .stream_position()
                    .map_err(|e| VmError::UnhandledException {
                        class_name: io_exception_for_error(&e).to_string(),
                    })?;
                file.seek(SeekFrom::Current(n))
                    .map_err(|e| VmError::UnhandledException {
                        class_name: io_exception_for_error(&e).to_string(),
                    })?;
                let after = file
                    .stream_position()
                    .map_err(|e| VmError::UnhandledException {
                        class_name: io_exception_for_error(&e).to_string(),
                    })?;
                Ok((after - before) as i64)
            })?;
            Ok(Some(Value::Long(skipped)))
        }
        ("java/io/FileInputStream", "close0", "()V") => {
            let obj_ref = args[0].as_reference()?;
            let fd_ref = fis_get_fd_ref(vm, obj_ref)?;
            let rid = fd_resource_id(vm, fd_ref)?;
            fd_set_resource_id(vm, fd_ref, 0)?;
            vm.io_resources.close(rid);
            Ok(None)
        }
        // --- FileOutputStream ---
        ("java/io/FileOutputStream", "<init>", "(Ljava/io/File;Z)V") => {
            let obj_ref = args[0].as_reference()?;
            let file_ref = args[1].as_reference()?;
            let append = args[2].as_int()? != 0;
            let path = file_path_string(vm, file_ref)?;
            if let Some(path) = path {
                let fd_ref = new_file_descriptor(vm, 0);
                {
                    let mut heap = vm.heap.lock().unwrap();
                    if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                        fields[0] = Value::Reference(fd_ref);
                    }
                }
                let mode: u8 = if append { 3 } else { 2 };
                open_file_stream(vm, obj_ref, &path, mode)?;
            }
            Ok(None)
        }
        ("java/io/FileOutputStream", "<init>", "(Ljava/io/FileDescriptor;)V") => {
            let obj_ref = args[0].as_reference()?;
            let fd_ref = args[1].as_reference()?;
            let mut heap = vm.heap.lock().unwrap();
            if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                fields[0] = Value::Reference(fd_ref);
            }
            Ok(None)
        }
        ("java/io/FileOutputStream", "open0", "(Ljava/lang/String;Z)V") => {
            let obj_ref = args[0].as_reference()?;
            let path_ref = args[1].as_reference()?;
            let append = args[2].as_int()? != 0;
            let path = crate::vm::builtin::helpers::stringify_reference(vm, path_ref)?;
            let mode: u8 = if append { 3 } else { 2 };
            open_file_stream(vm, obj_ref, &path, mode)?;
            Ok(None)
        }
        ("java/io/FileOutputStream", "write", "(IZ)V") => {
            let obj_ref = args[0].as_reference()?;
            let byte = args[1].as_int()? as u8;
            let append = args[2].as_int()? != 0;
            let fd_ref = fis_get_fd_ref(vm, obj_ref)?;
            let rid = fd_resource_id(vm, fd_ref)?;
            vm.io_resources.with_file(rid, |file| {
                if append {
                    file.seek(SeekFrom::End(0))
                        .map_err(|e| VmError::UnhandledException {
                            class_name: io_exception_for_error(&e).to_string(),
                        })?;
                }
                file.write_all(&[byte])
                    .map_err(|e| VmError::UnhandledException {
                        class_name: io_exception_for_error(&e).to_string(),
                    })
            })?;
            Ok(None)
        }
        ("java/io/FileOutputStream", "writeBytes", "([BIIZ)V") => {
            let obj_ref = args[0].as_reference()?;
            let buf_ref = args[1].as_reference()?;
            let off = args[2].as_int()? as usize;
            let len = args[3].as_int()? as usize;
            let fd_ref = fis_get_fd_ref(vm, obj_ref)?;
            let rid = fd_resource_id(vm, fd_ref)?;
            if len == 0 {
                return Ok(None);
            }
            vm.io_resources.with_file(rid, |file| {
                let heap = vm.heap.lock().unwrap();
                let data = match heap.get(buf_ref)? {
                    HeapValue::IntArray { values } => {
                        let end = (off + len).min(values.len());
                        values[off..end]
                            .iter()
                            .map(|&i| i as u8)
                            .collect::<Vec<u8>>()
                    }
                    _ => vec![],
                };
                drop(heap);
                file.write_all(&data)
                    .map_err(|e| VmError::UnhandledException {
                        class_name: io_exception_for_error(&e).to_string(),
                    })
            })?;
            Ok(None)
        }
        ("java/io/FileOutputStream", "close0", "()V") => {
            let obj_ref = args[0].as_reference()?;
            let fd_ref = fis_get_fd_ref(vm, obj_ref)?;
            let rid = fd_resource_id(vm, fd_ref)?;
            fd_set_resource_id(vm, fd_ref, 0)?;
            vm.io_resources.close(rid);
            Ok(None)
        }
        // --- RandomAccessFile ---
        ("java/io/RandomAccessFile", "<init>", "(Ljava/io/File;Ljava/lang/String;)V") => {
            let obj_ref = args[0].as_reference()?;
            let file_ref = args[1].as_reference()?;
            let mode_str =
                crate::vm::builtin::helpers::stringify_reference(vm, args[2].as_reference()?)?;
            let path = file_path_string(vm, file_ref)?;
            if let Some(path) = path {
                let rw = mode_str.contains('w');
                raf_set_rw(vm, obj_ref, rw)?;
                let fd_ref = new_file_descriptor(vm, 0);
                let path_value = vm.new_string(path.clone());
                {
                    let mut heap = vm.heap.lock().unwrap();
                    if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                        fields[0] = Value::Reference(fd_ref);
                        fields[1] = path_value;
                    }
                }
                let mode: u8 = if rw { 1 } else { 0 };
                open_file_stream(vm, obj_ref, &path, mode)?;
            }
            Ok(None)
        }
        ("java/io/RandomAccessFile", "open0", "(Ljava/lang/String;I)V") => {
            let obj_ref = args[0].as_reference()?;
            let path_ref = args[1].as_reference()?;
            let mode = args[2].as_int()? as u8;
            let path = crate::vm::builtin::helpers::stringify_reference(vm, path_ref)?;
            let rw = mode != 0;
            raf_set_rw(vm, obj_ref, rw)?;
            open_file_stream(vm, obj_ref, &path, mode)?;
            Ok(None)
        }
        ("java/io/RandomAccessFile", "read0", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let fd_ref = fis_get_fd_ref(vm, obj_ref)?;
            let rid = fd_resource_id(vm, fd_ref)?;
            let byte = vm.io_resources.with_file(rid, |file| {
                let mut buf = [0u8; 1];
                match file.read(&mut buf) {
                    Ok(0) => Ok(-1),
                    Ok(_) => Ok(buf[0] as i32),
                    Err(e) => Err(VmError::UnhandledException {
                        class_name: io_exception_for_error(&e).to_string(),
                    }),
                }
            })?;
            Ok(Some(Value::Int(byte)))
        }
        ("java/io/RandomAccessFile", "readBytes", "([BII)I") => {
            let obj_ref = args[0].as_reference()?;
            let buf_ref = args[1].as_reference()?;
            let off = args[2].as_int()? as usize;
            let len = args[3].as_int()? as usize;
            let fd_ref = fis_get_fd_ref(vm, obj_ref)?;
            let rid = fd_resource_id(vm, fd_ref)?;
            if len == 0 {
                return Ok(Some(Value::Int(0)));
            }
            let total = vm.io_resources.with_file(rid, |file| {
                let mut buffer = vec![0u8; len];
                match file.read(&mut buffer) {
                    Ok(0) => Ok(-1i32),
                    Ok(n) => {
                        let mut heap = vm.heap.lock().unwrap();
                        if let HeapValue::IntArray { values } = heap.get_mut(buf_ref)? {
                            let end = off + n;
                            if end <= values.len() {
                                for (slot, byte) in values[off..end].iter_mut().zip(&buffer[..n]) {
                                    *slot = i32::from(*byte);
                                }
                            }
                        }
                        Ok(n as i32)
                    }
                    Err(e) => Err(VmError::UnhandledException {
                        class_name: io_exception_for_error(&e).to_string(),
                    }),
                }
            })?;
            Ok(Some(Value::Int(total)))
        }
        ("java/io/RandomAccessFile", "write0", "(I)V") => {
            let obj_ref = args[0].as_reference()?;
            let byte = args[1].as_int()? as u8;
            let fd_ref = fis_get_fd_ref(vm, obj_ref)?;
            let rid = fd_resource_id(vm, fd_ref)?;
            vm.io_resources.with_file(rid, |file| {
                file.write_all(&[byte])
                    .map_err(|e| VmError::UnhandledException {
                        class_name: io_exception_for_error(&e).to_string(),
                    })
            })?;
            Ok(None)
        }
        ("java/io/RandomAccessFile", "writeBytes", "([BII)V") => {
            let obj_ref = args[0].as_reference()?;
            let buf_ref = args[1].as_reference()?;
            let off = args[2].as_int()? as usize;
            let len = args[3].as_int()? as usize;
            let fd_ref = fis_get_fd_ref(vm, obj_ref)?;
            let rid = fd_resource_id(vm, fd_ref)?;
            if len == 0 {
                return Ok(None);
            }
            vm.io_resources.with_file(rid, |file| {
                let heap = vm.heap.lock().unwrap();
                let data = match heap.get(buf_ref)? {
                    HeapValue::IntArray { values } => {
                        let end = (off + len).min(values.len());
                        values[off..end]
                            .iter()
                            .map(|&i| i as u8)
                            .collect::<Vec<u8>>()
                    }
                    _ => vec![],
                };
                drop(heap);
                file.write_all(&data)
                    .map_err(|e| VmError::UnhandledException {
                        class_name: io_exception_for_error(&e).to_string(),
                    })
            })?;
            Ok(None)
        }
        ("java/io/RandomAccessFile", "seek0", "(J)V") => {
            let obj_ref = args[0].as_reference()?;
            let pos = args[1].as_long()? as u64;
            let fd_ref = fis_get_fd_ref(vm, obj_ref)?;
            let rid = fd_resource_id(vm, fd_ref)?;
            vm.io_resources.with_file(rid, |file| {
                file.seek(SeekFrom::Start(pos))
                    .map_err(|e| VmError::UnhandledException {
                        class_name: io_exception_for_error(&e).to_string(),
                    })?;
                Ok(())
            })?;
            Ok(None)
        }
        ("java/io/RandomAccessFile", "length", "()J") => {
            let obj_ref = args[0].as_reference()?;
            let fd_ref = fis_get_fd_ref(vm, obj_ref)?;
            let rid = fd_resource_id(vm, fd_ref)?;
            let len = vm.io_resources.with_file(rid, |file| {
                Ok(file
                    .metadata()
                    .map_err(|e| VmError::UnhandledException {
                        class_name: io_exception_for_error(&e).to_string(),
                    })?
                    .len() as i64)
            })?;
            Ok(Some(Value::Long(len)))
        }
        ("java/io/RandomAccessFile", "close0", "()V") => {
            let obj_ref = args[0].as_reference()?;
            let fd_ref = fis_get_fd_ref(vm, obj_ref)?;
            let rid = fd_resource_id(vm, fd_ref)?;
            fd_set_resource_id(vm, fd_ref, 0)?;
            vm.io_resources.close(rid);
            Ok(None)
        }
        // --- ByteArrayOutputStream native impl ---
        ("java/io/ByteArrayOutputStream", "<init>", "()V") => {
            let obj_ref = args[0].as_reference()?;
            let buf = vm.heap.lock().unwrap().allocate(HeapValue::IntArray {
                values: vec![0; 32],
            });
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields[0] = Value::Reference(buf);
                    fields[1] = Value::Int(0);
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
                        let buf_ref = match fields.get(0) {
                            Some(Value::Reference(r)) => Some(*r),
                            _ => None,
                        };
                        let count = match fields.get(1) {
                            Some(Value::Int(i)) => Some(*i),
                            _ => None,
                        };
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
                        fields[1] = Value::Int(current_count + 1);
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
            let len = if args.len() > 3 {
                args[3].as_int()?
            } else {
                src_count
            };
            let offset = offset.max(0);
            let len = len.max(0).min(src_count.saturating_sub(offset));
            let (target_buf, current_count) = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let target_buf = match fields.get(0) {
                            Some(Value::Reference(r)) => Some(*r),
                            _ => None,
                        };
                        let count = match fields.get(1) {
                            Some(Value::Int(i)) => Some(*i),
                            _ => None,
                        };
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
                        fields[1] = Value::Int(current_count + len);
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
                        let buf_ref = match fields.get(0) {
                            Some(Value::Reference(r)) => Some(*r),
                            _ => None,
                        };
                        let count = match fields.get(1) {
                            Some(Value::Int(i)) => Some(*i),
                            _ => None,
                        };
                        (buf_ref, count)
                    }
                    _ => (None, None),
                }
            };
            if let (Some(buf_ref), Some(count)) = (buf_ref, count) {
                let chars: String = {
                    let heap = vm.heap.lock().unwrap();
                    match heap.get(buf_ref)? {
                        HeapValue::IntArray { values } => values
                            .iter()
                            .take(count as usize)
                            .map(|&v| v as u8 as char)
                            .collect(),
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
                        let buf_ref = match fields.get(0) {
                            Some(Value::Reference(r)) => Some(*r),
                            _ => None,
                        };
                        let count = match fields.get(1) {
                            Some(Value::Int(i)) => Some(*i),
                            _ => None,
                        };
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
                let arr_ref = vm
                    .heap
                    .lock()
                    .unwrap()
                    .allocate(HeapValue::IntArray { values: bytes });
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
                    HeapValue::Object { fields, .. } => match fields.get(1) {
                        Some(Value::Int(i)) => Some(*i),
                        _ => None,
                    },
                    _ => None,
                }
            };
            Ok(Some(count.map(Value::Int).unwrap_or(Value::Int(0))))
        }
        ("java/io/ByteArrayOutputStream", "reset", "()V") => {
            let obj_ref = args[0].as_reference()?;
            let mut heap = vm.heap.lock().unwrap();
            if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                fields[1] = Value::Int(0);
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
        // --- BufferedWriter ---
        ("java/io/BufferedWriter", "<init>", "(Ljava/io/Writer;)V") => {
            let obj_ref = args[0].as_reference()?;
            let writer_ref = args[1].as_reference()?;
            let mut heap = vm.heap.lock().unwrap();
            if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                fields[0] = Value::Reference(writer_ref);
            }
            Ok(None)
        }
        ("java/io/BufferedWriter", "write", "(I)V") => {
            let obj_ref = args[0].as_reference()?;
            let byte = args[1].as_int()? as u8;
            invoke_nio::write_byte_to_writer(vm, obj_ref, byte)?;
            Ok(None)
        }
        ("java/io/BufferedWriter", "write", "([C)V") => {
            let obj_ref = args[0].as_reference()?;
            let buf_ref = args[1].as_reference()?;
            let len = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(buf_ref)? {
                    HeapValue::IntArray { values } => values.len(),
                    _ => 0,
                }
            };
            invoke_nio::write_chars_to_writer(vm, obj_ref, buf_ref, 0, len as i32)?;
            Ok(None)
        }
        ("java/io/BufferedWriter", "write", "([CII)V") => {
            let obj_ref = args[0].as_reference()?;
            let buf_ref = args[1].as_reference()?;
            let off = args[2].as_int()?;
            let len = args[3].as_int()?;
            invoke_nio::write_chars_to_writer(vm, obj_ref, buf_ref, off, len)?;
            Ok(None)
        }
        ("java/io/BufferedWriter", "flush", "()V") => {
            invoke_nio::flush_writer(vm, args[0].as_reference()?)?;
            Ok(None)
        }
        ("java/io/BufferedWriter", "close", "()V") => {
            invoke_nio::close_writer(vm, args[0].as_reference()?)?;
            Ok(None)
        }
        // --- PrintWriter println/print ---
        ("java/io/PrintWriter", "println", "()V") => {
            println!("");
            vm.output.lock().unwrap().push(String::new());
            Ok(None)
        }
        ("java/io/PrintWriter", "println", "(Z)V") => {
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
            let text = if args[1].as_int()? != 0 {
                "true"
            } else {
                "false"
            }
            .to_string();
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
        // --- BufferedReader ---
        ("java/io/BufferedReader", "<init>", "(Ljava/io/Reader;)V") => {
            let obj_ref = args[0].as_reference()?;
            let reader_ref = args[1].as_reference()?;
            let mut heap = vm.heap.lock().unwrap();
            if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                fields[0] = Value::Reference(reader_ref);
            }
            Ok(None)
        }
        ("java/io/BufferedReader", "read", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let reader_ref = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => match fields.first() {
                        Some(Value::Reference(r)) => *r,
                        _ => Reference::Null,
                    },
                    _ => Reference::Null,
                }
            };
            if reader_ref == Reference::Null {
                return Ok(Some(Value::Int(-1)));
            }
            let c = invoke_nio::read_char_from_reader(vm, reader_ref)?;
            Ok(Some(Value::Int(c)))
        }
        ("java/io/BufferedReader", "read", "(I)I") => {
            let obj_ref = args[0].as_reference()?;
            let _ = args[1].as_int()?;
            let reader_ref = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => match fields.first() {
                        Some(Value::Reference(r)) => *r,
                        _ => Reference::Null,
                    },
                    _ => Reference::Null,
                }
            };
            if reader_ref == Reference::Null {
                return Ok(Some(Value::Int(-1)));
            }
            let c = invoke_nio::read_char_from_reader(vm, reader_ref)?;
            Ok(Some(Value::Int(c)))
        }
        ("java/io/BufferedReader", "read", "([C)I") => {
            let obj_ref = args[0].as_reference()?;
            let buf_ref = args[1].as_reference()?;
            let reader_ref = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => match fields.first() {
                        Some(Value::Reference(r)) => *r,
                        _ => Reference::Null,
                    },
                    _ => Reference::Null,
                }
            };
            if reader_ref == Reference::Null {
                return Ok(Some(Value::Int(-1)));
            }
            let len = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(buf_ref)? {
                    HeapValue::IntArray { values } => values.len() as i32,
                    _ => 0,
                }
            };
            let n = invoke_nio::read_into_char_array_from_reader(vm, reader_ref, buf_ref, 0, len)?;
            Ok(Some(Value::Int(n)))
        }
        ("java/io/BufferedReader", "read", "([CII)I") => {
            let obj_ref = args[0].as_reference()?;
            let buf_ref = args[1].as_reference()?;
            let off = args[2].as_int()? as usize;
            let len = args[3].as_int()? as usize;
            let reader_ref = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => match fields.first() {
                        Some(Value::Reference(r)) => *r,
                        _ => Reference::Null,
                    },
                    _ => Reference::Null,
                }
            };
            if reader_ref == Reference::Null {
                return Ok(Some(Value::Int(-1)));
            }
            let n = invoke_nio::read_into_char_array_from_reader(
                vm, reader_ref, buf_ref, off as i32, len as i32,
            )?;
            Ok(Some(Value::Int(n)))
        }
        ("java/io/BufferedReader", "skip", "(J)J") => Ok(Some(Value::Long(0))),
        ("java/io/BufferedReader", "ready", "()Z") => {
            let obj_ref = args[0].as_reference()?;
            let is_ref = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object {
                        class_name, fields, ..
                    } => {
                        if class_name == "java/io/BufferedReader" {
                            let reader_ref = match fields.first() {
                                Some(Value::Reference(r)) => *r,
                                _ => Reference::Null,
                            };
                            if reader_ref != Reference::Null {
                                match heap.get(reader_ref)? {
                                    HeapValue::Object { fields, .. } => {
                                        fields.first().and_then(|v| match v {
                                            Value::Reference(r) => Some(*r),
                                            _ => None,
                                        })
                                    }
                                    _ => None,
                                }
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
            let is_ref = match is_ref {
                Some(r) if r != Reference::Null => r,
                _ => return Ok(Some(Value::Int(0))),
            };
            let avail = invoke_nio::get_input_stream_available(vm, is_ref)?;
            Ok(Some(Value::Int(if avail > 0 { 1 } else { 0 })))
        }
        ("java/io/BufferedReader", "close", "()V") => Ok(None),
        ("java/io/BufferedReader", "readLine", "()Ljava/lang/String;") => {
            let obj_ref = args[0].as_reference()?;
            let reader_ref = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => match fields.first() {
                        Some(Value::Reference(r)) => *r,
                        _ => Reference::Null,
                    },
                    _ => Reference::Null,
                }
            };
            if reader_ref == Reference::Null {
                return Ok(Some(Value::Reference(Reference::Null)));
            }
            let mut line = String::new();
            loop {
                let c = invoke_nio::read_char_from_reader(vm, reader_ref)?;
                if c < 0 {
                    if line.is_empty() {
                        return Ok(Some(Value::Reference(Reference::Null)));
                    }
                    return Ok(Some(vm.new_string(line)));
                }
                let ch = c as u8 as char;
                if ch == '\n' {
                    return Ok(Some(vm.new_string(line)));
                }
                if ch != '\r' {
                    line.push(ch);
                }
            }
        }
        // --- InputStreamReader ---
        ("java/io/InputStreamReader", "<init>", "(Ljava/io/InputStream;)V") => {
            let obj_ref = args[0].as_reference()?;
            let is_ref = args[1].as_reference()?;
            let mut heap = vm.heap.lock().unwrap();
            if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                fields[0] = Value::Reference(is_ref);
            }
            Ok(None)
        }
        ("java/io/InputStreamReader", "read", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let is_ref = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => match fields.first() {
                        Some(Value::Reference(r)) => *r,
                        _ => Reference::Null,
                    },
                    _ => Reference::Null,
                }
            };
            if is_ref == Reference::Null {
                return Ok(Some(Value::Int(-1)));
            }
            let byte = invoke_nio::read_byte_from_input_stream(vm, is_ref)?;
            if byte < 0 {
                Ok(Some(Value::Int(-1)))
            } else {
                Ok(Some(Value::Int(byte as i32)))
            }
        }
        ("java/io/InputStreamReader", "read", "(I)I") => {
            let obj_ref = args[0].as_reference()?;
            let _ = args[1].as_int()?;
            let is_ref = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => match fields.first() {
                        Some(Value::Reference(r)) => *r,
                        _ => Reference::Null,
                    },
                    _ => Reference::Null,
                }
            };
            if is_ref == Reference::Null {
                return Ok(Some(Value::Int(-1)));
            }
            let byte = invoke_nio::read_byte_from_input_stream(vm, is_ref)?;
            if byte < 0 {
                Ok(Some(Value::Int(-1)))
            } else {
                Ok(Some(Value::Int(byte as i32)))
            }
        }
        ("java/io/InputStreamReader", "read", "([C)I") => {
            let obj_ref = args[0].as_reference()?;
            let buf_ref = args[1].as_reference()?;
            let is_ref = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => match fields.first() {
                        Some(Value::Reference(r)) => *r,
                        _ => Reference::Null,
                    },
                    _ => Reference::Null,
                }
            };
            if is_ref == Reference::Null {
                return Ok(Some(Value::Int(-1)));
            }
            let n = invoke_nio::read_into_byte_array_from_input_stream(vm, is_ref, buf_ref, 0, {
                let heap = vm.heap.lock().unwrap();
                match heap.get(buf_ref)? {
                    HeapValue::IntArray { values } => values.len() as i32,
                    _ => 0,
                }
            })?;
            Ok(Some(Value::Int(n)))
        }
        ("java/io/InputStreamReader", "read", "([CII)I") => {
            let obj_ref = args[0].as_reference()?;
            let buf_ref = args[1].as_reference()?;
            let off = args[2].as_int()? as usize;
            let len = args[3].as_int()? as usize;
            let is_ref = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => match fields.first() {
                        Some(Value::Reference(r)) => *r,
                        _ => Reference::Null,
                    },
                    _ => Reference::Null,
                }
            };
            if is_ref == Reference::Null {
                return Ok(Some(Value::Int(-1)));
            }
            let n = invoke_nio::read_into_byte_array_from_input_stream(
                vm, is_ref, buf_ref, off as i32, len as i32,
            )?;
            Ok(Some(Value::Int(n)))
        }
        ("java/io/InputStreamReader", "close", "()V") => Ok(None),
        // --- OutputStreamWriter ---
        ("java/io/OutputStreamWriter", "<init>", "(Ljava/io/OutputStream;)V") => {
            let obj_ref = args[0].as_reference()?;
            let os_ref = args[1].as_reference()?;
            let mut heap = vm.heap.lock().unwrap();
            if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                fields[0] = Value::Reference(os_ref);
            }
            Ok(None)
        }
        ("java/io/OutputStreamWriter", "write", "(I)V") => {
            let obj_ref = args[0].as_reference()?;
            let byte = args[1].as_int()? as u8;
            invoke_nio::write_byte_to_writer(vm, obj_ref, byte)?;
            Ok(None)
        }
        ("java/io/OutputStreamWriter", "write", "([C)V") => {
            let obj_ref = args[0].as_reference()?;
            let buf_ref = args[1].as_reference()?;
            let len = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(buf_ref)? {
                    HeapValue::IntArray { values } => values.len(),
                    _ => 0,
                }
            };
            invoke_nio::write_chars_to_writer(vm, obj_ref, buf_ref, 0, len as i32)?;
            Ok(None)
        }
        ("java/io/OutputStreamWriter", "write", "([CII)V") => {
            let obj_ref = args[0].as_reference()?;
            let buf_ref = args[1].as_reference()?;
            let off = args[2].as_int()?;
            let len = args[3].as_int()?;
            invoke_nio::write_chars_to_writer(vm, obj_ref, buf_ref, off, len)?;
            Ok(None)
        }
        ("java/io/OutputStreamWriter", "write", "(Ljava/lang/String;)V") => {
            let obj_ref = args[0].as_reference()?;
            let str_ref = args[1].as_reference()?;
            let s = crate::vm::builtin::helpers::stringify_reference(vm, str_ref)?;
            invoke_nio::write_string_to_writer(vm, obj_ref, &s)?;
            Ok(None)
        }
        ("java/io/OutputStreamWriter", "write", "(Ljava/lang/String;II)V") => {
            let obj_ref = args[0].as_reference()?;
            let str_ref = args[1].as_reference()?;
            let off = args[2].as_int()? as usize;
            let len = args[3].as_int()? as usize;
            let s = crate::vm::builtin::helpers::stringify_reference(vm, str_ref)?;
            let slice: String = s.chars().skip(off).take(len).collect();
            invoke_nio::write_string_to_writer(vm, obj_ref, &slice)?;
            Ok(None)
        }
        ("java/io/OutputStreamWriter", "flush", "()V") => {
            invoke_nio::flush_writer(vm, args[0].as_reference()?)?;
            Ok(None)
        }
        ("java/io/OutputStreamWriter", "close", "()V") => {
            invoke_nio::close_writer(vm, args[0].as_reference()?)?;
            Ok(None)
        }
        // --- File ---
        ("java/io/File", "<init>", "(Ljava/lang/String;)V") => {
            let obj_ref = args[0].as_reference()?;
            let path_str = args[1].as_reference()?;
            if let Ok(HeapValue::Object { fields, .. }) = vm.heap.lock().unwrap().get_mut(obj_ref) {
                fields[0] = Value::Reference(path_str);
            }
            Ok(None)
        }
        ("java/io/File", "exists", "()Z") => {
            let path = file_path_string(vm, args[0].as_reference()?)?;
            Ok(Some(Value::Int(
                path.map(|path| PathBuf::from(path).exists() as i32)
                    .unwrap_or(0),
            )))
        }
        ("java/io/File", "isFile", "()Z") => {
            let path = file_path_string(vm, args[0].as_reference()?)?;
            Ok(Some(Value::Int(
                path.map(|path| PathBuf::from(path).is_file() as i32)
                    .unwrap_or(0),
            )))
        }
        ("java/io/File", "isDirectory", "()Z") => {
            let path = file_path_string(vm, args[0].as_reference()?)?;
            Ok(Some(Value::Int(
                path.map(|path| PathBuf::from(path).is_dir() as i32)
                    .unwrap_or(0),
            )))
        }
        ("java/io/File", "isHidden", "()Z") => {
            let path = file_path_string(vm, args[0].as_reference()?)?;
            let hidden = path
                .and_then(|path| {
                    PathBuf::from(path)
                        .file_name()
                        .map(|name| name.to_string_lossy().starts_with('.'))
                })
                .unwrap_or(false);
            Ok(Some(Value::Int(hidden as i32)))
        }
        ("java/io/File", "length", "()J") => {
            let path = file_path_string(vm, args[0].as_reference()?)?;
            let length = path
                .and_then(|path| {
                    fs::metadata(path)
                        .ok()
                        .map(|metadata| metadata.len() as i64)
                })
                .unwrap_or(0);
            Ok(Some(Value::Long(length)))
        }
        ("java/io/File", "getPath", "()Ljava/lang/String;") => {
            let obj_ref = args[0].as_reference()?;
            let path_ref = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => match fields.get(0) {
                        Some(Value::Reference(r)) => Some(*r),
                        _ => None,
                    },
                    _ => None,
                }
            };
            Ok(Some(
                path_ref
                    .map(Value::Reference)
                    .unwrap_or(Value::Reference(Reference::Null)),
            ))
        }
        ("java/io/File", "getName", "()Ljava/lang/String;") => {
            let obj_ref = args[0].as_reference()?;
            let name = file_path_string(vm, obj_ref)?;
            let name = name
                .and_then(|path| {
                    PathBuf::from(path)
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                })
                .unwrap_or_default();
            Ok(Some(vm.new_string(name)))
        }
        ("java/io/File", "getParent", "()Ljava/lang/String;") => {
            let obj_ref = args[0].as_reference()?;
            let parent = file_path_string(vm, obj_ref)?
                .and_then(|path| {
                    PathBuf::from(path)
                        .parent()
                        .filter(|parent| !parent.as_os_str().is_empty())
                        .map(|parent| parent.to_string_lossy().into_owned())
                })
                .map(|parent| vm.new_string(parent))
                .unwrap_or(Value::Reference(Reference::Null));
            Ok(Some(parent))
        }
        ("java/io/File", "canRead", "()Z") => {
            let path = file_path_string(vm, args[0].as_reference()?)?;
            let can_read = path.and_then(|path| fs::metadata(path).ok()).is_some();
            Ok(Some(Value::Int(can_read as i32)))
        }
        ("java/io/File", "canWrite", "()Z") => {
            let path = file_path_string(vm, args[0].as_reference()?)?;
            let can_write = path
                .and_then(|path| fs::metadata(path).ok())
                .map(|metadata| !metadata.permissions().readonly())
                .unwrap_or(false);
            Ok(Some(Value::Int(can_write as i32)))
        }
        ("java/io/File", "canExecute", "()Z") => {
            let path = file_path_string(vm, args[0].as_reference()?)?;
            #[cfg(unix)]
            let can_execute = path
                .and_then(|path| fs::metadata(path).ok())
                .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
                .unwrap_or(false);
            #[cfg(not(unix))]
            let can_execute = false;
            Ok(Some(Value::Int(can_execute as i32)))
        }
        ("java/io/File", "mkdir", "()Z") => {
            let path = file_path_string(vm, args[0].as_reference()?)?;
            let created = path
                .map(|path| fs::create_dir(PathBuf::from(path)).is_ok() as i32)
                .unwrap_or(0);
            Ok(Some(Value::Int(created)))
        }
        ("java/io/File", "createNewFile", "()Z") => {
            let path = file_path_string(vm, args[0].as_reference()?)?;
            let Some(path) = path else {
                return Ok(Some(Value::Int(0)));
            };
            let result = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(PathBuf::from(path));
            match result {
                Ok(_) => Ok(Some(Value::Int(1))),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    Ok(Some(Value::Int(0)))
                }
                Err(_) => Err(VmError::UnhandledException {
                    class_name: "java/io/IOException".to_string(),
                }),
            }
        }
        ("java/io/File", "delete", "()Z") => {
            let path = file_path_string(vm, args[0].as_reference()?)?;
            let deleted = match path {
                Some(path) => {
                    let path = PathBuf::from(path);
                    match fs::metadata(&path) {
                        Ok(metadata) if metadata.is_dir() => fs::remove_dir(&path).is_ok(),
                        Ok(_) => fs::remove_file(&path).is_ok(),
                        Err(_) => false,
                    }
                }
                None => false,
            };
            Ok(Some(Value::Int(deleted as i32)))
        }
        ("java/io/File", "list", "()[Ljava/lang/String;") => {
            let path = file_path_string(vm, args[0].as_reference()?)?;
            let Some(path) = path else {
                return Ok(Some(Value::Reference(Reference::Null)));
            };
            let entries = match fs::read_dir(PathBuf::from(path)) {
                Ok(entries) => entries,
                Err(_) => return Ok(Some(Value::Reference(Reference::Null))),
            };
            let mut names = Vec::new();
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    names.push(match vm.new_string(name.to_string()) {
                        Value::Reference(reference) => reference,
                        _ => Reference::Null,
                    });
                }
            }
            let array = vm
                .heap
                .lock()
                .unwrap()
                .allocate_reference_array("java/lang/String", names);
            Ok(Some(Value::Reference(array)))
        }
        ("java/io/File", "listFiles", "()[Ljava/io/File;") => {
            let path = file_path_string(vm, args[0].as_reference()?)?;
            let Some(path) = path else {
                return Ok(Some(Value::Reference(Reference::Null)));
            };
            let entries = match fs::read_dir(PathBuf::from(&path)) {
                Ok(entries) => entries,
                Err(_) => return Ok(Some(Value::Reference(Reference::Null))),
            };
            let mut files = Vec::new();
            for entry in entries.flatten() {
                files.push(new_file_object(
                    vm,
                    entry.path().to_string_lossy().into_owned(),
                ));
            }
            let array = vm
                .heap
                .lock()
                .unwrap()
                .allocate_reference_array("java/io/File", files);
            Ok(Some(Value::Reference(array)))
        }
        _ => Err(VmError::UnhandledException {
            class_name: "".to_string(),
        }),
    }
}
