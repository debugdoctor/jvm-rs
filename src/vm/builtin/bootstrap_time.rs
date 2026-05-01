use std::collections::HashMap;

use crate::vm::{ClassMethod, RuntimeClass, Value, Vm};

pub(super) fn bootstrap_java_time(vm: &mut Vm) {
    let mut instant_methods = HashMap::new();
    for (name, desc) in [
        ("now", "()Ljava/time/Instant;"),
        ("ofEpochSecond", "(J)Ljava/time/Instant;"),
        ("ofEpochSecond", "(JJ)Ljava/time/Instant;"),
        ("ofEpochMilli", "(J)Ljava/time/Instant;"),
        ("getEpochSecond", "()J"),
        ("getNano", "()I"),
        ("toEpochMilli", "()J"),
        ("isAfter", "(Ljava/time/Instant;)Z"),
        ("isBefore", "(Ljava/time/Instant;)Z"),
        ("plusSeconds", "(J)Ljava/time/Instant;"),
        ("plusMillis", "(J)Ljava/time/Instant;"),
        ("plusNanos", "(J)Ljava/time/Instant;"),
        ("minusSeconds", "(J)Ljava/time/Instant;"),
        ("minusMillis", "(J)Ljava/time/Instant;"),
        ("compareTo", "(Ljava/time/Instant;)I"),
        ("equals", "(Ljava/lang/Object;)Z"),
        ("hashCode", "()I"),
        ("toString", "()Ljava/lang/String;"),
    ] {
        instant_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/time/Instant".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: instant_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![
            ("__epoch_second".to_string(), "J".to_string()),
            ("__nano".to_string(), "I".to_string()),
        ],
        interfaces: vec![],
    });

    let mut duration_methods = HashMap::new();
    for (name, desc) in [
        ("ofSeconds", "(J)Ljava/time/Duration;"),
        ("ofMillis", "(J)Ljava/time/Duration;"),
        ("ofNanos", "(J)Ljava/time/Duration;"),
        ("getSeconds", "()J"),
        ("getNano", "()I"),
        ("toMillis", "()J"),
        ("toMicros", "()J"),
        ("isNegative", "()Z"),
        ("isZero", "()Z"),
        ("plus", "(Ljava/time/Duration;)Ljava/time/Duration;"),
        ("plusSeconds", "(J)Ljava/time/Duration;"),
        ("minus", "(Ljava/time/Duration;)Ljava/time/Duration;"),
        ("compareTo", "(Ljava/time/Duration;)I"),
        ("equals", "(Ljava/lang/Object;)Z"),
        ("hashCode", "()I"),
        ("toString", "()Ljava/lang/String;"),
    ] {
        duration_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/time/Duration".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: duration_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![
            ("__seconds".to_string(), "J".to_string()),
            ("__nano".to_string(), "I".to_string()),
        ],
        interfaces: vec![],
    });

    let mut localdate_methods = HashMap::new();
    for (name, desc) in [
        ("now", "()Ljava/time/LocalDate;"),
        ("of", "(III)Ljava/time/LocalDate;"),
        ("getYear", "()I"),
        ("getMonthValue", "()I"),
        ("getDayOfMonth", "()I"),
        ("getDayOfWeek", "()Ljava/time/DayOfWeek;"),
        ("lengthOfMonth", "()I"),
        ("lengthOfYear", "()I"),
        ("isLeapYear", "()Z"),
        ("plusDays", "(J)Ljava/time/LocalDate;"),
        ("minusDays", "(J)Ljava/time/LocalDate;"),
        ("plusYears", "(J)Ljava/time/LocalDate;"),
        ("compareTo", "(Ljava/time/LocalDate;)I"),
        ("equals", "(Ljava/lang/Object;)Z"),
        ("hashCode", "()I"),
        ("toString", "()Ljava/lang/String;"),
    ] {
        localdate_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/time/LocalDate".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: localdate_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![
            ("__year".to_string(), "I".to_string()),
            ("__month".to_string(), "I".to_string()),
            ("__day".to_string(), "I".to_string()),
        ],
        interfaces: vec![],
    });

    let mut localdatetime_methods = HashMap::new();
    for (name, desc) in [
        ("now", "()Ljava/time/LocalDateTime;"),
        ("of", "(IIIIIII)Ljava/time/LocalDateTime;"),
        ("getYear", "()I"),
        ("getMonthValue", "()I"),
        ("getDayOfMonth", "()I"),
        ("getHour", "()I"),
        ("getMinute", "()I"),
        ("getSecond", "()I"),
        ("getNano", "()I"),
        ("toLocalDate", "()Ljava/time/LocalDate;"),
        ("toLocalTime", "()Ljava/time/LocalTime;"),
        ("plusSeconds", "(J)Ljava/time/LocalDateTime;"),
        ("plusMinutes", "(J)Ljava/time/LocalDateTime;"),
        ("plusHours", "(J)Ljava/time/LocalDateTime;"),
        ("plusDays", "(J)Ljava/time/LocalDateTime;"),
        ("compareTo", "(Ljava/time/LocalDateTime;)I"),
        ("equals", "(Ljava/lang/Object;)Z"),
        ("hashCode", "()I"),
        ("toString", "()Ljava/lang/String;"),
    ] {
        localdatetime_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/time/LocalDateTime".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: localdatetime_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![
            ("__year".to_string(), "I".to_string()),
            ("__month".to_string(), "I".to_string()),
            ("__day".to_string(), "I".to_string()),
            ("__hour".to_string(), "I".to_string()),
            ("__minute".to_string(), "I".to_string()),
            ("__second".to_string(), "I".to_string()),
            ("__nano".to_string(), "I".to_string()),
        ],
        interfaces: vec![],
    });

    let mut localtime_methods = HashMap::new();
    for (name, desc) in [
        ("now", "()Ljava/time/LocalTime;"),
        ("of", "(II)Ljava/time/LocalTime;"),
        ("of", "(III)Ljava/time/LocalTime;"),
        ("getHour", "()I"),
        ("getMinute", "()I"),
        ("getSecond", "()I"),
        ("getNano", "()I"),
        ("toSecondOfDay", "()I"),
        ("plusHours", "(J)Ljava/time/LocalTime;"),
        ("plusMinutes", "(J)Ljava/time/LocalTime;"),
        ("compareTo", "(Ljava/time/LocalTime;)I"),
        ("equals", "(Ljava/lang/Object;)Z"),
        ("hashCode", "()I"),
        ("toString", "()Ljava/lang/String;"),
    ] {
        localtime_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/time/LocalTime".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: localtime_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![
            ("__hour".to_string(), "I".to_string()),
            ("__minute".to_string(), "I".to_string()),
            ("__second".to_string(), "I".to_string()),
            ("__nano".to_string(), "I".to_string()),
        ],
        interfaces: vec![],
    });

    let mut dayofweek_methods = HashMap::new();
    for (name, desc) in [("getValue", "()I"), ("toString", "()Ljava/lang/String;")] {
        dayofweek_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    let mut dayofweek_static = HashMap::new();
    for (name, val) in [
        ("MONDAY", 1),
        ("TUESDAY", 2),
        ("WEDNESDAY", 3),
        ("THURSDAY", 4),
        ("FRIDAY", 5),
        ("SATURDAY", 6),
        ("SUNDAY", 7),
    ] {
        let mut fields = std::collections::HashMap::new();
        fields.insert("__value".to_string(), Value::Int(val));
        let r = vm
            .heap
            .lock()
            .unwrap()
            .allocate(crate::vm::HeapValue::Object {
                class_name: "java/time/DayOfWeek".to_string(),
                fields,
            });
        dayofweek_static.insert(name.to_string(), Value::Reference(r));
    }
    vm.register_class(RuntimeClass {
        name: "java/time/DayOfWeek".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: dayofweek_methods,
        static_fields: dayofweek_static,
        instance_fields: vec![("__value".to_string(), "I".to_string())],
        interfaces: vec![],
    });

    let mut zoneddatetime_methods = HashMap::new();
    for (name, desc) in [
        ("now", "()Ljava/time/ZonedDateTime;"),
        ("now", "(Ljava/time/ZoneId;)Ljava/time/ZonedDateTime;"),
        (
            "of",
            "(Ljava/time/LocalDateTime;Ljava/time/ZoneId;)Ljava/time/ZonedDateTime;",
        ),
        ("getYear", "()I"),
        ("getMonthValue", "()I"),
        ("getDayOfMonth", "()I"),
        ("getHour", "()I"),
        ("getMinute", "()I"),
        ("getSecond", "()I"),
        ("getNano", "()I"),
        ("getZone", "()Ljava/time/ZoneId;"),
        ("toLocalDateTime", "()Ljava/time/LocalDateTime;"),
        ("toInstant", "()Ljava/time/Instant;"),
        ("toString", "()Ljava/lang/String;"),
    ] {
        zoneddatetime_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/time/ZonedDateTime".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: zoneddatetime_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![
            ("__year".to_string(), "I".to_string()),
            ("__month".to_string(), "I".to_string()),
            ("__day".to_string(), "I".to_string()),
            ("__hour".to_string(), "I".to_string()),
            ("__minute".to_string(), "I".to_string()),
            ("__second".to_string(), "I".to_string()),
            ("__nano".to_string(), "I".to_string()),
            ("__zone_id".to_string(), "Ljava/lang/String;".to_string()),
        ],
        interfaces: vec![],
    });

    let mut zoneid_methods = HashMap::new();
    for (name, desc) in [
        ("systemDefault", "()Ljava/time/ZoneId;"),
        ("of", "(Ljava/lang/String;)Ljava/time/ZoneId;"),
        ("getId", "()Ljava/lang/String;"),
        ("toString", "()Ljava/lang/String;"),
    ] {
        zoneid_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    let mut zoneid_static = HashMap::new();
    let utc_zone = {
        let mut fields = std::collections::HashMap::new();
        fields.insert("__id".to_string(), vm.new_string("UTC".to_string()));
        let r = vm
            .heap
            .lock()
            .unwrap()
            .allocate(crate::vm::HeapValue::Object {
                class_name: "java/time/ZoneId".to_string(),
                fields,
            });
        r
    };
    zoneid_static.insert("UTC".to_string(), Value::Reference(utc_zone));
    vm.register_class(RuntimeClass {
        name: "java/time/ZoneId".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: zoneid_methods,
        static_fields: zoneid_static,
        instance_fields: vec![("__id".to_string(), "Ljava/lang/String;".to_string())],
        interfaces: vec![],
    });

    let mut clock_methods = HashMap::new();
    for (name, desc) in [
        ("systemUTC", "()Ljava/time/Clock;"),
        ("systemDefaultZone", "()Ljava/time/Clock;"),
        ("millis", "()J"),
        ("instant", "()Ljava/time/Instant;"),
        ("getZone", "()Ljava/time/ZoneId;"),
    ] {
        clock_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
    vm.register_class(RuntimeClass {
        name: "java/time/Clock".to_string(),
        super_class: Some("java/lang/Object".to_string()),
        methods: clock_methods,
        static_fields: HashMap::new(),
        instance_fields: vec![
            ("__millis".to_string(), "J".to_string()),
            ("__zone_id".to_string(), "Ljava/lang/String;".to_string()),
        ],
        interfaces: vec![],
    });

    let mut zoneddatetime_methods = HashMap::new();
    for (name, desc) in [("now", "()Ljava/time/ZonedDateTime;")] {
        zoneddatetime_methods.insert((name.to_string(), desc.to_string()), ClassMethod::Native);
    }
}
