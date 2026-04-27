use std::collections::HashMap;

use crate::vm::{ClassMethod, HeapValue, RuntimeClass, Value, Vm};

pub(super) fn bootstrap_java_lang(vm: &mut Vm) {
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
    vm.register_class(RuntimeClass {
        name: "java/lang/Object".to_string(),
        super_class: None,
        methods: object_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

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
    vm.register_class(RuntimeClass {
        name: "java/lang/Class".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: class_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![("__name".to_string(), "Ljava/lang/String;".to_string())],
        interfaces: vec![],
    });

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
    ] {
        string_methods.insert(
            (name.to_string(), desc.to_string()),
            ClassMethod::Native,
        );
    }
    vm.register_class(RuntimeClass {
        name: "java/lang/String".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: string_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

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
    let int_type = vm.class_object("int");
    integer_static.insert("TYPE".to_string(), Value::Reference(int_type));
    vm.register_class(RuntimeClass {
        name: "java/lang/Integer".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: integer_methods,
        static_fields: integer_static,
        instance_fields: vec![("value".to_string(), "I".to_string())],
        interfaces: vec![],
    });

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
    vm.register_class(RuntimeClass {
        name: "java/lang/Long".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: long_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![("value".to_string(), "J".to_string())],
        interfaces: vec![],
    });

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
    vm.register_class(RuntimeClass {
        name: "java/lang/Character".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: character_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

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
    vm.register_class(RuntimeClass {
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
    vm.register_class(RuntimeClass {
        name: "java/lang/StringBuilder".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: sb_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

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
    vm.register_class(RuntimeClass {
        name: "java/lang/Math".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: math_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    vm.register_class(RuntimeClass {
        name: "java/lang/Runnable".to_string(),
        super_class: None,
        methods: HashMap::new(),
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

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
    vm.register_class(RuntimeClass {
        name: "java/lang/Thread".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: thread_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![("target".to_string(), "Ljava/lang/Runnable;".to_string())],
        interfaces: vec![],
    });

    let exception_chain = [
        ("java/lang/Throwable", "java/lang/Object"),
        ("java/lang/Exception", "java/lang/Throwable"),
        ("java/lang/RuntimeException", "java/lang/Exception"),
        ("java/lang/IllegalThreadStateException", "java/lang/RuntimeException"),
        ("java/lang/ArithmeticException", "java/lang/RuntimeException"),
        ("java/lang/NullPointerException", "java/lang/RuntimeException"),
        ("java/lang/ClassCastException", "java/lang/RuntimeException"),
        ("java/lang/NegativeArraySizeException", "java/lang/RuntimeException"),
        ("java/lang/ArrayIndexOutOfBoundsException", "java/lang/RuntimeException"),
        ("java/lang/IndexOutOfBoundsException", "java/lang/RuntimeException"),
        ("java/lang/IllegalMonitorStateException", "java/lang/RuntimeException"),
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
        vm.register_class(RuntimeClass {
            name: name.to_string(),
            super_class: Some(parent.to_string()),
            methods,
            static_fields: HashMap::new(),
            instance_fields: vec![("message".to_string(), "Ljava/lang/String;".to_string())],
            interfaces: vec![],
        });
    }

    vm.register_class(RuntimeClass {
        name: "java/lang/Comparable".to_string(),
        super_class: None,
        methods: HashMap::new(),
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    vm.register_class(RuntimeClass {
        name: "java/lang/CharSequence".to_string(),
        super_class: None,
        methods: HashMap::new(),
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    for boxed in [
        "java/lang/Integer",
        "java/lang/Long",
        "java/lang/Boolean",
    ] {
        if let Some(class) = vm
            .runtime
            .lock()
            .unwrap()
            .classes
            .get_mut(boxed)
        {
            class.interfaces.push("java/lang/Comparable".to_string());
        }
    }
    if let Some(class) = vm
        .runtime
        .lock()
        .unwrap()
        .classes
        .get_mut("java/lang/String")
    {
        class.interfaces.push("java/lang/Comparable".to_string());
        class.interfaces.push("java/lang/CharSequence".to_string());
    }
}

pub(super) fn bootstrap_java_io(vm: &mut Vm) {
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
    vm.register_class(RuntimeClass {
        name: "java/io/PrintStream".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: ps_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    let print_stream_ref = vm.heap.lock().unwrap().allocate(HeapValue::Object {
        class_name: "java/io/PrintStream".to_string(),
        fields: HashMap::new(),
    });

    let err_stream_ref = vm.heap.lock().unwrap().allocate(HeapValue::Object {
        class_name: "java/io/PrintStream".to_string(),
        fields: HashMap::new(),
    });

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
    vm.register_class(RuntimeClass {
        name: "java/lang/System".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: system_methods,
        static_fields: system_static,
        instance_fields: vec![],
        interfaces: vec![],
    });

    // java/io/InputStream - abstract base for input streams
    let mut input_stream_methods = HashMap::new();
    for (name, desc) in [
        ("read", "()I"),
        ("read", "([B)I"),
        ("read", "([BII)I"),
        ("skip", "(J)J"),
        ("available", "()I"),
        ("close", "()V"),
        ("reset", "()V"),
        ("mark", "(I)V"),
        ("markSupported", "()Z"),
    ] {
        input_stream_methods.insert(
            (name.to_string(), desc.to_string()),
            ClassMethod::Native,
        );
    }
    vm.register_class(RuntimeClass {
        name: "java/io/InputStream".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: input_stream_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // java/io/OutputStream - abstract base for output streams
    let mut output_stream_methods = HashMap::new();
    for (name, desc) in [
        ("write", "(I)V"),
        ("write", "([B)V"),
        ("write", "([BII)V"),
        ("flush", "()V"),
        ("close", "()V"),
    ] {
        output_stream_methods.insert(
            (name.to_string(), desc.to_string()),
            ClassMethod::Native,
        );
    }
    vm.register_class(RuntimeClass {
        name: "java/io/OutputStream".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: output_stream_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // java/io/ByteArrayOutputStream - output stream backed by byte array
    let mut baos_methods = HashMap::new();
    baos_methods.insert(("<init>".to_string(), "()V".to_string()), ClassMethod::Native);
    for (name, desc) in [
        ("write", "(I)V"),
        ("write", "([B)V"),
        ("write", "([BII)V"),
        ("flush", "()V"),
        ("close", "()V"),
        ("toString", "()Ljava/lang/String;"),
        ("toByteArray", "()[B"),
        ("size", "()I"),
        ("reset", "()V"),
    ] {
        baos_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/io/ByteArrayOutputStream".to_string(),
        super_class: Some("java/io/OutputStream".to_string()),
        methods: baos_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![
            ("buf".to_string(), "[B".to_string()),
            ("count".to_string(), "I".to_string()),
        ],
        interfaces: vec![],
    });
}

pub(super) fn bootstrap_java_io_writer(vm: &mut Vm) {
    // java/io/Writer - abstract base for character output streams
    let mut writer_methods = HashMap::new();
    for (name, desc) in [
        ("write", "(I)V"),
        ("write", "([C)V"),
        ("write", "([CII)V"),
        ("write", "(Ljava/lang/String;)V"),
        ("write", "(Ljava/lang/String;II)V"),
        ("flush", "()V"),
        ("close", "()V"),
    ] {
        writer_methods.insert(
            (name.to_string(), desc.to_string()),
            ClassMethod::Native,
        );
    }
    vm.register_class(RuntimeClass {
        name: "java/io/Writer".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: writer_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // java/io/BufferedWriter - for efficient character output
    let mut bw_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "(Ljava/io/Writer;)V"),
        ("write", "(I)V"),
        ("write", "([C)V"),
        ("write", "([CII)V"),
        ("flush", "()V"),
        ("close", "()V"),
    ] {
        bw_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/io/BufferedWriter".to_string(),
        super_class: Some("java/io/Writer".to_string()),
        methods: bw_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // java/io/PrintWriter - character output stream with println support
    let mut pw_methods = HashMap::new();
    for desc in [
        "()V",
        "(Ljava/io/Writer;)V",
        "(Ljava/lang/String;)V",
        "(Z)V",
        "(C)V",
        "(I)V",
        "(J)V",
        "(F)V",
        "(D)V",
        "(Ljava/lang/String;)V",
        "(Ljava/lang/Object;)V",
    ] {
        pw_methods.insert(("println".to_string(), desc.to_string()), ClassMethod::Native);
        pw_methods.insert(("print".to_string(), desc.to_string()), ClassMethod::Native);
    }
    pw_methods.insert(
        ("<init>".to_string(), "()V".to_string()),
        ClassMethod::Native,
    );
    pw_methods.insert(
        ("<init>".to_string(), "(Ljava/io/Writer;)V".to_string()),
        ClassMethod::Native,
    );
    pw_methods.insert(
        ("<init>".to_string(), "(Ljava/io/OutputStream;)V".to_string()),
        ClassMethod::Native,
    );
    pw_methods.insert(
        ("append".to_string(), "(C)Ljava/io/Writer;".to_string()),
        ClassMethod::Native,
    );
    pw_methods.insert(
        ("append".to_string(), "(Ljava/lang/CharSequence;)Ljava/io/Writer;".to_string()),
        ClassMethod::Native,
    );
    pw_methods.insert(
        ("append".to_string(), "(Ljava/lang/CharSequence;II)Ljava/io/Writer;".to_string()),
        ClassMethod::Native,
    );
    pw_methods.insert(("flush".to_string(), "()V".to_string()), ClassMethod::Native);
    pw_methods.insert(("close".to_string(), "()V".to_string()), ClassMethod::Native);
    vm.register_class(RuntimeClass {
        name: "java/io/PrintWriter".to_string(),
        super_class: Some("java/io/Writer".to_string()),
        methods: pw_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // java/io/BufferedReader - for efficient character input
    let mut br_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "(Ljava/io/Reader;)V"),
        ("read", "()I"),
        ("read", "(I)I"),
        ("read", "([C)I"),
        ("read", "([CII)I"),
        ("skip", "(J)J"),
        ("ready", "()Z"),
        ("close", "()V"),
    ] {
        br_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    br_methods.insert(
        ("readLine".to_string(), "()Ljava/lang/String;".to_string()),
        ClassMethod::Native,
    );
    vm.register_class(RuntimeClass {
        name: "java/io/BufferedReader".to_string(),
        super_class: Some("java/io/Reader".to_string()),
        methods: br_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // java/io/Reader - abstract base for character input streams
    let mut reader_methods = HashMap::new();
    for (name, desc) in [
        ("read", "()I"),
        ("read", "(I)I"),
        ("read", "([C)I"),
        ("read", "([CII)I"),
        ("skip", "(J)J"),
        ("ready", "()Z"),
        ("close", "()V"),
        ("mark", "(I)V"),
        ("reset", "()V"),
        ("markSupported", "()Z"),
    ] {
        reader_methods.insert(
            (name.to_string(), desc.to_string()),
            ClassMethod::Native,
        );
    }
    vm.register_class(RuntimeClass {
        name: "java/io/Reader".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: reader_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // java/io/InputStreamReader - bridge from byte streams to characters
    let mut isr_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "(Ljava/io/InputStream;)V"),
        ("read", "()I"),
        ("read", "(I)I"),
        ("read", "([C)I"),
        ("read", "([CII)I"),
        ("close", "()V"),
    ] {
        isr_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/io/InputStreamReader".to_string(),
        super_class: Some("java/io/Reader".to_string()),
        methods: isr_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // java/io/OutputStreamWriter - bridge from characters to byte streams
    let mut osr_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "(Ljava/io/OutputStream;)V"),
        ("write", "(I)V"),
        ("write", "([C)V"),
        ("write", "([CII)V"),
        ("write", "(Ljava/lang/String;)V"),
        ("write", "(Ljava/lang/String;II)V"),
        ("flush", "()V"),
        ("close", "()V"),
    ] {
        osr_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/io/OutputStreamWriter".to_string(),
        super_class: Some("java/io/Writer".to_string()),
        methods: osr_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // java/io/File - represents file/directory paths
    let mut file_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "(Ljava/lang/String;)V"),
        ("exists", "()Z"),
        ("isFile", "()Z"),
        ("isDirectory", "()Z"),
        ("isHidden", "()Z"),
        ("length", "()J"),
        ("getPath", "()Ljava/lang/String;"),
        ("getName", "()Ljava/lang/String;"),
        ("getParent", "()Ljava/lang/String;"),
        ("canRead", "()Z"),
        ("canWrite", "()Z"),
        ("canExecute", "()Z"),
        ("mkdir", "()Z"),
        ("createNewFile", "()Z"),
        ("delete", "()Z"),
        ("list", "()[Ljava/lang/String;"),
        ("listFiles", "()[Ljava/io/File;"),
    ] {
        file_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/io/File".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: file_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![("path".to_string(), "Ljava/lang/String;".to_string())],
        interfaces: vec![],
    });
}

pub(super) fn bootstrap_java_util(vm: &mut Vm) {
    vm.register_class(RuntimeClass {
        name: "java/util/stream/IntStream".to_string(),
        super_class: None,
        methods: HashMap::new(),
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });
    vm.register_class(RuntimeClass {
        name: "java/util/stream/Stream".to_string(),
        super_class: None,
        methods: HashMap::new(),
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

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
    vm.register_class(RuntimeClass {
        name: "__jvm_rs/NativeIntStream".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: native_int_stream_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![
            ("__array".to_string(), "[I".to_string()),
        ],
        interfaces: vec!["java/util/stream/IntStream".to_string()],
    });

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
    vm.register_class(RuntimeClass {
        name: "__jvm_rs/NativeLongStream".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: native_long_stream_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![
            ("__array".to_string(), "[J".to_string()),
        ],
        interfaces: vec!["java/util/stream/LongStream".to_string()],
    });

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
    vm.register_class(RuntimeClass {
        name: "__jvm_rs/NativeDoubleStream".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: native_double_stream_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![
            ("__array".to_string(), "[D".to_string()),
        ],
        interfaces: vec!["java/util/stream/DoubleStream".to_string()],
    });

    let mut native_collector_methods = HashMap::new();
    native_collector_methods.insert(
        ("get".to_string(), "()Ljava/lang/Object;".to_string()),
        ClassMethod::Native,
    );
    native_collector_methods.insert(
        ("size".to_string(), "()I".to_string()),
        ClassMethod::Native,
    );
    vm.register_class(RuntimeClass {
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

    vm.register_class(RuntimeClass {
        name: "java/util/stream/Collectors".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: HashMap::new(),
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    vm.register_class(RuntimeClass {
        name: "java/util/stream/LongStream".to_string(),
        super_class: None,
        methods: HashMap::new(),
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });
    vm.register_class(RuntimeClass {
        name: "java/util/stream/DoubleStream".to_string(),
        super_class: None,
        methods: HashMap::new(),
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

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
        vm.register_class(RuntimeClass {
            name: name.to_string(),
            super_class: Some("java/lang/Object".to_string()),
            methods,
            static_fields: HashMap::new(),
            instance_fields: fields,
            interfaces: vec![],
        });
    }

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
    vm.register_class(RuntimeClass {
        name: "java/util/Optional".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: optional_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![("value".to_string(), "Ljava/lang/Object;".to_string())],
        interfaces: vec![],
    });

    // java/util/Scanner - for parsing input (Locale-less subset)
    let mut scanner_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "(Ljava/io/InputStream;)V"),
        ("<init>", "(Ljava/lang/String;)V"),
        ("hasNext", "()Z"),
        ("next", "()Ljava/lang/String;"),
        ("nextLine", "()Ljava/lang/String;"),
        ("hasNextInt", "()Z"),
        ("nextInt", "()I"),
        ("hasNextLong", "()Z"),
        ("nextLong", "()J"),
        ("hasNextDouble", "()Z"),
        ("nextDouble", "()D"),
        ("close", "()V"),
    ] {
        scanner_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/Scanner".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: scanner_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![("__input".to_string(), "Ljava/lang/String;".to_string())],
        interfaces: vec![],
    });
}

pub(super) fn bootstrap_java_nio(vm: &mut Vm) {
    let mut byte_buffer_methods = HashMap::new();
    for (name, desc) in [
        ("allocate", "(I)Ljava/nio/ByteBuffer;"),
        ("wrap", "( [B)Ljava/nio/ByteBuffer;"),
        ("wrap", "( [BII)Ljava/nio/ByteBuffer;"),
        ("capacity", "()I"),
        ("position", "()I"),
        ("position", "(I)Ljava/nio/Buffer;"),
        ("limit", "()I"),
        ("limit", "(I)Ljava/nio/Buffer;"),
        ("mark", "()Ljava/nio/Buffer;"),
        ("reset", "()Ljava/nio/Buffer;"),
        ("clear", "()Ljava/nio/Buffer;"),
        ("flip", "()Ljava/nio/Buffer;"),
        ("rewind", "()Ljava/nio/Buffer;"),
        ("remaining", "()I"),
        ("hasRemaining", "()Z"),
        ("get", "()B"),
        ("get", "(I)B"),
        ("put", "(B)Ljava/nio/ByteBuffer;"),
        ("put", "(IB)Ljava/nio/ByteBuffer;"),
        ("array", "()[B"),
        ("isDirect", "()Z"),
    ] {
        byte_buffer_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/nio/ByteBuffer".to_string(),
        super_class: Some("java/nio/Buffer".to_string()),
        methods: byte_buffer_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![
            ("__backing".to_string(), "[B".to_string()),
            ("__offset".to_string(), "I".to_string()),
        ],
        interfaces: vec![],
    });

    let mut buffer_methods = HashMap::new();
    for (name, desc) in [
        ("capacity", "()I"),
        ("position", "()I"),
        ("position", "(I)Ljava/nio/Buffer;"),
        ("limit", "()I"),
        ("limit", "(I)Ljava/nio/Buffer;"),
        ("mark", "()Ljava/nio/Buffer;"),
        ("reset", "()Ljava/nio/Buffer;"),
        ("clear", "()Ljava/nio/Buffer;"),
        ("flip", "()Ljava/nio/Buffer;"),
        ("rewind", "()Ljava/nio/Buffer;"),
        ("remaining", "()I"),
        ("hasRemaining", "()Z"),
    ] {
        buffer_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/nio/Buffer".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: buffer_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![
            ("__capacity".to_string(), "I".to_string()),
            ("__position".to_string(), "I".to_string()),
            ("__limit".to_string(), "I".to_string()),
        ],
        interfaces: vec![],
    });

    let mut char_buffer_methods = HashMap::new();
    for (name, desc) in [
        ("allocate", "(I)Ljava/nio/CharBuffer;"),
        ("wrap", "([C)Ljava/nio/CharBuffer;"),
        ("wrap", "([CII)Ljava/nio/CharBuffer;"),
        ("capacity", "()I"),
        ("position", "()I"),
        ("position", "(I)Ljava/nio/Buffer;"),
        ("limit", "()I"),
        ("limit", "(I)Ljava/nio/Buffer;"),
        ("mark", "()Ljava/nio/Buffer;"),
        ("reset", "()Ljava/nio/Buffer;"),
        ("clear", "()Ljava/nio/Buffer;"),
        ("flip", "()Ljava/nio/Buffer;"),
        ("rewind", "()Ljava/nio/Buffer;"),
        ("remaining", "()I"),
        ("hasRemaining", "()Z"),
        ("get", "()C"),
        ("get", "(I)C"),
        ("put", "(C)Ljava/nio/CharBuffer;"),
        ("put", "(IC)Ljava/nio/CharBuffer;"),
        ("array", "()[C"),
        ("length", "()I"),
    ] {
        char_buffer_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/nio/CharBuffer".to_string(),
        super_class: Some("java/nio/Buffer".to_string()),
        methods: char_buffer_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![
            ("__backing".to_string(), "[C".to_string()),
            ("__offset".to_string(), "I".to_string()),
        ],
        interfaces: vec!["java/lang/Appendable".to_string()],
    });

    // java.nio.file.Path - file system path representation
    let mut path_methods = HashMap::new();
    for (name, desc) in [
        ("getFileName", "()Ljava/lang/String;"),
        ("getParent", "()Ljava/nio/file/Path;"),
        ("getRoot", "()Ljava/nio/file/Path;"),
        ("isAbsolute", "()Z"),
        ("getNameCount", "()I"),
        ("getName", "(I)Ljava/lang/String;"),
        ("subpath", "(II)Ljava/nio/file/Path;"),
        ("toString", "()Ljava/lang/String;"),
        ("toUri", "()Ljava/net/URI;"),
        ("toAbsolutePath", "()Ljava/nio/file/Path;"),
        ("normalize", "()Ljava/nio/file/Path;"),
        ("resolve", "(Ljava/lang/String;)Ljava/nio/file/Path;"),
        ("startsWith", "(Ljava/lang/String;)Z"),
        ("endsWith", "(Ljava/lang/String;)Z"),
    ] {
        path_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/nio/file/Path".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: path_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![("__path".to_string(), "Ljava/lang/String;".to_string())],
        interfaces: vec![],
    });

    // java.nio.file.Paths - Path factory
    let mut paths_methods = HashMap::new();
    paths_methods.insert(
        ("get".to_string(), "(Ljava/lang/String;[Ljava/lang/String;)Ljava/nio/file/Path;".to_string()),
        ClassMethod::Native,
    );
    vm.register_class(RuntimeClass {
        name: "java/nio/file/Paths".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: paths_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // java.nio.file.Files - file operations utility
    let mut files_methods = HashMap::new();
    for (name, desc) in [
        ("exists", "(Ljava/nio/file/Path;[Ljava/nio/file/attribute/FileAttribute;)Z"),
        ("isRegularFile", "(Ljava/nio/file/Path;)Z"),
        ("isDirectory", "(Ljava/nio/file/Path;)Z"),
        ("createFile", "(Ljava/nio/file/Path;[Ljava/nio/file/attribute/FileAttribute;)Ljava/nio/file/Path;"),
        ("delete", "(Ljava/nio/file/Path;)V"),
        ("copy", "(Ljava/nio/file/Path;Ljava/nio/file/Path;[Ljava/nio/file/CopyOption;)Ljava/nio/file/Path;"),
        ("move", "(Ljava/nio/file/Path;Ljava/nio/file/Path;[Ljava/nio/file/CopyOption;)Ljava/nio/file/Path;"),
        ("readString", "(Ljava/nio/file/Path;)Ljava/lang/String;"),
        ("writeString", "(Ljava/nio/file/Path;Ljava/lang/CharSequence;[Ljava/nio/file/OpenOption;[Ljava/nio/file/attribute/FileAttribute;)Ljava/nio/file/Path;"),
        ("size", "(Ljava/nio/file/Path;)J"),
        ("isHidden", "(Ljava/nio/file/Path;)Z"),
        ("getFileStore", "(Ljava/nio/file/Path;)Ljava/nio/file/FileStore;"),
        ("newInputStream", "(Ljava/nio/file/Path;[Ljava/nio/file/OpenOption;)Ljava/io/InputStream;"),
        ("newOutputStream", "(Ljava/nio/file/Path;[Ljava/nio/file/OpenOption;)Ljava/io/OutputStream;"),
        ("newBufferedReader", "(Ljava/nio/file/Path;)Ljava/io/BufferedReader;"),
        ("newBufferedWriter", "(Ljava/nio/file/Path;[Ljava/nio/file/OpenOption;)Ljava/io/BufferedWriter;"),
    ] {
        files_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/nio/file/Files".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: files_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // java.nio.file.FileStore - file store representation
    let mut filestore_methods = HashMap::new();
    for (name, desc) in [
        ("name", "()Ljava/lang/String;"),
        ("type", "()Ljava/lang/String;"),
        ("getTotalSpace", "()J"),
        ("getUsableSpace", "()J"),
        ("getUnallocatedSpace", "()J"),
        ("isReadOnly", "()Z"),
    ] {
        filestore_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/nio/file/FileStore".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: filestore_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // java.nio.channels.Channels - channel utilities
    let mut channels_methods = HashMap::new();
    for (name, desc) in [
        ("newInputStream", "(Ljava/nio/channels/ReadableByteChannel;)Ljava/io/InputStream;"),
        ("newOutputStream", "(Ljava/nio/channels/WritableByteChannel;)Ljava/io/OutputStream;"),
        ("newChannel", "(Ljava/io/InputStream;)Ljava/nio/channels/ReadableByteChannel;"),
        ("newChannel", "(Ljava/io/OutputStream;)Ljava/nio/channels/WritableByteChannel;"),
    ] {
        channels_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/nio/channels/Channels".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: channels_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // java.io.Console - console access
    let mut console_methods = HashMap::new();
    for (name, desc) in [
        ("readLine", "()Ljava/lang/String;"),
        ("readLine", "(Ljava/lang/String;;[Ljava/lang/Object;)Ljava/lang/String;"),
        ("printf", "(Ljava/lang/String;[Ljava/lang/Object;)Ljava/io/Console;"),
        ("format", "(Ljava/lang/String;[Ljava/lang/Object;)Ljava/io/Console;"),
        ("flush", "()V"),
    ] {
        console_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    let console_ref = vm.heap.lock().unwrap().allocate(HeapValue::Object {
        class_name: "java/io/Console".to_string(),
        fields: HashMap::new(),
    });
    let mut console_static = HashMap::new();
    console_static.insert("__instance".to_string(), Value::Reference(console_ref));
    vm.register_class(RuntimeClass {
        name: "java/io/Console".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: console_methods,
        static_fields: console_static,
        instance_fields: vec![],
        interfaces: vec![],
    });

    // java.nio.file.OpenOption - marker interface for file open options
    vm.register_class(RuntimeClass {
        name: "java/nio/file/OpenOption".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: HashMap::new(),
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // java.nio.file.StandardOpenOption - enum with file open options
    let mut standard_open_option_methods = HashMap::new();
    standard_open_option_methods.insert(
        ("name".to_string(), "()Ljava/lang/String;".to_string()),
        ClassMethod::Native,
    );
    standard_open_option_methods.insert(
        ("ordinal".to_string(), "()I".to_string()),
        ClassMethod::Native,
    );

    // Create enum constant instances
    let read_name = vm.new_string("READ");
    let write_name = vm.new_string("WRITE");
    let append_name = vm.new_string("APPEND");
    let truncate_existing_name = vm.new_string("TRUNCATE_EXISTING");
    let create_name = vm.new_string("CREATE");
    let create_new_name = vm.new_string("CREATE_NEW");
    let delete_on_close_name = vm.new_string("DELETE_ON_CLOSE");

    let read_ref = vm.heap.lock().unwrap().allocate(HeapValue::Object {
        class_name: "java/nio/file/StandardOpenOption".to_string(),
        fields: {
            let mut f = HashMap::new();
            f.insert("name".to_string(), read_name);
            f.insert("ordinal".to_string(), Value::Int(0));
            f
        },
    });
    let write_ref = vm.heap.lock().unwrap().allocate(HeapValue::Object {
        class_name: "java/nio/file/StandardOpenOption".to_string(),
        fields: {
            let mut f = HashMap::new();
            f.insert("name".to_string(), write_name);
            f.insert("ordinal".to_string(), Value::Int(1));
            f
        },
    });
    let append_ref = vm.heap.lock().unwrap().allocate(HeapValue::Object {
        class_name: "java/nio/file/StandardOpenOption".to_string(),
        fields: {
            let mut f = HashMap::new();
            f.insert("name".to_string(), append_name);
            f.insert("ordinal".to_string(), Value::Int(2));
            f
        },
    });
    let truncate_existing_ref = vm.heap.lock().unwrap().allocate(HeapValue::Object {
        class_name: "java/nio/file/StandardOpenOption".to_string(),
        fields: {
            let mut f = HashMap::new();
            f.insert("name".to_string(), truncate_existing_name);
            f.insert("ordinal".to_string(), Value::Int(3));
            f
        },
    });
    let create_ref = vm.heap.lock().unwrap().allocate(HeapValue::Object {
        class_name: "java/nio/file/StandardOpenOption".to_string(),
        fields: {
            let mut f = HashMap::new();
            f.insert("name".to_string(), create_name);
            f.insert("ordinal".to_string(), Value::Int(4));
            f
        },
    });
    let create_new_ref = vm.heap.lock().unwrap().allocate(HeapValue::Object {
        class_name: "java/nio/file/StandardOpenOption".to_string(),
        fields: {
            let mut f = HashMap::new();
            f.insert("name".to_string(), create_new_name);
            f.insert("ordinal".to_string(), Value::Int(5));
            f
        },
    });
    let delete_on_close_ref = vm.heap.lock().unwrap().allocate(HeapValue::Object {
        class_name: "java/nio/file/StandardOpenOption".to_string(),
        fields: {
            let mut f = HashMap::new();
            f.insert("name".to_string(), delete_on_close_name);
            f.insert("ordinal".to_string(), Value::Int(6));
            f
        },
    });

    let mut static_fields = HashMap::new();
    static_fields.insert("READ".to_string(), Value::Reference(read_ref));
    static_fields.insert("WRITE".to_string(), Value::Reference(write_ref));
    static_fields.insert("APPEND".to_string(), Value::Reference(append_ref));
    static_fields.insert("TRUNCATE_EXISTING".to_string(), Value::Reference(truncate_existing_ref));
    static_fields.insert("CREATE".to_string(), Value::Reference(create_ref));
    static_fields.insert("CREATE_NEW".to_string(), Value::Reference(create_new_ref));
    static_fields.insert("DELETE_ON_CLOSE".to_string(), Value::Reference(delete_on_close_ref));

    vm.register_class(RuntimeClass {
        name: "java/nio/file/StandardOpenOption".to_string(),
        super_class: Some("java/lang/Enum".to_string()),
        methods: standard_open_option_methods,
        static_fields,
        instance_fields: vec![
            ("name".to_string(), "Ljava/lang/String;".to_string()),
            ("ordinal".to_string(), "I".to_string()),
        ],
        interfaces: vec!["java/nio/file/OpenOption".to_string()],
    });

    // java.nio.file.CopyOption - marker interface (used by Files.copy/move)
    vm.register_class(RuntimeClass {
        name: "java/nio/file/CopyOption".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: HashMap::new(),
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });
}

pub(super) fn bootstrap_other(vm: &mut Vm) {
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
    let unsafe_instance_ref = vm.heap.lock().unwrap().allocate(HeapValue::Object {
        class_name: "jdk/internal/misc/Unsafe".to_string(),
        fields: HashMap::new(),
    });
    let mut unsafe_static = HashMap::new();
    unsafe_static.insert("theUnsafe".to_string(), Value::Reference(unsafe_instance_ref));
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
    vm.register_class(RuntimeClass {
        name: "jdk/internal/misc/Unsafe".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: unsafe_methods,
        static_fields: unsafe_static,
        instance_fields: vec![],
        interfaces: vec![],
    });
}

pub(super) fn bootstrap_java_util_concurrent(vm: &mut Vm) {
    // --- java.util.concurrent.atomic ---
    let mut atomic_integer_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "(I)V"),
        ("get", "()I"),
        ("set", "(I)V"),
        ("getAndSet", "(I)I"),
        ("compareAndSet", "(II)Z"),
        ("incrementAndGet", "()I"),
        ("decrementAndGet", "()I"),
        ("getAndIncrement", "()I"),
        ("getAndDecrement", "()I"),
        ("addAndGet", "(I)I"),
        ("getAndAdd", "(I)I"),
    ] {
        atomic_integer_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/atomic/AtomicInteger".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: atomic_integer_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![("__value".to_string(), "I".to_string())],
        interfaces: vec![],
    });

    let mut atomic_long_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "(J)V"),
        ("get", "()J"),
        ("set", "(J)V"),
        ("getAndSet", "(J)J"),
        ("compareAndSet", "(JJ)Z"),
        ("incrementAndGet", "()J"),
        ("decrementAndGet", "()J"),
        ("addAndGet", "(J)J"),
    ] {
        atomic_long_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/atomic/AtomicLong".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: atomic_long_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![("__value".to_string(), "J".to_string())],
        interfaces: vec![],
    });

    let mut atomic_reference_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "(Ljava/lang/Object;)V"),
        ("get", "()Ljava/lang/Object;"),
        ("set", "(Ljava/lang/Object;)V"),
        ("getAndSet", "(Ljava/lang/Object;)Ljava/lang/Object;"),
        ("compareAndSet", "(Ljava/lang/Object;Ljava/lang/Object;)Z"),
    ] {
        atomic_reference_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/atomic/AtomicReference".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: atomic_reference_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![("__value".to_string(), "Ljava/lang/Object;".to_string())],
        interfaces: vec![],
    });

    let mut long_adder_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "()V"),
        ("add", "(J)V"),
        ("sum", "()J"),
        ("increment", "()V"),
        ("decrement", "()V"),
        ("reset", "()V"),
    ] {
        long_adder_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/atomic/LongAdder".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: long_adder_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![("__value".to_string(), "J".to_string())],
        interfaces: vec![],
    });

    let mut double_adder_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "()V"),
        ("add", "(D)V"),
        ("sum", "()D"),
        ("sumThenReset", "()D"),
    ] {
        double_adder_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/atomic/DoubleAdder".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: double_adder_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![("__value".to_string(), "D".to_string())],
        interfaces: vec![],
    });

    let mut long_accumulator_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "(Ljava/util/function/LongBinaryOperator;J)V"),
        ("get", "()J"),
        ("reset", "()V"),
        ("getThenReset", "()J"),
        ("accumulate", "(J)V"),
    ] {
        long_accumulator_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/atomic/LongAccumulator".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: long_accumulator_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    let mut double_accumulator_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "(Ljava/util/function/DoubleBinaryOperator;D)V"),
        ("get", "()D"),
        ("reset", "()V"),
        ("getThenReset", "()D"),
        ("accumulate", "(D)V"),
    ] {
        double_accumulator_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/atomic/DoubleAccumulator".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: double_accumulator_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // --- java.util.concurrent.locks ---
    let mut reentrant_lock_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "()V"),
        ("<init>", "(Z)V"),
        ("lock", "()V"),
        ("unlock", "()V"),
        ("tryLock", "()Z"),
        ("isHeldByCurrentThread", "()Z"),
        ("getHoldCount", "()I"),
    ] {
        reentrant_lock_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/locks/ReentrantLock".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: reentrant_lock_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![("__held".to_string(), "I".to_string())],
        interfaces: vec!["java/util/concurrent/locks/Lock".to_string()],
    });

    let mut read_write_lock_methods = HashMap::new();
    for (name, desc) in [
        ("readLock", "()Ljava/util/concurrent/locks/Lock;"),
        ("writeLock", "()Ljava/util/concurrent/locks/Lock;"),
    ] {
        read_write_lock_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/locks/ReadWriteLock".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: read_write_lock_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    let mut lock_methods = HashMap::new();
    for (name, desc) in [
        ("lock", "()V"),
        ("unlock", "()V"),
        ("tryLock", "()Z"),
        ("newCondition", "()Ljava/util/concurrent/locks/Condition;"),
    ] {
        lock_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/locks/Lock".to_string(),
        super_class: None,
        methods: lock_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    let mut condition_methods = HashMap::new();
    for (name, desc) in [
        ("await", "()V"),
        ("signal", "()V"),
        ("signalAll", "()V"),
    ] {
        condition_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/locks/Condition".to_string(),
        super_class: None,
        methods: condition_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    let mut stamped_lock_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "()V"),
        ("readLock", "()J"),
        ("writeLock", "()J"),
        ("tryReadLock", "()J"),
        ("tryWriteLock", "()J"),
        ("unlockRead", "(J)V"),
        ("unlockWrite", "(J)V"),
        ("unlock", "(J)V"),
        ("tryConvertToReadLock", "(J)J"),
        ("tryConvertToWriteLock", "(J)J"),
        ("isReadLocked", "()Z"),
        ("isWriteLocked", "()Z"),
        ("getReadLockCount", "()I"),
        ("validate", "(J)Z"),
    ] {
        stamped_lock_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/locks/StampedLock".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: stamped_lock_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    let mut abstract_ownable_synchronizer_methods = HashMap::new();
    for (name, desc) in [
        ("setExclusiveOwnerThread", "(Ljava/lang/Thread;)V"),
        ("getExclusiveOwnerThread", "()Ljava/lang/Thread;"),
    ] {
        abstract_ownable_synchronizer_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/locks/AbstractOwnableSynchronizer".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: abstract_ownable_synchronizer_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // --- java.util.concurrent ---
    let mut concurrent_hash_map_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "()V"),
        ("<init>", "(I)V"),
        ("<init>", "(IF)V"),
        ("<init>", "(Ljava/util/Map;)V"),
        ("size", "()I"),
        ("isEmpty", "()Z"),
        ("get", "(Ljava/lang/Object;)Ljava/lang/Object;"),
        ("put", "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;"),
        ("putIfAbsent", "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;"),
        ("remove", "(Ljava/lang/Object;)Ljava/lang/Object;"),
        ("containsKey", "(Ljava/lang/Object;)Z"),
        ("contains", "(Ljava/lang/Object;)Z"),
        ("clear", "()V"),
        ("keys", "()Ljava/util/Enumeration;"),
        ("elements", "()Ljava/util/Enumeration;"),
    ] {
        concurrent_hash_map_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/ConcurrentHashMap".to_string(),
        super_class: Some("java/lang/AbstractMap".to_string()),
        methods: concurrent_hash_map_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    let mut concurrent_linked_queue_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "()V"),
        ("<init>", "(Ljava/util/Collection;)V"),
        ("size", "()I"),
        ("isEmpty", "()Z"),
        ("offer", "(Ljava/lang/Object;)Z"),
        ("poll", "()Ljava/lang/Object;"),
        ("peek", "()Ljava/lang/Object;"),
        ("remove", "(Ljava/lang/Object;)Z"),
        ("contains", "(Ljava/lang/Object;)Z"),
    ] {
        concurrent_linked_queue_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/ConcurrentLinkedQueue".to_string(),
        super_class: Some("java/lang/AbstractQueue".to_string()),
        methods: concurrent_linked_queue_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec!["java/util/concurrent/BlockingQueue".to_string()],
    });

    let mut concurrent_linked_deque_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "()V"),
        ("<init>", "(Ljava/util/Collection;)V"),
        ("addFirst", "(Ljava/lang/Object;)V"),
        ("addLast", "(Ljava/lang/Object;)V"),
        ("offerFirst", "(Ljava/lang/Object;)Z"),
        ("offerLast", "(Ljava/lang/Object;)Z"),
        ("pollFirst", "()Ljava/lang/Object;"),
        ("pollLast", "()Ljava/lang/Object;"),
        ("peekFirst", "()Ljava/lang/Object;"),
        ("peekLast", "()Ljava/lang/Object;"),
        ("removeFirst", "()Ljava/lang/Object;"),
        ("removeLast", "()Ljava/lang/Object;"),
        ("size", "()I"),
        ("isEmpty", "()Z"),
    ] {
        concurrent_linked_deque_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/ConcurrentLinkedDeque".to_string(),
        super_class: Some("java/lang/AbstractQueue".to_string()),
        methods: concurrent_linked_deque_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    let mut copy_on_write_array_list_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "()V"),
        ("<init>", "([Ljava/lang/Object;)V"),
        ("<init>", "(Ljava/util/Collection;)V"),
        ("size", "()I"),
        ("isEmpty", "()Z"),
        ("get", "(I)Ljava/lang/Object;"),
        ("set", "(ILjava/lang/Object;)Ljava/lang/Object;"),
        ("add", "(ILjava/lang/Object;)V"),
        ("add", "(Ljava/lang/Object;)Z"),
        ("remove", "(Ljava/lang/Object;)Z"),
        ("remove", "(I)Ljava/lang/Object;"),
        ("contains", "(Ljava/lang/Object;)Z"),
        ("clear", "()V"),
    ] {
        copy_on_write_array_list_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/CopyOnWriteArrayList".to_string(),
        super_class: Some("java/lang/AbstractList".to_string()),
        methods: copy_on_write_array_list_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // --- Synchronizers ---
    let mut semaphore_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "(I)V"),
        ("<init>", "(IZ)V"),
        ("acquire", "()V"),
        ("acquire", "(I)V"),
        ("release", "()V"),
        ("release", "(I)V"),
        ("tryAcquire", "()Z"),
        ("tryAcquire", "(I)Z"),
        ("drainPermits", "()I"),
        ("availablePermits", "()I"),
    ] {
        semaphore_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/Semaphore".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: semaphore_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![("__permits".to_string(), "I".to_string())],
        interfaces: vec![],
    });

    let mut count_down_latch_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "(J)V"),
        ("await", "()V"),
        ("await", "(JLjava/util/concurrent/TimeUnit;)Z"),
        ("countDown", "()V"),
        ("getCount", "()J"),
    ] {
        count_down_latch_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/CountDownLatch".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: count_down_latch_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![("__count".to_string(), "J".to_string())],
        interfaces: vec![],
    });

    let mut cyclic_barrier_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "(I)V"),
        ("<init>", "(ILjava/lang/Runnable;)V"),
        ("await", "()I"),
        ("await", "(JLjava/util/concurrent/TimeUnit;)I"),
        ("reset", "()V"),
        ("getNumberWaiting", "()I"),
        ("isBroken", "()Z"),
    ] {
        cyclic_barrier_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/CyclicBarrier".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: cyclic_barrier_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![("__parties".to_string(), "I".to_string())],
        interfaces: vec![],
    });

    let mut exchanger_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "()V"),
        ("exchange", "(Ljava/lang/Object;)Ljava/lang/Object;"),
        ("exchange", "(Ljava/lang/Object;JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;"),
    ] {
        exchanger_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/Exchanger".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: exchanger_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    let mut phaser_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "()V"),
        ("<init>", "(I)V"),
        ("<init>", "(Ljava/util/concurrent/Phaser;)V"),
        ("register", "()I"),
        ("arrive", "()I"),
        ("arriveAndAwaitAdvance", "()I"),
        ("arriveAndDeregister", "()I"),
        ("bulkRegister", "(I)I"),
        ("getPhase", "()I"),
        ("getRegisteredParties", "()I"),
        ("getArrivedParties", "()I"),
        ("getUnarrivedParties", "()I"),
        ("forceTermination", "()V"),
    ] {
        phaser_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/Phaser".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: phaser_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // --- BlockingQueue implementations ---
    let mut array_blocking_queue_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "(I)V"),
        ("<init>", "(ILZ)V"),
        ("<init>", "(ILZLjava/util/Collection;)V"),
        ("offer", "(Ljava/lang/Object;)Z"),
        ("poll", "()Ljava/lang/Object;"),
        ("peek", "()Ljava/lang/Object;"),
        ("size", "()I"),
        ("isEmpty", "()Z"),
        ("remainingCapacity", "()I"),
        ("clear", "()V"),
        ("contains", "(Ljava/lang/Object;)Z"),
    ] {
        array_blocking_queue_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/ArrayBlockingQueue".to_string(),
        super_class: Some("java/lang/AbstractQueue".to_string()),
        methods: array_blocking_queue_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec!["java/util/concurrent/BlockingQueue".to_string()],
    });

    let mut linked_blocking_queue_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "()V"),
        ("<init>", "(I)V"),
        ("<init>", "(Ljava/util/Collection;)V"),
        ("offer", "(Ljava/lang/Object;)Z"),
        ("offer", "(Ljava/lang/Object;JLjava/util/concurrent/TimeUnit;)Z"),
        ("poll", "()Ljava/lang/Object;"),
        ("poll", "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;"),
        ("take", "()Ljava/lang/Object;"),
        ("peek", "()Ljava/lang/Object;"),
        ("size", "()I"),
        ("isEmpty", "()Z"),
        ("remainingCapacity", "()I"),
        ("clear", "()V"),
        ("contains", "(Ljava/lang/Object;)Z"),
    ] {
        linked_blocking_queue_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/LinkedBlockingQueue".to_string(),
        super_class: Some("java/lang/AbstractQueue".to_string()),
        methods: linked_blocking_queue_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec!["java/util/concurrent/BlockingQueue".to_string()],
    });

    let mut linked_blocking_deque_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "()V"),
        ("<init>", "(I)V"),
        ("<init>", "(Ljava/util/Collection;)V"),
        ("offerFirst", "(Ljava/lang/Object;)Z"),
        ("offerLast", "(Ljava/lang/Object;)Z"),
        ("offer", "(Ljava/lang/Object;)Z"),
        ("offer", "(Ljava/lang/Object;JLjava/util/concurrent/TimeUnit;)Z"),
        ("pollFirst", "()Ljava/lang/Object;"),
        ("pollLast", "()Ljava/lang/Object;"),
        ("poll", "()Ljava/lang/Object;"),
        ("poll", "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;"),
        ("takeFirst", "()Ljava/lang/Object;"),
        ("takeLast", "()Ljava/lang/Object;"),
        ("peekFirst", "()Ljava/lang/Object;"),
        ("peekLast", "()Ljava/lang/Object;"),
        ("size", "()I"),
        ("isEmpty", "()Z"),
        ("clear", "()V"),
        ("contains", "(Ljava/lang/Object;)Z"),
    ] {
        linked_blocking_deque_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/LinkedBlockingDeque".to_string(),
        super_class: Some("java/lang/AbstractDeque".to_string()),
        methods: linked_blocking_deque_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec!["java/util/concurrent/BlockingDeque".to_string()],
    });

    let mut synchronous_queue_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "()V"),
        ("<init>", "(Z)V"),
        ("offer", "(Ljava/lang/Object;)Z"),
        ("offer", "(Ljava/lang/Object;JLjava/util/concurrent/TimeUnit;)Z"),
        ("poll", "()Ljava/lang/Object;"),
        ("poll", "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;"),
        ("isEmpty", "()Z"),
        ("size", "()I"),
    ] {
        synchronous_queue_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/SynchronousQueue".to_string(),
        super_class: Some("java/lang/AbstractQueue".to_string()),
        methods: synchronous_queue_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec!["java/util/concurrent/BlockingQueue".to_string()],
    });

    let mut priority_blocking_queue_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "()V"),
        ("<init>", "(I)V"),
        ("<init>", "(Ljava/util/Collection;)V"),
        ("<init>", "(ILjava/util/Comparator;)V"),
        ("offer", "(Ljava/lang/Object;)Z"),
        ("offer", "(Ljava/lang/Object;JLjava/util/concurrent/TimeUnit;)Z"),
        ("poll", "()Ljava/lang/Object;"),
        ("poll", "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;"),
        ("take", "()Ljava/lang/Object;"),
        ("peek", "()Ljava/lang/Object;"),
        ("size", "()I"),
        ("isEmpty", "()Z"),
        ("clear", "()V"),
        ("contains", "(Ljava/lang/Object;)Z"),
    ] {
        priority_blocking_queue_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/PriorityBlockingQueue".to_string(),
        super_class: Some("java/lang/AbstractQueue".to_string()),
        methods: priority_blocking_queue_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec!["java/util/concurrent/BlockingQueue".to_string()],
    });

    let mut delay_queue_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "()V"),
        ("<init>", "(Ljava/util/Collection;)V"),
        ("offer", "(Ljava/lang/Object;)Z"),
        ("offer", "(Ljava/lang/Object;JLjava/util/concurrent/TimeUnit;)Z"),
        ("poll", "()Ljava/lang/Object;"),
        ("poll", "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;"),
        ("take", "()Ljava/lang/Object;"),
        ("peek", "()Ljava/lang/Object;"),
        ("size", "()I"),
        ("isEmpty", "()Z"),
        ("clear", "()V"),
        ("contains", "(Ljava/lang/Object;)Z"),
    ] {
        delay_queue_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/DelayQueue".to_string(),
        super_class: Some("java/lang/AbstractQueue".to_string()),
        methods: delay_queue_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec!["java/util/concurrent/BlockingQueue".to_string()],
    });

    // --- Queue interfaces ---
    let mut blocking_queue_methods = HashMap::new();
    for (name, desc) in [
        ("put", "(Ljava/lang/Object;)V"),
        ("offer", "(Ljava/lang/Object;)Z"),
        ("offer", "(Ljava/lang/Object;JLjava/util/concurrent/TimeUnit;)Z"),
        ("take", "()Ljava/lang/Object;"),
        ("poll", "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;"),
    ] {
        blocking_queue_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/BlockingQueue".to_string(),
        super_class: None,
        methods: blocking_queue_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/BlockingDeque".to_string(),
        super_class: None,
        methods: HashMap::new(),
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // --- Thread pools and executors ---
    let mut executor_service_methods = HashMap::new();
    for (name, desc) in [
        ("shutdown", "()V"),
        ("shutdownNow", "()Ljava/util/List;"),
        ("isShutdown", "()Z"),
        ("isTerminated", "()Z"),
        ("awaitTermination", "(JLjava/util/concurrent/TimeUnit;)Z"),
        ("submit", "(Ljava/lang/Runnable;)Ljava/util/concurrent/Future;"),
        ("submit", "(Ljava/lang/Runnable;Ljava/lang/Object;)Ljava/util/concurrent/Future;"),
        ("submit", "(Ljava/util/concurrent/Callable;)Ljava/util/concurrent/Future;"),
    ] {
        executor_service_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/ExecutorService".to_string(),
        super_class: None,
        methods: executor_service_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec!["java/util/concurrent/Executor".to_string()],
    });

    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/Executor".to_string(),
        super_class: None,
        methods: {
            let mut m = HashMap::new();
            m.insert(("execute".to_string(), "(Ljava/lang/Runnable;)V".to_string()), ClassMethod::Native);
            m
        },
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    let mut thread_pool_executor_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "(ILjava/util/concurrent/TimeUnit;Ljava/util/concurrent/BlockingQueue;)V"),
        ("<init>", "(IIJLjava/util/concurrent/TimeUnit;Ljava/util/concurrent/BlockingQueue;)V"),
        ("<init>", "(IIJLjava/util/concurrent/TimeUnit;Ljava/util/concurrent/BlockingQueue;Ljava/lang/ThreadFactory;)V"),
        ("<init>", "(IIJLjava/util/concurrent/TimeUnit;Ljava/util/concurrent/BlockingQueue;Ljava/util/concurrent/RejectedExecutionHandler;)V"),
        ("<init>", "(IIJLjava/util/concurrent/TimeUnit;Ljava/util/concurrent/BlockingQueue;Ljava/lang/ThreadFactory;Ljava/util/concurrent/RejectedExecutionHandler;)V"),
        ("execute", "(Ljava/lang/Runnable;)V"),
        ("shutdown", "()V"),
        ("shutdownNow", "()Ljava/util/List;"),
        ("isShutdown", "()Z"),
        ("isTerminated", "()Z"),
        ("isTerminating", "()Z"),
        ("awaitTermination", "(JLjava/util/concurrent/TimeUnit;)Z"),
        ("submit", "(Ljava/lang/Runnable;)Ljava/util/concurrent/Future;"),
        ("getPoolSize", "()I"),
        ("getActiveCount", "()I"),
        ("getTaskCount", "()J"),
        ("getCompletedTaskCount", "()J"),
        ("remove", "(Ljava/lang/Runnable;)Z"),
        ("purge", "()V"),
    ] {
        thread_pool_executor_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/ThreadPoolExecutor".to_string(),
        super_class: Some("java/util/concurrent/AbstractExecutorService".to_string()),
        methods: thread_pool_executor_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    let mut abstract_executor_service_methods = HashMap::new();
    for (name, desc) in [
        ("submit", "(Ljava/lang/Runnable;)Ljava/util/concurrent/Future;"),
        ("submit", "(Ljava/lang/Runnable;Ljava/lang/Object;)Ljava/util/concurrent/Future;"),
        ("submit", "(Ljava/util/concurrent/Callable;)Ljava/util/concurrent/Future;"),
        ("invokeAll", "(Ljava/util/Collection;)Ljava/util/List;"),
        ("invokeAll", "(Ljava/util/Collection;JLjava/util/concurrent/TimeUnit;)Ljava/util/List;"),
        ("invokeAny", "(Ljava/util/Collection;)Ljava/lang/Object;"),
        ("invokeAny", "(Ljava/util/Collection;JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;"),
    ] {
        abstract_executor_service_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/AbstractExecutorService".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: abstract_executor_service_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec!["java/util/concurrent/ExecutorService".to_string()],
    });

    let mut executors_methods = HashMap::new();
    for (name, desc) in [
        ("newSingleThreadExecutor", "()Ljava/util/concurrent/ExecutorService;"),
        ("newFixedThreadPool", "(I)Ljava/util/concurrent/ExecutorService;"),
        ("newCachedThreadPool", "()Ljava/util/concurrent/ExecutorService;"),
        ("newSingleThreadScheduledExecutor", "()Ljava/util/concurrent/ScheduledExecutorService;"),
        ("newScheduledThreadPool", "(I)Ljava/util/concurrent/ScheduledExecutorService;"),
    ] {
        executors_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/Executors".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: executors_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // --- ScheduledExecutorService ---
    let mut scheduled_executor_service_methods = HashMap::new();
    for (name, desc) in [
        ("schedule", "(Ljava/lang/Runnable;JLjava/util/concurrent/TimeUnit;)Ljava/util/concurrent/ScheduledFuture;"),
        ("schedule", "(Ljava/util/concurrent/Callable;JLjava/util/concurrent/TimeUnit;)Ljava/util/concurrent/ScheduledFuture;"),
        ("scheduleAtFixedRate", "(Ljava/lang/Runnable;JJLjava/util/concurrent/TimeUnit;)Ljava/util/concurrent/ScheduledFuture;"),
        ("scheduleWithFixedDelay", "(Ljava/lang/Runnable;JJLjava/util/concurrent/TimeUnit;)Ljava/util/concurrent/ScheduledFuture;"),
    ] {
        scheduled_executor_service_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/ScheduledExecutorService".to_string(),
        super_class: None,
        methods: scheduled_executor_service_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec!["java/util/concurrent/ExecutorService".to_string()],
    });

    let mut scheduled_future_methods = HashMap::new();
    scheduled_future_methods.insert(("getDelay".to_string(), "(Ljava/util/concurrent/TimeUnit;)J".to_string()), ClassMethod::Native);
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/ScheduledFuture".to_string(),
        super_class: None,
        methods: scheduled_future_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec!["java/util/concurrent/Delayed".to_string()],
    });

    let mut delayed_methods = HashMap::new();
    delayed_methods.insert(("getDelay".to_string(), "(Ljava/util/concurrent/TimeUnit;)J".to_string()), ClassMethod::Native);
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/Delayed".to_string(),
        super_class: None,
        methods: delayed_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec!["java/lang/Comparable".to_string()],
    });

    // --- ForkJoin ---
    let mut fork_join_pool_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "(I)V"),
        ("<init>", "(ILjava/util/concurrent/ForkJoinPool$Factory;)V"),
        ("<init>", "(ILjava/util/concurrent/ForkJoinPool$Factory;Ljava/util/concurrent/RejectedExecutionHandler;Z)V"),
        ("submit", "(Ljava/lang/Runnable;)Ljava/util/concurrent/Future;"),
        ("submit", "(Ljava/util/concurrent/ForkJoinTask;)Ljava/util/concurrent/ForkJoinTask;"),
        ("invoke", "(Ljava/util/concurrent/ForkJoinTask;)Ljava/lang/Object;"),
        ("execute", "(Ljava/lang/Runnable;)V"),
        ("shutdown", "()V"),
        ("shutdownNow", "()Ljava/util/List;"),
        ("isShutdown", "()Z"),
        ("isTerminated", "()Z"),
        ("isTerminating", "()Z"),
        ("awaitTermination", "(JLjava/util/concurrent/TimeUnit;)Z"),
        ("getPoolSize", "()I"),
        ("getActiveThreadCount", "()I"),
        ("getStealCount", "()J"),
        ("getQueuedTaskCount", "()J"),
        ("getQueuedSubmissionCount", "()I"),
        ("hasQueuedSubmissions", "()Z"),
        ("commonPool", "()Ljava/util/concurrent/ForkJoinPool;"),
    ] {
        fork_join_pool_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/ForkJoinPool".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: fork_join_pool_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    let mut fork_join_task_methods = HashMap::new();
    for (name, desc) in [
        ("fork", "()Ljava/util/concurrent/ForkJoinTask;"),
        ("join", "()Ljava/lang/Object;"),
        ("invoke", "()Ljava/lang/Object;"),
        ("cancel", "(Z)Z"),
        ("isDone", "()Z"),
        ("isCompletedNormally", "()Z"),
        ("isCompletedAbnormally", "()Z"),
        ("isCancelled", "()Z"),
        ("quietlyJoin", "()V"),
        ("quietlyFork", "()V"),
        ("get", "()Ljava/lang/Object;"),
        ("get", "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;"),
    ] {
        fork_join_task_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/ForkJoinTask".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: fork_join_task_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec!["java/util/concurrent/Future".to_string()],
    });

    let mut counted_completer_methods = HashMap::new();
    for (name, desc) in [
        ("compute", "()V"),
        ("onCompletion", "(Ljava/util/concurrent/CountedCompleter;)V"),
        ("getRawResult", "()Ljava/lang/Object;"),
    ] {
        counted_completer_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/CountedCompleter".to_string(),
        super_class: Some("java/util/concurrent/ForkJoinTask".to_string()),
        methods: counted_completer_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    let mut recursive_task_methods = HashMap::new();
    for (name, desc) in [
        ("compute", "()Ljava/lang/Object;"),
        ("getRawResult", "()Ljava/lang/Object;"),
    ] {
        recursive_task_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/RecursiveTask".to_string(),
        super_class: Some("java/util/concurrent/ForkJoinTask".to_string()),
        methods: recursive_task_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    let mut recursive_action_methods = HashMap::new();
    for (name, desc) in [
        ("compute", "()V"),
        ("getRawResult", "()Ljava/lang/Object;"),
    ] {
        recursive_action_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/RecursiveAction".to_string(),
        super_class: Some("java/util/concurrent/ForkJoinTask".to_string()),
        methods: recursive_action_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // --- Futures ---
    let mut future_methods = HashMap::new();
    for (name, desc) in [
        ("get", "()Ljava/lang/Object;"),
        ("get", "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;"),
        ("isDone", "()Z"),
        ("isCancelled", "()Z"),
        ("cancel", "(Z)Z"),
    ] {
        future_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/Future".to_string(),
        super_class: None,
        methods: future_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    let mut completable_future_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "()V"),
        ("get", "()Ljava/lang/Object;"),
        ("get", "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;"),
        ("isDone", "()Z"),
        ("isCancelled", "()Z"),
        ("cancel", "(Z)Z"),
        ("complete", "(Ljava/lang/Object;)Z"),
        ("completedFuture", "(Ljava/lang/Object;)Ljava/util/concurrent/CompletableFuture;"),
        ("runAsync", "(Ljava/lang/Runnable;)Ljava/util/concurrent/CompletableFuture;"),
        ("supplyAsync", "(Ljava/util/function/Supplier;)Ljava/util/concurrent/CompletableFuture;"),
        ("thenApply", "(Ljava/util/function/Function;)Ljava/util/concurrent/CompletableFuture;"),
        ("thenApplyAsync", "(Ljava/util/function/Function;)Ljava/util/concurrent/CompletableFuture;"),
        ("thenAccept", "(Ljava/util/function/Consumer;)Ljava/util/concurrent/CompletableFuture;"),
        ("thenRun", "(Ljava/lang/Runnable;)Ljava/util/concurrent/CompletableFuture;"),
        ("join", "()Ljava/lang/Object;"),
    ] {
        completable_future_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/CompletableFuture".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: completable_future_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec!["java/util/concurrent/Future".to_string(), "java/util/concurrent/CompletionStage".to_string()],
    });

    let mut completion_stage_methods = HashMap::new();
    for (name, desc) in [
        ("thenApply", "(Ljava/util/function/Function;)Ljava/util/concurrent/CompletionStage;"),
        ("thenApplyAsync", "(Ljava/util/function/Function;)Ljava/util/concurrent/CompletionStage;"),
        ("thenAccept", "(Ljava/util/function/Consumer;)Ljava/util/concurrent/CompletionStage;"),
        ("thenAcceptAsync", "(Ljava/util/function/Consumer;)Ljava/util/concurrent/CompletionStage;"),
        ("thenRun", "(Ljava/lang/Runnable;)Ljava/util/concurrent/CompletionStage;"),
        ("thenRunAsync", "(Ljava/lang/Runnable;)Ljava/util/concurrent/CompletionStage;"),
        ("thenCombine", "(Ljava/util/concurrent/CompletionStage;Ljava/util/function/BiFunction;)Ljava/util/concurrent/CompletionStage;"),
        ("thenCompose", "(Ljava/util/function/Function;)Ljava/util/concurrent/CompletionStage;"),
        ("exceptionally", "(Ljava/util/function/Function;)Ljava/util/concurrent/CompletionStage;"),
        ("whenComplete", "(Ljava/util/function/BiConsumer;)Ljava/util/concurrent/CompletionStage;"),
        ("handle", "(Ljava/util/function/BiFunction;)Ljava/util/concurrent/CompletionStage;"),
    ] {
        completion_stage_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/CompletionStage".to_string(),
        super_class: None,
        methods: completion_stage_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // --- TimeUnit ---
    let mut time_unit_methods = HashMap::new();
    for (name, desc) in [
        ("toNanos", "(J)J"),
        ("toMicros", "(J)J"),
        ("toMillis", "(J)J"),
        ("toSeconds", "(J)J"),
        ("sleep", "(J)V"),
    ] {
        time_unit_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/TimeUnit".to_string(),
        super_class: Some("java/lang/Enum".to_string()),
        methods: time_unit_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // --- Concurrent collection views ---
    let mut concurrent_skip_list_map_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "()V"),
        ("<init>", "(Ljava/util/Comparator;)V"),
        ("<init>", "(Ljava/util/Map;)V"),
        ("size", "()I"),
        ("isEmpty", "()Z"),
        ("get", "(Ljava/lang/Object;)Ljava/lang/Object;"),
        ("put", "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;"),
        ("remove", "(Ljava/lang/Object;)Ljava/lang/Object;"),
        ("containsKey", "(Ljava/lang/Object;)Z"),
        ("clear", "()V"),
        ("firstKey", "()Ljava/lang/Object;"),
        ("lastKey", "()Ljava/lang/Object;"),
    ] {
        concurrent_skip_list_map_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/ConcurrentSkipListMap".to_string(),
        super_class: Some("java/lang/AbstractMap".to_string()),
        methods: concurrent_skip_list_map_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    let mut concurrent_skip_list_set_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "()V"),
        ("<init>", "(Ljava/util/Comparator;)V"),
        ("<init>", "(Ljava/util/Collection;)V"),
        ("<init>", "(Ljava/util/SortedSet;)V"),
        ("size", "()I"),
        ("isEmpty", "()Z"),
        ("add", "(Ljava/lang/Object;)Z"),
        ("remove", "(Ljava/lang/Object;)Z"),
        ("contains", "(Ljava/lang/Object;)Z"),
        ("clear", "()V"),
        ("first", "()Ljava/lang/Object;"),
        ("last", "()Ljava/lang/Object;"),
    ] {
        concurrent_skip_list_set_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/ConcurrentSkipListSet".to_string(),
        super_class: Some("java/lang/AbstractSet".to_string()),
        methods: concurrent_skip_list_set_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    let mut key_set_view_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "(Ljava/util/concurrent/ConcurrentHashMap;Ljava/lang/Object;)V"),
        ("size", "()I"),
        ("isEmpty", "()Z"),
        ("contains", "(Ljava/lang/Object;)Z"),
        ("add", "(Ljava/lang/Object;)Z"),
        ("remove", "(Ljava/lang/Object;)Z"),
        ("getMap", "()Ljava/util/concurrent/ConcurrentHashMap;"),
    ] {
        key_set_view_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/ConcurrentHashMap$KeySetView".to_string(),
        super_class: Some("java/lang/AbstractSet".to_string()),
        methods: key_set_view_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // --- SubmissionPublisher (Flow) ---
    let mut submission_publisher_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "()V"),
        ("<init>", "(Ljava/util/concurrent/ExecutorService;I)V"),
        ("submit", "(Ljava/lang/Object;)I"),
        ("offer", "(Ljava/lang/Object;Ljava/util/concurrent/TimeUnit;)I"),
        ("close", "()V"),
        ("isClosed", "()Z"),
        ("hasSubscribers", "()Z"),
        ("getSubscriberCount", "()I"),
    ] {
        submission_publisher_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/SubmissionPublisher".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: submission_publisher_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec!["java/util/concurrent/Flow$Publisher".to_string()],
    });

    let mut flow_subscriber_methods = HashMap::new();
    for (name, desc) in [
        ("onNext", "(Ljava/lang/Object;)V"),
        ("onError", "(Ljava/lang/Throwable;)V"),
        ("onComplete", "()V"),
        ("onSubscribe", "(Ljava/util/concurrent/Flow$Subscription;)V"),
    ] {
        flow_subscriber_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/Flow$Subscriber".to_string(),
        super_class: None,
        methods: flow_subscriber_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    let mut flow_subscription_methods = HashMap::new();
    for (name, desc) in [
        ("request", "(J)V"),
        ("cancel", "()V"),
    ] {
        flow_subscription_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/Flow$Subscription".to_string(),
        super_class: None,
        methods: flow_subscription_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    let mut flow_processor_methods = HashMap::new();
    for (name, desc) in [
        ("onNext", "(Ljava/lang/Object;)V"),
        ("onError", "(Ljava/lang/Throwable;)V"),
        ("onComplete", "()V"),
        ("onSubscribe", "(Ljava/util/concurrent/Flow$Subscription;)V"),
    ] {
        flow_processor_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/Flow$Processor".to_string(),
        super_class: None,
        methods: flow_processor_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec!["java/util/concurrent/Flow$Subscriber".to_string(), "java/util/concurrent/Flow$Publisher".to_string()],
    });

    let mut flow_publisher_methods = HashMap::new();
    flow_publisher_methods.insert(("subscribe".to_string(), "(Ljava/util/concurrent/Flow$Subscriber;)V".to_string()), ClassMethod::Native);
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/Flow$Publisher".to_string(),
        super_class: None,
        methods: flow_publisher_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // --- Callable and ThreadFactory ---
    let mut callable_methods = HashMap::new();
    callable_methods.insert(("call".to_string(), "()Ljava/lang/Object;".to_string()), ClassMethod::Native);
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/Callable".to_string(),
        super_class: None,
        methods: callable_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    let mut thread_factory_methods = HashMap::new();
    thread_factory_methods.insert(("newThread".to_string(), "(Ljava/lang/Runnable;)Ljava/lang/Thread;".to_string()), ClassMethod::Native);
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/ThreadFactory".to_string(),
        super_class: None,
        methods: thread_factory_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    let mut rejected_execution_handler_methods = HashMap::new();
    rejected_execution_handler_methods.insert(("rejectedExecution".to_string(), "(Ljava/lang/Runnable;Ljava/util/concurrent/ThreadPoolExecutor;)V".to_string()), ClassMethod::Native);
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/RejectedExecutionHandler".to_string(),
        super_class: None,
        methods: rejected_execution_handler_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    // --- VarHandle ---
    let mut varhandle_methods = HashMap::new();
    for (name, desc) in [
        ("get", "(Ljava/lang/Object;)Ljava/lang/Object;"),
        ("set", "(Ljava/lang/Object;Ljava/lang/Object;)V"),
        ("compareAndSet", "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Z"),
        ("weakCompareAndSet", "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Z"),
        ("getAndSet", "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;"),
    ] {
        varhandle_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/concurrent/atomic/VarHandle".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: varhandle_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });
}
