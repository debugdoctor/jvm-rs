use crate::vm::{HeapValue, Reference, Value, Vm, VmError};

pub(super) fn invoke_reflect(
    vm: &mut Vm,
    class_name: &str,
    method_name: &str,
    descriptor: &str,
    args: &[Value],
) -> Result<Option<Value>, VmError> {
    match (class_name, method_name, descriptor) {
        ("java/lang/Class", "getDeclaredMethod", "(Ljava/lang/String;[Ljava/lang/Class;)Ljava/lang/reflect/Method;") => {
            let name_ref = args[0].as_reference()?;
            let _param_types_ref = args[1].as_reference()?;
            let this_ref = args[2].as_reference()?;
            let class_name_str = get_class_name(vm, this_ref)?;
            let method_name_str = crate::vm::builtin::helpers::stringify_reference(vm, name_ref)?;
            let mut fields = std::collections::HashMap::new();
            fields.insert("__declaring_class".to_string(), Value::Reference(this_ref));
            fields.insert("__name".to_string(), Value::Reference(name_ref));
            fields.insert("__descriptor".to_string(), Value::Reference(Reference::Null));
            fields.insert("__parameter_types".to_string(), Value::Reference(Reference::Null));
            fields.insert("__return_type".to_string(), Value::Reference(Reference::Null));
            fields.insert("__modifiers".to_string(), Value::Int(1));
            let heap = &mut vm.heap.lock().unwrap();
            let obj_ref = heap.allocate(HeapValue::Object {
                class_name: "java/lang/reflect/Method".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/lang/Class", "getDeclaredField", "(Ljava/lang/String;)Ljava/lang/reflect/Field;") => {
            let name_ref = args[0].as_reference()?;
            let this_ref = args[1].as_reference()?;
            let mut fields = std::collections::HashMap::new();
            fields.insert("__declaring_class".to_string(), Value::Reference(this_ref));
            fields.insert("__name".to_string(), Value::Reference(name_ref));
            fields.insert("__type".to_string(), Value::Reference(Reference::Null));
            fields.insert("__descriptor".to_string(), Value::Reference(Reference::Null));
            fields.insert("__modifiers".to_string(), Value::Int(1));
            fields.insert("__slot".to_string(), Value::Int(0));
            let heap = &mut vm.heap.lock().unwrap();
            let obj_ref = heap.allocate(HeapValue::Object {
                class_name: "java/lang/reflect/Field".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/lang/Class", "getDeclaredFields", "()[Ljava/lang/reflect/Field;") => {
            let heap = vm.heap.lock().unwrap();
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/lang/Class", "getDeclaredMethods", "()[Ljava/lang/reflect/Method;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/lang/Class", "getDeclaredConstructor", "([Ljava/lang/Class;)Ljava/lang/reflect/Constructor;") => {
            let this_ref = args.last().unwrap().as_reference()?;
            let mut fields = std::collections::HashMap::new();
            fields.insert("__declaring_class".to_string(), Value::Reference(this_ref));
            fields.insert("__parameter_types".to_string(), Value::Reference(Reference::Null));
            fields.insert("__modifiers".to_string(), Value::Int(1));
            fields.insert("__slot".to_string(), Value::Int(0));
            let heap = &mut vm.heap.lock().unwrap();
            let obj_ref = heap.allocate(HeapValue::Object {
                class_name: "java/lang/reflect/Constructor".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/lang/Class", "getDeclaredConstructors", "()[Ljava/lang/reflect/Constructor;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/lang/Class", "getSuperclass", "()Ljava/lang/Class;") => {
            let this_ref = args[0].as_reference()?;
            let class_name_str = get_class_name(vm, this_ref)?;
            let super_class = match class_name_str.as_str() {
                "java/lang/Object" => None,
                "java/lang/String" => Some("java/lang/Object".to_string()),
                _ => {
                    let rt = vm.get_class(&class_name_str).ok();
                    rt.and_then(|rc| rc.super_class.clone())
                }
            };
            match super_class {
                Some(ref sc) => Ok(Some(Value::Reference(vm.class_object(sc)))),
                None => Ok(Some(Value::Reference(Reference::Null))),
            }
        }
        ("java/lang/Class", "getInterfaces", "()[Ljava/lang/Class;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/lang/Class", "getModifiers", "()I") => {
            Ok(Some(Value::Int(1)))
        }
        ("java/lang/Class", "getComponentType", "()Ljava/lang/Class;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/lang/Class", "isHidden", "()Z") => {
            Ok(Some(Value::Int(0)))
        }
        ("java/lang/reflect/Method", "getName", "()Ljava/lang/String;") => {
            let this_ref = args[0].as_reference()?;
            let heap = vm.heap.lock().unwrap();
            if let Ok(HeapValue::Object { fields, .. }) = heap.get(this_ref) {
                if let Some(Value::Reference(r)) = fields.get("__name") {
                    return Ok(Some(Value::Reference(*r)));
                }
            }
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/lang/reflect/Method", "getDeclaringClass", "()Ljava/lang/Class;") => {
            let this_ref = args[0].as_reference()?;
            let heap = vm.heap.lock().unwrap();
            if let Ok(HeapValue::Object { fields, .. }) = heap.get(this_ref) {
                if let Some(Value::Reference(r)) = fields.get("__declaring_class") {
                    return Ok(Some(Value::Reference(*r)));
                }
            }
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/lang/reflect/Method", "getReturnType", "()Ljava/lang/Class;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/lang/reflect/Method", "getParameterTypes", "()[Ljava/lang/Class;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/lang/reflect/Method", "getModifiers", "()I") => {
            let this_ref = args[0].as_reference()?;
            let heap = vm.heap.lock().unwrap();
            if let Ok(HeapValue::Object { fields, .. }) = heap.get(this_ref) {
                if let Some(Value::Int(m)) = fields.get("__modifiers") {
                    return Ok(Some(Value::Int(*m)));
                }
            }
            Ok(Some(Value::Int(0)))
        }
        ("java/lang/reflect/Method", "invoke", "(Ljava/lang/Object;[Ljava/lang/Object;)Ljava/lang/Object;") => {
            let _obj_ref = args[0].as_reference()?;
            let _param_ref = args[1].as_reference()?;
            Err(VmError::UnhandledException {
                class_name: "java/lang/reflect/InvocationTargetException".to_string(),
            })
        }
        ("java/lang/reflect/Field", "getName", "()Ljava/lang/String;") => {
            let this_ref = args[0].as_reference()?;
            let heap = vm.heap.lock().unwrap();
            if let Ok(HeapValue::Object { fields, .. }) = heap.get(this_ref) {
                if let Some(Value::Reference(r)) = fields.get("__name") {
                    return Ok(Some(Value::Reference(*r)));
                }
            }
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/lang/reflect/Field", "getType", "()Ljava/lang/Class;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/lang/reflect/Field", "getDeclaringClass", "()Ljava/lang/Class;") => {
            let this_ref = args[0].as_reference()?;
            let heap = vm.heap.lock().unwrap();
            if let Ok(HeapValue::Object { fields, .. }) = heap.get(this_ref) {
                if let Some(Value::Reference(r)) = fields.get("__declaring_class") {
                    return Ok(Some(Value::Reference(*r)));
                }
            }
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/lang/reflect/Field", "getModifiers", "()I") => {
            let this_ref = args[0].as_reference()?;
            let heap = vm.heap.lock().unwrap();
            if let Ok(HeapValue::Object { fields, .. }) = heap.get(this_ref) {
                if let Some(Value::Int(m)) = fields.get("__modifiers") {
                    return Ok(Some(Value::Int(*m)));
                }
            }
            Ok(Some(Value::Int(0)))
        }
        ("java/lang/reflect/Field", "get", "(Ljava/lang/Object;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/lang/reflect/Field", "set", "(Ljava/lang/Object;Ljava/lang/Object;)V") => {
            Ok(None)
        }
        ("java/lang/reflect/Field", "getInt", "(Ljava/lang/Object;)I") => {
            Ok(Some(Value::Int(0)))
        }
        ("java/lang/reflect/Field", "setInt", "(Ljava/lang/Object;I)V") => {
            Ok(None)
        }
        ("java/lang/reflect/Field", "getLong", "(Ljava/lang/Object;)J") => {
            Ok(Some(Value::Long(0)))
        }
        ("java/lang/reflect/Field", "setLong", "(Ljava/lang/Object;J)V") => {
            Ok(None)
        }
        ("java/lang/reflect/Field", "getObject", "(Ljava/lang/Object;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/lang/reflect/Field", "setObject", "(Ljava/lang/Object;Ljava/lang/Object;)V") => {
            Ok(None)
        }
        ("java/lang/reflect/Constructor", "getParameterTypes", "()[Ljava/lang/Class;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/lang/reflect/Constructor", "getDeclaringClass", "()Ljava/lang/Class;") => {
            let this_ref = args[0].as_reference()?;
            let heap = vm.heap.lock().unwrap();
            if let Ok(HeapValue::Object { fields, .. }) = heap.get(this_ref) {
                if let Some(Value::Reference(r)) = fields.get("__declaring_class") {
                    return Ok(Some(Value::Reference(*r)));
                }
            }
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/lang/reflect/Constructor", "getModifiers", "()I") => {
            let this_ref = args[0].as_reference()?;
            let heap = vm.heap.lock().unwrap();
            if let Ok(HeapValue::Object { fields, .. }) = heap.get(this_ref) {
                if let Some(Value::Int(m)) = fields.get("__modifiers") {
                    return Ok(Some(Value::Int(*m)));
                }
            }
            Ok(Some(Value::Int(0)))
        }
        ("java/lang/reflect/Constructor", "newInstance", "([Ljava/lang/Object;)Ljava/lang/Object;") => {
            Err(VmError::UnhandledException {
                class_name: "java/lang/reflect/InvocationTargetException".to_string(),
            })
        }
        ("java/lang/reflect/AccessibleObject", "setAccessible", "(Z)V") => {
            Ok(None)
        }
        ("java/lang/reflect/AccessibleObject", "canAccess", "(Ljava/lang/Object;)Z") => {
            Ok(Some(Value::Int(1)))
        }
        ("java/lang/reflect/Modifier", "isPublic", "(I)Z") => {
            let m = args[0].as_int()?;
            Ok(Some(Value::Int(if m & 0x0001 != 0 { 1 } else { 0 })))
        }
        ("java/lang/reflect/Modifier", "isPrivate", "(I)Z") => {
            let m = args[0].as_int()?;
            Ok(Some(Value::Int(if m & 0x0002 != 0 { 1 } else { 0 })))
        }
        ("java/lang/reflect/Modifier", "isProtected", "(I)Z") => {
            let m = args[0].as_int()?;
            Ok(Some(Value::Int(if m & 0x0004 != 0 { 1 } else { 0 })))
        }
        ("java/lang/reflect/Modifier", "isStatic", "(I)Z") => {
            let m = args[0].as_int()?;
            Ok(Some(Value::Int(if m & 0x0008 != 0 { 1 } else { 0 })))
        }
        ("java/lang/reflect/Modifier", "isFinal", "(I)Z") => {
            let m = args[0].as_int()?;
            Ok(Some(Value::Int(if m & 0x0010 != 0 { 1 } else { 0 })))
        }
        ("java/lang/reflect/Modifier", "isSynchronized", "(I)Z") => {
            let m = args[0].as_int()?;
            Ok(Some(Value::Int(if m & 0x0020 != 0 { 1 } else { 0 })))
        }
        ("java/lang/reflect/Modifier", "toString", "(I)Ljava/lang/String;") => {
            let m = args[0].as_int()?;
            let mut parts = Vec::new();
            if m & 0x0001 != 0 { parts.push("public"); }
            if m & 0x0002 != 0 { parts.push("private"); }
            if m & 0x0004 != 0 { parts.push("protected"); }
            if m & 0x0008 != 0 { parts.push("static"); }
            if m & 0x0010 != 0 { parts.push("final"); }
            if m & 0x0020 != 0 { parts.push("synchronized"); }
            if m & 0x0400 != 0 { parts.push("volatile"); }
            if m & 0x0800 != 0 { parts.push("transient"); }
            Ok(Some(vm.new_string(parts.join(" "))))
        }
        _ => Err(VmError::UnhandledException {
            class_name: "".to_string(),
        }),
    }
}

fn get_class_name(vm: &Vm, class_ref: Reference) -> Result<String, VmError> {
    crate::vm::builtin::helpers::class_internal_name(vm, class_ref)
}