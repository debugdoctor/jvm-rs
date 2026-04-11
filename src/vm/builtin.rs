//! Built-in class registration and native method dispatch.
//!
//! This module bootstraps the core JDK classes (`java/lang/Object`,
//! `java/io/PrintStream`, `java/lang/System`) and provides the native
//! method implementations that back them.

use std::collections::BTreeMap;

use super::{ClassMethod, HeapValue, Reference, RuntimeClass, Value, Vm, VmError};

impl Vm {
    /// Register built-in classes required by the JVM specification.
    ///
    /// Creates the `java/lang/Object`, `java/io/PrintStream`, and
    /// `java/lang/System` classes with their native methods and
    /// initializes `System.out` to a `PrintStream` instance.
    pub(super) fn bootstrap(&mut self) {
        // java/lang/Object
        let mut object_methods = BTreeMap::new();
        object_methods.insert(
            ("<init>".to_string(), "()V".to_string()),
            ClassMethod::Native,
        );
        self.classes.insert(
            "java/lang/Object".to_string(),
            RuntimeClass {
                name: "java/lang/Object".to_string(),
                super_class: None,
                methods: object_methods,
                static_fields: BTreeMap::new(),
                instance_fields: vec![],
            },
        );

        // java/io/PrintStream
        let mut ps_methods = BTreeMap::new();
        for desc in [
            "()V",
            "(I)V",
            "(J)V",
            "(F)V",
            "(D)V",
            "(Z)V",
            "(C)V",
            "(Ljava/lang/String;)V",
        ] {
            ps_methods.insert(
                ("println".to_string(), desc.to_string()),
                ClassMethod::Native,
            );
            ps_methods.insert(
                ("print".to_string(), desc.to_string()),
                ClassMethod::Native,
            );
        }
        ps_methods.insert(
            ("<init>".to_string(), "()V".to_string()),
            ClassMethod::Native,
        );
        self.classes.insert(
            "java/io/PrintStream".to_string(),
            RuntimeClass {
                name: "java/io/PrintStream".to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods: ps_methods,
                static_fields: BTreeMap::new(),
                instance_fields: vec![],
            },
        );

        // Create the PrintStream instance for System.out
        let print_stream_ref = self.heap.allocate(HeapValue::Object {
            class_name: "java/io/PrintStream".to_string(),
            fields: BTreeMap::new(),
        });

        // java/lang/System
        let mut system_static = BTreeMap::new();
        system_static.insert("out".to_string(), Value::Reference(print_stream_ref));
        self.classes.insert(
            "java/lang/System".to_string(),
            RuntimeClass {
                name: "java/lang/System".to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods: BTreeMap::new(),
                static_fields: system_static,
                instance_fields: vec![],
            },
        );

        // java/lang/String
        let mut string_methods = BTreeMap::new();
        for (name, desc) in [
            ("<init>", "()V"),
            ("length", "()I"),
            ("charAt", "(I)I"),
            ("equals", "(Ljava/lang/Object;)Z"),
            ("hashCode", "()I"),
        ] {
            string_methods.insert(
                (name.to_string(), desc.to_string()),
                ClassMethod::Native,
            );
        }
        self.classes.insert(
            "java/lang/String".to_string(),
            RuntimeClass {
                name: "java/lang/String".to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods: string_methods,
                static_fields: BTreeMap::new(),
                instance_fields: vec![],
            },
        );

        // java/lang/Integer
        let mut integer_methods = BTreeMap::new();
        for (name, desc) in [
            ("<init>", "(I)V"),
            ("intValue", "()I"),
            ("valueOf", "(I)Ljava/lang/Integer;"),
            ("parseInt", "(Ljava/lang/String;)I"),
        ] {
            integer_methods.insert(
                (name.to_string(), desc.to_string()),
                ClassMethod::Native,
            );
        }
        self.classes.insert(
            "java/lang/Integer".to_string(),
            RuntimeClass {
                name: "java/lang/Integer".to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods: integer_methods,
                static_fields: BTreeMap::new(),
                instance_fields: vec![("value".to_string(), "I".to_string())],
            },
        );

        // java/lang/StringBuilder
        let mut sb_methods = BTreeMap::new();
        sb_methods.insert(
            ("<init>".to_string(), "()V".to_string()),
            ClassMethod::Native,
        );
        sb_methods.insert(
            ("<init>".to_string(), "(Ljava/lang/String;)V".to_string()),
            ClassMethod::Native,
        );
        for desc in [
            "(Ljava/lang/String;)Ljava/lang/StringBuilder;",
            "(I)Ljava/lang/StringBuilder;",
            "(J)Ljava/lang/StringBuilder;",
            "(C)Ljava/lang/StringBuilder;",
            "(Z)Ljava/lang/StringBuilder;",
            "(F)Ljava/lang/StringBuilder;",
            "(D)Ljava/lang/StringBuilder;",
            "(Ljava/lang/Object;)Ljava/lang/StringBuilder;",
        ] {
            sb_methods.insert(
                ("append".to_string(), desc.to_string()),
                ClassMethod::Native,
            );
        }
        sb_methods.insert(
            ("toString".to_string(), "()Ljava/lang/String;".to_string()),
            ClassMethod::Native,
        );
        sb_methods.insert(
            ("length".to_string(), "()I".to_string()),
            ClassMethod::Native,
        );
        self.classes.insert(
            "java/lang/StringBuilder".to_string(),
            RuntimeClass {
                name: "java/lang/StringBuilder".to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods: sb_methods,
                static_fields: BTreeMap::new(),
                instance_fields: vec![],
            },
        );

        // java/lang/Math
        let mut math_methods = BTreeMap::new();
        for (name, desc) in [
            ("max", "(II)I"),
            ("min", "(II)I"),
            ("abs", "(I)I"),
            ("max", "(JJ)J"),
            ("min", "(JJ)J"),
            ("abs", "(J)J"),
            ("max", "(DD)D"),
            ("min", "(DD)D"),
            ("abs", "(D)D"),
            ("sqrt", "(D)D"),
            ("pow", "(DD)D"),
        ] {
            math_methods.insert(
                (name.to_string(), desc.to_string()),
                ClassMethod::Native,
            );
        }
        self.classes.insert(
            "java/lang/Math".to_string(),
            RuntimeClass {
                name: "java/lang/Math".to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods: math_methods,
                static_fields: BTreeMap::new(),
                instance_fields: vec![],
            },
        );

        // Exception class hierarchy
        let exception_chain = [
            ("java/lang/Throwable", "java/lang/Object"),
            ("java/lang/Exception", "java/lang/Throwable"),
            ("java/lang/RuntimeException", "java/lang/Exception"),
            (
                "java/lang/ArithmeticException",
                "java/lang/RuntimeException",
            ),
            (
                "java/lang/NullPointerException",
                "java/lang/RuntimeException",
            ),
            (
                "java/lang/ClassCastException",
                "java/lang/RuntimeException",
            ),
            (
                "java/lang/NegativeArraySizeException",
                "java/lang/RuntimeException",
            ),
            (
                "java/lang/ArrayIndexOutOfBoundsException",
                "java/lang/RuntimeException",
            ),
            (
                "java/lang/IndexOutOfBoundsException",
                "java/lang/RuntimeException",
            ),
            (
                "java/lang/IllegalMonitorStateException",
                "java/lang/RuntimeException",
            ),
        ];
        for (name, parent) in exception_chain {
            let mut methods = BTreeMap::new();
            methods.insert(
                ("<init>".to_string(), "()V".to_string()),
                ClassMethod::Native,
            );
            self.classes.insert(
                name.to_string(),
                RuntimeClass {
                    name: name.to_string(),
                    super_class: Some(parent.to_string()),
                    methods,
                    static_fields: BTreeMap::new(),
                    instance_fields: vec![],
                },
            );
        }
    }

    /// Dispatch a native method call.
    ///
    /// For instance methods, `args[0]` is the receiver; for static methods the
    /// slice starts with the first declared parameter.
    pub(super) fn invoke_native(
        &mut self,
        class_name: &str,
        method_name: &str,
        descriptor: &str,
        args: &[Value],
    ) -> Result<Option<Value>, VmError> {
        match (class_name, method_name, descriptor) {
            // --- PrintStream.println ---
            ("java/io/PrintStream", "println", "(I)V") => {
                let line = args[1].as_int()?.to_string();
                println!("{line}");
                self.output.push(line);
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
                self.output.push(line);
                Ok(None)
            }
            ("java/io/PrintStream", "println", "(C)V") => {
                let ch = args[1].as_int()? as u8 as char;
                let line = ch.to_string();
                println!("{line}");
                self.output.push(line);
                Ok(None)
            }
            ("java/io/PrintStream", "println", "(Ljava/lang/String;)V") => {
                let reference = args[1].as_reference()?;
                let line = self.stringify_reference(reference)?;
                println!("{line}");
                self.output.push(line);
                Ok(None)
            }
            ("java/io/PrintStream", "println", "(J)V") => {
                let line = args[1].as_long()?.to_string();
                println!("{line}");
                self.output.push(line);
                Ok(None)
            }
            ("java/io/PrintStream", "println", "(F)V") => {
                let v = args[1].as_float()?;
                let line = format_float(v as f64);
                println!("{line}");
                self.output.push(line);
                Ok(None)
            }
            ("java/io/PrintStream", "println", "(D)V") => {
                let v = args[1].as_double()?;
                let line = format_float(v);
                println!("{line}");
                self.output.push(line);
                Ok(None)
            }
            ("java/io/PrintStream", "println", "()V") => {
                println!();
                self.output.push(String::new());
                Ok(None)
            }

            // --- PrintStream.print ---
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
                let text = self.stringify_reference(reference)?;
                print!("{text}");
                Ok(None)
            }
            ("java/io/PrintStream", "print", "(J)V") => {
                let text = args[1].as_long()?.to_string();
                print!("{text}");
                Ok(None)
            }
            ("java/io/PrintStream", "print", "(F)V") => {
                let text = format_float(args[1].as_float()? as f64);
                print!("{text}");
                Ok(None)
            }
            ("java/io/PrintStream", "print", "(D)V") => {
                let text = format_float(args[1].as_double()?);
                print!("{text}");
                Ok(None)
            }
            ("java/io/PrintStream", "print", "()V") => Ok(None),

            // --- String methods ---
            ("java/lang/String", "length", "()I") => {
                let s = self.stringify_reference(args[0].as_reference()?)?;
                Ok(Some(Value::Int(s.len() as i32)))
            }
            ("java/lang/String", "charAt", "(I)I") => {
                let s = self.stringify_reference(args[0].as_reference()?)?;
                let index = args[1].as_int()?;
                let ch = s.chars().nth(index as usize).unwrap_or('\0');
                Ok(Some(Value::Int(ch as i32)))
            }
            ("java/lang/String", "equals", "(Ljava/lang/Object;)Z") => {
                let a = self.stringify_reference(args[0].as_reference()?)?;
                let b_ref = args[1].as_reference()?;
                let result = match b_ref {
                    Reference::Null => 0,
                    _ => {
                        if let Ok(b) = self.stringify_reference(b_ref) {
                            if a == b { 1 } else { 0 }
                        } else {
                            0
                        }
                    }
                };
                Ok(Some(Value::Int(result)))
            }
            ("java/lang/String", "hashCode", "()I") => {
                let s = self.stringify_reference(args[0].as_reference()?)?;
                let mut h: i32 = 0;
                for ch in s.chars() {
                    h = h.wrapping_mul(31).wrapping_add(ch as i32);
                }
                Ok(Some(Value::Int(h)))
            }

            // --- Integer methods ---
            ("java/lang/Integer", "intValue", "()I") => {
                let obj_ref = args[0].as_reference()?;
                match self.heap.get(obj_ref)? {
                    HeapValue::Object { fields, .. } => {
                        let value = fields
                            .get("value")
                            .copied()
                            .unwrap_or(Value::Int(0));
                        Ok(Some(value))
                    }
                    _ => Ok(Some(Value::Int(0))),
                }
            }
            ("java/lang/Integer", "valueOf", "(I)Ljava/lang/Integer;") => {
                let value = args[0].as_int()?;
                let mut fields = BTreeMap::new();
                fields.insert("value".to_string(), Value::Int(value));
                let reference = self.heap.allocate(HeapValue::Object {
                    class_name: "java/lang/Integer".to_string(),
                    fields,
                });
                Ok(Some(Value::Reference(reference)))
            }
            ("java/lang/Integer", "parseInt", "(Ljava/lang/String;)I") => {
                let s = self.stringify_reference(args[0].as_reference()?)?;
                let value = s.parse::<i32>().unwrap_or(0);
                Ok(Some(Value::Int(value)))
            }

            // --- StringBuilder methods ---
            ("java/lang/StringBuilder", "<init>", "()V") => {
                // The receiver is already a StringBuilder heap value allocated by `new`.
                // But `new` creates a HeapValue::Object. We need to replace it with a
                // HeapValue::StringBuilder. Let's handle this by modifying the heap.
                let obj_ref = args[0].as_reference()?;
                *self.heap.get_mut(obj_ref)? = HeapValue::StringBuilder(std::string::String::new());
                Ok(None)
            }
            ("java/lang/StringBuilder", "<init>", "(Ljava/lang/String;)V") => {
                let obj_ref = args[0].as_reference()?;
                let s = self.stringify_reference(args[1].as_reference()?)?;
                *self.heap.get_mut(obj_ref)? = HeapValue::StringBuilder(s);
                Ok(None)
            }
            ("java/lang/StringBuilder", "append", _) => {
                let obj_ref = args[0].as_reference()?;
                let text = self.format_value_for_append(descriptor, &args[1..])?;
                if let HeapValue::StringBuilder(buf) = self.heap.get_mut(obj_ref)? {
                    buf.push_str(&text);
                }
                Ok(Some(Value::Reference(obj_ref)))
            }
            ("java/lang/StringBuilder", "toString", "()Ljava/lang/String;") => {
                let obj_ref = args[0].as_reference()?;
                let s = match self.heap.get(obj_ref)? {
                    HeapValue::StringBuilder(buf) => buf.clone(),
                    _ => std::string::String::new(),
                };
                Ok(Some(self.new_string(s)))
            }
            ("java/lang/StringBuilder", "length", "()I") => {
                let obj_ref = args[0].as_reference()?;
                let len = match self.heap.get(obj_ref)? {
                    HeapValue::StringBuilder(buf) => buf.len() as i32,
                    _ => 0,
                };
                Ok(Some(Value::Int(len)))
            }

            // --- Math methods ---
            ("java/lang/Math", "max", "(II)I") => {
                Ok(Some(Value::Int(args[0].as_int()?.max(args[1].as_int()?))))
            }
            ("java/lang/Math", "min", "(II)I") => {
                Ok(Some(Value::Int(args[0].as_int()?.min(args[1].as_int()?))))
            }
            ("java/lang/Math", "abs", "(I)I") => {
                Ok(Some(Value::Int(args[0].as_int()?.wrapping_abs())))
            }
            ("java/lang/Math", "max", "(JJ)J") => {
                Ok(Some(Value::Long(args[0].as_long()?.max(args[1].as_long()?))))
            }
            ("java/lang/Math", "min", "(JJ)J") => {
                Ok(Some(Value::Long(args[0].as_long()?.min(args[1].as_long()?))))
            }
            ("java/lang/Math", "abs", "(J)J") => {
                Ok(Some(Value::Long(args[0].as_long()?.wrapping_abs())))
            }
            ("java/lang/Math", "max", "(DD)D") => {
                Ok(Some(Value::Double(args[0].as_double()?.max(args[1].as_double()?))))
            }
            ("java/lang/Math", "min", "(DD)D") => {
                Ok(Some(Value::Double(args[0].as_double()?.min(args[1].as_double()?))))
            }
            ("java/lang/Math", "abs", "(D)D") => {
                Ok(Some(Value::Double(args[0].as_double()?.abs())))
            }
            ("java/lang/Math", "sqrt", "(D)D") => {
                Ok(Some(Value::Double(args[0].as_double()?.sqrt())))
            }
            ("java/lang/Math", "pow", "(DD)D") => {
                Ok(Some(Value::Double(args[0].as_double()?.powf(args[1].as_double()?))))
            }

            // --- Constructors ---
            ("java/lang/Integer", "<init>", "(I)V") => {
                let obj_ref = args[0].as_reference()?;
                let value = args[1].as_int()?;
                if let Ok(HeapValue::Object { fields, .. }) = self.heap.get_mut(obj_ref) {
                    fields.insert("value".to_string(), Value::Int(value));
                }
                Ok(None)
            }

            // Catch-all: any <init> on a built-in class that has no special
            // logic is treated as a no-op constructor.
            (_, "<init>", _) => Ok(None),

            _ => Err(VmError::UnsupportedNativeMethod {
                class_name: class_name.to_string(),
                method_name: method_name.to_string(),
                descriptor: descriptor.to_string(),
            }),
        }
    }

    /// Convert a value to a string for StringBuilder.append based on the descriptor.
    fn format_value_for_append(
        &self,
        descriptor: &str,
        args: &[Value],
    ) -> Result<std::string::String, VmError> {
        match descriptor {
            "(Ljava/lang/String;)Ljava/lang/StringBuilder;" => {
                self.stringify_reference(args[0].as_reference()?)
            }
            "(I)Ljava/lang/StringBuilder;" => Ok(args[0].as_int()?.to_string()),
            "(J)Ljava/lang/StringBuilder;" => Ok(args[0].as_long()?.to_string()),
            "(C)Ljava/lang/StringBuilder;" => {
                Ok((args[0].as_int()? as u16 as u32)
                    .try_into()
                    .map(|c: char| c.to_string())
                    .unwrap_or_default())
            }
            "(Z)Ljava/lang/StringBuilder;" => {
                Ok(if args[0].as_int()? != 0 { "true" } else { "false" }.to_string())
            }
            "(F)Ljava/lang/StringBuilder;" => Ok(format_float(args[0].as_float()? as f64)),
            "(D)Ljava/lang/StringBuilder;" => Ok(format_float(args[0].as_double()?)),
            "(Ljava/lang/Object;)Ljava/lang/StringBuilder;" => {
                let r = args[0].as_reference()?;
                if r == Reference::Null {
                    Ok("null".to_string())
                } else {
                    // Try to stringify; fall back to class@hash
                    self.stringify_reference(r)
                        .or_else(|_| Ok(format!("Object@{r:?}")))
                }
            }
            _ => Ok("?".to_string()),
        }
    }

    /// Resolve a heap string reference to its Rust `String` value.
    pub(super) fn stringify_reference(&self, reference: Reference) -> Result<String, VmError> {
        match reference {
            Reference::Null => Ok("null".to_string()),
            _ => match self.heap.get(reference)? {
                HeapValue::String(value) => Ok(value.clone()),
                value => Err(VmError::InvalidHeapValue {
                    expected: "string",
                    actual: value.kind_name(),
                }),
            },
        }
    }
}

/// Format a float/double the same way Java does: always include a decimal point.
fn format_float(v: f64) -> String {
    if v.is_nan() {
        "NaN".to_string()
    } else if v.is_infinite() {
        if v > 0.0 { "Infinity".to_string() } else { "-Infinity".to_string() }
    } else if v == 0.0 && v.is_sign_negative() {
        "-0.0".to_string()
    } else {
        let s = format!("{v}");
        if s.contains('.') { s } else { format!("{v}.0") }
    }
}
