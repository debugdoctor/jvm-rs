use std::collections::HashMap;

use crate::vm::{ClassMethod, RuntimeClass, Value, Vm};

pub(super) fn bootstrap_java_text(vm: &mut Vm) {
    let mut numberformat_methods = HashMap::new();
    for (name, desc) in [
        ("format", "(I)Ljava/lang/String;"),
        ("format", "(J)Ljava/lang/String;"),
        ("format", "(F)Ljava/lang/String;"),
        ("format", "(D)Ljava/lang/String;"),
        ("format", "(Ljava/lang/Object;)Ljava/lang/String;"),
    ] {
        numberformat_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/text/NumberFormat".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: numberformat_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    let mut decimalformat_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "(Ljava/lang/String;)V"),
        ("applyPattern", "(Ljava/lang/String;)V"),
        ("format", "(D)Ljava/lang/String;"),
        ("format", "(I)Ljava/lang/String;"),
        ("format", "(J)Ljava/lang/String;"),
        ("setMaximumFractionDigits", "(I)V"),
        ("setMinimumFractionDigits", "(I)V"),
        ("setMaximumIntegerDigits", "(I)V"),
        ("setMinimumIntegerDigits", "(I)V"),
    ] {
        decimalformat_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/text/DecimalFormat".to_string(),
        super_class: Some("java/text/NumberFormat".to_string()),
        methods: decimalformat_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![("__pattern".to_string(), "Ljava/lang/String;".to_string())],
        interfaces: vec![],
    });

    let mut messageformat_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "(Ljava/lang/String;)V"),
        (
            "format",
            "(Ljava/lang/String;[Ljava/lang/Object;)Ljava/lang/String;",
        ),
        ("format", "(Ljava/lang/Object;)Ljava/lang/String;"),
    ] {
        messageformat_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/text/MessageFormat".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: messageformat_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![("__pattern".to_string(), "Ljava/lang/String;".to_string())],
        interfaces: vec![],
    });
}
