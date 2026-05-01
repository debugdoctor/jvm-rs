pub(super) mod bootstrap;
pub(super) mod bootstrap_reflect;
pub(super) mod bootstrap_regex;
pub(super) mod bootstrap_text;
pub(super) mod bootstrap_time;
pub(super) mod format;
pub(super) mod helpers;
pub(super) mod invoke;
pub(super) mod invoke_concurrent;
pub(super) mod invoke_nio;
pub(super) mod invoke_other;
pub(super) mod invoke_reflect;
pub(super) mod invoke_regex;
pub(super) mod invoke_text;
pub(super) mod invoke_time;
pub(super) mod invoke_util;

use std::collections::HashMap;

use crate::vm::types::stub_return_value;
use crate::vm::{ClassMethod, HeapValue, Reference, RuntimeClass, Value, Vm, VmError};

fn is_not_handled(e: &VmError) -> bool {
    matches!(e, VmError::UnhandledException { class_name } if class_name.is_empty())
}

impl Vm {
    pub(super) fn bootstrap(&mut self) {
        bootstrap::bootstrap_java_lang(self);
        bootstrap::bootstrap_java_io(self);
        bootstrap::bootstrap_java_io_writer(self);
        bootstrap::bootstrap_java_util(self);
        bootstrap::bootstrap_java_nio(self);
        bootstrap::bootstrap_java_util_concurrent(self);
        bootstrap_reflect::bootstrap_java_lang_reflect(self);
        bootstrap_regex::bootstrap_java_util_regex(self);
        bootstrap_text::bootstrap_java_text(self);
        bootstrap_time::bootstrap_java_time(self);
        bootstrap::bootstrap_other(self);
    }

    pub(super) fn invoke_native(
        &mut self,
        class_name: &str,
        method_name: &str,
        descriptor: &str,
        args: &[Value],
    ) -> Result<Option<Value>, VmError> {
        let result = invoke::invoke_io(self, class_name, method_name, descriptor, args);
        match result {
            Ok(Some(v)) => return Ok(Some(v)),
            Ok(None) => return Ok(None),
            Err(e) if is_not_handled(&e) => {}
            Err(e) => return Err(e),
        }

        let result = invoke::invoke_lang(self, class_name, method_name, descriptor, args);
        match result {
            Ok(Some(v)) => return Ok(Some(v)),
            Ok(None) => return Ok(None),
            Err(e) if is_not_handled(&e) => {}
            Err(e) => return Err(e),
        }

        let result = invoke_util::invoke_util(self, class_name, method_name, descriptor, args);
        match result {
            Ok(Some(v)) => return Ok(Some(v)),
            Ok(None) => return Ok(None),
            Err(e) if is_not_handled(&e) => {}
            Err(e) => return Err(e),
        }

        let result = invoke_other::invoke_other(self, class_name, method_name, descriptor, args);
        match result {
            Ok(Some(v)) => return Ok(Some(v)),
            Ok(None) => return Ok(None),
            Err(e) if is_not_handled(&e) => {}
            Err(e) => return Err(e),
        }

        let result = invoke_nio::invoke_nio(self, class_name, method_name, descriptor, args);
        match result {
            Ok(Some(v)) => return Ok(Some(v)),
            Ok(None) => return Ok(None),
            Err(e) if is_not_handled(&e) => {}
            Err(e) => return Err(e),
        }

        let result =
            invoke_concurrent::invoke_concurrent(self, class_name, method_name, descriptor, args);
        match result {
            Ok(Some(v)) => return Ok(Some(v)),
            Ok(None) => return Ok(None),
            Err(e) if is_not_handled(&e) => {}
            Err(e) => return Err(e),
        }

        let result = invoke_text::invoke_text(self, class_name, method_name, descriptor, args);
        match result {
            Ok(Some(v)) => return Ok(Some(v)),
            Ok(None) => return Ok(None),
            Err(e) if is_not_handled(&e) => {}
            Err(e) => return Err(e),
        }

        let result = invoke_regex::invoke_regex(self, class_name, method_name, descriptor, args);
        match result {
            Ok(Some(v)) => return Ok(Some(v)),
            Ok(None) => return Ok(None),
            Err(e) if is_not_handled(&e) => {}
            Err(e) => return Err(e),
        }

        let result = invoke_time::invoke_time(self, class_name, method_name, descriptor, args);
        match result {
            Ok(Some(v)) => return Ok(Some(v)),
            Ok(None) => return Ok(None),
            Err(e) if is_not_handled(&e) => {}
            Err(e) => return Err(e),
        }

        let result =
            invoke_reflect::invoke_reflect(self, class_name, method_name, descriptor, args);
        match result {
            Ok(Some(v)) => return Ok(Some(v)),
            Ok(None) => return Ok(None),
            Err(e) if is_not_handled(&e) => {}
            Err(e) => return Err(e),
        }

        match (class_name, method_name, descriptor) {
            (cls, "<init>", "(Ljava/lang/String;)V")
                if crate::vm::builtin::helpers::is_throwable_class(self, cls)? =>
            {
                let obj_ref = args[0].as_reference()?;
                let message = args[1];
                self.set_object_field(obj_ref, "message", message)?;
                Ok(None)
            }
            (cls, "<init>", "(Ljava/lang/String;Ljava/lang/Throwable;)V")
                if crate::vm::builtin::helpers::is_throwable_class(self, cls)? =>
            {
                let obj_ref = args[0].as_reference()?;
                let message = args[1];
                self.set_object_field(obj_ref, "message", message)?;
                Ok(None)
            }
            (cls, "<init>", "(Ljava/lang/Throwable;)V")
                if crate::vm::builtin::helpers::is_throwable_class(self, cls)? =>
            {
                Ok(None)
            }
            (cls, "getMessage", "()Ljava/lang/String;")
                if crate::vm::builtin::helpers::is_throwable_class(self, cls)? =>
            {
                let obj_ref = args[0].as_reference()?;
                let msg = self.get_object_field(obj_ref, "message")?;
                Ok(Some(msg))
            }
            (_, "<init>", _) => Ok(None),
            _ => Err(VmError::UnsupportedNativeMethod {
                class_name: class_name.to_string(),
                method_name: method_name.to_string(),
                descriptor: descriptor.to_string(),
            }),
        }
    }

    fn native_int_stream_collect(
        &mut self,
        stream_ref: Reference,
        collector_ref: Reference,
    ) -> Result<Option<Value>, VmError> {
        let source_array = crate::vm::builtin::helpers::native_int_stream_array(self, stream_ref)?;
        let mode = crate::vm::builtin::helpers::native_collector_mode(self, collector_ref)?;
        let heap = self.heap.lock().unwrap();
        let elements: Vec<Reference> = match heap.get(source_array)? {
            HeapValue::IntArray { values } => values
                .iter()
                .map(|&v| Reference::Heap(v as usize))
                .collect(),
            _ => return Ok(None),
        };
        drop(heap);
        crate::vm::builtin::helpers::collect_with_mode(self, elements, mode, collector_ref)
    }

    fn native_long_stream_collect(
        &mut self,
        stream_ref: Reference,
        collector_ref: Reference,
    ) -> Result<Option<Value>, VmError> {
        let source_array = crate::vm::builtin::helpers::native_long_stream_array(self, stream_ref)?;
        let mode = crate::vm::builtin::helpers::native_collector_mode(self, collector_ref)?;
        let heap = self.heap.lock().unwrap();
        let elements: Vec<Reference> = match heap.get(source_array)? {
            HeapValue::LongArray { values } => values
                .iter()
                .map(|&v| Reference::Heap(v as usize))
                .collect(),
            _ => return Ok(None),
        };
        drop(heap);
        crate::vm::builtin::helpers::collect_with_mode(self, elements, mode, collector_ref)
    }

    fn native_double_stream_collect(
        &mut self,
        stream_ref: Reference,
        collector_ref: Reference,
    ) -> Result<Option<Value>, VmError> {
        let source_array =
            crate::vm::builtin::helpers::native_double_stream_array(self, stream_ref)?;
        let mode = crate::vm::builtin::helpers::native_collector_mode(self, collector_ref)?;
        let heap = self.heap.lock().unwrap();
        let elements: Vec<Reference> = match heap.get(source_array)? {
            HeapValue::DoubleArray { values } => values
                .iter()
                .map(|&v| Reference::Heap(v as usize))
                .collect(),
            _ => return Ok(None),
        };
        drop(heap);
        crate::vm::builtin::helpers::collect_with_mode(self, elements, mode, collector_ref)
    }

    fn native_collectors_to_list(&mut self) -> Result<Option<Value>, VmError> {
        let mut fields = std::collections::HashMap::new();
        fields.insert("__mode".to_string(), Value::Int(1));
        let r = self.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: "__jvm_rs/NativeCollector".to_string(),
            fields,
        });
        Ok(Some(Value::Reference(r)))
    }

    fn native_collectors_to_set(&mut self) -> Result<Option<Value>, VmError> {
        let mut fields = std::collections::HashMap::new();
        fields.insert("__mode".to_string(), Value::Int(2));
        let r = self.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: "__jvm_rs/NativeCollector".to_string(),
            fields,
        });
        Ok(Some(Value::Reference(r)))
    }

    fn native_collectors_counting(&mut self) -> Result<Option<Value>, VmError> {
        let mut fields = std::collections::HashMap::new();
        fields.insert("__mode".to_string(), Value::Int(3));
        let r = self.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: "__jvm_rs/NativeCollector".to_string(),
            fields,
        });
        Ok(Some(Value::Reference(r)))
    }

    fn native_collectors_joining(
        &mut self,
        delimiter: Option<Reference>,
    ) -> Result<Option<Value>, VmError> {
        let mut fields = std::collections::HashMap::new();
        if let Some(d) = delimiter {
            fields.insert("__mode".to_string(), Value::Int(5));
            fields.insert("__array".to_string(), Value::Reference(d));
        } else {
            fields.insert("__mode".to_string(), Value::Int(4));
        }
        let r = self.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: "__jvm_rs/NativeCollector".to_string(),
            fields,
        });
        Ok(Some(Value::Reference(r)))
    }

    fn native_collectors_reducing(
        &mut self,
        identity: Reference,
        _combiner: Reference,
    ) -> Result<Option<Value>, VmError> {
        let mut fields = std::collections::HashMap::new();
        fields.insert("__mode".to_string(), Value::Int(6));
        fields.insert("__array".to_string(), Value::Reference(identity));
        let r = self.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: "__jvm_rs/NativeCollector".to_string(),
            fields,
        });
        Ok(Some(Value::Reference(r)))
    }

    fn native_collectors_to_map(
        &mut self,
        key_mapper: Reference,
        value_mapper: Reference,
    ) -> Result<Option<Value>, VmError> {
        let mut fields = std::collections::HashMap::new();
        fields.insert("__mode".to_string(), Value::Int(7));
        fields.insert("__array".to_string(), Value::Reference(key_mapper));
        let r = self.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: "__jvm_rs/NativeCollector".to_string(),
            fields,
        });
        Ok(Some(Value::Reference(r)))
    }
}
