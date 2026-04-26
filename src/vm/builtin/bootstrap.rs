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
