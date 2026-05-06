use crate::vm::{HeapValue, Reference, Value, Vm, VmError};

pub(super) fn invoke_reflect(
    vm: &mut Vm,
    class_name: &str,
    method_name: &str,
    descriptor: &str,
    args: &[Value],
) -> Result<Option<Value>, VmError> {
    match (class_name, method_name, descriptor) {
        ("java/lang/Class", "forName", "(Ljava/lang/String;)Ljava/lang/Class;") => {
            let name_ref = args[0].as_reference()?;
            let dotted = crate::vm::builtin::helpers::stringify_reference(vm, name_ref)?;
            let internal = dotted.replace('.', "/");
            vm.ensure_class(&internal)?;
            Ok(Some(Value::Reference(vm.class_object(&internal))))
        }
        (
            "java/lang/Class",
            "getDeclaredMethod",
            "(Ljava/lang/String;[Ljava/lang/Class;)Ljava/lang/reflect/Method;",
        ) => {
            let this_ref = args[0].as_reference()?;
            let name_ref = args[1].as_reference()?;
            let param_types_ref = args[2].as_reference()?;
            let class_name_str = get_class_name(vm, this_ref)?;
            let method_name_str = crate::vm::builtin::helpers::stringify_reference(vm, name_ref)?;
            let param_descriptors = class_array_to_descriptors(vm, param_types_ref)?;
            let method_descriptor = find_method_descriptor(
                vm,
                &class_name_str,
                &method_name_str,
                &param_descriptors,
                false,
            )?;
            let mut fields = std::collections::HashMap::new();
            fields.insert("__declaring_class".to_string(), Value::Reference(this_ref));
            fields.insert("__name".to_string(), Value::Reference(name_ref));
            fields.insert(
                "__descriptor".to_string(),
                vm.intern_string(&method_descriptor),
            );
            fields.insert(
                "__parameter_types".to_string(),
                Value::Reference(param_types_ref),
            );
            fields.insert(
                "__return_type".to_string(),
                Value::Reference(class_ref_for_return_type(vm, &class_name_str, &method_descriptor)?),
            );
            fields.insert("__modifiers".to_string(), Value::Int(1));
            let heap = &mut vm.heap.lock().unwrap();
            let obj_ref = heap.allocate(HeapValue::Object {
                class_name: "java/lang/reflect/Method".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(obj_ref)))
        }
        (
            "java/lang/Class",
            "getMethod",
            "(Ljava/lang/String;[Ljava/lang/Class;)Ljava/lang/reflect/Method;",
        ) => {
            let this_ref = args[0].as_reference()?;
            let name_ref = args[1].as_reference()?;
            let param_types_ref = args[2].as_reference()?;
            let class_name_str = get_class_name(vm, this_ref)?;
            let method_name_str = crate::vm::builtin::helpers::stringify_reference(vm, name_ref)?;
            let param_descriptors = class_array_to_descriptors(vm, param_types_ref)?;
            let method_descriptor = find_method_descriptor(
                vm,
                &class_name_str,
                &method_name_str,
                &param_descriptors,
                true,
            )?;
            let mut fields = std::collections::HashMap::new();
            fields.insert("__declaring_class".to_string(), Value::Reference(this_ref));
            fields.insert("__name".to_string(), Value::Reference(name_ref));
            fields.insert(
                "__descriptor".to_string(),
                vm.intern_string(method_descriptor.clone()),
            );
            fields.insert(
                "__parameter_types".to_string(),
                Value::Reference(param_types_ref),
            );
            fields.insert(
                "__return_type".to_string(),
                Value::Reference(class_ref_for_return_type(vm, &class_name_str, &method_descriptor)?),
            );
            fields.insert("__modifiers".to_string(), Value::Int(1));
            let heap = &mut vm.heap.lock().unwrap();
            let obj_ref = heap.allocate(HeapValue::Object {
                class_name: "java/lang/reflect/Method".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(obj_ref)))
        }
        (
            "java/lang/Class",
            "getDeclaredField",
            "(Ljava/lang/String;)Ljava/lang/reflect/Field;",
        ) => {
            let this_ref = args[0].as_reference()?;
            let name_ref = args[1].as_reference()?;
            let class_name_str = get_class_name(vm, this_ref)?;
            let field_name = crate::vm::builtin::helpers::stringify_reference(vm, name_ref)?;
            let (field_descriptor, is_static) =
                find_field_descriptor(vm, &class_name_str, &field_name, false)?;
            let mut fields = std::collections::HashMap::new();
            fields.insert("__declaring_class".to_string(), Value::Reference(this_ref));
            fields.insert("__name".to_string(), Value::Reference(name_ref));
            fields.insert(
                "__descriptor".to_string(),
                vm.intern_string(field_descriptor.clone()),
            );
            fields.insert(
                "__type".to_string(),
                Value::Reference(class_ref_for_descriptor(vm, &field_descriptor)?),
            );
            fields.insert(
                "__modifiers".to_string(),
                Value::Int(if is_static { 0x0001 | 0x0008 } else { 0x0001 }),
            );
            fields.insert("__slot".to_string(), Value::Int(0));
            let heap = &mut vm.heap.lock().unwrap();
            let obj_ref = heap.allocate(HeapValue::Object {
                class_name: "java/lang/reflect/Field".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(obj_ref)))
        }
        ("java/lang/Class", "getField", "(Ljava/lang/String;)Ljava/lang/reflect/Field;") => {
            let this_ref = args[0].as_reference()?;
            let name_ref = args[1].as_reference()?;
            let class_name_str = get_class_name(vm, this_ref)?;
            let field_name = crate::vm::builtin::helpers::stringify_reference(vm, name_ref)?;
            let (field_descriptor, is_static) =
                find_field_descriptor(vm, &class_name_str, &field_name, true)?;
            let mut fields = std::collections::HashMap::new();
            fields.insert("__declaring_class".to_string(), Value::Reference(this_ref));
            fields.insert("__name".to_string(), Value::Reference(name_ref));
            fields.insert(
                "__descriptor".to_string(),
                vm.intern_string(field_descriptor.clone()),
            );
            fields.insert(
                "__type".to_string(),
                Value::Reference(class_ref_for_descriptor(vm, &field_descriptor)?),
            );
            fields.insert(
                "__modifiers".to_string(),
                Value::Int(if is_static { 0x0001 | 0x0008 } else { 0x0001 }),
            );
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
        (
            "java/lang/Class",
            "getDeclaredConstructor",
            "([Ljava/lang/Class;)Ljava/lang/reflect/Constructor;",
        ) => {
            let this_ref = args[0].as_reference()?;
            let parameter_types_ref = args[1].as_reference()?;
            let class_name_str = get_class_name(vm, this_ref)?;
            let param_descriptors = class_array_to_descriptors(vm, parameter_types_ref)?;
            let constructor_descriptor =
                find_constructor_descriptor(vm, &class_name_str, &param_descriptors, false)?;
            let mut fields = std::collections::HashMap::new();
            fields.insert("__declaring_class".to_string(), Value::Reference(this_ref));
            fields.insert(
                "__parameter_types".to_string(),
                Value::Reference(parameter_types_ref),
            );
            fields.insert("__modifiers".to_string(), Value::Int(1));
            fields.insert(
                "__descriptor".to_string(),
                vm.intern_string(constructor_descriptor),
            );
            fields.insert("__slot".to_string(), Value::Int(0));
            let heap = &mut vm.heap.lock().unwrap();
            let obj_ref = heap.allocate(HeapValue::Object {
                class_name: "java/lang/reflect/Constructor".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(obj_ref)))
        }
        (
            "java/lang/Class",
            "getConstructor",
            "([Ljava/lang/Class;)Ljava/lang/reflect/Constructor;",
        ) => {
            let this_ref = args[0].as_reference()?;
            let parameter_types_ref = args[1].as_reference()?;
            let class_name_str = get_class_name(vm, this_ref)?;
            let param_descriptors = class_array_to_descriptors(vm, parameter_types_ref)?;
            let constructor_descriptor =
                find_constructor_descriptor(vm, &class_name_str, &param_descriptors, true)?;
            let mut fields = std::collections::HashMap::new();
            fields.insert("__declaring_class".to_string(), Value::Reference(this_ref));
            fields.insert(
                "__parameter_types".to_string(),
                Value::Reference(parameter_types_ref),
            );
            fields.insert("__modifiers".to_string(), Value::Int(1));
            fields.insert(
                "__descriptor".to_string(),
                vm.intern_string(constructor_descriptor),
            );
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
        ("java/lang/Class", "newInstance", "()Ljava/lang/Object;") => {
            let this_ref = args[0].as_reference()?;
            let class_name_str = get_class_name(vm, this_ref)?;
            let object = vm.reflect_new_instance(&class_name_str, "()V", vec![])?;
            Ok(Some(Value::Reference(object)))
        }
        ("java/lang/Class", "getClassLoader", "()Ljava/lang/ClassLoader;") => {
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
        ("java/lang/Class", "getModifiers", "()I") => Ok(Some(Value::Int(1))),
        ("java/lang/Class", "getComponentType", "()Ljava/lang/Class;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/lang/Class", "isHidden", "()Z") => Ok(Some(Value::Int(0))),
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
        (
            "java/lang/reflect/Method",
            "invoke",
            "(Ljava/lang/Object;[Ljava/lang/Object;)Ljava/lang/Object;",
        ) => {
            let this_ref = args[0].as_reference()?;
            let receiver_ref = args[1].as_reference()?;
            let params_ref = args[2].as_reference()?;
            let metadata = method_metadata(vm, this_ref)?;
            let invoke_args = reflection_arguments(vm, params_ref, &metadata.descriptor)?;
            let receiver = if receiver_ref == Reference::Null {
                None
            } else {
                Some(receiver_ref)
            };
            let result = vm.reflect_invoke_method(
                &metadata.declaring_class,
                &metadata.name,
                &metadata.descriptor,
                receiver,
                invoke_args,
            )?;
            Ok(Some(box_reflection_return(
                vm,
                result.unwrap_or(Value::Reference(Reference::Null)),
                return_descriptor(&metadata.descriptor),
            )?))
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
            let this_ref = args[0].as_reference()?;
            let heap = vm.heap.lock().unwrap();
            if let Ok(HeapValue::Object { fields, .. }) = heap.get(this_ref) {
                if let Some(Value::Reference(r)) = fields.get("__type") {
                    return Ok(Some(Value::Reference(*r)));
                }
            }
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
            let this_ref = args[0].as_reference()?;
            let target_ref = args[1].as_reference()?;
            let metadata = field_metadata(vm, this_ref)?;
            let value = read_field_value(vm, &metadata, target_ref)?;
            Ok(Some(box_reflection_return(vm, value, &metadata.descriptor)?))
        }
        ("java/lang/reflect/Field", "set", "(Ljava/lang/Object;Ljava/lang/Object;)V") => Ok(None),
        ("java/lang/reflect/Field", "getInt", "(Ljava/lang/Object;)I") => {
            let this_ref = args[0].as_reference()?;
            let target_ref = args[1].as_reference()?;
            let metadata = field_metadata(vm, this_ref)?;
            let value = read_field_value(vm, &metadata, target_ref)?;
            Ok(Some(unbox_reflection_value(vm, value, "I")?))
        }
        ("java/lang/reflect/Field", "setInt", "(Ljava/lang/Object;I)V") => Ok(None),
        ("java/lang/reflect/Field", "getLong", "(Ljava/lang/Object;)J") => {
            let this_ref = args[0].as_reference()?;
            let target_ref = args[1].as_reference()?;
            let metadata = field_metadata(vm, this_ref)?;
            let value = read_field_value(vm, &metadata, target_ref)?;
            Ok(Some(unbox_reflection_value(vm, value, "J")?))
        }
        ("java/lang/reflect/Field", "setLong", "(Ljava/lang/Object;J)V") => Ok(None),
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
        (
            "java/lang/reflect/Constructor",
            "newInstance",
            "([Ljava/lang/Object;)Ljava/lang/Object;",
        ) => {
            let this_ref = args[0].as_reference()?;
            let params_ref = args[1].as_reference()?;
            let metadata = constructor_metadata(vm, this_ref)?;
            let ctor_args = reflection_arguments(vm, params_ref, &metadata.descriptor)?;
            let object = vm.reflect_new_instance(&metadata.declaring_class, &metadata.descriptor, ctor_args)?;
            Ok(Some(Value::Reference(object)))
        }
        ("java/lang/reflect/AccessibleObject", "setAccessible", "(Z)V") => Ok(None),
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
            if m & 0x0001 != 0 {
                parts.push("public");
            }
            if m & 0x0002 != 0 {
                parts.push("private");
            }
            if m & 0x0004 != 0 {
                parts.push("protected");
            }
            if m & 0x0008 != 0 {
                parts.push("static");
            }
            if m & 0x0010 != 0 {
                parts.push("final");
            }
            if m & 0x0020 != 0 {
                parts.push("synchronized");
            }
            if m & 0x0400 != 0 {
                parts.push("volatile");
            }
            if m & 0x0800 != 0 {
                parts.push("transient");
            }
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

#[derive(Clone)]
struct ExecutableMetadata {
    declaring_class: String,
    name: String,
    descriptor: String,
}

#[derive(Clone)]
struct FieldMetadata {
    declaring_class: String,
    name: String,
    descriptor: String,
    is_static: bool,
}

fn class_array_to_descriptors(vm: &Vm, array_ref: Reference) -> Result<Vec<String>, VmError> {
    if array_ref == Reference::Null {
        return Ok(vec![]);
    }
    let heap = vm.heap.lock().unwrap();
    let values = match heap.get(array_ref)? {
        HeapValue::ReferenceArray { values, .. } => values.clone(),
        _ => vec![],
    };
    drop(heap);
    values
        .into_iter()
        .map(|class_ref| class_ref_to_descriptor(vm, class_ref))
        .collect()
}

fn class_ref_to_descriptor(vm: &Vm, class_ref: Reference) -> Result<String, VmError> {
    let name = get_class_name(vm, class_ref)?;
    Ok(match name.as_str() {
        "boolean" => "Z".to_string(),
        "byte" => "B".to_string(),
        "char" => "C".to_string(),
        "short" => "S".to_string(),
        "int" => "I".to_string(),
        "long" => "J".to_string(),
        "float" => "F".to_string(),
        "double" => "D".to_string(),
        "void" => "V".to_string(),
        _ if name.starts_with('[') => name,
        _ => format!("L{name};"),
    })
}

fn class_ref_for_descriptor(vm: &mut Vm, descriptor: &str) -> Result<Reference, VmError> {
    let internal = match descriptor {
        "Z" => "boolean".to_string(),
        "B" => "byte".to_string(),
        "C" => "char".to_string(),
        "S" => "short".to_string(),
        "I" => "int".to_string(),
        "J" => "long".to_string(),
        "F" => "float".to_string(),
        "D" => "double".to_string(),
        "V" => "void".to_string(),
        _ if descriptor.starts_with('[') => descriptor.to_string(),
        _ if descriptor.starts_with('L') && descriptor.ends_with(';') => {
            descriptor[1..descriptor.len() - 1].to_string()
        }
        _ => descriptor.to_string(),
    };
    if !matches!(
        internal.as_str(),
        "boolean" | "byte" | "char" | "short" | "int" | "long" | "float" | "double" | "void"
    ) {
        vm.ensure_class(&internal)?;
    }
    Ok(vm.class_object(&internal))
}

fn class_ref_for_return_type(
    vm: &mut Vm,
    _class_name: &str,
    descriptor: &str,
) -> Result<Reference, VmError> {
    class_ref_for_descriptor(vm, return_descriptor(descriptor))
}

fn find_method_descriptor(
    vm: &mut Vm,
    class_name: &str,
    method_name: &str,
    param_descriptors: &[String],
    include_inherited: bool,
) -> Result<String, VmError> {
    let mut current = Some(class_name.to_string());
    while let Some(name) = current {
        vm.ensure_class(&name)?;
        let class = vm.get_class(&name)?;
        for ((candidate_name, descriptor), _) in &class.methods {
            if candidate_name == method_name
                && parameter_descriptors(descriptor) == param_descriptors
            {
                return Ok(descriptor.clone());
            }
        }
        if include_inherited {
            current = class.super_class.clone();
        } else {
            break;
        }
    }
    Err(VmError::MethodNotFound {
        class_name: class_name.to_string(),
        method_name: method_name.to_string(),
        descriptor: format!("({})", param_descriptors.join("")),
    })
}

fn find_constructor_descriptor(
    vm: &mut Vm,
    class_name: &str,
    param_descriptors: &[String],
    include_inherited: bool,
) -> Result<String, VmError> {
    find_method_descriptor(vm, class_name, "<init>", param_descriptors, include_inherited)
}

fn find_field_descriptor(
    vm: &mut Vm,
    class_name: &str,
    field_name: &str,
    include_inherited: bool,
) -> Result<(String, bool), VmError> {
    let mut current = Some(class_name.to_string());
    while let Some(name) = current {
        vm.ensure_class(&name)?;
        let class = vm.get_class(&name)?;
        if let Some((_, descriptor)) = class.instance_fields.iter().find(|(n, _)| n == field_name) {
            return Ok((descriptor.clone(), false));
        }
        if let Some(value) = class.static_fields.get(field_name) {
            return Ok((descriptor_for_value(*value), true));
        }
        if include_inherited {
            current = class.super_class.clone();
        } else {
            break;
        }
    }
    Err(VmError::FieldNotFound {
        class_name: class_name.to_string(),
        field_name: field_name.to_string(),
    })
}

fn descriptor_for_value(value: Value) -> String {
    match value {
        Value::Int(_) => "I".to_string(),
        Value::Long(_) => "J".to_string(),
        Value::Float(_) => "F".to_string(),
        Value::Double(_) => "D".to_string(),
        Value::Reference(_) => "Ljava/lang/Object;".to_string(),
        Value::ReturnAddress(_) => "I".to_string(),
    }
}

fn parameter_descriptors(descriptor: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut chars = descriptor.chars().peekable();
    if chars.next() != Some('(') {
        return result;
    }
    while let Some(ch) = chars.next() {
        if ch == ')' {
            break;
        }
        let mut token = String::new();
        token.push(ch);
        if ch == 'L' {
            for c in chars.by_ref() {
                token.push(c);
                if c == ';' {
                    break;
                }
            }
        } else if ch == '[' {
            while let Some('[') = chars.peek() {
                token.push(chars.next().unwrap());
            }
            if let Some(c) = chars.next() {
                token.push(c);
                if c == 'L' {
                    for inner in chars.by_ref() {
                        token.push(inner);
                        if inner == ';' {
                            break;
                        }
                    }
                }
            }
        }
        result.push(token);
    }
    result
}

fn return_descriptor(descriptor: &str) -> &str {
    descriptor
        .split_once(')')
        .map(|(_, ret)| ret)
        .unwrap_or("V")
}

fn method_metadata(vm: &Vm, this_ref: Reference) -> Result<ExecutableMetadata, VmError> {
    let heap = vm.heap.lock().unwrap();
    let fields = match heap.get(this_ref)? {
        HeapValue::Object { fields, .. } => fields,
        _ => {
            return Err(VmError::InvalidHeapValue {
                expected: "object",
                actual: "non-object",
            });
        }
    };
    let declaring_class_ref = fields
        .get("__declaring_class")
        .copied()
        .unwrap_or(Value::Reference(Reference::Null))
        .as_reference()?;
    let name_ref = fields
        .get("__name")
        .copied()
        .unwrap_or(Value::Reference(Reference::Null))
        .as_reference()?;
    let descriptor_ref = fields
        .get("__descriptor")
        .copied()
        .unwrap_or(Value::Reference(Reference::Null))
        .as_reference()?;
    drop(heap);
    Ok(ExecutableMetadata {
        declaring_class: get_class_name(vm, declaring_class_ref)?,
        name: crate::vm::builtin::helpers::stringify_reference(vm, name_ref)?,
        descriptor: crate::vm::builtin::helpers::stringify_reference(vm, descriptor_ref)?,
    })
}

fn constructor_metadata(vm: &Vm, this_ref: Reference) -> Result<ExecutableMetadata, VmError> {
    let heap = vm.heap.lock().unwrap();
    let fields = match heap.get(this_ref)? {
        HeapValue::Object { fields, .. } => fields,
        _ => {
            return Err(VmError::InvalidHeapValue {
                expected: "object",
                actual: "non-object",
            });
        }
    };
    let declaring_class_ref = fields
        .get("__declaring_class")
        .copied()
        .unwrap_or(Value::Reference(Reference::Null))
        .as_reference()?;
    let descriptor_ref = fields
        .get("__descriptor")
        .copied()
        .unwrap_or(Value::Reference(Reference::Null))
        .as_reference()?;
    drop(heap);
    Ok(ExecutableMetadata {
        declaring_class: get_class_name(vm, declaring_class_ref)?,
        name: "<init>".to_string(),
        descriptor: crate::vm::builtin::helpers::stringify_reference(vm, descriptor_ref)?,
    })
}

fn field_metadata(vm: &Vm, this_ref: Reference) -> Result<FieldMetadata, VmError> {
    let heap = vm.heap.lock().unwrap();
    let fields = match heap.get(this_ref)? {
        HeapValue::Object { fields, .. } => fields,
        _ => {
            return Err(VmError::InvalidHeapValue {
                expected: "object",
                actual: "non-object",
            });
        }
    };
    let declaring_class_ref = fields
        .get("__declaring_class")
        .copied()
        .unwrap_or(Value::Reference(Reference::Null))
        .as_reference()?;
    let name_ref = fields
        .get("__name")
        .copied()
        .unwrap_or(Value::Reference(Reference::Null))
        .as_reference()?;
    let descriptor_ref = fields
        .get("__descriptor")
        .copied()
        .unwrap_or(Value::Reference(Reference::Null))
        .as_reference()?;
    let modifiers = fields
        .get("__modifiers")
        .copied()
        .unwrap_or(Value::Int(0))
        .as_int()?;
    drop(heap);
    Ok(FieldMetadata {
        declaring_class: get_class_name(vm, declaring_class_ref)?,
        name: crate::vm::builtin::helpers::stringify_reference(vm, name_ref)?,
        descriptor: crate::vm::builtin::helpers::stringify_reference(vm, descriptor_ref)?,
        is_static: modifiers & 0x0008 != 0,
    })
}

fn reflection_arguments(
    vm: &Vm,
    params_ref: Reference,
    descriptor: &str,
) -> Result<Vec<Value>, VmError> {
    let parameter_types = parameter_descriptors(descriptor);
    if params_ref == Reference::Null {
        return Ok(vec![]);
    }
    let heap = vm.heap.lock().unwrap();
    let refs = match heap.get(params_ref)? {
        HeapValue::ReferenceArray { values, .. } => values.clone(),
        _ => vec![],
    };
    drop(heap);
    parameter_types
        .iter()
        .enumerate()
        .map(|(index, ty)| {
            let value_ref = refs.get(index).copied().unwrap_or(Reference::Null);
            unbox_reflection_value(vm, Value::Reference(value_ref), ty)
        })
        .collect()
}

fn read_field_value(
    vm: &Vm,
    metadata: &FieldMetadata,
    target_ref: Reference,
) -> Result<Value, VmError> {
    if metadata.is_static {
        vm.get_static_field(&metadata.declaring_class, &metadata.name)
    } else {
        vm.get_instance_field(target_ref, &metadata.name)
    }
}

fn unbox_reflection_value(vm: &Vm, value: Value, descriptor: &str) -> Result<Value, VmError> {
    match descriptor {
        "I" | "Z" | "B" | "C" | "S" => match value {
            Value::Int(_) => Ok(value),
            Value::Reference(reference) => Ok(Value::Int(
                crate::vm::builtin::helpers::integer_value(vm, reference)?,
            )),
            _ => Ok(Value::Int(0)),
        },
        "J" => match value {
            Value::Long(_) => Ok(value),
            Value::Reference(reference) => match vm.heap.lock().unwrap().get(reference)? {
                HeapValue::Object { fields, .. } => Ok(fields
                    .get("value")
                    .copied()
                    .unwrap_or(Value::Long(0))),
                _ => Ok(Value::Long(0)),
            },
            _ => Ok(Value::Long(0)),
        },
        _ => Ok(value),
    }
}

fn box_reflection_return(vm: &mut Vm, value: Value, descriptor: &str) -> Result<Value, VmError> {
    match descriptor {
        "V" => Ok(Value::Reference(Reference::Null)),
        "I" | "Z" | "B" | "C" | "S" => vm
            .invoke_native("java/lang/Integer", "valueOf", "(I)Ljava/lang/Integer;", &[value])?
            .ok_or(VmError::NullReference),
        "J" => {
            let long_value = value.as_long()?;
            let mut fields = std::collections::HashMap::new();
            fields.insert("value".to_string(), Value::Long(long_value));
            let reference = vm.heap.lock().unwrap().allocate(HeapValue::Object {
                class_name: "java/lang/Long".to_string(),
                fields,
            });
            Ok(Value::Reference(reference))
        }
        _ => Ok(value),
    }
}
