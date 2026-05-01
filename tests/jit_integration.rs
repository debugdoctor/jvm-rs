//! Differential JIT tests: compile Java with `javac`, run the same `main`
//! twice — once with the JIT effectively disabled (threshold = u32::MAX) and
//! once with it forced on the first invocation (threshold = 1) — and assert
//! the printed output matches.
//!
//! These tests exist to *drive* JIT correctness. A test failing here means
//! the JIT produced different observable behavior than the interpreter for
//! a real Java program.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use jvm_rs::launcher::{self, LaunchOptions};
use jvm_rs::vm::{ExecutionResult, FieldRef, Method, MethodRef, RuntimeClass, Value, Vm};

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
