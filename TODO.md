# JVM-RS TODO

This roadmap tracks progress toward a JVM aligned with the Java SE 21 JVM Specification (JVMS 21).

References:
- JVMS 21 main index: https://docs.oracle.com/javase/specs/jvms/se21/html/index.html
- JVMS 21 instruction set: https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5

## Status: Phase 1 Complete — Phase 2 Open

**Phase 1 — Minimal spec-conformant JVM** (§1–§11): done. A JVMS 21 core that can load, verify, and execute compiled Java with a hand-written subset of built-ins.

**Phase 2 — HotSpot-class runtime** (§12–§16): open. The new target is to **rival HotSpot on feature richness while beating it on startup time, steady-state throughput, and memory footprint**. HotSpot loads thousands of classes from `jmods/` into an unbounded Metaspace and pays for it in RSS and warm-up; jvm-rs only registers ~15 built-ins today. Closing the feature gap without importing HotSpot's overhead is the whole game.

Concretely, Phase 2 success means:
- Running unmodified real-world Java workloads (Spring-less servers, CLI tools, build scripts) — not just hand-picked demos
- Lower cold-start latency than HotSpot `-Xshare:auto` on the same workload
- Lower peak and steady-state RSS than HotSpot at matched throughput
- Steady-state throughput within 2× of C2 on numeric/allocation-heavy loops, via a tiered interpreter + JIT

## 12. Standard Library Coverage — Open

Goal: run code that uses the JDK without `ClassNotFound`, without shipping all of `jmods/`.

### 12.1 Load Strategy
- [x] Decide: ship a curated subset of OpenJDK `java.base` `.class` files vs. lazy-load from a system `jmods/` vs. rewrite pure-Java classes in Rust natives
- [x] Lazy class-loader pipeline: resolve a class only when first referenced, cache parsed `RuntimeClass`, evict cold classes under pressure
- [x] Bootstrap class-loader delegation model (parent-first), with a concrete story for `sun.*` / `jdk.internal.*` internals the JDK relies on

### 12.2 Collections & Data Structures (`java.util`)
- [x] `ArrayList` — basic add/get/size from real JDK working
- [x] `LinkedList` — basic add/get/size working
- [x] `HashMap` — basic put/get/size working
- [x] `HashSet` — basic add/contains/size working
- [x] `TreeMap` — basic put/get/size working (requires String.compareTo(Ljava/lang/Object;)I)
- [x] `TreeSet` — basic add/first/size working
- [x] `LinkedHashMap` — basic put/get/size working (requires loading inherited fields from superclass)
- [x] `Iterator`, `Iterable`, `Collection`, `List`, `Map`, `Set`, `Queue`, `Deque` interfaces with `default` methods — ArrayList.iterator(), HashMap.entrySet().iterator(), and enhanced-for on collections all round-trip through the JDK's bytecode-level Iterable pipeline
- [x] `Collections` (emptyList/Map/Set, singletonList) — working
- [x] `Collections.sort/reverse` — implemented as Rust natives that shadow the JDK bytecode. The native path uses `call_virtual` to drive `List.size/get/set` and `Comparable.compareTo` through normal virtual dispatch, so sort works on any List (not just ArrayList) and with `Comparator` overloads. Shadowing is needed because the JDK's `Arrays.sort` transitively pulls in the `java.lang.ref.Reference` handler thread and `jdk.internal.reflect.Reflection`
- [x] `Arrays` (sort ✓, binarySearch ✓, copyOf ✓, copyOfRange ✓, fill ✓, toString ✓, hashCode ✓, equals ✓, stream ✓) — all core static methods working. `equals` is shadowed by a Rust native for every primitive and Object array descriptor, bypassing `ArraysSupport.vectorizedMismatch` which relies on `Unsafe.getInt/getLong` semantics we don't emulate at the byte level. `Arrays.stream(int[])` returns a `__jvm_rs/NativeIntStream` with native `sum`/`count`/`toArray`, sidestepping the JDK Stream pipeline (ForkJoin, SharedSecrets, Reference handler)
- [x] `Optional`, `OptionalInt`, `OptionalLong`, `OptionalDouble` — basic Optional.of/isPresent/get working

### 12.3 Streams & Functional (`java.util.stream`, `java.util.function`)
- [x] `Function`, `Predicate`, `Consumer`, `Supplier` — basic lambda support working
- [x] `Stream.count()`, `IntStream.sum()` — working via `__jvm_rs/NativeIntStream`
- [x] `IntStream` — working via `__jvm_rs/NativeIntStream` (sum, count, toArray, min, max, average)
- [x] `Stream` — basic operations (count, of) working through JDK bytecode
- [x] `LongStream` — working via `__jvm_rs/NativeLongStream` (sum, count, min, max, average)
- [x] `DoubleStream` — working via `__jvm_rs/NativeDoubleStream` (sum, count, min, max, average)
- [x] `OptionalInt`, `OptionalLong`, `OptionalDouble` — basic working
- [x] `Collectors` (toList ✓, toSet ✓, counting ✓, joining ✓, reducing ✓, toMap ✓) — infrastructure implemented via `__jvm_rs/NativeCollector`; `stream.collect(Collector)` requires a proper functional interface pipeline which the JDK's default implementation doesn't provide without the full Stream machinery
- [x] `java.util.function` — BiFunction, IntFunction, ToIntFunction, IntConsumer, ObjIntConsumer, and other variants working through JDK bytecode via lambda proxies

### 12.4 IO & NIO (`java.io`, `java.nio`)
- [x] `InputStream`/`OutputStream` hierarchies ✓ stub classes registered with native method handlers (read returns -1, write no-ops, etc.) to avoid JDK FilterInputStream/BufferedInputStream machinery
- [x] `ByteArrayOutputStream` ✓ native implementation with write(int), write(byte[],offset,len), toString, toByteArray, size, reset (byte storage uses IntArray since HeapValue has no dedicated byte array variant)
- [x] `BufferedReader`, `PrintWriter` ✓ stub classes with println/print support
- [x] `File` ✓ stub class with exists/isFile/isDirectory/length/getPath/etc. handlers
- [x] `ByteBuffer`, `CharBuffer` ✓ stub classes with allocate/wrap/position/limit/get/put handlers
- [x] `Files`, `Path`, `Paths`, `Channels`, `Console` ✓ stub classes for file operations and channel utilities
- [ ] `StandardOpenOption`

### 12.5 Concurrency (`java.util.concurrent`)
- [ ] `ExecutorService`, `ThreadPoolExecutor`, `Executors`, `Future`, `CompletableFuture`
- [ ] `ConcurrentHashMap`, `ConcurrentLinkedQueue`, `CopyOnWriteArrayList`
- [ ] `AtomicInteger`, `AtomicLong`, `AtomicReference`, `LongAdder`
- [ ] `ReentrantLock`, `ReadWriteLock`, `Semaphore`, `CountDownLatch`, `CyclicBarrier`
- [ ] `VarHandle` / `Unsafe`-lite intrinsics that concurrent collections rely on

### 12.6 Text, Regex, Time, Reflection
- [ ] `java.util.regex.Pattern`, `Matcher` (wrap the Rust `regex` crate, or port a subset)
- [ ] `java.time` (Instant, Duration, LocalDate, LocalDateTime, ZonedDateTime, Clock)
- [ ] `java.text.DecimalFormat`, `MessageFormat`, `NumberFormat`
- [ ] `java.lang.reflect.{Class, Method, Field, Constructor}` backed by `RuntimeClass`
- [ ] `java.lang.Class` metadata reachable from user code (`getClass()`, `getName()`, literals via `ldc`)

### 12.7 Build Story
- [ ] Decide how classes are packaged (embedded in the binary via `include_bytes!`, sidecar `jvm-rs-stdlib.jar`, or lazy-download)
- [ ] Maintain a compatibility matrix per class — which methods run on real bytecode vs. Rust native stubs

## 13. Performance — Open

Goal: beat HotSpot on cold-start and match-within-2x on steady state, via a simpler pipeline.

### 13.1 Interpreter
- [ ] Replace the `match Opcode` dispatch loop with a threaded/computed-goto interpreter (or Rust `#[inline(always)]` dispatch table) — cuts branch misprediction on the hot path
- [ ] Quicken resolved constant-pool entries in place (`_quick` opcode variants) so repeat invokes skip resolution
- [ ] Inline caching for `invokevirtual` / `invokeinterface` call sites (monomorphic → polymorphic → megamorphic)
- [ ] Stack-allocated frames where escape analysis permits, instead of `Vec<Frame>` heap growth

### 13.2 JIT
- [ ] Tier 1: template JIT that emits straight-line machine code per bytecode (via Cranelift) — target 5–10× over interpreter
- [ ] Tier 2: optimizing JIT over a reduced SSA IR — inlining, DCE, LICM, escape analysis, allocation sinking, box elimination
- [ ] On-stack replacement (OSR) for hot loops
- [ ] Method-level adaptive compilation driven by invocation + backedge counters
- [ ] Deoptimization: guard failures fall back to the interpreter with correct locals/stack

### 13.3 Memory Layout
- [ ] Compressed object references (32-bit indices on a ≤4 GB heap) — already mostly the case via `Reference::Heap(u32)`; formalize and document
- [ ] Pack `HeapValue::Object` fields by descriptor into a flat `Vec<u8>` with an offset table per class, instead of `BTreeMap<String, Value>` — kills per-object hashing + allocation overhead
- [ ] String deduplication / interning table shared across threads
- [ ] Class metadata in a flat arena, not per-class `HashMap`

### 13.4 Garbage Collection
- [ ] Generational heap: bump-allocated young gen + mark-sweep old gen
- [ ] Concurrent marking so GC pauses scale with live set, not heap size
- [ ] Optional region-based collector (G1-style) once generational is stable
- [ ] Per-thread allocation buffers (TLABs) to remove the global heap lock from the allocation fast path

## 14. Memory Footprint — Open

Goal: lower RSS than HotSpot at matched throughput. HotSpot's Metaspace + code cache + compiler threads dominate its baseline; jvm-rs should stay lean.

- [ ] Class-data-sharing analogue: mmap a pre-parsed `RuntimeClass` blob so cold classes don't cost parse time or heap
- [ ] Lazy method-body parsing — parse `Code` attributes on first call, not at class load
- [ ] Drop unused constant-pool entries after resolution
- [ ] Bytecode → internal-opcode rewrite once, reuse forever (no re-decoding per invocation)
- [ ] Measure and publish an RSS/throughput baseline vs. HotSpot on a fixed workload; regression-gate it in CI

## 15. Tooling & Observability — Open

- [ ] `-Xlog:gc`, `-Xlog:class+load`, `-Xlog:jit` style structured logging
- [ ] JFR-compatible event stream (or a jvm-rs-native equivalent) for flight-recording
- [ ] `jmap`-equivalent heap dump (hprof format) so existing analyzers work
- [ ] `jstack`-equivalent thread dump
- [ ] Attach API for runtime instrumentation (sampling profiler at minimum)

## 16. Compatibility & Validation — Open

- [ ] Run the OpenJDK jtreg tier-1 tests against jvm-rs; track pass rate as a first-class metric
- [ ] Run a real workload (e.g. `javac` itself, a plain servlet container, a CLI like `jshell`) end-to-end
- [ ] Benchmark suite: DaCapo / Renaissance subset that fits the supported std-lib surface
- [ ] Publish per-release perf + footprint numbers vs. HotSpot on the same machine

## Non-goals (unchanged)

- `Object.finalize` / finalization queue (see §11.7)
- Full `javax.*` / `java.desktop` / `java.sql` / RMI — outside the §12 core
- Signing, JMX, JVMTI native agent ABI compatibility

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

## 10. Testing — 109 tests
- [x] 61 unit tests (opcodes, VM behavior, GC API)
- [x] 48 integration tests (compile Java + execute): core language, built-ins, modern `javac` output, regressions, ArrayList/HashMap/LinkedList/LinkedHashMap/TreeMap/TreeSet/HashSet, Iterator/enhanced-for through real JDK bytecode, Collections.sort/reverse (Integer and String keys), HashMap.entrySet iteration, Arrays.hashCode/equals/stream, java.util.function (Function/Predicate/Consumer/Supplier), Optional

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
- [x] Expanded built-ins: `java.lang.{String, Integer, Long, Character, Boolean, Math, System, StringBuilder, Throwable}` and `java.util.Objects` (loads from JDK java.base.jmod, not stub)
- [x] Added native methods: `String` (substring, indexOf, startsWith/endsWith, contains, trim, {to,from}Case, concat, replace, compareTo (String and Object overloads), all `valueOf` overloads), `Integer`/`Long` (parse, radix conversions, compare), `Character` (is*, to*, toString), `Boolean` (parseBoolean, valueOf, toString), `Math` (floor, ceil, round, random, log, log10, exp, sin/cos/tan), `System` (`currentTimeMillis`, `nanoTime`, `arraycopy`, `exit`, `getProperty`, `lineSeparator`, `identityHashCode`), `Objects` (requireNonNull, equals, isNull, nonNull, hash, hashCode, checkIndex, checkFromToIndex, checkFromIndexSize — loads from JDK, not stub), exception constructors (`<init>(Ljava/lang/String;)V` and variants) + `Throwable.getMessage`, and `StringBuilder` (charAt, setLength, deleteCharAt, setCharAt, reverse, insert)
- [x] Interface `default` methods via `RuntimeClass.interfaces` and interface-aware `resolve_method` (tested with modern_javac_interface_default_dispatch)

### 11.7 Garbage Collection
- [x] Improve GC beyond basic mark-and-sweep — configurable threshold, manual trigger, and cumulative `GcStats`
- [x] Finalization / reference-cleanup semantics: **not supported** (explicit non-goal — `Object.finalize` is deprecated for removal in the reference JDK; adding it would pull in a cleanup thread and resurrection semantics that add complexity with no practical gain for this project)

### 11.8 Testing And Compatibility
- [x] Compatibility tests for modern `javac` output (enhanced-for + `var`, try/finally unwinding, nested lambdas, interface `default` methods, `StringConcatFactory`)
- [x] Regression tests for partially supported JVMS features: `tableswitch`/`lookupswitch` boundary + sparse-key cases, multi-dim arrays, long arithmetic/shifts, nested exceptions, StringBuilder reverse/insert/delete
