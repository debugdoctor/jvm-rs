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
