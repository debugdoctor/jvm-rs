# jvm-rs

A JVM implementation in Rust, aligned with the Java SE 21 JVM Specification (JVMS 21).

## Usage

```sh
# Build
cargo build --release

# Run
jvm-rs -cp <classpath> <MainClass> [args...]

# Example
jvm-rs -cp examples demo.Main

# Multiple classpath entries (colon-separated)
jvm-rs -cp lib:classes demo.Main

# Enable bytecode execution trace
jvm-rs -Xtrace -cp examples demo.Main
```

## Features

### Instruction Set Coverage

**Constants** (all implemented)
`aconst_null`, `iconst_m1`..`iconst_5`, `lconst_0/1`, `fconst_0/1/2`, `dconst_0/1`, `bipush`, `sipush`, `ldc`, `ldc_w`, `ldc2_w`

**Loads** (all implemented)
`iload`, `lload`, `fload`, `dload`, `aload` + all `_0`..`_3` shortforms, `iaload`, `laload`, `faload`, `daload`, `aaload`, `baload`, `caload`, `saload`

**Stores** (all implemented)
`istore`, `lstore`, `fstore`, `dstore`, `astore` + all `_0`..`_3` shortforms, `iastore`, `lastore`, `fastore`, `dastore`, `aastore`, `bastore`, `castore`, `sastore`

**Stack** (all implemented)
`pop`, `pop2`, `dup`, `dup_x1`, `dup_x2`, `dup2`, `dup2_x1`, `dup2_x2`, `swap`

**Math** (all implemented)
`iadd`..`ineg`, `ladd`..`lneg`, `fadd`..`fneg`, `dadd`..`dneg`, `ishl`, `ishr`, `iushr`, `lshl`, `lshr`, `lushr`, `iand`, `ior`, `ixor`, `land`, `lor`, `lxor`, `iinc`

**Conversions** (all implemented)
`i2l`, `i2f`, `i2d`, `l2i`, `l2f`, `l2d`, `f2i`, `f2l`, `f2d`, `d2i`, `d2l`, `d2f`, `i2b`, `i2c`, `i2s`

**Comparisons** (all implemented)
`ifeq`, `ifne`, `iflt`, `ifge`, `ifgt`, `ifle`, `if_icmpeq`, `if_icmpne`, `if_icmplt`, `if_icmpge`, `if_icmpgt`, `if_icmple`, `if_acmpeq`, `if_acmpne`, `lcmp`, `fcmpl`, `fcmpg`, `dcmpl`, `dcmpg`, `instanceof`

**Control** (all implemented)
`goto`, `goto_w`, `jsr`, `jsr_w`, `ret`, `tableswitch`, `lookupswitch`, `ireturn`, `lreturn`, `freturn`, `dreturn`, `areturn`, `return`

**References** (implemented, with partial `invokedynamic` support)
`getstatic`, `putstatic`, `getfield`, `putfield`, `invokevirtual`, `invokespecial`, `invokestatic`, `invokeinterface`, `invokedynamic`, `new`, `newarray`, `anewarray`, `multianewarray`, `arraylength`, `athrow`, `checkcast`, `instanceof`, `monitorenter`, `monitorexit`, `ifnull`, `ifnonnull`

**Extended**: `wide`

| Not yet implemented | Notes |
|---|---|
| Full `invokedynamic` bootstrap coverage | Lambda proxies and `StringConcatFactory` are supported; other bootstrap patterns may still be incomplete |

### Runtime Features

- **Class loading**: On-demand from classpath, multiple entries supported
- **Method invocation**: Full call stack, virtual dispatch with super-class resolution, interface dispatch
- **Object model**: Heap-allocated objects with fields, all primitive types (int/long/float/double), arrays (all types, multi-dimensional)
- **Exception handling**: Exception tables, `athrow`, try-catch-finally, VM errors converted to Java exceptions (NPE, AIOOBE, ArithmeticException, ClassCast, NegativeArraySize)
- **Static initializers**: `<clinit>` executed on first class access
- **Threading model**: `Vm::spawn()` shares heap, loaded classes, class initialization state, monitors, and captured output across threads
- **Java thread API**: Built-in `Thread` / `Runnable` support with `Thread.start()` and `Thread.join()`
- **Monitors**: Reentrant lock count per object, blocking monitor acquisition across threads, `Object.wait/notify/notifyAll`, `IllegalMonitorStateException` on invalid exit
- **Verification**: Structural and data-flow bytecode verification with `StackMapTable` checks
- **`invokedynamic`**: Supports lambda proxies via `LambdaMetafactory` and modern string concatenation via `StringConcatFactory`

### Built-in Classes

| Class | Methods |
|---|---|
| `java.lang.Object` | `<init>`, `wait`, `notify`, `notifyAll` |
| `java.lang.System` | `out` (static field) |
| `java.io.PrintStream` | `println`/`print` for void, int, long, float, double, boolean, char, String |
| `java.lang.String` | `length`, `charAt`, `equals`, `hashCode` |
| `java.lang.Integer` | `parseInt`, `valueOf`, `intValue` |
| `java.lang.StringBuilder` | `<init>`, `append` (all types), `toString`, `length` |
| `java.lang.Math` | `max`, `min`, `abs` (int/long/double), `sqrt`, `pow` |
| `java.lang.Thread` / `java.lang.Runnable` | `Thread.<init>`, `Thread.start`, `Thread.run`, `Thread.join` |
| Exception hierarchy | `Throwable` through `ArithmeticException`, `NullPointerException`, `ClassCastException`, etc. |

## Project Structure

```
src/
  main.rs          CLI entry point
  lib.rs           Library root
  bytecode.rs      Opcode enum and bytecode definitions
  classfile.rs     Java .class file parser
  launcher.rs      Class loading and main method invocation
  vm/
    mod.rs         Execution engine (frames, heap, execute loop)
    builtin.rs     Built-in class bootstrap and native methods
tests/
  integration.rs   End-to-end tests (compile Java + execute)
```

## Testing

```sh
# Run all tests (55 total: 47 unit + 8 integration)
cargo test

# Run only integration tests
cargo test --test integration

# Run a specific test
cargo test fibonacci
```

## References

- [JVMS 21](https://docs.oracle.com/javase/specs/jvms/se21/html/index.html) - Java Virtual Machine Specification
- [JVMS 21 Instruction Set](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5) - Complete opcode reference
