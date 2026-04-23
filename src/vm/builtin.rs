//! Built-in class registration and native method dispatch.
//!
//! This module bootstraps the core JDK classes (`java/lang/Object`,
//! `java/io/PrintStream`, `java/lang/System`, `java/lang/Thread`) and provides the native
//! method implementations that back them.

use std::collections::HashMap;

use super::types::stub_return_value;
use super::{ClassMethod, HeapValue, Reference, RuntimeClass, Value, Vm, VmError};

impl Vm {
    /// Register built-in classes required by the JVM specification.
    ///
    /// Creates the `java/lang/Object`, `java/io/PrintStream`, and
    /// `java/lang/System` classes with their native methods and
    /// initializes `System.out` to a `PrintStream` instance.
    pub(super) fn bootstrap(&mut self) {
        // java/lang/Object
        let mut object_methods = HashMap::new();
        for (name, desc) in [
            ("<init>", "()V"),
            ("wait", "()V"),
            ("notify", "()V"),
            ("notifyAll", "()V"),
            ("hashCode", "()I"),
            ("equals", "(Ljava/lang/Object;)Z"),
            ("toString", "()Ljava/lang/String;"),
            ("getClass", "()Ljava/lang/Class;"),
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
                static_fields: HashMap::new(),
                instance_fields: vec![],
                interfaces: vec![],
            });

        // java/lang/Class — registered as a minimal stub so the VM can mint
        // Class heap objects for `ldc class` without triggering the real
        // JDK's `java/lang/Class` initialization (which transitively pulls
        // in ClassLoader, Module, security, reflection, etc. — a rabbit
        // hole that stalls startup). Bytecode that calls simple Class
        // methods dispatches to these native stubs. `__name` is an internal
        // string field holding the internal class name (e.g.
        // `java/util/HashMap`); `getName` converts it to dotted form.
        let mut class_methods = HashMap::new();
        for (name, desc) in [
            ("desiredAssertionStatus", "()Z"),
            ("getName", "()Ljava/lang/String;"),
            ("getSimpleName", "()Ljava/lang/String;"),
            ("isArray", "()Z"),
            ("isInterface", "()Z"),
            ("isPrimitive", "()Z"),
            ("toString", "()Ljava/lang/String;"),
        ] {
            class_methods.insert(
                (name.to_string(), desc.to_string()),
                ClassMethod::Native,
            );
        }
        self.register_class(RuntimeClass {
                name: "java/lang/Class".to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods: class_methods,
                static_fields: HashMap::new(),
                instance_fields: vec![("__name".to_string(), "Ljava/lang/String;".to_string())],
                interfaces: vec![],
            });

        // java/io/PrintStream
        let mut ps_methods = HashMap::new();
        for desc in [
            "()V",
            "(I)V",
            "(J)V",
            "(F)V",
            "(D)V",
            "(Z)V",
            "(C)V",
            "(Ljava/lang/String;)V",
            "(Ljava/lang/Object;)V",
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
                static_fields: HashMap::new(),
                instance_fields: vec![],
                interfaces: vec![],
            });

        // Create the PrintStream instance for System.out
        let print_stream_ref = self.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: "java/io/PrintStream".to_string(),
            fields: HashMap::new(),
        });

        // Create a second PrintStream instance for System.err
        let err_stream_ref = self.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: "java/io/PrintStream".to_string(),
            fields: HashMap::new(),
        });

        // java/lang/System
        let mut system_static = HashMap::new();
        system_static.insert("out".to_string(), Value::Reference(print_stream_ref));
        system_static.insert("err".to_string(), Value::Reference(err_stream_ref));
        let mut system_methods = HashMap::new();
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
        let mut string_methods = HashMap::new();
        for (name, desc) in [
            ("<init>", "()V"),
            ("length", "()I"),
            ("charAt", "(I)C"),
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
            ("compareTo", "(Ljava/lang/Object;)I"),
            ("valueOf", "(I)Ljava/lang/String;"),
            ("valueOf", "(J)Ljava/lang/String;"),
            ("valueOf", "(Z)Ljava/lang/String;"),
            ("valueOf", "(C)Ljava/lang/String;"),
            ("valueOf", "(D)Ljava/lang/String;"),
            ("valueOf", "(F)Ljava/lang/String;"),
            // Note: valueOf(Ljava/lang/Object;) is NOT registered here - it's a real
            // Java method that calls toString() on the object, not a native method
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
                static_fields: HashMap::new(),
                instance_fields: vec![],
                interfaces: vec![],
            });

        // java/lang/Integer
        let mut integer_methods = HashMap::new();
        for (name, desc) in [
            ("<init>", "(I)V"),
            ("intValue", "()I"),
            ("valueOf", "(I)Ljava/lang/Integer;"),
            ("parseInt", "(Ljava/lang/String;)I"),
            ("parseInt", "(Ljava/lang/String;I)I"),
            ("compareTo", "(Ljava/lang/Integer;)I"),
            ("compareTo", "(Ljava/lang/Object;)I"),
            ("toString", "(I)Ljava/lang/String;"),
            ("toString", "(II)Ljava/lang/String;"),
            ("toBinaryString", "(I)Ljava/lang/String;"),
            ("toHexString", "(I)Ljava/lang/String;"),
            ("toOctalString", "(I)Ljava/lang/String;"),
            ("compare", "(II)I"),
            ("numberOfLeadingZeros", "(I)I"),
            ("numberOfTrailingZeros", "(I)I"),
            ("bitCount", "(I)I"),
            ("reverse", "(I)I"),
            ("reverseBytes", "(I)I"),
            ("highestOneBit", "(I)I"),
            ("lowestOneBit", "(I)I"),
            ("signum", "(I)I"),
        ] {
            integer_methods.insert(
                (name.to_string(), desc.to_string()),
                ClassMethod::Native,
            );
        }
        let mut integer_static = HashMap::new();
        integer_static.insert("MIN_VALUE".to_string(), Value::Int(i32::MIN));
        integer_static.insert("MAX_VALUE".to_string(), Value::Int(i32::MAX));
        integer_static.insert("SIZE".to_string(), Value::Int(32));
        integer_static.insert("BYTES".to_string(), Value::Int(4));
        // `Integer.TYPE` is the primitive-int Class mirror. Seed it with our
        // own Class object so bytecode that does `Integer.TYPE == otherClass`
        // comparisons or passes it to Array.newInstance sees a non-null.
        let int_type = self.class_object("int");
        integer_static.insert("TYPE".to_string(), Value::Reference(int_type));
        self.register_class(RuntimeClass {
                name: "java/lang/Integer".to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods: integer_methods,
                static_fields: integer_static,
                instance_fields: vec![("value".to_string(), "I".to_string())],
                interfaces: vec![],
            });

        // java/lang/Long
        let mut long_methods = HashMap::new();
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
                static_fields: HashMap::new(),
                instance_fields: vec![("value".to_string(), "J".to_string())],
                interfaces: vec![],
            });

        // java/lang/Character
        let mut character_methods = HashMap::new();
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
                static_fields: HashMap::new(),
                instance_fields: vec![],
                interfaces: vec![],
            });

        // java/lang/Boolean
        let mut boolean_methods = HashMap::new();
        for (name, desc) in [
            ("parseBoolean", "(Ljava/lang/String;)Z"),
            ("toString", "(Z)Ljava/lang/String;"),
            ("valueOf", "(Z)Ljava/lang/Boolean;"),
            ("booleanValue", "()Z"),
            ("getBoolean", "(Ljava/lang/String;)Z"),
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
                static_fields: HashMap::new(),
                instance_fields: vec![("value".to_string(), "Z".to_string())],
                interfaces: vec![],
            });

        
        let mut sb_methods = HashMap::new();
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
                static_fields: HashMap::new(),
                instance_fields: vec![],
                interfaces: vec![],
            });

        // java/lang/Math
        let mut math_methods = HashMap::new();
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
            ("floorDiv", "(II)I"),
            ("floorDiv", "(JJ)J"),
            ("floorMod", "(II)I"),
            ("floorMod", "(JJ)J"),
            ("addExact", "(II)I"),
            ("addExact", "(JJ)J"),
            ("subtractExact", "(II)I"),
            ("multiplyExact", "(II)I"),
            ("multiplyExact", "(JJ)J"),
            ("signum", "(I)I"),
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
                static_fields: HashMap::new(),
                instance_fields: vec![],
                interfaces: vec![],
            });

        // java/lang/Runnable
        self.register_class(RuntimeClass {
                name: "java/lang/Runnable".to_string(),
                super_class: None,
                methods: HashMap::new(),
                static_fields: HashMap::new(),
                instance_fields: vec![],
                interfaces: vec![],
            });

        // jdk/internal/misc/Unsafe — registered as a builtin stub so the
        // real JDK class (which depends on native intrinsics we don't
        // implement) never loads. Methods that user/library bytecode
        // actually reaches go through `invoke_native`; anything unlisted
        // falls through to the Unsafe wildcard arm in invoke_native.
        let mut unsafe_methods = HashMap::new();
        for (name, desc) in [
            ("registerNatives", "()V"),
            ("getUnsafe", "()Ljdk/internal/misc/Unsafe;"),
            ("arrayBaseOffset", "(Ljava/lang/Class;)I"),
            ("arrayIndexScale", "(Ljava/lang/Class;)I"),
            ("addressSize", "()I"),
            ("pageSize", "()I"),
            ("objectFieldOffset", "(Ljava/lang/reflect/Field;)J"),
            ("staticFieldOffset", "(Ljava/lang/reflect/Field;)J"),
            ("staticFieldBase", "(Ljava/lang/reflect/Field;)Ljava/lang/Object;"),
            ("allocateMemory", "(J)J"),
            ("freeMemory", "(J)V"),
            ("compareAndSetInt", "(Ljava/lang/Object;JII)Z"),
            ("compareAndSetLong", "(Ljava/lang/Object;JJJ)Z"),
            ("compareAndSetReference", "(Ljava/lang/Object;JLjava/lang/Object;Ljava/lang/Object;)Z"),
            ("compareAndSetObject", "(Ljava/lang/Object;JLjava/lang/Object;Ljava/lang/Object;)Z"),
            ("getReferenceVolatile", "(Ljava/lang/Object;J)Ljava/lang/Object;"),
            ("putReferenceVolatile", "(Ljava/lang/Object;JLjava/lang/Object;)V"),
            ("getIntVolatile", "(Ljava/lang/Object;J)I"),
            ("putIntVolatile", "(Ljava/lang/Object;JI)V"),
            ("storeFence", "()V"),
            ("loadFence", "()V"),
            ("fullFence", "()V"),
        ] {
            unsafe_methods.insert(
                (name.to_string(), desc.to_string()),
                ClassMethod::Native,
            );
        }
        let unsafe_instance_ref = self.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: "jdk/internal/misc/Unsafe".to_string(),
            fields: HashMap::new(),
        });
        let mut unsafe_static = HashMap::new();
        unsafe_static.insert("theUnsafe".to_string(), Value::Reference(unsafe_instance_ref));
        // ARRAY_*_BASE_OFFSET / INDEX_SCALE are normally filled from the
        // real Unsafe's native <clinit>. We seed plausible values so JDK
        // helpers like ArraysSupport that read them don't trip
        // FieldNotFound. BASE_OFFSET=0, INDEX_SCALE=1 treats logical index
        // as byte offset — which matches how our native CAS/getReference
        // stubs (non-authoritative) handle the `offset` parameter anyway.
        for prim in [
            "BOOLEAN", "BYTE", "SHORT", "CHAR", "INT", "LONG", "FLOAT", "DOUBLE", "OBJECT",
        ] {
            unsafe_static.insert(
                format!("ARRAY_{prim}_BASE_OFFSET"),
                Value::Int(0),
            );
            unsafe_static.insert(
                format!("ARRAY_{prim}_INDEX_SCALE"),
                Value::Int(1),
            );
        }
        unsafe_static.insert("ADDRESS_SIZE".to_string(), Value::Int(8));
        unsafe_static.insert("INVALID_FIELD_OFFSET".to_string(), Value::Int(-1));
        self.register_class(RuntimeClass {
                name: "jdk/internal/misc/Unsafe".to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods: unsafe_methods,
                static_fields: unsafe_static,
                instance_fields: vec![],
                interfaces: vec![],
            });

        // java/util/stream/IntStream — interface stub. The JDK version has
        // a <clinit> that pulls in ForkJoinPool, SharedSecrets, and the
        // full pipeline factory. We don't need any of that for the simple
        // terminal-op workloads that show up in practice, so the class is
        // registered as an interface marker and Arrays.stream hands back
        // our own `__jvm_rs/NativeIntStream` implementation.
        self.register_class(RuntimeClass {
                name: "java/util/stream/IntStream".to_string(),
                super_class: None,
                methods: HashMap::new(),
                static_fields: HashMap::new(),
                instance_fields: vec![],
                interfaces: vec![],
            });
        self.register_class(RuntimeClass {
                name: "java/util/stream/Stream".to_string(),
                super_class: None,
                methods: HashMap::new(),
                static_fields: HashMap::new(),
                instance_fields: vec![],
                interfaces: vec![],
            });

        // __jvm_rs/NativeIntStream — our non-lazy IntStream backing. Holds
        // an int[] and services terminal ops (sum, count, min, max) via
        // native methods. User code that declares the variable as
        // `IntStream` still routes here because has_native_override
        // matches on the interface's method names regardless of receiver
        // class.
        let mut native_int_stream_methods = HashMap::new();
        for (name, desc) in [
            ("sum", "()I"),
            ("count", "()J"),
            ("min", "()Ljava/util/OptionalInt;"),
            ("max", "()Ljava/util/OptionalInt;"),
            ("average", "()Ljava/util/OptionalDouble;"),
            ("toArray", "()[I"),
            ("asLongStream", "()Ljava/util/stream/LongStream;"),
            ("asDoubleStream", "()Ljava/util/stream/DoubleStream;"),
            ("collect", "(Ljava/util/stream/Collector;)Ljava/lang/Object;"),
        ] {
            native_int_stream_methods.insert(
                (name.to_string(), desc.to_string()),
                ClassMethod::Native,
            );
        }
        self.register_class(RuntimeClass {
                name: "__jvm_rs/NativeIntStream".to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods: native_int_stream_methods,
                static_fields: HashMap::new(),
                instance_fields: vec![
                    ("__array".to_string(), "[I".to_string()),
                ],
                interfaces: vec!["java/util/stream/IntStream".to_string()],
            });

        // __jvm_rs/NativeLongStream — non-lazy LongStream backing for Arrays.stream(long[])
        let mut native_long_stream_methods = HashMap::new();
        for (name, desc) in [
            ("sum", "()J"),
            ("count", "()J"),
            ("min", "()Ljava/util/OptionalLong;"),
            ("max", "()Ljava/util/OptionalLong;"),
            ("average", "()Ljava/util/OptionalDouble;"),
            ("toArray", "()[J"),
            ("asDoubleStream", "()Ljava/util/stream/DoubleStream;"),
            ("collect", "(Ljava/util/stream/Collector;)Ljava/lang/Object;"),
        ] {
            native_long_stream_methods.insert(
                (name.to_string(), desc.to_string()),
                ClassMethod::Native,
            );
        }
        self.register_class(RuntimeClass {
                name: "__jvm_rs/NativeLongStream".to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods: native_long_stream_methods,
                static_fields: HashMap::new(),
                instance_fields: vec![
                    ("__array".to_string(), "[J".to_string()),
                ],
                interfaces: vec!["java/util/stream/LongStream".to_string()],
            });

        // __jvm_rs/NativeDoubleStream — non-lazy DoubleStream backing for Arrays.stream(double[])
        let mut native_double_stream_methods = HashMap::new();
        for (name, desc) in [
            ("sum", "()D"),
            ("count", "()J"),
            ("min", "()Ljava/util/OptionalDouble;"),
            ("max", "()Ljava/util/OptionalDouble;"),
            ("average", "()D"),
            ("toArray", "()[D"),
            ("collect", "(Ljava/util/stream/Collector;)Ljava/lang/Object;"),
        ] {
            native_double_stream_methods.insert(
                (name.to_string(), desc.to_string()),
                ClassMethod::Native,
            );
        }
        self.register_class(RuntimeClass {
                name: "__jvm_rs/NativeDoubleStream".to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods: native_double_stream_methods,
                static_fields: HashMap::new(),
                instance_fields: vec![
                    ("__array".to_string(), "[D".to_string()),
                ],
                interfaces: vec!["java/util/stream/DoubleStream".to_string()],
            });

        // __jvm_rs/NativeCollector — our non-lazy Collector backing for stream.collect().
        let mut native_collector_methods = HashMap::new();
        native_collector_methods.insert(
            ("get".to_string(), "()Ljava/lang/Object;".to_string()),
            ClassMethod::Native,
        );
        native_collector_methods.insert(
            ("size".to_string(), "()I".to_string()),
            ClassMethod::Native,
        );
        self.register_class(RuntimeClass {
            name: "__jvm_rs/NativeCollector".to_string(),
            super_class: Some("java/lang/Object".to_string()),
            methods: native_collector_methods,
            static_fields: HashMap::new(),
            instance_fields: vec![
                ("__array".to_string(), "[Ljava/lang/Object;".to_string()),
                ("__mode".to_string(), "I".to_string()),
            ],
            interfaces: vec![],
        });

        // java/util/stream/Collectors — stub for JDK class loading
        self.register_class(RuntimeClass {
            name: "java/util/stream/Collectors".to_string(),
            super_class: Some("java/lang/Object".to_string()),
            methods: HashMap::new(),
            static_fields: HashMap::new(),
            instance_fields: vec![],
            interfaces: vec![],
        });

        // java/util/OptionalInt / OptionalLong / OptionalDouble — stubs for stream terminal ops
        self.register_class(RuntimeClass {
                name: "java/util/stream/LongStream".to_string(),
                super_class: None,
                methods: HashMap::new(),
                static_fields: HashMap::new(),
                instance_fields: vec![],
                interfaces: vec![],
            });
        self.register_class(RuntimeClass {
                name: "java/util/stream/DoubleStream".to_string(),
                super_class: None,
                methods: HashMap::new(),
                static_fields: HashMap::new(),
                instance_fields: vec![],
                interfaces: vec![],
            });

        // java/util/OptionalInt / OptionalLong / OptionalDouble — stubs for stream terminal ops
        for (name, fields) in [
            ("java/util/OptionalInt", vec![("value".to_string(), "I".to_string())]),
            ("java/util/OptionalLong", vec![("value".to_string(), "J".to_string())]),
            ("java/util/OptionalDouble", vec![("value".to_string(), "D".to_string())]),
        ] {
            let mut methods = HashMap::new();
            for (mname, mdesc) in [
                ("isPresent", "()Z"),
                ("getAsInt", "()I"),
                ("getAsLong", "()J"),
                ("getAsDouble", "()D"),
                ("orElse", "(I)I"),
                ("orElse", "(J)J"),
                ("orElse", "(D)D"),
            ] {
                methods.insert((mname.to_string(), mdesc.to_string()), ClassMethod::Native);
            }
            self.register_class(RuntimeClass {
                name: name.to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods,
                static_fields: HashMap::new(),
                instance_fields: fields,
                interfaces: vec![],
            });
        }

        // java/util/Optional — basic stub
        let mut optional_methods = HashMap::new();
        for (mname, mdesc) in [
            ("of", "(Ljava/lang/Object;)Ljava/util/Optional;"),
            ("isPresent", "()Z"),
            ("get", "()Ljava/lang/Object;"),
            ("orElse", "(Ljava/lang/Object;)Ljava/lang/Object;"),
            ("isEmpty", "()Z"),
            ("filter", "(Ljava/util/function/Predicate;)Ljava/util/Optional;"),
            ("map", "(Ljava/util/function/Function;)Ljava/util/Optional;"),
        ] {
            optional_methods.insert((mname.to_string(), mdesc.to_string()), ClassMethod::Native);
        }
        self.register_class(RuntimeClass {
                name: "java/util/Optional".to_string(),
                super_class: Some("java/lang/Object".to_string()),
                methods: optional_methods,
                static_fields: HashMap::new(),
                instance_fields: vec![("value".to_string(), "Ljava/lang/Object;".to_string())],
                interfaces: vec![],
            });

        // java/lang/Comparable — marker-only; real dispatch lands on each
        // implementing class's `compareTo` native via `resolve_method`.
        self.register_class(RuntimeClass {
                name: "java/lang/Comparable".to_string(),
                super_class: None,
                methods: HashMap::new(),
                static_fields: HashMap::new(),
                instance_fields: vec![],
                interfaces: vec![],
            });


        // java/lang/CharSequence — used by `String.contains` and similar.
        self.register_class(RuntimeClass {
                name: "java/lang/CharSequence".to_string(),
                super_class: None,
                methods: HashMap::new(),
                static_fields: HashMap::new(),
                instance_fields: vec![],
                interfaces: vec![],
            });

        // Mark built-in wrapper types as implementing Comparable so `checkcast`
        // and interface dispatch both succeed.
        for boxed in [
            "java/lang/Integer",
            "java/lang/Long",
            "java/lang/Boolean",
        ] {
            if let Some(class) = self
                .runtime
                .lock()
                .unwrap()
                .classes
                .get_mut(boxed)
            {
                class.interfaces.push("java/lang/Comparable".to_string());
            }
        }
        if let Some(class) = self
            .runtime
            .lock()
            .unwrap()
            .classes
            .get_mut("java/lang/String")
        {
            class.interfaces.push("java/lang/Comparable".to_string());
            class.interfaces.push("java/lang/CharSequence".to_string());
        }

        // java/lang/Thread
        let mut thread_methods = HashMap::new();
        for (name, desc) in [
            ("<init>", "()V"),
            ("<init>", "(Ljava/lang/Runnable;)V"),
            ("start", "()V"),
            ("run", "()V"),
            ("join", "()V"),
            ("currentThread", "()Ljava/lang/Thread;"),
            ("getThreadGroup", "()Ljava/lang/ThreadGroup;"),
            ("getContextClassLoader", "()Ljava/lang/ClassLoader;"),
            ("setContextClassLoader", "(Ljava/lang/ClassLoader;)V"),
            ("getName", "()Ljava/lang/String;"),
            ("isAlive", "()Z"),
            ("isInterrupted", "()Z"),
            ("interrupt", "()V"),
            ("getId", "()J"),
            ("getPriority", "()I"),
            ("setPriority", "(I)V"),
            ("setDaemon", "(Z)V"),
            ("isDaemon", "()Z"),
            ("sleep", "(J)V"),
            ("yield", "()V"),
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
                static_fields: HashMap::new(),
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
            let mut methods = HashMap::new();
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
                    static_fields: HashMap::new(),
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
            ("java/io/PrintStream", "println", "(Ljava/lang/Object;)V") => {
                let reference = args[1].as_reference()?;
                let line = if reference == Reference::Null {
                    "null".to_string()
                } else {
                    self.stringify_heap(reference)?
                };
                println!("{line}");
                self.output.lock().unwrap().push(line);
                Ok(None)
            }
            ("java/io/PrintStream", "print", "(Ljava/lang/Object;)V") => {
                let reference = args[1].as_reference()?;
                let text = if reference == Reference::Null {
                    "null".to_string()
                } else {
                    self.stringify_heap(reference)?
                };
                print!("{text}");
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
            ("java/lang/Object", "hashCode", "()I") => {
                let r = args[0].as_reference()?;
                Ok(Some(Value::Int(match r {
                    Reference::Null => 0,
                    Reference::Heap(i) => i as i32,
                })))
            }
            ("java/lang/Object", "equals", "(Ljava/lang/Object;)Z") => {
                Ok(Some(Value::Int(i32::from(
                    args[0].as_reference()? == args[1].as_reference()?,
                ))))
            }
            ("java/lang/Object", "toString", "()Ljava/lang/String;") => {
                let r = args[0].as_reference()?;
                let (cls, id) = match r {
                    Reference::Null => ("null".to_string(), 0usize),
                    Reference::Heap(i) => {
                        let name = match self.heap.lock().unwrap().get(r)? {
                            HeapValue::Object { class_name, .. } => class_name.clone(),
                            v => v.kind_name().to_string(),
                        };
                        (name, i)
                    }
                };
                Ok(Some(self.new_string(format!("{}@{:x}", cls.replace('/', "."), id))))
            }
            ("java/lang/Object", "getClass", "()Ljava/lang/Class;") => {
                let r = args[0].as_reference()?;
                let class_name = match r {
                    Reference::Null => return Err(VmError::NullReference),
                    Reference::Heap(_) => match self.heap.lock().unwrap().get(r)? {
                        HeapValue::Object { class_name, .. } => class_name.clone(),
                        HeapValue::String(_) => "java/lang/String".to_string(),
                        HeapValue::StringBuilder(_) => "java/lang/StringBuilder".to_string(),
                        HeapValue::IntArray { .. } => "[I".to_string(),
                        HeapValue::LongArray { .. } => "[J".to_string(),
                        HeapValue::FloatArray { .. } => "[F".to_string(),
                        HeapValue::DoubleArray { .. } => "[D".to_string(),
                        HeapValue::ReferenceArray { component_type, .. } => {
                            format!("[{component_type}")
                        }
                    },
                };
                Ok(Some(Value::Reference(self.class_object(&class_name))))
            }
            ("java/lang/String", "length", "()I") => {
                let s = self.stringify_reference(args[0].as_reference()?)?;
                Ok(Some(Value::Int(s.len() as i32)))
            }
            ("java/lang/String", "charAt", "(I)C") => {
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
            ("java/lang/String", "compareTo", "(Ljava/lang/Object;)I") => {
                let a_ref = args[0].as_reference()?;
                let b_ref = args[1].as_reference()?;
                let a = self.stringify_reference(a_ref)?;
                let b = self.stringify_reference(b_ref)?;
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
            // Note: valueOf(Ljava/lang/Object;) is NOT handled here - it's a real Java method

            // --- Integer methods ---
            ("java/lang/Integer", "numberOfLeadingZeros", "(I)I") => {
                Ok(Some(Value::Int(args[0].as_int()?.leading_zeros() as i32)))
            }
            ("java/lang/Integer", "numberOfTrailingZeros", "(I)I") => {
                Ok(Some(Value::Int(args[0].as_int()?.trailing_zeros() as i32)))
            }
            ("java/lang/Integer", "bitCount", "(I)I") => {
                Ok(Some(Value::Int(args[0].as_int()?.count_ones() as i32)))
            }
            ("java/lang/Integer", "reverse", "(I)I") => {
                Ok(Some(Value::Int(args[0].as_int()?.reverse_bits())))
            }
            ("java/lang/Integer", "reverseBytes", "(I)I") => {
                Ok(Some(Value::Int(args[0].as_int()?.swap_bytes())))
            }
            ("java/lang/Integer", "highestOneBit", "(I)I") => {
                let v = args[0].as_int()? as u32;
                Ok(Some(Value::Int(if v == 0 {
                    0
                } else {
                    (1u32 << (31 - v.leading_zeros())) as i32
                })))
            }
            ("java/lang/Integer", "lowestOneBit", "(I)I") => {
                let v = args[0].as_int()?;
                Ok(Some(Value::Int(v & v.wrapping_neg())))
            }
            ("java/lang/Integer", "signum", "(I)I") => {
                Ok(Some(Value::Int(args[0].as_int()?.signum())))
            }
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
                let mut fields = HashMap::new();
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
            ("java/lang/Integer", "compareTo", "(Ljava/lang/Integer;)I")
            | ("java/lang/Integer", "compareTo", "(Ljava/lang/Object;)I") => {
                let a = self.integer_value(args[0].as_reference()?)?;
                let b = self.integer_value(args[1].as_reference()?)?;
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
                let mut fields = HashMap::new();
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
            ("java/lang/Boolean", "getBoolean", "(Ljava/lang/String;)Z") => {
                // Query system property — we don't support -D properties yet, so
                // any key is treated as absent which equals false.
                Ok(Some(Value::Int(0)))
            }
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
                let mut fields = HashMap::new();
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
            ("java/util/Objects", "hash", "([Ljava/lang/Object;)I") => {
                let arr_ref = args[0].as_reference()?;
                let hash = self.hash_array_elements(arr_ref)?;
                Ok(Some(Value::Int(hash)))
            }
            ("java/util/Objects", "hashCode", "(Ljava/lang/Object;)I") => {
                let r = args[0].as_reference()?;
                if r == Reference::Null {
                    Ok(Some(Value::Int(0)))
                } else {
                    Ok(Some(Value::Int(self.hash_object(r))))
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
            ("java/lang/Math", "floorDiv", "(II)I") => {
                let (x, y) = (args[0].as_int()?, args[1].as_int()?);
                if y == 0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Some(Value::Int(x.div_euclid(y).wrapping_add(
                    if (x % y != 0) && ((x ^ y) < 0) { -1 + 1 } else { 0 },
                ))))
            }
            ("java/lang/Math", "floorDiv", "(JJ)J") => {
                let (x, y) = (args[0].as_long()?, args[1].as_long()?);
                if y == 0 {
                    return Err(VmError::DivisionByZero);
                }
                let q = x / y;
                let q = if (x % y != 0) && ((x ^ y) < 0) { q - 1 } else { q };
                Ok(Some(Value::Long(q)))
            }
            ("java/lang/Math", "floorMod", "(II)I") => {
                let (x, y) = (args[0].as_int()?, args[1].as_int()?);
                if y == 0 {
                    return Err(VmError::DivisionByZero);
                }
                let r = x % y;
                Ok(Some(Value::Int(if (r != 0) && ((r ^ y) < 0) { r + y } else { r })))
            }
            ("java/lang/Math", "floorMod", "(JJ)J") => {
                let (x, y) = (args[0].as_long()?, args[1].as_long()?);
                if y == 0 {
                    return Err(VmError::DivisionByZero);
                }
                let r = x % y;
                Ok(Some(Value::Long(if (r != 0) && ((r ^ y) < 0) { r + y } else { r })))
            }
            ("java/lang/Math", "addExact", "(II)I") => {
                let (a, b) = (args[0].as_int()?, args[1].as_int()?);
                a.checked_add(b)
                    .map(|v| Some(Value::Int(v)))
                    .ok_or(VmError::DivisionByZero)
            }
            ("java/lang/Math", "addExact", "(JJ)J") => {
                let (a, b) = (args[0].as_long()?, args[1].as_long()?);
                a.checked_add(b)
                    .map(|v| Some(Value::Long(v)))
                    .ok_or(VmError::DivisionByZero)
            }
            ("java/lang/Math", "subtractExact", "(II)I") => {
                let (a, b) = (args[0].as_int()?, args[1].as_int()?);
                a.checked_sub(b)
                    .map(|v| Some(Value::Int(v)))
                    .ok_or(VmError::DivisionByZero)
            }
            ("java/lang/Math", "multiplyExact", "(II)I") => {
                let (a, b) = (args[0].as_int()?, args[1].as_int()?);
                a.checked_mul(b)
                    .map(|v| Some(Value::Int(v)))
                    .ok_or(VmError::DivisionByZero)
            }
            ("java/lang/Math", "multiplyExact", "(JJ)J") => {
                let (a, b) = (args[0].as_long()?, args[1].as_long()?);
                a.checked_mul(b)
                    .map(|v| Some(Value::Long(v)))
                    .ok_or(VmError::DivisionByZero)
            }
            ("java/lang/Math", "signum", "(I)I") => {
                Ok(Some(Value::Int(args[0].as_int()?.signum())))
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
            ("java/lang/Thread", "currentThread", "()Ljava/lang/Thread;") => {
                // Return a singleton "current thread" stub that carries no real
                // thread identity — enough for JDK bytecode that only inspects
                // it to avoid NPEs. Cached via the class_objects map keyed by a
                // reserved sentinel so GC keeps it alive.
                const KEY: &str = "__current_thread";
                if let Some(r) = self
                    .runtime
                    .lock()
                    .unwrap()
                    .class_objects
                    .get(KEY)
                    .copied()
                {
                    return Ok(Some(Value::Reference(r)));
                }
                let reference = self.heap.lock().unwrap().allocate(HeapValue::Object {
                    class_name: "java/lang/Thread".to_string(),
                    fields: HashMap::new(),
                });
                self.runtime
                    .lock()
                    .unwrap()
                    .class_objects
                    .insert(KEY.to_string(), reference);
                Ok(Some(Value::Reference(reference)))
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

            // --- java/lang/Class natives ---
            // Not loading the real JDK's `java/lang/Class` keeps startup
            // cheap; these cover the methods user code and the stdlib's
            // bootstrap path actually exercise.
            ("java/lang/Class", "desiredAssertionStatus", "()Z") => Ok(Some(Value::Int(0))),
            ("java/lang/Class", "isArray", "()Z") => {
                let name = self.class_internal_name(args[0].as_reference()?)?;
                Ok(Some(Value::Int(i32::from(name.starts_with('[')))))
            }
            ("java/lang/Class", "isPrimitive", "()Z") => {
                let name = self.class_internal_name(args[0].as_reference()?)?;
                let primitive = matches!(
                    name.as_str(),
                    "boolean" | "byte" | "char" | "short" | "int" | "long" | "float" | "double" | "void"
                );
                Ok(Some(Value::Int(i32::from(primitive))))
            }
            ("java/lang/Class", "isInterface", "()Z") => Ok(Some(Value::Int(0))),
            ("java/lang/Class", "getName", "()Ljava/lang/String;")
            | ("java/lang/Class", "toString", "()Ljava/lang/String;") => {
                let internal = self.class_internal_name(args[0].as_reference()?)?;
                let dotted = internal.replace('/', ".");
                Ok(Some(self.new_string(dotted)))
            }
            ("java/lang/Class", "getSimpleName", "()Ljava/lang/String;") => {
                let internal = self.class_internal_name(args[0].as_reference()?)?;
                let simple = internal
                    .rsplit_once('/')
                    .map(|(_, s)| s)
                    .unwrap_or(internal.as_str())
                    .rsplit_once('$')
                    .map(|(_, s)| s.to_string())
                    .unwrap_or_else(|| {
                        internal
                            .rsplit_once('/')
                            .map(|(_, s)| s.to_string())
                            .unwrap_or(internal.clone())
                    });
                Ok(Some(self.new_string(simple)))
            }

            // --- jdk/internal/reflect stubs ---
            ("jdk/internal/reflect/Reflection", "getCallerClass", "()Ljava/lang/Class;") => {
                Ok(Some(Value::Reference(Reference::Null)))
            }
            ("jdk/internal/reflect/Reflection", _, _) => Ok(stub_return_value(descriptor)),

            // --- java/lang/Runtime natives ---
            ("java/lang/Runtime", "availableProcessors", "()I") => {
                let n = std::thread::available_parallelism()
                    .map(|n| n.get() as i32)
                    .unwrap_or(1);
                Ok(Some(Value::Int(n)))
            }
            ("java/lang/Runtime", "freeMemory", "()J")
            | ("java/lang/Runtime", "totalMemory", "()J")
            | ("java/lang/Runtime", "maxMemory", "()J") => {
                // Not meaningful for our heap model; report a plausible constant
                // so JDK code that compares these values can proceed.
                Ok(Some(Value::Long(256 * 1024 * 1024)))
            }
            ("java/lang/Runtime", "gc", "()V") => {
                self.request_gc();
                Ok(None)
            }

            // --- jdk/internal/misc/Unsafe natives ---
            // The stdlib only uses Unsafe for concurrency primitives we
            // don't need under our single-threaded interpreter; every
            // meaningful call here either returns a plausible constant or
            // falls back to descriptor-typed zero.
            ("jdk/internal/misc/Unsafe", "registerNatives", "()V") => Ok(None),
            ("jdk/internal/misc/Unsafe", "getUnsafe", "()Ljdk/internal/misc/Unsafe;") => {
                // Return the singleton we allocated during bootstrap. JDK
                // bytecode (e.g., ArraysSupport.<clinit>) caches the result
                // in a static and then dispatches through it, so the ref
                // must be non-null to avoid downstream NPEs.
                Ok(Some(
                    self.get_static_field("jdk/internal/misc/Unsafe", "theUnsafe")?,
                ))
            }
            ("jdk/internal/misc/Unsafe", "arrayBaseOffset", "(Ljava/lang/Class;)I") => {
                Ok(Some(Value::Int(0)))
            }
            ("jdk/internal/misc/Unsafe", "arrayIndexScale", "(Ljava/lang/Class;)I") => {
                // Scale of 1 means index == offset for our synthetic array model.
                Ok(Some(Value::Int(1)))
            }
            ("jdk/internal/misc/Unsafe", "addressSize", "()I") => Ok(Some(Value::Int(8))),
            ("jdk/internal/misc/Unsafe", "isBigEndian", "()Z") => Ok(Some(Value::Int(
                i32::from(cfg!(target_endian = "big")),
            ))),
            ("jdk/internal/misc/Unsafe", "pageSize", "()I") => Ok(Some(Value::Int(4096))),
            ("jdk/internal/misc/Unsafe", "objectFieldOffset", _)
            | ("jdk/internal/misc/Unsafe", "staticFieldOffset", _) => Ok(Some(Value::Long(0))),
            ("jdk/internal/misc/Unsafe", "staticFieldBase", _) => {
                Ok(Some(Value::Reference(Reference::Null)))
            }
            ("jdk/internal/misc/Unsafe", "storeFence", "()V")
            | ("jdk/internal/misc/Unsafe", "loadFence", "()V")
            | ("jdk/internal/misc/Unsafe", "fullFence", "()V") => Ok(None),
            // CAS primitives: our interpreter is single-threaded, so a
            // read-compare-set is atomic by construction. The Object arg is
            // a direct heap reference; since we can't address fields by
            // offset reliably, approximate by succeeding unconditionally
            // when expected == current (trivial for null→value on freshly
            // allocated objects, which is the `VarHandle.compareAndSet`
            // pattern the stdlib uses during lazy-init). Callers that
            // re-check via volatile reads still see a consistent state.
            (
                "jdk/internal/misc/Unsafe",
                "compareAndSetInt" | "compareAndSetLong"
                | "compareAndSetReference" | "compareAndSetObject",
                _,
            ) => Ok(Some(Value::Int(1))),
            ("jdk/internal/misc/Unsafe", "getReferenceVolatile", _) => {
                Ok(Some(Value::Reference(Reference::Null)))
            }
            ("jdk/internal/misc/Unsafe", "putReferenceVolatile", _)
            | ("jdk/internal/misc/Unsafe", "putIntVolatile", _) => Ok(None),
            ("jdk/internal/misc/Unsafe", "getIntVolatile", _) => Ok(Some(Value::Int(0))),
            ("jdk/internal/misc/Unsafe", _, _) => Ok(stub_return_value(descriptor)),

            // --- java/util/Arrays.equals — element-wise, avoiding the
            //     Unsafe.getInt/getLong vectorized-mismatch path that our
            //     offset-based stubs can't service correctly.
            ("java/util/Arrays", "equals", "([I[I)Z") => {
                Ok(Some(Value::Int(i32::from(
                    self.native_arrays_equals_int(args[0].as_reference()?, args[1].as_reference()?)?,
                ))))
            }
            ("java/util/Arrays", "equals", "([J[J)Z") => {
                Ok(Some(Value::Int(i32::from(
                    self.native_arrays_equals_long(args[0].as_reference()?, args[1].as_reference()?)?,
                ))))
            }
            ("java/util/Arrays", "equals", "([B[B)Z")
            | ("java/util/Arrays", "equals", "([S[S)Z")
            | ("java/util/Arrays", "equals", "([C[C)Z")
            | ("java/util/Arrays", "equals", "([Z[Z)Z") => {
                // Booleans/bytes/shorts/chars all land in HeapValue::IntArray.
                Ok(Some(Value::Int(i32::from(
                    self.native_arrays_equals_int(args[0].as_reference()?, args[1].as_reference()?)?,
                ))))
            }
            ("java/util/Arrays", "equals", "([F[F)Z") => {
                Ok(Some(Value::Int(i32::from(
                    self.native_arrays_equals_float(args[0].as_reference()?, args[1].as_reference()?)?,
                ))))
            }
            ("java/util/Arrays", "equals", "([D[D)Z") => {
                Ok(Some(Value::Int(i32::from(
                    self.native_arrays_equals_double(args[0].as_reference()?, args[1].as_reference()?)?,
                ))))
            }
            (
                "java/util/Arrays",
                "equals",
                "([Ljava/lang/Object;[Ljava/lang/Object;)Z",
            ) => {
                Ok(Some(Value::Int(i32::from(
                    self.native_arrays_equals_ref(args[0].as_reference()?, args[1].as_reference()?)?,
                ))))
            }

            // --- java/util/Arrays.stream → NativeIntStream ---
            ("java/util/Arrays", "stream", "([I)Ljava/util/stream/IntStream;") => {
                let array_ref = args[0].as_reference()?;
                let mut fields = HashMap::new();
                fields.insert("__array".to_string(), Value::Reference(array_ref));
                let r = self.heap.lock().unwrap().allocate(HeapValue::Object {
                    class_name: "__jvm_rs/NativeIntStream".to_string(),
                    fields,
                });
                Ok(Some(Value::Reference(r)))
            }
            ("java/util/Arrays", "stream", "([J)Ljava/util/stream/LongStream;") => {
                let array_ref = args[0].as_reference()?;
                let mut fields = HashMap::new();
                fields.insert("__array".to_string(), Value::Reference(array_ref));
                let r = self.heap.lock().unwrap().allocate(HeapValue::Object {
                    class_name: "__jvm_rs/NativeLongStream".to_string(),
                    fields,
                });
                Ok(Some(Value::Reference(r)))
            }
            ("java/util/Arrays", "stream", "([D)Ljava/util/stream/DoubleStream;") => {
                let array_ref = args[0].as_reference()?;
                let mut fields = HashMap::new();
                fields.insert("__array".to_string(), Value::Reference(array_ref));
                let r = self.heap.lock().unwrap().allocate(HeapValue::Object {
                    class_name: "__jvm_rs/NativeDoubleStream".to_string(),
                    fields,
                });
                Ok(Some(Value::Reference(r)))
            }

            // --- __jvm_rs/NativeLongStream terminal ops ---
            ("__jvm_rs/NativeLongStream", "sum", "()J") => {
                let array = self.native_long_stream_array(args[0].as_reference()?)?;
                let mut heap = self.heap.lock().unwrap();
                if let HeapValue::LongArray { values } = heap.get(array)? {
                    Ok(Some(Value::Long(values.iter().sum())))
                } else {
                    Ok(Some(Value::Long(0)))
                }
            }
            ("__jvm_rs/NativeLongStream", "count", "()J") => {
                let array = self.native_long_stream_array(args[0].as_reference()?)?;
                let mut heap = self.heap.lock().unwrap();
                if let HeapValue::LongArray { values } = heap.get(array)? {
                    Ok(Some(Value::Long(values.len() as i64)))
                } else {
                    Ok(Some(Value::Long(0)))
                }
            }
            ("__jvm_rs/NativeLongStream", "toArray", "()[J") => {
                let array = self.native_long_stream_array(args[0].as_reference()?)?;
                Ok(Some(Value::Reference(array)))
            }

            // --- __jvm_rs/NativeDoubleStream terminal ops ---
            ("__jvm_rs/NativeDoubleStream", "sum", "()D") => {
                let array = self.native_double_stream_array(args[0].as_reference()?)?;
                let mut heap = self.heap.lock().unwrap();
                if let HeapValue::DoubleArray { values } = heap.get(array)? {
                    Ok(Some(Value::Double(values.iter().sum::<f64>())))
                } else {
                    Ok(Some(Value::Double(0.0)))
                }
            }
            ("__jvm_rs/NativeDoubleStream", "count", "()J") => {
                let array = self.native_double_stream_array(args[0].as_reference()?)?;
                let mut heap = self.heap.lock().unwrap();
                if let HeapValue::DoubleArray { values } = heap.get(array)? {
                    Ok(Some(Value::Long(values.len() as i64)))
                } else {
                    Ok(Some(Value::Long(0)))
                }
            }
            ("__jvm_rs/NativeDoubleStream", "average", "()D") => {
                let array = self.native_double_stream_array(args[0].as_reference()?)?;
                let mut heap = self.heap.lock().unwrap();
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
                let array = self.native_double_stream_array(args[0].as_reference()?)?;
                Ok(Some(Value::Reference(array)))
            }
            ("__jvm_rs/NativeIntStream", "collect", "(Ljava/util/stream/Collector;)Ljava/lang/Object;") => {
                self.native_int_stream_collect(args[0].as_reference()?, args[1].as_reference()?)
            }
            ("__jvm_rs/NativeLongStream", "collect", "(Ljava/util/stream/Collector;)Ljava/lang/Object;") => {
                self.native_long_stream_collect(args[0].as_reference()?, args[1].as_reference()?)
            }
            ("__jvm_rs/NativeDoubleStream", "collect", "(Ljava/util/stream/Collector;)Ljava/lang/Object;") => {
                self.native_double_stream_collect(args[0].as_reference()?, args[1].as_reference()?)
            }

            // --- java/util/stream/Collectors natives ---
            ("java/util/stream/Collectors", "toList", "()Ljava/util/stream/Collector;") => {
                self.native_collectors_to_list()
            }
            ("java/util/stream/Collectors", "toSet", "()Ljava/util/stream/Collector;") => {
                self.native_collectors_to_set()
            }
            ("java/util/stream/Collectors", "counting", "()Ljava/util/function/Supplier;") => {
                self.native_collectors_counting()
            }
            ("java/util/stream/Collectors", "joining", "()Ljava/util/stream/Collector;") => {
                self.native_collectors_joining(None)
            }
            ("java/util/stream/Collectors", "joining", "(Ljava/lang/CharSequence;)Ljava/util/stream/Collector;") => {
                self.native_collectors_joining(Some(args[0].as_reference()?))
            }
            ("java/util/stream/Collectors", "reducing", "(Ljava/lang/Object;Ljava/util/function/BinaryOperator;)Ljava/util/stream/Collector;") => {
                self.native_collectors_reducing(args[0].as_reference()?, args[1].as_reference()?)
            }
            ("java/util/stream/Collectors", "toMap", "(Ljava/util/function/Function;Ljava/util/function/Function;)Ljava/util/stream/Collector;") => {
                self.native_collectors_to_map(args[0].as_reference()?, args[1].as_reference()?)
            }

            // --- __jvm_rs/NativeIntStream terminal ops ---
            ("__jvm_rs/NativeIntStream", "sum", "()I") => {
                let array = self.native_int_stream_array(args[0].as_reference()?)?;
                let mut heap = self.heap.lock().unwrap();
                if let HeapValue::IntArray { values } = heap.get(array)? {
                    Ok(Some(Value::Int(values.iter().map(|v| *v as i64).sum::<i64>() as i32)))
                } else {
                    Ok(Some(Value::Int(0)))
                }
            }
            ("__jvm_rs/NativeIntStream", "count", "()J") => {
                let array = self.native_int_stream_array(args[0].as_reference()?)?;
                let mut heap = self.heap.lock().unwrap();
                if let HeapValue::IntArray { values } = heap.get(array)? {
                    Ok(Some(Value::Long(values.len() as i64)))
                } else {
                    Ok(Some(Value::Long(0)))
                }
            }
            ("__jvm_rs/NativeIntStream", "min", "()Ljava/util/OptionalInt;") => {
                let array = self.native_int_stream_array(args[0].as_reference()?)?;
                let mut heap = self.heap.lock().unwrap();
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
                let array = self.native_int_stream_array(args[0].as_reference()?)?;
                let mut heap = self.heap.lock().unwrap();
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
                let array = self.native_int_stream_array(args[0].as_reference()?)?;
                let mut heap = self.heap.lock().unwrap();
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
                let array = self.native_int_stream_array(args[0].as_reference()?)?;
                Ok(Some(Value::Reference(array)))
            }

            // --- __jvm_rs/NativeLongStream terminal ops ---
            ("__jvm_rs/NativeLongStream", "min", "()Ljava/util/OptionalLong;") => {
                let array = self.native_long_stream_array(args[0].as_reference()?)?;
                let mut heap = self.heap.lock().unwrap();
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
                let array = self.native_long_stream_array(args[0].as_reference()?)?;
                let mut heap = self.heap.lock().unwrap();
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
                let array = self.native_long_stream_array(args[0].as_reference()?)?;
                let mut heap = self.heap.lock().unwrap();
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

            // --- __jvm_rs/NativeDoubleStream terminal ops ---
            ("__jvm_rs/NativeDoubleStream", "min", "()Ljava/util/OptionalDouble;") => {
                let array = self.native_double_stream_array(args[0].as_reference()?)?;
                let mut heap = self.heap.lock().unwrap();
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
                let array = self.native_double_stream_array(args[0].as_reference()?)?;
                let mut heap = self.heap.lock().unwrap();
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

            // --- java/util/Collections natives ---
            // Implemented in Rust to avoid pulling in the JDK's bytecode
            // path, which drags in Reference handler threads, security,
            // and reflection machinery during its <clinit>.
            ("java/util/Collections", "sort", "(Ljava/util/List;)V") => {
                self.native_collections_sort(args[0].as_reference()?, None)?;
                Ok(None)
            }
            (
                "java/util/Collections",
                "sort",
                "(Ljava/util/List;Ljava/util/Comparator;)V",
            ) => {
                let list = args[0].as_reference()?;
                let cmp = args[1].as_reference()?;
                let cmp_opt = if cmp == Reference::Null { None } else { Some(cmp) };
                self.native_collections_sort(list, cmp_opt)?;
                Ok(None)
            }
            ("java/util/Collections", "reverse", "(Ljava/util/List;)V") => {
                self.native_collections_reverse(args[0].as_reference()?)?;
                Ok(None)
            }

            // --- java/lang/Thread generic fallback ---
            // Our thread model has no ThreadGroup / ContextClassLoader /
            // Priority etc.; stub anything not explicitly implemented so JDK
            // bytecode that only inspects the result can proceed.
            ("java/lang/Thread", _, _) => Ok(stub_return_value(descriptor)),
            ("java/lang/ThreadGroup", _, _) => Ok(stub_return_value(descriptor)),

            // --- java/util/Optional / OptionalInt / OptionalLong / OptionalDouble natives ---
            ("java/util/Optional", "of", "(Ljava/lang/Object;)Ljava/util/Optional;") => {
                let value_ref = args[0].as_reference()?;
                let mut fields = HashMap::new();
                fields.insert("value".to_string(), Value::Reference(value_ref));
                let r = self.heap.lock().unwrap().allocate(HeapValue::Object {
                    class_name: "java/util/Optional".to_string(),
                    fields,
                });
                Ok(Some(Value::Reference(r)))
            }
            ("java/util/Optional", "isPresent", "()Z")
            | ("java/util/Optional", "isEmpty", "()Z") => {
                let opt_ref = args[0].as_reference()?;
                let is_empty = match self.heap.lock().unwrap().get(opt_ref)? {
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
                    "()Z" => !is_empty,
                    "()Z" => is_empty,
                    _ => false,
                };
                Ok(Some(Value::Int(if result { 1 } else { 0 })))
            }
            ("java/util/Optional", "get", "()Ljava/lang/Object;") => {
                let opt_ref = args[0].as_reference()?;
                match self.heap.lock().unwrap().get(opt_ref)? {
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
                let value = match self.heap.lock().unwrap().get(opt_ref)? {
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
                match self.heap.lock().unwrap().get(opt_ref)? {
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
                match self.heap.lock().unwrap().get(opt_ref)? {
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
                match self.heap.lock().unwrap().get(opt_ref)? {
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
                match self.heap.lock().unwrap().get(opt_ref)? {
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
                match self.heap.lock().unwrap().get(opt_ref)? {
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
                match self.heap.lock().unwrap().get(opt_ref)? {
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
                match self.heap.lock().unwrap().get(opt_ref)? {
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
                match self.heap.lock().unwrap().get(opt_ref)? {
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
                match self.heap.lock().unwrap().get(opt_ref)? {
                    HeapValue::Object { fields, .. } => {
                        match fields.get("value") {
                            Some(Value::Double(v)) => Ok(Some(Value::Double(*v))),
                            _ => Ok(Some(Value::Double(fallback))),
                        }
                    }
                    _ => Ok(Some(Value::Double(fallback))),
                }
            }

            // --- JDK Internal Stubs ---
            // CDS.isDumpingClassList0 — returns false (not dumping)
            ("jdk/internal/misc/CDS", "isDumpingClassList0", "()Z") => Ok(Some(Value::Int(0))),
            // CDS.isDumpingArchive0 — returns false
            ("jdk/internal/misc/CDS", "isDumpingArchive0", "()Z") => Ok(Some(Value::Int(0))),
            // CDS.isSharingEnabled0 — returns false
            ("jdk/internal/misc/CDS", "isSharingEnabled0", "()Z") => Ok(Some(Value::Int(0))),
            // CDS generic — CDS is disabled; stub every method to a
            // descriptor-correct zero value (void → None, primitives → 0,
            // references → null). Prior wildcard always pushed a null ref,
            // which silently corrupted the operand stack for void methods
            // like initializeFromArchive(Ljava/lang/Class;)V.
            ("jdk/internal/misc/CDS", _, _) => Ok(stub_return_value(descriptor)),

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
                self.stringify_heap(r)
            }
            _ => Ok("?".to_string()),
        }
    }

    fn native_arrays_equals_int(&self, a: Reference, b: Reference) -> Result<bool, VmError> {
        if a == b {
            return Ok(true);
        }
        if a == Reference::Null || b == Reference::Null {
            return Ok(false);
        }
        let mut heap = self.heap.lock().unwrap();
        match (heap.get(a)?, heap.get(b)?) {
            (HeapValue::IntArray { values: x }, HeapValue::IntArray { values: y }) => Ok(x == y),
            _ => Ok(false),
        }
    }

    fn native_arrays_equals_long(&self, a: Reference, b: Reference) -> Result<bool, VmError> {
        if a == b {
            return Ok(true);
        }
        if a == Reference::Null || b == Reference::Null {
            return Ok(false);
        }
        let mut heap = self.heap.lock().unwrap();
        match (heap.get(a)?, heap.get(b)?) {
            (HeapValue::LongArray { values: x }, HeapValue::LongArray { values: y }) => {
                Ok(x == y)
            }
            _ => Ok(false),
        }
    }

    fn native_arrays_equals_float(&self, a: Reference, b: Reference) -> Result<bool, VmError> {
        if a == b {
            return Ok(true);
        }
        if a == Reference::Null || b == Reference::Null {
            return Ok(false);
        }
        let mut heap = self.heap.lock().unwrap();
        match (heap.get(a)?, heap.get(b)?) {
            (HeapValue::FloatArray { values: x }, HeapValue::FloatArray { values: y }) => {
                // Per Float.equals: NaN == NaN when their raw int bits match.
                Ok(x.len() == y.len()
                    && x.iter().zip(y.iter()).all(|(a, b)| a.to_bits() == b.to_bits()))
            }
            _ => Ok(false),
        }
    }

    fn native_arrays_equals_double(&self, a: Reference, b: Reference) -> Result<bool, VmError> {
        if a == b {
            return Ok(true);
        }
        if a == Reference::Null || b == Reference::Null {
            return Ok(false);
        }
        let mut heap = self.heap.lock().unwrap();
        match (heap.get(a)?, heap.get(b)?) {
            (HeapValue::DoubleArray { values: x }, HeapValue::DoubleArray { values: y }) => {
                Ok(x.len() == y.len()
                    && x.iter().zip(y.iter()).all(|(a, b)| a.to_bits() == b.to_bits()))
            }
            _ => Ok(false),
        }
    }

    fn native_arrays_equals_ref(&mut self, a: Reference, b: Reference) -> Result<bool, VmError> {
        if a == b {
            return Ok(true);
        }
        if a == Reference::Null || b == Reference::Null {
            return Ok(false);
        }
        let (xs, ys): (Vec<Reference>, Vec<Reference>) = {
            let mut heap = self.heap.lock().unwrap();
            match (heap.get(a)?, heap.get(b)?) {
                (
                    HeapValue::ReferenceArray { values: x, .. },
                    HeapValue::ReferenceArray { values: y, .. },
                ) => (x.clone(), y.clone()),
                _ => return Ok(false),
            }
        };
        if xs.len() != ys.len() {
            return Ok(false);
        }
        for (x, y) in xs.iter().zip(ys.iter()) {
            if x == y {
                continue;
            }
            if *x == Reference::Null || *y == Reference::Null {
                return Ok(false);
            }
            // Delegate to Object.equals for non-identity checks.
            let res = self.call_virtual(
                *x,
                "equals",
                "(Ljava/lang/Object;)Z",
                vec![Value::Reference(*y)],
            )?;
            match res {
                crate::vm::types::ExecutionResult::Value(Value::Int(0)) => return Ok(false),
                crate::vm::types::ExecutionResult::Value(Value::Int(_)) => {}
                _ => return Ok(false),
            }
        }
        Ok(true)
    }

    /// Read the `__array` field of a NativeIntStream.
    fn native_int_stream_array(&self, stream_ref: Reference) -> Result<Reference, VmError> {
        match self.heap.lock().unwrap().get(stream_ref)? {
            HeapValue::Object { fields, .. } => match fields.get("__array") {
                Some(Value::Reference(r)) => Ok(*r),
                _ => Err(VmError::NullReference),
            },
            value => Err(VmError::InvalidHeapValue {
                expected: "object",
                actual: value.kind_name(),
            }),
        }
    }

    fn native_long_stream_array(&self, stream_ref: Reference) -> Result<Reference, VmError> {
        match self.heap.lock().unwrap().get(stream_ref)? {
            HeapValue::Object { fields, .. } => match fields.get("__array") {
                Some(Value::Reference(r)) => Ok(*r),
                _ => Err(VmError::NullReference),
            },
            value => Err(VmError::InvalidHeapValue {
                expected: "object",
                actual: value.kind_name(),
            }),
        }
    }

    fn native_double_stream_array(&self, stream_ref: Reference) -> Result<Reference, VmError> {
        match self.heap.lock().unwrap().get(stream_ref)? {
            HeapValue::Object { fields, .. } => match fields.get("__array") {
                Some(Value::Reference(r)) => Ok(*r),
                _ => Err(VmError::NullReference),
            },
            value => Err(VmError::InvalidHeapValue {
                expected: "object",
                actual: value.kind_name(),
            }),
        }
    }

    fn native_collector_mode(&self, collector_ref: Reference) -> Result<i32, VmError> {
        match self.heap.lock().unwrap().get(collector_ref)? {
            HeapValue::Object { fields, .. } => match fields.get("__mode") {
                Some(Value::Int(mode)) => Ok(*mode),
                _ => Ok(0),
            },
            _ => Ok(0),
        }
    }

    fn native_collector_array(&self, collector_ref: Reference) -> Result<Reference, VmError> {
        match self.heap.lock().unwrap().get(collector_ref)? {
            HeapValue::Object { fields, .. } => match fields.get("__array") {
                Some(Value::Reference(r)) => Ok(*r),
                _ => Err(VmError::NullReference),
            },
            value => Err(VmError::InvalidHeapValue {
                expected: "object",
                actual: value.kind_name(),
            }),
        }
    }

    fn native_int_stream_collect(&mut self, stream_ref: Reference, collector_ref: Reference) -> Result<Option<Value>, VmError> {
        let source_array = self.native_int_stream_array(stream_ref)?;
        let mode = self.native_collector_mode(collector_ref)?;
        let heap = self.heap.lock().unwrap();
        let elements: Vec<Reference> = match heap.get(source_array)? {
            HeapValue::IntArray { values } => values.iter().map(|&v| Reference::Heap(v as usize)).collect(),
            _ => return Ok(None),
        };
        drop(heap);
        self.collect_with_mode(elements, mode, collector_ref)
    }

    fn native_long_stream_collect(&mut self, stream_ref: Reference, collector_ref: Reference) -> Result<Option<Value>, VmError> {
        let source_array = self.native_long_stream_array(stream_ref)?;
        let mode = self.native_collector_mode(collector_ref)?;
        let heap = self.heap.lock().unwrap();
        let elements: Vec<Reference> = match heap.get(source_array)? {
            HeapValue::LongArray { values } => values.iter().map(|&v| Reference::Heap(v as usize)).collect(),
            _ => return Ok(None),
        };
        drop(heap);
        self.collect_with_mode(elements, mode, collector_ref)
    }

    fn native_double_stream_collect(&mut self, stream_ref: Reference, collector_ref: Reference) -> Result<Option<Value>, VmError> {
        let source_array = self.native_double_stream_array(stream_ref)?;
        let mode = self.native_collector_mode(collector_ref)?;
        let heap = self.heap.lock().unwrap();
        let elements: Vec<Reference> = match heap.get(source_array)? {
            HeapValue::DoubleArray { values } => values.iter().map(|&v| Reference::Heap(v as usize)).collect(),
            _ => return Ok(None),
        };
        drop(heap);
        self.collect_with_mode(elements, mode, collector_ref)
    }

    fn collect_with_mode(&mut self, elements: Vec<Reference>, mode: i32, collector_ref: Reference) -> Result<Option<Value>, VmError> {
        match mode {
            1 => {
                let list_ref = self.heap.lock().unwrap().allocate(HeapValue::Object {
                    class_name: "java/util/ArrayList".to_string(),
                    fields: std::collections::HashMap::new(),
                });
                for elem_ref in elements {
                    self.call_virtual(list_ref, "add", "(Ljava/lang/Object;)Z", vec![Value::Reference(elem_ref)])?;
                }
                Ok(Some(Value::Reference(list_ref)))
            }
            2 => {
                let set_ref = self.heap.lock().unwrap().allocate(HeapValue::Object {
                    class_name: "java/util/HashSet".to_string(),
                    fields: std::collections::HashMap::new(),
                });
                for elem_ref in elements {
                    self.call_virtual(set_ref, "add", "(Ljava/lang/Object;)Z", vec![Value::Reference(elem_ref)])?;
                }
                Ok(Some(Value::Reference(set_ref)))
            }
            3 => {
                let count = elements.len() as i64;
                let mut fields = std::collections::HashMap::new();
                fields.insert("value".to_string(), Value::Long(count));
                let result = self.heap.lock().unwrap().allocate(HeapValue::Object {
                    class_name: "java/lang/Long".to_string(),
                    fields,
                });
                Ok(Some(Value::Reference(result)))
            }
            4 => {
                let mut strs = Vec::new();
                for elem_ref in elements {
                    if elem_ref != Reference::Null {
                        if let Ok(s) = self.stringify_heap(elem_ref) {
                            strs.push(s);
                        }
                    }
                }
                let result = self.new_string(strs.join(""));
                Ok(Some(result))
            }
            5 => {
                let delimiter_ref = self.native_collector_array(collector_ref)?;
                let delimiter = if delimiter_ref != Reference::Null {
                    self.stringify_heap(delimiter_ref)?
                } else {
                    String::new()
                };
                let mut strs = Vec::new();
                for elem_ref in elements {
                    if elem_ref != Reference::Null {
                        if let Ok(s) = self.stringify_heap(elem_ref) {
                            strs.push(s);
                        }
                    }
                }
                let result = self.new_string(strs.join(&delimiter));
                Ok(Some(result))
            }
            _ => Ok(None),
        }
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

    fn native_collectors_joining(&mut self, delimiter: Option<Reference>) -> Result<Option<Value>, VmError> {
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

    fn native_collectors_reducing(&mut self, identity: Reference, _combiner: Reference) -> Result<Option<Value>, VmError> {
        let mut fields = std::collections::HashMap::new();
        fields.insert("__mode".to_string(), Value::Int(6));
        fields.insert("__array".to_string(), Value::Reference(identity));
        let r = self.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: "__jvm_rs/NativeCollector".to_string(),
            fields,
        });
        Ok(Some(Value::Reference(r)))
    }

    fn native_collectors_to_map(&mut self, key_mapper: Reference, value_mapper: Reference) -> Result<Option<Value>, VmError> {
        let mut fields = std::collections::HashMap::new();
        fields.insert("__mode".to_string(), Value::Int(7));
        fields.insert("__array".to_string(), Value::Reference(key_mapper));
        let r = self.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: "__jvm_rs/NativeCollector".to_string(),
            fields,
        });
        Ok(Some(Value::Reference(r)))
    }

    /// Snapshot a `java.util/List`'s contents by calling `size()` and
    /// `get(i)` through virtual dispatch — works for ArrayList, LinkedList,
    /// and any user-defined List that implements the standard interface.
    fn list_snapshot(&mut self, list: Reference) -> Result<Vec<Reference>, VmError> {
        let size_res = self.call_virtual(list, "size", "()I", vec![])?;
        let size = match size_res {
            crate::vm::types::ExecutionResult::Value(Value::Int(n)) => n,
            _ => return Err(VmError::TypeMismatch {
                expected: "int",
                actual: "non-int from List.size()",
            }),
        };
        let mut out = Vec::with_capacity(size.max(0) as usize);
        for i in 0..size {
            let res = self.call_virtual(
                list,
                "get",
                "(I)Ljava/lang/Object;",
                vec![Value::Int(i)],
            )?;
            let r = match res {
                crate::vm::types::ExecutionResult::Value(Value::Reference(r)) => r,
                _ => return Err(VmError::TypeMismatch {
                    expected: "reference",
                    actual: "non-reference from List.get(I)",
                }),
            };
            out.push(r);
        }
        Ok(out)
    }

    /// Write a Rust-side vector back into a `java.util.List` using `set(i, v)`.
    fn list_overwrite(&mut self, list: Reference, values: &[Reference]) -> Result<(), VmError> {
        for (i, v) in values.iter().enumerate() {
            self.call_virtual(
                list,
                "set",
                "(ILjava/lang/Object;)Ljava/lang/Object;",
                vec![Value::Int(i as i32), Value::Reference(*v)],
            )?;
        }
        Ok(())
    }

    /// Compare two Java objects via `a.compareTo(b)`. Requires `a` to
    /// implement `Comparable`. Used by `Collections.sort` when no
    /// comparator is supplied.
    fn compare_natural(&mut self, a: Reference, b: Reference) -> Result<i32, VmError> {
        let res = self.call_virtual(
            a,
            "compareTo",
            "(Ljava/lang/Object;)I",
            vec![Value::Reference(b)],
        )?;
        match res {
            crate::vm::types::ExecutionResult::Value(Value::Int(n)) => Ok(n),
            _ => Err(VmError::TypeMismatch {
                expected: "int",
                actual: "non-int from compareTo",
            }),
        }
    }

    /// Compare via `Comparator.compare(a, b)`.
    fn compare_with(&mut self, cmp: Reference, a: Reference, b: Reference) -> Result<i32, VmError> {
        let res = self.call_virtual(
            cmp,
            "compare",
            "(Ljava/lang/Object;Ljava/lang/Object;)I",
            vec![Value::Reference(a), Value::Reference(b)],
        )?;
        match res {
            crate::vm::types::ExecutionResult::Value(Value::Int(n)) => Ok(n),
            _ => Err(VmError::TypeMismatch {
                expected: "int",
                actual: "non-int from Comparator.compare",
            }),
        }
    }

    fn native_collections_sort(
        &mut self,
        list: Reference,
        cmp: Option<Reference>,
    ) -> Result<(), VmError> {
        if list == Reference::Null {
            return Err(VmError::NullReference);
        }
        let mut values = self.list_snapshot(list)?;
        // Simple insertion sort — stable, fine for the sizes Java code
        // throws at Collections.sort in practice, and avoids the Arrays.sort
        // code path that currently pulls in unwanted JDK bytecode.
        for i in 1..values.len() {
            let mut j = i;
            while j > 0 {
                let cmp_result = match cmp {
                    Some(c) => self.compare_with(c, values[j - 1], values[j])?,
                    None => self.compare_natural(values[j - 1], values[j])?,
                };
                if cmp_result > 0 {
                    values.swap(j - 1, j);
                    j -= 1;
                } else {
                    break;
                }
            }
        }
        self.list_overwrite(list, &values)?;
        Ok(())
    }

    fn native_collections_reverse(&mut self, list: Reference) -> Result<(), VmError> {
        if list == Reference::Null {
            return Err(VmError::NullReference);
        }
        let mut values = self.list_snapshot(list)?;
        values.reverse();
        self.list_overwrite(list, &values)?;
        Ok(())
    }

    /// Read the internal JVM class name stored in a Class heap object's
    /// `__name` field (e.g., `java/util/HashMap`). Falls back to the heap
    /// object's declared class name if the field is missing.
    pub(super) fn class_internal_name(&self, reference: Reference) -> Result<String, VmError> {
        match self.heap.lock().unwrap().get(reference)? {
            HeapValue::Object { fields, class_name } => {
                if let Some(Value::Reference(name_ref)) = fields.get("__name") {
                    if let HeapValue::String(s) = self.heap.lock().unwrap().get(*name_ref)? {
                        return Ok(s.clone());
                    }
                }
                Ok(class_name.clone())
            }
            value => Err(VmError::InvalidHeapValue {
                expected: "object",
                actual: value.kind_name(),
            }),
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

    /// Unbox an `Integer` heap reference to its primitive `i32`.
    fn integer_value(&self, reference: Reference) -> Result<i32, VmError> {
        match self.heap.lock().unwrap().get(reference)? {
            HeapValue::Object { fields, .. } => Ok(fields
                .get("value")
                .and_then(|v| if let Value::Int(i) = v { Some(*i) } else { None })
                .unwrap_or(0)),
            _ => Ok(0),
        }
    }

    /// Compute a hash code for a heap object (identity hash for non-strings).
    fn hash_object(&self, reference: Reference) -> i32 {
        match reference {
            Reference::Null => 0,
            Reference::Heap(idx) => {
                let base = idx as i64;
                ((base >> 32) ^ base) as i32
            }
        }
    }

    fn hash_array_elements(&self, arr_ref: Reference) -> Result<i32, VmError> {
        let mut hash: i32 = 0;
        match self.heap.lock().unwrap().get(arr_ref)? {
            HeapValue::ReferenceArray { values, .. } => {
                for r in values {
                    let elem_hash = match r {
                        Reference::Null => 0,
                        _ => self.hash_object(*r),
                    };
                    hash = hash.wrapping_mul(31).wrapping_add(elem_hash);
                }
            }
            HeapValue::IntArray { values } => {
                for v in values {
                    hash = hash.wrapping_mul(31).wrapping_add(*v);
                }
            }
            HeapValue::LongArray { values } => {
                for v in values {
                    let elem_hash = ((*v as u64) ^ ((*v as u64) >> 32)) as i32;
                    hash = hash.wrapping_mul(31).wrapping_add(elem_hash);
                }
            }
            HeapValue::FloatArray { values } => {
                for v in values {
                    hash = hash.wrapping_mul(31).wrapping_add((*v as u32) as i32);
                }
            }
            HeapValue::DoubleArray { values } => {
                for v in values {
                    let bits = v.to_bits();
                    let elem_hash = ((bits as u64) ^ ((bits as u64) >> 32)) as i32;
                    hash = hash.wrapping_mul(31).wrapping_add(elem_hash);
                }
            }
            _ => {
                let base = match arr_ref {
                    Reference::Heap(idx) => idx as i64,
                    Reference::Null => 0,
                };
                hash = ((base >> 32) ^ base) as i32;
            }
        }
        Ok(hash)
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
            let mut heap = self.heap.lock().unwrap();
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
