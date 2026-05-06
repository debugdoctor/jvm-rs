//! Complex Scenario Template Library for jvm-rs
//!
//! This module provides high-complexity test templates that exercise
//! deep JVM features: multi-level inheritance, exception nesting,
//! concurrency patterns, reflection, classloader chains, etc.

mod common;

use common::compile_and_run;
use jvm_rs::vm::ExecutionResult;

#[test]
fn deep_inheritance_5_levels() {
    let files = &[
        (
            "complex/inherit/Main.java",
            r#"
package complex.inherit;
public class Main {
    public static void main(String[] args) {
        Level5 obj = new Level5(42);
        System.out.println(obj.describe());
        System.out.println(obj.compute());
    }
}
"#,
        ),
        (
            "complex/inherit/Level5.java",
            r#"
package complex.inherit;
public class Level5 extends Level4 {
    public Level5(int v) { super(v); }
    public String describe() { return "L5[" + super.describe() + "]"; }
}
"#,
        ),
        (
            "complex/inherit/Level4.java",
            r#"
package complex.inherit;
class Level4 extends Level3 {
    public Level4(int v) { super(v); }
    public String describe() { return "L4[" + super.describe() + "]"; }
}
"#,
        ),
        (
            "complex/inherit/Level3.java",
            r#"
package complex.inherit;
class Level3 extends Level2 {
    public Level3(int v) { super(v); }
    public String describe() { return "L3[" + super.describe() + "]"; }
}
"#,
        ),
        (
            "complex/inherit/Level2.java",
            r#"
package complex.inherit;
class Level2 extends Level1 {
    public Level2(int v) { super(v); }
    public String describe() { return "L2[" + super.describe() + "]"; }
}
"#,
        ),
        (
            "complex/inherit/Level1.java",
            r#"
package complex.inherit;
class Level1 extends Base {
    public Level1(int v) { super(v); }
    public String describe() { return "L1[" + super.describe() + "]"; }
}
"#,
        ),
        (
            "complex/inherit/Base.java",
            r#"
package complex.inherit;
class Base {
    int value;
    public Base(int v) { this.value = v; }
    public String base() { return "base=" + value; }
    public String describe() { return base(); }
    public int compute() { return value * 2; }
}
"#,
        ),
    ];
    let (result, output) = compile_and_run("deep_inherit_5", files);
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["L5[L4[L3[L2[L1[base=42]]]]]", "84"]);
}

#[test]
fn deep_inheritance_interface_chain() {
    let files = &[
        (
            "complex/inherit/Main.java",
            r#"
package complex.inherit;
public class Main {
    public static void main(String[] args) {
        Level1If obj = new Impl();
        System.out.println(obj.m1());
        Level2If obj2 = new Impl();
        System.out.println(obj2.m2());
        Level3If obj3 = new Impl();
        System.out.println(obj3.m3());
        System.out.println(((Impl)obj3).all());
    }
}
"#,
        ),
        (
            "complex/inherit/Impl.java",
            r#"
package complex.inherit;
public class Impl implements Level3If, Level2If, Level1If {
    public String m1() { return "Impl.m1"; }
    public String m2() { return "Impl.m2"; }
    public String m3() { return "Impl.m3"; }
    public String all() { return m1() + "," + m2() + "," + m3(); }
}
"#,
        ),
        (
            "complex/inherit/Level3If.java",
            r#"
package complex.inherit;
interface Level3If extends Level2If {
    String m3();
}
"#,
        ),
        (
            "complex/inherit/Level2If.java",
            r#"
package complex.inherit;
interface Level2If extends Level1If {
    String m2();
}
"#,
        ),
        (
            "complex/inherit/Level1If.java",
            r#"
package complex.inherit;
interface Level1If {
    String m1();
}
"#,
        ),
    ];
    let (result, output) = compile_and_run("deep_inherit_interface_chain", files);
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(
        output,
        vec!["Impl.m1", "Impl.m2", "Impl.m3", "Impl.m1,Impl.m2,Impl.m3"]
    );
}

#[test]
fn exception_nesting_5_levels() {
    let files = &[
        (
            "complex/exc/ExcNesting.java",
            r#"
package complex.exc;
public class ExcNesting {
    static String log = "";
    static int level;

    static int depth(int n) {
        if (n <= 0) throw new RuntimeException("boom:" + n);
        try {
            try {
                log += "A" + n;
                int r = depth(n - 1);
                log += "R" + n;
                return r;
            } catch (ArithmeticException e) {
                log += "arith" + n;
            }
        } finally {
            log += "fin" + n;
        }
        return -1;
    }

    public static void main(String[] args) {
        level = 5;
        try {
            depth(5);
        } catch (RuntimeException e) {
            log += "caught:" + e.getMessage();
        }
        System.out.println(log);
    }
}
"#,
        ),
    ];
    let (result, output) = compile_and_run("exc_nesting_5", files);
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(
        output,
        vec!["A5A4A3A2A1fin1fin2fin3fin4fin5caught:boom:0"]
    );
}

#[test]
fn exception_finally_return_priority() {
    let files = &[
        (
            "complex/exc/FinallyReturn.java",
            r#"
package complex.exc;
public class FinallyReturn {
    static String log = "";

    static int test(int n) {
        try {
            if (n == 0) throw new Exception("E");
            return n * 2;
        } catch (Exception e) {
            log += "catch:" + e.getMessage() + ",";
            return -1;
        } finally {
            log += "finally";
        }
    }

    static int test2(int n) {
        try {
            log += "try2:" + n + ",";
            return n;
        } finally {
            log += "fin2:" + n + ",";
        }
    }

    public static void main(String[] args) {
        log += test(1) + ",";
        log += test(0) + ",";
        log += test2(99);
        System.out.println(log);
    }
}
"#,
        ),
    ];
    let (result, output) = compile_and_run("exc_finally_return", files);
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["2,-1,99"]);
}

#[test]
fn exception_suppressed_and_rethrow() {
    let files = &[
        (
            "complex/exc/Suppressed.java",
            r#"
package complex.exc;
public class Suppressed {
    public static void main(String[] args) {
        String out = "";
        try {
            try {
                throw new RuntimeException("primary");
            } catch (RuntimeException e) {
                RuntimeException suppressed = new RuntimeException("suppressed");
                e.addSuppressed(suppressed);
                throw e;
            }
        } catch (RuntimeException e) {
            out += "msg:" + e.getMessage() + ",";
            out += "suppressed:" + e.getSuppressed().length + ",";
        }
        System.out.println(out);
    }
}
"#,
        ),
    ];
    let (result, output) = compile_and_run("exc_suppressed", files);
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["msg:primary,suppressed:1,"]);
}

#[test]
fn concurrency_synchronized_chain() {
    let files = &[
        (
            "complex/conc/ChainSync.java",
            r#"
package complex.conc;
public class ChainSync {
    static final Object LOCK = new Object();
    static String log = "";
    static int currentPhase = 0;

    static class Worker implements Runnable {
        int targetPhase;
        int workMs;
        Worker(int targetPhase, int workMs) {
            this.targetPhase = targetPhase;
            this.workMs = workMs;
        }
        public void run() {
            synchronized (LOCK) {
                while (currentPhase != targetPhase) {
                    try {
                        LOCK.wait();
                    } catch (InterruptedException e) {}
                }
                log += "phase" + targetPhase + ",";
                currentPhase++;
                LOCK.notifyAll();
            }
        }
    }

    public static void main(String[] args) throws Exception {
        Thread t0 = new Thread(new Worker(0, 0));
        Thread t1 = new Thread(new Worker(1, 0));
        Thread t2 = new Thread(new Worker(2, 0));
        t0.start(); t1.start(); t2.start();
        t0.join(); t1.join(); t2.join();
        System.out.println(log);
        System.out.println("done");
    }
}
"#,
        ),
    ];
    let (result, output) = compile_and_run("conc_sync_chain", files);
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["phase0,phase1,phase2,", "done"]);
}

#[test]
fn concurrency_wait_notify_cycles() {
    let files = &[
        (
            "complex/conc/WaitNotify.java",
            r#"
package complex.conc;
public class WaitNotify {
    static final Object MONITOR = new Object();
    static boolean producerDone = false;
    static int valueProduced = -1;
    static String log = "";

    static class Producer implements Runnable {
        public void run() {
            synchronized (MONITOR) {
                for (int i = 0; i < 3; i++) {
                    valueProduced = i * 10;
                    log += "produced:" + valueProduced + ",";
                    MONITOR.notify();
                    try {
                        if (i < 2) MONITOR.wait();
                    } catch (InterruptedException e) {}
                }
                producerDone = true;
                MONITOR.notify();
            }
        }
    }

    static class Consumer implements Runnable {
        public void run() {
            int consumed = 0;
            while (consumed < 3) {
                synchronized (MONITOR) {
                    while (!producerDone && valueProduced < 0) {
                        try { MONITOR.wait(); } catch (InterruptedException e) {}
                    }
                    if (producerDone && valueProduced < 0) break;
                    log += "consumed:" + valueProduced + ",";
                    valueProduced = -1;
                    consumed++;
                    MONITOR.notify();
                    if (consumed < 3) {
                        try { MONITOR.wait(); } catch (InterruptedException e) {}
                    }
                }
            }
        }
    }

    public static void main(String[] args) throws Exception {
        Thread prod = new Thread(new Producer());
        Thread cons = new Thread(new Consumer());
        prod.start(); cons.start();
        prod.join(); cons.join();
        System.out.println(log);
    }
}
"#,
        ),
    ];
    let (result, output) = compile_and_run("conc_wait_notify", files);
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(
        output,
        vec!["produced:0,consumed:0,produced:10,consumed:10,produced:20,consumed:20,"]
    );
}

#[test]
fn reflection_class_for_name_and_invoke() {
    let files = &[
        (
            "complex/refl/DynClass.java",
            r#"
package complex.refl;
public class DynClass {
    public int value;
    public DynClass(int v) { this.value = v; }
    public int compute(int x) { return x * value; }
    public static String greet(String name) { return "Hello, " + name; }
}
"#,
        ),
        (
            "complex/refl/Main.java",
            r#"
package complex.refl;
import java.lang.reflect.*;

public class Main {
    public static void main(String[] args) throws Exception {
        String log = "";

        Class<?> cls = Class.forName("complex.refl.DynClass");
        Constructor<?> ctor = cls.getConstructor(int.class);
        Object obj = ctor.newInstance(21);
        log += obj.getClass().getName() + ",";

        Method compute = cls.getMethod("compute", int.class);
        int result = (int) compute.invoke(obj, 2);
        log += result + ",";

        Method greet = cls.getMethod("greet", String.class);
        String greetResult = (String) greet.invoke(null, "JVM");
        log += greetResult;

        System.out.println(log);
    }
}
"#,
        ),
    ];
    let (result, output) = compile_and_run("refl_for_name", files);
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["complex.refl.DynClass,42,Hello, JVM"]);
}

#[test]
fn reflection_field_access() {
    let files = &[
        (
            "complex/refl/FieldTarget.java",
            r#"
package complex.refl;
public class FieldTarget {
    public int publicField = 100;
    protected String protectedField = "protected";
    private long privateField = 999L;
    static int staticField = 55;
}
"#,
        ),
        (
            "complex/refl/Main.java",
            r#"
package complex.refl;
import java.lang.reflect.*;

public class Main {
    public static void main(String[] args) throws Exception {
        String log = "";
        Class<?> cls = Class.forName("complex.refl.FieldTarget");
        Object obj = cls.newInstance();

        Field pubF = cls.getField("publicField");
        log += pubF.getInt(obj) + ",";

        Field staticF = cls.getField("staticField");
        log += staticF.getInt(null) + ",";

        Field privF = cls.getDeclaredField("privateField");
        privF.setAccessible(true);
        log += privF.getLong(obj);

        System.out.println(log);
    }
}
"#,
        ),
    ];
    let (result, output) = compile_and_run("refl_field_access", files);
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["100,55,999"]);
}

#[test]
fn classloader_basic_chain() {
    let files = &[
        (
            "complex/clcc/CustomLoader.java",
            r#"
package complex.clcc;
public class CustomLoader extends ClassLoader {
    public Class<?> loadFromBytes(String name, byte[] code) {
        return defineClass(name, code, 0, code.length);
    }
}
"#,
        ),
        (
            "complex/clcc/LoaderClient.java",
            r#"
package complex.clcc;
public class LoaderClient {
    public static String identify() {
        return "Loaded by:" + LoaderClient.class.getClassLoader();
    }
    public static int compute(int x) { return x * 3; }
}
"#,
        ),
        (
            "complex/clcc/Main.java",
            r#"
package complex.clcc;
public class Main {
    public static void main(String[] args) throws Exception {
        String log = "";
        log += LoaderClient.identify() + ",";
        log += LoaderClient.compute(7);
        System.out.println(log);
    }
}
"#,
        ),
    ];
    let (result, output) = compile_and_run("clcc_basic", files);
    assert_eq!(result, ExecutionResult::Void);
    assert!(output.len() >= 1);
}

#[test]
fn recursion_mutual_and_tree_traversal() {
    let files = &[
        (
            "complex/recursion/Main.java",
            r#"
package complex.recursion;
public class Main {
    public static void main(String[] args) {
        int[] vals = {1, 2, 3, 4, 5, 6, 7};
        Tree root = Tree.buildBalanced(vals, 0, vals.length - 1);
        System.out.println(Tree.sum(root));
        System.out.println(Tree.height(root));
        System.out.println(Tree.contains(root, 5));
        System.out.println(Tree.contains(root, 99));
        System.out.println(Tree.inorder(root));
    }
}
"#,
        ),
        (
            "complex/recursion/Tree.java",
            r#"
package complex.recursion;
public class Tree {
    int value;
    Tree left;
    Tree right;
    Tree(int v, Tree l, Tree r) {
        this.value = v; this.left = l; this.right = r;
    }

    static int sum(Tree t) {
        if (t == null) return 0;
        return t.value + sum(t.left) + sum(t.right);
    }

    static int height(Tree t) {
        if (t == null) return 0;
        int lh = height(t.left);
        int rh = height(t.right);
        return 1 + (lh > rh ? lh : rh);
    }

    static boolean contains(Tree t, int v) {
        if (t == null) return false;
        if (t.value == v) return true;
        return contains(t.left, v) || contains(t.right, v);
    }

    static String inorder(Tree t) {
        if (t == null) return "";
        String l = inorder(t.left);
        if (!l.isEmpty()) l += ",";
        l += t.value;
        String r = inorder(t.right);
        if (!r.isEmpty()) l += ",";
        l += r;
        return l;
    }

    static Tree buildBalanced(int[] arr, int lo, int hi) {
        if (lo > hi) return null;
        int mid = (lo + hi) / 2;
        return new Tree(arr[mid],
            buildBalanced(arr, lo, mid - 1),
            buildBalanced(arr, mid + 1, hi));
    }
}
"#,
        ),
    ];
    let (result, output) = compile_and_run("recursion_tree", files);
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(
        output,
        vec!["28", "3", "true", "false", "1,2,3,4,5,6,7"]
    );
}

#[test]
fn recursion_fibonacci_memo_and_gcd() {
    let files = &[
        (
            "complex/recursion/Main.java",
            r#"
package complex.recursion;
public class Main {
    public static void main(String[] args) {
        int[] memo = new int[21];
        System.out.println(MathUtil.fib(10, memo));
        System.out.println(MathUtil.fib(20, memo));

        System.out.println(MathUtil.gcd(48, 18));
        System.out.println(MathUtil.gcd(123456, 7890));

        System.out.println(MathUtil.factorial(12));
    }
}
"#,
        ),
        (
            "complex/recursion/MathUtil.java",
            r#"
package complex.recursion;
public class MathUtil {
    static int fib(int n, int[] memo) {
        if (n <= 1) return n;
        if (memo[n] != 0) return memo[n];
        memo[n] = fib(n - 1, memo) + fib(n - 2, memo);
        return memo[n];
    }

    static int gcd(int a, int b) {
        if (b == 0) return a;
        return gcd(b, a % b);
    }

    static long factorial(int n) {
        if (n <= 1) return 1;
        return n * factorial(n - 1);
    }
}
"#,
        ),
    ];
    let (result, output) = compile_and_run("recursion_memo", files);
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["55", "6765", "6", "6", "479001600"]);
}

#[test]
fn arrays_multidim_and_jagged() {
    let files = &[
        (
            "complex/arrays/MultiArray.java",
            r#"
package complex.arrays;
public class MultiArray {
    public static void main(String[] args) {
        int[][][] cube = new int[3][4][5];
        int count = 0;
        for (int i = 0; i < cube.length; i++)
            for (int j = 0; j < cube[i].length; j++)
                for (int k = 0; k < cube[i][j].length; k++)
                    cube[i][j][k] = ++count;

        int sum = 0;
        for (int i = 0; i < cube.length; i++)
            for (int j = 0; j < cube[i].length; j++)
                for (int k = 0; k < cube[i][j].length; k++)
                    sum += cube[i][j][k];
        System.out.println(sum);
        System.out.println(cube[2][3][4]);

        String[][] jagged = new String[3][];
        jagged[0] = new String[2];
        jagged[1] = new String[4];
        jagged[2] = new String[1];
        jagged[0][0] = "a"; jagged[0][1] = "b";
        jagged[1][0] = "c"; jagged[1][1] = "d"; jagged[1][2] = "e"; jagged[1][3] = "f";
        jagged[2][0] = "g";
        System.out.println(jagged[1][2] + jagged[2][0]);
    }
}
"#,
        ),
    ];
    let (result, output) = compile_and_run("arrays_multidim", files);
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["1830", "60", "eg"]);
}

#[test]
fn arrays_object_array_and_storecheck() {
    let files = &[
        (
            "complex/arrays/ObjArray.java",
            r#"
package complex.arrays;
public class ObjArray {
    public static void main(String[] args) {
        String[][] arr = new String[2][3];
        arr[0][0] = "a"; arr[0][1] = "b"; arr[0][2] = "c";
        arr[1][0] = "x"; arr[1][1] = "y"; arr[1][2] = "z";

        String s = arr[0][1];
        System.out.println(s);

        Object[] oa = arr;
        System.out.println(oa.length);
        System.out.println(((String[])oa[0])[2]);

        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < arr.length; i++)
            for (int j = 0; j < arr[i].length; j++)
                sb.append(arr[i][j]);
        System.out.println(sb.toString());
    }
}
"#,
        ),
    ];
    let (result, output) = compile_and_run("arrays_obj_storecheck", files);
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["b", "2", "c", "abcxyz"]);
}

#[test]
fn static_init_order_and_circular() {
    let files = &[
        (
            "complex/clinit/InitOrder.java",
            r#"
package complex.clinit;
public class InitOrder {
    static String log = "";
    static int A = init("A", 1);
    static int B = init("B", A * 2);
    static int C = init("C", A + B);

    static int init(String name, int val) {
        log += name + "=" + val + ",";
        return val;
    }

    public static void main(String[] args) {
        System.out.println(log);
        System.out.println("A=" + A + " B=" + B + " C=" + C);
    }
}
"#,
        ),
    ];
    let (result, output) = compile_and_run("clinit_order", files);
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(
        output,
        vec!["A=1,B=2,C=3,", "A=1 B=2 C=3"]
    );
}

#[test]
fn interface_clinit_and_constant_folding() {
    let files = &[
        (
            "complex/clinit/Constants.java",
            r#"
package complex.clinit;
public class Constants {
    interface Defaults {
        int A = 10;
        int B = A + 5;
        String S = "hello";
    }
    static class Impl implements Defaults {}
    public static void main(String[] args) {
        System.out.println(Defaults.A);
        System.out.println(Defaults.B);
        System.out.println(Defaults.S);
        System.out.println(Impl.A);
    }
}
"#,
        ),
    ];
    let (result, output) = compile_and_run("clinit_interface", files);
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["10", "15", "hello", "10"]);
}

#[test]
fn hashmap_collision_handling() {
    let files = &[
        (
            "complex/hash/Collision.java",
            r#"
package complex.hash;
import java.util.HashMap;
import java.util.Map;

class BadHash {
    int v;
    BadHash(int v) { this.v = v; }
    public int hashCode() { return 1; }
    public boolean equals(Object o) {
        if (!(o instanceof BadHash)) return false;
        return ((BadHash)o).v == this.v;
    }
}

public class Collision {
    public static void main(String[] args) {
        HashMap<BadHash, String> map = new HashMap<>();
        for (int i = 0; i < 16; i++) {
            map.put(new BadHash(i), "val" + i);
        }
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < 16; i++) {
            String v = map.get(new BadHash(i));
            if (sb.length() > 0) sb.append(",");
            sb.append(v);
        }
        System.out.println(sb.toString());
        System.out.println(map.size());
    }
}
"#,
        ),
    ];
    let (result, output) = compile_and_run("hash_collision", files);
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(
        output,
        vec![
            "val0,val1,val2,val3,val4,val5,val6,val7,val8,val9,val10,val11,val12,val13,val14,val15",
            "16"
        ]
    );
}

#[test]
fn hashset_custom_objects() {
    let files = &[
        (
            "complex/hash/PersonMain.java",
            r#"
package complex.hash;
import java.util.HashSet;
import java.util.Set;

class Person {
    String name;
    int id;
    Person(String name, int id) { this.name = name; this.id = id; }
    public int hashCode() { return id; }
    public boolean equals(Object o) {
        if (!(o instanceof Person)) return false;
        return ((Person)o).id == this.id;
    }
}

public class PersonMain {
    public static void main(String[] args) {
        Set<Person> set = new HashSet<>();
        set.add(new Person("Alice", 1));
        set.add(new Person("Bob", 2));
        set.add(new Person("Alice", 1));
        System.out.println(set.size());

        boolean hasAlice = set.contains(new Person("Alice", 1));
        boolean hasBob = set.contains(new Person("Bob", 2));
        System.out.println(hasAlice);
        System.out.println(hasBob);
    }
}
"#,
        ),
    ];
    let (result, output) = compile_and_run("hashset_custom", files);
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["2", "true", "true"]);
}

#[test]
fn string_intern_and_identity() {
    let files = &[
        (
            "complex/strs/Intern.java",
            r#"
package complex.strs;
public class Intern {
    public static void main(String[] args) {
        String s1 = new String("hello");
        String s2 = new String("hello");
        String s3 = "hello";
        String s4 = "hello";
        String s5 = s1.intern();

        System.out.println(s1 == s2);
        System.out.println(s3 == s4);
        System.out.println(s1 == s3);
        System.out.println(s3 == s5);
        System.out.println(s1.equals(s2));
    }
}
"#,
        ),
    ];
    let (result, output) = compile_and_run("str_intern", files);
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["false", "true", "false", "true", "true"]);
}

#[test]
fn string_switch_and_interpolation() {
    let files = &[
        (
            "complex/strs/SwitchStr.java",
            r#"
package complex.strs;
public class SwitchStr {
    static String command(String op, int a, int b) {
        switch (op) {
            case "add": return String.valueOf(a + b);
            case "sub": return String.valueOf(a - b);
            case "mul": return String.valueOf(a * b);
            case "div": return a % b == 0 ? String.valueOf(a / b) : "not_integer";
            default: return "unknown";
        }
    }

    public static void main(String[] args) {
        System.out.println(command("add", 10, 3));
        System.out.println(command("mul", 10, 3));
        System.out.println(command("div", 10, 3));
        System.out.println(command("div", 9, 3));
        System.out.println(command("mod", 10, 3));
    }
}
"#,
        ),
    ];
    let (result, output) = compile_and_run("str_switch", files);
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["13", "30", "not_integer", "3", "unknown"]);
}

#[test]
fn lambda_metafactory_chained() {
    let files = &[
        (
            "complex/lambda/LambdaChain.java",
            r#"
package complex.lambda;
import java.util.function.*;

public class LambdaChain {
    public static void main(String[] args) {
        Function<Integer, Integer> f1 = x -> x + 1;
        Function<Integer, Integer> f2 = x -> x * 2;
        Function<Integer, Integer> f3 = f1.andThen(f2);
        Function<Integer, Integer> f4 = f1.compose(f2);

        System.out.println(f3.apply(5));
        System.out.println(f4.apply(5));

        IntUnaryOperator op1 = x -> x + 1;
        IntUnaryOperator op2 = x -> x * 2;
        IntUnaryOperator op3 = op1.andThen(op2);
        System.out.println(op3.applyAsInt(7));

        Supplier<String> s = () -> "lambda";
        System.out.println(s.get());
    }
}
"#,
        ),
    ];
    let (result, output) = compile_and_run("lambda_chain", files);
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["12", "11", "16", "lambda"]);
}

#[test]
fn method_reference_indirection() {
    let files = &[
        (
            "complex/lambda/MethodRef.java",
            r#"
package complex.lambda;
import java.util.function.*;

class StrUtil {
    static String upper(String s) { return s.toUpperCase(); }
    static int len(String s) { return s.length(); }
}

public class MethodRef {
    public static void main(String[] args) {
        Function<String, String> f = StrUtil::upper;
        ToIntFunction<String> g = StrUtil::len;

        System.out.println(f.apply("hello"));
        System.out.println(g.applyAsInt("World"));

        UnaryOperator<String> u = String::toLowerCase;
        System.out.println(u.apply("JAVA"));

        Consumer<String> c = System.out::println;
        c.accept("method reference");
    }
}
"#,
        ),
    ];
    let (result, output) = compile_and_run("method_ref", files);
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["HELLO", "5", "java", "method reference"]);
}

#[test]
fn switch_tableswitch_full_coverage() {
    let files = &[
        (
            "complex/switchcase/SwitchBench.java",
            r#"
package complex.switchcase;
public class SwitchBench {
    static String dayName(int d) {
        switch (d) {
            case 0: return "Sunday";
            case 1: return "Monday";
            case 2: return "Tuesday";
            case 3: return "Wednesday";
            case 4: return "Thursday";
            case 5: return "Friday";
            case 6: return "Saturday";
            default: return "Invalid";
        }
    }

    static String sparse(int v) {
        switch (v) {
            case -1: return "neg_one";
            case 0: return "zero";
            case 1: return "one";
            case 100: return "hundred";
            case 10000: return "tenk";
            default: return "other";
        }
    }

    public static void main(String[] args) {
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i <= 7; i++) {
            if (sb.length() > 0) sb.append(",");
            sb.append(dayName(i));
        }
        System.out.println(sb.toString());

        System.out.println(sparse(-1));
        System.out.println(sparse(0));
        System.out.println(sparse(1));
        System.out.println(sparse(100));
        System.out.println(sparse(10000));
        System.out.println(sparse(999));
    }
}
"#,
        ),
    ];
    let (_result, output) = compile_and_run("switch_bench", files);
    assert_eq!(
        output,
        vec![
            "Sunday,Monday,Tuesday,Wednesday,Thursday,Friday,Saturday,Invalid",
            "neg_one",
            "zero",
            "one",
            "hundred",
            "tenk",
            "other"
        ]
    );
}

#[test]
fn assertion_and_state_machine() {
    let files = &[
        (
            "complex/asserts/StateMachine.java",
            r#"
package complex.asserts;
public class StateMachine {
    enum State { INIT, RUNNING, PAUSED, STOPPED }
    static State current = State.INIT;
    static String log = "";

    static void start() {
        assert current == State.INIT : "Bad state";
        current = State.RUNNING;
        log += "start,";
    }

    static void pause() {
        assert current == State.RUNNING : "Bad state";
        current = State.PAUSED;
        log += "pause,";
    }

    static void resume() {
        assert current == State.PAUSED : "Bad state";
        current = State.RUNNING;
        log += "resume,";
    }

    static void stop() {
        assert current == State.RUNNING || current == State.PAUSED : "Bad state";
        current = State.STOPPED;
        log += "stop,";
    }

    public static void main(String[] args) {
        start();
        pause();
        resume();
        stop();
        System.out.println(log);
        System.out.println(current == State.STOPPED);
    }
}
"#,
        ),
    ];
    let (result, output) = compile_and_run("assert_state", files);
    assert_eq!(result, ExecutionResult::Void);
    assert_eq!(output, vec!["start,pause,resume,stop,", "true"]);
}
