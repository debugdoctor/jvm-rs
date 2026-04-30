//! Differential JIT tests: compile Java with `javac`, run the same `main`
//! twice — once with the JIT effectively disabled (threshold = u32::MAX) and
//! once with it forced on the first invocation (threshold = 1) — and assert
//! the printed output matches.
//!
//! These tests exist to *drive* JIT correctness. A test failing here means
//! the JIT produced different observable behavior than the interpreter for
//! a real Java program.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use jvm_rs::launcher::{self, LaunchOptions};
use jvm_rs::vm::Vm;

fn temp_dir(test_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("jvm-rs-jit-{test_name}-{nanos}"));
    fs::create_dir_all(&path).unwrap();
    path
}

fn compile_java(test_name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = temp_dir(test_name);
    for (name, source) in files {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, source).unwrap();
    }
    let source_files: Vec<PathBuf> = files.iter().map(|(name, _)| root.join(name)).collect();
    let mut cmd = Command::new("javac");
    cmd.arg("--release").arg("8").arg("-d").arg(&root);
    for source in &source_files {
        cmd.arg(source);
    }
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "javac failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    root
}

struct RunResult {
    output: Vec<String>,
    jit_executions: u64,
}

fn run_with_jit_threshold(root: &PathBuf, main_class: &str, threshold: u32) -> RunResult {
    let options = LaunchOptions::new(root, main_class, vec![]);
    let mut vm = Vm::new().expect("failed to create VM");
    vm.set_class_path(options.class_path.clone());
    vm.set_jit_thresholds(threshold, threshold);
    let source = launcher::resolve_class_path(&options.class_path, main_class).unwrap();
    let method = launcher::load_main_method(&source, main_class, &[], &mut vm).unwrap();
    let _ = vm.execute(method).unwrap();
    RunResult {
        output: vm.take_output(),
        jit_executions: vm.jit_executions(),
    }
}

/// Run `main` twice — once with JIT disabled, once with JIT forced on the
/// first invocation — and assert outputs match.
fn assert_jit_matches_interpreter(test_name: &str, files: &[(&str, &str)]) {
    let root = compile_java(test_name, files);
    let main_file = files[0].0;
    let main_class = main_file.trim_end_matches(".java").replace('/', ".");

    let interp = run_with_jit_threshold(&root, &main_class, u32::MAX);
    let jit = run_with_jit_threshold(&root, &main_class, 1);

    assert_eq!(
        interp.jit_executions, 0,
        "interpreter run should not have invoked JIT (got {})",
        interp.jit_executions
    );
    assert!(
        jit.jit_executions > 0,
        "JIT-forced run should have executed at least one JIT entry; got 0. \
         Did `should_compile` reject the method (e.g., code.len() > 200), or \
         did compilation silently fall back to the interpreter?"
    );
    assert_eq!(
        jit.output, interp.output,
        "JIT output diverged from interpreter\nJIT:         {:?}\nInterpreter: {:?}",
        jit.output, interp.output
    );
}

// ---- Tier 1: pure arithmetic. Exercises iconst/iadd/imul/ireturn/print. ----

#[test]
fn jit_pure_int_arithmetic_matches_interpreter() {
    assert_jit_matches_interpreter(
        "pure_int_arithmetic",
        &[(
            "demo/Main.java",
            r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int a = 7;
        int b = 6;
        int product = a * b;
        int sum = a + b + 100;
        System.out.println(product);
        System.out.println(sum);
    }
}
"#,
        )],
    );
}

#[test]
fn jit_long_arithmetic_matches_interpreter() {
    assert_jit_matches_interpreter(
        "long_arithmetic",
        &[(
            "demo/Main.java",
            r#"
package demo;
public class Main {
    public static void main(String[] args) {
        long x = 1234567890123L;
        long y = 9876543210L;
        long sum = x + y;
        long product = x * 3L;
        System.out.println(sum);
        System.out.println(product);
    }
}
"#,
        )],
    );
}

// ---- Tier 2: invokestatic. Exercises method call ABI from JIT. ----

#[test]
fn jit_invokestatic_matches_interpreter() {
    assert_jit_matches_interpreter(
        "invokestatic",
        &[(
            "demo/Main.java",
            r#"
package demo;
public class Main {
    static int square(int n) { return n * n; }
    public static void main(String[] args) {
        int r = square(13);
        System.out.println(r);
    }
}
"#,
        )],
    );
}

// ---- Tier 3: getstatic + invokevirtual via System.out. -------
// Note: every println already exercises getstatic(System.out) +
// invokevirtual(println). Tier 1 covers that incidentally; this case
// makes the dependency explicit by performing a getstatic of a user
// field rather than just System.out.

#[test]
fn jit_user_static_field_matches_interpreter() {
    assert_jit_matches_interpreter(
        "user_static_field",
        &[(
            "demo/Main.java",
            r#"
package demo;
public class Main {
    static int CONST = 42;
    public static void main(String[] args) {
        System.out.println(CONST + 1);
    }
}
"#,
        )],
    );
}

// ---- Tier 4: new + putfield + getfield. -------

#[test]
fn jit_new_object_with_fields_matches_interpreter() {
    assert_jit_matches_interpreter(
        "new_object",
        &[(
            "demo/Main.java",
            r#"
package demo;
public class Main {
    int x;
    int y;
    public static void main(String[] args) {
        Main m = new Main();
        m.x = 10;
        m.y = 32;
        System.out.println(m.x + m.y);
    }
}
"#,
        )],
    );
}

// ---- Tier 5: invokevirtual on user-defined class. -------

#[test]
fn jit_invokevirtual_matches_interpreter() {
    assert_jit_matches_interpreter(
        "invokevirtual",
        &[(
            "demo/Main.java",
            r#"
package demo;
public class Main {
    int factor;
    Main(int f) { this.factor = f; }
    int multiply(int n) { return n * factor; }
    public static void main(String[] args) {
        Main m = new Main(7);
        System.out.println(m.multiply(6));
    }
}
"#,
        )],
    );
}

// ---- Tier 6: synchronized block (monitorenter/monitorexit). -------

#[test]
fn jit_synchronized_matches_interpreter() {
    assert_jit_matches_interpreter(
        "synchronized",
        &[(
            "demo/Main.java",
            r#"
package demo;
public class Main {
    public static void main(String[] args) {
        Object lock = new Object();
        int total = 0;
        synchronized (lock) {
            total = 1 + 2;
        }
        System.out.println(total);
    }
}
"#,
        )],
    );
}

// ---- Tier 7: athrow / try-catch. -------

#[test]
fn jit_athrow_matches_interpreter() {
    assert_jit_matches_interpreter(
        "athrow",
        &[(
            "demo/Main.java",
            r#"
package demo;
public class Main {
    public static void main(String[] args) {
        try {
            throw new RuntimeException("boom");
        } catch (RuntimeException e) {
            System.out.println("caught: " + e.getMessage());
        }
    }
}
"#,
        )],
    );
}
