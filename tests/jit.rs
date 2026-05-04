use std::collections::HashMap;
use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use jvm_rs::launcher::{self, LaunchOptions};
use jvm_rs::vm::jit::runtime::DeoptReason;
use jvm_rs::vm::{ExecutionResult, FieldRef, Method, MethodRef, RuntimeClass, Value, Vm, VmError};

const JAVAC_TIMEOUT: Duration = Duration::from_secs(10);

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(test_name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("jvm-rs-jit-{test_name}-{nanos}"));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

impl AsRef<Path> for TestDir {
    fn as_ref(&self) -> &Path {
        self.path()
    }
}

impl Deref for TestDir {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        self.path()
    }
}

fn compile_java(test_name: &str, files: &[(&str, &str)]) -> TestDir {
    let root = TestDir::new(test_name);
    for (name, source) in files {
        let path = root.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, source).unwrap();
    }
    let source_files: Vec<PathBuf> = files
        .iter()
        .map(|(name, _)| root.path().join(name))
        .collect();
    let mut cmd = Command::new("javac");
    cmd.arg("--release")
        .arg("8")
        .arg("-Xlint:-options")
        .arg("-g")
        .arg("-d")
        .arg(root.path());
    for source in &source_files {
        cmd.arg(source);
    }
    let mut child = cmd.spawn().unwrap();
    let started = Instant::now();
    let output = loop {
        if let Some(_status) = child.try_wait().unwrap() {
            break child.wait_with_output().unwrap();
        }
        if started.elapsed() >= JAVAC_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "javac timed out for {} after {:?}",
                test_name, JAVAC_TIMEOUT
            );
        }
        thread::sleep(Duration::from_millis(10));
    };
    assert!(
        output.status.success(),
        "javac failed for {}: {}\n{}",
        test_name,
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    root
}

struct RunResult {
    output: Vec<String>,
    jit_executions: u64,
}

fn run_with_jit_threshold(root: &Path, main_class: &str, threshold: u32) -> RunResult {
    run_with_jit_thresholds(root, main_class, threshold, threshold)
}

fn run_with_jit_thresholds(
    root: &Path,
    main_class: &str,
    invocation_threshold: u32,
    backedge_threshold: u32,
) -> RunResult {
    let options = LaunchOptions::new(root, main_class, vec![]);
    let mut vm = Vm::new().expect("failed to create VM");
    vm.set_class_path(options.class_path.clone());
    vm.set_jit_thresholds(invocation_threshold, backedge_threshold);
    let source = launcher::resolve_class_path(&options.class_path, main_class).unwrap();
    let method = launcher::load_main_method(&source, main_class, &[], &mut vm).unwrap();
    let _ = vm.execute(method).unwrap();
    RunResult {
        output: vm.take_output(),
        jit_executions: vm.jit_executions(),
    }
}

fn assert_jit_matches_interpreter(test_name: &str, files: &[(&str, &str)]) {
    let root = compile_java(test_name, files);
    let main_file = files[0].0;
    let main_class = main_file.trim_end_matches(".java").replace('/', ".");

    let interp = run_with_jit_threshold(root.path(), &main_class, u32::MAX);
    let jit = run_with_jit_threshold(root.path(), &main_class, 1);

    assert_eq!(
        interp.jit_executions, 0,
        "interpreter run should not have invoked JIT (got {})",
        interp.jit_executions
    );
    assert!(
        jit.jit_executions > 0,
        "JIT-forced run executed 0 JIT entries. Method may be too large or rejected by should_compile."
    );
    assert_eq!(
        jit.output, interp.output,
        "JIT output diverged from interpreter\nJIT:         {:?}\nInterpreter: {:?}",
        jit.output, interp.output
    );
}

mod integration_suite {
    use super::*;

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

    #[test]
    fn jit_osr_enters_hot_loop_from_interpreter() {
        let root = compile_java(
            "osr_hot_loop",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static int hot(int n) {
        int sum = 0;
        for (int i = 0; i < n; i++) {
            sum += (i * 3) - 1;
        }
        return sum;
    }
    public static void main(String[] args) {
        System.out.println(hot(25));
    }
}
"#,
            )],
        );

        let interp = run_with_jit_thresholds(root.path(), "demo.Main", u32::MAX, u32::MAX);
        let osr = run_with_jit_thresholds(root.path(), "demo.Main", u32::MAX, 1);

        assert_eq!(osr.output, interp.output);
        assert!(
            osr.jit_executions > 0,
            "expected OSR to enter the JIT tier from the hot loop"
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

    #[test]
    fn jit_invokestatic_pure_callee_executes_machine_code() {
        let root = compile_java(
            "invokestatic_pure_callee",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static int mix(int a, int b) {
        int product = a * b;
        return product + a + 5;
    }
    static long widen(long base, int scale) {
        return base * 3L + scale;
    }
    public static void main(String[] args) {
        System.out.println(mix(12, 4));
        System.out.println(widen(10000000000L, 7));
    }
}
"#,
            )],
        );
        let interp = run_with_jit_threshold(&root, "demo.Main", u32::MAX);
        let jit = run_with_jit_threshold(&root, "demo.Main", 1);

        assert_eq!(jit.output, interp.output);
        assert!(
            jit.jit_executions >= 3,
            "expected top-level JIT/deopt plus two pure static callees to reach the JIT tier, got {}",
            jit.jit_executions
        );
    }

    #[test]
    fn jit_compiled_method_can_invoke_static_helper() {
        let root = compile_java(
            "compiled_invokestatic_helper",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static int square(int n) {
        return n * n;
    }
    static int outer(int n) {
        return square(n) + 3;
    }
    public static void main(String[] args) {
        System.out.println(outer(9));
    }
}
"#,
            )],
        );
        let interp = run_with_jit_threshold(&root, "demo.Main", u32::MAX);
        let jit = run_with_jit_threshold(&root, "demo.Main", 1);

        assert_eq!(jit.output, interp.output);
        assert!(
            jit.jit_executions >= 2,
            "expected top-level JIT/deopt plus compiled outer() to reach JIT, got {}",
            jit.jit_executions
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

    #[test]
    fn jit_static_field_helpers_execute_machine_code() {
        let method = Method::new(
            [
                0xb2, 0x00, 0x01, // getstatic #1
                0x08, // iconst_5
                0x60, // iadd
                0xb3, 0x00, 0x01, // putstatic #1
                0xb2, 0x00, 0x01, // getstatic #1
                0xac, // ireturn
            ],
            0,
            2,
        )
        .with_metadata("demo/Main", "runFields", "()I", 0x0008)
        .with_field_refs(vec![
            None,
            Some(FieldRef {
                class_name: "demo/Fields".to_string(),
                field_name: "value".to_string(),
                descriptor: "I".to_string(),
            }),
        ]);

        let mut vm = Vm::new().expect("failed to create VM");
        vm.register_class(RuntimeClass {
            name: "demo/Fields".to_string(),
            super_class: Some("java/lang/Object".to_string()),
            methods: HashMap::new(),
            static_fields: HashMap::from([("value".to_string(), Value::Int(37))]),
            instance_fields: vec![],
            interfaces: vec![],
        });
        vm.set_jit_thresholds(1, 1);

        let result = vm.execute(method).unwrap();

        assert_eq!(result, ExecutionResult::Value(Value::Int(42)));
        assert!(
            vm.jit_executions() >= 1,
            "expected synthetic getstatic/putstatic method to execute through JIT"
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

    #[test]
    fn jit_instance_field_helpers_support_descriptor_types() {
        let root = compile_java(
            "instance_field_descriptor_types",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static class Box {
        int i;
        long l;
        float f;
        double d;
        Object o;
        int[] a;
    }
    static int run(Box box, Object marker, int[] array) {
        box.i = box.i + 5;
        box.l = box.l + 7L;
        box.f = box.f + 1.5f;
        box.d = box.d + 2.25d;
        box.o = marker;
        box.a = array;
        Object ignoredObject = box.o;
        int[] ignoredArray = box.a;
        return box.i + (int) box.l + (int) box.f + (int) box.d;
    }
    public static void main(String[] args) {
        Box box = new Box();
        box.i = 20;
        box.l = 8L;
        box.f = 3.0f;
        box.d = 4.0d;
        System.out.println(run(box, new Object(), new int[] { 1, 2, 3 }));
    }
}
"#,
            )],
        );
        let interp = run_with_jit_threshold(&root, "demo.Main", u32::MAX);
        let jit = run_with_jit_threshold(&root, "demo.Main", 1);

        assert_eq!(jit.output, interp.output);
        assert!(
            jit.jit_executions >= 2,
            "expected top-level JIT/deopt plus compiled run() field helper path, got {}",
            jit.jit_executions
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

    #[test]
    fn jit_compiled_method_can_invoke_virtual_helper() {
        let root = compile_java(
            "compiled_invokevirtual_helper",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static class Counter {
        int base;
        Counter(int base) { this.base = base; }
        int add(int value) { return base + value; }
    }
    static int run(Counter counter) {
        return counter.add(5) + 2;
    }
    public static void main(String[] args) {
        System.out.println(run(new Counter(40)));
    }
}
"#,
            )],
        );
        let interp = run_with_jit_threshold(&root, "demo.Main", u32::MAX);
        let jit = run_with_jit_threshold(&root, "demo.Main", 1);

        assert_eq!(jit.output, interp.output);
        assert!(
            jit.jit_executions >= 2,
            "expected top-level JIT/deopt plus compiled run() to reach JIT, got {}",
            jit.jit_executions
        );
    }

    #[test]
    fn jit_compiled_method_can_invoke_special_helper() {
        let root = compile_java(
            "compiled_invokespecial_helper",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    private int hidden(int value) {
        return value * 2;
    }
    static int run(Main m) {
        return m.hidden(21) + 1;
    }
    public static void main(String[] args) {
        System.out.println(run(new Main()));
    }
}
"#,
            )],
        );
        let interp = run_with_jit_threshold(&root, "demo.Main", u32::MAX);
        let jit = run_with_jit_threshold(&root, "demo.Main", 1);

        assert_eq!(jit.output, interp.output);
        assert!(
            jit.jit_executions >= 2,
            "expected top-level JIT/deopt plus compiled run() to reach JIT, got {}",
            jit.jit_executions
        );
    }

    #[test]
    fn jit_compiled_method_can_invoke_special_helper_with_long_return() {
        let root = compile_java(
            "compiled_invokespecial_helper_long_return",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    private long hidden(long value) {
        return value + 11L;
    }
    static long run(Main m) {
        return m.hidden(31L) + 1L;
    }
    public static void main(String[] args) {
        System.out.println(run(new Main()));
    }
}
"#,
            )],
        );
        let interp = run_with_jit_threshold(&root, "demo.Main", u32::MAX);
        let jit = run_with_jit_threshold(&root, "demo.Main", 1);

        assert_eq!(jit.output, interp.output);
        assert!(
            jit.jit_executions >= 2,
            "expected top-level JIT/deopt plus compiled run() to reach JIT, got {}",
            jit.jit_executions
        );
    }

    #[test]
    fn jit_compiled_method_can_invoke_interface_helper() {
        let root = compile_java(
            "compiled_invokeinterface_helper",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    interface Adder {
        int add(int value);
    }
    static class Impl implements Adder {
        public int add(int value) {
            return value + 30;
        }
    }
    static int run(Adder adder) {
        return adder.add(8) + 4;
    }
    public static void main(String[] args) {
        System.out.println(run(new Impl()));
    }
}
"#,
            )],
        );
        let interp = run_with_jit_threshold(&root, "demo.Main", u32::MAX);
        let jit = run_with_jit_threshold(&root, "demo.Main", 1);

        assert_eq!(jit.output, interp.output);
        assert!(
            jit.jit_executions >= 2,
            "expected top-level JIT/deopt plus compiled run() to reach JIT, got {}",
            jit.jit_executions
        );
    }

    #[test]
    fn jit_compiled_method_can_invoke_interface_helper_with_long_return() {
        let root = compile_java(
            "compiled_invokeinterface_helper_long_return",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    interface Adder {
        long add(long value);
    }
    static class Impl implements Adder {
        public long add(long value) {
            return value + 30L;
        }
    }
    static long run(Adder adder) {
        return adder.add(8L) + 4L;
    }
    public static void main(String[] args) {
        System.out.println(run(new Impl()));
    }
}
"#,
            )],
        );
        let interp = run_with_jit_threshold(&root, "demo.Main", u32::MAX);
        let jit = run_with_jit_threshold(&root, "demo.Main", 1);

        assert_eq!(jit.output, interp.output);
        assert!(
            jit.jit_executions >= 2,
            "expected top-level JIT/deopt plus compiled run() to reach JIT, got {}",
            jit.jit_executions
        );
    }

    #[test]
    fn jit_invoke_helper_supports_many_and_floating_args() {
        let root = compile_java(
            "invoke_helper_many_float_args",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static int sumMany(int a, int b, int c, int d, int e, int f, int g, int h) {
        return a + b + c + d + e + f + g + h;
    }
    static double mix(float f, double d, long l) {
        return f + d + l;
    }
    static double run() {
        return sumMany(1, 2, 3, 4, 5, 6, 7, 8) + mix(1.5f, 2.25, 3L);
    }
    public static void main(String[] args) {
        System.out.println(run());
    }
}
"#,
            )],
        );
        let interp = run_with_jit_threshold(&root, "demo.Main", u32::MAX);
        let jit = run_with_jit_threshold(&root, "demo.Main", 1);

        assert_eq!(jit.output, interp.output);
        assert!(
            jit.jit_executions >= 2,
            "expected top-level JIT/deopt plus compiled run() to reach JIT, got {}",
            jit.jit_executions
        );
    }

    #[test]
    fn jit_compiled_method_can_invoke_dynamic_helper() {
        let root = compile_java(
            "compiled_invokedynamic_helper",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    interface IntSupplier {
        int get();
    }
    static IntSupplier make(int base) {
        return () -> base + 7;
    }
    public static void main(String[] args) {
        System.out.println(make(35).get());
    }
}
"#,
            )],
        );
        let interp = run_with_jit_threshold(&root, "demo.Main", u32::MAX);
        let jit = run_with_jit_threshold(&root, "demo.Main", 1);

        assert_eq!(jit.output, interp.output);
        assert!(
            jit.jit_executions >= 2,
            "expected top-level JIT/deopt plus compiled make() to reach JIT, got {}",
            jit.jit_executions
        );
    }

    #[test]
    fn jit_invokenative_helper_executes_native_method() {
        let method = Method::new(
            [
                0x10, 0xd6, // bipush -42
                0xfe, 0x00, 0x01, // invokenative #1 java/lang/Math.abs(I)I
                0xac, // ireturn
            ],
            0,
            1,
        )
        .with_metadata("demo/Main", "runNative", "()I", 0x0008)
        .with_method_refs(vec![
            None,
            Some(MethodRef {
                class_name: "java/lang/Math".to_string(),
                method_name: "abs".to_string(),
                descriptor: "(I)I".to_string(),
            }),
        ]);

        let mut vm = Vm::new().expect("failed to create VM");
        vm.set_jit_thresholds(1, 1);
        let result = vm.execute(method).unwrap();

        assert_eq!(result, ExecutionResult::Value(Value::Int(42)));
        assert!(
            vm.jit_executions() >= 1,
            "expected synthetic invokenative method to execute through JIT"
        );
    }

    #[test]
    fn jit_direct_athrow_reports_pending_exception() {
        let method = Method::new(
            [
                0xbb, 0x00, 0x01, // new #1 demo/Thrown
                0xbf, // athrow
            ],
            0,
            1,
        )
        .with_metadata("demo/Main", "throwNow", "()V", 0x0008)
        .with_reference_classes(vec![None, Some("demo/Thrown".to_string())]);

        let mut vm = Vm::new().expect("failed to create VM");
        vm.register_class(RuntimeClass {
            name: "demo/Thrown".to_string(),
            super_class: Some("java/lang/RuntimeException".to_string()),
            methods: HashMap::new(),
            static_fields: HashMap::new(),
            instance_fields: vec![],
            interfaces: vec![],
        });
        vm.set_jit_thresholds(1, 1);

        let err = vm
            .execute(method)
            .expect_err("expected JIT athrow to escape");
        assert_eq!(
            err,
            VmError::UnhandledException {
                class_name: "demo/Thrown".to_string()
            }
        );
        assert!(
            vm.jit_executions() >= 1,
            "expected synthetic athrow method to execute through JIT"
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

    // ---- Tier 8: arrays + loops + method calls (Dijkstra). ----

    #[test]
    fn jit_dijkstra_matches_interpreter() {
        assert_jit_matches_interpreter(
            "dijkstra",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static final int INF = Integer.MAX_VALUE;
    static int[][] graph = {
        {0, 4, 0, 0, 0, 0, 0, 8, 0},
        {4, 0, 8, 0, 0, 0, 0, 11, 0},
        {0, 8, 0, 7, 0, 4, 0, 0, 2},
        {0, 0, 7, 0, 9, 14, 0, 0, 0},
        {0, 0, 0, 9, 0, 10, 0, 0, 0},
        {0, 0, 4, 14, 10, 0, 2, 0, 0},
        {0, 0, 0, 0, 0, 2, 0, 1, 6},
        {8, 11, 0, 0, 0, 0, 1, 0, 7},
        {0, 0, 2, 0, 0, 0, 6, 7, 0}
    };
    static int[] dist = new int[9];
    static boolean[] visited = new boolean[9];
    static int minDistance() {
        int min = INF;
        int idx = -1;
        for (int i = 0; i < 9; i++) {
            if (!visited[i] && dist[i] < min) {
                min = dist[i];
                idx = i;
            }
        }
        return idx;
    }
    static void dijkstra(int src) {
        for (int i = 0; i < 9; i++) {
            dist[i] = INF;
            visited[i] = false;
        }
        dist[src] = 0;
        for (int i = 0; i < 9 - 1; i++) {
            int u = minDistance();
            if (u == -1) break;
            visited[u] = true;
            for (int v = 0; v < 9; v++) {
                if (!visited[v] && graph[u][v] != 0 && dist[u] != INF && dist[u] + graph[u][v] < dist[v]) {
                    dist[v] = dist[u] + graph[u][v];
                }
            }
        }
    }
    public static void main(String[] args) {
        dijkstra(0);
        for (int i = 0; i < 9; i++) {
            System.out.println(i + ":" + dist[i]);
        }
    }
}
"#,
            )],
        );
    }

    // ---- Tier 9: arrays + nested loops (KMP). ----

    #[test]
    fn jit_kmp_matches_interpreter() {
        assert_jit_matches_interpreter(
            "kmp",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static int[] computeLPS(String pattern) {
        int m = pattern.length();
        int[] lps = new int[m];
        lps[0] = 0;
        int len = 0;
        int i = 1;
        while (i < m) {
            if (pattern.charAt(i) == pattern.charAt(len)) {
                len++;
                lps[i] = len;
                i++;
            } else {
                if (len != 0) {
                    len = lps[len - 1];
                } else {
                    lps[i] = 0;
                    i++;
                }
            }
        }
        return lps;
    }
    static int KMPSearch(String txt, String pat) {
        int n = txt.length();
        int m = pat.length();
        int[] lps = computeLPS(pat);
        int occurrences = 0;
        int i = 0;
        int j = 0;
        while (i < n) {
            if (pat.charAt(j) == txt.charAt(i)) {
                i++;
                j++;
                if (j == m) {
                    occurrences++;
                    j = lps[j - 1];
                }
            } else {
                if (j != 0) {
                    j = lps[j - 1];
                } else {
                    i++;
                }
            }
        }
        return occurrences;
    }
    public static void main(String[] args) {
        String txt = "ABABDABACDABABCABAB";
        String pat = "ABAB";
        int result = KMPSearch(txt, pat);
        System.out.println(result);
    }
}
"#,
            )],
        );
    }

    // ---- Tier 10: bubble sort (nested loops + array swap). ----

    #[test]
    fn jit_bubble_sort_matches_interpreter() {
        assert_jit_matches_interpreter(
            "bubble_sort",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static void bubbleSort(int[] arr) {
        int n = arr.length;
        for (int i = 0; i < n - 1; i++) {
            for (int j = 0; j < n - i - 1; j++) {
                if (arr[j] > arr[j + 1]) {
                    int tmp = arr[j];
                    arr[j] = arr[j + 1];
                    arr[j + 1] = tmp;
                }
            }
        }
    }
    public static void main(String[] args) {
        int[] data = {64, 34, 25, 12, 22, 11, 90};
        bubbleSort(data);
        for (int i = 0; i < data.length; i++) {
            System.out.println(data[i]);
        }
    }
}
"#,
            )],
        );
    }

    // ---- Tier 11: matrix multiplication (2D arrays + triple nested loops). ----

    #[test]
    fn jit_matrix_multiply_matches_interpreter() {
        assert_jit_matches_interpreter(
            "matrix_multiply",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static void multiply(int[][] a, int[][] b, int[][] r, int n) {
        for (int i = 0; i < n; i++) {
            for (int j = 0; j < n; j++) {
                for (int k = 0; k < n; k++) {
                    r[i][j] = r[i][j] + a[i][k] * b[k][j];
                }
            }
        }
    }
    public static void main(String[] args) {
        int n = 2;
        int[][] a = {{1, 2}, {3, 4}};
        int[][] b = {{5, 6}, {7, 8}};
        int[][] r = {{0, 0}, {0, 0}};
        multiply(a, b, r, n);
        System.out.println(r[0][0]);
        System.out.println(r[0][1]);
        System.out.println(r[1][0]);
        System.out.println(r[1][1]);
    }
}
"#,
            )],
        );
    }

    // ---- Tier 12: quicksort (recursive partition). ----

    #[test]
    fn jit_quicksort_matches_interpreter() {
        assert_jit_matches_interpreter(
            "quicksort",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static void sort(int[] arr, int low, int high) {
        if (low < high) {
            int pi = partition(arr, low, high);
            sort(arr, low, pi - 1);
            sort(arr, pi + 1, high);
        }
    }
    static int partition(int[] arr, int low, int high) {
        int pivot = arr[high];
        int i = low - 1;
        for (int j = low; j < high; j++) {
            if (arr[j] <= pivot) {
                i++;
                int tmp = arr[i];
                arr[i] = arr[j];
                arr[j] = tmp;
            }
        }
        int tmp = arr[i + 1];
        arr[i + 1] = arr[high];
        arr[high] = tmp;
        return i + 1;
    }
    public static void main(String[] args) {
        int[] data = {10, 7, 8, 9, 1, 5};
        sort(data, 0, data.length - 1);
        for (int i = 0; i < data.length; i++) {
            System.out.println(data[i]);
        }
    }
}
"#,
            )],
        );
    }

    // ---- Tier 13: merge sort (divide and conquer + arrays). ----

    #[test]
    fn jit_merge_sort_matches_interpreter() {
        assert_jit_matches_interpreter(
            "merge_sort",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static void merge(int[] arr, int l, int m, int r) {
        int n1 = m - l + 1;
        int n2 = r - m;
        int[] left = new int[n1];
        int[] right = new int[n2];
        for (int i = 0; i < n1; i++) left[i] = arr[l + i];
        for (int j = 0; j < n2; j++) right[j] = arr[m + 1 + j];
        int i = 0, j = 0, k = l;
        while (i < n1 && j < n2) {
            if (left[i] <= right[j]) { arr[k] = left[i]; i++; }
            else { arr[k] = right[j]; j++; }
            k++;
        }
        while (i < n1) { arr[k] = left[i]; i++; k++; }
        while (j < n2) { arr[k] = right[j]; j++; k++; }
    }
    static void sort(int[] arr, int l, int r) {
        if (l < r) {
            int m = l + (r - l) / 2;
            sort(arr, l, m);
            sort(arr, m + 1, r);
            merge(arr, l, m, r);
        }
    }
    public static void main(String[] args) {
        int[] data = {38, 27, 43, 3, 9, 82, 10};
        sort(data, 0, data.length - 1);
        for (int i = 0; i < data.length; i++) {
            System.out.println(data[i]);
        }
    }
}
"#,
            )],
        );
    }

    // ---- Tier 14: binary search (iterative + method calls). ----

    #[test]
    fn jit_binary_search_matches_interpreter() {
        assert_jit_matches_interpreter(
            "binary_search",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static int binarySearch(int[] arr, int target) {
        int left = 0, right = arr.length - 1;
        while (left <= right) {
            int mid = left + (right - left) / 2;
            if (arr[mid] == target) return mid;
            if (arr[mid] < target) left = mid + 1;
            else right = mid - 1;
        }
        return -1;
    }
    public static void main(String[] args) {
        int[] arr = {2, 3, 4, 10, 40, 50, 60, 70};
        System.out.println(binarySearch(arr, 10));
        System.out.println(binarySearch(arr, 5));
    }
}
"#,
            )],
        );
    }

    // ---- Tier 15: longest common subsequence (dynamic programming). ----

    #[test]
    fn jit_lcs_matches_interpreter() {
        assert_jit_matches_interpreter(
            "lcs",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static int lcs(String s1, String s2) {
        int m = s1.length(), n = s2.length();
        int[][] dp = new int[m + 1][n + 1];
        for (int i = 1; i <= m; i++) {
            for (int j = 1; j <= n; j++) {
                if (s1.charAt(i - 1) == s2.charAt(j - 1)) {
                    dp[i][j] = dp[i - 1][j - 1] + 1;
                } else {
                    dp[i][j] = Math.max(dp[i - 1][j], dp[i][j - 1]);
                }
            }
        }
        return dp[m][n];
    }
    public static void main(String[] args) {
        String s1 = "AGGTAB";
        String s2 = "GXTXAYB";
        System.out.println(lcs(s1, s2));
    }
}
"#,
            )],
        );
    }

    // ---- Tier 16: 0/1 knapsack (dynamic programming). ----

    #[test]
    fn jit_knapsack_matches_interpreter() {
        assert_jit_matches_interpreter(
            "knapsack",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static int knapsack(int W, int[] wt, int[] val, int n) {
        int[][] dp = new int[n + 1][W + 1];
        for (int i = 1; i <= n; i++) {
            for (int w = 0; w <= W; w++) {
                if (wt[i - 1] <= w) {
                    dp[i][w] = Math.max(val[i - 1] + dp[i - 1][w - wt[i - 1]], dp[i - 1][w]);
                } else {
                    dp[i][w] = dp[i - 1][w];
                }
            }
        }
        return dp[n][W];
    }
    public static void main(String[] args) {
        int[] val = {60, 100, 120};
        int[] wt = {10, 20, 30};
        int W = 50;
        System.out.println(knapsack(W, wt, val, 3));
    }
}
"#,
            )],
        );
    }

    // ---- Tier 17: BFS graph traversal. ----

    #[test]
    fn jit_bfs_matches_interpreter() {
        assert_jit_matches_interpreter(
            "bfs",
            &[(
                "demo/Main.java",
                r#"
package demo;
import java.util.*;
public class Main {
    static void bfs(int[][] graph, int start) {
        int n = graph.length;
        boolean[] visited = new boolean[n];
        Queue<Integer> queue = new LinkedList<>();
        visited[start] = true;
        queue.offer(start);
        while (!queue.isEmpty()) {
            int v = queue.poll();
            System.out.println(v);
            for (int i = 0; i < n; i++) {
                if (graph[v][i] == 1 && !visited[i]) {
                    visited[i] = true;
                    queue.offer(i);
                }
            }
        }
    }
    public static void main(String[] args) {
        int[][] graph = {
            {0, 1, 1, 0, 0},
            {1, 0, 0, 1, 1},
            {1, 0, 0, 0, 1},
            {0, 1, 0, 0, 0},
            {0, 1, 1, 0, 0}
        };
        bfs(graph, 0);
    }
}
"#,
            )],
        );
    }

    // ---- Tier 18: DFS graph traversal (recursive). ----

    #[test]
    fn jit_dfs_matches_interpreter() {
        assert_jit_matches_interpreter(
            "dfs",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static void dfs(int[][] graph, int v, boolean[] visited) {
        visited[v] = true;
        System.out.println(v);
        for (int i = 0; i < graph.length; i++) {
            if (graph[v][i] == 1 && !visited[i]) {
                dfs(graph, i, visited);
            }
        }
    }
    public static void main(String[] args) {
        int[][] graph = {
            {0, 1, 1, 0, 0},
            {1, 0, 0, 1, 1},
            {1, 0, 0, 0, 1},
            {0, 1, 0, 0, 0},
            {0, 1, 1, 0, 0}
        };
        boolean[] visited = new boolean[5];
        dfs(graph, 0, visited);
    }
}
"#,
            )],
        );
    }

    // ---- Tier 19: Sieve of Eratosthenes (prime numbers). ----

    #[test]
    fn jit_sieve_eratosthenes_matches_interpreter() {
        assert_jit_matches_interpreter(
            "sieve",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static void sieve(int n) {
        boolean[] isPrime = new boolean[n + 1];
        for (int i = 2; i <= n; i++) isPrime[i] = true;
        for (int p = 2; p * p <= n; p++) {
            if (isPrime[p]) {
                for (int i = p * p; i <= n; i += p) {
                    isPrime[i] = false;
                }
            }
        }
        for (int i = 2; i <= n; i++) {
            if (isPrime[i]) {
                System.out.println(i);
            }
        }
    }
    public static void main(String[] args) {
        sieve(30);
    }
}
"#,
            )],
        );
    }

    // ---- Tier 20: Fibonacci (iterative with array). ----

    #[test]
    fn jit_fibonacci_iterative_matches_interpreter() {
        assert_jit_matches_interpreter(
            "fibonacci_iterative",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static int[] fibonacci(int n) {
        int[] fib = new int[n + 1];
        fib[0] = 0;
        fib[1] = 1;
        for (int i = 2; i <= n; i++) {
            fib[i] = fib[i - 1] + fib[i - 2];
        }
        return fib;
    }
    public static void main(String[] args) {
        int[] fib = fibonacci(10);
        for (int i = 0; i < fib.length; i++) {
            System.out.println(fib[i]);
        }
    }
}
"#,
            )],
        );
    }

    // ---- Tier 21: Tower of Hanoi (recursive). ----

    #[test]
    fn jit_tower_of_hanoi_matches_interpreter() {
        assert_jit_matches_interpreter(
            "tower_of_hanoi",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static void towerOfHanoi(int n, char from, char to, char aux) {
        if (n == 1) {
            System.out.println("Move disk 1 from " + from + " to " + to);
            return;
        }
        towerOfHanoi(n - 1, from, aux, to);
        System.out.println("Move disk " + n + " from " + from + " to " + to);
        towerOfHanoi(n - 1, aux, to, from);
    }
    public static void main(String[] args) {
        towerOfHanoi(4, 'A', 'C', 'B');
    }
}
"#,
            )],
        );
    }

    // ---- Tier 22: Floyd-Warshall all-pairs shortest path. ----

    #[test]
    fn jit_floyd_warshall_matches_interpreter() {
        assert_jit_matches_interpreter(
            "floyd_warshall",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static final int INF = 99999;
    static void floydWarshall(int[][] graph, int n) {
        int[][] dist = new int[n][n];
        for (int i = 0; i < n; i++) {
            for (int j = 0; j < n; j++) {
                dist[i][j] = graph[i][j];
            }
        }
        for (int k = 0; k < n; k++) {
            for (int i = 0; i < n; i++) {
                for (int j = 0; j < n; j++) {
                    if (dist[i][k] + dist[k][j] < dist[i][j]) {
                        dist[i][j] = dist[i][k] + dist[k][j];
                    }
                }
            }
        }
        for (int i = 0; i < n; i++) {
            for (int j = 0; j < n; j++) {
                if (dist[i][j] == INF) {
                    System.out.print("INF ");
                } else {
                    System.out.print(dist[i][j] + " ");
                }
            }
            System.out.println();
        }
    }
    public static void main(String[] args) {
        int[][] graph = {
            {0, 5, INF, 10},
            {INF, 0, 3, INF},
            {INF, INF, 0, 1},
            {INF, INF, INF, 0}
        };
        floydWarshall(graph, 4);
    }
}
"#,
            )],
        );
    }

    // ---- Tier 23: Selection sort (nested loops + swaps). ----

    #[test]
    fn jit_selection_sort_matches_interpreter() {
        assert_jit_matches_interpreter(
            "selection_sort",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static void selectionSort(int[] arr) {
        int n = arr.length;
        for (int i = 0; i < n - 1; i++) {
            int minIdx = i;
            for (int j = i + 1; j < n; j++) {
                if (arr[j] < arr[minIdx]) {
                    minIdx = j;
                }
            }
            int temp = arr[minIdx];
            arr[minIdx] = arr[i];
            arr[i] = temp;
        }
    }
    public static void main(String[] args) {
        int[] data = {64, 25, 12, 22, 11};
        selectionSort(data);
        for (int i = 0; i < data.length; i++) {
            System.out.println(data[i]);
        }
    }
}
"#,
            )],
        );
    }

    // ---- Tier 24: Insertion sort. ----

    #[test]
    fn jit_insertion_sort_matches_interpreter() {
        assert_jit_matches_interpreter(
            "insertion_sort",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static void insertionSort(int[] arr) {
        for (int i = 1; i < arr.length; i++) {
            int key = arr[i];
            int j = i - 1;
            while (j >= 0 && arr[j] > key) {
                arr[j + 1] = arr[j];
                j--;
            }
            arr[j + 1] = key;
        }
    }
    public static void main(String[] args) {
        int[] data = {12, 11, 13, 5, 6};
        insertionSort(data);
        for (int i = 0; i < data.length; i++) {
            System.out.println(data[i]);
        }
    }
}
"#,
            )],
        );
    }

    // ---- Tier 25: Edit distance (Levenshtein distance). ----

    #[test]
    fn jit_edit_distance_matches_interpreter() {
        assert_jit_matches_interpreter(
            "edit_distance",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static int min(int a, int b, int c) {
        return Math.min(Math.min(a, b), c);
    }
    static int editDistance(String s1, String s2) {
        int m = s1.length(), n = s2.length();
        int[][] dp = new int[m + 1][n + 1];
        for (int i = 0; i <= m; i++) dp[i][0] = i;
        for (int j = 0; j <= n; j++) dp[0][j] = j;
        for (int i = 1; i <= m; i++) {
            for (int j = 1; j <= n; j++) {
                if (s1.charAt(i - 1) == s2.charAt(j - 1)) {
                    dp[i][j] = dp[i - 1][j - 1];
                } else {
                    dp[i][j] = 1 + min(dp[i - 1][j], dp[i][j - 1], dp[i - 1][j - 1]);
                }
            }
        }
        return dp[m][n];
    }
    public static void main(String[] args) {
        String s1 = "sitting";
        String s2 = "kitten";
        System.out.println(editDistance(s1, s2));
    }
}
"#,
            )],
        );
    }

    // ---- Tier 27: Heap sort. ----

    #[test]
    fn jit_heap_sort_matches_interpreter() {
        assert_jit_matches_interpreter(
            "heap_sort",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static void heapSort(int[] arr) {
        int n = arr.length;
        for (int i = n / 2 - 1; i >= 0; i--) {
            heapify(arr, n, i);
        }
        for (int i = n - 1; i > 0; i--) {
            int temp = arr[0];
            arr[0] = arr[i];
            arr[i] = temp;
            heapify(arr, i, 0);
        }
    }
    static void heapify(int[] arr, int n, int i) {
        int largest = i;
        int left = 2 * i + 1;
        int right = 2 * i + 2;
        if (left < n && arr[left] > arr[largest]) largest = left;
        if (right < n && arr[right] > arr[largest]) largest = right;
        if (largest != i) {
            int temp = arr[i];
            arr[i] = arr[largest];
            arr[largest] = temp;
            heapify(arr, n, largest);
        }
    }
    public static void main(String[] args) {
        int[] data = {12, 11, 13, 5, 6, 7};
        heapSort(data);
        for (int i = 0; i < data.length; i++) {
            System.out.println(data[i]);
        }
    }
}
"#,
            )],
        );
    }

    // ---- Tier 28: Comprehensive mixed-type algorithm tests. ----

    #[test]
    fn jit_comprehensive_mixed_types_matches_interpreter() {
        assert_jit_matches_interpreter(
            "comprehensive_mixed",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static class Data {
        int key;
        long value;
        double score;
        float ratio;
        boolean active;
    }
    static int partition(Data[] arr, int low, int high) {
        int pivot = arr[high].key;
        int i = low - 1;
        for (int j = low; j < high; j++) {
            if (arr[j].key <= pivot) {
                i++;
                Data tmp = arr[i];
                arr[i] = arr[j];
                arr[j] = tmp;
            }
        }
        Data tmp = arr[i + 1];
        arr[i + 1] = arr[high];
        arr[high] = tmp;
        return i + 1;
    }
    static void sort(Data[] arr, int low, int high) {
        if (low < high) {
            int pi = partition(arr, low, high);
            sort(arr, low, pi - 1);
            sort(arr, pi + 1, high);
        }
    }
    static double computeScore(Data[] arr) {
        double sum = 0.0;
        for (int i = 0; i < arr.length; i++) {
            if (arr[i].active) {
                sum += arr[i].score * arr[i].ratio;
            }
        }
        return sum;
    }
    static long factorial(int n) {
        long result = 1L;
        for (int i = 2; i <= n; i++) {
            result *= i;
        }
        return result;
    }
    static boolean isPrime(int n) {
        if (n <= 1) return false;
        if (n <= 3) return true;
        if (n % 2 == 0 || n % 3 == 0) return false;
        for (int i = 5; i * i <= n; i += 6) {
            if (n % i == 0 || n % (i + 2) == 0) return false;
        }
        return true;
    }
    public static void main(String[] args) {
        Data[] data = new Data[6];
        data[0] = new Data(); data[0].key = 5; data[0].value = 100L; data[0].score = 3.14; data[0].ratio = 1.5f; data[0].active = true;
        data[1] = new Data(); data[1].key = 2; data[1].value = 200L; data[1].score = 2.71; data[1].ratio = 2.0f; data[1].active = true;
        data[2] = new Data(); data[2].key = 8; data[2].value = 50L; data[2].score = 1.41; data[2].ratio = 0.5f; data[2].active = false;
        data[3] = new Data(); data[3].key = 1; data[3].value = 300L; data[3].score = 1.73; data[3].ratio = 1.0f; data[3].active = true;
        data[4] = new Data(); data[4].key = 9; data[4].value = 150L; data[4].score = 2.24; data[4].ratio = 0.8f; data[4].active = true;
        data[5] = new Data(); data[5].key = 3; data[5].value = 75L; data[5].score = 1.41; data[5].ratio = 1.2f; data[5].active = false;
        sort(data, 0, data.length - 1);
        for (int i = 0; i < data.length; i++) {
            System.out.println(data[i].key);
            System.out.println(data[i].value);
        }
        System.out.println(computeScore(data));
        System.out.println(factorial(12));
        for (int i = 2; i <= 30; i++) {
            if (isPrime(i)) System.out.println(i);
        }
    }
}
"#,
            )],
        );
    }

    #[test]
    fn jit_mixed_float_double_arithmetic_matches_interpreter() {
        assert_jit_matches_interpreter(
            "mixed_float_double",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static double complexCalc(double a, float b, long c, int d) {
        double result = a * b + c - d;
        result = result / 2.5 + Math.sqrt(Math.abs(result));
        return result;
    }
    static float[] processFloats(float[] arr, double multiplier) {
        for (int i = 0; i < arr.length; i++) {
            arr[i] = (float)(arr[i] * multiplier);
        }
        return arr;
    }
    static double[] initializeDoubles(int n) {
        double[] arr = new double[n];
        for (int i = 0; i < n; i++) {
            arr[i] = i * 1.5;
        }
        return arr;
    }
    static double computeVariance(double[] values) {
        double mean = 0.0;
        for (int i = 0; i < values.length; i++) {
            mean += values[i];
        }
        mean /= values.length;
        double sumSq = 0.0;
        for (int i = 0; i < values.length; i++) {
            double diff = values[i] - mean;
            sumSq += diff * diff;
        }
        return sumSq / values.length;
    }
    public static void main(String[] args) {
        System.out.println(complexCalc(3.14, 2.5f, 100L, 25));
        float[] floats = {1.0f, 2.0f, 3.0f, 4.0f};
        processFloats(floats, 2.0);
        for (int i = 0; i < floats.length; i++) {
            System.out.println(floats[i]);
        }
        double[] doubles = initializeDoubles(5);
        for (int i = 0; i < doubles.length; i++) {
            System.out.println(doubles[i]);
        }
        System.out.println(computeVariance(doubles));
    }
}
"#,
            )],
        );
    }

    // ---- Tier 29: 2D double arrays (drives double array load/store JIT lowering). ----

    #[test]
    fn jit_2d_double_array_matches_interpreter() {
        assert_jit_matches_interpreter(
            "2d_double_array",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static double sumMatrix(double[][] m) {
        double sum = 0.0;
        for (int i = 0; i < m.length; i++) {
            for (int j = 0; j < m[i].length; j++) {
                sum += m[i][j];
            }
        }
        return sum;
    }
    static double[][] transpose(double[][] m, int n) {
        double[][] r = new double[n][n];
        for (int i = 0; i < n; i++) {
            for (int j = 0; j < n; j++) {
                r[i][j] = m[j][i];
            }
        }
        return r;
    }
    public static void main(String[] args) {
        double[][] a = {{1.5, 2.5}, {3.5, 4.5}};
        System.out.println(sumMatrix(a));
        double[][] t = transpose(a, 2);
        System.out.println(t[0][0]);
        System.out.println(t[1][0]);
    }
}
"#,
            )],
        );
    }

    // ---- Tier 30: StringBuilder (drives StringBuilder/append/toString JIT lowering). ----

    #[test]
    fn jit_stringbuilder_matches_interpreter() {
        assert_jit_matches_interpreter(
            "stringbuilder",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static String buildString(int n) {
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < n; i++) {
            sb.append(i).append(":").append("val").append(" ");
        }
        return sb.toString();
    }
    static String repeat(String s, int count) {
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < count; i++) {
            sb.append(s);
        }
        return sb.toString();
    }
    public static void main(String[] args) {
        System.out.println(buildString(3));
        System.out.println(repeat("ab", 4));
    }
}
"#,
            )],
        );
    }

    // ---- Tier 31: String concatenation with + (drives string concat JIT lowering). ----

    #[test]
    fn jit_string_concat_operator_matches_interpreter() {
        assert_jit_matches_interpreter(
            "string_concat_op",
            &[(
                "demo/Main.java",
                r#"
package demo;
public class Main {
    static String concatAll(int a, double b, boolean c) {
        return "int=" + a + " double=" + b + " bool=" + c;
    }
    static String nestedConcat(String a, int b) {
        return "A" + a + "B" + b + "C";
    }
    public static void main(String[] args) {
        System.out.println(concatAll(42, 3.14, true));
        System.out.println(nestedConcat("foo", 99));
    }
}
"#,
            )],
        );
    }
}

mod comprehensive_suite {
    use super::*;
    // ARRAY OPERATIONS - Test iaload, iastore, aload, astore, aaload, aastore
    // =============================================================================

    mod array_operations {
        use super::*;

        // Test iaload - int array load
        #[test]
        fn jit_array_iaload() {
            assert_jit_matches_interpreter(
                "jit_array_iaload",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int[] arr = {10, 20, 30, 40, 50};
        System.out.println(arr[0]);
        System.out.println(arr[2]);
        System.out.println(arr[4]);
    }
}
"#,
                )],
            );
        }

        // Test iastore - int array store
        #[test]
        fn jit_array_iastore() {
            assert_jit_matches_interpreter(
                "jit_array_iastore",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int[] arr = new int[3];
        arr[0] = 100;
        arr[1] = 200;
        arr[2] = 300;
        System.out.println(arr[0]);
        System.out.println(arr[1]);
        System.out.println(arr[2]);
    }
}
"#,
                )],
            );
        }

        // Test aload - reference array load
        #[test]
        fn jit_array_aload() {
            assert_jit_matches_interpreter(
                "jit_array_aload",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        String[] arr = {"hello", "world", "test"};
        System.out.println(arr[0]);
        System.out.println(arr[1]);
        System.out.println(arr[2]);
    }
}
"#,
                )],
            );
        }

        // Test aaload - multi-dimensional array load
        #[test]
        fn jit_array_aaload() {
            assert_jit_matches_interpreter(
                "jit_array_aaload",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        String[][] arr = {{"a", "b"}, {"c", "d"}};
        System.out.println(arr[0][0]);
        System.out.println(arr[1][1]);
    }
}
"#,
                )],
            );
        }

        // Test aastore - reference array store
        #[test]
        fn jit_array_aastore() {
            assert_jit_matches_interpreter(
                "jit_array_aastore",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        String[] arr = new String[3];
        arr[0] = "first";
        arr[1] = "second";
        arr[2] = "third";
        System.out.println(arr[0]);
        System.out.println(arr[1]);
        System.out.println(arr[2]);
    }
}
"#,
                )],
            );
        }

        // Test arraylength
        #[test]
        fn jit_arraylength() {
            assert_jit_matches_interpreter(
                "jit_arraylength",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int[] arr1 = new int[5];
        int[] arr2 = new int[10];
        String[] arr3 = {"a", "b", "c", "d"};
        System.out.println(arr1.length);
        System.out.println(arr2.length);
        System.out.println(arr3.length);
    }
}
"#,
                )],
            );
        }

        // Test newarray
        #[test]
        fn jit_newarray() {
            assert_jit_matches_interpreter(
                "jit_newarray",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int[] arr1 = new int[3];
        arr1[0] = 1; arr1[1] = 2; arr1[2] = 3;
        boolean[] arr2 = new boolean[2];
        arr2[0] = true; arr2[1] = false;
        System.out.println(arr1.length);
        System.out.println(arr2.length);
    }
}
"#,
                )],
            );
        }

        // Test anewarray
        #[test]
        fn jit_anewarray() {
            assert_jit_matches_interpreter(
                "jit_anewarray",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        String[] arr = new String[3];
        arr[0] = "hello";
        System.out.println(arr.length);
        System.out.println(arr[0]);
    }
}
"#,
                )],
            );
        }

        // Test combined array access in loop
        #[test]
        fn jit_array_access_in_loop() {
            assert_jit_matches_interpreter(
                "jit_array_access_in_loop",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int[] arr = {1, 2, 3, 4, 5};
        int sum = 0;
        for (int i = 0; i < arr.length; i++) {
            sum += arr[i];
        }
        System.out.println(sum);
    }
}
"#,
                )],
            );
        }

        // Test array swap (critical for bubble sort)
        #[test]
        fn jit_array_swap() {
            assert_jit_matches_interpreter(
                "jit_array_swap",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int[] arr = {1, 2, 3};
        int tmp = arr[0];
        arr[0] = arr[1];
        arr[1] = tmp;
        System.out.println(arr[0]);
        System.out.println(arr[1]);
        System.out.println(arr[2]);
    }
}
"#,
                )],
            );
        }

        // Test array reverse
        #[test]
        fn jit_array_reverse() {
            assert_jit_matches_interpreter(
                "jit_array_reverse",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int[] arr = {1, 2, 3, 4, 5};
        for (int i = 0; i < arr.length / 2; i++) {
            int tmp = arr[i];
            arr[i] = arr[arr.length - 1 - i];
            arr[arr.length - 1 - i] = tmp;
        }
        for (int i = 0; i < arr.length; i++) {
            System.out.println(arr[i]);
        }
    }
}
"#,
                )],
            );
        }
    }

    // =============================================================================
    // LOCAL VARIABLE OPERATIONS - Test iload, istore, aload, astore, iinc
    // =============================================================================

    mod local_variables {
        use super::*;

        // Test iload/istore basic
        #[test]
        fn jit_local_int_basic() {
            assert_jit_matches_interpreter(
                "jit_local_int_basic",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int a = 10;
        int b = 20;
        int c = a + b;
        System.out.println(c);
    }
}
"#,
                )],
            );
        }

        // Test multiple local variables
        #[test]
        fn jit_local_multiple() {
            assert_jit_matches_interpreter(
                "jit_local_multiple",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int a = 1;
        int b = 2;
        int c = 3;
        int d = 4;
        int e = a + b + c + d;
        System.out.println(e);
    }
}
"#,
                )],
            );
        }

        // Test istore with index > 3 (uses wide index)
        #[test]
        fn jit_local_high_index() {
            assert_jit_matches_interpreter(
                "jit_local_high_index",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int[] arr = {10, 20, 30};
        int sum = 0;
        for (int i = 0; i < arr.length; i++) {
            int val = arr[i];
            sum = sum + val;
        }
        System.out.println(sum);
    }
}
"#,
                )],
            );
        }

        // Test iinc (increment)
        #[test]
        fn jit_iinc() {
            assert_jit_matches_interpreter(
                "jit_iinc",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int count = 0;
        for (int i = 0; i < 5; i++) {
            count++;
        }
        System.out.println(count);
    }
}
"#,
                )],
            );
        }

        // Test iinc with negative increment
        #[test]
        fn jit_iinc_negative() {
            assert_jit_matches_interpreter(
                "jit_iinc_negative",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int count = 10;
        for (int i = 0; i < 3; i++) {
            count -= 2;
        }
        System.out.println(count);
    }
}
"#,
                )],
            );
        }

        // Test aload/astore (reference locals)
        #[test]
        fn jit_local_reference() {
            assert_jit_matches_interpreter(
                "jit_local_reference",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        String a = "hello";
        String b = "world";
        System.out.println(a);
        System.out.println(b);
    }
}
"#,
                )],
            );
        }

        // Test local variable redefinition
        #[test]
        fn jit_local_redefine() {
            assert_jit_matches_interpreter(
                "jit_local_redefine",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int x = 5;
        System.out.println(x);
        x = 10;
        System.out.println(x);
        x = x + 1;
        System.out.println(x);
    }
}
"#,
                )],
            );
        }
    }

    // =============================================================================
    // ARITHMETIC OPERATIONS - Test iadd, isub, imul, idiv, irem, ineg, etc.
    // =============================================================================

    mod arithmetic {
        use super::*;

        // Test iadd
        #[test]
        fn jit_iadd() {
            assert_jit_matches_interpreter(
                "jit_iadd",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        System.out.println(1 + 2);
        System.out.println(100 + 200);
        int a = 10, b = 20;
        System.out.println(a + b);
    }
}
"#,
                )],
            );
        }

        // Test isub
        #[test]
        fn jit_isub() {
            assert_jit_matches_interpreter(
                "jit_isub",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        System.out.println(5 - 3);
        System.out.println(100 - 200);
        int a = 10, b = 20;
        System.out.println(a - b);
    }
}
"#,
                )],
            );
        }

        // Test imul
        #[test]
        fn jit_imul() {
            assert_jit_matches_interpreter(
                "jit_imul",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        System.out.println(3 * 4);
        System.out.println(100 * 200);
        int a = 10, b = 20;
        System.out.println(a * b);
    }
}
"#,
                )],
            );
        }

        // Test idiv
        #[test]
        fn jit_idiv() {
            assert_jit_matches_interpreter(
                "jit_idiv",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        System.out.println(20 / 4);
        System.out.println(100 / 3);
        int a = 100, b = 7;
        System.out.println(a / b);
    }
}
"#,
                )],
            );
        }

        // Test irem (remainder)
        #[test]
        fn jit_irem() {
            assert_jit_matches_interpreter(
                "jit_irem",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        System.out.println(20 % 4);
        System.out.println(100 % 7);
        int a = 100, b = 7;
        System.out.println(a % b);
    }
}
"#,
                )],
            );
        }

        // Test ineg (negation)
        #[test]
        fn jit_ineg() {
            assert_jit_matches_interpreter(
                "jit_ineg",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        System.out.println(-5);
        System.out.println(-(10 - 20));
        int a = -100;
        System.out.println(-a);
    }
}
"#,
                )],
            );
        }

        // Test combined arithmetic
        #[test]
        fn jit_arithmetic_combined() {
            assert_jit_matches_interpreter(
                "jit_arithmetic_combined",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int result = 1 + 2 * 3 - 4 / 2;
        System.out.println(result);
        result = (1 + 2) * (3 - 4) / 2 + 10 % 3;
        System.out.println(result);
    }
}
"#,
                )],
            );
        }
    }

    // =============================================================================
    // COMPARE AND BRANCH - Test if_icmpeq, if_icmpne, if_icmplt, etc.
    // =============================================================================

    mod compare_branch {
        use super::*;

        // Test if_icmpeq
        #[test]
        fn jit_if_icmpeq() {
            assert_jit_matches_interpreter(
                "jit_if_icmpeq",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int a = 10;
        int b = 10;
        int c = 20;
        if (a == b) System.out.println("equal");
        else System.out.println("not equal");
        if (a == c) System.out.println("equal");
        else System.out.println("not equal");
    }
}
"#,
                )],
            );
        }

        // Test if_icmpne
        #[test]
        fn jit_if_icmpne() {
            assert_jit_matches_interpreter(
                "jit_if_icmpne",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int a = 10;
        int b = 20;
        if (a != b) System.out.println("different");
        else System.out.println("same");
    }
}
"#,
                )],
            );
        }

        // Test if_icmplt
        #[test]
        fn jit_if_icmplt() {
            assert_jit_matches_interpreter(
                "jit_if_icmplt",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int a = 5;
        int b = 10;
        if (a < b) System.out.println("a < b");
        else System.out.println("a >= b");
    }
}
"#,
                )],
            );
        }

        // Test if_icmpgt
        #[test]
        fn jit_if_icmpgt() {
            assert_jit_matches_interpreter(
                "jit_if_icmpgt",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int a = 10;
        int b = 5;
        if (a > b) System.out.println("a > b");
        else System.out.println("a <= b");
    }
}
"#,
                )],
            );
        }

        // Test if_icmple
        #[test]
        fn jit_if_icmple() {
            assert_jit_matches_interpreter(
                "jit_if_icmple",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int a = 5;
        int b = 5;
        if (a <= b) System.out.println("a <= b");
        else System.out.println("a > b");
    }
}
"#,
                )],
            );
        }

        // Test if_icmpge
        #[test]
        fn jit_if_icmpge() {
            assert_jit_matches_interpreter(
                "jit_if_icmpge",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int a = 10;
        int b = 5;
        if (a >= b) System.out.println("a >= b");
        else System.out.println("a < b");
    }
}
"#,
                )],
            );
        }

        // Test if_acmpeq (reference compare)
        #[test]
        fn jit_if_acmpeq() {
            assert_jit_matches_interpreter(
                "jit_if_acmpeq",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        String a = new String("hello");
        String b = new String("hello");
        String c = a;
        if (a == b) System.out.println("a == b");
        else System.out.println("a != b");
        if (a == c) System.out.println("a == c");
        else System.out.println("a != c");
    }
}
"#,
                )],
            );
        }

        // Test if_acmpne
        #[test]
        fn jit_if_acmpne() {
            assert_jit_matches_interpreter(
                "jit_if_acmpne",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        String a = new String("hello");
        String b = new String("world");
        if (a != b) System.out.println("different");
        else System.out.println("same");
    }
}
"#,
                )],
            );
        }

        // Test ifnull / ifnonnull
        #[test]
        fn jit_ifnull() {
            assert_jit_matches_interpreter(
                "jit_ifnull",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        String a = null;
        String b = "hello";
        if (a == null) System.out.println("a is null");
        else System.out.println("a is not null");
        if (b == null) System.out.println("b is null");
        else System.out.println("b is not null");
    }
}
"#,
                )],
            );
        }

        #[test]
        fn jit_ifnull_in_helper_method() {
            let root = compile_java(
                "jit_ifnull_in_helper_method",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    static int check(String value) {
        if (value == null) return 10;
        if (value != null) return 20;
        return -1;
    }
    public static void main(String[] args) {
        System.out.println(check(null));
        System.out.println(check("hello"));
    }
}
"#,
                )],
            );
            let interp = run_with_jit_threshold(&root, "demo.Main", u32::MAX);
            let jit = run_with_jit_threshold(&root, "demo.Main", 1);

            assert_eq!(jit.output, interp.output);
            assert!(
                jit.jit_executions >= 2,
                "expected top-level JIT/deopt plus compiled check() to reach JIT, got {}",
                jit.jit_executions
            );
        }

        // Test ifeq, ifne, iflt, ifge, ifgt, ifle (compare with zero)
        #[test]
        fn jit_compare_with_zero() {
            assert_jit_matches_interpreter(
                "jit_compare_with_zero",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int a = 0;
        int b = 1;
        int c = -1;
        if (a == 0) System.out.println("a is zero");
        if (b != 0) System.out.println("b is not zero");
        if (c < 0) System.out.println("c is negative");
        if (c > 0) System.out.println("c is positive");
    }
}
"#,
                )],
            );
        }

        // Test goto
        #[test]
        fn jit_goto() {
            assert_jit_matches_interpreter(
                "jit_goto",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int count = 0;
        loop:
        for (int i = 0; i < 10; i++) {
            count++;
            if (count >= 5) break loop;
        }
        System.out.println(count);
    }
}
"#,
                )],
            );
        }
    }

    // =============================================================================
    // LOOPS - Test for loops, while loops, do-while loops
    // =============================================================================

    mod loops {
        use super::*;

        // Test simple for loop
        #[test]
        fn jit_for_loop() {
            assert_jit_matches_interpreter(
                "jit_for_loop",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int sum = 0;
        for (int i = 1; i <= 5; i++) {
            sum += i;
        }
        System.out.println(sum);
    }
}
"#,
                )],
            );
        }

        // Test while loop
        #[test]
        fn jit_while_loop() {
            assert_jit_matches_interpreter(
                "jit_while_loop",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int count = 0;
        while (count < 5) {
            count++;
        }
        System.out.println(count);
    }
}
"#,
                )],
            );
        }

        // Test nested loops
        #[test]
        fn jit_nested_loops() {
            assert_jit_matches_interpreter(
                "jit_nested_loops",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int sum = 0;
        for (int i = 0; i < 3; i++) {
            for (int j = 0; j < 3; j++) {
                sum++;
            }
        }
        System.out.println(sum);
    }
}
"#,
                )],
            );
        }

        // Test loop with break
        #[test]
        fn jit_loop_break() {
            assert_jit_matches_interpreter(
                "jit_loop_break",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int count = 0;
        for (int i = 0; i < 100; i++) {
            if (i == 10) break;
            count++;
        }
        System.out.println(count);
    }
}
"#,
                )],
            );
        }

        // Test loop with continue
        #[test]
        fn jit_loop_continue() {
            assert_jit_matches_interpreter(
                "jit_loop_continue",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int count = 0;
        for (int i = 0; i < 10; i++) {
            if (i % 2 == 0) continue;
            count++;
        }
        System.out.println(count);
    }
}
"#,
                )],
            );
        }

        // Test countdown loop
        #[test]
        fn jit_countdown() {
            assert_jit_matches_interpreter(
                "jit_countdown",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        for (int i = 5; i > 0; i--) {
            System.out.println(i);
        }
        System.out.println("done");
    }
}
"#,
                )],
            );
        }
    }

    // =============================================================================
    // STACK OPERATIONS - Test dup, dup2, pop, swap, etc.
    // =============================================================================

    mod stack_operations {
        use super::*;

        // Test dup
        #[test]
        fn jit_dup() {
            assert_jit_matches_interpreter(
                "jit_dup",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int a = 10;
        int b = 20;
        int sum1 = a + b;
        int sum2 = a + b;
        System.out.println(sum1);
        System.out.println(sum2);
    }
}
"#,
                )],
            );
        }

        // Test dup_x1
        #[test]
        fn jit_dup_x1() {
            assert_jit_matches_interpreter(
                "jit_dup_x1",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int a = 1, b = 2;
        int c = a + b;
        System.out.println(a);
        System.out.println(b);
        System.out.println(c);
    }
}
"#,
                )],
            );
        }

        // Test dup2
        #[test]
        fn jit_dup2() {
            assert_jit_matches_interpreter(
                "jit_dup2",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        long a = 100L;
        long b = 200L;
        System.out.println(a);
        System.out.println(b);
    }
}
"#,
                )],
            );
        }

        // Test swap
        #[test]
        fn jit_swap() {
            assert_jit_matches_interpreter(
                "jit_swap",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int a = 1, b = 2;
        int temp = a;
        a = b;
        b = temp;
        System.out.println(a);
        System.out.println(b);
    }
}
"#,
                )],
            );
        }

        // Test pop
        #[test]
        fn jit_pop() {
            assert_jit_matches_interpreter(
                "jit_pop",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int a = 1;
        int b = 2;
        int c = a + b;
        System.out.println(c);
    }
}
"#,
                )],
            );
        }

        #[test]
        fn jit_pop2_long_return_in_helper() {
            assert_jit_matches_interpreter(
                "jit_pop2_long_return_in_helper",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    static long foo() { return 42L; }
    static int bar() { return 7; }
    public static void main(String[] args) {
        foo();
        int value = bar();
        System.out.println(value);
    }
}
"#,
                )],
            );
        }

        #[test]
        fn jit_dup2_long_post_increment_helper() {
            assert_jit_matches_interpreter(
                "jit_dup2_long_post_increment_helper",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    static long test() {
        long x = 1L;
        return x++;
    }
    public static void main(String[] args) {
        System.out.println(test());
    }
}
"#,
                )],
            );
        }
    }

    // =============================================================================
    // METHOD INVOCATION - Test invokestatic, invokevirtual, invokespecial
    // =============================================================================

    mod method_invocation {
        use super::*;

        // Test static method call
        #[test]
        fn jit_invokestatic() {
            assert_jit_matches_interpreter(
                "jit_invokestatic",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    static int add(int a, int b) {
        return a + b;
    }
    public static void main(String[] args) {
        System.out.println(add(1, 2));
        System.out.println(add(10, 20));
    }
}
"#,
                )],
            );
        }

        // Test static method with multiple calls
        #[test]
        fn jit_invokestatic_multiple() {
            assert_jit_matches_interpreter(
                "jit_invokestatic_multiple",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    static int mul(int a, int b) { return a * b; }
    static int add(int a, int b) { return a + b; }
    public static void main(String[] args) {
        int r1 = mul(add(1, 2), add(3, 4));
        System.out.println(r1);
    }
}
"#,
                )],
            );
        }

        // Test instance method call
        #[test]
        fn jit_invokevirtual() {
            assert_jit_matches_interpreter(
                "jit_invokevirtual",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        String s = "hello world";
        System.out.println(s.length());
        System.out.println(s.substring(0, 5));
    }
}
"#,
                )],
            );
        }

        // Test recursive method call
        #[test]
        fn jit_recursive() {
            assert_jit_matches_interpreter(
                "jit_recursive",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    static int factorial(int n) {
        if (n <= 1) return 1;
        return n * factorial(n - 1);
    }
    public static void main(String[] args) {
        System.out.println(factorial(5));
        System.out.println(factorial(10));
    }
}
"#,
                )],
            );
        }

        // Test method with many parameters
        #[test]
        fn jit_many_params() {
            assert_jit_matches_interpreter(
                "jit_many_params",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    static int sum(int a, int b, int c, int d, int e) {
        return a + b + c + d + e;
    }
    public static void main(String[] args) {
        System.out.println(sum(1, 2, 3, 4, 5));
    }
}
"#,
                )],
            );
        }

        // Test method returning long
        #[test]
        fn jit_return_long() {
            assert_jit_matches_interpreter(
                "jit_return_long",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    static long mul(long a, long b) { return a * b; }
    public static void main(String[] args) {
        System.out.println(mul(100L, 200L));
    }
}
"#,
                )],
            );
        }
    }

    // =============================================================================
    // OBJECT CREATION AND FIELD ACCESS
    // =============================================================================

    mod object_creation {
        use super::*;

        // Test new object creation
        #[test]
        fn jit_new_object() {
            assert_jit_matches_interpreter(
                "jit_new_object",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        Object o = new Object();
        System.out.println(o != null);
    }
}
"#,
                )],
            );
        }

        // Test instance field access
        #[test]
        fn jit_instance_field() {
            assert_jit_matches_interpreter(
                "jit_instance_field",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    int value = 10;
    public static void main(String[] args) {
        Main m = new Main();
        System.out.println(m.value);
        m.value = 20;
        System.out.println(m.value);
    }
}
"#,
                )],
            );
        }

        // Test static field access
        #[test]
        fn jit_static_field() {
            assert_jit_matches_interpreter(
                "jit_static_field",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    static int counter = 0;
    public static void main(String[] args) {
        counter++;
        counter++;
        counter++;
        System.out.println(counter);
    }
}
"#,
                )],
            );
        }

        // Test object construction with fields
        #[test]
        fn jit_constructor() {
            assert_jit_matches_interpreter(
                "jit_constructor",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    int x;
    int y;
    Main(int x, int y) {
        this.x = x;
        this.y = y;
    }
    public static void main(String[] args) {
        Main p = new Main(10, 20);
        System.out.println(p.x);
        System.out.println(p.y);
    }
}
"#,
                )],
            );
        }
    }

    // =============================================================================
    // TYPE CONVERSIONS - Test i2l, i2f, l2i, etc.
    // =============================================================================

    mod type_conversions {
        use super::*;

        // Test i2l (int to long)
        #[test]
        fn jit_i2l() {
            assert_jit_matches_interpreter(
                "jit_i2l",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int a = 100;
        long b = a;
        System.out.println(b);
    }
}
"#,
                )],
            );
        }

        // Test l2i (long to int)
        #[test]
        fn jit_l2i() {
            assert_jit_matches_interpreter(
                "jit_l2i",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        long a = 100L;
        int b = (int) a;
        System.out.println(b);
    }
}
"#,
                )],
            );
        }

        // Test i2f (int to float)
        #[test]
        fn jit_i2f() {
            assert_jit_matches_interpreter(
                "jit_i2f",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int a = 100;
        float b = a;
        System.out.println(b);
    }
}
"#,
                )],
            );
        }

        // Test i2d (int to double)
        #[test]
        fn jit_i2d() {
            assert_jit_matches_interpreter(
                "jit_i2d",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int a = 100;
        double b = a;
        System.out.println(b);
    }
}
"#,
                )],
            );
        }
    }

    // =============================================================================
    // TABLESWITCH AND LOOKUPSWITCH
    // =============================================================================

    mod switch_statements {
        use super::*;

        // Test tableswitch
        #[test]
        fn jit_tableswitch() {
            assert_jit_matches_interpreter(
                "jit_tableswitch",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int day = 3;
        String name;
        switch(day) {
            case 1: name = "Monday"; break;
            case 2: name = "Tuesday"; break;
            case 3: name = "Wednesday"; break;
            case 4: name = "Thursday"; break;
            case 5: name = "Friday"; break;
            default: name = "Weekend";
        }
        System.out.println(name);
    }
}
"#,
                )],
            );
        }

        // Test lookupswitch
        #[test]
        fn jit_lookupswitch() {
            assert_jit_matches_interpreter(
                "jit_lookupswitch",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int code = 100;
        String result;
        switch(code) {
            case 10: result = "ten"; break;
            case 50: result = "fifty"; break;
            case 100: result = "hundred"; break;
            case 500: result = "five hundred"; break;
            default: result = "other";
        }
        System.out.println(result);
    }
}
"#,
                )],
            );
        }

        // Test switch with sparse cases
        #[test]
        fn jit_sparse_switch() {
            assert_jit_matches_interpreter(
                "jit_sparse_switch",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int code = 0;
        switch(code) {
            case 0: System.out.println("zero"); break;
            case 1: System.out.println("one"); break;
            case 10: System.out.println("ten"); break;
            case 100: System.out.println("hundred"); break;
            case 1000: System.out.println("thousand"); break;
            default: System.out.println("other");
        }
    }
}
"#,
                )],
            );
        }

        #[test]
        fn jit_tableswitch_in_helper_method() {
            assert_jit_matches_interpreter(
                "jit_tableswitch_in_helper_method",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    static int score(int day) {
        switch(day) {
            case 1: return 10;
            case 2: return 20;
            case 3: return 30;
            case 4: return 40;
            case 5: return 50;
            default: return -1;
        }
    }

    public static void main(String[] args) {
        int sum = 0;
        for (int i = 1; i <= 5; i++) {
            sum += score(i);
        }
        System.out.println(sum);
    }
}
"#,
                )],
            );
        }

        #[test]
        fn jit_lookupswitch_in_helper_method() {
            assert_jit_matches_interpreter(
                "jit_lookupswitch_in_helper_method",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    static int map(int code) {
        switch(code) {
            case 10: return 1;
            case 50: return 5;
            case 100: return 10;
            case 500: return 50;
            default: return -1;
        }
    }

    public static void main(String[] args) {
        int total = map(10) + map(50) + map(100) + map(500);
        System.out.println(total);
    }
}
"#,
                )],
            );
        }
    }

    // =============================================================================
    // EXCEPTION HANDLING
    // =============================================================================

    mod exception_handling {
        use super::*;

        // Test try-catch
        #[test]
        fn jit_try_catch() {
            assert_jit_matches_interpreter(
                "jit_try_catch",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int result = 0;
        try {
            int[] arr = new int[2];
            result = arr[5];
        } catch (ArrayIndexOutOfBoundsException e) {
            result = -1;
        }
        System.out.println(result);
    }
}
"#,
                )],
            );
        }

        // Test try-catch-finally
        #[test]
        fn jit_try_catch_finally() {
            assert_jit_matches_interpreter(
                "jit_try_catch_finally",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int result = 0;
        try {
            result = 10;
        } catch (Exception e) {
            result = -1;
        } finally {
            result += 1;
        }
        System.out.println(result);
    }
}
"#,
                )],
            );
        }

        #[test]
        fn jit_try_catch_in_helper_method() {
            let root = compile_java(
                "jit_try_catch_in_helper_method",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    static int safeLoad() {
        try {
            int[] arr = new int[2];
            return arr[5];
        } catch (ArrayIndexOutOfBoundsException e) {
            return -1;
        }
    }
    public static void main(String[] args) {
        System.out.println(safeLoad());
    }
}
"#,
                )],
            );
            let interp = run_with_jit_threshold(&root, "demo.Main", u32::MAX);
            let jit = run_with_jit_threshold(&root, "demo.Main", 1);

            assert_eq!(jit.output, interp.output);
            assert!(
                jit.jit_executions >= 2,
                "expected top-level JIT/deopt plus compiled safeLoad() to reach JIT, got {}",
                jit.jit_executions
            );
        }

        #[test]
        fn jit_checkcast_failure_resumes_via_interpreter() {
            let root = compile_java(
                "jit_checkcast_failure_resumes_via_interpreter",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    static int castCheck() {
        Object value = new Object();
        try {
            String text = (String) value;
            return 99;
        } catch (ClassCastException e) {
            return -7;
        }
    }
    public static void main(String[] args) {
        System.out.println(castCheck());
    }
}
"#,
                )],
            );
            let interp = run_with_jit_threshold(&root, "demo.Main", u32::MAX);
            let jit = run_with_jit_threshold(&root, "demo.Main", 1);

            assert_eq!(jit.output, interp.output);
            assert_eq!(jit.output, vec!["-7".to_string()]);
            assert!(
                jit.jit_executions >= 2,
                "expected top-level JIT/deopt plus compiled castCheck() to reach JIT, got {}",
                jit.jit_executions
            );
        }

        #[test]
        fn repeated_classcast_deopts_eventually_abandon_jit() {
            let root = compile_java(
                "repeated_classcast_deopts_eventually_abandon_jit",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    static int castCheck() {
        Object value = new Object();
        try {
            String text = (String) value;
            return 99;
        } catch (ClassCastException e) {
            return -7;
        }
    }
    public static void main(String[] args) {
        System.out.println(castCheck());
    }
}
"#,
                )],
            );

            let options = LaunchOptions::new(root.path(), "demo.Main", vec![]);
            let mut vm = Vm::new().expect("failed to create VM");
            vm.set_class_path(options.class_path.clone());
            vm.set_jit_thresholds(1, 1);
            let source =
                launcher::resolve_class_path(&options.class_path, "demo.Main").unwrap();
            let method = launcher::load_main_method(&source, "demo.Main", &[], &mut vm).unwrap();

            vm.execute(method.clone()).unwrap();
            assert_eq!(vm.take_output(), vec!["-7".to_string()]);
            assert_eq!(
                vm.jit_deopt_count("demo/Main", "castCheck", "()I", DeoptReason::ClassCast),
                1
            );
            assert_eq!(
                vm.jit_interpreter_only_reason("demo/Main", "castCheck", "()I"),
                None
            );
            let (deopt_pc, deopt_hits) = vm
                .jit_hottest_deopt_site("demo/Main", "castCheck", "()I")
                .expect("castCheck() should expose its first deopt site");
            assert_eq!(deopt_hits, 1);
            assert_eq!(
                vm.jit_deopt_site_count(
                    "demo/Main",
                    "castCheck",
                    "()I",
                    deopt_pc,
                    DeoptReason::ClassCast
                ),
                1
            );

            vm.execute(method.clone()).unwrap();
            assert_eq!(vm.take_output(), vec!["-7".to_string()]);
            assert_eq!(
                vm.jit_deopt_count("demo/Main", "castCheck", "()I", DeoptReason::ClassCast),
                1
            );
            assert_eq!(
                vm.jit_hottest_deopt_site("demo/Main", "castCheck", "()I"),
                Some((deopt_pc, 2))
            );
            assert_eq!(
                vm.jit_interpreter_only_reason("demo/Main", "castCheck", "()I"),
                None
            );
            assert_eq!(
                vm.jit_deopt_count("demo/Main", "castCheck", "()I", DeoptReason::SiteFallback),
                1
            );

            vm.execute(method).unwrap();
            assert_eq!(vm.take_output(), vec!["-7".to_string()]);
            assert_eq!(
                vm.jit_deopt_count("demo/Main", "castCheck", "()I", DeoptReason::ClassCast),
                1
            );
            assert_eq!(
                vm.jit_deopt_count("demo/Main", "castCheck", "()I", DeoptReason::SiteFallback),
                2
            );
            assert_eq!(
                vm.jit_hottest_deopt_site("demo/Main", "castCheck", "()I"),
                Some((deopt_pc, 3)),
                "the same checkcast bytecode site should now be compiled as a planned fallback"
            );
            assert_eq!(
                vm.jit_interpreter_only_reason("demo/Main", "castCheck", "()I"),
                None
            );
        }

        #[test]
        fn repeated_nullcheck_field_deopts_recompile_to_site_fallback() {
            let root = compile_java(
                "repeated_nullcheck_field_deopts_recompile_to_site_fallback",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    static final class Holder {
        int value = 7;
    }
    static int readNullField() {
        try {
            Holder holder = null;
            return holder.value;
        } catch (NullPointerException e) {
            return -1;
        }
    }
    public static void main(String[] args) {
        System.out.println(readNullField());
    }
}
"#,
                )],
            );

            let options = LaunchOptions::new(root.path(), "demo.Main", vec![]);
            let mut vm = Vm::new().expect("failed to create VM");
            vm.set_class_path(options.class_path.clone());
            vm.set_jit_thresholds(1, 1);
            let source =
                launcher::resolve_class_path(&options.class_path, "demo.Main").unwrap();
            let method = launcher::load_main_method(&source, "demo.Main", &[], &mut vm).unwrap();

            vm.execute(method.clone()).unwrap();
            assert_eq!(vm.take_output(), vec!["-1".to_string()]);
            assert_eq!(
                vm.jit_deopt_count("demo/Main", "readNullField", "()I", DeoptReason::NullCheck),
                1
            );
            assert_eq!(
                vm.jit_deopt_count(
                    "demo/Main",
                    "readNullField",
                    "()I",
                    DeoptReason::SiteFallback
                ),
                0
            );
            let (deopt_pc, deopt_hits) = vm
                .jit_hottest_deopt_site("demo/Main", "readNullField", "()I")
                .expect("readNullField() should expose its first deopt site");
            assert_eq!(deopt_hits, 1);

            vm.execute(method.clone()).unwrap();
            assert_eq!(vm.take_output(), vec!["-1".to_string()]);
            assert_eq!(
                vm.jit_deopt_count("demo/Main", "readNullField", "()I", DeoptReason::NullCheck),
                1
            );
            assert_eq!(
                vm.jit_deopt_count(
                    "demo/Main",
                    "readNullField",
                    "()I",
                    DeoptReason::SiteFallback
                ),
                1
            );
            assert_eq!(
                vm.jit_hottest_deopt_site("demo/Main", "readNullField", "()I"),
                Some((deopt_pc, 2))
            );
            assert_eq!(
                vm.jit_interpreter_only_reason("demo/Main", "readNullField", "()I"),
                None
            );

            vm.execute(method).unwrap();
            assert_eq!(vm.take_output(), vec!["-1".to_string()]);
            assert_eq!(
                vm.jit_deopt_count("demo/Main", "readNullField", "()I", DeoptReason::NullCheck),
                1
            );
            assert_eq!(
                vm.jit_deopt_count(
                    "demo/Main",
                    "readNullField",
                    "()I",
                    DeoptReason::SiteFallback
                ),
                2
            );
            assert_eq!(
                vm.jit_hottest_deopt_site("demo/Main", "readNullField", "()I"),
                Some((deopt_pc, 3))
            );
            assert_eq!(
                vm.jit_interpreter_only_reason("demo/Main", "readNullField", "()I"),
                None
            );
        }

        #[test]
        fn repeated_nullcheck_invokevirtual_deopts_recompile_to_site_fallback() {
            let root = compile_java(
                "repeated_nullcheck_invokevirtual_deopts_recompile_to_site_fallback",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    static final class Holder {
        int value() { return 7; }
    }
    static int callNullVirtual() {
        try {
            Holder holder = null;
            return holder.value();
        } catch (NullPointerException e) {
            return -1;
        }
    }
    public static void main(String[] args) {
        System.out.println(callNullVirtual());
    }
}
"#,
                )],
            );

            let options = LaunchOptions::new(root.path(), "demo.Main", vec![]);
            let mut vm = Vm::new().expect("failed to create VM");
            vm.set_class_path(options.class_path.clone());
            vm.set_jit_thresholds(1, 1);
            let source =
                launcher::resolve_class_path(&options.class_path, "demo.Main").unwrap();
            let method = launcher::load_main_method(&source, "demo.Main", &[], &mut vm).unwrap();

            vm.execute(method.clone()).unwrap();
            assert_eq!(vm.take_output(), vec!["-1".to_string()]);
            assert_eq!(
                vm.jit_deopt_count("demo/Main", "callNullVirtual", "()I", DeoptReason::NullCheck),
                1
            );
            assert_eq!(
                vm.jit_deopt_count(
                    "demo/Main",
                    "callNullVirtual",
                    "()I",
                    DeoptReason::SiteFallback
                ),
                0
            );
            let (deopt_pc, deopt_hits) = vm
                .jit_hottest_deopt_site("demo/Main", "callNullVirtual", "()I")
                .expect("callNullVirtual() should expose its first deopt site");
            assert_eq!(deopt_hits, 1);

            vm.execute(method.clone()).unwrap();
            assert_eq!(vm.take_output(), vec!["-1".to_string()]);
            assert_eq!(
                vm.jit_deopt_count("demo/Main", "callNullVirtual", "()I", DeoptReason::NullCheck),
                1
            );
            assert_eq!(
                vm.jit_deopt_count(
                    "demo/Main",
                    "callNullVirtual",
                    "()I",
                    DeoptReason::SiteFallback
                ),
                1
            );
            assert_eq!(
                vm.jit_hottest_deopt_site("demo/Main", "callNullVirtual", "()I"),
                Some((deopt_pc, 2))
            );
            assert_eq!(
                vm.jit_interpreter_only_reason("demo/Main", "callNullVirtual", "()I"),
                None
            );

            vm.execute(method).unwrap();
            assert_eq!(vm.take_output(), vec!["-1".to_string()]);
            assert_eq!(
                vm.jit_deopt_count("demo/Main", "callNullVirtual", "()I", DeoptReason::NullCheck),
                1
            );
            assert_eq!(
                vm.jit_deopt_count(
                    "demo/Main",
                    "callNullVirtual",
                    "()I",
                    DeoptReason::SiteFallback
                ),
                2
            );
            assert_eq!(
                vm.jit_hottest_deopt_site("demo/Main", "callNullVirtual", "()I"),
                Some((deopt_pc, 3))
            );
            assert_eq!(
                vm.jit_interpreter_only_reason("demo/Main", "callNullVirtual", "()I"),
                None
            );
        }

        #[test]
        fn repeated_exception_invokestatic_records_exception_reason() {
            let root = compile_java(
                "repeated_exception_invokestatic_records_exception_reason",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    static int alwaysThrow() {
        throw new RuntimeException("boom");
    }
    static int callStaticThrow() {
        try {
            return alwaysThrow();
        } catch (RuntimeException e) {
            return -1;
        }
    }
    public static void main(String[] args) {
        System.out.println(callStaticThrow());
    }
}
"#,
                )],
            );

            let options = LaunchOptions::new(root.path(), "demo.Main", vec![]);
            let mut vm = Vm::new().expect("failed to create VM");
            vm.set_class_path(options.class_path.clone());
            vm.set_jit_thresholds(1, 1);
            let source =
                launcher::resolve_class_path(&options.class_path, "demo.Main").unwrap();
            let method = launcher::load_main_method(&source, "demo.Main", &[], &mut vm).unwrap();

            vm.execute(method.clone()).unwrap();
            assert_eq!(vm.take_output(), vec!["-1".to_string()]);
            assert_eq!(
                vm.jit_deopt_count(
                    "demo/Main",
                    "callStaticThrow",
                    "()I",
                    DeoptReason::Exception
                ),
                1
            );
            assert_eq!(
                vm.jit_deopt_count(
                    "demo/Main",
                    "callStaticThrow",
                    "()I",
                    DeoptReason::SiteFallback
                ),
                0
            );
            let (deopt_pc, deopt_hits) = vm
                .jit_hottest_deopt_site("demo/Main", "callStaticThrow", "()I")
                .expect("callStaticThrow() should expose its first deopt site");
            assert_eq!(deopt_hits, 1);

            vm.execute(method.clone()).unwrap();
            assert_eq!(vm.take_output(), vec!["-1".to_string()]);
            assert_eq!(
                vm.jit_deopt_count(
                    "demo/Main",
                    "callStaticThrow",
                    "()I",
                    DeoptReason::Exception
                ),
                2
            );
            assert_eq!(
                vm.jit_deopt_count(
                    "demo/Main",
                    "callStaticThrow",
                    "()I",
                    DeoptReason::SiteFallback
                ),
                0
            );
            assert_eq!(
                vm.jit_hottest_deopt_site("demo/Main", "callStaticThrow", "()I"),
                Some((deopt_pc, 2))
            );
            assert_eq!(
                vm.jit_interpreter_only_reason("demo/Main", "callStaticThrow", "()I"),
                None
            );

            vm.execute(method).unwrap();
            assert_eq!(vm.take_output(), vec!["-1".to_string()]);
            assert_eq!(
                vm.jit_deopt_count(
                    "demo/Main",
                    "callStaticThrow",
                    "()I",
                    DeoptReason::Exception
                ),
                3
            );
            assert_eq!(
                vm.jit_deopt_count(
                    "demo/Main",
                    "callStaticThrow",
                    "()I",
                    DeoptReason::SiteFallback
                ),
                0
            );
            assert_eq!(
                vm.jit_hottest_deopt_site("demo/Main", "callStaticThrow", "()I"),
                Some((deopt_pc, 3))
            );
            assert_eq!(
                vm.jit_interpreter_only_reason("demo/Main", "callStaticThrow", "()I"),
                None
            );
        }

        #[test]
        fn repeated_exception_getstatic_records_exception_reason() {
            let root = compile_java(
                "repeated_exception_getstatic_records_exception_reason",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    static final class Bomb {
        static int VALUE = explode();
        static int explode() {
            throw new RuntimeException("boom");
        }
    }
    static int readBomb() {
        try {
            return Bomb.VALUE;
        } catch (Throwable t) {
            return -1;
        }
    }
    public static void main(String[] args) {
        System.out.println(readBomb());
    }
}
"#,
                )],
            );

            let options = LaunchOptions::new(root.path(), "demo.Main", vec![]);
            let mut vm = Vm::new().expect("failed to create VM");
            vm.set_class_path(options.class_path.clone());
            vm.set_jit_thresholds(1, 1);
            let source =
                launcher::resolve_class_path(&options.class_path, "demo.Main").unwrap();
            let method = launcher::load_main_method(&source, "demo.Main", &[], &mut vm).unwrap();

            vm.execute(method.clone()).unwrap();
            assert_eq!(vm.take_output(), vec!["-1".to_string()]);
            assert_eq!(
                vm.jit_deopt_count("demo/Main", "readBomb", "()I", DeoptReason::Exception),
                1
            );
            assert_eq!(
                vm.jit_deopt_count("demo/Main", "readBomb", "()I", DeoptReason::SiteFallback),
                0
            );
            let (deopt_pc, deopt_hits) = vm
                .jit_hottest_deopt_site("demo/Main", "readBomb", "()I")
                .expect("readBomb() should expose its first deopt site");
            assert_eq!(deopt_hits, 1);

            vm.execute(method.clone()).unwrap();
            assert_eq!(vm.take_output(), vec!["-1".to_string()]);
            assert_eq!(
                vm.jit_deopt_count("demo/Main", "readBomb", "()I", DeoptReason::Exception),
                2
            );
            assert_eq!(
                vm.jit_deopt_count("demo/Main", "readBomb", "()I", DeoptReason::SiteFallback),
                0
            );
            assert_eq!(
                vm.jit_hottest_deopt_site("demo/Main", "readBomb", "()I"),
                Some((deopt_pc, 2))
            );
            assert_eq!(
                vm.jit_interpreter_only_reason("demo/Main", "readBomb", "()I"),
                None
            );

            vm.execute(method).unwrap();
            assert_eq!(vm.take_output(), vec!["-1".to_string()]);
            assert_eq!(
                vm.jit_deopt_count("demo/Main", "readBomb", "()I", DeoptReason::Exception),
                3
            );
            assert_eq!(
                vm.jit_deopt_count("demo/Main", "readBomb", "()I", DeoptReason::SiteFallback),
                0
            );
            assert_eq!(
                vm.jit_hottest_deopt_site("demo/Main", "readBomb", "()I"),
                Some((deopt_pc, 3))
            );
            assert_eq!(
                vm.jit_interpreter_only_reason("demo/Main", "readBomb", "()I"),
                None
            );
        }
    }

    // =============================================================================
    // COMPLEX ALGORITHMS - Selection sort, insertion sort, etc.
    // =============================================================================

    mod sorting_algorithms {
        use super::*;

        // Test selection sort
        #[test]
        fn jit_selection_sort() {
            assert_jit_matches_interpreter(
                "jit_selection_sort",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    static void selectionSort(int[] arr) {
        int n = arr.length;
        for (int i = 0; i < n - 1; i++) {
            int minIdx = i;
            for (int j = i + 1; j < n; j++) {
                if (arr[j] < arr[minIdx]) {
                    minIdx = j;
                }
            }
            int temp = arr[minIdx];
            arr[minIdx] = arr[i];
            arr[i] = temp;
        }
    }
    public static void main(String[] args) {
        int[] arr = {64, 25, 12, 22, 11};
        selectionSort(arr);
        for (int i = 0; i < arr.length; i++) {
            System.out.println(arr[i]);
        }
    }
}
"#,
                )],
            );
        }

        // Test insertion sort
        #[test]
        fn jit_insertion_sort() {
            assert_jit_matches_interpreter(
                "jit_insertion_sort",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    static void insertionSort(int[] arr) {
        int n = arr.length;
        for (int i = 1; i < n; i++) {
            int key = arr[i];
            int j = i - 1;
            while (j >= 0 && arr[j] > key) {
                arr[j + 1] = arr[j];
                j--;
            }
            arr[j + 1] = key;
        }
    }
    public static void main(String[] args) {
        int[] arr = {64, 34, 25, 12, 22, 11, 90};
        insertionSort(arr);
        for (int i = 0; i < arr.length; i++) {
            System.out.println(arr[i]);
        }
    }
}
"#,
                )],
            );
        }

        // Test linear search
        #[test]
        fn jit_linear_search() {
            assert_jit_matches_interpreter(
                "jit_linear_search",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    static int linearSearch(int[] arr, int target) {
        for (int i = 0; i < arr.length; i++) {
            if (arr[i] == target) return i;
        }
        return -1;
    }
    public static void main(String[] args) {
        int[] arr = {10, 20, 30, 40, 50};
        System.out.println(linearSearch(arr, 30));
        System.out.println(linearSearch(arr, 100));
    }
}
"#,
                )],
            );
        }

        // Test binary search
        #[test]
        fn jit_binary_search() {
            assert_jit_matches_interpreter(
                "jit_binary_search",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    static int binarySearch(int[] arr, int target) {
        int left = 0, right = arr.length - 1;
        while (left <= right) {
            int mid = left + (right - left) / 2;
            if (arr[mid] == target) return mid;
            if (arr[mid] < target) left = mid + 1;
            else right = mid - 1;
        }
        return -1;
    }
    public static void main(String[] args) {
        int[] arr = {10, 20, 30, 40, 50};
        System.out.println(binarySearch(arr, 30));
        System.out.println(binarySearch(arr, 100));
    }
}
"#,
                )],
            );
        }
    }

    // =============================================================================
    // EDGE CASES AND BOUNDARY CONDITIONS
    // =============================================================================

    mod edge_cases {
        use super::*;

        // Test empty array
        #[test]
        fn jit_empty_array() {
            assert_jit_matches_interpreter(
                "jit_empty_array",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int[] arr = new int[0];
        System.out.println(arr.length);
    }
}
"#,
                )],
            );
        }

        // Test single element array
        #[test]
        fn jit_single_element_array() {
            assert_jit_matches_interpreter(
                "jit_single_element_array",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int[] arr = new int[1];
        arr[0] = 42;
        System.out.println(arr[0]);
    }
}
"#,
                )],
            );
        }

        // Test max int value
        #[test]
        fn jit_max_int() {
            assert_jit_matches_interpreter(
                "jit_max_int",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        System.out.println(Integer.MAX_VALUE);
        System.out.println(Integer.MIN_VALUE);
        System.out.println(Integer.MAX_VALUE + 1);
    }
}
"#,
                )],
            );
        }

        // Test zero division handling
        #[test]
        fn jit_zero_division() {
            assert_jit_matches_interpreter(
                "jit_zero_division",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int result = 0;
        try {
            result = 10 / 0;
        } catch (ArithmeticException e) {
            result = -1;
        }
        System.out.println(result);
    }
}
"#,
                )],
            );
        }

        // Test negative array index
        #[test]
        fn jit_negative_array_index() {
            assert_jit_matches_interpreter(
                "jit_negative_array_index",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int result = 0;
        try {
            int[] arr = new int[5];
            result = arr[-1];
        } catch (ArrayIndexOutOfBoundsException e) {
            result = -1;
        }
        System.out.println(result);
    }
}
"#,
                )],
            );
        }

        // Test null array access
        #[test]
        fn jit_null_array_access() {
            assert_jit_matches_interpreter(
                "jit_null_array_access",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int result = 0;
        try {
            int[] arr = null;
            result = arr[0];
        } catch (NullPointerException e) {
            result = -1;
        }
        System.out.println(result);
    }
}
"#,
                )],
            );
        }

        // Test very long method (many instructions)
        #[test]
        fn jit_long_method() {
            assert_jit_matches_interpreter(
                "jit_long_method",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int sum = 0;
        for (int i = 0; i < 100; i++) {
            sum += i;
            sum *= 2;
            sum -= i;
        }
        System.out.println(sum);
    }
}
"#,
                )],
            );
        }

        // Test many local variables
        #[test]
        fn jit_many_locals() {
            assert_jit_matches_interpreter(
                "jit_many_locals",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int a1=1, a2=2, a3=3, a4=4, a5=5;
        int a6=6, a7=7, a8=8, a9=9, a10=10;
        int sum = a1+a2+a3+a4+a5+a6+a7+a8+a9+a10;
        System.out.println(sum);
    }
}
"#,
                )],
            );
        }
    }

    // =============================================================================
    // 2D ARRAYS
    // =============================================================================

    mod two_dimensional_arrays {
        use super::*;

        // Test 2D array creation and access
        #[test]
        fn jit_2d_array_basic() {
            assert_jit_matches_interpreter(
                "jit_2d_array_basic",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int[][] arr = new int[3][3];
        arr[0][0] = 1;
        arr[1][1] = 2;
        arr[2][2] = 3;
        System.out.println(arr[0][0]);
        System.out.println(arr[1][1]);
        System.out.println(arr[2][2]);
    }
}
"#,
                )],
            );
        }

        // Test 2D array iteration
        #[test]
        fn jit_2d_array_iteration() {
            assert_jit_matches_interpreter(
                "jit_2d_array_iteration",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int[][] arr = {{1, 2, 3}, {4, 5, 6}, {7, 8, 9}};
        int sum = 0;
        for (int i = 0; i < 3; i++) {
            for (int j = 0; j < 3; j++) {
                sum += arr[i][j];
            }
        }
        System.out.println(sum);
    }
}
"#,
                )],
            );
        }

        // Test ragged 2D array
        #[test]
        fn jit_ragged_array() {
            assert_jit_matches_interpreter(
                "jit_ragged_array",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int[][] arr = new int[3][];
        arr[0] = new int[2];
        arr[1] = new int[3];
        arr[2] = new int[4];
        arr[0][0] = 1;
        arr[1][2] = 2;
        arr[2][3] = 3;
        System.out.println(arr[0][0]);
        System.out.println(arr[1][2]);
        System.out.println(arr[2][3]);
    }
}
"#,
                )],
            );
        }
    }

    // =============================================================================
    // STRING OPERATIONS
    // =============================================================================

    mod string_operations {
        use super::*;

        // Test string concatenation
        #[test]
        fn jit_string_concat() {
            assert_jit_matches_interpreter(
                "jit_string_concat",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        String a = "Hello";
        String b = "World";
        String c = a + " " + b;
        System.out.println(c);
    }
}
"#,
                )],
            );
        }

        // Test string length
        #[test]
        fn jit_string_length() {
            assert_jit_matches_interpreter(
                "jit_string_length",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        String s = "Hello World";
        System.out.println(s.length());
    }
}
"#,
                )],
            );
        }

        // Test string charAt
        #[test]
        fn jit_string_charat() {
            assert_jit_matches_interpreter(
                "jit_string_charat",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        String s = "ABC";
        System.out.println(s.charAt(0));
        System.out.println(s.charAt(1));
        System.out.println(s.charAt(2));
    }
}
"#,
                )],
            );
        }
    }

    // =============================================================================
    // BOOLEAN OPERATIONS
    // =============================================================================

    mod boolean_operations {
        use super::*;

        // Test boolean AND
        #[test]
        fn jit_boolean_and() {
            assert_jit_matches_interpreter(
                "jit_boolean_and",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        boolean a = true;
        boolean b = false;
        System.out.println(a && b);
        System.out.println(a && !b);
    }
}
"#,
                )],
            );
        }

        // Test boolean OR
        #[test]
        fn jit_boolean_or() {
            assert_jit_matches_interpreter(
                "jit_boolean_or",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        boolean a = true;
        boolean b = false;
        System.out.println(a || b);
        System.out.println(b || b);
    }
}
"#,
                )],
            );
        }

        // Test boolean NOT
        #[test]
        fn jit_boolean_not() {
            assert_jit_matches_interpreter(
                "jit_boolean_not",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        boolean a = true;
        boolean b = false;
        System.out.println(!a);
        System.out.println(!b);
    }
}
"#,
                )],
            );
        }
    }

    // =============================================================================
    // BITWISE OPERATIONS
    // =============================================================================

    mod bitwise_operations {
        use super::*;

        // Test bitwise AND
        #[test]
        fn jit_bitwise_and() {
            assert_jit_matches_interpreter(
                "jit_bitwise_and",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        System.out.println(10 & 6);
        System.out.println(15 & 7);
    }
}
"#,
                )],
            );
        }

        // Test bitwise OR
        #[test]
        fn jit_bitwise_or() {
            assert_jit_matches_interpreter(
                "jit_bitwise_or",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        System.out.println(10 | 6);
        System.out.println(8 | 1);
    }
}
"#,
                )],
            );
        }

        // Test bitwise XOR
        #[test]
        fn jit_bitwise_xor() {
            assert_jit_matches_interpreter(
                "jit_bitwise_xor",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        System.out.println(10 ^ 6);
        System.out.println(15 ^ 7);
    }
}
"#,
                )],
            );
        }

        // Test shift operations
        #[test]
        fn jit_shift_operations() {
            assert_jit_matches_interpreter(
                "jit_shift_operations",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        System.out.println(8 << 2);
        System.out.println(8 >> 1);
        System.out.println(-8 >> 1);
        System.out.println(8 >>> 1);
    }
}
"#,
                )],
            );
        }
    }

    // =============================================================================
    // MATH OPERATIONS
    // =============================================================================

    mod math_operations {
        use super::*;

        // Test Math.abs
        #[test]
        fn jit_math_abs() {
            assert_jit_matches_interpreter(
                "jit_math_abs",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        System.out.println(Math.abs(-5));
        System.out.println(Math.abs(5));
        System.out.println(Math.abs(0));
    }
}
"#,
                )],
            );
        }

        // Test Math.min / Math.max
        #[test]
        fn jit_math_min_max() {
            assert_jit_matches_interpreter(
                "jit_math_min_max",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        System.out.println(Math.min(10, 20));
        System.out.println(Math.max(10, 20));
        System.out.println(Math.min(-5, -10));
    }
}
"#,
                )],
            );
        }

        // Test compound math
        #[test]
        fn jit_math_compound() {
            assert_jit_matches_interpreter(
                "jit_math_compound",
                &[(
                    "demo/Main.java",
                    r#"
package demo;
public class Main {
    public static void main(String[] args) {
        int sum = 0;
        for (int i = 1; i <= 10; i++) {
            sum += i * i;
        }
        System.out.println(sum);
    }
}
"#,
                )],
            );
        }
    }
}
