use crate::vm::types::{UnsafeClassification, classify_unsafe_method, stub_return_value_tracked};
use crate::vm::{Reference, Value, Vm, VmError};

pub(super) fn invoke_other(
    vm: &mut Vm,
    class_name: &str,
    method_name: &str,
    descriptor: &str,
    args: &[Value],
) -> Result<Option<Value>, VmError> {
    if let Some(result) = super::method_handle_combinators::try_invoke_combinator(
        vm,
        class_name,
        method_name,
        descriptor,
        args,
    )? {
        return Ok(result);
    }
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
            "findVarHandle",
            "(Ljava/lang/Class;Ljava/lang/String;Ljava/lang/Class;)Ljava/lang/invoke/VarHandle;",
        ) => {
            let owner_class_ref = args[1].as_reference()?;
            let name_ref = args[2].as_reference()?;
            let type_class_ref = args[3].as_reference()?;
            let owner = crate::vm::builtin::helpers::class_internal_name(vm, owner_class_ref)?;
            let name = crate::vm::builtin::helpers::stringify_reference(vm, name_ref)?;
            let type_internal =
                crate::vm::builtin::helpers::class_internal_name(vm, type_class_ref)?;
            let descriptor = type_class_to_descriptor(&type_internal);
            let vh = vm.allocate_var_handle(0, &owner, &name, &descriptor)?;
            Ok(Some(Value::Reference(vh)))
        }
        (
            "java/lang/invoke/MethodHandles$Lookup",
            "findStaticVarHandle",
            "(Ljava/lang/Class;Ljava/lang/String;Ljava/lang/Class;)Ljava/lang/invoke/VarHandle;",
        ) => {
            let owner_class_ref = args[1].as_reference()?;
            let name_ref = args[2].as_reference()?;
            let type_class_ref = args[3].as_reference()?;
            let owner = crate::vm::builtin::helpers::class_internal_name(vm, owner_class_ref)?;
            let name = crate::vm::builtin::helpers::stringify_reference(vm, name_ref)?;
            let type_internal =
                crate::vm::builtin::helpers::class_internal_name(vm, type_class_ref)?;
            let descriptor = type_class_to_descriptor(&type_internal);
            let vh = vm.allocate_var_handle(1, &owner, &name, &descriptor)?;
            Ok(Some(Value::Reference(vh)))
        }
        (
            "java/lang/invoke/MethodHandles",
            "arrayElementVarHandle",
            "(Ljava/lang/Class;)Ljava/lang/invoke/VarHandle;",
        ) => {
            let array_class_ref = args[0].as_reference()?;
            let array_name =
                crate::vm::builtin::helpers::class_internal_name(vm, array_class_ref)?;
            // array_name is e.g. `[I`, `[J`, `[Ljava/lang/Object;`.
            let element_desc = array_name
                .strip_prefix('[')
                .map(|s| s.to_string())
                .unwrap_or_else(|| "Ljava/lang/Object;".to_string());
            let vh = vm.allocate_var_handle(2, &array_name, "", &element_desc)?;
            Ok(Some(Value::Reference(vh)))
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
            "lookupClass",
            "()Ljava/lang/Class;",
        ) => Ok(Some(vm.get_object_field(args[0].as_reference()?, "__lookupClass")?)),
        (
            "java/lang/invoke/MethodHandles$Lookup",
            "previousLookupClass",
            "()Ljava/lang/Class;",
        ) => Ok(Some(
            vm.get_object_field(args[0].as_reference()?, "__previousLookupClass")
                .unwrap_or(Value::Reference(Reference::Null)),
        )),
        (
            "java/lang/invoke/MethodHandles$Lookup",
            "hasFullPrivilegeAccess",
            "()Z",
        ) => {
            let modes = lookup_modes(vm, args[0].as_reference()?)?;
            // PRIVATE (0x02) | MODULE (0x10) bits required for full privilege.
            let full = (modes & 0x02 != 0) && (modes & 0x10 != 0);
            Ok(Some(Value::Int(if full { 1 } else { 0 })))
        }
        (
            "java/lang/invoke/MethodHandles$Lookup",
            "in",
            "(Ljava/lang/Class;)Ljava/lang/invoke/MethodHandles$Lookup;",
        ) => {
            let lookup_ref = args[0].as_reference()?;
            let target_class_ref = args[1].as_reference()?;
            let prev_lookup_class = lookup_class_name(vm, lookup_ref)?;
            let prev_modes = lookup_modes(vm, lookup_ref)?;
            let target_class =
                crate::vm::builtin::helpers::class_internal_name(vm, target_class_ref)?;
            // JVMS: in(C) drops PROTECTED unconditionally; drops PRIVATE,
            // PACKAGE if C is in a different package; drops PRIVATE, PACKAGE,
            // MODULE if C is in a different module (which we collapse to
            // "different runtime package" for jvm-rs).
            let mut new_modes = prev_modes & !0x04; // drop PROTECTED
            if !vm.same_runtime_package(&prev_lookup_class, &target_class) {
                new_modes &= !(0x02 | 0x08); // drop PRIVATE, PACKAGE
                new_modes &= !0x10; // drop MODULE (no real module tracking)
            }
            Ok(Some(Value::Reference(
                vm.allocate_bootstrap_lookup_full(
                    &target_class,
                    new_modes,
                    Some(&prev_lookup_class),
                )?,
            )))
        }
        (
            "java/lang/invoke/MethodHandles$Lookup",
            "dropLookupMode",
            "(I)Ljava/lang/invoke/MethodHandles$Lookup;",
        ) => {
            let lookup_ref = args[0].as_reference()?;
            let drop_mode = args[1].as_int()?;
            let lookup_class = lookup_class_name(vm, lookup_ref)?;
            let modes = lookup_modes(vm, lookup_ref)?;
            // Per JVMS dropping PROTECTED is a no-op; dropping PUBLIC nukes all
            // access bits.
            let new_modes = if drop_mode == 0x04 {
                modes
            } else if drop_mode == 0x01 {
                0
            } else {
                modes & !drop_mode
            };
            let prev =
                vm.get_object_field(lookup_ref, "__previousLookupClass")?.as_reference()?;
            let prev_class = if prev == Reference::Null {
                None
            } else {
                Some(crate::vm::builtin::helpers::class_internal_name(vm, prev)?)
            };
            Ok(Some(Value::Reference(
                vm.allocate_bootstrap_lookup_full(
                    &lookup_class,
                    new_modes,
                    prev_class.as_deref(),
                )?,
            )))
        }
        (
            "java/lang/invoke/MethodHandles$Lookup",
            "accessClass",
            "(Ljava/lang/Class;)Ljava/lang/Class;",
        ) => {
            // Best-effort: load the class and return it. The full JVMS check
            // would validate against modes — defer until a real workload needs
            // it.
            let class_ref = args[1].as_reference()?;
            let class_name =
                crate::vm::builtin::helpers::class_internal_name(vm, class_ref)?;
            vm.ensure_class_loaded(&class_name)?;
            Ok(Some(Value::Reference(class_ref)))
        }
        (
            "java/lang/invoke/MethodHandles$Lookup",
            "ensureInitialized",
            "(Ljava/lang/Class;)Ljava/lang/Class;",
        ) => {
            let class_ref = args[1].as_reference()?;
            let class_name =
                crate::vm::builtin::helpers::class_internal_name(vm, class_ref)?;
            vm.ensure_class_loaded(&class_name)?;
            vm.ensure_class_initialized(&class_name)?;
            Ok(Some(Value::Reference(class_ref)))
        }
        (
            "java/lang/invoke/MethodHandles$Lookup",
            "revealDirect",
            "(Ljava/lang/invoke/MethodHandle;)Ljava/lang/invoke/MethodHandleInfo;",
        ) => {
            let handle = args[1].as_reference()?;
            let info = vm.allocate_method_handle_info(handle)?;
            Ok(Some(Value::Reference(info)))
        }
        (
            "java/lang/invoke/MethodHandles$Lookup",
            "defineHiddenClass",
            "([BZ[Ljava/lang/invoke/MethodHandles$Lookup$ClassOption;)Ljava/lang/invoke/MethodHandles$Lookup;",
        )
        | (
            "java/lang/invoke/MethodHandles$Lookup",
            "defineHiddenClassWithClassData",
            "([BLjava/lang/Object;Z[Ljava/lang/invoke/MethodHandles$Lookup$ClassOption;)Ljava/lang/invoke/MethodHandles$Lookup;",
        ) => {
            let lookup_ref = args[0].as_reference()?;
            let bytes_ref = args[1].as_reference()?;
            let initialize = if method_name == "defineHiddenClass" {
                args[2].as_int()? != 0
            } else {
                args[3].as_int()? != 0
            };
            let lookup_class = lookup_class_name(vm, lookup_ref)?;
            let new_class = vm.define_hidden_class(&lookup_class, bytes_ref, initialize)?;
            let modes = lookup_modes(vm, lookup_ref)?;
            let new_lookup = vm.allocate_bootstrap_lookup_full(
                &new_class,
                modes,
                Some(&lookup_class),
            )?;
            Ok(Some(Value::Reference(new_lookup)))
        }
        ("java/lang/invoke/MethodHandleInfo", "getReferenceKind", "()I") => {
            Ok(Some(vm.get_object_field(args[0].as_reference()?, "__referenceKind")?))
        }
        ("java/lang/invoke/MethodHandleInfo", "getDeclaringClass", "()Ljava/lang/Class;") => {
            Ok(Some(vm.get_object_field(args[0].as_reference()?, "__declaringClass")?))
        }
        ("java/lang/invoke/MethodHandleInfo", "getName", "()Ljava/lang/String;") => {
            Ok(Some(vm.get_object_field(args[0].as_reference()?, "__name")?))
        }
        ("java/lang/invoke/MethodHandleInfo", "getMethodType", "()Ljava/lang/invoke/MethodType;") => {
            Ok(Some(vm.get_object_field(args[0].as_reference()?, "__methodType")?))
        }
        ("java/lang/invoke/MethodHandleInfo", "getModifiers", "()I") => {
            Ok(Some(Value::Int(0x0001))) // ACC_PUBLIC
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
        (
            "java/lang/invoke/CallSite",
            "dynamicInvoker",
            "()Ljava/lang/invoke/MethodHandle;",
        )
        | (
            "java/lang/invoke/ConstantCallSite",
            "dynamicInvoker",
            "()Ljava/lang/invoke/MethodHandle;",
        )
        | (
            "java/lang/invoke/MutableCallSite",
            "dynamicInvoker",
            "()Ljava/lang/invoke/MethodHandle;",
        )
        | (
            "java/lang/invoke/VolatileCallSite",
            "dynamicInvoker",
            "()Ljava/lang/invoke/MethodHandle;",
        ) => {
            let callsite = args[0].as_reference()?;
            // Read the current target's descriptor to use as the invoker's
            // type. If the target is null, fall back to a no-op `()V`.
            let target_value = vm.get_object_field(callsite, "__target")?;
            let target_ref = target_value.as_reference()?;
            let desc = if target_ref == Reference::Null {
                "()V".to_string()
            } else {
                let target_desc_ref = vm
                    .get_object_field(target_ref, "__targetDesc")?
                    .as_reference()?;
                if target_desc_ref == Reference::Null {
                    "()V".to_string()
                } else {
                    let s = vm
                        .get_object_field(target_desc_ref, "__descriptor")?
                        .as_reference()?;
                    crate::vm::builtin::helpers::stringify_reference(vm, s)?
                }
            };
            let mh = vm.allocate_derived_method_handle(
                crate::vm::MH_KIND_INVOKER,
                &desc,
                vec![
                    (
                        "__invokerKind",
                        Value::Int(crate::vm::MH_INVOKER_CALLSITE),
                    ),
                    ("__invokerCallsite", Value::Reference(callsite)),
                ],
            )?;
            Ok(Some(Value::Reference(mh)))
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
        ("jdk/internal/misc/Unsafe", "objectFieldOffset", _) => {
            // Args: (this, Field) or (this, Class, String). We support both shapes.
            let offset = if args.len() == 3 {
                let field_ref = args[1].as_reference()?;
                unsafe_field_offset_from_field(vm, field_ref)?
            } else if args.len() == 4 {
                let class_ref = args[1].as_reference()?;
                let name_ref = args[2].as_reference()?;
                unsafe_field_offset_from_class_name(vm, class_ref, name_ref)?
            } else {
                0
            };
            Ok(Some(Value::Long(offset)))
        }
        ("jdk/internal/misc/Unsafe", "staticFieldOffset", _) => Ok(Some(Value::Long(0))),
        ("jdk/internal/misc/Unsafe", "staticFieldBase", _) => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("jdk/internal/misc/Unsafe", "storeFence", "()V")
        | ("jdk/internal/misc/Unsafe", "loadFence", "()V")
        | ("jdk/internal/misc/Unsafe", "fullFence", "()V") => Ok(None),
        (
            "jdk/internal/misc/Unsafe",
            "compareAndSetInt",
            "(Ljava/lang/Object;JII)Z",
        ) => unsafe_cas_int(vm, args),
        (
            "jdk/internal/misc/Unsafe",
            "compareAndSetLong",
            "(Ljava/lang/Object;JJJ)Z",
        ) => unsafe_cas_long(vm, args),
        (
            "jdk/internal/misc/Unsafe",
            "compareAndSetReference" | "compareAndSetObject",
            "(Ljava/lang/Object;JLjava/lang/Object;Ljava/lang/Object;)Z",
        ) => unsafe_cas_reference(vm, args),
        (
            "jdk/internal/misc/Unsafe",
            "getAndAddInt",
            "(Ljava/lang/Object;JI)I",
        ) => unsafe_get_and_add_int(vm, args),
        (
            "jdk/internal/misc/Unsafe",
            "getAndAddLong",
            "(Ljava/lang/Object;JJ)J",
        ) => unsafe_get_and_add_long(vm, args),
        (
            "jdk/internal/misc/Unsafe",
            "getAndSetInt",
            "(Ljava/lang/Object;JI)I",
        ) => unsafe_get_and_set_int(vm, args),
        (
            "jdk/internal/misc/Unsafe",
            "getAndSetLong",
            "(Ljava/lang/Object;JJ)J",
        ) => unsafe_get_and_set_long(vm, args),
        (
            "jdk/internal/misc/Unsafe",
            "getAndSetReference" | "getAndSetObject",
            "(Ljava/lang/Object;JLjava/lang/Object;)Ljava/lang/Object;",
        ) => unsafe_get_and_set_reference(vm, args),
        (
            "jdk/internal/misc/Unsafe",
            "getIntVolatile",
            "(Ljava/lang/Object;J)I",
        ) => unsafe_get_int(vm, args),
        (
            "jdk/internal/misc/Unsafe",
            "putIntVolatile",
            "(Ljava/lang/Object;JI)V",
        ) => unsafe_put_int(vm, args),
        (
            "jdk/internal/misc/Unsafe",
            "getLongVolatile",
            "(Ljava/lang/Object;J)J",
        ) => unsafe_get_long(vm, args),
        (
            "jdk/internal/misc/Unsafe",
            "putLongVolatile",
            "(Ljava/lang/Object;JJ)V",
        ) => unsafe_put_long(vm, args),
        (
            "jdk/internal/misc/Unsafe",
            "getReferenceVolatile",
            "(Ljava/lang/Object;J)Ljava/lang/Object;",
        ) => unsafe_get_reference(vm, args),
        (
            "jdk/internal/misc/Unsafe",
            "putReferenceVolatile",
            "(Ljava/lang/Object;JLjava/lang/Object;)V",
        ) => unsafe_put_reference(vm, args),
        // Non-volatile field accessors — same semantics as volatile in our single-threaded
        // heap model, so reuse the same helper functions.
        ("jdk/internal/misc/Unsafe", "getInt", "(Ljava/lang/Object;J)I") => {
            unsafe_get_int(vm, args)
        }
        ("jdk/internal/misc/Unsafe", "putInt", "(Ljava/lang/Object;JI)V") => {
            unsafe_put_int(vm, args)
        }
        ("jdk/internal/misc/Unsafe", "getLong", "(Ljava/lang/Object;J)J") => {
            unsafe_get_long(vm, args)
        }
        ("jdk/internal/misc/Unsafe", "putLong", "(Ljava/lang/Object;JJ)V") => {
            unsafe_put_long(vm, args)
        }
        (
            "jdk/internal/misc/Unsafe",
            "getReference" | "getObject",
            "(Ljava/lang/Object;J)Ljava/lang/Object;",
        ) => unsafe_get_reference(vm, args),
        (
            "jdk/internal/misc/Unsafe",
            "putReference" | "putObject",
            "(Ljava/lang/Object;JLjava/lang/Object;)V",
        ) => unsafe_put_reference(vm, args),
        ("jdk/internal/misc/Unsafe", "getByte", "(Ljava/lang/Object;J)B") => {
            unsafe_get_int(vm, args)
        }
        ("jdk/internal/misc/Unsafe", "putByte", "(Ljava/lang/Object;JB)V") => {
            unsafe_put_int(vm, args)
        }
        ("jdk/internal/misc/Unsafe", "getBoolean", "(Ljava/lang/Object;J)Z") => {
            unsafe_get_int(vm, args)
        }
        ("jdk/internal/misc/Unsafe", "putBoolean", "(Ljava/lang/Object;JZ)V") => {
            unsafe_put_int(vm, args)
        }
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

/// Convert a `Class<?>` internal name (`int`, `long`, `java/lang/String`, `[I`)
/// into a JVMS field descriptor.
fn type_class_to_descriptor(internal: &str) -> String {
    match internal {
        "int" => "I".to_string(),
        "long" => "J".to_string(),
        "float" => "F".to_string(),
        "double" => "D".to_string(),
        "boolean" => "Z".to_string(),
        "byte" => "B".to_string(),
        "char" => "C".to_string(),
        "short" => "S".to_string(),
        "void" => "V".to_string(),
        s if s.starts_with('[') => s.to_string(),
        s if s.len() == 1 && "BCDFIJSZV".contains(s) => s.to_string(),
        s => format!("L{};", s),
    }
}

// ----- Unsafe field / array RMW helpers (M3.4) -----

fn unsafe_field_offset_from_field(vm: &mut Vm, field_ref: Reference) -> Result<i64, VmError> {
    if field_ref == Reference::Null {
        return Ok(0);
    }
    let declaring_ref = vm
        .get_object_field(field_ref, "__declaring_class")?
        .as_reference()?;
    let name_ref = vm.get_object_field(field_ref, "__name")?.as_reference()?;
    unsafe_field_offset_from_class_name(vm, declaring_ref, name_ref)
}

fn unsafe_field_offset_from_class_name(
    vm: &mut Vm,
    class_ref: Reference,
    name_ref: Reference,
) -> Result<i64, VmError> {
    if class_ref == Reference::Null || name_ref == Reference::Null {
        return Ok(0);
    }
    let class_internal = crate::vm::builtin::helpers::class_internal_name(vm, class_ref)?;
    let name = crate::vm::builtin::helpers::stringify_reference(vm, name_ref)?;
    if let Ok(class) = vm.get_class(&class_internal) {
        if let Some(offset) = class.field_offsets.get(&name).copied() {
            return Ok(offset as i64);
        }
    }
    Ok(0)
}

/// Apply an `RMW` closure to an instance field slot or an array element while
/// holding the heap mutex. Returns the closure's result.
fn with_target_slot<F, R>(vm: &mut Vm, target: Reference, offset: i64, f: F) -> Result<R, VmError>
where
    F: FnOnce(SlotView<'_>) -> Result<R, VmError>,
{
    if target == Reference::Null {
        return Err(VmError::UnhandledException {
            class_name: "java/lang/NullPointerException".to_string(),
        });
    }
    let mut heap = vm.heap.lock().unwrap();
    let hv = heap.get_mut(target)?;
    match hv {
        crate::vm::HeapValue::Object { fields, .. } => {
            let idx = offset as usize;
            if idx >= fields.len() {
                return Err(VmError::FieldNotFound {
                    class_name: String::new(),
                    field_name: format!("offset {offset}"),
                });
            }
            f(SlotView::Single(&mut fields[idx]))
        }
        crate::vm::HeapValue::IntArray { values } => {
            let idx = offset as usize;
            if idx >= values.len() {
                return Err(VmError::ArrayIndexOutOfBounds {
                    index: offset as i32,
                    len: values.len(),
                });
            }
            f(SlotView::IntArr(values, idx))
        }
        crate::vm::HeapValue::LongArray { values } => {
            let idx = offset as usize;
            if idx >= values.len() {
                return Err(VmError::ArrayIndexOutOfBounds {
                    index: offset as i32,
                    len: values.len(),
                });
            }
            f(SlotView::LongArr(values, idx))
        }
        crate::vm::HeapValue::ReferenceArray { values, .. } => {
            let idx = offset as usize;
            if idx >= values.len() {
                return Err(VmError::ArrayIndexOutOfBounds {
                    index: offset as i32,
                    len: values.len(),
                });
            }
            f(SlotView::RefArr(values, idx))
        }
        other => Err(VmError::InvalidHeapValue {
            expected: "object-or-array",
            actual: other.kind_name(),
        }),
    }
}

enum SlotView<'a> {
    Single(&'a mut Value),
    IntArr(&'a mut Vec<i32>, usize),
    LongArr(&'a mut Vec<i64>, usize),
    RefArr(&'a mut Vec<Reference>, usize),
}

fn unsafe_cas_int(vm: &mut Vm, args: &[Value]) -> Result<Option<Value>, VmError> {
    let target = args[1].as_reference()?;
    let offset = args[2].as_long()?;
    let expected = args[3].as_int()?;
    let new = args[4].as_int()?;
    let ok = with_target_slot(vm, target, offset, |slot| match slot {
        SlotView::Single(v) => {
            let prev = v.as_int().unwrap_or(0);
            if prev == expected {
                *v = Value::Int(new);
                Ok(true)
            } else {
                Ok(false)
            }
        }
        SlotView::IntArr(arr, idx) => {
            if arr[idx] == expected {
                arr[idx] = new;
                Ok(true)
            } else {
                Ok(false)
            }
        }
        _ => Ok(false),
    })?;
    Ok(Some(Value::Int(if ok { 1 } else { 0 })))
}

fn unsafe_cas_long(vm: &mut Vm, args: &[Value]) -> Result<Option<Value>, VmError> {
    let target = args[1].as_reference()?;
    let offset = args[2].as_long()?;
    let expected = args[3].as_long()?;
    let new = args[4].as_long()?;
    let ok = with_target_slot(vm, target, offset, |slot| match slot {
        SlotView::Single(v) => {
            let prev = v.as_long().unwrap_or(0);
            if prev == expected {
                *v = Value::Long(new);
                Ok(true)
            } else {
                Ok(false)
            }
        }
        SlotView::LongArr(arr, idx) => {
            if arr[idx] == expected {
                arr[idx] = new;
                Ok(true)
            } else {
                Ok(false)
            }
        }
        _ => Ok(false),
    })?;
    Ok(Some(Value::Int(if ok { 1 } else { 0 })))
}

fn unsafe_cas_reference(vm: &mut Vm, args: &[Value]) -> Result<Option<Value>, VmError> {
    let target = args[1].as_reference()?;
    let offset = args[2].as_long()?;
    let expected = args[3].as_reference()?;
    let new = args[4].as_reference()?;
    let ok = with_target_slot(vm, target, offset, |slot| match slot {
        SlotView::Single(v) => {
            let prev = v.as_reference().unwrap_or(Reference::Null);
            if prev == expected {
                *v = Value::Reference(new);
                Ok(true)
            } else {
                Ok(false)
            }
        }
        SlotView::RefArr(arr, idx) => {
            if arr[idx] == expected {
                arr[idx] = new;
                Ok(true)
            } else {
                Ok(false)
            }
        }
        _ => Ok(false),
    })?;
    Ok(Some(Value::Int(if ok { 1 } else { 0 })))
}

fn unsafe_get_and_add_int(vm: &mut Vm, args: &[Value]) -> Result<Option<Value>, VmError> {
    let target = args[1].as_reference()?;
    let offset = args[2].as_long()?;
    let delta = args[3].as_int()?;
    let prev = with_target_slot(vm, target, offset, |slot| match slot {
        SlotView::Single(v) => {
            let p = v.as_int().unwrap_or(0);
            *v = Value::Int(p.wrapping_add(delta));
            Ok(p)
        }
        SlotView::IntArr(arr, idx) => {
            let p = arr[idx];
            arr[idx] = p.wrapping_add(delta);
            Ok(p)
        }
        _ => Ok(0),
    })?;
    Ok(Some(Value::Int(prev)))
}

fn unsafe_get_and_add_long(vm: &mut Vm, args: &[Value]) -> Result<Option<Value>, VmError> {
    let target = args[1].as_reference()?;
    let offset = args[2].as_long()?;
    let delta = args[3].as_long()?;
    let prev = with_target_slot(vm, target, offset, |slot| match slot {
        SlotView::Single(v) => {
            let p = v.as_long().unwrap_or(0);
            *v = Value::Long(p.wrapping_add(delta));
            Ok(p)
        }
        SlotView::LongArr(arr, idx) => {
            let p = arr[idx];
            arr[idx] = p.wrapping_add(delta);
            Ok(p)
        }
        _ => Ok(0),
    })?;
    Ok(Some(Value::Long(prev)))
}

fn unsafe_get_and_set_int(vm: &mut Vm, args: &[Value]) -> Result<Option<Value>, VmError> {
    let target = args[1].as_reference()?;
    let offset = args[2].as_long()?;
    let new = args[3].as_int()?;
    let prev = with_target_slot(vm, target, offset, |slot| match slot {
        SlotView::Single(v) => {
            let p = v.as_int().unwrap_or(0);
            *v = Value::Int(new);
            Ok(p)
        }
        SlotView::IntArr(arr, idx) => {
            let p = arr[idx];
            arr[idx] = new;
            Ok(p)
        }
        _ => Ok(0),
    })?;
    Ok(Some(Value::Int(prev)))
}

fn unsafe_get_and_set_long(vm: &mut Vm, args: &[Value]) -> Result<Option<Value>, VmError> {
    let target = args[1].as_reference()?;
    let offset = args[2].as_long()?;
    let new = args[3].as_long()?;
    let prev = with_target_slot(vm, target, offset, |slot| match slot {
        SlotView::Single(v) => {
            let p = v.as_long().unwrap_or(0);
            *v = Value::Long(new);
            Ok(p)
        }
        SlotView::LongArr(arr, idx) => {
            let p = arr[idx];
            arr[idx] = new;
            Ok(p)
        }
        _ => Ok(0),
    })?;
    Ok(Some(Value::Long(prev)))
}

fn unsafe_get_and_set_reference(vm: &mut Vm, args: &[Value]) -> Result<Option<Value>, VmError> {
    let target = args[1].as_reference()?;
    let offset = args[2].as_long()?;
    let new = args[3].as_reference()?;
    let prev = with_target_slot(vm, target, offset, |slot| match slot {
        SlotView::Single(v) => {
            let p = v.as_reference().unwrap_or(Reference::Null);
            *v = Value::Reference(new);
            Ok(p)
        }
        SlotView::RefArr(arr, idx) => {
            let p = arr[idx];
            arr[idx] = new;
            Ok(p)
        }
        _ => Ok(Reference::Null),
    })?;
    Ok(Some(Value::Reference(prev)))
}

fn unsafe_get_int(vm: &mut Vm, args: &[Value]) -> Result<Option<Value>, VmError> {
    let target = args[1].as_reference()?;
    let offset = args[2].as_long()?;
    let value = with_target_slot(vm, target, offset, |slot| match slot {
        SlotView::Single(v) => Ok(v.as_int().unwrap_or(0)),
        SlotView::IntArr(arr, idx) => Ok(arr[idx]),
        _ => Ok(0),
    })?;
    Ok(Some(Value::Int(value)))
}

fn unsafe_put_int(vm: &mut Vm, args: &[Value]) -> Result<Option<Value>, VmError> {
    let target = args[1].as_reference()?;
    let offset = args[2].as_long()?;
    let new = args[3].as_int()?;
    with_target_slot(vm, target, offset, |slot| {
        match slot {
            SlotView::Single(v) => *v = Value::Int(new),
            SlotView::IntArr(arr, idx) => arr[idx] = new,
            _ => {}
        }
        Ok(())
    })?;
    Ok(None)
}

fn unsafe_get_long(vm: &mut Vm, args: &[Value]) -> Result<Option<Value>, VmError> {
    let target = args[1].as_reference()?;
    let offset = args[2].as_long()?;
    let value = with_target_slot(vm, target, offset, |slot| match slot {
        SlotView::Single(v) => Ok(v.as_long().unwrap_or(0)),
        SlotView::LongArr(arr, idx) => Ok(arr[idx]),
        _ => Ok(0),
    })?;
    Ok(Some(Value::Long(value)))
}

fn unsafe_put_long(vm: &mut Vm, args: &[Value]) -> Result<Option<Value>, VmError> {
    let target = args[1].as_reference()?;
    let offset = args[2].as_long()?;
    let new = args[3].as_long()?;
    with_target_slot(vm, target, offset, |slot| {
        match slot {
            SlotView::Single(v) => *v = Value::Long(new),
            SlotView::LongArr(arr, idx) => arr[idx] = new,
            _ => {}
        }
        Ok(())
    })?;
    Ok(None)
}

fn unsafe_get_reference(vm: &mut Vm, args: &[Value]) -> Result<Option<Value>, VmError> {
    let target = args[1].as_reference()?;
    let offset = args[2].as_long()?;
    let value = with_target_slot(vm, target, offset, |slot| match slot {
        SlotView::Single(v) => Ok(v.as_reference().unwrap_or(Reference::Null)),
        SlotView::RefArr(arr, idx) => Ok(arr[idx]),
        _ => Ok(Reference::Null),
    })?;
    Ok(Some(Value::Reference(value)))
}

fn unsafe_put_reference(vm: &mut Vm, args: &[Value]) -> Result<Option<Value>, VmError> {
    let target = args[1].as_reference()?;
    let offset = args[2].as_long()?;
    let new = args[3].as_reference()?;
    with_target_slot(vm, target, offset, |slot| {
        match slot {
            SlotView::Single(v) => *v = Value::Reference(new),
            SlotView::RefArr(arr, idx) => arr[idx] = new,
            _ => {}
        }
        Ok(())
    })?;
    Ok(None)
}
