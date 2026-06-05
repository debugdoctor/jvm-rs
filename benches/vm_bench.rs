use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use criterion::{criterion_group, criterion_main, Criterion};

use jvm_rs::launcher::{load_main_method, resolve_class_path, LaunchOptions};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("jvm-rs-bench-{label}-{nanos}"));
    fs::create_dir_all(&path).unwrap();
    path
}

/// Compile Java source with javac and return the output directory.
fn compile_java(label: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = temp_dir(label);
    for (name, src) in files {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, src).unwrap();
    }
    let source_files: Vec<PathBuf> = files.iter().map(|(n, _)| root.join(n)).collect();
    let status = Command::new("javac")
        .arg("--release")
        .arg("8")
        .arg("-d")
        .arg(&root)
        .args(&source_files)
        .status()
        .expect("javac not found");
    assert!(status.success(), "javac compilation failed for {label}");
    root
}

// ---------------------------------------------------------------------------
// Pre-compiled class directories (compiled once per bench run via OnceLock)
// ---------------------------------------------------------------------------

static HELLO_WORLD_DIR: OnceLock<PathBuf> = OnceLock::new();
static NUMERIC_LOOP_DIR: OnceLock<PathBuf> = OnceLock::new();
static ALLOC_LOOP_DIR: OnceLock<PathBuf> = OnceLock::new();
static STRING_CONCAT_DIR: OnceLock<PathBuf> = OnceLock::new();

fn hello_world_dir() -> &'static PathBuf {
    HELLO_WORLD_DIR.get_or_init(|| {
        compile_java(
            "hello",
            &[(
                "Hello.java",
                r#"
public class Hello {
    public static void main(String[] args) {
        System.out.println("Hello, World!");
    }
}
"#,
            )],
        )
    })
}

fn numeric_loop_dir() -> &'static PathBuf {
    NUMERIC_LOOP_DIR.get_or_init(|| {
        compile_java(
            "numloop",
            &[(
                "NumLoop.java",
                r#"
public class NumLoop {
    public static void main(String[] args) {
        long sum = 0;
        for (int i = 0; i < 1_000_000; i++) {
            sum += i;
        }
        // prevent dead-code elimination
        if (sum == 0) System.out.println("zero");
    }
}
"#,
            )],
        )
    })
}

fn alloc_loop_dir() -> &'static PathBuf {
    ALLOC_LOOP_DIR.get_or_init(|| {
        compile_java(
            "allocloop",
            &[(
                "AllocLoop.java",
                r#"
public class AllocLoop {
    public static void main(String[] args) {
        Object last = null;
        for (int i = 0; i < 100_000; i++) {
            last = new Object();
        }
        if (last == null) System.out.println("null");
    }
}
"#,
            )],
        )
    })
}

fn string_concat_dir() -> &'static PathBuf {
    STRING_CONCAT_DIR.get_or_init(|| {
        compile_java(
            "strconcat",
            &[(
                "StrConcat.java",
                r#"
public class StrConcat {
    public static void main(String[] args) {
        String s = "";
        for (int i = 0; i < 1_000; i++) {
            s = s + "x";
        }
        if (s.length() == 0) System.out.println("empty");
    }
}
"#,
            )],
        )
    })
}

// ---------------------------------------------------------------------------
// Run helper: creates a fresh VM + loads + executes for each iteration.
// ---------------------------------------------------------------------------

fn run_main(class_dir: &PathBuf, main_class: &str) {
    let opts = LaunchOptions::new(class_dir, main_class, vec![]);
    let mut vm = jvm_rs::vm::Vm::new().expect("Vm::new failed");
    vm.set_class_path(opts.class_path.clone());
    let source = resolve_class_path(&opts.class_path, main_class)
        .unwrap_or_else(|| panic!("resolve_class_path({main_class}): class not found"));
    let method = load_main_method(&source, main_class, &[], &mut vm)
        .unwrap_or_else(|e| panic!("load_main_method({main_class}): {e:?}"));
    vm.execute(method).unwrap_or_else(|e| panic!("execute({main_class}): {e:?}"));
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_hello_world(c: &mut Criterion) {
    let dir = hello_world_dir();
    c.bench_function("hello_world_cold_start", |b| {
        b.iter(|| run_main(dir, "Hello"));
    });
}

fn bench_numeric_loop(c: &mut Criterion) {
    let dir = numeric_loop_dir();
    c.bench_function("numeric_loop_1m_iters", |b| {
        b.iter(|| run_main(dir, "NumLoop"));
    });
}

fn bench_alloc_loop(c: &mut Criterion) {
    let dir = alloc_loop_dir();
    c.bench_function("object_alloc_loop_100k", |b| {
        b.iter(|| run_main(dir, "AllocLoop"));
    });
}

fn bench_string_concat(c: &mut Criterion) {
    let dir = string_concat_dir();
    c.bench_function("string_concat_1k", |b| {
        b.iter(|| run_main(dir, "StrConcat"));
    });
}

criterion_group!(
    benches,
    bench_hello_world,
    bench_numeric_loop,
    bench_alloc_loop,
    bench_string_concat
);
criterion_main!(benches);
