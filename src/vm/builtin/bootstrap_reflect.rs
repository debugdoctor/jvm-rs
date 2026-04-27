use std::collections::HashMap;

use crate::vm::{ClassMethod, RuntimeClass, Value, Vm};

pub(super) fn bootstrap_java_lang_reflect(vm: &mut Vm) {
    let mut class_methods = HashMap::new();
    for (name, desc) in [
        ("getDeclaredMethod", "(Ljava/lang/String;[Ljava/lang/Class;)Ljava/lang/reflect/Method;"),
        ("getDeclaredMethods", "()[Ljava/lang/reflect/Method;"),
        ("getDeclaredField", "(Ljava/lang/String;)Ljava/lang/reflect/Field;"),
        ("getDeclaredFields", "()[Ljava/lang/reflect/Field;"),
        ("getDeclaredConstructor", "([Ljava/lang/Class;)Ljava/lang/reflect/Constructor;"),
        ("getDeclaredConstructors", "()[Ljava/lang/reflect/Constructor;"),
        ("getSuperclass", "()Ljava/lang/Class;"),
        ("getInterfaces", "()[Ljava/lang/Class;"),
        ("getModifiers", "()I"),
        ("getComponentType", "()Ljava/lang/Class;"),
        ("desiredAssertionStatus", "()Z"),
        ("getName", "()Ljava/lang/String;"),
        ("getSimpleName", "()Ljava/lang/String;"),
        ("isArray", "()Z"),
        ("isInterface", "()Z"),
        ("isPrimitive", "()Z"),
        ("isHidden", "()Z"),
        ("toString", "()Ljava/lang/String;"),
    ] {
        class_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/lang/Class".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: class_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![("__name".to_string(), "Ljava/lang/String;".to_string())],
        interfaces: vec![],
    });

    let mut method_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "(Ljava/lang/Class;Ljava/lang/String;Ljava/lang/String;[Ljava/lang/Class;)V"),
        ("getName", "()Ljava/lang/String;"),
        ("getReturnType", "()Ljava/lang/Class;"),
        ("getParameterTypes", "()[Ljava/lang/Class;"),
        ("getDeclaringClass", "()Ljava/lang/Class;"),
        ("getModifiers", "()I"),
        ("invoke", "(Ljava/lang/Object;[Ljava/lang/Object;)Ljava/lang/Object;"),
        ("equals", "(Ljava/lang/Object;)Z"),
        ("hashCode", "()I"),
        ("toString", "()Ljava/lang/String;"),
    ] {
        method_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/lang/reflect/Method".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: method_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![
            ("__declaring_class".to_string(), "Ljava/lang/Class;".to_string()),
            ("__name".to_string(), "Ljava/lang/String;".to_string()),
            ("__descriptor".to_string(), "Ljava/lang/String;".to_string()),
            ("__parameter_types".to_string(), "[Ljava/lang/Class;".to_string()),
            ("__return_type".to_string(), "Ljava/lang/Class;".to_string()),
            ("__modifiers".to_string(), "I".to_string()),
        ],
        interfaces: vec![],
    });

    let mut field_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "(Ljava/lang/Class;Ljava/lang/String;Ljava/lang/String;Ljava/lang/Class;I)V"),
        ("getName", "()Ljava/lang/String;"),
        ("getType", "()Ljava/lang/Class;"),
        ("getDeclaringClass", "()Ljava/lang/Class;"),
        ("getModifiers", "()I"),
        ("get", "(Ljava/lang/Object;)Ljava/lang/Object;"),
        ("set", "(Ljava/lang/Object;Ljava/lang/Object;)V"),
        ("getInt", "(Ljava/lang/Object;)I"),
        ("setInt", "(Ljava/lang/Object;I)V"),
        ("getLong", "(Ljava/lang/Object;)J"),
        ("setLong", "(Ljava/lang/Object;J)V"),
        ("getObject", "(Ljava/lang/Object;)Ljava/lang/Object;"),
        ("setObject", "(Ljava/lang/Object;Ljava/lang/Object;)V"),
        ("equals", "(Ljava/lang/Object;)Z"),
        ("hashCode", "()I"),
        ("toString", "()Ljava/lang/String;"),
    ] {
        field_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/lang/reflect/Field".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: field_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![
            ("__declaring_class".to_string(), "Ljava/lang/Class;".to_string()),
            ("__name".to_string(), "Ljava/lang/String;".to_string()),
            ("__type".to_string(), "Ljava/lang/Class;".to_string()),
            ("__descriptor".to_string(), "Ljava/lang/String;".to_string()),
            ("__modifiers".to_string(), "I".to_string()),
            ("__slot".to_string(), "I".to_string()),
        ],
        interfaces: vec![],
    });

    let mut constructor_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "([Ljava/lang/Class;)V"),
        ("getParameterTypes", "()[Ljava/lang/Class;"),
        ("getDeclaringClass", "()Ljava/lang/Class;"),
        ("getModifiers", "()I"),
        ("newInstance", "([Ljava/lang/Object;)Ljava/lang/Object;"),
        ("equals", "(Ljava/lang/Object;)Z"),
        ("hashCode", "()I"),
        ("toString", "()Ljava/lang/String;"),
    ] {
        constructor_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/lang/reflect/Constructor".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: constructor_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![
            ("__declaring_class".to_string(), "Ljava/lang/Class;".to_string()),
            ("__parameter_types".to_string(), "[Ljava/lang/Class;".to_string()),
            ("__modifiers".to_string(), "I".to_string()),
            ("__slot".to_string(), "I".to_string()),
        ],
        interfaces: vec![],
    });

    let mut accessibleobject_methods = HashMap::new();
    for (name, desc) in [
        ("setAccessible", "(Z)V"),
        ("canAccess", "(Ljava/lang/Object;)Z"),
    ] {
        accessibleobject_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/lang/reflect/AccessibleObject".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: accessibleobject_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });

    let mut modifier_methods = HashMap::new();
    for (name, desc) in [
        ("isPublic", "(I)Z"),
        ("isPrivate", "(I)Z"),
        ("isProtected", "(I)Z"),
        ("isStatic", "(I)Z"),
        ("isFinal", "(I)Z"),
        ("isSynchronized", "(I)Z"),
        ("toString", "(I)Ljava/lang/String;"),
    ] {
        modifier_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/lang/reflect/Modifier".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: modifier_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![],
        interfaces: vec![],
    });
}