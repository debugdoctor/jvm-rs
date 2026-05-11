//! Native handlers for `java.lang.invoke.MethodHandles.*` and
//! `java.lang.invoke.MethodHandle.{bindTo, asType, asCollector, asSpreader,
//! asVarargsCollector, type}` adapter factories. Each handler validates the
//! incoming Java-level arguments, computes the post-transformation descriptor
//! per JVMS, and allocates a derived MethodHandle via
//! `Vm::allocate_derived_method_handle`.

use crate::vm::types::{parse_arg_count, parse_return_type};
use crate::vm::{HeapValue, Reference, Value, Vm, VmError};

/// Try to handle a `MethodHandle.*` / `MethodHandles.*` combinator factory
/// native call. Returns `Ok(Some(_))` on a match (even for void returns where
/// the inner value is `None`), `Ok(None)` if the call doesn't match any
/// factory in this table.
pub(super) fn try_invoke_combinator(
    vm: &mut Vm,
    class_name: &str,
    method_name: &str,
    descriptor: &str,
    args: &[Value],
) -> Result<Option<Option<Value>>, VmError> {
    match (class_name, method_name, descriptor) {
        // ---- MethodHandle.bindTo(Object) ----
        ("java/lang/invoke/MethodHandle", "bindTo", "(Ljava/lang/Object;)Ljava/lang/invoke/MethodHandle;") => {
            let inner = args[0].as_reference()?;
            let bind = args[1];
            let inner_desc = mh_descriptor(vm, inner)?;
            let new_desc = drop_leading_param(&inner_desc)?;
            let mh = vm.allocate_derived_method_handle(
                crate::vm::MH_KIND_BIND_TO,
                &new_desc,
                vec![("__inner", Value::Reference(inner)), ("__bindArg", bind)],
            )?;
            Ok(Some(Some(Value::Reference(mh))))
        }

        // ---- MethodHandle.asType(MethodType) ----
        ("java/lang/invoke/MethodHandle", "asType", "(Ljava/lang/invoke/MethodType;)Ljava/lang/invoke/MethodHandle;") => {
            let inner = args[0].as_reference()?;
            let new_type = args[1].as_reference()?;
            let new_desc = method_type_descriptor(vm, new_type)?;
            let inner_desc = mh_descriptor(vm, inner)?;
            validate_as_type_arity(&inner_desc, &new_desc)?;
            // Validate primitive arity compatibility at creation time.
            let (inner_args, inner_ret) =
                Vm::split_method_descriptor_public(&inner_desc)
                    .ok_or(VmError::InvalidDescriptor { descriptor: inner_desc.clone() })?;
            let (new_args, new_ret) = Vm::split_method_descriptor_public(&new_desc)
                .ok_or(VmError::InvalidDescriptor { descriptor: new_desc.clone() })?;
            for (a, b) in new_args.iter().zip(inner_args.iter()) {
                validate_as_type_pair(a, b)?;
            }
            if new_ret != "V" && inner_ret != "V" {
                validate_as_type_pair(&inner_ret, &new_ret)?;
            }
            let mh = vm.allocate_derived_method_handle(
                crate::vm::MH_KIND_AS_TYPE,
                &new_desc,
                vec![("__inner", Value::Reference(inner))],
            )?;
            Ok(Some(Some(Value::Reference(mh))))
        }

        // ---- MethodHandle.asCollector(Class, int) ----
        (
            "java/lang/invoke/MethodHandle",
            "asCollector",
            "(Ljava/lang/Class;I)Ljava/lang/invoke/MethodHandle;",
        ) => {
            let inner = args[0].as_reference()?;
            let array_class = args[1].as_reference()?;
            let count = args[2].as_int()?;
            let component = array_component_descriptor(vm, array_class)?;
            let inner_desc = mh_descriptor(vm, inner)?;
            let new_desc =
                collector_descriptor(&inner_desc, &component, count as usize, false)?;
            let pos = parse_arg_count(&inner_desc)?.saturating_sub(1);
            let mh = vm.allocate_derived_method_handle(
                crate::vm::MH_KIND_AS_COLLECTOR,
                &new_desc,
                vec![
                    ("__inner", Value::Reference(inner)),
                    ("__collectPos", Value::Int(pos as i32)),
                    ("__collectCount", Value::Int(count)),
                    ("__collectComponent", Value::Reference(array_class)),
                    ("__isVarargs", Value::Int(0)),
                ],
            )?;
            Ok(Some(Some(Value::Reference(mh))))
        }

        // ---- MethodHandle.asVarargsCollector(Class) ----
        (
            "java/lang/invoke/MethodHandle",
            "asVarargsCollector",
            "(Ljava/lang/Class;)Ljava/lang/invoke/MethodHandle;",
        ) => {
            let inner = args[0].as_reference()?;
            let array_class = args[1].as_reference()?;
            let component = array_component_descriptor(vm, array_class)?;
            let inner_desc = mh_descriptor(vm, inner)?;
            let arity = parse_arg_count(&inner_desc)?;
            // Position of the trailing array arg in the inner handle.
            let pos = arity.saturating_sub(1);
            // The collector descriptor matches the inner descriptor (the trailing
            // array param stays, varargs reshape happens at invocation time).
            let new_desc = inner_desc.clone();
            // Count of params before the trailing array.
            let mh = vm.allocate_derived_method_handle(
                crate::vm::MH_KIND_AS_COLLECTOR,
                &new_desc,
                vec![
                    ("__inner", Value::Reference(inner)),
                    ("__collectPos", Value::Int(pos as i32)),
                    ("__collectCount", Value::Int(0)), // not used in varargs mode
                    ("__collectComponent", Value::Reference(array_class)),
                    ("__isVarargs", Value::Int(1)),
                ],
            )?;
            let _ = component;
            Ok(Some(Some(Value::Reference(mh))))
        }

        // ---- MethodHandle.asSpreader(Class, int) ----
        (
            "java/lang/invoke/MethodHandle",
            "asSpreader",
            "(Ljava/lang/Class;I)Ljava/lang/invoke/MethodHandle;",
        ) => {
            let inner = args[0].as_reference()?;
            let array_class = args[1].as_reference()?;
            let count = args[2].as_int()?;
            let component = array_component_descriptor(vm, array_class)?;
            let inner_desc = mh_descriptor(vm, inner)?;
            let new_desc =
                spreader_descriptor(&inner_desc, &component, count as usize)?;
            let arity = parse_arg_count(&inner_desc)?;
            let pos = arity.saturating_sub(count as usize);
            let mh = vm.allocate_derived_method_handle(
                crate::vm::MH_KIND_AS_SPREADER,
                &new_desc,
                vec![
                    ("__inner", Value::Reference(inner)),
                    ("__spreadPos", Value::Int(pos as i32)),
                    ("__spreadCount", Value::Int(count)),
                    ("__spreadComponent", Value::Reference(array_class)),
                ],
            )?;
            Ok(Some(Some(Value::Reference(mh))))
        }

        // ---- MethodHandle.type() ----
        (
            "java/lang/invoke/MethodHandle",
            "type",
            "()Ljava/lang/invoke/MethodType;",
        ) => {
            let inner = args[0].as_reference()?;
            let desc = mh_descriptor(vm, inner)?;
            let mt = vm.allocate_bootstrap_method_type(&desc)?;
            Ok(Some(Some(Value::Reference(mt))))
        }

        // ---- MethodHandles.insertArguments(MethodHandle,int,Object[]) ----
        (
            "java/lang/invoke/MethodHandles",
            "insertArguments",
            "(Ljava/lang/invoke/MethodHandle;I[Ljava/lang/Object;)Ljava/lang/invoke/MethodHandle;",
        ) => {
            let inner = args[0].as_reference()?;
            let pos = args[1].as_int()? as usize;
            let inserts_array = args[2].as_reference()?;
            let inserts = read_object_array(vm, inserts_array)?;
            let inner_desc = mh_descriptor(vm, inner)?;
            let new_desc = drop_params(&inner_desc, pos, inserts.len())?;
            // Store inserts in a Reference[] (component Ljava/lang/Object;).
            let inserts_obj = vm
                .heap
                .lock()
                .unwrap()
                .allocate(HeapValue::ReferenceArray {
                    component_type: "Ljava/lang/Object;".to_string(),
                    values: inserts.iter().map(|r| *r).collect(),
                });
            // Reuse a fresh insertArgs storage but as raw Values (we want to
            // preserve primitive types where possible). When the bind comes
            // from Object[] we only have references; matchups happen at
            // invocation time via `__insertArgs`.
            let mh = vm.allocate_derived_method_handle(
                crate::vm::MH_KIND_INSERT_ARGUMENTS,
                &new_desc,
                vec![
                    ("__inner", Value::Reference(inner)),
                    ("__insertPos", Value::Int(pos as i32)),
                    ("__insertArgs", Value::Reference(inserts_obj)),
                ],
            )?;
            Ok(Some(Some(Value::Reference(mh))))
        }

        // ---- MethodHandles.dropArguments(MethodHandle, int, Class[]) ----
        (
            "java/lang/invoke/MethodHandles",
            "dropArguments",
            "(Ljava/lang/invoke/MethodHandle;I[Ljava/lang/Class;)Ljava/lang/invoke/MethodHandle;",
        ) => {
            let inner = args[0].as_reference()?;
            let pos = args[1].as_int()? as usize;
            let classes_ref = args[2].as_reference()?;
            let classes = read_class_array_descriptors(vm, classes_ref)?;
            let inner_desc = mh_descriptor(vm, inner)?;
            let new_desc = insert_params(&inner_desc, pos, &classes)?;
            let mh = vm.allocate_derived_method_handle(
                crate::vm::MH_KIND_DROP_ARGUMENTS,
                &new_desc,
                vec![
                    ("__inner", Value::Reference(inner)),
                    ("__dropPos", Value::Int(pos as i32)),
                    ("__dropCount", Value::Int(classes.len() as i32)),
                ],
            )?;
            Ok(Some(Some(Value::Reference(mh))))
        }

        // ---- MethodHandles.permuteArguments(MethodHandle, MethodType, int[]) ----
        (
            "java/lang/invoke/MethodHandles",
            "permuteArguments",
            "(Ljava/lang/invoke/MethodHandle;Ljava/lang/invoke/MethodType;[I)Ljava/lang/invoke/MethodHandle;",
        ) => {
            let inner = args[0].as_reference()?;
            let new_type = args[1].as_reference()?;
            let perm_ref = args[2].as_reference()?;
            let new_desc = method_type_descriptor(vm, new_type)?;
            let mh = vm.allocate_derived_method_handle(
                crate::vm::MH_KIND_PERMUTE_ARGUMENTS,
                &new_desc,
                vec![
                    ("__inner", Value::Reference(inner)),
                    ("__permute", Value::Reference(perm_ref)),
                ],
            )?;
            Ok(Some(Some(Value::Reference(mh))))
        }

        // ---- MethodHandles.filterArguments(MethodHandle, int, MethodHandle[]) ----
        (
            "java/lang/invoke/MethodHandles",
            "filterArguments",
            "(Ljava/lang/invoke/MethodHandle;I[Ljava/lang/invoke/MethodHandle;)Ljava/lang/invoke/MethodHandle;",
        ) => {
            let inner = args[0].as_reference()?;
            let pos = args[1].as_int()? as usize;
            let filters_ref = args[2].as_reference()?;
            let filters = read_object_array(vm, filters_ref)?;
            let inner_desc = mh_descriptor(vm, inner)?;
            let new_desc =
                filter_arguments_descriptor(vm, &inner_desc, pos, &filters)?;
            let mh = vm.allocate_derived_method_handle(
                crate::vm::MH_KIND_FILTER_ARGUMENTS,
                &new_desc,
                vec![
                    ("__inner", Value::Reference(inner)),
                    ("__filterPos", Value::Int(pos as i32)),
                    ("__filterHandles", Value::Reference(filters_ref)),
                ],
            )?;
            Ok(Some(Some(Value::Reference(mh))))
        }

        // ---- MethodHandles.filterReturnValue(MethodHandle, MethodHandle) ----
        (
            "java/lang/invoke/MethodHandles",
            "filterReturnValue",
            "(Ljava/lang/invoke/MethodHandle;Ljava/lang/invoke/MethodHandle;)Ljava/lang/invoke/MethodHandle;",
        ) => {
            let inner = args[0].as_reference()?;
            let filter = args[1].as_reference()?;
            let inner_desc = mh_descriptor(vm, inner)?;
            let filter_desc = mh_descriptor(vm, filter)?;
            let new_desc = replace_return_type(&inner_desc, &filter_desc)?;
            let mh = vm.allocate_derived_method_handle(
                crate::vm::MH_KIND_FILTER_RETURN,
                &new_desc,
                vec![
                    ("__inner", Value::Reference(inner)),
                    ("__retFilter", Value::Reference(filter)),
                ],
            )?;
            Ok(Some(Some(Value::Reference(mh))))
        }

        // ---- MethodHandles.foldArguments(MethodHandle, MethodHandle) ----
        (
            "java/lang/invoke/MethodHandles",
            "foldArguments",
            "(Ljava/lang/invoke/MethodHandle;Ljava/lang/invoke/MethodHandle;)Ljava/lang/invoke/MethodHandle;",
        ) => {
            let inner = args[0].as_reference()?;
            let combiner = args[1].as_reference()?;
            let inner_desc = mh_descriptor(vm, inner)?;
            let combiner_desc = mh_descriptor(vm, combiner)?;
            let new_desc = fold_descriptor(&inner_desc, &combiner_desc, 0)?;
            let mh = vm.allocate_derived_method_handle(
                crate::vm::MH_KIND_FOLD_ARGUMENTS,
                &new_desc,
                vec![
                    ("__inner", Value::Reference(inner)),
                    ("__foldCombiner", Value::Reference(combiner)),
                    ("__foldPos", Value::Int(0)),
                ],
            )?;
            Ok(Some(Some(Value::Reference(mh))))
        }

        // ---- MethodHandles.guardWithTest(MethodHandle, MethodHandle, MethodHandle) ----
        (
            "java/lang/invoke/MethodHandles",
            "guardWithTest",
            "(Ljava/lang/invoke/MethodHandle;Ljava/lang/invoke/MethodHandle;Ljava/lang/invoke/MethodHandle;)Ljava/lang/invoke/MethodHandle;",
        ) => {
            let test = args[0].as_reference()?;
            let target = args[1].as_reference()?;
            let fallback = args[2].as_reference()?;
            let target_desc = mh_descriptor(vm, target)?;
            let mh = vm.allocate_derived_method_handle(
                crate::vm::MH_KIND_GUARD_WITH_TEST,
                &target_desc,
                vec![
                    ("__guardTest", Value::Reference(test)),
                    ("__guardTarget", Value::Reference(target)),
                    ("__guardFallback", Value::Reference(fallback)),
                ],
            )?;
            Ok(Some(Some(Value::Reference(mh))))
        }

        // ---- MethodHandles.catchException(MethodHandle, Class, MethodHandle) ----
        (
            "java/lang/invoke/MethodHandles",
            "catchException",
            "(Ljava/lang/invoke/MethodHandle;Ljava/lang/Class;Ljava/lang/invoke/MethodHandle;)Ljava/lang/invoke/MethodHandle;",
        ) => {
            let target = args[0].as_reference()?;
            let exc_class = args[1].as_reference()?;
            let handler = args[2].as_reference()?;
            let target_desc = mh_descriptor(vm, target)?;
            let mh = vm.allocate_derived_method_handle(
                crate::vm::MH_KIND_CATCH_EXCEPTION,
                &target_desc,
                vec![
                    ("__guardTarget", Value::Reference(target)),
                    ("__catchType", Value::Reference(exc_class)),
                    ("__catchHandler", Value::Reference(handler)),
                ],
            )?;
            Ok(Some(Some(Value::Reference(mh))))
        }

        // ---- MethodHandles.identity(Class) ----
        (
            "java/lang/invoke/MethodHandles",
            "identity",
            "(Ljava/lang/Class;)Ljava/lang/invoke/MethodHandle;",
        ) => {
            let class_ref = args[0].as_reference()?;
            let class_internal =
                crate::vm::builtin::helpers::class_internal_name(vm, class_ref)?;
            let descriptor = class_to_descriptor(&class_internal);
            let target_desc = format!("({}){}", descriptor, descriptor);
            // Build (T)T as permute(perm=[0]) on a (T,T) target — simplest is to
            // use a derived "filter" handle: we emit a permute that picks arg 0
            // from a (T)T input.
            let perm_ref = vm
                .heap
                .lock()
                .unwrap()
                .allocate_int_array(vec![0]);
            // Inner is a dummy that just returns arg 0; we synthesise via
            // permuteArguments on an identity-shaped handle by routing through
            // kind 13 (permute) over an inner that returns its single arg —
            // which we model as a kind-0 (constant) handle whose value comes
            // from arg 0. Simpler: use kind-0 with a sentinel that's overridden
            // by the dispatcher. We instead emit a `permute` adapter whose
            // inner is a synthetic identity (kind=0 with __constantValue = arg 0).
            //
            // The pragmatic implementation: directly emit a `kind=13 permute`
            // adapter wrapping a kind-0 handle, but the kind-0 dispatch returns
            // its `__constantValue` — which we can't set per-call. To keep
            // semantics simple we instead implement identity as a kind-22
            // exact-invoker shape: callers normally won't use identity outside
            // of compositions, but we handle them by short-circuiting in
            // `invoke_derived_method_handle` via a dedicated marker.
            //
            // For M2 we represent identity as a kind=14 (asType) over a stub
            // that already preserves args: we emit a kind=0 with
            // __constantValue=null and rely on coercion arity check failing.
            //
            // Easiest correct path: emit a synthetic combinator that simply
            // returns its first arg. We model it as `permuteArguments` (kind 13)
            // with permutation [0], inner = a no-arg handle that's never invoked
            // for the no-arg case... too convoluted. Just emit a new MH_KIND_IDENTITY.
            let _ = perm_ref;
            let mh = vm.allocate_derived_method_handle(
                crate::vm::MH_KIND_INVOKER, // reuse invoker dispatch with __invokerKind=4
                &target_desc,
                vec![
                    (
                        "__invokerKind",
                        Value::Int(crate::vm::MH_INVOKER_IDENTITY),
                    ),
                ],
            )?;
            Ok(Some(Some(Value::Reference(mh))))
        }

        // ---- MethodHandles.zero(Class) ----
        (
            "java/lang/invoke/MethodHandles",
            "zero",
            "(Ljava/lang/Class;)Ljava/lang/invoke/MethodHandle;",
        ) => {
            let class_ref = args[0].as_reference()?;
            let class_internal =
                crate::vm::builtin::helpers::class_internal_name(vm, class_ref)?;
            let descriptor = class_to_descriptor(&class_internal);
            let target_desc = format!("(){}", descriptor);
            let zero_value = match descriptor.as_str() {
                "I" | "B" | "C" | "S" | "Z" => Value::Int(0),
                "J" => Value::Long(0),
                "F" => Value::Float(0.0),
                "D" => Value::Double(0.0),
                _ => Value::Reference(Reference::Null),
            };
            // Constant handle (kind 0) returns its __constantValue from
            // invoke_method_handle.
            let mh = vm.allocate_bootstrap_method_handle(
                0,
                "",
                "",
                &target_desc,
                Some(zero_value),
            )?;
            Ok(Some(Some(Value::Reference(mh))))
        }

        // ---- MethodHandles.empty(MethodType) ----
        (
            "java/lang/invoke/MethodHandles",
            "empty",
            "(Ljava/lang/invoke/MethodType;)Ljava/lang/invoke/MethodHandle;",
        ) => {
            let mt = args[0].as_reference()?;
            let desc = method_type_descriptor(vm, mt)?;
            let ret = parse_return_type(&desc)?;
            let zero_value = match ret {
                Some(b'I') | Some(b'B') | Some(b'C') | Some(b'S') | Some(b'Z') => Value::Int(0),
                Some(b'J') => Value::Long(0),
                Some(b'F') => Value::Float(0.0),
                Some(b'D') => Value::Double(0.0),
                _ => Value::Reference(Reference::Null),
            };
            let mh = vm.allocate_bootstrap_method_handle(
                0,
                "",
                "",
                &desc,
                Some(zero_value),
            )?;
            Ok(Some(Some(Value::Reference(mh))))
        }

        // ---- MethodHandles.exactInvoker(MethodType) ----
        (
            "java/lang/invoke/MethodHandles",
            "exactInvoker",
            "(Ljava/lang/invoke/MethodType;)Ljava/lang/invoke/MethodHandle;",
        )
        | (
            "java/lang/invoke/MethodHandles",
            "invoker",
            "(Ljava/lang/invoke/MethodType;)Ljava/lang/invoke/MethodHandle;",
        ) => {
            let mt = args[0].as_reference()?;
            let inner_desc = method_type_descriptor(vm, mt)?;
            // Outer descriptor prepends a MethodHandle param.
            let outer_desc = prepend_mh_param(&inner_desc)?;
            let kind = if method_name == "exactInvoker" {
                crate::vm::MH_INVOKER_EXACT
            } else {
                crate::vm::MH_INVOKER_GENERIC
            };
            let mh = vm.allocate_derived_method_handle(
                crate::vm::MH_KIND_INVOKER,
                &outer_desc,
                vec![("__invokerKind", Value::Int(kind))],
            )?;
            Ok(Some(Some(Value::Reference(mh))))
        }

        // ---- MethodHandles.spreadInvoker(MethodType, int) ----
        (
            "java/lang/invoke/MethodHandles",
            "spreadInvoker",
            "(Ljava/lang/invoke/MethodType;I)Ljava/lang/invoke/MethodHandle;",
        ) => {
            let mt = args[0].as_reference()?;
            let leading = args[1].as_int()?;
            let inner_desc = method_type_descriptor(vm, mt)?;
            // Outer descriptor: (MethodHandle, leadingArgs..., Object[]) -> ret.
            let outer_desc =
                spread_invoker_descriptor(&inner_desc, leading as usize)?;
            let inner_arity = parse_arg_count(&inner_desc)?;
            let spread_count = inner_arity.saturating_sub(leading as usize);
            let mh = vm.allocate_derived_method_handle(
                crate::vm::MH_KIND_INVOKER,
                &outer_desc,
                vec![
                    ("__invokerKind", Value::Int(crate::vm::MH_INVOKER_SPREAD)),
                    ("__spreadArrayCount", Value::Int(spread_count as i32)),
                ],
            )?;
            Ok(Some(Some(Value::Reference(mh))))
        }

        _ => Ok(None),
    }
}

// ---------------- internal helpers ----------------

fn mh_descriptor(vm: &mut Vm, handle: Reference) -> Result<String, VmError> {
    let desc_ref = vm
        .get_object_field(handle, "__targetDesc")?
        .as_reference()?;
    if desc_ref == Reference::Null {
        return Ok(String::new());
    }
    let s = vm.get_object_field(desc_ref, "__descriptor")?.as_reference()?;
    if s == Reference::Null {
        return Ok(String::new());
    }
    crate::vm::builtin::helpers::stringify_reference(vm, s)
}

fn method_type_descriptor(vm: &mut Vm, method_type: Reference) -> Result<String, VmError> {
    if method_type == Reference::Null {
        return Ok(String::new());
    }
    let s = vm
        .get_object_field(method_type, "__descriptor")?
        .as_reference()?;
    crate::vm::builtin::helpers::stringify_reference(vm, s)
}

fn array_component_descriptor(vm: &mut Vm, class_ref: Reference) -> Result<String, VmError> {
    let name = crate::vm::builtin::helpers::class_internal_name(vm, class_ref)?;
    if let Some(stripped) = name.strip_prefix('[') {
        Ok(stripped.to_string())
    } else {
        // Single-dim primitive-array Class objects use names like "[I" but if
        // we got a non-array class, fall back to its own descriptor form.
        Ok(class_to_descriptor(&name))
    }
}

fn class_to_descriptor(internal: &str) -> String {
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

fn read_object_array(vm: &mut Vm, reference: Reference) -> Result<Vec<Reference>, VmError> {
    if reference == Reference::Null {
        return Ok(Vec::new());
    }
    match vm.heap.lock().unwrap().get(reference)? {
        HeapValue::ReferenceArray { values, .. } => Ok(values.clone()),
        _ => Err(VmError::InvalidHeapValue {
            expected: "reference-array",
            actual: "other",
        }),
    }
}

fn read_class_array_descriptors(
    vm: &mut Vm,
    reference: Reference,
) -> Result<Vec<String>, VmError> {
    let refs = read_object_array(vm, reference)?;
    let mut out = Vec::with_capacity(refs.len());
    for r in refs {
        if r == Reference::Null {
            out.push("Ljava/lang/Object;".to_string());
        } else {
            let n = crate::vm::builtin::helpers::class_internal_name(vm, r)?;
            out.push(class_to_descriptor(&n));
        }
    }
    Ok(out)
}

fn drop_leading_param(descriptor: &str) -> Result<String, VmError> {
    let (mut params, ret) = split(descriptor)?;
    if params.is_empty() {
        return Err(VmError::InvalidDescriptor {
            descriptor: descriptor.to_string(),
        });
    }
    params.remove(0);
    Ok(format!("({}){}", params.join(""), ret))
}

fn drop_params(descriptor: &str, pos: usize, count: usize) -> Result<String, VmError> {
    let (mut params, ret) = split(descriptor)?;
    if pos + count > params.len() {
        return Err(VmError::InvalidDescriptor {
            descriptor: descriptor.to_string(),
        });
    }
    params.drain(pos..pos + count);
    Ok(format!("({}){}", params.join(""), ret))
}

fn insert_params(descriptor: &str, pos: usize, types: &[String]) -> Result<String, VmError> {
    let (mut params, ret) = split(descriptor)?;
    if pos > params.len() {
        return Err(VmError::InvalidDescriptor {
            descriptor: descriptor.to_string(),
        });
    }
    for (i, t) in types.iter().enumerate() {
        params.insert(pos + i, t.clone());
    }
    Ok(format!("({}){}", params.join(""), ret))
}

fn collector_descriptor(
    inner: &str,
    component: &str,
    count: usize,
    _varargs: bool,
) -> Result<String, VmError> {
    let (mut params, ret) = split(inner)?;
    if params.is_empty() {
        return Err(VmError::InvalidDescriptor { descriptor: inner.to_string() });
    }
    // Replace the trailing array param with `count` copies of `component`.
    let _trailing = params.pop().unwrap();
    for _ in 0..count {
        params.push(component.to_string());
    }
    Ok(format!("({}){}", params.join(""), ret))
}

fn spreader_descriptor(
    inner: &str,
    component: &str,
    count: usize,
) -> Result<String, VmError> {
    let (mut params, ret) = split(inner)?;
    if count > params.len() {
        return Err(VmError::InvalidDescriptor { descriptor: inner.to_string() });
    }
    let drop_start = params.len() - count;
    params.drain(drop_start..);
    params.push(format!("[{}", component));
    Ok(format!("({}){}", params.join(""), ret))
}

fn filter_arguments_descriptor(
    vm: &mut Vm,
    inner: &str,
    pos: usize,
    filters: &[Reference],
) -> Result<String, VmError> {
    let (mut params, ret) = split(inner)?;
    for (i, filter) in filters.iter().enumerate() {
        if *filter == Reference::Null {
            continue;
        }
        let fdesc = mh_descriptor(vm, *filter)?;
        let (filter_args, _) = Vm::split_method_descriptor_public(&fdesc)
            .ok_or(VmError::InvalidDescriptor { descriptor: fdesc.clone() })?;
        if filter_args.len() != 1 {
            return Err(VmError::InvalidDescriptor { descriptor: fdesc });
        }
        if pos + i < params.len() {
            params[pos + i] = filter_args[0].clone();
        }
    }
    Ok(format!("({}){}", params.join(""), ret))
}

fn replace_return_type(inner: &str, filter: &str) -> Result<String, VmError> {
    let (params, _) = split(inner)?;
    let (_, new_ret) = split(filter)?;
    Ok(format!("({}){}", params.join(""), new_ret))
}

fn fold_descriptor(
    inner: &str,
    combiner: &str,
    pos: usize,
) -> Result<String, VmError> {
    let (mut params, ret) = split(inner)?;
    let combiner_ret = parse_return_type(combiner)?;
    if combiner_ret.is_some() {
        if pos >= params.len() {
            return Err(VmError::InvalidDescriptor {
                descriptor: inner.to_string(),
            });
        }
        params.remove(pos);
    }
    Ok(format!("({}){}", params.join(""), ret))
}

fn prepend_mh_param(inner: &str) -> Result<String, VmError> {
    let (mut params, ret) = split(inner)?;
    params.insert(0, "Ljava/lang/invoke/MethodHandle;".to_string());
    Ok(format!("({}){}", params.join(""), ret))
}

fn spread_invoker_descriptor(inner: &str, leading: usize) -> Result<String, VmError> {
    let (mut params, ret) = split(inner)?;
    if leading > params.len() {
        return Err(VmError::InvalidDescriptor { descriptor: inner.to_string() });
    }
    let mut new_params: Vec<String> = Vec::new();
    new_params.push("Ljava/lang/invoke/MethodHandle;".to_string());
    new_params.extend(params.drain(..leading));
    new_params.push("[Ljava/lang/Object;".to_string());
    Ok(format!("({}){}", new_params.join(""), ret))
}

fn validate_as_type_arity(inner: &str, new: &str) -> Result<(), VmError> {
    let a = parse_arg_count(inner)?;
    let b = parse_arg_count(new)?;
    if a != b {
        return Err(VmError::UnhandledException {
            class_name: "java/lang/invoke/WrongMethodTypeException".to_string(),
        });
    }
    Ok(())
}

fn validate_as_type_pair(from: &str, to: &str) -> Result<(), VmError> {
    if from == to {
        return Ok(());
    }
    let from_byte = from.as_bytes().first().copied().unwrap_or(b'?');
    let to_byte = to.as_bytes().first().copied().unwrap_or(b'?');
    let from_prim = matches!(
        from_byte,
        b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z' | b'V'
    );
    let to_prim = matches!(
        to_byte,
        b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z' | b'V'
    );
    if from_prim && to_prim {
        // Only widening allowed at creation time. Use the same table as
        // `widen_primitive` for symmetry.
        let ok = matches!(
            (from_byte, to_byte),
            (b'B' | b'C' | b'S' | b'Z' | b'I', b'I' | b'J' | b'F' | b'D')
                | (b'J', b'F' | b'D')
                | (b'F', b'D')
        );
        if !ok {
            return Err(VmError::UnhandledException {
                class_name: "java/lang/invoke/WrongMethodTypeException".to_string(),
            });
        }
    }
    Ok(())
}

fn split(descriptor: &str) -> Result<(Vec<String>, String), VmError> {
    Vm::split_method_descriptor_public(descriptor)
        .ok_or(VmError::InvalidDescriptor { descriptor: descriptor.to_string() })
}
