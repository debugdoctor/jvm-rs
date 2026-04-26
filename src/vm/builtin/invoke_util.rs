use std::collections::HashMap;

use crate::vm::{HeapValue, Reference, Value, Vm, VmError};

pub(super) fn invoke_util(
    vm: &mut Vm,
    class_name: &str,
    method_name: &str,
    descriptor: &str,
    args: &[Value],
) -> Result<Option<Value>, VmError> {
    match (class_name, method_name, descriptor) {
        ("java/util/Objects", "requireNonNull", "(Ljava/lang/Object;)Ljava/lang/Object;") => {
            let r = args[0].as_reference()?;
            if r == Reference::Null {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/NullPointerException".to_string(),
                });
            }
            Ok(Some(Value::Reference(r)))
        }
        ("java/util/Objects", "requireNonNull", "(Ljava/lang/Object;Ljava/lang/String;)Ljava/lang/Object;") => {
            let r = args[0].as_reference()?;
            if r == Reference::Null {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/NullPointerException".to_string(),
                });
            }
            Ok(Some(Value::Reference(r)))
        }
        ("java/util/Objects", "equals", "(Ljava/lang/Object;Ljava/lang/Object;)Z") => {
            let a = args[0].as_reference()?;
            let b = args[1].as_reference()?;
            let eq = match (a, b) {
                (Reference::Null, Reference::Null) => true,
                (Reference::Null, _) | (_, Reference::Null) => false,
                _ => {
                    match (
                        crate::vm::builtin::helpers::stringify_reference(vm, a).ok(),
                        crate::vm::builtin::helpers::stringify_reference(vm, b).ok(),
                    ) {
                        (Some(sa), Some(sb)) => sa == sb,
                        _ => a == b,
                    }
                }
            };
            Ok(Some(Value::Int(if eq { 1 } else { 0 })))
        }
        ("java/util/Objects", "isNull", "(Ljava/lang/Object;)Z") => {
            let r = args[0].as_reference()?;
            Ok(Some(Value::Int(if r == Reference::Null { 1 } else { 0 })))
        }
        ("java/util/Objects", "nonNull", "(Ljava/lang/Object;)Z") => {
            let r = args[0].as_reference()?;
            Ok(Some(Value::Int(if r == Reference::Null { 0 } else { 1 })))
        }
        ("java/util/Objects", "hash", "([Ljava/lang/Object;)I") => {
            let arr_ref = args[0].as_reference()?;
            let hash = crate::vm::builtin::helpers::hash_array_elements(vm, arr_ref)?;
            Ok(Some(Value::Int(hash)))
        }
        ("java/util/Objects", "hashCode", "(Ljava/lang/Object;)I") => {
            let r = args[0].as_reference()?;
            if r == Reference::Null {
                Ok(Some(Value::Int(0)))
            } else {
                Ok(Some(Value::Int(crate::vm::builtin::helpers::hash_object(vm, r))))
            }
        }
        ("java/util/Objects", "checkIndex", "(II)I") => {
            let index = args[0].as_int()?;
            let length = args[1].as_int()?;
            if index < 0 || index >= length {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/ArrayIndexOutOfBoundsException".to_string(),
                });
            }
            Ok(Some(Value::Int(index)))
        }
        ("java/util/Objects", "checkIndex", "(JJ)J") => {
            let index = args[0].as_long()?;
            let length = args[1].as_long()?;
            if index < 0 || index >= length {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/ArrayIndexOutOfBoundsException".to_string(),
                });
            }
            Ok(Some(Value::Long(index)))
        }
        ("java/util/Objects", "checkFromToIndex", "(III)I") => {
            let from = args[0].as_int()?;
            let to = args[1].as_int()?;
            let length = args[2].as_int()?;
            if from < 0 || to > length || from > to {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/IndexOutOfBoundsException".to_string(),
                });
            }
            Ok(Some(Value::Int(from)))
        }
        ("java/util/Objects", "checkFromToIndex", "(JJJ)J") => {
            let from = args[0].as_long()?;
            let to = args[1].as_long()?;
            let length = args[2].as_long()?;
            if from < 0 || to > length || from > to {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/IndexOutOfBoundsException".to_string(),
                });
            }
            Ok(Some(Value::Long(from)))
        }
        ("java/util/Objects", "checkFromIndexSize", "(III)I") => {
            let from = args[0].as_int()?;
            let size = args[1].as_int()?;
            let length = args[2].as_int()?;
            if from < 0 || size < 0 || from > length - size {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/IndexOutOfBoundsException".to_string(),
                });
            }
            Ok(Some(Value::Int(from)))
        }
        ("java/util/Objects", "checkFromIndexSize", "(JJJ)J") => {
            let from = args[0].as_long()?;
            let size = args[1].as_long()?;
            let length = args[2].as_long()?;
            if from < 0 || size < 0 || from > length - size {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/IndexOutOfBoundsException".to_string(),
                });
            }
            Ok(Some(Value::Long(from)))
        }
        ("java/util/Arrays", "equals", "([I[I)Z") => {
            Ok(Some(Value::Int(i32::from(
                crate::vm::builtin::helpers::native_arrays_equals_int(vm, args[0].as_reference()?, args[1].as_reference()?)?,
            ))))
        }
        ("java/util/Arrays", "equals", "([J[J)Z") => {
            Ok(Some(Value::Int(i32::from(
                crate::vm::builtin::helpers::native_arrays_equals_long(vm, args[0].as_reference()?, args[1].as_reference()?)?,
            ))))
        }
        ("java/util/Arrays", "equals", "([B[B)Z")
        | ("java/util/Arrays", "equals", "([S[S)Z")
        | ("java/util/Arrays", "equals", "([C[C)Z")
        | ("java/util/Arrays", "equals", "([Z[Z)Z") => {
            Ok(Some(Value::Int(i32::from(
                crate::vm::builtin::helpers::native_arrays_equals_int(vm, args[0].as_reference()?, args[1].as_reference()?)?,
            ))))
        }
        ("java/util/Arrays", "equals", "([F[F)Z") => {
            Ok(Some(Value::Int(i32::from(
                crate::vm::builtin::helpers::native_arrays_equals_float(vm, args[0].as_reference()?, args[1].as_reference()?)?,
            ))))
        }
        ("java/util/Arrays", "equals", "([D[D)Z") => {
            Ok(Some(Value::Int(i32::from(
                crate::vm::builtin::helpers::native_arrays_equals_double(vm, args[0].as_reference()?, args[1].as_reference()?)?,
            ))))
        }
        ("java/util/Arrays", "equals", "([Ljava/lang/Object;[Ljava/lang/Object;)Z") => {
            Ok(Some(Value::Int(i32::from(
                crate::vm::builtin::helpers::native_arrays_equals_ref(vm, args[0].as_reference()?, args[1].as_reference()?)?,
            ))))
        }
        ("java/util/Arrays", "stream", "([I)Ljava/util/stream/IntStream;") => {
            let array_ref = args[0].as_reference()?;
            let mut fields = HashMap::new();
            fields.insert("__array".to_string(), Value::Reference(array_ref));
            let r = vm.heap.lock().unwrap().allocate(HeapValue::Object {
                class_name: "__jvm_rs/NativeIntStream".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(r)))
        }
        ("java/util/Arrays", "stream", "([J)Ljava/util/stream/LongStream;") => {
            let array_ref = args[0].as_reference()?;
            let mut fields = HashMap::new();
            fields.insert("__array".to_string(), Value::Reference(array_ref));
            let r = vm.heap.lock().unwrap().allocate(HeapValue::Object {
                class_name: "__jvm_rs/NativeLongStream".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(r)))
        }
        ("java/util/Arrays", "stream", "([D)Ljava/util/stream/DoubleStream;") => {
            let array_ref = args[0].as_reference()?;
            let mut fields = HashMap::new();
            fields.insert("__array".to_string(), Value::Reference(array_ref));
            let r = vm.heap.lock().unwrap().allocate(HeapValue::Object {
                class_name: "__jvm_rs/NativeDoubleStream".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(r)))
        }
        ("__jvm_rs/NativeLongStream", "sum", "()J") => {
            let array = crate::vm::builtin::helpers::native_long_stream_array(vm, args[0].as_reference()?)?;
            let mut heap = vm.heap.lock().unwrap();
            if let HeapValue::LongArray { values } = heap.get(array)? {
                Ok(Some(Value::Long(values.iter().sum())))
            } else {
                Ok(Some(Value::Long(0)))
            }
        }
        ("__jvm_rs/NativeLongStream", "count", "()J") => {
            let array = crate::vm::builtin::helpers::native_long_stream_array(vm, args[0].as_reference()?)?;
            let mut heap = vm.heap.lock().unwrap();
            if let HeapValue::LongArray { values } = heap.get(array)? {
                Ok(Some(Value::Long(values.len() as i64)))
            } else {
                Ok(Some(Value::Long(0)))
            }
        }
        ("__jvm_rs/NativeLongStream", "toArray", "()[J") => {
            let array = crate::vm::builtin::helpers::native_long_stream_array(vm, args[0].as_reference()?)?;
            Ok(Some(Value::Reference(array)))
        }
        ("__jvm_rs/NativeDoubleStream", "sum", "()D") => {
            let array = crate::vm::builtin::helpers::native_double_stream_array(vm, args[0].as_reference()?)?;
            let mut heap = vm.heap.lock().unwrap();
            if let HeapValue::DoubleArray { values } = heap.get(array)? {
                Ok(Some(Value::Double(values.iter().sum::<f64>())))
            } else {
                Ok(Some(Value::Double(0.0)))
            }
        }
        ("__jvm_rs/NativeDoubleStream", "count", "()J") => {
            let array = crate::vm::builtin::helpers::native_double_stream_array(vm, args[0].as_reference()?)?;
            let mut heap = vm.heap.lock().unwrap();
            if let HeapValue::DoubleArray { values } = heap.get(array)? {
                Ok(Some(Value::Long(values.len() as i64)))
            } else {
                Ok(Some(Value::Long(0)))
            }
        }
        ("__jvm_rs/NativeDoubleStream", "average", "()D") => {
            let array = crate::vm::builtin::helpers::native_double_stream_array(vm, args[0].as_reference()?)?;
            let mut heap = vm.heap.lock().unwrap();
            if let HeapValue::DoubleArray { values } = heap.get(array)? {
                if values.is_empty() {
                    return Err(VmError::UnhandledException {
                        class_name: "java/util/NoSuchElementException".to_string(),
                    });
                }
                let sum: f64 = values.iter().sum();
                Ok(Some(Value::Double(sum / values.len() as f64)))
            } else {
                Ok(Some(Value::Double(0.0)))
            }
        }
        ("__jvm_rs/NativeDoubleStream", "toArray", "()[D") => {
            let array = crate::vm::builtin::helpers::native_double_stream_array(vm, args[0].as_reference()?)?;
            Ok(Some(Value::Reference(array)))
        }
        ("__jvm_rs/NativeIntStream", "collect", "(Ljava/util/stream/Collector;)Ljava/lang/Object;") => {
            vm.native_int_stream_collect(args[0].as_reference()?, args[1].as_reference()?)
        }
        ("__jvm_rs/NativeLongStream", "collect", "(Ljava/util/stream/Collector;)Ljava/lang/Object;") => {
            vm.native_long_stream_collect(args[0].as_reference()?, args[1].as_reference()?)
        }
        ("__jvm_rs/NativeDoubleStream", "collect", "(Ljava/util/stream/Collector;)Ljava/lang/Object;") => {
            vm.native_double_stream_collect(args[0].as_reference()?, args[1].as_reference()?)
        }
        ("java/util/stream/Collectors", "toList", "()Ljava/util/stream/Collector;") => {
            vm.native_collectors_to_list()
        }
        ("java/util/stream/Collectors", "toSet", "()Ljava/util/stream/Collector;") => {
            vm.native_collectors_to_set()
        }
        ("java/util/stream/Collectors", "counting", "()Ljava/util/function/Supplier;") => {
            vm.native_collectors_counting()
        }
        ("java/util/stream/Collectors", "joining", "()Ljava/util/stream/Collector;") => {
            vm.native_collectors_joining(None)
        }
        ("java/util/stream/Collectors", "joining", "(Ljava/lang/CharSequence;)Ljava/util/stream/Collector;") => {
            vm.native_collectors_joining(Some(args[0].as_reference()?))
        }
        ("java/util/stream/Collectors", "reducing", "(Ljava/lang/Object;Ljava/util/function/BinaryOperator;)Ljava/util/stream/Collector;") => {
            vm.native_collectors_reducing(args[0].as_reference()?, args[1].as_reference()?)
        }
        ("java/util/stream/Collectors", "toMap", "(Ljava/util/function/Function;Ljava/util/function/Function;)Ljava/util/stream/Collector;") => {
            vm.native_collectors_to_map(args[0].as_reference()?, args[1].as_reference()?)
        }
        ("__jvm_rs/NativeIntStream", "sum", "()I") => {
            let array = crate::vm::builtin::helpers::native_int_stream_array(vm, args[0].as_reference()?)?;
            let mut heap = vm.heap.lock().unwrap();
            if let HeapValue::IntArray { values } = heap.get(array)? {
                Ok(Some(Value::Int(values.iter().map(|v| *v as i64).sum::<i64>() as i32)))
            } else {
                Ok(Some(Value::Int(0)))
            }
        }
        ("__jvm_rs/NativeIntStream", "count", "()J") => {
            let array = crate::vm::builtin::helpers::native_int_stream_array(vm, args[0].as_reference()?)?;
            let mut heap = vm.heap.lock().unwrap();
            if let HeapValue::IntArray { values } = heap.get(array)? {
                Ok(Some(Value::Long(values.len() as i64)))
            } else {
                Ok(Some(Value::Long(0)))
            }
        }
        ("__jvm_rs/NativeIntStream", "min", "()Ljava/util/OptionalInt;") => {
            let array = crate::vm::builtin::helpers::native_int_stream_array(vm, args[0].as_reference()?)?;
            let mut heap = vm.heap.lock().unwrap();
            if let HeapValue::IntArray { values } = heap.get(array)? {
                let min_val = values.iter().min().copied();
                match min_val {
                    Some(v) => {
                        let mut fields = HashMap::new();
                        fields.insert("value".to_string(), Value::Int(v));
                        let r = heap.allocate(HeapValue::Object {
                            class_name: "java/util/OptionalInt".to_string(),
                            fields,
                        });
                        Ok(Some(Value::Reference(r)))
                    }
                    None => Ok(Some(Value::Reference(Reference::Null))),
                }
            } else {
                Ok(Some(Value::Reference(Reference::Null)))
            }
        }
        ("__jvm_rs/NativeIntStream", "max", "()Ljava/util/OptionalInt;") => {
            let array = crate::vm::builtin::helpers::native_int_stream_array(vm, args[0].as_reference()?)?;
            let mut heap = vm.heap.lock().unwrap();
            if let HeapValue::IntArray { values } = heap.get(array)? {
                let max_val = values.iter().max().copied();
                match max_val {
                    Some(v) => {
                        let mut fields = HashMap::new();
                        fields.insert("value".to_string(), Value::Int(v));
                        let r = heap.allocate(HeapValue::Object {
                            class_name: "java/util/OptionalInt".to_string(),
                            fields,
                        });
                        Ok(Some(Value::Reference(r)))
                    }
                    None => Ok(Some(Value::Reference(Reference::Null))),
                }
            } else {
                Ok(Some(Value::Reference(Reference::Null)))
            }
        }
        ("__jvm_rs/NativeIntStream", "average", "()Ljava/util/OptionalDouble;") => {
            let array = crate::vm::builtin::helpers::native_int_stream_array(vm, args[0].as_reference()?)?;
            let mut heap = vm.heap.lock().unwrap();
            if let HeapValue::IntArray { values } = heap.get(array)? {
                if values.is_empty() {
                    return Ok(Some(Value::Reference(Reference::Null)));
                }
                let sum: i64 = values.iter().map(|v| *v as i64).sum();
                let avg = sum as f64 / values.len() as f64;
                let mut fields = HashMap::new();
                fields.insert("value".to_string(), Value::Double(avg));
                let r = heap.allocate(HeapValue::Object {
                    class_name: "java/util/OptionalDouble".to_string(),
                    fields,
                });
                Ok(Some(Value::Reference(r)))
            } else {
                Ok(Some(Value::Reference(Reference::Null)))
            }
        }
        ("__jvm_rs/NativeIntStream", "toArray", "()[I") => {
            let array = crate::vm::builtin::helpers::native_int_stream_array(vm, args[0].as_reference()?)?;
            Ok(Some(Value::Reference(array)))
        }
        ("__jvm_rs/NativeLongStream", "min", "()Ljava/util/OptionalLong;") => {
            let array = crate::vm::builtin::helpers::native_long_stream_array(vm, args[0].as_reference()?)?;
            let mut heap = vm.heap.lock().unwrap();
            if let HeapValue::LongArray { values } = heap.get(array)? {
                let min_val = values.iter().min().copied();
                match min_val {
                    Some(v) => {
                        let mut fields = HashMap::new();
                        fields.insert("value".to_string(), Value::Long(v));
                        let r = heap.allocate(HeapValue::Object {
                            class_name: "java/util/OptionalLong".to_string(),
                            fields,
                        });
                        Ok(Some(Value::Reference(r)))
                    }
                    None => Ok(Some(Value::Reference(Reference::Null))),
                }
            } else {
                Ok(Some(Value::Reference(Reference::Null)))
            }
        }
        ("__jvm_rs/NativeLongStream", "max", "()Ljava/util/OptionalLong;") => {
            let array = crate::vm::builtin::helpers::native_long_stream_array(vm, args[0].as_reference()?)?;
            let mut heap = vm.heap.lock().unwrap();
            if let HeapValue::LongArray { values } = heap.get(array)? {
                let max_val = values.iter().max().copied();
                match max_val {
                    Some(v) => {
                        let mut fields = HashMap::new();
                        fields.insert("value".to_string(), Value::Long(v));
                        let r = heap.allocate(HeapValue::Object {
                            class_name: "java/util/OptionalLong".to_string(),
                            fields,
                        });
                        Ok(Some(Value::Reference(r)))
                    }
                    None => Ok(Some(Value::Reference(Reference::Null))),
                }
            } else {
                Ok(Some(Value::Reference(Reference::Null)))
            }
        }
        ("__jvm_rs/NativeLongStream", "average", "()Ljava/util/OptionalDouble;") => {
            let array = crate::vm::builtin::helpers::native_long_stream_array(vm, args[0].as_reference()?)?;
            let mut heap = vm.heap.lock().unwrap();
            if let HeapValue::LongArray { values } = heap.get(array)? {
                if values.is_empty() {
                    return Ok(Some(Value::Reference(Reference::Null)));
                }
                let sum: i64 = values.iter().sum();
                let avg = sum as f64 / values.len() as f64;
                let mut fields = HashMap::new();
                fields.insert("value".to_string(), Value::Double(avg));
                let r = heap.allocate(HeapValue::Object {
                    class_name: "java/util/OptionalDouble".to_string(),
                    fields,
                });
                Ok(Some(Value::Reference(r)))
            } else {
                Ok(Some(Value::Reference(Reference::Null)))
            }
        }
        ("__jvm_rs/NativeDoubleStream", "min", "()Ljava/util/OptionalDouble;") => {
            let array = crate::vm::builtin::helpers::native_double_stream_array(vm, args[0].as_reference()?)?;
            let mut heap = vm.heap.lock().unwrap();
            if let HeapValue::DoubleArray { values } = heap.get(array)? {
                let min_val = values.iter().cloned().filter(|v| !v.is_nan()).min_by(|a, b| a.partial_cmp(b).unwrap());
                match min_val {
                    Some(v) => {
                        let mut fields = HashMap::new();
                        fields.insert("value".to_string(), Value::Double(v));
                        let r = heap.allocate(HeapValue::Object {
                            class_name: "java/util/OptionalDouble".to_string(),
                            fields,
                        });
                        Ok(Some(Value::Reference(r)))
                    }
                    None => Ok(Some(Value::Reference(Reference::Null))),
                }
            } else {
                Ok(Some(Value::Reference(Reference::Null)))
            }
        }
        ("__jvm_rs/NativeDoubleStream", "max", "()Ljava/util/OptionalDouble;") => {
            let array = crate::vm::builtin::helpers::native_double_stream_array(vm, args[0].as_reference()?)?;
            let mut heap = vm.heap.lock().unwrap();
            if let HeapValue::DoubleArray { values } = heap.get(array)? {
                let max_val = values.iter().cloned().filter(|v| !v.is_nan()).max_by(|a, b| a.partial_cmp(b).unwrap());
                match max_val {
                    Some(v) => {
                        let mut fields = HashMap::new();
                        fields.insert("value".to_string(), Value::Double(v));
                        let r = heap.allocate(HeapValue::Object {
                            class_name: "java/util/OptionalDouble".to_string(),
                            fields,
                        });
                        Ok(Some(Value::Reference(r)))
                    }
                    None => Ok(Some(Value::Reference(Reference::Null))),
                }
            } else {
                Ok(Some(Value::Reference(Reference::Null)))
            }
        }
        ("java/util/Collections", "sort", "(Ljava/util/List;)V") => {
            crate::vm::builtin::helpers::native_collections_sort(vm, args[0].as_reference()?, None)?;
            Ok(None)
        }
        ("java/util/Collections", "sort", "(Ljava/util/List;Ljava/util/Comparator;)V") => {
            let list = args[0].as_reference()?;
            let cmp = args[1].as_reference()?;
            let cmp_opt = if cmp == Reference::Null { None } else { Some(cmp) };
            crate::vm::builtin::helpers::native_collections_sort(vm, list, cmp_opt)?;
            Ok(None)
        }
        ("java/util/Collections", "reverse", "(Ljava/util/List;)V") => {
            crate::vm::builtin::helpers::native_collections_reverse(vm, args[0].as_reference()?)?;
            Ok(None)
        }
        ("java/util/Optional", "of", "(Ljava/lang/Object;)Ljava/util/Optional;") => {
            let value_ref = args[0].as_reference()?;
            let mut fields = HashMap::new();
            fields.insert("value".to_string(), Value::Reference(value_ref));
            let r = vm.heap.lock().unwrap().allocate(HeapValue::Object {
                class_name: "java/util/Optional".to_string(),
                fields,
            });
            Ok(Some(Value::Reference(r)))
        }
        ("java/util/Optional", "isPresent", "()Z")
        | ("java/util/Optional", "isEmpty", "()Z") => {
            let opt_ref = args[0].as_reference()?;
            let is_empty = match vm.heap.lock().unwrap().get(opt_ref)? {
                HeapValue::Object { fields, .. } => {
                    match fields.get("value") {
                        Some(Value::Reference(Reference::Null)) => true,
                        None => true,
                        _ => false,
                    }
                }
                _ => true,
            };
            let result = match descriptor {
                "()Z" if method_name == "isPresent" => !is_empty,
                "()Z" if method_name == "isEmpty" => is_empty,
                _ => false,
            };
            Ok(Some(Value::Int(if result { 1 } else { 0 })))
        }
        ("java/util/Optional", "get", "()Ljava/lang/Object;") => {
            let opt_ref = args[0].as_reference()?;
            match vm.heap.lock().unwrap().get(opt_ref)? {
                HeapValue::Object { fields, .. } => {
                    match fields.get("value") {
                        Some(Value::Reference(r)) if *r != Reference::Null => {
                            Ok(Some(Value::Reference(*r)))
                        }
                        _ => Err(VmError::UnhandledException {
                            class_name: "java/util/NoSuchElementException".to_string(),
                        }),
                    }
                }
                _ => Err(VmError::UnhandledException {
                    class_name: "java/util/NoSuchElementException".to_string(),
                }),
            }
        }
        ("java/util/Optional", "orElse", "(Ljava/lang/Object;)Ljava/lang/Object;") => {
            let opt_ref = args[0].as_reference()?;
            let fallback = args[1].as_reference()?;
            let value = match vm.heap.lock().unwrap().get(opt_ref)? {
                HeapValue::Object { fields, .. } => {
                    match fields.get("value") {
                        Some(Value::Reference(r)) if *r != Reference::Null => *r,
                        _ => fallback,
                    }
                }
                _ => fallback,
            };
            Ok(Some(Value::Reference(value)))
        }
        ("java/util/OptionalInt", "isPresent", "()Z") => {
            let opt_ref = args[0].as_reference()?;
            if opt_ref == Reference::Null {
                return Ok(Some(Value::Int(0)));
            }
            match vm.heap.lock().unwrap().get(opt_ref)? {
                HeapValue::Object { fields, .. } => {
                    match fields.get("value") {
                        Some(Value::Int(_)) => Ok(Some(Value::Int(1))),
                        _ => Ok(Some(Value::Int(0))),
                    }
                }
                _ => Ok(Some(Value::Int(0))),
            }
        }
        ("java/util/OptionalInt", "getAsInt", "()I") => {
            let opt_ref = args[0].as_reference()?;
            if opt_ref == Reference::Null {
                return Err(VmError::UnhandledException {
                    class_name: "java/util/NoSuchElementException".to_string(),
                });
            }
            match vm.heap.lock().unwrap().get(opt_ref)? {
                HeapValue::Object { fields, .. } => {
                    match fields.get("value") {
                        Some(Value::Int(v)) => Ok(Some(Value::Int(*v))),
                        _ => Err(VmError::UnhandledException {
                            class_name: "java/util/NoSuchElementException".to_string(),
                        }),
                    }
                }
                _ => Err(VmError::UnhandledException {
                    class_name: "java/util/NoSuchElementException".to_string(),
                }),
            }
        }
        ("java/util/OptionalInt", "orElse", "(I)I") => {
            let opt_ref = args[0].as_reference()?;
            let fallback = args[1].as_int()?;
            if opt_ref == Reference::Null {
                return Ok(Some(Value::Int(fallback)));
            }
            match vm.heap.lock().unwrap().get(opt_ref)? {
                HeapValue::Object { fields, .. } => {
                    match fields.get("value") {
                        Some(Value::Int(v)) => Ok(Some(Value::Int(*v))),
                        _ => Ok(Some(Value::Int(fallback))),
                    }
                }
                _ => Ok(Some(Value::Int(fallback))),
            }
        }
        ("java/util/OptionalLong", "isPresent", "()Z") => {
            let opt_ref = args[0].as_reference()?;
            if opt_ref == Reference::Null {
                return Ok(Some(Value::Int(0)));
            }
            match vm.heap.lock().unwrap().get(opt_ref)? {
                HeapValue::Object { fields, .. } => {
                    match fields.get("value") {
                        Some(Value::Long(_)) => Ok(Some(Value::Int(1))),
                        _ => Ok(Some(Value::Int(0))),
                    }
                }
                _ => Ok(Some(Value::Int(0))),
            }
        }
        ("java/util/OptionalLong", "getAsLong", "()J") => {
            let opt_ref = args[0].as_reference()?;
            if opt_ref == Reference::Null {
                return Err(VmError::UnhandledException {
                    class_name: "java/util/NoSuchElementException".to_string(),
                });
            }
            match vm.heap.lock().unwrap().get(opt_ref)? {
                HeapValue::Object { fields, .. } => {
                    match fields.get("value") {
                        Some(Value::Long(v)) => Ok(Some(Value::Long(*v))),
                        _ => Err(VmError::UnhandledException {
                            class_name: "java/util/NoSuchElementException".to_string(),
                        }),
                    }
                }
                _ => Err(VmError::UnhandledException {
                    class_name: "java/util/NoSuchElementException".to_string(),
                }),
            }
        }
        ("java/util/OptionalLong", "orElse", "(J)J") => {
            let opt_ref = args[0].as_reference()?;
            let fallback = args[1].as_long()?;
            if opt_ref == Reference::Null {
                return Ok(Some(Value::Long(fallback)));
            }
            match vm.heap.lock().unwrap().get(opt_ref)? {
                HeapValue::Object { fields, .. } => {
                    match fields.get("value") {
                        Some(Value::Long(v)) => Ok(Some(Value::Long(*v))),
                        _ => Ok(Some(Value::Long(fallback))),
                    }
                }
                _ => Ok(Some(Value::Long(fallback))),
            }
        }
        ("java/util/OptionalDouble", "isPresent", "()Z") => {
            let opt_ref = args[0].as_reference()?;
            if opt_ref == Reference::Null {
                return Ok(Some(Value::Int(0)));
            }
            match vm.heap.lock().unwrap().get(opt_ref)? {
                HeapValue::Object { fields, .. } => {
                    match fields.get("value") {
                        Some(Value::Double(_)) => Ok(Some(Value::Int(1))),
                        _ => Ok(Some(Value::Int(0))),
                    }
                }
                _ => Ok(Some(Value::Int(0))),
            }
        }
        ("java/util/OptionalDouble", "getAsDouble", "()D") => {
            let opt_ref = args[0].as_reference()?;
            if opt_ref == Reference::Null {
                return Err(VmError::UnhandledException {
                    class_name: "java/util/NoSuchElementException".to_string(),
                });
            }
            match vm.heap.lock().unwrap().get(opt_ref)? {
                HeapValue::Object { fields, .. } => {
                    match fields.get("value") {
                        Some(Value::Double(v)) => Ok(Some(Value::Double(*v))),
                        _ => Err(VmError::UnhandledException {
                            class_name: "java/util/NoSuchElementException".to_string(),
                        }),
                    }
                }
                _ => Err(VmError::UnhandledException {
                    class_name: "java/util/NoSuchElementException".to_string(),
                }),
            }
        }
        ("java/util/OptionalDouble", "orElse", "(D)D") => {
            let opt_ref = args[0].as_reference()?;
            let fallback = args[1].as_double()?;
            if opt_ref == Reference::Null {
                return Ok(Some(Value::Double(fallback)));
            }
            match vm.heap.lock().unwrap().get(opt_ref)? {
                HeapValue::Object { fields, .. } => {
                    match fields.get("value") {
                        Some(Value::Double(v)) => Ok(Some(Value::Double(*v))),
                        _ => Ok(Some(Value::Double(fallback))),
                    }
                }
                _ => Ok(Some(Value::Double(fallback))),
            }
        }
        _ => Err(VmError::UnhandledException {
            class_name: "".to_string(),
        }),
    }
}
