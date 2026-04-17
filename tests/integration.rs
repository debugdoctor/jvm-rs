//! End-to-end integration tests that compile Java source code with `javac`
//! and execute the resulting `.class` files through the JVM.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use jvm_rs::launcher::LaunchOptions;
use jvm_rs::vm::ExecutionResult;

fn temp_dir(test_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("jvm-rs-integration-{test_name}-{nanos}"));
    fs::create_dir_all(&path).unwrap();
    path
}

fn compile_and_run_with_javac_args(
    test_name: &str,
    javac_args: &[&str],
    files: &[(&str, &str)],
) -> (ExecutionResult, Vec<String>) {
    let root = temp_dir(test_name);
    for (name, source) in files {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, source).unwrap();
    }

    let source_files: Vec<PathBuf> = files.iter().map(|(name, _)| root.join(name)).collect();
    let mut cmd = Command::new("javac");
    cmd.args(javac_args).arg("-d").arg(&root);
    for source in &source_files {
        cmd.arg(source);
    }
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "javac failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Derive main class from the first file name (e.g., "demo/Main.java" -> "demo.Main")
    let main_file = files[0].0;
    let main_class = main_file
        .trim_end_matches(".java")
        .replace('/', ".");

    let options = LaunchOptions::new(&root, &main_class, vec![]);
    let mut vm = jvm_rs::vm::Vm::new();
    vm.set_class_path(options.class_path.clone());
    let source = jvm_rs::launcher::resolve_class_path(&options.class_path, &main_class).unwrap();
    let method = jvm_rs::launcher::load_main_method(
        &source,
        &main_class,
        &[],
        &mut vm,
    )
    .unwrap();
    let result = vm.execute(method).unwrap();
    let output = vm.take_output();
    (result, output)
}

fn compile_and_run(test_name: &str, files: &[(&str, &str)]) -> (ExecutionResult, Vec<String>) {
    compile_and_run_with_javac_args(test_name, &["--release", "8"], files)
}

#[test]
fn hello_world() {
    let (result, output) = compile_and_run(
        "hello_world",
        &[("demo/Main.java", r#"
package demo;
public class Main {
    public static void main(String[] args) {
        System.out.println("Hello, World!");
    }
}
"#)],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["Hello, World!"]);
}

#[test]
fn fibonacci() {
    let (result, output) = compile_and_run(
        "fibonacci",
        &[("demo/Fib.java", r#"
package demo;
public class Fib {
    public static int fib(int n) {
        if (n <= 1) return n;
        return fib(n - 1) + fib(n - 2);
    }
    public static void main(String[] args) {
        System.out.println(fib(10));
    }
}
"#)],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["55"]);
}

#[test]
fn string_concatenation() {
    let (_, output) = compile_and_run(
        "string_concat",
        &[("demo/Concat.java", r#"
package demo;
public class Concat {
    public static void main(String[] args) {
        String greeting = "Hello, " + "World" + "!";
        System.out.println(greeting);
        int n = 42;
        System.out.println("n=" + n);
    }
}
"#)],
    );
    assert_eq!(output, vec!["Hello, World!", "n=42"]);
}

#[test]
fn polymorphism() {
    let (_, output) = compile_and_run(
        "polymorphism",
        &[
            ("demo/Zoo.java", r#"
package demo;
public class Zoo {
    public static void main(String[] args) {
        Animal a = new Dog();
        Animal b = new Cat();
        System.out.println(a.speak());
        System.out.println(b.speak());
        System.out.println(a instanceof Dog);
        System.out.println(b instanceof Dog);
    }
}
"#),
            ("demo/Animal.java", r#"
package demo;
public class Animal {
    public String speak() { return "..."; }
}
"#),
            ("demo/Dog.java", r#"
package demo;
public class Dog extends Animal {
    public String speak() { return "Woof"; }
}
"#),
            ("demo/Cat.java", r#"
package demo;
public class Cat extends Animal {
    public String speak() { return "Meow"; }
}
"#),
        ],
    );
    assert_eq!(output, vec!["Woof", "Meow", "true", "false"]);
}

#[test]
fn exception_handling() {
    let (_, output) = compile_and_run(
        "exceptions",
        &[("demo/Exc.java", r#"
package demo;
public class Exc {
    public static void main(String[] args) {
        try {
            int x = 10 / 0;
        } catch (ArithmeticException e) {
            System.out.println("caught divide by zero");
        }
        try {
            Object obj = null;
            // This would throw NPE in a real JVM but our VM
            // doesn't convert null derefs to NPE yet; test the
            // exception table path that works: explicit throw.
            throw new RuntimeException();
        } catch (RuntimeException e) {
            System.out.println("caught runtime");
        }
        System.out.println("done");
    }
}
"#)],
    );
    assert_eq!(
        output,
        vec!["caught divide by zero", "caught runtime", "done"]
    );
}

#[test]
fn static_initializer() {
    let (_, output) = compile_and_run(
        "clinit",
        &[("demo/Config.java", r#"
package demo;
public class Config {
    static int VERSION;
    static {
        VERSION = 42;
    }
    public static void main(String[] args) {
        System.out.println("v" + VERSION);
    }
}
"#)],
    );
    assert_eq!(output, vec!["v42"]);
}

#[test]
fn array_operations() {
    let (_, output) = compile_and_run(
        "arrays",
        &[("demo/Arr.java", r#"
package demo;
public class Arr {
    public static void main(String[] args) {
        int[] a = {10, 20, 30};
        int sum = 0;
        for (int i = 0; i < a.length; i++) {
            sum += a[i];
        }
        System.out.println(sum);

        long[] longs = new long[2];
        longs[0] = 100L;
        longs[1] = 200L;
        System.out.println(longs[0] + longs[1]);
    }
}
"#)],
    );
    assert_eq!(output, vec!["60", "300"]);
}

#[test]
fn switch_statement() {
    let (_, output) = compile_and_run(
        "switch",
        &[("demo/Switch.java", r#"
package demo;
public class Switch {
    public static String day(int n) {
        switch (n) {
            case 1: return "Mon";
            case 2: return "Tue";
            case 3: return "Wed";
            default: return "?";
        }
    }
    public static void main(String[] args) {
        System.out.println(day(1));
        System.out.println(day(2));
        System.out.println(day(99));
    }
}
"#)],
    );
    assert_eq!(output, vec!["Mon", "Tue", "?"]);
}

#[test]
fn thread_start_and_join() {
    let (_, output) = compile_and_run(
        "thread_start_and_join",
        &[("demo/ThreadDemo.java", r#"
package demo;

public class ThreadDemo {
    static class Worker implements Runnable {
        public void run() {
            System.out.println("worker");
        }
    }

    public static void main(String[] args) throws Exception {
        Thread thread = new Thread(new Worker());
        thread.start();
        thread.join();
        System.out.println("done");
    }
}
"#)],
    );
    assert_eq!(output, vec!["worker", "done"]);
}

#[test]
fn object_wait_and_notify() {
    let (_, output) = compile_and_run(
        "object_wait_and_notify",
        &[("demo/WaitNotifyDemo.java", r#"
package demo;

public class WaitNotifyDemo {
    static final Object LOCK = new Object();

    static class Worker implements Runnable {
        public void run() {
            synchronized (LOCK) {
                System.out.println("worker-ready");
                LOCK.notify();
            }
        }
    }

    public static void main(String[] args) throws Exception {
        synchronized (LOCK) {
            Thread thread = new Thread(new Worker());
            thread.start();
            LOCK.wait();
            System.out.println("main-resumed");
            thread.join();
        }
        System.out.println("done");
    }
}
"#)],
    );
    assert_eq!(output, vec!["worker-ready", "main-resumed", "done"]);
}

#[test]
fn modern_string_concat_factory() {
    let (_, output) = compile_and_run_with_javac_args(
        "modern_string_concat_factory",
        &[],
        &[("demo/ModernConcat.java", r#"
package demo;

public class ModernConcat {
    public static void main(String[] args) {
        int n = 42;
        String label = "items";
        System.out.println("count=" + n + ", label=" + label);
    }
}
"#)],
    );
    assert_eq!(output, vec!["count=42, label=items"]);
}

#[test]
fn lambda_metafactory() {
    let (_, output) = compile_and_run_with_javac_args(
        "lambda_metafactory",
        &["--release", "8"],
        &[("demo/LambdaDemo.java", r#"
package demo;

public class LambdaDemo {
    interface Greeter {
        String greet(String name);
    }

    public static void main(String[] args) {
        Greeter greeter = name -> "Hello, " + name;
        System.out.println(greeter.greet("JVM"));
    }
}
"#)],
    );
    assert_eq!(output, vec!["Hello, JVM"]);
}

// --- New: extended built-in library tests ---

#[test]
fn string_utility_methods() {
    let (_, output) = compile_and_run(
        "string_utility_methods",
        &[("demo/Strings.java", r#"
package demo;
public class Strings {
    public static void main(String[] args) {
        String s = "  Hello, World!  ";
        System.out.println(s.trim());
        System.out.println(s.trim().toUpperCase());
        System.out.println(s.trim().toLowerCase());
        System.out.println("abcdef".substring(2));
        System.out.println("abcdef".substring(1, 4));
        System.out.println("banana".indexOf('n'));
        System.out.println("banana".indexOf("na"));
        System.out.println("Hello".startsWith("He"));
        System.out.println("Hello".endsWith("lo"));
        System.out.println("Hello".contains("ell"));
        System.out.println("".isEmpty());
        System.out.println("ab".concat("cd"));
        System.out.println("foo.bar".replace('.', '/'));
        System.out.println(String.valueOf(42));
        System.out.println(String.valueOf(true));
    }
}
"#)],
    );
    assert_eq!(
        output,
        vec![
            "Hello, World!",
            "HELLO, WORLD!",
            "hello, world!",
            "cdef",
            "bcd",
            "2",
            "2",
            "true",
            "true",
            "true",
            "true",
            "abcd",
            "foo/bar",
            "42",
            "true",
        ]
    );
}

#[test]
fn integer_long_character_boolean_utilities() {
    let (_, output) = compile_and_run(
        "integer_long_character_boolean_utilities",
        &[("demo/Utils.java", r#"
package demo;
public class Utils {
    public static void main(String[] args) {
        System.out.println(Integer.parseInt("123"));
        System.out.println(Integer.toString(42));
        System.out.println(Integer.toBinaryString(10));
        System.out.println(Integer.toHexString(255));
        System.out.println(Long.parseLong("9999999999"));
        System.out.println(Long.toString(-7L));
        System.out.println(Character.isDigit('5'));
        System.out.println(Character.isLetter('a'));
        System.out.println(Character.toUpperCase('q'));
        System.out.println(Boolean.parseBoolean("TRUE"));
        System.out.println(Boolean.toString(false));
    }
}
"#)],
    );
    assert_eq!(
        output,
        vec!["123", "42", "1010", "ff", "9999999999", "-7", "true", "true", "Q", "true", "false"]
    );
}

#[test]
fn math_extended_functions() {
    let (_, output) = compile_and_run(
        "math_extended_functions",
        &[("demo/MathDemo.java", r#"
package demo;
public class MathDemo {
    public static void main(String[] args) {
        System.out.println(Math.floor(3.7));
        System.out.println(Math.ceil(3.2));
        System.out.println(Math.round(2.5));
        System.out.println((long) Math.log(Math.exp(3.0) + 0.5));
        double r = Math.random();
        System.out.println(r >= 0.0 && r < 1.0);
    }
}
"#)],
    );
    assert_eq!(output, vec!["3.0", "4.0", "3", "3", "true"]);
}

#[test]
fn system_arraycopy_and_properties() {
    let (_, output) = compile_and_run(
        "system_arraycopy_and_properties",
        &[("demo/SysDemo.java", r#"
package demo;
public class SysDemo {
    public static void main(String[] args) {
        int[] src = {1, 2, 3, 4, 5};
        int[] dst = new int[5];
        System.arraycopy(src, 1, dst, 0, 3);
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < dst.length; i++) {
            if (i > 0) sb.append(',');
            sb.append(dst[i]);
        }
        System.out.println(sb.toString());

        long t1 = System.currentTimeMillis();
        long t2 = System.currentTimeMillis();
        System.out.println(t2 >= t1);
    }
}
"#)],
    );
    assert_eq!(output, vec!["2,3,4,0,0", "true"]);
}

#[test]
fn objects_utility_methods() {
    let (_, output) = compile_and_run(
        "objects_utility_methods",
        &[("demo/ObjDemo.java", r#"
package demo;
import java.util.Objects;
public class ObjDemo {
    public static void main(String[] args) {
        String a = "x";
        System.out.println(Objects.requireNonNull(a));
        System.out.println(Objects.equals("a", "a"));
        System.out.println(Objects.equals("a", "b"));
        System.out.println(Objects.equals(null, null));
        System.out.println(Objects.isNull(null));
        System.out.println(Objects.nonNull(a));
        try {
            Objects.requireNonNull(null);
            System.out.println("no-exception");
        } catch (NullPointerException e) {
            System.out.println("npe-caught");
        }
    }
}
"#)],
    );
    assert_eq!(
        output,
        vec!["x", "true", "false", "true", "true", "true", "npe-caught"]
    );
}

// --- New: compatibility tests for modern javac output ---

#[test]
fn modern_javac_enhanced_for_and_var() {
    let (_, output) = compile_and_run_with_javac_args(
        "modern_javac_enhanced_for_and_var",
        &[],
        &[("demo/Modern.java", r#"
package demo;
public class Modern {
    public static void main(String[] args) {
        int[] values = {1, 2, 3, 4};
        int sum = 0;
        for (var v : values) {
            sum += v;
        }
        System.out.println(sum);
    }
}
"#)],
    );
    assert_eq!(output, vec!["10"]);
}

#[test]
fn modern_javac_try_with_resources_like_pattern() {
    // Javac compiles try-with-resources into synthetic finally blocks that use
    // the `athrow` / exception-table machinery. This exercises those paths
    // without requiring AutoCloseable / Throwable.addSuppressed implementations.
    let (_, output) = compile_and_run_with_javac_args(
        "modern_javac_finally_unwind",
        &[],
        &[("demo/Finally.java", r#"
package demo;
public class Finally {
    static int run() {
        try {
            return 1;
        } finally {
            System.out.println("finally");
        }
    }
    public static void main(String[] args) {
        System.out.println(run());
        try {
            try {
                throw new RuntimeException("boom");
            } finally {
                System.out.println("inner-finally");
            }
        } catch (RuntimeException e) {
            System.out.println("outer-caught");
        }
    }
}
"#)],
    );
    assert_eq!(
        output,
        vec!["finally", "1", "inner-finally", "outer-caught"]
    );
}

#[test]
fn modern_javac_nested_lambdas_and_concat() {
    let (_, output) = compile_and_run_with_javac_args(
        "modern_javac_nested_lambdas",
        &[],
        &[("demo/Nested.java", r#"
package demo;

public class Nested {
    interface Op {
        int apply(int x);
    }

    public static void main(String[] args) {
        Op add1 = x -> x + 1;
        Op add2 = x -> add1.apply(x) + 1;
        int result = add2.apply(10);
        System.out.println("result=" + result);
    }
}
"#)],
    );
    assert_eq!(output, vec!["result=12"]);
}

#[test]
fn modern_javac_interface_default_dispatch() {
    let (_, output) = compile_and_run_with_javac_args(
        "modern_javac_interface_default",
        &[],
        &[("demo/Defaults.java", r#"
package demo;
public class Defaults {
    interface Named {
        default String describe() { return "Named:" + name(); }
        String name();
    }
    static class A implements Named {
        public String name() { return "A"; }
    }
    public static void main(String[] args) {
        Named n = new A();
        System.out.println(n.describe());
    }
}
"#)],
    );
    assert_eq!(output, vec!["Named:A"]);
}

// --- New: regression tests for partially supported JVMS features ---

#[test]
fn regression_deeply_nested_exceptions() {
    let (_, output) = compile_and_run(
        "regression_nested_exceptions",
        &[("demo/Nested.java", r#"
package demo;
public class Nested {
    static int compute(int x) {
        try {
            try {
                if (x == 0) throw new ArithmeticException("inner");
                return 100 / x;
            } catch (NullPointerException e) {
                return -1;
            }
        } catch (ArithmeticException e) {
            return -2;
        }
    }
    public static void main(String[] args) {
        System.out.println(compute(5));
        System.out.println(compute(0));
    }
}
"#)],
    );
    assert_eq!(output, vec!["20", "-2"]);
}

#[test]
fn regression_tableswitch_boundaries() {
    let (_, output) = compile_and_run(
        "regression_tableswitch",
        &[("demo/Table.java", r#"
package demo;
public class Table {
    static String label(int n) {
        switch (n) {
            case 0: return "zero";
            case 1: return "one";
            case 2: return "two";
            case 3: return "three";
            default: return "other";
        }
    }
    public static void main(String[] args) {
        System.out.println(label(-1));
        System.out.println(label(0));
        System.out.println(label(3));
        System.out.println(label(100));
    }
}
"#)],
    );
    assert_eq!(output, vec!["other", "zero", "three", "other"]);
}

#[test]
fn regression_lookupswitch_sparse_keys() {
    let (_, output) = compile_and_run(
        "regression_lookupswitch",
        &[("demo/Lookup.java", r#"
package demo;
public class Lookup {
    static String sparse(int n) {
        switch (n) {
            case 1: return "a";
            case 100: return "b";
            case 10000: return "c";
            default: return "?";
        }
    }
    public static void main(String[] args) {
        System.out.println(sparse(1));
        System.out.println(sparse(100));
        System.out.println(sparse(10000));
        System.out.println(sparse(50));
    }
}
"#)],
    );
    assert_eq!(output, vec!["a", "b", "c", "?"]);
}

#[test]
fn regression_multidim_array_allocation() {
    let (_, output) = compile_and_run(
        "regression_multidim_array",
        &[("demo/Multi.java", r#"
package demo;
public class Multi {
    public static void main(String[] args) {
        int[][] grid = new int[3][2];
        grid[0][0] = 1; grid[0][1] = 2;
        grid[1][0] = 3; grid[1][1] = 4;
        grid[2][0] = 5; grid[2][1] = 6;
        int sum = 0;
        for (int i = 0; i < grid.length; i++) {
            for (int j = 0; j < grid[i].length; j++) {
                sum += grid[i][j];
            }
        }
        System.out.println(sum);
    }
}
"#)],
    );
    assert_eq!(output, vec!["21"]);
}

#[test]
fn regression_long_arithmetic_and_shifts() {
    let (_, output) = compile_and_run(
        "regression_long_arithmetic",
        &[("demo/Longs.java", r#"
package demo;
public class Longs {
    public static void main(String[] args) {
        long a = 0x1234567890ABCDEFL;
        long b = a >> 16;
        long c = a << 4;
        long d = a >>> 1;
        System.out.println(b);
        System.out.println(c);
        System.out.println(d > 0);
        System.out.println(a & 0xFFL);
        System.out.println(a | 0xF000L);
    }
}
"#)],
    );
    // Constants below are computed against the same arithmetic a real JVM
    // would perform, so they pin down shift/logic behavior for longs.
    assert_eq!(
        output,
        vec![
            "20015998341291",           // a >> 16
            "2541551403008843504",      // a << 4
            "true",                     // a >>> 1 > 0
            "239",                      // a & 0xFF
            "1311768467294911983",      // a | 0xF000
        ]
    );
}

#[test]
fn regression_string_builder_reverse_and_insert() {
    let (_, output) = compile_and_run(
        "regression_stringbuilder_reverse_and_insert",
        &[("demo/SB.java", r#"
package demo;
public class SB {
    public static void main(String[] args) {
        StringBuilder sb = new StringBuilder("world");
        sb.insert(0, "hello, ");
        System.out.println(sb.toString());
        sb.reverse();
        System.out.println(sb.toString());
        sb.deleteCharAt(0);
        System.out.println(sb.toString());
    }
}
"#)],
    );
    assert_eq!(
        output,
        vec!["hello, world", "dlrow ,olleh", "lrow ,olleh"]
    );
}
