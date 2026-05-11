use crate::vm::types::{UnsafeClassification, classify_unsafe_method, stub_return_value_tracked};
use crate::vm::{Reference, Value, Vm, VmError};

pub(super) fn invoke_other(
    vm: &mut Vm,
    class_name: &str,
    method_name: &str,
    descriptor: &str,
    args: &[Value],
) -> Result<Option<Value>, VmError> {
    match (class_name, method_name, descriptor) {
        ("java/lang/System", "currentTimeMillis", "()J") => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            Ok(Some(Value::Long(now)))
        }
        ("java/lang/System", "nanoTime", "()J") => {
            use std::time::Instant;
            static BASELINE: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
            let base = BASELINE.get_or_init(Instant::now);
            Ok(Some(Value::Long(base.elapsed().as_nanos() as i64)))
        }
        ("java/lang/System", "arraycopy", "(Ljava/lang/Object;ILjava/lang/Object;II)V") => {
            let src = args[0].as_reference()?;
            let src_pos = args[1].as_int()?;
            let dst = args[2].as_reference()?;
            let dst_pos = args[3].as_int()?;
            let length = args[4].as_int()?;
            crate::vm::builtin::helpers::arraycopy(vm, src, src_pos, dst, dst_pos, length)?;
            Ok(None)
        }
        ("java/lang/System", "exit", "(I)V") => {
            let code = args[0].as_int()?;
            std::process::exit(code);
        }
        ("java/lang/System", "getProperty", "(Ljava/lang/String;)Ljava/lang/String;") => {
            let key =
                crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            let value = match key.as_str() {
                "line.separator" => Some("\n".to_string()),
                "file.separator" => Some(std::path::MAIN_SEPARATOR.to_string()),
                "path.separator" => Some(if cfg!(windows) {
                    ";".to_string()
                } else {
                    ":".to_string()
                }),
                "java.version" => Some("21".to_string()),
                "java.specification.version" => Some("21".to_string()),
                "os.name" => Some(std::env::consts::OS.to_string()),
                "os.arch" => Some(std::env::consts::ARCH.to_string()),
                other => std::env::var(other).ok(),
            };
            match value {
                Some(v) => Ok(Some(vm.new_string(v))),
                None => Ok(Some(Value::Reference(Reference::Null))),
            }
        }
        ("java/lang/System", "lineSeparator", "()Ljava/lang/String;") => {
            Ok(Some(vm.new_string("\n".to_string())))
        }
        ("java/lang/System", "identityHashCode", "(Ljava/lang/Object;)I") => {
            let r = args[0].as_reference()?;
            let hash = match r {
                Reference::Null => 0,
                Reference::Heap(i) => i as i32,
            };
            Ok(Some(Value::Int(hash)))
        }
        (
            "java/lang/invoke/MethodHandles",
            "lookup",
            "()Ljava/lang/invoke/MethodHandles$Lookup;",
        ) => Ok(Some(Value::Reference(
            vm.allocate_bootstrap_lookup("java/lang/Object")?,
        ))),
        (
            "java/lang/invoke/MethodHandles",
            "publicLookup",
            "()Ljava/lang/invoke/MethodHandles$Lookup;",
        ) => Ok(Some(Value::Reference(
            vm.allocate_bootstrap_lookup_with_modes("java/lang/Object", 0x01)?,
        ))),
        (
            "java/lang/invoke/MethodHandles",
            "privateLookupIn",
            "(Ljava/lang/Class;Ljava/lang/invoke/MethodHandles$Lookup;)Ljava/lang/invoke/MethodHandles$Lookup;",
        ) => {
            let target_class_ref = args[0].as_reference()?;
            let lookup_ref = args[1].as_reference()?;
            let lookup_class = lookup_class_name(vm, lookup_ref)?;
            let lookup_modes = lookup_modes(vm, lookup_ref)?;
            if lookup_modes & 0x02 == 0 || lookup_modes & 0x10 == 0 {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/IllegalAccessException".to_string(),
                });
            }
            let target_class =
                crate::vm::builtin::helpers::class_internal_name(vm, target_class_ref)?;
            let mut target_modes = 0x1f;
            if !vm.same_runtime_package(&lookup_class, &target_class) {
                target_modes &= !0x04;
            }
            Ok(Some(Value::Reference(
                vm.allocate_bootstrap_lookup_with_modes(&target_class, target_modes)?,
            )))
        }
        (
            "java/lang/invoke/MethodHandles",
            "constant",
            "(Ljava/lang/Class;Ljava/lang/Object;)Ljava/lang/invoke/MethodHandle;",
        ) => {
            let class_ref = args[0].as_reference()?;
            let target_class = crate::vm::builtin::helpers::class_internal_name(vm, class_ref)?;
            Ok(Some(Value::Reference(
                vm.allocate_bootstrap_method_handle(
                    0,
                    &target_class,
                    "",
                    "Ljava/lang/Object;",
                    Some(args[1]),
                )?,
            )))
        }
        (
            "java/lang/invoke/MethodHandles$Lookup",
            "findStatic",
            "(Ljava/lang/Class;Ljava/lang/String;Ljava/lang/invoke/MethodType;)Ljava/lang/invoke/MethodHandle;",
        ) => lookup_find_method(vm, args, 6),
        (
            "java/lang/invoke/MethodHandles$Lookup",
            "findVirtual",
            "(Ljava/lang/Class;Ljava/lang/String;Ljava/lang/invoke/MethodType;)Ljava/lang/invoke/MethodHandle;",
        ) => lookup_find_method(vm, args, 5),
        (
            "java/lang/invoke/MethodHandles$Lookup",
            "findSpecial",
            "(Ljava/lang/Class;Ljava/lang/String;Ljava/lang/invoke/MethodType;Ljava/lang/Class;)Ljava/lang/invoke/MethodHandle;",
        ) => lookup_find_method(vm, args, 7),
        (
            "java/lang/invoke/MethodHandles$Lookup",
            "findConstructor",
            "(Ljava/lang/Class;Ljava/lang/invoke/MethodType;)Ljava/lang/invoke/MethodHandle;",
        ) => {
            let lookup_class = lookup_class_name(vm, args[0].as_reference()?)?;
            let lookup_modes = lookup_modes(vm, args[0].as_reference()?)?;
            let class_ref = args[1].as_reference()?;
            let method_type_ref = args[2].as_reference()?;
            let descriptor_ref = vm
                .get_object_field(method_type_ref, "__descriptor")?
                .as_reference()?;
            let descriptor = crate::vm::builtin::helpers::stringify_reference(vm, descriptor_ref)?;
            let target_class = crate::vm::builtin::helpers::class_internal_name(vm, class_ref)?;
            vm.validate_method_handle_lookup(
                &lookup_class,
                lookup_modes,
                &target_class,
                "<init>",
                &descriptor,
                8,
            )?;
            Ok(Some(Value::Reference(
                vm.allocate_bootstrap_method_handle_with_lookup(
                    8,
                    &target_class,
                    "<init>",
                    &descriptor,
                    None,
                    Some(&lookup_class),
                )?,
            )))
        }
        ("java/lang/invoke/MethodHandles$Lookup", "lookupModes", "()I") => {
            Ok(Some(Value::Int(lookup_modes(vm, args[0].as_reference()?)?)))
        }
        (
            "java/lang/invoke/MethodHandles$Lookup",
            "findGetter",
            "(Ljava/lang/Class;Ljava/lang/String;Ljava/lang/Class;)Ljava/lang/invoke/MethodHandle;",
        ) => lookup_find_field(vm, args, 1),
        (
            "java/lang/invoke/MethodHandles$Lookup",
            "findSetter",
            "(Ljava/lang/Class;Ljava/lang/String;Ljava/lang/Class;)Ljava/lang/invoke/MethodHandle;",
        ) => lookup_find_field(vm, args, 3),
        (
            "java/lang/invoke/MethodHandles$Lookup",
            "findStaticGetter",
            "(Ljava/lang/Class;Ljava/lang/String;Ljava/lang/Class;)Ljava/lang/invoke/MethodHandle;",
        ) => lookup_find_field(vm, args, 2),
        (
            "java/lang/invoke/MethodHandles$Lookup",
            "findStaticSetter",
            "(Ljava/lang/Class;Ljava/lang/String;Ljava/lang/Class;)Ljava/lang/invoke/MethodHandle;",
        ) => lookup_find_field(vm, args, 4),
        (
            "java/lang/invoke/MethodHandles$Lookup",
            "unreflect",
            "(Ljava/lang/reflect/Method;)Ljava/lang/invoke/MethodHandle;",
        ) => unreflect_method(vm, args[0].as_reference()?, args[1].as_reference()?),
        (
            "java/lang/invoke/MethodHandles$Lookup",
            "unreflectConstructor",
            "(Ljava/lang/reflect/Constructor;)Ljava/lang/invoke/MethodHandle;",
        ) => unreflect_constructor(vm, args[0].as_reference()?, args[1].as_reference()?),
        (
            "java/lang/invoke/MethodHandles$Lookup",
            "unreflectGetter",
            "(Ljava/lang/reflect/Field;)Ljava/lang/invoke/MethodHandle;",
        ) => unreflect_field(vm, args[0].as_reference()?, args[1].as_reference()?, 1, 2),
        (
            "java/lang/invoke/MethodHandles$Lookup",
            "unreflectSetter",
            "(Ljava/lang/reflect/Field;)Ljava/lang/invoke/MethodHandle;",
        ) => unreflect_field(vm, args[0].as_reference()?, args[1].as_reference()?, 3, 4),
        (
            "java/lang/invoke/MethodType",
            "toMethodDescriptorString" | "toString",
            "()Ljava/lang/String;",
        ) => {
            let descriptor = vm.get_object_field(args[0].as_reference()?, "__descriptor")?;
            Ok(Some(descriptor))
        }
        ("java/lang/invoke/CallSite", "getTarget", "()Ljava/lang/invoke/MethodHandle;")
        | ("java/lang/invoke/ConstantCallSite", "getTarget", "()Ljava/lang/invoke/MethodHandle;")
        | ("java/lang/invoke/MutableCallSite", "getTarget", "()Ljava/lang/invoke/MethodHandle;")
        | ("java/lang/invoke/VolatileCallSite", "getTarget", "()Ljava/lang/invoke/MethodHandle;") => {
            Ok(Some(
                vm.get_object_field(args[0].as_reference()?, "__target")?,
            ))
        }
        ("java/lang/invoke/ConstantCallSite", "<init>", "(Ljava/lang/invoke/MethodHandle;)V")
        | ("java/lang/invoke/MutableCallSite", "<init>", "(Ljava/lang/invoke/MethodHandle;)V")
        | ("java/lang/invoke/VolatileCallSite", "<init>", "(Ljava/lang/invoke/MethodHandle;)V") => {
            vm.set_object_field(args[0].as_reference()?, "__target", args[1])?;
            Ok(None)
        }
        // MutableCallSite / VolatileCallSite construct with a bare MethodType
        // first, then `setTarget` later.
        ("java/lang/invoke/MutableCallSite", "<init>", "(Ljava/lang/invoke/MethodType;)V")
        | ("java/lang/invoke/VolatileCallSite", "<init>", "(Ljava/lang/invoke/MethodType;)V") => {
            vm.set_object_field(
                args[0].as_reference()?,
                "__target",
                Value::Reference(Reference::Null),
            )?;
            Ok(None)
        }
        (
            "java/lang/invoke/MutableCallSite",
            "setTarget",
            "(Ljava/lang/invoke/MethodHandle;)V",
        )
        | (
            "java/lang/invoke/VolatileCallSite",
            "setTarget",
            "(Ljava/lang/invoke/MethodHandle;)V",
        ) => {
            vm.set_object_field(args[0].as_reference()?, "__target", args[1])?;
            Ok(None)
        }
        ("java/lang/invoke/MutableCallSite", "syncAll", "([Ljava/lang/invoke/MutableCallSite;)V")
        | (
            "java/lang/invoke/VolatileCallSite",
            "syncAll",
            "([Ljava/lang/invoke/VolatileCallSite;)V",
        ) => {
            // No special memory-ordering work needed: stores go through the
            // heap mutex, which gives us SeqCst semantics for the field write.
            Ok(None)
        }
        ("jdk/internal/reflect/Reflection", "getCallerClass", "()Ljava/lang/Class;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("jdk/internal/reflect/Reflection", _, _) => {
            let _ = stub_return_value_tracked(class_name, method_name, descriptor);
            Ok(None)
        }
        ("jdk/internal/misc/Unsafe", "registerNatives", "()V") => Ok(None),
        ("jdk/internal/misc/Unsafe", "getUnsafe", "()Ljdk/internal/misc/Unsafe;") => Ok(Some(
            vm.get_static_field("jdk/internal/misc/Unsafe", "theUnsafe")?,
        )),
        ("jdk/internal/misc/Unsafe", "arrayBaseOffset", "(Ljava/lang/Class;)I") => {
            Ok(Some(Value::Int(0)))
        }
        ("jdk/internal/misc/Unsafe", "arrayIndexScale", "(Ljava/lang/Class;)I") => {
            Ok(Some(Value::Int(1)))
        }
        ("jdk/internal/misc/Unsafe", "addressSize", "()I") => Ok(Some(Value::Int(8))),
        ("jdk/internal/misc/Unsafe", "isBigEndian", "()Z") => {
            Ok(Some(Value::Int(i32::from(cfg!(target_endian = "big")))))
        }
        ("jdk/internal/misc/Unsafe", "pageSize", "()I") => Ok(Some(Value::Int(4096))),
        ("jdk/internal/misc/Unsafe", "objectFieldOffset", _)
        | ("jdk/internal/misc/Unsafe", "staticFieldOffset", _) => Ok(Some(Value::Long(0))),
        ("jdk/internal/misc/Unsafe", "staticFieldBase", _) => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("jdk/internal/misc/Unsafe", "storeFence", "()V")
        | ("jdk/internal/misc/Unsafe", "loadFence", "()V")
        | ("jdk/internal/misc/Unsafe", "fullFence", "()V") => Ok(None),
        (
            "jdk/internal/misc/Unsafe",
            "compareAndSetInt"
            | "compareAndSetLong"
            | "compareAndSetReference"
            | "compareAndSetObject",
            _,
        ) => Ok(Some(Value::Int(1))),
        ("jdk/internal/misc/Unsafe", "getReferenceVolatile", _) => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("jdk/internal/misc/Unsafe", "putReferenceVolatile", _)
        | ("jdk/internal/misc/Unsafe", "putIntVolatile", _) => Ok(None),
        ("jdk/internal/misc/Unsafe", "getIntVolatile", _) => Ok(Some(Value::Int(0))),
        ("jdk/internal/misc/Unsafe", _, _) => {
            let classification = classify_unsafe_method(method_name, descriptor);
            if classification == UnsafeClassification::DangerousStub {
                crate::vm::types::STUB_STATS.record_dangerous_stub(
                    class_name,
                    method_name,
                    descriptor,
                );
                if vm.fail_fast {
                    return Err(VmError::UnsupportedNativeMethod {
                        class_name: class_name.to_string(),
                        method_name: method_name.to_string(),
                        descriptor: descriptor.to_string(),
                    });
                }
            }
            let _ = stub_return_value_tracked(class_name, method_name, descriptor);
            Ok(None)
        }
        ("jdk/internal/misc/CDS", "isDumpingClassList0", "()Z") => Ok(Some(Value::Int(0))),
        ("jdk/internal/misc/CDS", "isDumpingArchive0", "()Z") => Ok(Some(Value::Int(0))),
        ("jdk/internal/misc/CDS", "isSharingEnabled0", "()Z") => Ok(Some(Value::Int(0))),
        ("jdk/internal/misc/CDS", _, _) => {
            let _ = stub_return_value_tracked(class_name, method_name, descriptor);
            Ok(None)
        }
        _ => Err(VmError::UnhandledException {
            class_name: "".to_string(),
        }),
    }
}

fn lookup_find_method(
    vm: &mut Vm,
    args: &[Value],
    reference_kind: u8,
) -> Result<Option<Value>, VmError> {
    let lookup_ref = args[0].as_reference()?;
    let lookup_class = lookup_class_name(vm, lookup_ref)?;
    let lookup_modes = lookup_modes(vm, lookup_ref)?;
    let class_ref = args[1].as_reference()?;
    let method_name =
        crate::vm::builtin::helpers::stringify_reference(vm, args[2].as_reference()?)?;
    let method_type_ref = args[3].as_reference()?;
    let descriptor_ref = vm
        .get_object_field(method_type_ref, "__descriptor")?
        .as_reference()?;
    let descriptor = crate::vm::builtin::helpers::stringify_reference(vm, descriptor_ref)?;
    let target_class = crate::vm::builtin::helpers::class_internal_name(vm, class_ref)?;
    vm.validate_method_handle_lookup(
        &lookup_class,
        lookup_modes,
        &target_class,
        &method_name,
        &descriptor,
        reference_kind,
    )?;
    Ok(Some(Value::Reference(
        vm.allocate_bootstrap_method_handle_with_lookup(
            reference_kind,
            &target_class,
            &method_name,
            &descriptor,
            None,
            Some(&lookup_class),
        )?,
    )))
}

fn lookup_find_field(
    vm: &mut Vm,
    args: &[Value],
    reference_kind: u8,
) -> Result<Option<Value>, VmError> {
    let lookup_ref = args[0].as_reference()?;
    let lookup_class = lookup_class_name(vm, lookup_ref)?;
    let lookup_modes = lookup_modes(vm, lookup_ref)?;
    let class_ref = args[1].as_reference()?;
    let field_name = crate::vm::builtin::helpers::stringify_reference(vm, args[2].as_reference()?)?;
    let field_class_ref = args[3].as_reference()?;
    let target_class = crate::vm::builtin::helpers::class_internal_name(vm, class_ref)?;
    let field_class = crate::vm::builtin::helpers::class_internal_name(vm, field_class_ref)?;
    let descriptor = crate::vm::builtin::helpers::class_name_to_descriptor(&field_class);
    let resolved_class = vm.validate_field_method_handle_lookup(
        &lookup_class,
        lookup_modes,
        &target_class,
        &field_name,
        &descriptor,
        reference_kind,
    )?;
    Ok(Some(Value::Reference(
        vm.allocate_bootstrap_method_handle_with_lookup(
            reference_kind,
            &resolved_class,
            &field_name,
            &descriptor,
            None,
            Some(&lookup_class),
        )?,
    )))
}

fn unreflect_method(
    vm: &mut Vm,
    lookup_ref: Reference,
    method_ref: Reference,
) -> Result<Option<Value>, VmError> {
    let lookup_class = lookup_class_name(vm, lookup_ref)?;
    let lookup_modes = lookup_modes(vm, lookup_ref)?;
    let declaring_class_ref = vm
        .get_object_field(method_ref, "__declaring_class")?
        .as_reference()?;
    let name_ref = vm.get_object_field(method_ref, "__name")?.as_reference()?;
    let desc_ref = vm
        .get_object_field(method_ref, "__descriptor")?
        .as_reference()?;
    let modifiers = vm.get_object_field(method_ref, "__modifiers")?.as_int()? as u16;
    let target_class = crate::vm::builtin::helpers::class_internal_name(vm, declaring_class_ref)?;
    let method_name = crate::vm::builtin::helpers::stringify_reference(vm, name_ref)?;
    let descriptor = crate::vm::builtin::helpers::stringify_reference(vm, desc_ref)?;
    vm.validate_lookup_member_access(&lookup_class, lookup_modes, &target_class, modifiers)?;
    let reference_kind = if modifiers & 0x0008 != 0 { 6 } else { 5 };
    Ok(Some(Value::Reference(
        vm.allocate_bootstrap_method_handle_with_lookup(
            reference_kind,
            &target_class,
            &method_name,
            &descriptor,
            None,
            Some(&lookup_class),
        )?,
    )))
}

fn lookup_class_name(vm: &Vm, lookup_ref: Reference) -> Result<String, VmError> {
    let lookup_class_ref = vm
        .get_object_field(lookup_ref, "__lookupClass")?
        .as_reference()?;
    crate::vm::builtin::helpers::class_internal_name(vm, lookup_class_ref)
}

fn lookup_modes(vm: &Vm, lookup_ref: Reference) -> Result<i32, VmError> {
    vm.get_object_field(lookup_ref, "__modes")
        .and_then(|value| value.as_int())
        .or(Ok(0x5f))
}

fn unreflect_constructor(
    vm: &mut Vm,
    lookup_ref: Reference,
    ctor_ref: Reference,
) -> Result<Option<Value>, VmError> {
    let lookup_class = lookup_class_name(vm, lookup_ref)?;
    let lookup_modes = lookup_modes(vm, lookup_ref)?;
    let declaring_class_ref = vm
        .get_object_field(ctor_ref, "__declaring_class")?
        .as_reference()?;
    let desc_ref = vm
        .get_object_field(ctor_ref, "__descriptor")?
        .as_reference()?;
    let modifiers = vm.get_object_field(ctor_ref, "__modifiers")?.as_int()? as u16;
    let target_class = crate::vm::builtin::helpers::class_internal_name(vm, declaring_class_ref)?;
    let descriptor = crate::vm::builtin::helpers::stringify_reference(vm, desc_ref)?;
    vm.validate_lookup_member_access(&lookup_class, lookup_modes, &target_class, modifiers)?;
    Ok(Some(Value::Reference(
        vm.allocate_bootstrap_method_handle_with_lookup(
            8,
            &target_class,
            "<init>",
            &descriptor,
            None,
            Some(&lookup_class),
        )?,
    )))
}

fn unreflect_field(
    vm: &mut Vm,
    lookup_ref: Reference,
    field_ref: Reference,
    instance_kind: u8,
    static_kind: u8,
) -> Result<Option<Value>, VmError> {
    let lookup_class = lookup_class_name(vm, lookup_ref)?;
    let lookup_modes = lookup_modes(vm, lookup_ref)?;
    let declaring_class_ref = vm
        .get_object_field(field_ref, "__declaring_class")?
        .as_reference()?;
    let name_ref = vm.get_object_field(field_ref, "__name")?.as_reference()?;
    let type_ref = vm.get_object_field(field_ref, "__type")?.as_reference()?;
    let modifiers = vm.get_object_field(field_ref, "__modifiers")?.as_int()?;
    let target_class = crate::vm::builtin::helpers::class_internal_name(vm, declaring_class_ref)?;
    let field_name = crate::vm::builtin::helpers::stringify_reference(vm, name_ref)?;
    let field_type = crate::vm::builtin::helpers::class_internal_name(vm, type_ref)?;
    let descriptor = crate::vm::builtin::helpers::class_name_to_descriptor(&field_type);
    vm.validate_lookup_member_access(&lookup_class, lookup_modes, &target_class, modifiers as u16)?;
    let reference_kind = if modifiers & 0x0008 != 0 {
        static_kind
    } else {
        instance_kind
    };
    Ok(Some(Value::Reference(
        vm.allocate_bootstrap_method_handle_with_lookup(
            reference_kind,
            &target_class,
            &field_name,
            &descriptor,
            None,
            Some(&lookup_class),
        )?,
    )))
}
