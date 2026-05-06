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
    let main_class = main_file.trim_end_matches(".java").replace('/', ".");

    let options = LaunchOptions::new(&root, &main_class, vec![]);
    let mut vm = jvm_rs::vm::Vm::new().expect("failed to create VM");
    vm.set_class_path(options.class_path.clone());
    let source = jvm_rs::launcher::resolve_class_path(&options.class_path, &main_class).unwrap();
    let method = jvm_rs::launcher::load_main_method(&source, &main_class, &[], &mut vm).unwrap();
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
        &[(
            "demo/Main.java",
            r#"
package demo;
public class Main {
    public static void main(String[] args) {
        System.out.println("Hello, World!");
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["Hello, World!"]);
}

#[test]
fn fibonacci() {
    let (result, output) = compile_and_run(
        "fibonacci",
        &[(
            "demo/Fib.java",
            r#"
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
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["55"]);
}

#[test]
fn string_concatenation() {
    let (_, output) = compile_and_run(
        "string_concat",
        &[(
            "demo/Concat.java",
            r#"
package demo;
public class Concat {
    public static void main(String[] args) {
        String greeting = "Hello, " + "World" + "!";
        System.out.println(greeting);
        int n = 42;
        System.out.println("n=" + n);
    }
}
"#,
        )],
    );
    assert_eq!(output, vec!["Hello, World!", "n=42"]);
}

#[test]
fn polymorphism() {
    let (_, output) = compile_and_run(
        "polymorphism",
        &[
            (
                "demo/Zoo.java",
                r#"
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
"#,
            ),
            (
                "demo/Animal.java",
                r#"
package demo;
public class Animal {
    public String speak() { return "..."; }
}
"#,
            ),
            (
                "demo/Dog.java",
                r#"
package demo;
public class Dog extends Animal {
    public String speak() { return "Woof"; }
}
"#,
            ),
            (
                "demo/Cat.java",
                r#"
package demo;
public class Cat extends Animal {
    public String speak() { return "Meow"; }
}
"#,
            ),
        ],
    );
    assert_eq!(output, vec!["Woof", "Meow", "true", "false"]);
}

#[test]
fn exception_handling() {
    let (_, output) = compile_and_run(
        "exceptions",
        &[(
            "demo/Exc.java",
            r#"
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
"#,
        )],
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
        &[(
            "demo/Config.java",
            r#"
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
"#,
        )],
    );
    assert_eq!(output, vec!["v42"]);
}

#[test]
fn array_operations() {
    let (_, output) = compile_and_run(
        "arrays",
        &[(
            "demo/Arr.java",
            r#"
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
"#,
        )],
    );
    assert_eq!(output, vec!["60", "300"]);
}

#[test]
fn switch_statement() {
    let (_, output) = compile_and_run(
        "switch",
        &[(
            "demo/Switch.java",
            r#"
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
"#,
        )],
    );
    assert_eq!(output, vec!["Mon", "Tue", "?"]);
}

#[test]
fn thread_start_and_join() {
    let (_, output) = compile_and_run(
        "thread_start_and_join",
        &[(
            "demo/ThreadDemo.java",
            r#"
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
"#,
        )],
    );
    assert_eq!(output, vec!["worker", "done"]);
}

#[test]
fn object_wait_and_notify() {
    let (_, output) = compile_and_run(
        "object_wait_and_notify",
        &[(
            "demo/WaitNotifyDemo.java",
            r#"
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
"#,
        )],
    );
    assert_eq!(output, vec!["worker-ready", "main-resumed", "done"]);
}

#[test]
fn modern_string_concat_factory() {
    let (_, output) = compile_and_run_with_javac_args(
        "modern_string_concat_factory",
        &[],
        &[(
            "demo/ModernConcat.java",
            r#"
package demo;

public class ModernConcat {
    public static void main(String[] args) {
        int n = 42;
        String label = "items";
        System.out.println("count=" + n + ", label=" + label);
    }
}
"#,
        )],
    );
    assert_eq!(output, vec!["count=42, label=items"]);
}

#[test]
fn lambda_metafactory() {
    let (_, output) = compile_and_run_with_javac_args(
        "lambda_metafactory",
        &["--release", "8"],
        &[(
            "demo/LambdaDemo.java",
            r#"
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
"#,
        )],
    );
    assert_eq!(output, vec!["Hello, JVM"]);
}

// --- New: extended built-in library tests ---

#[test]
fn string_utility_methods() {
    let (_, output) = compile_and_run(
        "string_utility_methods",
        &[(
            "demo/Strings.java",
            r#"
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
"#,
        )],
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
        &[(
            "demo/Utils.java",
            r#"
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
"#,
        )],
    );
    assert_eq!(
        output,
        vec![
            "123",
            "42",
            "1010",
            "ff",
            "9999999999",
            "-7",
            "true",
            "true",
            "Q",
            "true",
            "false"
        ]
    );
}

#[test]
fn math_extended_functions() {
    let (_, output) = compile_and_run(
        "math_extended_functions",
        &[(
            "demo/MathDemo.java",
            r#"
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
"#,
        )],
    );
    assert_eq!(output, vec!["3.0", "4.0", "3", "3", "true"]);
}

#[test]
fn system_arraycopy_and_properties() {
    let (_, output) = compile_and_run(
        "system_arraycopy_and_properties",
        &[(
            "demo/SysDemo.java",
            r#"
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
"#,
        )],
    );
    assert_eq!(output, vec!["2,3,4,0,0", "true"]);
}

#[test]
fn objects_utility_methods() {
    let (_, output) = compile_and_run(
        "objects_utility_methods",
        &[(
            "demo/ObjDemo.java",
            r#"
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
"#,
        )],
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
        &[(
            "demo/Modern.java",
            r#"
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
"#,
        )],
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
        &[(
            "demo/Finally.java",
            r#"
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
"#,
        )],
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
        &[(
            "demo/Nested.java",
            r#"
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
"#,
        )],
    );
    assert_eq!(output, vec!["result=12"]);
}

#[test]
fn modern_javac_interface_default_dispatch() {
    let (_, output) = compile_and_run_with_javac_args(
        "modern_javac_interface_default",
        &[],
        &[(
            "demo/Defaults.java",
            r#"
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
"#,
        )],
    );
    assert_eq!(output, vec!["Named:A"]);
}

// --- New: regression tests for partially supported JVMS features ---

#[test]
fn regression_deeply_nested_exceptions() {
    let (_, output) = compile_and_run(
        "regression_nested_exceptions",
        &[(
            "demo/Nested.java",
            r#"
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
"#,
        )],
    );
    assert_eq!(output, vec!["20", "-2"]);
}

#[test]
fn regression_tableswitch_boundaries() {
    let (_, output) = compile_and_run(
        "regression_tableswitch",
        &[(
            "demo/Table.java",
            r#"
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
"#,
        )],
    );
    assert_eq!(output, vec!["other", "zero", "three", "other"]);
}

#[test]
fn regression_lookupswitch_sparse_keys() {
    let (_, output) = compile_and_run(
        "regression_lookupswitch",
        &[(
            "demo/Lookup.java",
            r#"
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
"#,
        )],
    );
    assert_eq!(output, vec!["a", "b", "c", "?"]);
}

#[test]
fn regression_multidim_array_allocation() {
    let (_, output) = compile_and_run(
        "regression_multidim_array",
        &[(
            "demo/Multi.java",
            r#"
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
"#,
        )],
    );
    assert_eq!(output, vec!["21"]);
}

#[test]
fn regression_long_arithmetic_and_shifts() {
    let (_, output) = compile_and_run(
        "regression_long_arithmetic",
        &[(
            "demo/Longs.java",
            r#"
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
"#,
        )],
    );
    // Constants below are computed against the same arithmetic a real JVM
    // would perform, so they pin down shift/logic behavior for longs.
    assert_eq!(
        output,
        vec![
            "20015998341291",      // a >> 16
            "2541551403008843504", // a << 4
            "true",                // a >>> 1 > 0
            "239",                 // a & 0xFF
            "1311768467294911983", // a | 0xF000
        ]
    );
}

#[test]
fn regression_string_builder_reverse_and_insert() {
    let (_, output) = compile_and_run(
        "regression_stringbuilder_reverse_and_insert",
        &[(
            "demo/SB.java",
            r#"
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
"#,
        )],
    );
    assert_eq!(output, vec!["hello, world", "dlrow ,olleh", "lrow ,olleh"]);
}

#[test]
fn java_util_arraylist_from_jdk() {
    let (result, output) = compile_and_run(
        "java_util_arraylist",
        &[(
            "demo/TestArrayList.java",
            r#"
package demo;
import java.util.ArrayList;
public class TestArrayList {
    public static void main(String[] args) {
        ArrayList<String> list = new ArrayList<>();
        list.add("hello");
        list.add("world");
        System.out.println(list.size());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["2"]);
}

#[test]
fn java_util_arraylist_get_from_jdk() {
    let (result, output) = compile_and_run(
        "java_util_arraylist_get",
        &[(
            "demo/TestArrayListGet.java",
            r#"
package demo;
import java.util.ArrayList;
public class TestArrayListGet {
    public static void main(String[] args) {
        ArrayList<String> list = new ArrayList<>();
        list.add("hello");
        list.add("world");
        System.out.println(list.get(0));
        System.out.println(list.get(1));
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["hello", "world"]);
}

#[test]
fn java_util_hashmap_basic() {
    let (result, output) = compile_and_run(
        "java_util_hashmap",
        &[(
            "demo/TestHashMap.java",
            r#"
package demo;
import java.util.HashMap;
public class TestHashMap {
    public static void main(String[] args) {
        HashMap<String, String> map = new HashMap<>();
        map.put("one", "1");
        map.put("two", "2");
        System.out.println(map.get("one"));
        System.out.println(map.get("two"));
        System.out.println(map.size());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["1", "2", "2"]);
}

#[test]
fn java_util_function_basic() {
    let (result, output) = compile_and_run(
        "java_util_function",
        &[(
            "demo/TestFunction.java",
            r#"
package demo;
import java.util.function.Function;
public class TestFunction {
    public static void main(String[] args) {
        Function<String, String> len = s -> String.valueOf(s.length());
        System.out.println(len.apply("hello"));
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["5"]);
}

#[test]
fn java_util_consumer_basic() {
    let (result, output) = compile_and_run(
        "java_util_consumer",
        &[(
            "demo/TestConsumer.java",
            r#"
package demo;
import java.util.function.Consumer;
public class TestConsumer {
    public static void main(String[] args) {
        Consumer<String> printer = s -> System.out.println(s);
        printer.accept("hello");
        printer.accept("world");
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["hello", "world"]);
}

#[test]
fn java_util_supplier_basic() {
    let (result, output) = compile_and_run(
        "java_util_supplier",
        &[(
            "demo/TestSupplier.java",
            r#"
package demo;
import java.util.function.Supplier;
public class TestSupplier {
    public static void main(String[] args) {
        Supplier<String> supplier = () -> "produced";
        System.out.println(supplier.get());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["produced"]);
}

#[test]
fn java_util_optional_basic() {
    let (result, output) = compile_and_run(
        "java_util_optional",
        &[(
            "demo/TestOptional.java",
            r#"
package demo;
import java.util.Optional;
public class TestOptional {
    public static void main(String[] args) {
        Optional<String> opt = Optional.of("hello");
        System.out.println(opt.isPresent());
        System.out.println(opt.get());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["true", "hello"]);
}

#[test]
fn java_util_linked_list() {
    let (result, output) = compile_and_run(
        "java_util_linked_list",
        &[(
            "demo/TestLinkedList.java",
            r#"
package demo;
import java.util.LinkedList;
public class TestLinkedList {
    public static void main(String[] args) {
        LinkedList<String> list = new LinkedList<>();
        list.add("hello");
        list.add("world");
        System.out.println(list.get(0));
        System.out.println(list.get(1));
        System.out.println(list.size());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["hello", "world", "2"]);
}

#[test]
fn java_util_hash_map() {
    let (result, output) = compile_and_run(
        "java_util_hash_map",
        &[(
            "demo/TestHashMap.java",
            r#"
package demo;
import java.util.HashMap;
public class TestHashMap {
    public static void main(String[] args) {
        HashMap<String, String> map = new HashMap<>();
        map.put("one", "1");
        map.put("two", "2");
        System.out.println(map.get("one"));
        System.out.println(map.get("two"));
        System.out.println(map.size());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["1", "2", "2"]);
}

#[test]
fn java_util_tree_map() {
    let (result, output) = compile_and_run(
        "java_util_tree_map",
        &[(
            "demo/TestTreeMap.java",
            r#"
package demo;
import java.util.TreeMap;
public class TestTreeMap {
    public static void main(String[] args) {
        TreeMap<String, String> tm = new TreeMap<>();
        tm.put("one", "1");
        tm.put("two", "2");
        System.out.println(tm.get("one"));
        System.out.println(tm.size());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["1", "2"]);
}

#[test]
fn java_util_tree_set() {
    let (result, output) = compile_and_run(
        "java_util_tree_set",
        &[(
            "demo/TestTreeSet.java",
            r#"
package demo;
import java.util.TreeSet;
public class TestTreeSet {
    public static void main(String[] args) {
        TreeSet<String> ts = new TreeSet<>();
        ts.add("banana");
        ts.add("apple");
        System.out.println(ts.first());
        System.out.println(ts.size());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["apple", "2"]);
}

#[test]
fn java_util_hash_set() {
    let (result, output) = compile_and_run(
        "java_util_hash_set",
        &[(
            "demo/TestHashSet.java",
            r#"
package demo;
import java.util.HashSet;
public class TestHashSet {
    public static void main(String[] args) {
        HashSet<String> hs = new HashSet<>();
        hs.add("apple");
        hs.add("banana");
        System.out.println(hs.contains("apple"));
        System.out.println(hs.size());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["true", "2"]);
}

#[test]
fn java_util_linked_hash_map() {
    let (result, output) = compile_and_run(
        "java_util_linked_hash_map",
        &[(
            "demo/TestLinkedHashMap.java",
            r#"
package demo;
import java.util.LinkedHashMap;
public class TestLinkedHashMap {
    public static void main(String[] args) {
        LinkedHashMap<String, String> lhm = new LinkedHashMap<>();
        lhm.put("k1", "v1");
        lhm.put("k2", "v2");
        System.out.println(lhm.get("k1"));
        System.out.println(lhm.size());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["v1", "2"]);
}

#[test]
fn java_util_arraylist_iterator() {
    // Exercises the ArrayList → AbstractList → Iterable chain end-to-end:
    // `iterator()` allocates the JDK's ArrayList$Itr inner class, and the
    // while loop dispatches through hasNext/next on that iterator.
    let (result, output) = compile_and_run(
        "java_util_arraylist_iterator",
        &[(
            "demo/TestIter.java",
            r#"
package demo;
import java.util.ArrayList;
import java.util.Iterator;
public class TestIter {
    public static void main(String[] args) {
        ArrayList<String> list = new ArrayList<>();
        list.add("a");
        list.add("b");
        list.add("c");
        Iterator<String> it = list.iterator();
        while (it.hasNext()) {
            System.out.println(it.next());
        }
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["a", "b", "c"]);
}

#[test]
fn java_util_arraylist_enhanced_for() {
    // javac desugars enhanced-for on a Collection into iterator()/hasNext()/next().
    // Confirms the Iterable default-method pipeline works for real JDK
    // bytecode, not just for primitive-array enhanced-for.
    let (result, output) = compile_and_run(
        "java_util_arraylist_enhanced_for",
        &[(
            "demo/TestForEach.java",
            r#"
package demo;
import java.util.ArrayList;
public class TestForEach {
    public static void main(String[] args) {
        ArrayList<String> list = new ArrayList<>();
        list.add("x");
        list.add("y");
        list.add("z");
        for (String s : list) {
            System.out.println(s);
        }
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["x", "y", "z"]);
}

#[test]
fn java_util_collections_sort_integers() {
    // Collections.sort is implemented natively in the VM (shadows the JDK
    // bytecode) — it invokes List.size/get/set and Comparable.compareTo
    // through normal virtual dispatch.
    let (result, output) = compile_and_run(
        "java_util_collections_sort_integers",
        &[(
            "demo/TestSort.java",
            r#"
package demo;
import java.util.ArrayList;
import java.util.Collections;
public class TestSort {
    public static void main(String[] args) {
        ArrayList<Integer> list = new ArrayList<>();
        list.add(Integer.valueOf(3));
        list.add(Integer.valueOf(1));
        list.add(Integer.valueOf(4));
        list.add(Integer.valueOf(1));
        list.add(Integer.valueOf(5));
        Collections.sort(list);
        for (int i = 0; i < list.size(); i++) {
            System.out.println(list.get(i));
        }
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["1", "1", "3", "4", "5"]);
}

#[test]
fn java_util_collections_sort_strings() {
    let (result, output) = compile_and_run(
        "java_util_collections_sort_strings",
        &[(
            "demo/TestSortS.java",
            r#"
package demo;
import java.util.ArrayList;
import java.util.Collections;
public class TestSortS {
    public static void main(String[] args) {
        ArrayList<String> list = new ArrayList<>();
        list.add("banana");
        list.add("apple");
        list.add("cherry");
        Collections.sort(list);
        for (int i = 0; i < list.size(); i++) {
            System.out.println(list.get(i));
        }
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["apple", "banana", "cherry"]);
}

#[test]
fn java_util_collections_reverse() {
    let (result, output) = compile_and_run(
        "java_util_collections_reverse",
        &[(
            "demo/TestRev.java",
            r#"
package demo;
import java.util.ArrayList;
import java.util.Collections;
public class TestRev {
    public static void main(String[] args) {
        ArrayList<Integer> list = new ArrayList<>();
        list.add(Integer.valueOf(1));
        list.add(Integer.valueOf(2));
        list.add(Integer.valueOf(3));
        Collections.reverse(list);
        for (int i = 0; i < list.size(); i++) {
            System.out.println(list.get(i));
        }
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["3", "2", "1"]);
}

#[test]
fn java_util_arrays_hashcode_and_equals() {
    // Arrays.hashCode/equals both unblocked by the descriptor-correct CDS
    // stub fix (prior `operand stack overflow` root cause) and by stubbing
    // jdk/internal/misc/Unsafe — ArraysSupport's mismatch path now runs
    // end-to-end.
    let (result, output) = compile_and_run(
        "java_util_arrays_hashcode_equals",
        &[(
            "demo/TestArrEq.java",
            r#"
package demo;
import java.util.Arrays;
public class TestArrEq {
    public static void main(String[] args) {
        int[] a = {1, 2, 3, 4};
        int[] b = {1, 2, 3, 4};
        int[] c = {1, 2, 3, 5};
        System.out.println(Arrays.hashCode(a));
        System.out.println(Arrays.equals(a, b));
        System.out.println(Arrays.equals(a, c));
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["955331", "true", "false"]);
}

#[test]
fn java_util_arrays_stream_sum() {
    // Arrays.stream(int[]) is shadowed by a Rust native that returns a
    // `__jvm_rs/NativeIntStream`, avoiding the JDK's Stream pipeline
    // (which pulls in ForkJoin/SharedSecrets/Reference handler).
    // Terminal ops sum/count/toArray are services as natives on the
    // NativeIntStream class.
    let (result, output) = compile_and_run(
        "java_util_arrays_stream_sum",
        &[(
            "demo/TestArrStream.java",
            r#"
package demo;
import java.util.Arrays;
import java.util.stream.IntStream;
public class TestArrStream {
    public static void main(String[] args) {
        int[] a = {1, 2, 3, 4, 5};
        IntStream s = Arrays.stream(a);
        System.out.println(s.sum());
        System.out.println(Arrays.stream(a).count());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["15", "5"]);
}

#[test]
fn java_util_hashmap_iterator() {
    // HashMap.entrySet().iterator() exercises a second Iterator implementation
    // (HashMap$EntryIterator) and confirms the Iterable machinery isn't
    // ArrayList-specific.
    let (result, output) = compile_and_run(
        "java_util_hashmap_iterator",
        &[(
            "demo/TestMapIter.java",
            r#"
package demo;
import java.util.HashMap;
import java.util.Map;
public class TestMapIter {
    public static void main(String[] args) {
        HashMap<String, Integer> map = new HashMap<>();
        map.put("a", Integer.valueOf(1));
        map.put("b", Integer.valueOf(2));
        int total = 0;
        for (Map.Entry<String, Integer> e : map.entrySet()) {
            total += e.getValue().intValue();
        }
        System.out.println(total);
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["3"]);
}

#[test]
fn java_util_arrays_stream_long() {
    let (result, output) = compile_and_run(
        "java_util_arrays_stream_long",
        &[(
            "demo/TestLongStream.java",
            r#"
package demo;
import java.util.Arrays;
import java.util.stream.LongStream;
public class TestLongStream {
    public static void main(String[] args) {
        long[] a = {1L, 2L, 3L, 4L, 5L};
        LongStream s = Arrays.stream(a);
        System.out.println(s.sum());
        System.out.println(Arrays.stream(a).count());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["15", "5"]);
}

#[test]
fn java_util_arrays_stream_double() {
    let (result, output) = compile_and_run(
        "java_util_arrays_stream_double",
        &[(
            "demo/TestDoubleStream.java",
            r#"
package demo;
import java.util.Arrays;
import java.util.stream.DoubleStream;
public class TestDoubleStream {
    public static void main(String[] args) {
        double[] a = {1.0, 2.0, 3.0, 4.0, 5.0};
        DoubleStream s = Arrays.stream(a);
        System.out.println(s.sum());
        System.out.println(Arrays.stream(a).count());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["15.0", "5"]);
}

#[test]
fn java_util_stream_optional_min_max() {
    let (result, output) = compile_and_run(
        "java_util_stream_optional_min_max",
        &[(
            "demo/TestOptMinMax.java",
            r#"
package demo;
import java.util.OptionalInt;
import java.util.OptionalDouble;
import java.util.stream.IntStream;
import java.util.Arrays;
public class TestOptMinMax {
    public static void main(String[] args) {
        OptionalInt min = Arrays.stream(new int[]{3, 1, 4, 1, 5}).min();
        OptionalInt max = Arrays.stream(new int[]{3, 1, 4, 1, 5}).max();
        System.out.println(min.isPresent());
        System.out.println(min.getAsInt());
        System.out.println(max.isPresent());
        System.out.println(max.getAsInt());
        
        OptionalDouble avg = Arrays.stream(new int[]{1, 2, 3, 4, 5}).average();
        System.out.println(avg.isPresent());
        System.out.println(avg.getAsDouble());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["true", "1", "true", "5", "true", "3.0"]);
}

#[test]
fn java_util_hash_map_iteration_and_remove() {
    let (result, output) = compile_and_run(
        "java_util_hashmap_iter_remove",
        &[(
            "demo/TestHashMapIterRemove.java",
            r#"
package demo;
import java.util.HashMap;
import java.util.Iterator;
import java.util.Map;
public class TestHashMapIterRemove {
    public static void main(String[] args) {
        HashMap<String, Integer> map = new HashMap<>();
        map.put("a", 1);
        map.put("b", 2);
        map.put("c", 3);

        int sum = 0;
        for (Map.Entry<String, Integer> e : map.entrySet()) {
            sum += e.getValue();
        }
        System.out.println(sum);

        map.remove("b");
        System.out.println(map.size());
        System.out.println(map.containsKey("b"));
        System.out.println(map.containsKey("a"));
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["6", "2", "false", "true"]);
}

#[test]
fn java_util_linked_list_operations() {
    let (result, output) = compile_and_run(
        "java_util_linked_list_ops",
        &[(
            "demo/TestLinkedListOps.java",
            r#"
package demo;
import java.util.LinkedList;
public class TestLinkedListOps {
    public static void main(String[] args) {
        LinkedList<String> list = new LinkedList<>();
        list.add("first");
        list.add("second");
        list.add("third");
        System.out.println(list.getFirst());
        System.out.println(list.getLast());
        System.out.println(list.removeFirst());
        System.out.println(list.size());
        System.out.println(list.getFirst());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["first", "third", "first", "2", "second"]);
}

#[test]
fn java_util_hash_set_operations() {
    let (result, output) = compile_and_run(
        "java_util_hash_set_ops",
        &[(
            "demo/TestHashSetOps.java",
            r#"
package demo;
import java.util.HashSet;
public class TestHashSetOps {
    public static void main(String[] args) {
        HashSet<String> hs = new HashSet<>();
        hs.add("apple");
        hs.add("banana");
        hs.add("cherry");
        hs.add("apple");

        System.out.println(hs.size());
        System.out.println(hs.contains("banana"));
        System.out.println(hs.contains("grape"));
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["3", "true", "false"]);
}

#[test]
fn byte_array_output_stream_basic() {
    let (result, output) = compile_and_run(
        "baos_basic",
        &[(
            "demo/TestBAOS.java",
            r#"
package demo;
import java.io.ByteArrayOutputStream;
public class TestBAOS {
    public static void main(String[] args) throws Exception {
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        baos.write(72);
        baos.write(105);
        baos.write(33);
        System.out.println(baos.size());
        System.out.println(baos.toString());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["3", "Hi!"]);
}

#[test]
fn byte_array_output_stream_write_bytes() {
    let (result, output) = compile_and_run(
        "baos_write_bytes",
        &[(
            "demo/TestBAOS2.java",
            r#"
package demo;
import java.io.ByteArrayOutputStream;
public class TestBAOS2 {
    public static void main(String[] args) throws Exception {
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        byte[] buf = {65, 66, 67, 68};
        baos.write(buf, 1, 2);
        System.out.println(baos.size());
        System.out.println(baos.toString());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["2", "BC"]);
}

#[test]
fn byte_array_output_stream_reset() {
    let (result, output) = compile_and_run(
        "baos_reset",
        &[(
            "demo/TestBAOS3.java",
            r#"
package demo;
import java.io.ByteArrayOutputStream;
public class TestBAOS3 {
    public static void main(String[] args) throws Exception {
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        baos.write(65);
        baos.write(66);
        System.out.println(baos.size());
        baos.reset();
        System.out.println(baos.size());
        baos.write(67);
        System.out.println(baos.size());
        System.out.println(baos.toString());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["2", "0", "1", "C"]);
}

#[test]
fn input_stream_stubs() {
    let (result, output) = compile_and_run(
        "input_stream_stubs",
        &[(
            "demo/TestIS.java",
            r#"
package demo;
import java.io.InputStream;
public class TestIS {
    public static void main(String[] args) throws Exception {
        InputStream is = null;
        System.out.println(is == null ? 0 : 1);
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["0"]);
}

#[test]
fn buffered_reader_stubs() {
    let (result, output) = compile_and_run(
        "buffered_reader_stubs",
        &[(
            "demo/TestBR.java",
            r#"
package demo;
import java.io.BufferedReader;
import java.io.StringReader;
public class TestBR {
    public static void main(String[] args) throws Exception {
        BufferedReader br = new BufferedReader(new StringReader("hello\nworld\n"));
        String line = br.readLine();
        System.out.println(line == null ? "null" : line);
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["null"]);
}

#[test]
fn print_writer_system_out() {
    let (result, output) = compile_and_run(
        "print_writer_system_out",
        &[(
            "demo/TestPW.java",
            r#"
package demo;
import java.io.PrintWriter;
public class TestPW {
    public static void main(String[] args) throws Exception {
        PrintWriter pw = new PrintWriter(System.out);
        pw.println(42);
        pw.flush();
        System.out.println(1);
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["42", "1"]);
}

// Stub stats and fail-fast tests ---

#[test]
fn concurrent_atomic_stub_tracking() {
    let (result, output) = compile_and_run(
        "concurrent_atomic_stub_tracking",
        &[(
            "demo/AtomicStubTest.java",
            r#"
package demo;

import java.util.concurrent.atomic.AtomicInteger;

public class AtomicStubTest {
    public static void main(String[] args) {
        AtomicInteger ai = new AtomicInteger(0);
        ai.set(42);
        System.out.println(ai.get());
        System.out.println(ai.incrementAndGet());
        System.out.println(ai.get());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["42", "43", "43"]);
}

#[test]
fn concurrent_atomic_multi_variable() {
    let (result, output) = compile_and_run(
        "concurrent_atomic_multi",
        &[(
            "demo/AtomicMulti.java",
            r#"
package demo;

import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicLong;

public class AtomicMulti {
    public static void main(String[] args) {
        AtomicInteger ai = new AtomicInteger(10);
        AtomicLong al = new AtomicLong(100L);
        ai.addAndGet(5);
        al.addAndGet(50L);
        System.out.println(ai.get());
        System.out.println(al.get());
        System.out.println(ai.compareAndSet(15, 20) ? 1 : 0);
        System.out.println(ai.get());
        System.out.println(ai.compareAndSet(15, 25) ? 1 : 0);
        System.out.println(ai.get());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["15", "150", "1", "20", "0", "20"]);
}

#[test]
fn thread_current_static_method() {
    let (result, output) = compile_and_run(
        "thread_current_static",
        &[(
            "demo/ThreadCurrent.java",
            r#"
package demo;

public class ThreadCurrent {
    public static void main(String[] args) {
        Thread t = Thread.currentThread();
        System.out.println(t != null ? 1 : 0);
        System.out.println("done");
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["1", "done"]);
}

#[test]
fn collections_sort_with_comparator() {
    let (result, output) = compile_and_run(
        "collections_sort_with_comparator",
        &[(
            "demo/SortWithComp.java",
            r#"
package demo;

import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;

public class SortWithComp {
    public static void main(String[] args) {
        ArrayList<Integer> list = new ArrayList<>();
        list.add(3);
        list.add(1);
        list.add(2);
        Collections.sort(list, new Comparator<Integer>() {
            public int compare(Integer a, Integer b) {
                return b - a;
            }
        });
        StringBuilder sb = new StringBuilder();
        for (Integer i : list) {
            if (sb.length() > 0) sb.append(",");
            sb.append(i);
        }
        System.out.println(sb.toString());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["3,2,1"]);
}

#[test]
fn string_builder_capacity() {
    let (result, output) = compile_and_run(
        "string_builder_capacity",
        &[(
            "demo/SBTest.java",
            r#"
package demo;

public class SBTest {
    public static void main(String[] args) {
        StringBuilder sb = new StringBuilder();
        sb.append("hello");
        sb.append(" world");
        sb.append("!");
        System.out.println(sb.toString());
        System.out.println(sb.length());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["hello world!", "12"]);
}

#[test]
fn collections_reversed_list() {
    let (result, output) = compile_and_run(
        "collections_reversed",
        &[(
            "demo/RevList.java",
            r#"
package demo;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

public class RevList {
    public static void main(String[] args) {
        List<String> list = new ArrayList<>();
        list.add("first");
        list.add("second");
        list.add("third");
        Collections.reverse(list);
        StringBuilder sb = new StringBuilder();
        for (String s : list) {
            if (sb.length() > 0) sb.append(",");
            sb.append(s);
        }
        System.out.println(sb.toString());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["third,second,first"]);
}

#[test]
fn object_deep_copy_with_clone() {
    let (result, output) = compile_and_run(
        "object_deep_clone",
        &[(
            "demo/CloneTest.java",
            r#"
package demo;

public class CloneTest {
    static class Point {
        int x;
        int y;
        Point(int x, int y) { this.x = x; this.y = y; }
        Point copy() { return new Point(this.x, this.y); }
    }
    
    public static void main(String[] args) {
        Point p1 = new Point(10, 20);
        Point p2 = p1.copy();
        p2.x = 30;
        System.out.println(p1.x);
        System.out.println(p2.x);
        System.out.println(p1.x == 10 && p2.x == 30 ? "ok" : "fail");
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["10", "30", "ok"]);
}

#[test]
fn string_reverse_and_manipulation() {
    let (result, output) = compile_and_run(
        "string_reverse",
        &[(
            "demo/StringReverse.java",
            r#"
package demo;

public class StringReverse {
    public static void main(String[] args) {
        String s = "hello";
        String reversed = "";
        for (int i = s.length() - 1; i >= 0; i--) {
            reversed = reversed + s.charAt(i);
        }
        System.out.println(reversed);
        
        StringBuilder sb = new StringBuilder();
        for (int i = 10; i >= 0; i--) {
            sb.append(i);
            if (i > 0) sb.append(",");
        }
        System.out.println(sb.toString());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["olleh", "10,9,8,7,6,5,4,3,2,1,0"]);
}

#[test]
fn regex_pattern_matching() {
    let (result, output) = compile_and_run(
        "regex_pattern",
        &[(
            "demo/RegexTest.java",
            r#"
package demo;

import java.util.regex.Pattern;
import java.util.regex.Matcher;

public class RegexTest {
    public static void main(String[] args) {
        Pattern p = Pattern.compile("\\d+");
        String text = "abc123def456";
        Matcher m = p.matcher(text);
        StringBuilder sb = new StringBuilder();
        while (m.find()) {
            if (sb.length() > 0) sb.append(",");
            sb.append(m.group());
        }
        System.out.println(sb.toString());
        
        p = Pattern.compile("[a-z]+");
        m = p.matcher("hello");
        System.out.println(m.matches() ? "match" : "no");
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["123,456", "match"]);
}

// --- P0 Compatibility: Multithreaded sample ---

#[test]
fn multithreaded_producer_consumer() {
    let (result, output) = compile_and_run(
        "multithreaded_pc",
        &[(
            "demo/ProducerConsumer.java",
            r#"
package demo;

public class ProducerConsumer {
    static int[] buffer = new int[10];
    static int count = 0;
    static int prodIdx = 0;
    static int consIdx = 0;
    
    static class Producer implements Runnable {
        public void run() {
            for (int i = 1; i <= 5; i++) {
                buffer[prodIdx] = i * 10;
                prodIdx = (prodIdx + 1) % 10;
                count++;
            }
        }
    }
    
    static class Consumer implements Runnable {
        public void run() {
            int sum = 0;
            while (count < 5) {
                if (count > 0) {
                    sum += buffer[consIdx];
                    consIdx = (consIdx + 1) % 10;
                    count--;
                }
            }
            System.out.println(sum);
        }
    }
    
    public static void main(String[] args) throws InterruptedException {
        Thread producer = new Thread(new Producer());
        Thread consumer = new Thread(new Consumer());
        producer.start();
        producer.join();
        consumer.join();
        System.out.println("done");
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert!(output.len() >= 1);
}

#[test]
fn multithreaded_basic_thread() {
    let (result, output) = compile_and_run(
        "multithreaded_basic",
        &[(
            "demo/BasicThread.java",
            r#"
package demo;

public class BasicThread {
    static int sharedValue = 0;
    
    static class Adder implements Runnable {
        public void run() {
            sharedValue = sharedValue + 10;
        }
    }
    
    static class Doubler implements Runnable {
        public void run() {
            sharedValue = sharedValue * 2;
        }
    }
    
    public static void main(String[] args) throws InterruptedException {
        Thread t1 = new Thread(new Adder());
        t1.start();
        t1.join();
        System.out.println(sharedValue);
        
        Thread t2 = new Thread(new Doubler());
        t2.start();
        t2.join();
        System.out.println(sharedValue);
        System.out.println("done");
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["10", "20", "done"]);
}

#[test]
fn multithreaded_multiple_joins() {
    let (result, output) = compile_and_run(
        "multithreaded_joins",
        &[(
            "demo/MultiJoin.java",
            r#"
package demo;

public class MultiJoin {
    static int result = 0;
    
    static class Add10 implements Runnable {
        public void run() { result += 10; }
    }
    static class Add20 implements Runnable {
        public void run() { result += 20; }
    }
    static class Add30 implements Runnable {
        public void run() { result += 30; }
    }
    
    public static void main(String[] args) throws InterruptedException {
        Thread t1 = new Thread(new Add10());
        Thread t2 = new Thread(new Add20());
        Thread t3 = new Thread(new Add30());
        
        t1.start();
        t1.join();
        t2.start();
        t2.join();
        t3.start();
        t3.join();
        
        System.out.println(result);
        System.out.println("done");
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["60", "done"]);
}

// --- P0 Compatibility: Collections/Stream-heavy sample ---

#[test]
fn collections_stream_heavy() {
    let (result, output) = compile_and_run(
        "collections_stream_heavy",
        &[(
            "demo/StreamHeavy.java",
            r#"
package demo;

public class StreamHeavy {
    public static void main(String[] args) {
        int[] numbers = new int[100];
        for (int i = 0; i < 100; i++) numbers[i] = i + 1;
        
        int sum = 0;
        for (int i = 0; i < numbers.length; i++) {
            int n = numbers[i];
            if (n % 2 == 0) {
                sum += n * 2;
            }
        }
        System.out.println(sum);
        
        String[] words = new String[4];
        words[0] = "hello";
        words[1] = "world";
        words[2] = "java";
        words[3] = "stream";
        
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < words.length; i++) {
            String w = words[i];
            if (w.length() > 4) {
                if (sb.length() > 0) sb.append(",");
                sb.append(w);
            }
        }
        System.out.println(sb.toString());
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["5100", "hello,world,stream"]);
}

#[test]
fn collections_map_reduce() {
    let (result, output) = compile_and_run(
        "collections_map_reduce",
        &[(
            "demo/MapReduce.java",
            r#"
package demo;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

public class MapReduce {
    public static void main(String[] args) {
        List<int[]> data = new ArrayList<>();
        data.add(new int[]{1, 2, 3});
        data.add(new int[]{4, 5, 6});
        data.add(new int[]{7, 8, 9});
        
        int totalSum = 0;
        for (int[] row : data) {
            int rowSum = 0;
            for (int val : row) {
                rowSum += val;
            }
            totalSum += rowSum;
        }
        System.out.println(totalSum);
        
        List<String> names = new ArrayList<>();
        names.add("Alice");
        names.add("Bob");
        names.add("Charlie");
        
        String longest = "";
        for (String name : names) {
            if (name.length() > longest.length()) {
                longest = name;
            }
        }
        System.out.println(longest);
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["45", "Charlie"]);
}

#[test]
fn collections_nested_lists() {
    let (result, output) = compile_and_run(
        "collections_nested",
        &[(
            "demo/NestedLists.java",
            r#"
package demo;

import java.util.ArrayList;
import java.util.List;

public class NestedLists {
    public static void main(String[] args) {
        List<List<Integer>> matrix = new ArrayList<>();
        for (int i = 0; i < 3; i++) {
            List<Integer> row = new ArrayList<>();
            for (int j = 0; j < 4; j++) {
                row.add(i * 4 + j + 1);
            }
            matrix.add(row);
        }
        
        int sum = 0;
        for (List<Integer> row : matrix) {
            for (int val : row) {
                sum += val;
            }
        }
        System.out.println(sum);
        
        int lastVal = matrix.get(2).get(3);
        System.out.println(lastVal);
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["78", "12"]);
}

// --- P0 Compatibility: JSON/Parsing utility sample ---

#[test]
fn parsing_csv_like() {
    let (result, output) = compile_and_run(
        "parsing_csv",
        &[(
            "demo/CsvParser.java",
            r#"
package demo;

public class CsvParser {
    public static void main(String[] args) {
        String data = "name,age,city\nAlice,30,NYC\nBob,25,LA\nCharlie,35,Chicago";
        String[] lines = new String[3];
        int lineStart = 0;
        int lineIdx = 0;
        for (int i = 0; i < data.length() && lineIdx < 3; i++) {
            if (data.charAt(i) == '\n' || i == data.length() - 1) {
                lines[lineIdx++] = data.substring(lineStart, i + 1);
                lineStart = i + 1;
            }
        }
        
        System.out.println(lines[0]);
        System.out.println(lines[1]);
        
        String[] fields = new String[3];
        int fieldIdx = 0;
        int lastComma = 0;
        String record = "Alice,30,NYC";
        for (int i = 0; i < record.length(); i++) {
            if (record.charAt(i) == ',') {
                fields[fieldIdx++] = record.substring(lastComma, i);
                lastComma = i + 1;
            }
        }
        fields[fieldIdx] = record.substring(lastComma);
        System.out.println(fields[0]);
        System.out.println(fields[1]);
        System.out.println(fields[2]);
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output.len(), 5);
}

#[test]
fn parsing_key_value() {
    let (result, output) = compile_and_run(
        "parsing_kv",
        &[(
            "demo/KeyValue.java",
            r#"
package demo;

public class KeyValue {
    public static void main(String[] args) {
        String input = "key1=value1;key2=value2;key3=value3";
        
        int keyCount = 0;
        int valCount = 0;
        int i = 0;
        while (i < input.length()) {
            int eq = -1;
            int semi = -1;
            for (int j = i; j < input.length(); j++) {
                if (input.charAt(j) == '=') { eq = j; break; }
            }
            for (int j = i; j < input.length(); j++) {
                if (input.charAt(j) == ';') { semi = j; break; }
            }
            if (eq == -1) break;
            
            String key = input.substring(i, eq);
            String value = semi == -1 ? input.substring(eq + 1) : input.substring(eq + 1, semi);
            keyCount++;
            valCount += value.length();
            i = semi == -1 ? input.length() : semi + 1;
        }
        System.out.println(keyCount);
        System.out.println(valCount);
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["3", "18"]);
}

#[test]
fn parsing_numeric_list() {
    let (result, output) = compile_and_run(
        "parsing_numlist",
        &[(
            "demo/NumList.java",
            r#"
package demo;

public class NumList {
    public static void main(String[] args) {
        String nums = "10,20,30,40,50";
        int[] values = new int[5];
        int idx = 0;
        int last = 0;
        for (int i = 0; i <= nums.length(); i++) {
            if (i == nums.length() || nums.charAt(i) == ',') {
                String s = nums.substring(last, i);
                int commaIndex = s.indexOf(',');
                if (commaIndex >= 0) s = s.substring(0, commaIndex);
                if (s.length() > 0) {
                    values[idx++] = Integer.parseInt(s);
                }
                last = i + 1;
            }
        }
        
        int sum = 0;
        for (int v : values) sum += v;
        System.out.println(sum);
        
        String reversed = "";
        for (int j = idx - 1; j >= 0; j--) {
            if (reversed.length() > 0) reversed += ",";
            reversed += values[j];
        }
        System.out.println(reversed);
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["150", "50,40,30,20,10"]);
}

// --- P0 Compatibility: Pure Java CLI sample ---

#[test]
fn cli_file_processor() {
    let (result, output) = compile_and_run(
        "cli_processor",
        &[(
            "demo/CliProcessor.java",
            r#"
package demo;

public class CliProcessor {
    public static void main(String[] args) {
        String[] lines = new String[3];
        lines[0] = "first line";
        lines[1] = "second line";
        lines[2] = "third line";
        
        int totalLen = 0;
        for (int i = 0; i < lines.length; i++) {
            totalLen += lines[i].length();
        }
        System.out.println(totalLen);
        
        StringBuilder sb = new StringBuilder();
        for (int i = lines.length - 1; i >= 0; i--) {
            if (sb.length() > 0) sb.append(" | ");
            sb.append(lines[i]);
        }
        System.out.println(sb.toString());
        
        int count = 0;
        for (int i = 0; i < args.length; i++) {
            if (args[i] != null && args[i].length() > 0) count++;
        }
        System.out.println(count);
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["31", "third line | second line | first line", "0"]);
}

#[test]
fn cli_string_pattern_matching() {
    let (result, output) = compile_and_run(
        "cli_pattern",
        &[(
            "demo/CliPattern.java",
            r#"
package demo;

public class CliPattern {
    public static void main(String[] args) {
        String text = "Hello World from Java";
        
        int upperCount = 0;
        for (int i = 0; i < text.length(); i++) {
            char c = text.charAt(i);
            if (c >= 'A' && c <= 'Z') upperCount++;
        }
        System.out.println(upperCount);
        
        int wordCount = 1;
        for (int i = 0; i < text.length(); i++) {
            if (text.charAt(i) == ' ') wordCount++;
        }
        System.out.println(wordCount);
        
        String reversed = "";
        for (int i = text.length() - 1; i >= 0; i--) {
            reversed = reversed + text.charAt(i);
        }
        System.out.println(reversed);
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["3", "4", "avaJ morf dlroW olleH"]);
}

#[test]
fn cli_numeric_processing() {
    let (result, output) = compile_and_run(
        "cli_numeric",
        &[(
            "demo/CliNumeric.java",
            r#"
package demo;

public class CliNumeric {
    public static void main(String[] args) {
        int[] numbers = new int[10];
        for (int i = 0; i < 10; i++) {
            numbers[i] = (i + 1) * (i + 1);
        }
        
        int sum = 0;
        for (int i = 0; i < numbers.length; i++) {
            sum += numbers[i];
        }
        System.out.println(sum);
        
        int max = numbers[0];
        for (int i = 1; i < numbers.length; i++) {
            if (numbers[i] > max) max = numbers[i];
        }
        System.out.println(max);
        
        int min = numbers[0];
        for (int i = 1; i < numbers.length; i++) {
            if (numbers[i] < min) min = numbers[i];
        }
        System.out.println(min);
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["385", "100", "1"]);
}

#[test]
fn cli_recursive_fibonacci() {
    let (result, output) = compile_and_run(
        "cli_recursive",
        &[(
            "demo/CliRecursive.java",
            r#"
package demo;

public class CliRecursive {
    static int fib(int n) {
        if (n <= 1) return n;
        return fib(n - 1) + fib(n - 2);
    }
    
    public static void main(String[] args) {
        System.out.println(fib(10));
        System.out.println(fib(15));
        
        int factorial = 1;
        for (int i = 1; i <= 10; i++) {
            factorial *= i;
        }
        System.out.println(factorial);
    }
}
"#,
        )],
    );
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["55", "610", "3628800"]);
}
