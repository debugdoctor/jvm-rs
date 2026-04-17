# JVM-RS TODO

This roadmap tracks progress toward a JVM aligned with the Java SE 21 JVM Specification (JVMS 21).

References:
- JVMS 21 main index: https://docs.oracle.com/javase/specs/jvms/se21/html/index.html
- JVMS 21 instruction set: https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5

## Status: Feature Complete

All actionable items on this roadmap have been implemented. The remaining unchecked boxes are explicit non-goals (e.g. finalization) that we have decided not to pursue — see §11.7 for the rationale.

## 1. JVMS 21 Foundations — Complete

### 1.1 Class File Format
- [x] Parse `ClassFile`, constant pool, fields, methods, attributes
- [x] Parse `Code`, `ExceptionTable`, `LineNumberTable`, `SourceFile`, `BootstrapMethods`
- [x] Parse `StackMapTable`
- [x] Remaining standard attributes stored as `RawAttribute`

### 1.2 Run-Time Data Areas
- [x] Frames with locals, operand stack, `pc`; real JVM thread stack
- [x] Heap (objects, arrays, strings, StringBuilder)
- [x] Run-time constant pool per method; method area via `RuntimeClass` registry

### 1.3 Loading, Linking, And Initialization
- [x] On-demand class loading from multiple classpath entries
- [x] `<clinit>` static initializers, `super_class` hierarchy resolution
- [x] Linking resolution at execution time; bytecode verification (`vm/verify.rs`)

### 1.4 Built-In Classes
- [x] `Object`, `System`, `PrintStream`, `String`, `Integer`, `StringBuilder`, `Math`
- [x] Exception hierarchy: `Throwable` → `RuntimeException` → `ArithmeticException`, `NullPointerException`, `ClassCastException`, `ArrayIndexOutOfBoundsException`, `NegativeArraySizeException`, `IllegalMonitorStateException`

## 2. Instruction Set — Complete

### Implemented (199 opcodes)
- **Constants**: `aconst_null`, `iconst_m1`..`iconst_5`, `lconst_0/1`, `fconst_0/1/2`, `dconst_0/1`, `bipush`, `sipush`, `ldc`, `ldc_w`, `ldc2_w`
- **Loads**: all `iload`/`lload`/`fload`/`dload`/`aload` + `_0`..`_3` shortforms + all array loads
- **Stores**: all `istore`/`lstore`/`fstore`/`dstore`/`astore` + `_0`..`_3` shortforms + all array stores
- **Stack**: `pop`, `pop2`, `dup`, `dup_x1`, `dup_x2`, `dup2`, `dup2_x1`, `dup2_x2`, `swap`
- **Math**: all int/long/float/double arithmetic, shifts, bitwise, `iinc`
- **Conversions**: all 15 type conversions
- **Comparisons**: all int/reference branches, `lcmp`, `fcmpl/g`, `dcmpl/g`, `instanceof`
- **Control**: `goto`, `goto_w`, `jsr`, `jsr_w`, `ret`, `tableswitch`, `lookupswitch`, all typed returns
- **References**: `getstatic`, `putstatic`, `getfield`, `putfield`, `invokevirtual`, `invokespecial`, `invokestatic`, `invokeinterface`, `invokedynamic`, `new`, `newarray` (all types), `anewarray`, `multianewarray`, `arraylength`, `athrow`, `checkcast`, `instanceof`, `monitorenter`, `monitorexit`, `ifnull`, `ifnonnull`
- **Extended**: `wide`

## 3. Method Invocation — Complete
- [x] Call stack, argument passing, return values
- [x] Virtual dispatch with super-class resolution, interface dispatch
- [x] `<init>`, `<clinit>`, native method dispatch
- [x] `invokedynamic` with LambdaMetafactory lambda proxy support

## 4. Objects, Arrays, And Types — Complete
- [x] All primitive types (int, long, float, double), all array types, multi-dimensional
- [x] Heap objects with fields, strings, StringBuilder
- [x] Default field values by descriptor type

## 5. Exceptions — Complete
- [x] Exception tables, `athrow`, try-catch-finally, call-stack unwinding
- [x] VM errors → Java exceptions: NPE, AIOOBE, ArithmeticException, ClassCast, NegativeArraySize, IllegalMonitorState

## 6. Memory Management — Implemented
- [x] Mark-and-sweep garbage collection (default threshold: 1024 allocations)
- [x] Slot reuse for freed heap objects, trailing compaction
- [x] Configurable threshold (`Vm::set_gc_threshold`), `Vm::disable_gc`, manual `Vm::request_gc`, and `Vm::gc_stats` counters

## 7. Bytecode Verification — Implemented
- [x] Structural verification: valid opcodes, instruction boundaries, branch targets (`vm/verify.rs`)
- [x] Data-flow verification: locals / operand stack type-state propagation
- [x] `StackMapTable` parsing and consistency checks
- [x] Runtime checks: stack underflow/overflow, local bounds, null references

## 8. Multi-Threading — Implemented
- [x] `Vm::spawn()` creates a child thread with cloned VM state
- [x] `JvmThread::join()` waits for completion
- [x] Per-thread IDs, reentrant monitors with owner tracking
- [x] `monitorenter` blocks (yield-based) when lock is held by another thread

## 9. Launcher — Complete
- [x] `jvm-rs [-cp path:path] [-Xtrace] MainClass [args...]`
- [x] Multiple classpath entries, on-demand class loading
- [x] Execution tracing (`-Xtrace`), improved error diagnostics

## 10. Testing — 84 tests
- [x] 57 unit tests (opcodes, VM behavior, GC API)
- [x] 27 integration tests (compile Java + execute): core language, built-ins (String/Integer/Long/Character/Boolean/Math/System/Objects), modern `javac` output (`var`, try/finally unwinding, nested lambdas, interface `default` methods, `StringConcatFactory`), and regressions for `tableswitch`/`lookupswitch`, multi-dim arrays, long arithmetic, nested exceptions, StringBuilder edits

## 11. Remaining TODOs

### 11.1 Spec Coverage
- [x] Reconcile docs so `README.md` and `TODO.md` describe `invokedynamic` support consistently

### 11.2 Verification
- [x] Implement a fuller JVMS 4.10 verifier with type-state / data-flow checking
- [x] Parse and validate `StackMapTable` instead of relying on structural verification only

### 11.3 Class Loading And Linking
- [x] Expand class loading beyond flat classpath file lookup with directory + JAR classpath support
- [x] Add broader support for standard class-file attributes that were previously preserved as `RawAttribute`
- [x] Support loading classes from JARs instead of only loose `.class` files

### 11.4 Runtime And Concurrency
- [x] Replace cloned-state threading with shared-heap threading semantics
- [x] Align monitor behavior with a more complete Java memory / synchronization model
- [x] Support Java-level thread APIs on top of the VM threading model

### 11.5 `invokedynamic` And Bootstrap Methods
- [x] Extend `invokedynamic` beyond lambda proxies to more bootstrap method patterns
- [x] Support common modern JDK bootstrap use cases such as `StringConcatFactory`

### 11.6 Built-In Classes And Native Methods
- [x] Expanded built-ins: `java.lang.{String, Integer, Long, Character, Boolean, Math, System, StringBuilder, Throwable}` and `java.util.Objects`
- [x] Added native methods: `String` (substring, indexOf, startsWith/endsWith, contains, trim, {to,from}Case, concat, replace, compareTo, all `valueOf` overloads), `Integer`/`Long` (parse, radix conversions, compare), `Character` (is*, to*, toString), `Boolean` (parseBoolean, valueOf, toString), `Math` (floor, ceil, round, random, log, log10, exp, sin/cos/tan), `System` (`currentTimeMillis`, `nanoTime`, `arraycopy`, `exit`, `getProperty`, `lineSeparator`, `identityHashCode`), `Objects` (requireNonNull, equals, isNull, nonNull), exception constructors (`<init>(Ljava/lang/String;)V` and variants) + `Throwable.getMessage`, and `StringBuilder` (charAt, setLength, deleteCharAt, setCharAt, reverse, insert)
- [x] Interface `default` methods via `RuntimeClass.interfaces` and interface-aware `resolve_method`

### 11.7 Garbage Collection
- [x] Improve GC beyond basic mark-and-sweep — configurable threshold, manual trigger, and cumulative `GcStats`
- [x] Finalization / reference-cleanup semantics: **not supported** (explicit non-goal — `Object.finalize` is deprecated for removal in the reference JDK; adding it would pull in a cleanup thread and resurrection semantics that add complexity with no practical gain for this project)

### 11.8 Testing And Compatibility
- [x] Compatibility tests for modern `javac` output (enhanced-for + `var`, try/finally unwinding, nested lambdas, interface `default` methods, `StringConcatFactory`)
- [x] Regression tests for partially supported JVMS features: `tableswitch`/`lookupswitch` boundary + sparse-key cases, multi-dim arrays, long arithmetic/shifts, nested exceptions, StringBuilder reverse/insert/delete
