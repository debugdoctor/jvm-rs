//! Built-in class registration and native method dispatch.
//!
//! This module bootstraps the core JDK classes (`java/lang/Object`,
//! `java/io/PrintStream`, `java/lang/System`, `java/lang/Thread`) and provides the native
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
        for (name, desc) in [
            ("<init>", "()V"),
            ("wait", "()V"),
            ("notify", "()V"),
            ("notifyAll", "()V"),
        ] {
            object_methods.insert(
                (name.to_string(), desc.to_string()),
                ClassMethod::Native,
            );
        }
        self.register_class(RuntimeClass {
                name: "java/lang/Object".to_string(),
                super_class: None,
                methods: object_methods,
                static_fields: BTreeMap::new(),
                instance_fields: vec![],
                interfaces: vec![],
            });

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
        self.register_class(RuntimeClass {
                name: "java/io/PrintStream".to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods: ps_methods,
                static_fields: BTreeMap::new(),
                instance_fields: vec![],
                interfaces: vec![],
            });

        // Create the PrintStream instance for System.out
        let print_stream_ref = self.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: "java/io/PrintStream".to_string(),
            fields: BTreeMap::new(),
        });

        // Create a second PrintStream instance for System.err
        let err_stream_ref = self.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: "java/io/PrintStream".to_string(),
            fields: BTreeMap::new(),
        });

        // java/lang/System
        let mut system_static = BTreeMap::new();
        system_static.insert("out".to_string(), Value::Reference(print_stream_ref));
        system_static.insert("err".to_string(), Value::Reference(err_stream_ref));
        let mut system_methods = BTreeMap::new();
        for (name, desc) in [
            ("currentTimeMillis", "()J"),
            ("nanoTime", "()J"),
            ("arraycopy", "(Ljava/lang/Object;ILjava/lang/Object;II)V"),
            ("exit", "(I)V"),
            ("getProperty", "(Ljava/lang/String;)Ljava/lang/String;"),
            ("lineSeparator", "()Ljava/lang/String;"),
            ("identityHashCode", "(Ljava/lang/Object;)I"),
        ] {
            system_methods.insert(
                (name.to_string(), desc.to_string()),
                ClassMethod::Native,
            );
        }
        self.register_class(RuntimeClass {
                name: "java/lang/System".to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods: system_methods,
                static_fields: system_static,
                instance_fields: vec![],
                interfaces: vec![],
            });

        // java/lang/String
        let mut string_methods = BTreeMap::new();
        for (name, desc) in [
            ("<init>", "()V"),
            ("length", "()I"),
            ("charAt", "(I)I"),
            ("equals", "(Ljava/lang/Object;)Z"),
            ("hashCode", "()I"),
            ("isEmpty", "()Z"),
            ("trim", "()Ljava/lang/String;"),
            ("toLowerCase", "()Ljava/lang/String;"),
            ("toUpperCase", "()Ljava/lang/String;"),
            ("toString", "()Ljava/lang/String;"),
            ("concat", "(Ljava/lang/String;)Ljava/lang/String;"),
            ("substring", "(I)Ljava/lang/String;"),
            ("substring", "(II)Ljava/lang/String;"),
            ("indexOf", "(I)I"),
            ("indexOf", "(Ljava/lang/String;)I"),
            ("startsWith", "(Ljava/lang/String;)Z"),
            ("endsWith", "(Ljava/lang/String;)Z"),
            ("contains", "(Ljava/lang/CharSequence;)Z"),
            ("replace", "(CC)Ljava/lang/String;"),
            ("compareTo", "(Ljava/lang/String;)I"),
            ("valueOf", "(I)Ljava/lang/String;"),
            ("valueOf", "(J)Ljava/lang/String;"),
            ("valueOf", "(Z)Ljava/lang/String;"),
            ("valueOf", "(C)Ljava/lang/String;"),
            ("valueOf", "(D)Ljava/lang/String;"),
            ("valueOf", "(F)Ljava/lang/String;"),
            ("valueOf", "(Ljava/lang/Object;)Ljava/lang/String;"),
        ] {
            string_methods.insert(
                (name.to_string(), desc.to_string()),
                ClassMethod::Native,
            );
        }
        self.register_class(RuntimeClass {
                name: "java/lang/String".to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods: string_methods,
                static_fields: BTreeMap::new(),
                instance_fields: vec![],
                interfaces: vec![],
            });

        // java/lang/Integer
        let mut integer_methods = BTreeMap::new();
        for (name, desc) in [
            ("<init>", "(I)V"),
            ("intValue", "()I"),
            ("valueOf", "(I)Ljava/lang/Integer;"),
            ("parseInt", "(Ljava/lang/String;)I"),
            ("parseInt", "(Ljava/lang/String;I)I"),
            ("toString", "(I)Ljava/lang/String;"),
            ("toString", "(II)Ljava/lang/String;"),
            ("toBinaryString", "(I)Ljava/lang/String;"),
            ("toHexString", "(I)Ljava/lang/String;"),
            ("toOctalString", "(I)Ljava/lang/String;"),
            ("compare", "(II)I"),
        ] {
            integer_methods.insert(
                (name.to_string(), desc.to_string()),
                ClassMethod::Native,
            );
        }
        self.register_class(RuntimeClass {
                name: "java/lang/Integer".to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods: integer_methods,
                static_fields: BTreeMap::new(),
                instance_fields: vec![("value".to_string(), "I".to_string())],
                interfaces: vec![],
            });

        // java/lang/Long
        let mut long_methods = BTreeMap::new();
        for (name, desc) in [
            ("<init>", "(J)V"),
            ("longValue", "()J"),
            ("valueOf", "(J)Ljava/lang/Long;"),
            ("parseLong", "(Ljava/lang/String;)J"),
            ("toString", "(J)Ljava/lang/String;"),
            ("compare", "(JJ)I"),
        ] {
            long_methods.insert(
                (name.to_string(), desc.to_string()),
                ClassMethod::Native,
            );
        }
        self.register_class(RuntimeClass {
                name: "java/lang/Long".to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods: long_methods,
                static_fields: BTreeMap::new(),
                instance_fields: vec![("value".to_string(), "J".to_string())],
                interfaces: vec![],
            });

        // java/lang/Character
        let mut character_methods = BTreeMap::new();
        for (name, desc) in [
            ("isDigit", "(C)Z"),
            ("isLetter", "(C)Z"),
            ("isLetterOrDigit", "(C)Z"),
            ("isWhitespace", "(C)Z"),
            ("isUpperCase", "(C)Z"),
            ("isLowerCase", "(C)Z"),
            ("toLowerCase", "(C)C"),
            ("toUpperCase", "(C)C"),
            ("toString", "(C)Ljava/lang/String;"),
        ] {
            character_methods.insert(
                (name.to_string(), desc.to_string()),
                ClassMethod::Native,
            );
        }
        self.register_class(RuntimeClass {
                name: "java/lang/Character".to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods: character_methods,
                static_fields: BTreeMap::new(),
                instance_fields: vec![],
                interfaces: vec![],
            });

        // java/lang/Boolean
        let mut boolean_methods = BTreeMap::new();
        for (name, desc) in [
            ("parseBoolean", "(Ljava/lang/String;)Z"),
            ("toString", "(Z)Ljava/lang/String;"),
            ("valueOf", "(Z)Ljava/lang/Boolean;"),
            ("booleanValue", "()Z"),
        ] {
            boolean_methods.insert(
                (name.to_string(), desc.to_string()),
                ClassMethod::Native,
            );
        }
        self.register_class(RuntimeClass {
                name: "java/lang/Boolean".to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods: boolean_methods,
                static_fields: BTreeMap::new(),
                instance_fields: vec![("value".to_string(), "Z".to_string())],
                interfaces: vec![],
            });

        // java/util/Objects
        let mut objects_methods = BTreeMap::new();
        for (name, desc) in [
            ("requireNonNull", "(Ljava/lang/Object;)Ljava/lang/Object;"),
            ("requireNonNull", "(Ljava/lang/Object;Ljava/lang/String;)Ljava/lang/Object;"),
            ("equals", "(Ljava/lang/Object;Ljava/lang/Object;)Z"),
            ("isNull", "(Ljava/lang/Object;)Z"),
            ("nonNull", "(Ljava/lang/Object;)Z"),
        ] {
            objects_methods.insert(
                (name.to_string(), desc.to_string()),
                ClassMethod::Native,
            );
        }
        self.register_class(RuntimeClass {
                name: "java/util/Objects".to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods: objects_methods,
                static_fields: BTreeMap::new(),
                instance_fields: vec![],
                interfaces: vec![],
            });

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
        for (name, desc) in [
            ("charAt", "(I)C"),
            ("setLength", "(I)V"),
            ("deleteCharAt", "(I)Ljava/lang/StringBuilder;"),
            ("setCharAt", "(IC)V"),
            ("reverse", "()Ljava/lang/StringBuilder;"),
            ("insert", "(ILjava/lang/String;)Ljava/lang/StringBuilder;"),
        ] {
            sb_methods.insert(
                (name.to_string(), desc.to_string()),
                ClassMethod::Native,
            );
        }
        self.register_class(RuntimeClass {
                name: "java/lang/StringBuilder".to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods: sb_methods,
                static_fields: BTreeMap::new(),
                instance_fields: vec![],
                interfaces: vec![],
            });

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
            ("floor", "(D)D"),
            ("ceil", "(D)D"),
            ("round", "(D)J"),
            ("round", "(F)I"),
            ("random", "()D"),
            ("log", "(D)D"),
            ("log10", "(D)D"),
            ("exp", "(D)D"),
            ("sin", "(D)D"),
            ("cos", "(D)D"),
            ("tan", "(D)D"),
        ] {
            math_methods.insert(
                (name.to_string(), desc.to_string()),
                ClassMethod::Native,
            );
        }
        self.register_class(RuntimeClass {
                name: "java/lang/Math".to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods: math_methods,
                static_fields: BTreeMap::new(),
                instance_fields: vec![],
                interfaces: vec![],
            });

        // java/lang/Runnable
        self.register_class(RuntimeClass {
                name: "java/lang/Runnable".to_string(),
                super_class: None,
                methods: BTreeMap::new(),
                static_fields: BTreeMap::new(),
                instance_fields: vec![],
                interfaces: vec![],
            });

        // java/lang/Thread
        let mut thread_methods = BTreeMap::new();
        for (name, desc) in [
            ("<init>", "()V"),
            ("<init>", "(Ljava/lang/Runnable;)V"),
            ("start", "()V"),
            ("run", "()V"),
            ("join", "()V"),
        ] {
            thread_methods.insert(
                (name.to_string(), desc.to_string()),
                ClassMethod::Native,
            );
        }
        self.register_class(RuntimeClass {
                name: "java/lang/Thread".to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods: thread_methods,
                static_fields: BTreeMap::new(),
                instance_fields: vec![("target".to_string(), "Ljava/lang/Runnable;".to_string())],
                interfaces: vec![],
            });

        // Exception class hierarchy
        let exception_chain = [
            ("java/lang/Throwable", "java/lang/Object"),
            ("java/lang/Exception", "java/lang/Throwable"),
            ("java/lang/RuntimeException", "java/lang/Exception"),
            (
                "java/lang/IllegalThreadStateException",
                "java/lang/RuntimeException",
            ),
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
            for (mname, mdesc) in [
                ("<init>", "()V"),
                ("<init>", "(Ljava/lang/String;)V"),
                ("<init>", "(Ljava/lang/String;Ljava/lang/Throwable;)V"),
                ("<init>", "(Ljava/lang/Throwable;)V"),
                ("getMessage", "()Ljava/lang/String;"),
            ] {
                methods.insert(
                    (mname.to_string(), mdesc.to_string()),
                    ClassMethod::Native,
                );
            }
            self.register_class(RuntimeClass {
                    name: name.to_string(),
                    super_class: Some(parent.to_string()),
                    methods,
                    static_fields: BTreeMap::new(),
                    instance_fields: vec![("message".to_string(), "Ljava/lang/String;".to_string())],
                    interfaces: vec![],
                });
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
                self.output.lock().unwrap().push(line);
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
                self.output.lock().unwrap().push(line);
                Ok(None)
            }
            ("java/io/PrintStream", "println", "(C)V") => {
                let ch = args[1].as_int()? as u8 as char;
                let line = ch.to_string();
                println!("{line}");
                self.output.lock().unwrap().push(line);
                Ok(None)
            }
            ("java/io/PrintStream", "println", "(Ljava/lang/String;)V") => {
                let reference = args[1].as_reference()?;
                let line = self.stringify_reference(reference)?;
                println!("{line}");
                self.output.lock().unwrap().push(line);
                Ok(None)
            }
            ("java/io/PrintStream", "println", "(J)V") => {
                let line = args[1].as_long()?.to_string();
                println!("{line}");
                self.output.lock().unwrap().push(line);
                Ok(None)
            }
            ("java/io/PrintStream", "println", "(F)V") => {
                let v = args[1].as_float()?;
                let line = format_float(v as f64);
                println!("{line}");
                self.output.lock().unwrap().push(line);
                Ok(None)
            }
            ("java/io/PrintStream", "println", "(D)V") => {
                let v = args[1].as_double()?;
                let line = format_float(v);
                println!("{line}");
                self.output.lock().unwrap().push(line);
                Ok(None)
            }
            ("java/io/PrintStream", "println", "()V") => {
                println!();
                self.output.lock().unwrap().push(String::new());
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
            ("java/lang/Object", "wait", "()V") => {
                self.wait_on_monitor(args[0].as_reference()?)?;
                Ok(None)
            }
            ("java/lang/Object", "notify", "()V") => {
                self.notify_monitor(args[0].as_reference()?, false)?;
                Ok(None)
            }
            ("java/lang/Object", "notifyAll", "()V") => {
                self.notify_monitor(args[0].as_reference()?, true)?;
                Ok(None)
            }
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
            ("java/lang/String", "isEmpty", "()Z") => {
                let s = self.stringify_reference(args[0].as_reference()?)?;
                Ok(Some(Value::Int(if s.is_empty() { 1 } else { 0 })))
            }
            ("java/lang/String", "trim", "()Ljava/lang/String;") => {
                let s = self.stringify_reference(args[0].as_reference()?)?;
                Ok(Some(self.new_string(s.trim().to_string())))
            }
            ("java/lang/String", "toLowerCase", "()Ljava/lang/String;") => {
                let s = self.stringify_reference(args[0].as_reference()?)?;
                Ok(Some(self.new_string(s.to_lowercase())))
            }
            ("java/lang/String", "toUpperCase", "()Ljava/lang/String;") => {
                let s = self.stringify_reference(args[0].as_reference()?)?;
                Ok(Some(self.new_string(s.to_uppercase())))
            }
            ("java/lang/String", "toString", "()Ljava/lang/String;") => {
                Ok(Some(Value::Reference(args[0].as_reference()?)))
            }
            ("java/lang/String", "concat", "(Ljava/lang/String;)Ljava/lang/String;") => {
                let mut a = self.stringify_reference(args[0].as_reference()?)?;
                let b = self.stringify_reference(args[1].as_reference()?)?;
                a.push_str(&b);
                Ok(Some(self.new_string(a)))
            }
            ("java/lang/String", "substring", "(I)Ljava/lang/String;") => {
                let s = self.stringify_reference(args[0].as_reference()?)?;
                let start = args[1].as_int()?;
                let chars: Vec<char> = s.chars().collect();
                let len = chars.len() as i32;
                if start < 0 || start > len {
                    return Err(VmError::UnhandledException {
                        class_name: "java/lang/StringIndexOutOfBoundsException".to_string(),
                    });
                }
                let sub: String = chars[start as usize..].iter().collect();
                Ok(Some(self.new_string(sub)))
            }
            ("java/lang/String", "substring", "(II)Ljava/lang/String;") => {
                let s = self.stringify_reference(args[0].as_reference()?)?;
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
                Ok(Some(self.new_string(sub)))
            }
            ("java/lang/String", "indexOf", "(I)I") => {
                let s = self.stringify_reference(args[0].as_reference()?)?;
                let ch = args[1].as_int()? as u32;
                let needle = char::from_u32(ch).unwrap_or('\0');
                let pos = s.chars().position(|c| c == needle);
                Ok(Some(Value::Int(pos.map(|p| p as i32).unwrap_or(-1))))
            }
            ("java/lang/String", "indexOf", "(Ljava/lang/String;)I") => {
                let s = self.stringify_reference(args[0].as_reference()?)?;
                let needle = self.stringify_reference(args[1].as_reference()?)?;
                let pos = match s.find(&needle) {
                    Some(byte_pos) => s[..byte_pos].chars().count() as i32,
                    None => -1,
                };
                Ok(Some(Value::Int(pos)))
            }
            ("java/lang/String", "startsWith", "(Ljava/lang/String;)Z") => {
                let s = self.stringify_reference(args[0].as_reference()?)?;
                let prefix = self.stringify_reference(args[1].as_reference()?)?;
                Ok(Some(Value::Int(if s.starts_with(&prefix) { 1 } else { 0 })))
            }
            ("java/lang/String", "endsWith", "(Ljava/lang/String;)Z") => {
                let s = self.stringify_reference(args[0].as_reference()?)?;
                let suffix = self.stringify_reference(args[1].as_reference()?)?;
                Ok(Some(Value::Int(if s.ends_with(&suffix) { 1 } else { 0 })))
            }
            ("java/lang/String", "contains", "(Ljava/lang/CharSequence;)Z") => {
                let s = self.stringify_reference(args[0].as_reference()?)?;
                let needle = self.stringify_reference(args[1].as_reference()?)?;
                Ok(Some(Value::Int(if s.contains(&needle) { 1 } else { 0 })))
            }
            ("java/lang/String", "replace", "(CC)Ljava/lang/String;") => {
                let s = self.stringify_reference(args[0].as_reference()?)?;
                let from = char::from_u32(args[1].as_int()? as u32).unwrap_or('\0');
                let to = char::from_u32(args[2].as_int()? as u32).unwrap_or('\0');
                let result: String = s.chars().map(|c| if c == from { to } else { c }).collect();
                Ok(Some(self.new_string(result)))
            }
            ("java/lang/String", "compareTo", "(Ljava/lang/String;)I") => {
                let a = self.stringify_reference(args[0].as_reference()?)?;
                let b = self.stringify_reference(args[1].as_reference()?)?;
                let cmp = match a.cmp(&b) {
                    std::cmp::Ordering::Less => -1,
                    std::cmp::Ordering::Equal => 0,
                    std::cmp::Ordering::Greater => 1,
                };
                Ok(Some(Value::Int(cmp)))
            }
            ("java/lang/String", "valueOf", "(I)Ljava/lang/String;") => {
                Ok(Some(self.new_string(args[0].as_int()?.to_string())))
            }
            ("java/lang/String", "valueOf", "(J)Ljava/lang/String;") => {
                Ok(Some(self.new_string(args[0].as_long()?.to_string())))
            }
            ("java/lang/String", "valueOf", "(Z)Ljava/lang/String;") => {
                let s = if args[0].as_int()? != 0 { "true" } else { "false" };
                Ok(Some(self.new_string(s.to_string())))
            }
            ("java/lang/String", "valueOf", "(C)Ljava/lang/String;") => {
                let ch = char::from_u32(args[0].as_int()? as u32).unwrap_or('\0');
                Ok(Some(self.new_string(ch.to_string())))
            }
            ("java/lang/String", "valueOf", "(D)Ljava/lang/String;") => {
                Ok(Some(self.new_string(format_float(args[0].as_double()?))))
            }
            ("java/lang/String", "valueOf", "(F)Ljava/lang/String;") => {
                Ok(Some(self.new_string(format_float(args[0].as_float()? as f64))))
            }
            ("java/lang/String", "valueOf", "(Ljava/lang/Object;)Ljava/lang/String;") => {
                let r = args[0].as_reference()?;
                let text = if r == Reference::Null {
                    "null".to_string()
                } else {
                    self.stringify_reference(r).unwrap_or_else(|_| format!("Object@{r:?}"))
                };
                Ok(Some(self.new_string(text)))
            }

            // --- Integer methods ---
            ("java/lang/Integer", "intValue", "()I") => {
                let obj_ref = args[0].as_reference()?;
                match self.heap.lock().unwrap().get(obj_ref)? {
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
                let reference = self.heap.lock().unwrap().allocate(HeapValue::Object {
                    class_name: "java/lang/Integer".to_string(),
                    fields,
                });
                Ok(Some(Value::Reference(reference)))
            }
            ("java/lang/Integer", "parseInt", "(Ljava/lang/String;)I") => {
                let s = self.stringify_reference(args[0].as_reference()?)?;
                let value = s.parse::<i32>().map_err(|_| VmError::UnhandledException {
                    class_name: "java/lang/NumberFormatException".to_string(),
                })?;
                Ok(Some(Value::Int(value)))
            }
            ("java/lang/Integer", "parseInt", "(Ljava/lang/String;I)I") => {
                let s = self.stringify_reference(args[0].as_reference()?)?;
                let radix = args[1].as_int()? as u32;
                let value = i32::from_str_radix(&s, radix).map_err(|_| {
                    VmError::UnhandledException {
                        class_name: "java/lang/NumberFormatException".to_string(),
                    }
                })?;
                Ok(Some(Value::Int(value)))
            }
            ("java/lang/Integer", "toString", "(I)Ljava/lang/String;") => {
                Ok(Some(self.new_string(args[0].as_int()?.to_string())))
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
                // For negative in non-10, Java uses two's-complement; emulate for common radices.
                let s = if value < 0 && radix != 10 {
                    format!("-{}", format_unsigned_radix(value.unsigned_abs() as u64, radix))
                } else {
                    s
                };
                Ok(Some(self.new_string(s)))
            }
            ("java/lang/Integer", "toBinaryString", "(I)Ljava/lang/String;") => {
                Ok(Some(self.new_string(format!("{:b}", args[0].as_int()? as u32))))
            }
            ("java/lang/Integer", "toHexString", "(I)Ljava/lang/String;") => {
                Ok(Some(self.new_string(format!("{:x}", args[0].as_int()? as u32))))
            }
            ("java/lang/Integer", "toOctalString", "(I)Ljava/lang/String;") => {
                Ok(Some(self.new_string(format!("{:o}", args[0].as_int()? as u32))))
            }
            ("java/lang/Integer", "compare", "(II)I") => {
                let a = args[0].as_int()?;
                let b = args[1].as_int()?;
                Ok(Some(Value::Int(a.cmp(&b) as i32)))
            }

            // --- Long methods ---
            ("java/lang/Long", "<init>", "(J)V") => {
                let obj_ref = args[0].as_reference()?;
                let value = args[1].as_long()?;
                if let Ok(HeapValue::Object { fields, .. }) = self.heap.lock().unwrap().get_mut(obj_ref) {
                    fields.insert("value".to_string(), Value::Long(value));
                }
                Ok(None)
            }
            ("java/lang/Long", "longValue", "()J") => {
                let obj_ref = args[0].as_reference()?;
                match self.heap.lock().unwrap().get(obj_ref)? {
                    HeapValue::Object { fields, .. } => Ok(Some(
                        fields.get("value").copied().unwrap_or(Value::Long(0)),
                    )),
                    _ => Ok(Some(Value::Long(0))),
                }
            }
            ("java/lang/Long", "valueOf", "(J)Ljava/lang/Long;") => {
                let value = args[0].as_long()?;
                let mut fields = BTreeMap::new();
                fields.insert("value".to_string(), Value::Long(value));
                let reference = self.heap.lock().unwrap().allocate(HeapValue::Object {
                    class_name: "java/lang/Long".to_string(),
                    fields,
                });
                Ok(Some(Value::Reference(reference)))
            }
            ("java/lang/Long", "parseLong", "(Ljava/lang/String;)J") => {
                let s = self.stringify_reference(args[0].as_reference()?)?;
                let value = s.parse::<i64>().map_err(|_| VmError::UnhandledException {
                    class_name: "java/lang/NumberFormatException".to_string(),
                })?;
                Ok(Some(Value::Long(value)))
            }
            ("java/lang/Long", "toString", "(J)Ljava/lang/String;") => {
                Ok(Some(self.new_string(args[0].as_long()?.to_string())))
            }
            ("java/lang/Long", "compare", "(JJ)I") => {
                let a = args[0].as_long()?;
                let b = args[1].as_long()?;
                Ok(Some(Value::Int(a.cmp(&b) as i32)))
            }

            // --- Character methods ---
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
                Ok(Some(self.new_string(ch.to_string())))
            }

            // --- Boolean methods ---
            ("java/lang/Boolean", "parseBoolean", "(Ljava/lang/String;)Z") => {
                let s = self.stringify_reference(args[0].as_reference()?)?;
                Ok(Some(Value::Int(if s.eq_ignore_ascii_case("true") { 1 } else { 0 })))
            }
            ("java/lang/Boolean", "toString", "(Z)Ljava/lang/String;") => {
                let s = if args[0].as_int()? != 0 { "true" } else { "false" };
                Ok(Some(self.new_string(s.to_string())))
            }
            ("java/lang/Boolean", "valueOf", "(Z)Ljava/lang/Boolean;") => {
                let value = args[0].as_int()?;
                let mut fields = BTreeMap::new();
                fields.insert("value".to_string(), Value::Int(value));
                let reference = self.heap.lock().unwrap().allocate(HeapValue::Object {
                    class_name: "java/lang/Boolean".to_string(),
                    fields,
                });
                Ok(Some(Value::Reference(reference)))
            }
            ("java/lang/Boolean", "booleanValue", "()Z") => {
                let obj_ref = args[0].as_reference()?;
                match self.heap.lock().unwrap().get(obj_ref)? {
                    HeapValue::Object { fields, .. } => Ok(Some(
                        fields.get("value").copied().unwrap_or(Value::Int(0)),
                    )),
                    _ => Ok(Some(Value::Int(0))),
                }
            }

            // --- Objects utility methods ---
            ("java/util/Objects", "requireNonNull", "(Ljava/lang/Object;)Ljava/lang/Object;") => {
                let r = args[0].as_reference()?;
                if r == Reference::Null {
                    return Err(VmError::UnhandledException {
                        class_name: "java/lang/NullPointerException".to_string(),
                    });
                }
                Ok(Some(Value::Reference(r)))
            }
            (
                "java/util/Objects",
                "requireNonNull",
                "(Ljava/lang/Object;Ljava/lang/String;)Ljava/lang/Object;",
            ) => {
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
                        // Compare string heap values by content; else by reference identity.
                        match (
                            self.stringify_reference(a).ok(),
                            self.stringify_reference(b).ok(),
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

            // --- System methods ---
            ("java/lang/System", "currentTimeMillis", "()J") => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                Ok(Some(Value::Long(now)))
            }
            ("java/lang/System", "nanoTime", "()J") => {
                use std::time::Instant;
                // Use a monotonic clock. Anchor to a process-start baseline so repeated
                // calls return a monotonically increasing nanosecond count.
                static BASELINE: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
                let base = BASELINE.get_or_init(Instant::now);
                Ok(Some(Value::Long(base.elapsed().as_nanos() as i64)))
            }
            (
                "java/lang/System",
                "arraycopy",
                "(Ljava/lang/Object;ILjava/lang/Object;II)V",
            ) => {
                let src = args[0].as_reference()?;
                let src_pos = args[1].as_int()?;
                let dst = args[2].as_reference()?;
                let dst_pos = args[3].as_int()?;
                let length = args[4].as_int()?;
                self.arraycopy(src, src_pos, dst, dst_pos, length)?;
                Ok(None)
            }
            ("java/lang/System", "exit", "(I)V") => {
                let code = args[0].as_int()?;
                std::process::exit(code);
            }
            ("java/lang/System", "getProperty", "(Ljava/lang/String;)Ljava/lang/String;") => {
                let key = self.stringify_reference(args[0].as_reference()?)?;
                let value = match key.as_str() {
                    "line.separator" => Some("\n".to_string()),
                    "file.separator" => Some(std::path::MAIN_SEPARATOR.to_string()),
                    "path.separator" => Some(if cfg!(windows) { ";".to_string() } else { ":".to_string() }),
                    "java.version" => Some("21".to_string()),
                    "java.specification.version" => Some("21".to_string()),
                    "os.name" => Some(std::env::consts::OS.to_string()),
                    "os.arch" => Some(std::env::consts::ARCH.to_string()),
                    other => std::env::var(other).ok(),
                };
                match value {
                    Some(v) => Ok(Some(self.new_string(v))),
                    None => Ok(Some(Value::Reference(Reference::Null))),
                }
            }
            ("java/lang/System", "lineSeparator", "()Ljava/lang/String;") => {
                Ok(Some(self.new_string("\n".to_string())))
            }
            ("java/lang/System", "identityHashCode", "(Ljava/lang/Object;)I") => {
                let r = args[0].as_reference()?;
                let hash = match r {
                    Reference::Null => 0,
                    Reference::Heap(i) => i as i32,
                };
                Ok(Some(Value::Int(hash)))
            }

            // --- Math (extended) ---
            ("java/lang/Math", "floor", "(D)D") => {
                Ok(Some(Value::Double(args[0].as_double()?.floor())))
            }
            ("java/lang/Math", "ceil", "(D)D") => {
                Ok(Some(Value::Double(args[0].as_double()?.ceil())))
            }
            ("java/lang/Math", "round", "(D)J") => {
                let v = args[0].as_double()?;
                // Java's Math.round(double): (long) Math.floor(d + 0.5)
                let r = (v + 0.5).floor() as i64;
                Ok(Some(Value::Long(r)))
            }
            ("java/lang/Math", "round", "(F)I") => {
                let v = args[0].as_float()?;
                let r = (v + 0.5).floor() as i32;
                Ok(Some(Value::Int(r)))
            }
            ("java/lang/Math", "random", "()D") => {
                // Simple deterministic-ish PRNG using a process-wide counter +
                // nanosecond seed. Avoids adding an rng dependency.
                use std::sync::atomic::{AtomicU64, Ordering};
                static STATE: AtomicU64 = AtomicU64::new(0x9E3779B97F4A7C15);
                // xorshift*
                let mut x = STATE.load(Ordering::Relaxed);
                if x == 0 {
                    x = 0x9E3779B97F4A7C15;
                }
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                STATE.store(x, Ordering::Relaxed);
                // Produce a [0, 1) double using the top 53 bits.
                let bits = (x >> 11) & ((1u64 << 53) - 1);
                let v = bits as f64 / ((1u64 << 53) as f64);
                Ok(Some(Value::Double(v)))
            }
            ("java/lang/Math", "log", "(D)D") => {
                Ok(Some(Value::Double(args[0].as_double()?.ln())))
            }
            ("java/lang/Math", "log10", "(D)D") => {
                Ok(Some(Value::Double(args[0].as_double()?.log10())))
            }
            ("java/lang/Math", "exp", "(D)D") => {
                Ok(Some(Value::Double(args[0].as_double()?.exp())))
            }
            ("java/lang/Math", "sin", "(D)D") => {
                Ok(Some(Value::Double(args[0].as_double()?.sin())))
            }
            ("java/lang/Math", "cos", "(D)D") => {
                Ok(Some(Value::Double(args[0].as_double()?.cos())))
            }
            ("java/lang/Math", "tan", "(D)D") => {
                Ok(Some(Value::Double(args[0].as_double()?.tan())))
            }

            // --- StringBuilder methods ---
            ("java/lang/StringBuilder", "<init>", "()V") => {
                // The receiver is already a StringBuilder heap value allocated by `new`.
                // But `new` creates a HeapValue::Object. We need to replace it with a
                // HeapValue::StringBuilder. Let's handle this by modifying the heap.
                let obj_ref = args[0].as_reference()?;
                *self.heap.lock().unwrap().get_mut(obj_ref)? =
                    HeapValue::StringBuilder(std::string::String::new());
                Ok(None)
            }
            ("java/lang/StringBuilder", "<init>", "(Ljava/lang/String;)V") => {
                let obj_ref = args[0].as_reference()?;
                let s = self.stringify_reference(args[1].as_reference()?)?;
                *self.heap.lock().unwrap().get_mut(obj_ref)? = HeapValue::StringBuilder(s);
                Ok(None)
            }
            ("java/lang/StringBuilder", "append", _) => {
                let obj_ref = args[0].as_reference()?;
                let text = self.format_value_for_append(descriptor, &args[1..])?;
                if let HeapValue::StringBuilder(buf) = self.heap.lock().unwrap().get_mut(obj_ref)? {
                    buf.push_str(&text);
                }
                Ok(Some(Value::Reference(obj_ref)))
            }
            ("java/lang/StringBuilder", "toString", "()Ljava/lang/String;") => {
                let obj_ref = args[0].as_reference()?;
                let s = match self.heap.lock().unwrap().get(obj_ref)? {
                    HeapValue::StringBuilder(buf) => buf.clone(),
                    _ => std::string::String::new(),
                };
                Ok(Some(self.new_string(s)))
            }
            ("java/lang/StringBuilder", "length", "()I") => {
                let obj_ref = args[0].as_reference()?;
                let len = match self.heap.lock().unwrap().get(obj_ref)? {
                    HeapValue::StringBuilder(buf) => buf.chars().count() as i32,
                    _ => 0,
                };
                Ok(Some(Value::Int(len)))
            }
            ("java/lang/StringBuilder", "charAt", "(I)C") => {
                let obj_ref = args[0].as_reference()?;
                let index = args[1].as_int()?;
                let ch = match self.heap.lock().unwrap().get(obj_ref)? {
                    HeapValue::StringBuilder(buf) => {
                        buf.chars().nth(index as usize).ok_or_else(|| {
                            VmError::UnhandledException {
                                class_name: "java/lang/StringIndexOutOfBoundsException"
                                    .to_string(),
                            }
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
                if let HeapValue::StringBuilder(buf) =
                    self.heap.lock().unwrap().get_mut(obj_ref)?
                {
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
                if let HeapValue::StringBuilder(buf) =
                    self.heap.lock().unwrap().get_mut(obj_ref)?
                {
                    let mut chars: Vec<char> = buf.chars().collect();
                    if index < 0 || (index as usize) >= chars.len() {
                        return Err(VmError::UnhandledException {
                            class_name: "java/lang/StringIndexOutOfBoundsException"
                                .to_string(),
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
                if let HeapValue::StringBuilder(buf) =
                    self.heap.lock().unwrap().get_mut(obj_ref)?
                {
                    let mut chars: Vec<char> = buf.chars().collect();
                    if index < 0 || (index as usize) >= chars.len() {
                        return Err(VmError::UnhandledException {
                            class_name: "java/lang/StringIndexOutOfBoundsException"
                                .to_string(),
                        });
                    }
                    chars[index as usize] = ch;
                    *buf = chars.into_iter().collect();
                }
                Ok(None)
            }
            ("java/lang/StringBuilder", "reverse", "()Ljava/lang/StringBuilder;") => {
                let obj_ref = args[0].as_reference()?;
                if let HeapValue::StringBuilder(buf) =
                    self.heap.lock().unwrap().get_mut(obj_ref)?
                {
                    *buf = buf.chars().rev().collect();
                }
                Ok(Some(Value::Reference(obj_ref)))
            }
            (
                "java/lang/StringBuilder",
                "insert",
                "(ILjava/lang/String;)Ljava/lang/StringBuilder;",
            ) => {
                let obj_ref = args[0].as_reference()?;
                let index = args[1].as_int()?;
                let s = self.stringify_reference(args[2].as_reference()?)?;
                if let HeapValue::StringBuilder(buf) =
                    self.heap.lock().unwrap().get_mut(obj_ref)?
                {
                    let mut chars: Vec<char> = buf.chars().collect();
                    let n = chars.len() as i32;
                    if index < 0 || index > n {
                        return Err(VmError::UnhandledException {
                            class_name: "java/lang/StringIndexOutOfBoundsException"
                                .to_string(),
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
                if let Ok(HeapValue::Object { fields, .. }) = self.heap.lock().unwrap().get_mut(obj_ref) {
                    fields.insert("value".to_string(), Value::Int(value));
                }
                Ok(None)
            }
            ("java/lang/Thread", "<init>", "()V") => {
                let obj_ref = args[0].as_reference()?;
                self.set_object_field(obj_ref, "target", Value::Reference(Reference::Null))?;
                Ok(None)
            }
            ("java/lang/Thread", "<init>", "(Ljava/lang/Runnable;)V") => {
                let obj_ref = args[0].as_reference()?;
                self.set_object_field(obj_ref, "target", args[1])?;
                Ok(None)
            }
            ("java/lang/Thread", "start", "()V") => {
                let thread_ref = args[0].as_reference()?;
                let target = self.get_object_field(thread_ref, "target")?.as_reference()?;
                let receiver = if target == Reference::Null {
                    thread_ref
                } else {
                    target
                };
                let class_name = self.get_object_class(receiver)?;
                self.start_java_thread(
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
                let target = self.get_object_field(thread_ref, "target")?.as_reference()?;
                if target != Reference::Null {
                    let class_name = self.get_object_class(target)?;
                    let (resolved_class, class_method) =
                        self.resolve_method(&class_name, "run", "()V")?;
                    match class_method {
                        ClassMethod::Native => {
                            self.invoke_native(
                                &resolved_class,
                                "run",
                                "()V",
                                &[Value::Reference(target)],
                            )?;
                        }
                        ClassMethod::Bytecode(method) => {
                            let callee =
                                method.with_initial_locals(vec![Some(Value::Reference(target))]);
                            let _ = self.execute(callee)?;
                        }
                    }
                }
                Ok(None)
            }
            ("java/lang/Thread", "join", "()V") => {
                let thread_ref = args[0].as_reference()?;
                self.join_java_thread(thread_ref)?;
                Ok(None)
            }

            // --- Throwable-family constructors that stash a message ---
            (cls, "<init>", "(Ljava/lang/String;)V")
                if self.is_throwable_class(cls)? =>
            {
                let obj_ref = args[0].as_reference()?;
                let message = args[1];
                self.set_object_field(obj_ref, "message", message)?;
                Ok(None)
            }
            (cls, "<init>", "(Ljava/lang/String;Ljava/lang/Throwable;)V")
                if self.is_throwable_class(cls)? =>
            {
                let obj_ref = args[0].as_reference()?;
                let message = args[1];
                self.set_object_field(obj_ref, "message", message)?;
                Ok(None)
            }
            (cls, "<init>", "(Ljava/lang/Throwable;)V")
                if self.is_throwable_class(cls)? =>
            {
                Ok(None)
            }
            (cls, "getMessage", "()Ljava/lang/String;")
                if self.is_throwable_class(cls)? =>
            {
                let obj_ref = args[0].as_reference()?;
                let msg = self.get_object_field(obj_ref, "message")?;
                Ok(Some(msg))
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
            _ => match self.heap.lock().unwrap().get(reference)? {
                HeapValue::String(value) => Ok(value.clone()),
                value => Err(VmError::InvalidHeapValue {
                    expected: "string",
                    actual: value.kind_name(),
                }),
            },
        }
    }

    /// Whether `class_name` is `java/lang/Throwable` or one of its subclasses.
    fn is_throwable_class(&mut self, class_name: &str) -> Result<bool, VmError> {
        self.is_instance_of(class_name, "java/lang/Throwable")
    }

    /// Copy `length` elements between typed arrays, matching `System.arraycopy` semantics
    /// for the homogeneous primitive cases and reference-array copies we support.
    fn arraycopy(
        &mut self,
        src: Reference,
        src_pos: i32,
        dst: Reference,
        dst_pos: i32,
        length: i32,
    ) -> Result<(), VmError> {
        if length < 0 || src_pos < 0 || dst_pos < 0 {
            return Err(VmError::UnhandledException {
                class_name: "java/lang/ArrayIndexOutOfBoundsException".to_string(),
            });
        }
        let src_pos = src_pos as usize;
        let dst_pos = dst_pos as usize;
        let length = length as usize;

        // Snapshot the source slice, then mutate the destination; avoids holding
        // a borrow across mutations when src and dst are different heap objects.
        let src_kind;
        let src_slice_int;
        let src_slice_long;
        let src_slice_float;
        let src_slice_double;
        let src_slice_ref;
        {
            let heap = self.heap.lock().unwrap();
            let value = heap.get(src)?;
            match value {
                HeapValue::IntArray { values } => {
                    if src_pos + length > values.len() {
                        return Err(VmError::UnhandledException {
                            class_name: "java/lang/ArrayIndexOutOfBoundsException"
                                .to_string(),
                        });
                    }
                    src_kind = "I";
                    src_slice_int = values[src_pos..src_pos + length].to_vec();
                    src_slice_long = Vec::new();
                    src_slice_float = Vec::new();
                    src_slice_double = Vec::new();
                    src_slice_ref = Vec::new();
                }
                HeapValue::LongArray { values } => {
                    if src_pos + length > values.len() {
                        return Err(VmError::UnhandledException {
                            class_name: "java/lang/ArrayIndexOutOfBoundsException"
                                .to_string(),
                        });
                    }
                    src_kind = "J";
                    src_slice_int = Vec::new();
                    src_slice_long = values[src_pos..src_pos + length].to_vec();
                    src_slice_float = Vec::new();
                    src_slice_double = Vec::new();
                    src_slice_ref = Vec::new();
                }
                HeapValue::FloatArray { values } => {
                    if src_pos + length > values.len() {
                        return Err(VmError::UnhandledException {
                            class_name: "java/lang/ArrayIndexOutOfBoundsException"
                                .to_string(),
                        });
                    }
                    src_kind = "F";
                    src_slice_int = Vec::new();
                    src_slice_long = Vec::new();
                    src_slice_float = values[src_pos..src_pos + length].to_vec();
                    src_slice_double = Vec::new();
                    src_slice_ref = Vec::new();
                }
                HeapValue::DoubleArray { values } => {
                    if src_pos + length > values.len() {
                        return Err(VmError::UnhandledException {
                            class_name: "java/lang/ArrayIndexOutOfBoundsException"
                                .to_string(),
                        });
                    }
                    src_kind = "D";
                    src_slice_int = Vec::new();
                    src_slice_long = Vec::new();
                    src_slice_float = Vec::new();
                    src_slice_double = values[src_pos..src_pos + length].to_vec();
                    src_slice_ref = Vec::new();
                }
                HeapValue::ReferenceArray { values, .. } => {
                    if src_pos + length > values.len() {
                        return Err(VmError::UnhandledException {
                            class_name: "java/lang/ArrayIndexOutOfBoundsException"
                                .to_string(),
                        });
                    }
                    src_kind = "L";
                    src_slice_int = Vec::new();
                    src_slice_long = Vec::new();
                    src_slice_float = Vec::new();
                    src_slice_double = Vec::new();
                    src_slice_ref = values[src_pos..src_pos + length].to_vec();
                }
                other => {
                    return Err(VmError::InvalidHeapValue {
                        expected: "array",
                        actual: other.kind_name(),
                    });
                }
            }
        }

        let mut heap = self.heap.lock().unwrap();
        match (src_kind, heap.get_mut(dst)?) {
            ("I", HeapValue::IntArray { values }) => {
                if dst_pos + length > values.len() {
                    return Err(VmError::UnhandledException {
                        class_name: "java/lang/ArrayIndexOutOfBoundsException".to_string(),
                    });
                }
                values[dst_pos..dst_pos + length].copy_from_slice(&src_slice_int);
            }
            ("J", HeapValue::LongArray { values }) => {
                if dst_pos + length > values.len() {
                    return Err(VmError::UnhandledException {
                        class_name: "java/lang/ArrayIndexOutOfBoundsException".to_string(),
                    });
                }
                values[dst_pos..dst_pos + length].copy_from_slice(&src_slice_long);
            }
            ("F", HeapValue::FloatArray { values }) => {
                if dst_pos + length > values.len() {
                    return Err(VmError::UnhandledException {
                        class_name: "java/lang/ArrayIndexOutOfBoundsException".to_string(),
                    });
                }
                values[dst_pos..dst_pos + length].copy_from_slice(&src_slice_float);
            }
            ("D", HeapValue::DoubleArray { values }) => {
                if dst_pos + length > values.len() {
                    return Err(VmError::UnhandledException {
                        class_name: "java/lang/ArrayIndexOutOfBoundsException".to_string(),
                    });
                }
                values[dst_pos..dst_pos + length].copy_from_slice(&src_slice_double);
            }
            ("L", HeapValue::ReferenceArray { values, .. }) => {
                if dst_pos + length > values.len() {
                    return Err(VmError::UnhandledException {
                        class_name: "java/lang/ArrayIndexOutOfBoundsException".to_string(),
                    });
                }
                values[dst_pos..dst_pos + length].copy_from_slice(&src_slice_ref);
            }
            _ => {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/ArrayStoreException".to_string(),
                });
            }
        }
        Ok(())
    }
}

fn format_unsigned_radix(mut value: u64, radix: u32) -> String {
    if value == 0 {
        return "0".to_string();
    }
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut buf = Vec::new();
    while value > 0 {
        buf.push(digits[(value % radix as u64) as usize]);
        value /= radix as u64;
    }
    buf.reverse();
    String::from_utf8(buf).unwrap()
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
