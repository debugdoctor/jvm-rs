use std::collections::HashMap;

use crate::vm::{ClassMethod, RuntimeClass, Value, Vm};

pub(super) fn bootstrap_java_util_regex(vm: &mut Vm) {
    let mut pattern_methods = HashMap::new();
    for (name, desc) in [
        ("compile", "(Ljava/lang/String;)Ljava/util/regex/Pattern;"),
        ("compile", "(Ljava/lang/String;I)Ljava/util/regex/Pattern;"),
        ("matches", "(Ljava/lang/String;Ljava/lang/CharSequence;)Z"),
        ("pattern", "()Ljava/lang/String;"),
        ("matcher", "(Ljava/lang/CharSequence;)Ljava/util/regex/Matcher;"),
        ("split", "(Ljava/lang/CharSequence;)[Ljava/lang/String;"),
        ("split", "(Ljava/lang/CharSequence;I)[Ljava/lang/String;"),
    ] {
        pattern_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/regex/Pattern".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: pattern_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![
            ("__regex".to_string(), "Ljava/lang/String;".to_string()),
            ("__flags".to_string(), "I".to_string()),
        ],
        interfaces: vec![],
    });

    let mut matcher_methods = HashMap::new();
    for (name, desc) in [
        ("<init>", "(Ljava/util/regex/Pattern;Ljava/lang/CharSequence;)V"),
        ("matches", "()Z"),
        ("find", "()Z"),
        ("find", "(I)Z"),
        ("lookingAt", "()Z"),
        ("reset", "()Ljava/util/regex/Matcher;"),
        ("reset", "(Ljava/lang/CharSequence;)Ljava/util/regex/Matcher;"),
        ("group", "(I)Ljava/lang/String;"),
        ("group", "()Ljava/lang/String;"),
        ("groupCount", "()I"),
        ("start", "()I"),
        ("start", "(I)I"),
        ("end", "()I"),
        ("end", "(I)I"),
        ("replaceAll", "(Ljava/lang/String;)Ljava/lang/String;"),
        ("replaceFirst", "(Ljava/lang/String;)Ljava/lang/String;"),
    ] {
        matcher_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/util/regex/Matcher".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: matcher_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![
            ("__pattern".to_string(), "Ljava/util/regex/Pattern;".to_string()),
            ("__input".to_string(), "Ljava/lang/CharSequence;".to_string()),
            ("__match_start".to_string(), "I".to_string()),
            ("__match_end".to_string(), "I".to_string()),
            ("__last_match_start".to_string(), "I".to_string()),
            ("__group_count".to_string(), "I".to_string()),
        ],
        interfaces: vec![],
    });
}