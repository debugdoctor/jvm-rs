use crate::vm::{HeapValue, Reference, Value, Vm, VmError};

pub(super) fn invoke_concurrent(
    vm: &mut Vm,
    class_name: &str,
    method_name: &str,
    descriptor: &str,
    args: &[Value],
) -> Result<Option<Value>, VmError> {
    match (class_name, method_name, descriptor) {
        // --- AtomicInteger ---
        ("java/util/concurrent/atomic/AtomicInteger", "get", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let val = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        fields.get("__value").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0)
                    }
                    _ => 0,
                }
            };
            Ok(Some(Value::Int(val)))
        }
        ("java/util/concurrent/atomic/AtomicInteger", "set", "(I)V") => {
            let obj_ref = args[0].as_reference()?;
            let new_val = args[1].as_int()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields.insert("__value".to_string(), Value::Int(new_val));
                }
            }
            Ok(None)
        }
        ("java/util/concurrent/atomic/AtomicInteger", "getAndSet", "(I)I") => {
            let obj_ref = args[0].as_reference()?;
            let new_val = args[1].as_int()?;
            let old_val = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        fields.get("__value").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0)
                    }
                    _ => 0,
                }
            };
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields.insert("__value".to_string(), Value::Int(new_val));
                }
            }
            Ok(Some(Value::Int(old_val)))
        }
        ("java/util/concurrent/atomic/AtomicInteger", "compareAndSet", "(II)Z") => {
            let obj_ref = args[0].as_reference()?;
            let expect = args[1].as_int()?;
            let new_val = args[2].as_int()?;
            let mut heap = vm.heap.lock().unwrap();
            let success = match heap.get_mut(obj_ref)? {
                HeapValue::Object { fields, .. } => {
                    match fields.get("__value") {
                        Some(Value::Int(current)) if *current == expect => {
                            fields.insert("__value".to_string(), Value::Int(new_val));
                            true
                        }
                        _ => false,
                    }
                }
                _ => false,
            };
            Ok(Some(Value::Int(if success { 1 } else { 0 })))
        }
        ("java/util/concurrent/atomic/AtomicInteger", "incrementAndGet", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let new_val = {
                let mut heap = vm.heap.lock().unwrap();
                match heap.get_mut(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let current = fields.get("__value").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
                        let new = current + 1;
                        fields.insert("__value".to_string(), Value::Int(new));
                        new
                    }
                    _ => 0,
                }
            };
            Ok(Some(Value::Int(new_val)))
        }
        ("java/util/concurrent/atomic/AtomicInteger", "decrementAndGet", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let new_val = {
                let mut heap = vm.heap.lock().unwrap();
                match heap.get_mut(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let current = fields.get("__value").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
                        let new = current - 1;
                        fields.insert("__value".to_string(), Value::Int(new));
                        new
                    }
                    _ => 0,
                }
            };
            Ok(Some(Value::Int(new_val)))
        }
        ("java/util/concurrent/atomic/AtomicInteger", "getAndIncrement", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let old_val = {
                let mut heap = vm.heap.lock().unwrap();
                match heap.get_mut(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let current = fields.get("__value").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
                        fields.insert("__value".to_string(), Value::Int(current + 1));
                        current
                    }
                    _ => 0,
                }
            };
            Ok(Some(Value::Int(old_val)))
        }
        ("java/util/concurrent/atomic/AtomicInteger", "getAndDecrement", "()I") => {
            let obj_ref = args[0].as_reference()?;
            let old_val = {
                let mut heap = vm.heap.lock().unwrap();
                match heap.get_mut(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let current = fields.get("__value").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
                        fields.insert("__value".to_string(), Value::Int(current - 1));
                        current
                    }
                    _ => 0,
                }
            };
            Ok(Some(Value::Int(old_val)))
        }
        ("java/util/concurrent/atomic/AtomicInteger", "addAndGet", "(I)I") => {
            let obj_ref = args[0].as_reference()?;
            let delta = args[1].as_int()?;
            let new_val = {
                let mut heap = vm.heap.lock().unwrap();
                match heap.get_mut(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let current = fields.get("__value").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
                        let new = current + delta;
                        fields.insert("__value".to_string(), Value::Int(new));
                        new
                    }
                    _ => 0,
                }
            };
            Ok(Some(Value::Int(new_val)))
        }
        ("java/util/concurrent/atomic/AtomicInteger", "getAndAdd", "(I)I") => {
            let obj_ref = args[0].as_reference()?;
            let delta = args[1].as_int()?;
            let old_val = {
                let mut heap = vm.heap.lock().unwrap();
                match heap.get_mut(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let current = fields.get("__value").and_then(|v| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        }).unwrap_or(0);
                        fields.insert("__value".to_string(), Value::Int(current + delta));
                        current
                    }
                    _ => 0,
                }
            };
            Ok(Some(Value::Int(old_val)))
        }
        ("java/util/concurrent/atomic/AtomicInteger", "<init>", "(I)V") => {
            let obj_ref = args[0].as_reference()?;
            let initial = args[1].as_int()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields.insert("__value".to_string(), Value::Int(initial));
                }
            }
            Ok(None)
        }
        // --- AtomicLong ---
        ("java/util/concurrent/atomic/AtomicLong", "get", "()J") => {
            let obj_ref = args[0].as_reference()?;
            let val = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        fields.get("__value").and_then(|v| match v {
                            Value::Long(l) => Some(*l),
                            _ => None,
                        }).unwrap_or(0)
                    }
                    _ => 0,
                }
            };
            Ok(Some(Value::Long(val)))
        }
        ("java/util/concurrent/atomic/AtomicLong", "set", "(J)V") => {
            let obj_ref = args[0].as_reference()?;
            let new_val = args[1].as_long()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields.insert("__value".to_string(), Value::Long(new_val));
                }
            }
            Ok(None)
        }
        ("java/util/concurrent/atomic/AtomicLong", "getAndSet", "(J)J") => {
            let obj_ref = args[0].as_reference()?;
            let new_val = args[1].as_long()?;
            let old_val = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        fields.get("__value").and_then(|v| match v {
                            Value::Long(l) => Some(*l),
                            _ => None,
                        }).unwrap_or(0)
                    }
                    _ => 0,
                }
            };
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields.insert("__value".to_string(), Value::Long(new_val));
                }
            }
            Ok(Some(Value::Long(old_val)))
        }
        ("java/util/concurrent/atomic/AtomicLong", "compareAndSet", "(JJ)Z") => {
            let obj_ref = args[0].as_reference()?;
            let expect = args[1].as_long()?;
            let new_val = args[2].as_long()?;
            let mut heap = vm.heap.lock().unwrap();
            let success = match heap.get_mut(obj_ref)? {
                HeapValue::Object { fields, .. } => {
                    match fields.get("__value") {
                        Some(Value::Long(current)) if *current == expect => {
                            fields.insert("__value".to_string(), Value::Long(new_val));
                            true
                        }
                        _ => false,
                    }
                }
                _ => false,
            };
            Ok(Some(Value::Int(if success { 1 } else { 0 })))
        }
        ("java/util/concurrent/atomic/AtomicLong", "incrementAndGet", "()J") => {
            let obj_ref = args[0].as_reference()?;
            let new_val = {
                let mut heap = vm.heap.lock().unwrap();
                match heap.get_mut(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let current = fields.get("__value").and_then(|v| match v {
                            Value::Long(l) => Some(*l),
                            _ => None,
                        }).unwrap_or(0);
                        let new = current + 1;
                        fields.insert("__value".to_string(), Value::Long(new));
                        new
                    }
                    _ => 0,
                }
            };
            Ok(Some(Value::Long(new_val)))
        }
        ("java/util/concurrent/atomic/AtomicLong", "decrementAndGet", "()J") => {
            let obj_ref = args[0].as_reference()?;
            let new_val = {
                let mut heap = vm.heap.lock().unwrap();
                match heap.get_mut(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let current = fields.get("__value").and_then(|v| match v {
                            Value::Long(l) => Some(*l),
                            _ => None,
                        }).unwrap_or(0);
                        let new = current - 1;
                        fields.insert("__value".to_string(), Value::Long(new));
                        new
                    }
                    _ => 0,
                }
            };
            Ok(Some(Value::Long(new_val)))
        }
        ("java/util/concurrent/atomic/AtomicLong", "addAndGet", "(J)J") => {
            let obj_ref = args[0].as_reference()?;
            let delta = args[1].as_long()?;
            let new_val = {
                let mut heap = vm.heap.lock().unwrap();
                match heap.get_mut(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let current = fields.get("__value").and_then(|v| match v {
                            Value::Long(l) => Some(*l),
                            _ => None,
                        }).unwrap_or(0);
                        let new = current + delta;
                        fields.insert("__value".to_string(), Value::Long(new));
                        new
                    }
                    _ => 0,
                }
            };
            Ok(Some(Value::Long(new_val)))
        }
        ("java/util/concurrent/atomic/AtomicLong", "<init>", "(J)V") => {
            let obj_ref = args[0].as_reference()?;
            let initial = args[1].as_long()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields.insert("__value".to_string(), Value::Long(initial));
                }
            }
            Ok(None)
        }
        // --- AtomicReference ---
        ("java/util/concurrent/atomic/AtomicReference", "get", "()Ljava/lang/Object;") => {
            let obj_ref = args[0].as_reference()?;
            let val = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        fields.get("__value").and_then(|v| match v {
                            Value::Reference(r) => Some(*r),
                            _ => None,
                        }).unwrap_or(Reference::Null)
                    }
                    _ => Reference::Null,
                }
            };
            Ok(Some(Value::Reference(val)))
        }
        ("java/util/concurrent/atomic/AtomicReference", "set", "(Ljava/lang/Object;)V") => {
            let obj_ref = args[0].as_reference()?;
            let new_val = args[1].as_reference()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields.insert("__value".to_string(), Value::Reference(new_val));
                }
            }
            Ok(None)
        }
        ("java/util/concurrent/atomic/AtomicReference", "getAndSet", "(Ljava/lang/Object;)Ljava/lang/Object;") => {
            let obj_ref = args[0].as_reference()?;
            let new_val = args[1].as_reference()?;
            let old_val = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        fields.get("__value").and_then(|v| match v {
                            Value::Reference(r) => Some(*r),
                            _ => None,
                        }).unwrap_or(Reference::Null)
                    }
                    _ => Reference::Null,
                }
            };
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields.insert("__value".to_string(), Value::Reference(new_val));
                }
            }
            Ok(Some(Value::Reference(old_val)))
        }
        ("java/util/concurrent/atomic/AtomicReference", "compareAndSet", "(Ljava/lang/Object;Ljava/lang/Object;)Z") => {
            let obj_ref = args[0].as_reference()?;
            let expect = args[1].as_reference()?;
            let new_val = args[2].as_reference()?;
            let mut heap = vm.heap.lock().unwrap();
            let success = match heap.get_mut(obj_ref)? {
                HeapValue::Object { fields, .. } => {
                    match fields.get("__value") {
                        Some(Value::Reference(current)) if *current == expect => {
                            fields.insert("__value".to_string(), Value::Reference(new_val));
                            true
                        }
                        _ => false,
                    }
                }
                _ => false,
            };
            Ok(Some(Value::Int(if success { 1 } else { 0 })))
        }
        ("java/util/concurrent/atomic/AtomicReference", "<init>", "(Ljava/lang/Object;)V") => {
            let obj_ref = args[0].as_reference()?;
            let initial = args[1].as_reference()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields.insert("__value".to_string(), Value::Reference(initial));
                }
            }
            Ok(None)
        }
        // --- LongAdder ---
        ("java/util/concurrent/atomic/LongAdder", "add", "(J)V") => {
            let obj_ref = args[0].as_reference()?;
            let delta = args[1].as_long()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    let current = fields.get("__value").and_then(|v| match v {
                        Value::Long(l) => Some(*l),
                        _ => None,
                    }).unwrap_or(0);
                    fields.insert("__value".to_string(), Value::Long(current + delta));
                }
            }
            Ok(None)
        }
        ("java/util/concurrent/atomic/LongAdder", "sum", "()J") => {
            let obj_ref = args[0].as_reference()?;
            let val = {
                let heap = vm.heap.lock().unwrap();
                match heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        fields.get("__value").and_then(|v| match v {
                            Value::Long(l) => Some(*l),
                            _ => None,
                        }).unwrap_or(0)
                    }
                    _ => 0,
                }
            };
            Ok(Some(Value::Long(val)))
        }
        ("java/util/concurrent/atomic/LongAdder", "increment", "()V") => {
            let obj_ref = args[0].as_reference()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    let current = fields.get("__value").and_then(|v| match v {
                        Value::Long(l) => Some(*l),
                        _ => None,
                    }).unwrap_or(0);
                    fields.insert("__value".to_string(), Value::Long(current + 1));
                }
            }
            Ok(None)
        }
        ("java/util/concurrent/atomic/LongAdder", "decrement", "()V") => {
            let obj_ref = args[0].as_reference()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    let current = fields.get("__value").and_then(|v| match v {
                        Value::Long(l) => Some(*l),
                        _ => None,
                    }).unwrap_or(0);
                    fields.insert("__value".to_string(), Value::Long(current - 1));
                }
            }
            Ok(None)
        }
        ("java/util/concurrent/atomic/LongAdder", "reset", "()V") => {
            let obj_ref = args[0].as_reference()?;
            {
                let mut heap = vm.heap.lock().unwrap();
                if let HeapValue::Object { fields, .. } = heap.get_mut(obj_ref)? {
                    fields.insert("__value".to_string(), Value::Long(0));
                }
            }
            Ok(None)
        }
        ("java/util/concurrent/atomic/LongAdder", "<init>", "()V") => Ok(None),
        // --- ReentrantLock ---
        ("java/util/concurrent/locks/ReentrantLock", "lock", "()V") => Ok(None),
        ("java/util/concurrent/locks/ReentrantLock", "unlock", "()V") => Ok(None),
        ("java/util/concurrent/locks/ReentrantLock", "tryLock", "()Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/locks/ReentrantLock", "isHeldByCurrentThread", "()Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/locks/ReentrantLock", "getHoldCount", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/locks/ReentrantLock", "<init>", "()V") => Ok(None),
        ("java/util/concurrent/locks/ReentrantLock", "<init>", "(Z)V") => Ok(None),
        // --- ReadWriteLock ---
        ("java/util/concurrent/locks/ReadWriteLock", "readLock", "()Ljava/util/concurrent/locks/Lock;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/locks/ReadWriteLock", "writeLock", "()Ljava/util/concurrent/locks/Lock;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        // --- Lock ---
        ("java/util/concurrent/locks/Lock", "lock", "()V") => Ok(None),
        ("java/util/concurrent/locks/Lock", "unlock", "()V") => Ok(None),
        ("java/util/concurrent/locks/Lock", "tryLock", "()Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/locks/Lock", "newCondition", "()Ljava/util/concurrent/locks/Condition;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        },
        // --- Condition ---
        ("java/util/concurrent/locks/AbstractOwnableSynchronizer", "setExclusiveOwnerThread", "(Ljava/lang/Thread;)V") => Ok(None),
        ("java/util/concurrent/locks/AbstractOwnableSynchronizer", "getExclusiveOwnerThread", "()Ljava/lang/Thread;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        },
        ("java/util/concurrent/locks/Condition", "await", "()V") => Ok(None),
        ("java/util/concurrent/locks/Condition", "signal", "()V") => Ok(None),
        ("java/util/concurrent/locks/Condition", "signalAll", "()V") => Ok(None),
        // --- Semaphore ---
        ("java/util/concurrent/Semaphore", "acquire", "()V") => Ok(None),
        ("java/util/concurrent/Semaphore", "acquire", "(I)V") => Ok(None),
        ("java/util/concurrent/Semaphore", "release", "()V") => Ok(None),
        ("java/util/concurrent/Semaphore", "release", "(I)V") => Ok(None),
        ("java/util/concurrent/Semaphore", "tryAcquire", "()Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/Semaphore", "tryAcquire", "(I)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/Semaphore", "drainPermits", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/Semaphore", "availablePermits", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/Semaphore", "<init>", "(I)V") => Ok(None),
        ("java/util/concurrent/Semaphore", "<init>", "(IZ)V") => Ok(None),
        // --- CountDownLatch ---
        ("java/util/concurrent/CountDownLatch", "await", "()V") => Ok(None),
        ("java/util/concurrent/CountDownLatch", "await", "(JLjava/util/concurrent/TimeUnit;)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/CountDownLatch", "countDown", "()V") => Ok(None),
        ("java/util/concurrent/CountDownLatch", "getCount", "()J") => Ok(Some(Value::Long(0))),
        ("java/util/concurrent/CountDownLatch", "<init>", "(J)V") => Ok(None),
        // --- CyclicBarrier ---
        ("java/util/concurrent/CyclicBarrier", "await", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/CyclicBarrier", "await", "(JLjava/util/concurrent/TimeUnit;)I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/CyclicBarrier", "reset", "()V") => Ok(None),
        ("java/util/concurrent/CyclicBarrier", "getNumberWaiting", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/CyclicBarrier", "isBroken", "()Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/CyclicBarrier", "<init>", "(I)V") => Ok(None),
        ("java/util/concurrent/CyclicBarrier", "<init>", "(ILjava/lang/Runnable;)V") => Ok(None),
        // --- ConcurrentHashMap ---
        ("java/util/concurrent/ConcurrentHashMap", "size", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ConcurrentHashMap", "isEmpty", "()Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/ConcurrentHashMap", "get", "(Ljava/lang/Object;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ConcurrentHashMap", "put", "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ConcurrentHashMap", "putIfAbsent", "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ConcurrentHashMap", "remove", "(Ljava/lang/Object;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ConcurrentHashMap", "containsKey", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ConcurrentHashMap", "contains", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ConcurrentHashMap", "clear", "()V") => Ok(None),
        ("java/util/concurrent/ConcurrentHashMap", "keys", "()Ljava/util/Enumeration;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ConcurrentHashMap", "elements", "()Ljava/util/Enumeration;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ConcurrentHashMap", "<init>", "()V") => Ok(None),
        ("java/util/concurrent/ConcurrentHashMap", "<init>", "(I)V") => Ok(None),
        ("java/util/concurrent/ConcurrentHashMap", "<init>", "(IF)V") => Ok(None),
        ("java/util/concurrent/ConcurrentHashMap", "<init>", "(Ljava/util/Map;)V") => Ok(None),
        // --- ConcurrentLinkedQueue ---
        ("java/util/concurrent/ConcurrentLinkedQueue", "size", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ConcurrentLinkedQueue", "isEmpty", "()Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/ConcurrentLinkedQueue", "offer", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/ConcurrentLinkedQueue", "poll", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ConcurrentLinkedQueue", "peek", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ConcurrentLinkedQueue", "remove", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ConcurrentLinkedQueue", "contains", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ConcurrentLinkedQueue", "<init>", "()V") => Ok(None),
        ("java/util/concurrent/ConcurrentLinkedQueue", "<init>", "(Ljava/util/Collection;)V") => Ok(None),
        // --- CopyOnWriteArrayList ---
        ("java/util/concurrent/CopyOnWriteArrayList", "size", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/CopyOnWriteArrayList", "isEmpty", "()Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/CopyOnWriteArrayList", "get", "(I)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/CopyOnWriteArrayList", "set", "(ILjava/lang/Object;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/CopyOnWriteArrayList", "add", "(ILjava/lang/Object;)V") => Ok(None),
        ("java/util/concurrent/CopyOnWriteArrayList", "add", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/CopyOnWriteArrayList", "remove", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/CopyOnWriteArrayList", "remove", "(I)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/CopyOnWriteArrayList", "contains", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/CopyOnWriteArrayList", "clear", "()V") => Ok(None),
        ("java/util/concurrent/CopyOnWriteArrayList", "<init>", "()V") => Ok(None),
        ("java/util/concurrent/CopyOnWriteArrayList", "<init>", "([Ljava/lang/Object;)V") => Ok(None),
        ("java/util/concurrent/CopyOnWriteArrayList", "<init>", "(Ljava/util/Collection;)V") => Ok(None),
        // --- Future ---
        ("java/util/concurrent/Future", "get", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/Future", "get", "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/Future", "isDone", "()Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/Future", "isCancelled", "()Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/Future", "cancel", "(Z)Z") => Ok(Some(Value::Int(0))),
        // --- CompletableFuture ---
        ("java/util/concurrent/CompletableFuture", "get", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/CompletableFuture", "get", "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/CompletableFuture", "isDone", "()Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/CompletableFuture", "isCancelled", "()Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/CompletableFuture", "cancel", "(Z)Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/CompletableFuture", "complete", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/CompletableFuture", "completedFuture", "(Ljava/lang/Object;)Ljava/util/concurrent/CompletableFuture;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/CompletableFuture", "runAsync", "(Ljava/lang/Runnable;)Ljava/util/concurrent/CompletableFuture;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/CompletableFuture", "supplyAsync", "(Ljava/util/function/Supplier;)Ljava/util/concurrent/CompletableFuture;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/CompletableFuture", "thenApply", "(Ljava/util/function/Function;)Ljava/util/concurrent/CompletableFuture;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/CompletableFuture", "thenApplyAsync", "(Ljava/util/function/Function;)Ljava/util/concurrent/CompletableFuture;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/CompletableFuture", "thenAccept", "(Ljava/util/function/Consumer;)Ljava/util/concurrent/CompletableFuture;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/CompletableFuture", "thenRun", "(Ljava/lang/Runnable;)Ljava/util/concurrent/CompletableFuture;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/CompletableFuture", "join", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/CompletableFuture", "<init>", "()V") => Ok(None),
        // --- ExecutorService ---
        ("java/util/concurrent/ExecutorService", "shutdown", "()V") => Ok(None),
        ("java/util/concurrent/ExecutorService", "shutdownNow", "()Ljava/util/List;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ExecutorService", "isShutdown", "()Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ExecutorService", "isTerminated", "()Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ExecutorService", "awaitTermination", "(JLjava/util/concurrent/TimeUnit;)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/ExecutorService", "submit", "(Ljava/lang/Runnable;)Ljava/util/concurrent/Future;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ExecutorService", "submit", "(Ljava/lang/Runnable;Ljava/lang/Object;)Ljava/util/concurrent/Future;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ExecutorService", "submit", "(Ljava/util/concurrent/Callable;)Ljava/util/concurrent/Future;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        // --- ThreadPoolExecutor ---
        ("java/util/concurrent/ThreadPoolExecutor", "execute", "(Ljava/lang/Runnable;)V") => Ok(None),
        ("java/util/concurrent/ThreadPoolExecutor", "shutdown", "()V") => Ok(None),
        ("java/util/concurrent/ThreadPoolExecutor", "shutdownNow", "()Ljava/util/List;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ThreadPoolExecutor", "isShutdown", "()Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ThreadPoolExecutor", "isTerminated", "()Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ThreadPoolExecutor", "isTerminating", "()Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ThreadPoolExecutor", "awaitTermination", "(JLjava/util/concurrent/TimeUnit;)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/ThreadPoolExecutor", "submit", "(Ljava/lang/Runnable;)Ljava/util/concurrent/Future;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ThreadPoolExecutor", "getPoolSize", "()I") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/ThreadPoolExecutor", "getActiveCount", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ThreadPoolExecutor", "getTaskCount", "()J") => Ok(Some(Value::Long(0))),
        ("java/util/concurrent/ThreadPoolExecutor", "getCompletedTaskCount", "()J") => Ok(Some(Value::Long(0))),
        ("java/util/concurrent/ThreadPoolExecutor", "remove", "(Ljava/lang/Runnable;)Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ThreadPoolExecutor", "purge", "()V") => Ok(None),
        ("java/util/concurrent/ThreadPoolExecutor", "<init>", "(ILjava/util/concurrent/TimeUnit;Ljava/util/concurrent/BlockingQueue;)V") => Ok(None),
        ("java/util/concurrent/ThreadPoolExecutor", "<init>", "(IIJLjava/util/concurrent/TimeUnit;Ljava/util/concurrent/BlockingQueue;)V") => Ok(None),
        ("java/util/concurrent/ThreadPoolExecutor", "<init>", "(IIJLjava/util/concurrent/TimeUnit;Ljava/util/concurrent/BlockingQueue;Ljava/lang/ThreadFactory;)V") => Ok(None),
        ("java/util/concurrent/ThreadPoolExecutor", "<init>", "(IIJLjava/util/concurrent/TimeUnit;Ljava/util/concurrent/BlockingQueue;Ljava/util/concurrent/RejectedExecutionHandler;)V") => Ok(None),
        ("java/util/concurrent/ThreadPoolExecutor", "<init>", "(IIJLjava/util/concurrent/TimeUnit;Ljava/util/concurrent/BlockingQueue;Ljava/lang/ThreadFactory;Ljava/util/concurrent/RejectedExecutionHandler;)V") => Ok(None),
        // --- Executors ---
        ("java/util/concurrent/Executors", "newSingleThreadExecutor", "()Ljava/util/concurrent/ExecutorService;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/Executors", "newFixedThreadPool", "(I)Ljava/util/concurrent/ExecutorService;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/Executors", "newCachedThreadPool", "()Ljava/util/concurrent/ExecutorService;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/Executors", "newSingleThreadScheduledExecutor", "()Ljava/util/concurrent/ScheduledExecutorService;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/Executors", "newScheduledThreadPool", "(I)Ljava/util/concurrent/ScheduledExecutorService;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        // --- ScheduledExecutorService ---
        ("java/util/concurrent/ScheduledExecutorService", "schedule", "(Ljava/lang/Runnable;JLjava/util/concurrent/TimeUnit;)Ljava/util/concurrent/ScheduledFuture;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ScheduledExecutorService", "schedule", "(Ljava/util/concurrent/Callable;JLjava/util/concurrent/TimeUnit;)Ljava/util/concurrent/ScheduledFuture;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ScheduledExecutorService", "scheduleAtFixedRate", "(Ljava/lang/Runnable;JJLjava/util/concurrent/TimeUnit;)Ljava/util/concurrent/ScheduledFuture;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ScheduledExecutorService", "scheduleWithFixedDelay", "(Ljava/lang/Runnable;JJLjava/util/concurrent/TimeUnit;)Ljava/util/concurrent/ScheduledFuture;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        },
        // --- ScheduledFuture ---
        ("java/util/concurrent/ScheduledFuture", "getDelay", "(Ljava/util/concurrent/TimeUnit;)J") => Ok(Some(Value::Long(0))),
        // --- Delayed ---
        ("java/util/concurrent/Delayed", "getDelay", "(Ljava/util/concurrent/TimeUnit;)J") => Ok(Some(Value::Long(0))),
        // --- TimeUnit ---
        ("java/util/concurrent/TimeUnit", "toNanos", "(J)J") => {
            let val = args[0].as_long()?;
            Ok(Some(Value::Long(val)))
        }
        ("java/util/concurrent/TimeUnit", "toMicros", "(J)J") => {
            let val = args[0].as_long()?;
            Ok(Some(Value::Long(val)))
        }
        ("java/util/concurrent/TimeUnit", "toMillis", "(J)J") => {
            let val = args[0].as_long()?;
            Ok(Some(Value::Long(val)))
        }
        ("java/util/concurrent/TimeUnit", "toSeconds", "(J)J") => {
            let val = args[0].as_long()?;
            Ok(Some(Value::Long(val)))
        }
        ("java/util/concurrent/TimeUnit", "sleep", "(J)V") => Ok(None),
        // --- BlockingQueue ---
        ("java/util/concurrent/BlockingQueue", "put", "(Ljava/lang/Object;)V") => Ok(None),
        ("java/util/concurrent/BlockingQueue", "offer", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/BlockingQueue", "offer", "(Ljava/lang/Object;JLjava/util/concurrent/TimeUnit;)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/BlockingQueue", "take", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/BlockingQueue", "poll", "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        },
        // --- ArrayBlockingQueue ---
        ("java/util/concurrent/ArrayBlockingQueue", "offer", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/ArrayBlockingQueue", "poll", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ArrayBlockingQueue", "peek", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ArrayBlockingQueue", "size", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ArrayBlockingQueue", "isEmpty", "()Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/ArrayBlockingQueue", "remainingCapacity", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ArrayBlockingQueue", "clear", "()V") => Ok(None),
        ("java/util/concurrent/ArrayBlockingQueue", "contains", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ArrayBlockingQueue", "<init>", "(I)V") => Ok(None),
        ("java/util/concurrent/ArrayBlockingQueue", "<init>", "(ILZ)V") => Ok(None),
        ("java/util/concurrent/ArrayBlockingQueue", "<init>", "(ILZLjava/util/Collection;)V") => Ok(None),
        // --- RejectedExecutionHandler ---
        ("java/util/concurrent/RejectedExecutionHandler", "rejectedExecution", "(Ljava/lang/Runnable;Ljava/util/concurrent/ThreadPoolExecutor;)V") => Ok(None),
        // --- ThreadFactory ---
        ("java/util/concurrent/ThreadFactory", "newThread", "(Ljava/lang/Runnable;)Ljava/lang/Thread;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        },
        // --- Callable ---
        ("java/util/concurrent/Callable", "call", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        },
        // --- Exchanger ---
        ("java/util/concurrent/Exchanger", "exchange", "(Ljava/lang/Object;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/Exchanger", "exchange", "(Ljava/lang/Object;JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/Exchanger", "<init>", "()V") => Ok(None),
        // --- Phaser ---
        ("java/util/concurrent/Phaser", "register", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/Phaser", "arrive", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/Phaser", "arriveAndAwaitAdvance", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/Phaser", "arriveAndDeregister", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/Phaser", "bulkRegister", "(I)I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/Phaser", "getPhase", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/Phaser", "getRegisteredParties", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/Phaser", "getArrivedParties", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/Phaser", "getUnarrivedParties", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/Phaser", "forceTermination", "()V") => Ok(None),
        ("java/util/concurrent/Phaser", "<init>", "()V") => Ok(None),
        ("java/util/concurrent/Phaser", "<init>", "(I)V") => Ok(None),
        ("java/util/concurrent/Phaser", "<init>", "(Ljava/util/concurrent/Phaser;)V") => Ok(None),
        // --- ForkJoinPool ---
        ("java/util/concurrent/ForkJoinPool", "submit", "(Ljava/lang/Runnable;)Ljava/util/concurrent/Future;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ForkJoinPool", "submit", "(Ljava/util/concurrent/ForkJoinTask;)Ljava/util/concurrent/ForkJoinTask;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ForkJoinPool", "invoke", "(Ljava/util/concurrent/ForkJoinTask;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ForkJoinPool", "execute", "(Ljava/lang/Runnable;)V") => Ok(None),
        ("java/util/concurrent/ForkJoinPool", "shutdown", "()V") => Ok(None),
        ("java/util/concurrent/ForkJoinPool", "shutdownNow", "()Ljava/util/List;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ForkJoinPool", "isShutdown", "()Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ForkJoinPool", "isTerminated", "()Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ForkJoinPool", "isTerminating", "()Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ForkJoinPool", "awaitTermination", "(JLjava/util/concurrent/TimeUnit;)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/ForkJoinPool", "getPoolSize", "()I") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/ForkJoinPool", "getActiveThreadCount", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ForkJoinPool", "getStealCount", "()J") => Ok(Some(Value::Long(0))),
        ("java/util/concurrent/ForkJoinPool", "getQueuedTaskCount", "()J") => Ok(Some(Value::Long(0))),
        ("java/util/concurrent/ForkJoinPool", "getQueuedSubmissionCount", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ForkJoinPool", "hasQueuedSubmissions", "()Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ForkJoinPool", "commonPool", "()Ljava/util/concurrent/ForkJoinPool;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ForkJoinPool", "<init>", "(I)V") => Ok(None),
        ("java/util/concurrent/ForkJoinPool", "<init>", "(ILjava/util/concurrent/ForkJoinPool$Factory;)V") => Ok(None),
        ("java/util/concurrent/ForkJoinPool", "<init>", "(ILjava/util/concurrent/ForkJoinPool$Factory;Ljava/util/concurrent/RejectedExecutionHandler;Z)V") => Ok(None),
        // --- ForkJoinTask ---
        ("java/util/concurrent/ForkJoinTask", "fork", "()Ljava/util/concurrent/ForkJoinTask;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ForkJoinTask", "join", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ForkJoinTask", "invoke", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ForkJoinTask", "cancel", "(Z)Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ForkJoinTask", "isDone", "()Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/ForkJoinTask", "isCompletedNormally", "()Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/ForkJoinTask", "isCompletedAbnormally", "()Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ForkJoinTask", "isCancelled", "()Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ForkJoinTask", "quietlyJoin", "()V") => Ok(None),
        ("java/util/concurrent/ForkJoinTask", "quietlyFork", "()V") => Ok(None),
        ("java/util/concurrent/ForkJoinTask", "get", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ForkJoinTask", "get", "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        },
        // --- CountedCompleter ---
        ("java/util/concurrent/CountedCompleter", "compute", "()V") => Ok(None),
        ("java/util/concurrent/CountedCompleter", "onCompletion", "(Ljava/util/concurrent/CountedCompleter;)V") => Ok(None),
        ("java/util/concurrent/CountedCompleter", "getRawResult", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        },
        // --- RecursiveTask ---
        ("java/util/concurrent/RecursiveTask", "compute", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/RecursiveTask", "getRawResult", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        },
        // --- RecursiveAction ---
        ("java/util/concurrent/RecursiveAction", "compute", "()V") => Ok(None),
        ("java/util/concurrent/RecursiveAction", "getRawResult", "()Ljava/lang/Object;") => Ok(None),
        // --- SubmissionPublisher ---
        ("java/util/concurrent/SubmissionPublisher", "submit", "(Ljava/lang/Object;)I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/SubmissionPublisher", "offer", "(Ljava/lang/Object;Ljava/util/concurrent/TimeUnit;)I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/SubmissionPublisher", "close", "()V") => Ok(None),
        ("java/util/concurrent/SubmissionPublisher", "isClosed", "()Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/SubmissionPublisher", "hasSubscribers", "()Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/SubmissionPublisher", "getSubscriberCount", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/SubmissionPublisher", "<init>", "()V") => Ok(None),
        ("java/util/concurrent/SubmissionPublisher", "<init>", "(Ljava/util/concurrent/ExecutorService;I)V") => Ok(None),
        // --- Flow ---
        ("java/util/concurrent/Flow$Publisher", "subscribe", "(Ljava/util/concurrent/Flow$Subscriber;)V") => Ok(None),
        ("java/util/concurrent/Flow$Subscriber", "onNext", "(Ljava/lang/Object;)V") => Ok(None),
        ("java/util/concurrent/Flow$Subscriber", "onError", "(Ljava/lang/Throwable;)V") => Ok(None),
        ("java/util/concurrent/Flow$Subscriber", "onComplete", "()V") => Ok(None),
        ("java/util/concurrent/Flow$Subscriber", "onSubscribe", "(Ljava/util/concurrent/Flow$Subscription;)V") => Ok(None),
        ("java/util/concurrent/Flow$Subscription", "request", "(J)V") => Ok(None),
        ("java/util/concurrent/Flow$Subscription", "cancel", "()V") => Ok(None),
        ("java/util/concurrent/Flow$Processor", "onNext", "(Ljava/lang/Object;)V") => Ok(None),
        ("java/util/concurrent/Flow$Processor", "onError", "(Ljava/lang/Throwable;)V") => Ok(None),
        ("java/util/concurrent/Flow$Processor", "onComplete", "()V") => Ok(None),
        ("java/util/concurrent/Flow$Processor", "onSubscribe", "(Ljava/util/concurrent/Flow$Subscription;)V") => Ok(None),
        // --- StampedLock ---
        ("java/util/concurrent/locks/StampedLock", "readLock", "()J") => Ok(Some(Value::Long(0))),
        ("java/util/concurrent/locks/StampedLock", "writeLock", "()J") => Ok(Some(Value::Long(0))),
        ("java/util/concurrent/locks/StampedLock", "tryReadLock", "()J") => Ok(Some(Value::Long(0))),
        ("java/util/concurrent/locks/StampedLock", "tryWriteLock", "()J") => Ok(Some(Value::Long(0))),
        ("java/util/concurrent/locks/StampedLock", "unlockRead", "(J)V") => Ok(None),
        ("java/util/concurrent/locks/StampedLock", "unlockWrite", "(J)V") => Ok(None),
        ("java/util/concurrent/locks/StampedLock", "unlock", "(J)V") => Ok(None),
        ("java/util/concurrent/locks/StampedLock", "tryConvertToReadLock", "(J)J") => Ok(Some(Value::Long(0))),
        ("java/util/concurrent/locks/StampedLock", "tryConvertToWriteLock", "(J)J") => Ok(Some(Value::Long(0))),
        ("java/util/concurrent/locks/StampedLock", "isReadLocked", "()Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/locks/StampedLock", "isWriteLocked", "()Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/locks/StampedLock", "getReadLockCount", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/locks/StampedLock", "validate", "(J)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/locks/StampedLock", "<init>", "()V") => Ok(None),
        // --- VarHandle ---
        ("java/util/concurrent/atomic/VarHandle", "get", "(Ljava/lang/Object;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/atomic/VarHandle", "set", "(Ljava/lang/Object;Ljava/lang/Object;)V") => Ok(None),
        ("java/util/concurrent/atomic/VarHandle", "compareAndSet", "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/atomic/VarHandle", "weakCompareAndSet", "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/atomic/VarHandle", "getAndSet", "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        },
        // --- ConcurrentLinkedDeque ---
        ("java/util/concurrent/ConcurrentLinkedDeque", "addFirst", "(Ljava/lang/Object;)V") => Ok(None),
        ("java/util/concurrent/ConcurrentLinkedDeque", "addLast", "(Ljava/lang/Object;)V") => Ok(None),
        ("java/util/concurrent/ConcurrentLinkedDeque", "offerFirst", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/ConcurrentLinkedDeque", "offerLast", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/ConcurrentLinkedDeque", "pollFirst", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ConcurrentLinkedDeque", "pollLast", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ConcurrentLinkedDeque", "peekFirst", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ConcurrentLinkedDeque", "peekLast", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ConcurrentLinkedDeque", "removeFirst", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ConcurrentLinkedDeque", "removeLast", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ConcurrentLinkedDeque", "size", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ConcurrentLinkedDeque", "isEmpty", "()Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/ConcurrentLinkedDeque", "<init>", "()V") => Ok(None),
        ("java/util/concurrent/ConcurrentLinkedDeque", "<init>", "(Ljava/util/Collection;)V") => Ok(None),
        // --- LinkedBlockingQueue ---
        ("java/util/concurrent/LinkedBlockingQueue", "offer", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/LinkedBlockingQueue", "offer", "(Ljava/lang/Object;JLjava/util/concurrent/TimeUnit;)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/LinkedBlockingQueue", "poll", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/LinkedBlockingQueue", "poll", "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/LinkedBlockingQueue", "take", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/LinkedBlockingQueue", "peek", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/LinkedBlockingQueue", "size", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/LinkedBlockingQueue", "isEmpty", "()Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/LinkedBlockingQueue", "remainingCapacity", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/LinkedBlockingQueue", "clear", "()V") => Ok(None),
        ("java/util/concurrent/LinkedBlockingQueue", "contains", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/LinkedBlockingQueue", "<init>", "()V") => Ok(None),
        ("java/util/concurrent/LinkedBlockingQueue", "<init>", "(I)V") => Ok(None),
        ("java/util/concurrent/LinkedBlockingQueue", "<init>", "(Ljava/util/Collection;)V") => Ok(None),
        // --- SynchronousQueue ---
        ("java/util/concurrent/SynchronousQueue", "offer", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/SynchronousQueue", "offer", "(Ljava/lang/Object;JLjava/util/concurrent/TimeUnit;)Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/SynchronousQueue", "poll", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/SynchronousQueue", "poll", "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/SynchronousQueue", "isEmpty", "()Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/SynchronousQueue", "size", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/SynchronousQueue", "<init>", "()V") => Ok(None),
        ("java/util/concurrent/SynchronousQueue", "<init>", "(Z)V") => Ok(None),
        // --- PriorityBlockingQueue ---
        ("java/util/concurrent/PriorityBlockingQueue", "offer", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/PriorityBlockingQueue", "offer", "(Ljava/lang/Object;JLjava/util/concurrent/TimeUnit;)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/PriorityBlockingQueue", "poll", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/PriorityBlockingQueue", "poll", "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/PriorityBlockingQueue", "take", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/PriorityBlockingQueue", "peek", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/PriorityBlockingQueue", "size", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/PriorityBlockingQueue", "isEmpty", "()Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/PriorityBlockingQueue", "clear", "()V") => Ok(None),
        ("java/util/concurrent/PriorityBlockingQueue", "contains", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/PriorityBlockingQueue", "<init>", "()V") => Ok(None),
        ("java/util/concurrent/PriorityBlockingQueue", "<init>", "(I)V") => Ok(None),
        ("java/util/concurrent/PriorityBlockingQueue", "<init>", "(Ljava/util/Collection;)V") => Ok(None),
        ("java/util/concurrent/PriorityBlockingQueue", "<init>", "(ILjava/util/Comparator;)V") => Ok(None),
        // --- DelayQueue ---
        ("java/util/concurrent/DelayQueue", "offer", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/DelayQueue", "offer", "(Ljava/lang/Object;JLjava/util/concurrent/TimeUnit;)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/DelayQueue", "poll", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/DelayQueue", "poll", "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/DelayQueue", "take", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/DelayQueue", "peek", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/DelayQueue", "size", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/DelayQueue", "isEmpty", "()Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/DelayQueue", "clear", "()V") => Ok(None),
        ("java/util/concurrent/DelayQueue", "contains", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/DelayQueue", "<init>", "()V") => Ok(None),
        ("java/util/concurrent/DelayQueue", "<init>", "(Ljava/util/Collection;)V") => Ok(None),
        // --- LinkedBlockingDeque ---
        ("java/util/concurrent/LinkedBlockingDeque", "offerFirst", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/LinkedBlockingDeque", "offerLast", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/LinkedBlockingDeque", "offer", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/LinkedBlockingDeque", "offer", "(Ljava/lang/Object;JLjava/util/concurrent/TimeUnit;)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/LinkedBlockingDeque", "pollFirst", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/LinkedBlockingDeque", "pollLast", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/LinkedBlockingDeque", "poll", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/LinkedBlockingDeque", "poll", "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/LinkedBlockingDeque", "takeFirst", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/LinkedBlockingDeque", "takeLast", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/LinkedBlockingDeque", "peekFirst", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/LinkedBlockingDeque", "peekLast", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/LinkedBlockingDeque", "size", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/LinkedBlockingDeque", "isEmpty", "()Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/LinkedBlockingDeque", "clear", "()V") => Ok(None),
        ("java/util/concurrent/LinkedBlockingDeque", "contains", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/LinkedBlockingDeque", "<init>", "()V") => Ok(None),
        ("java/util/concurrent/LinkedBlockingDeque", "<init>", "(I)V") => Ok(None),
        ("java/util/concurrent/LinkedBlockingDeque", "<init>", "(Ljava/util/Collection;)V") => Ok(None),
        // --- AbstractExecutorService ---
        ("java/util/concurrent/AbstractExecutorService", "submit", "(Ljava/lang/Runnable;)Ljava/util/concurrent/Future;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/AbstractExecutorService", "submit", "(Ljava/lang/Runnable;Ljava/lang/Object;)Ljava/util/concurrent/Future;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/AbstractExecutorService", "submit", "(Ljava/util/concurrent/Callable;)Ljava/util/concurrent/Future;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/AbstractExecutorService", "invokeAll", "(Ljava/util/Collection;)Ljava/util/List;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/AbstractExecutorService", "invokeAll", "(Ljava/util/Collection;JLjava/util/concurrent/TimeUnit;)Ljava/util/List;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/AbstractExecutorService", "invokeAny", "(Ljava/util/Collection;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/AbstractExecutorService", "invokeAny", "(Ljava/util/Collection;JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        },
        // --- CompletionStage ---
        ("java/util/concurrent/CompletionStage", "thenApply", "(Ljava/util/function/Function;)Ljava/util/concurrent/CompletionStage;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/CompletionStage", "thenApplyAsync", "(Ljava/util/function/Function;)Ljava/util/concurrent/CompletionStage;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/CompletionStage", "thenAccept", "(Ljava/util/function/Consumer;)Ljava/util/concurrent/CompletionStage;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/CompletionStage", "thenAcceptAsync", "(Ljava/util/function/Consumer;)Ljava/util/concurrent/CompletionStage;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/CompletionStage", "thenRun", "(Ljava/lang/Runnable;)Ljava/util/concurrent/CompletionStage;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/CompletionStage", "thenRunAsync", "(Ljava/lang/Runnable;)Ljava/util/concurrent/CompletionStage;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/CompletionStage", "thenCombine", "(Ljava/util/concurrent/CompletionStage;Ljava/util/function/BiFunction;)Ljava/util/concurrent/CompletionStage;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/CompletionStage", "thenCompose", "(Ljava/util/function/Function;)Ljava/util/concurrent/CompletionStage;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/CompletionStage", "exceptionally", "(Ljava/util/function/Function;)Ljava/util/concurrent/CompletionStage;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/CompletionStage", "whenComplete", "(Ljava/util/function/BiConsumer;)Ljava/util/concurrent/CompletionStage;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/CompletionStage", "handle", "(Ljava/util/function/BiFunction;)Ljava/util/concurrent/CompletionStage;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        },
        // --- ConcurrentSkipListMap ---
        ("java/util/concurrent/ConcurrentSkipListMap", "size", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ConcurrentSkipListMap", "isEmpty", "()Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/ConcurrentSkipListMap", "get", "(Ljava/lang/Object;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ConcurrentSkipListMap", "put", "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ConcurrentSkipListMap", "remove", "(Ljava/lang/Object;)Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ConcurrentSkipListMap", "containsKey", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ConcurrentSkipListMap", "clear", "()V") => Ok(None),
        ("java/util/concurrent/ConcurrentSkipListMap", "firstKey", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ConcurrentSkipListMap", "lastKey", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ConcurrentSkipListMap", "<init>", "()V") => Ok(None),
        ("java/util/concurrent/ConcurrentSkipListMap", "<init>", "(Ljava/util/Comparator;)V") => Ok(None),
        ("java/util/concurrent/ConcurrentSkipListMap", "<init>", "(Ljava/util/Map;)V") => Ok(None),
        // --- ConcurrentSkipListSet ---
        ("java/util/concurrent/ConcurrentSkipListSet", "size", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ConcurrentSkipListSet", "isEmpty", "()Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/ConcurrentSkipListSet", "add", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/ConcurrentSkipListSet", "remove", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ConcurrentSkipListSet", "contains", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ConcurrentSkipListSet", "clear", "()V") => Ok(None),
        ("java/util/concurrent/ConcurrentSkipListSet", "first", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ConcurrentSkipListSet", "last", "()Ljava/lang/Object;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ConcurrentSkipListSet", "<init>", "()V") => Ok(None),
        ("java/util/concurrent/ConcurrentSkipListSet", "<init>", "(Ljava/util/Comparator;)V") => Ok(None),
        ("java/util/concurrent/ConcurrentSkipListSet", "<init>", "(Ljava/util/Collection;)V") => Ok(None),
        ("java/util/concurrent/ConcurrentSkipListSet", "<init>", "(Ljava/util/SortedSet;)V") => Ok(None),
        // --- ConcurrentHashMap.KeySetView ---
        ("java/util/concurrent/ConcurrentHashMap$KeySetView", "size", "()I") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ConcurrentHashMap$KeySetView", "isEmpty", "()Z") => Ok(Some(Value::Int(1))),
        ("java/util/concurrent/ConcurrentHashMap$KeySetView", "contains", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ConcurrentHashMap$KeySetView", "add", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ConcurrentHashMap$KeySetView", "remove", "(Ljava/lang/Object;)Z") => Ok(Some(Value::Int(0))),
        ("java/util/concurrent/ConcurrentHashMap$KeySetView", "getMap", "()Ljava/util/concurrent/ConcurrentHashMap;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("java/util/concurrent/ConcurrentHashMap$KeySetView", "<init>", "(Ljava/util/concurrent/ConcurrentHashMap;Ljava/lang/Object;)V") => Ok(None),
        // --- LongAccumulator ---
        ("java/util/concurrent/atomic/LongAccumulator", "<init>", "(Ljava/util/function/LongBinaryOperator;J)V") => Ok(None),
        ("java/util/concurrent/atomic/LongAccumulator", "get", "()J") => Ok(Some(Value::Long(0))),
        ("java/util/concurrent/atomic/LongAccumulator", "reset", "()V") => Ok(None),
        ("java/util/concurrent/atomic/LongAccumulator", "getThenReset", "()J") => Ok(Some(Value::Long(0))),
        ("java/util/concurrent/atomic/LongAccumulator", "accumulate", "(J)V") => Ok(None),
        // --- DoubleAccumulator ---
        ("java/util/concurrent/atomic/DoubleAccumulator", "<init>", "(Ljava/util/function/DoubleBinaryOperator;D)V") => Ok(None),
        ("java/util/concurrent/atomic/DoubleAccumulator", "get", "()D") => Ok(Some(Value::Double(0.0))),
        ("java/util/concurrent/atomic/DoubleAccumulator", "reset", "()V") => Ok(None),
        ("java/util/concurrent/atomic/DoubleAccumulator", "getThenReset", "()D") => Ok(Some(Value::Double(0.0))),
        ("java/util/concurrent/atomic/DoubleAccumulator", "accumulate", "(D)V") => Ok(None),
        // --- DoubleAdder ---
        ("java/util/concurrent/atomic/DoubleAdder", "add", "(D)V") => Ok(None),
        ("java/util/concurrent/atomic/DoubleAdder", "sum", "()D") => Ok(Some(Value::Double(0.0))),
        ("java/util/concurrent/atomic/DoubleAdder", "sumThenReset", "()D") => Ok(Some(Value::Double(0.0))),
        ("java/util/concurrent/atomic/DoubleAdder", "<init>", "()V") => Ok(None),
        _ => Err(VmError::UnhandledException {
            class_name: "".to_string(),
        }),
    }
}