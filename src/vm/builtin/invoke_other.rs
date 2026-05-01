use crate::vm::types::stub_return_value;
use crate::vm::{Reference, Value, Vm, VmError};

pub(super) fn invoke_other(
    vm: &mut Vm,
    class_name: &str,
    method_name: &str,
    descriptor: &str,
    args: &[Value],
) -> Result<Option<Value>, VmError> {
    match (class_name, method_name, descriptor) {
        ("java/lang/System", "currentTimeMillis", "()J") => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            Ok(Some(Value::Long(now)))
        }
        ("java/lang/System", "nanoTime", "()J") => {
            use std::time::Instant;
            static BASELINE: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
            let base = BASELINE.get_or_init(Instant::now);
            Ok(Some(Value::Long(base.elapsed().as_nanos() as i64)))
        }
        ("java/lang/System", "arraycopy", "(Ljava/lang/Object;ILjava/lang/Object;II)V") => {
            let src = args[0].as_reference()?;
            let src_pos = args[1].as_int()?;
            let dst = args[2].as_reference()?;
            let dst_pos = args[3].as_int()?;
            let length = args[4].as_int()?;
            crate::vm::builtin::helpers::arraycopy(vm, src, src_pos, dst, dst_pos, length)?;
            Ok(None)
        }
        ("java/lang/System", "exit", "(I)V") => {
            let code = args[0].as_int()?;
            std::process::exit(code);
        }
        ("java/lang/System", "getProperty", "(Ljava/lang/String;)Ljava/lang/String;") => {
            let key =
                crate::vm::builtin::helpers::stringify_reference(vm, args[0].as_reference()?)?;
            let value = match key.as_str() {
                "line.separator" => Some("\n".to_string()),
                "file.separator" => Some(std::path::MAIN_SEPARATOR.to_string()),
                "path.separator" => Some(if cfg!(windows) {
                    ";".to_string()
                } else {
                    ":".to_string()
                }),
                "java.version" => Some("21".to_string()),
                "java.specification.version" => Some("21".to_string()),
                "os.name" => Some(std::env::consts::OS.to_string()),
                "os.arch" => Some(std::env::consts::ARCH.to_string()),
                other => std::env::var(other).ok(),
            };
            match value {
                Some(v) => Ok(Some(vm.new_string(v))),
                None => Ok(Some(Value::Reference(Reference::Null))),
            }
        }
        ("java/lang/System", "lineSeparator", "()Ljava/lang/String;") => {
            Ok(Some(vm.new_string("\n".to_string())))
        }
        ("java/lang/System", "identityHashCode", "(Ljava/lang/Object;)I") => {
            let r = args[0].as_reference()?;
            let hash = match r {
                Reference::Null => 0,
                Reference::Heap(i) => i as i32,
            };
            Ok(Some(Value::Int(hash)))
        }
        ("jdk/internal/reflect/Reflection", "getCallerClass", "()Ljava/lang/Class;") => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("jdk/internal/reflect/Reflection", _, _) => {
            let _ = stub_return_value(descriptor);
            Ok(None)
        }
        ("jdk/internal/misc/Unsafe", "registerNatives", "()V") => Ok(None),
        ("jdk/internal/misc/Unsafe", "getUnsafe", "()Ljdk/internal/misc/Unsafe;") => Ok(Some(
            vm.get_static_field("jdk/internal/misc/Unsafe", "theUnsafe")?,
        )),
        ("jdk/internal/misc/Unsafe", "arrayBaseOffset", "(Ljava/lang/Class;)I") => {
            Ok(Some(Value::Int(0)))
        }
        ("jdk/internal/misc/Unsafe", "arrayIndexScale", "(Ljava/lang/Class;)I") => {
            Ok(Some(Value::Int(1)))
        }
        ("jdk/internal/misc/Unsafe", "addressSize", "()I") => Ok(Some(Value::Int(8))),
        ("jdk/internal/misc/Unsafe", "isBigEndian", "()Z") => {
            Ok(Some(Value::Int(i32::from(cfg!(target_endian = "big")))))
        }
        ("jdk/internal/misc/Unsafe", "pageSize", "()I") => Ok(Some(Value::Int(4096))),
        ("jdk/internal/misc/Unsafe", "objectFieldOffset", _)
        | ("jdk/internal/misc/Unsafe", "staticFieldOffset", _) => Ok(Some(Value::Long(0))),
        ("jdk/internal/misc/Unsafe", "staticFieldBase", _) => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("jdk/internal/misc/Unsafe", "storeFence", "()V")
        | ("jdk/internal/misc/Unsafe", "loadFence", "()V")
        | ("jdk/internal/misc/Unsafe", "fullFence", "()V") => Ok(None),
        (
            "jdk/internal/misc/Unsafe",
            "compareAndSetInt"
            | "compareAndSetLong"
            | "compareAndSetReference"
            | "compareAndSetObject",
            _,
        ) => Ok(Some(Value::Int(1))),
        ("jdk/internal/misc/Unsafe", "getReferenceVolatile", _) => {
            Ok(Some(Value::Reference(Reference::Null)))
        }
        ("jdk/internal/misc/Unsafe", "putReferenceVolatile", _)
        | ("jdk/internal/misc/Unsafe", "putIntVolatile", _) => Ok(None),
        ("jdk/internal/misc/Unsafe", "getIntVolatile", _) => Ok(Some(Value::Int(0))),
        ("jdk/internal/misc/Unsafe", _, _) => {
            let _ = stub_return_value(descriptor);
            Ok(None)
        }
        ("jdk/internal/misc/CDS", "isDumpingClassList0", "()Z") => Ok(Some(Value::Int(0))),
        ("jdk/internal/misc/CDS", "isDumpingArchive0", "()Z") => Ok(Some(Value::Int(0))),
        ("jdk/internal/misc/CDS", "isSharingEnabled0", "()Z") => Ok(Some(Value::Int(0))),
        ("jdk/internal/misc/CDS", _, _) => {
            let _ = stub_return_value(descriptor);
            Ok(None)
        }
        _ => Err(VmError::UnhandledException {
            class_name: "".to_string(),
        }),
    }
}
