# jvm-rs TODO

Updated: 2026-05-05

The goal is not to clone every layer of HotSpot. The goal is to keep jvm-rs small, understandable, fast to start, and memory-conscious while steadily closing the compatibility and performance gaps that matter for real Java programs.

HotSpot is the comparison point: it has a complete JDK surface, mature GC implementations, interpreter plus C1/C2 tiered compilation, JVMTI/JFR diagnostics, precise deoptimization, and decades of production hardening. jvm-rs is currently a lightweight JVM that can execute modern `javac` output and has an experimental Cranelift-backed JIT.

## Current Status

- [x] Classfile parsing: constant pool, fields, methods, `Code`, exception tables, `BootstrapMethods`, `StackMapTable`, and common attributes.
- [x] Interpreter: broad JVMS opcode coverage, including objects, arrays, multidimensional arrays, exceptions, synchronization, lambda `invokedynamic`, and string concat `invokedynamic`.
- [x] Class loading: lazy directory and JAR classpath loading.
- [x] Runtime: heap objects, static fields, method invocation, virtual/interface dispatch, `<clinit>`, monitors, and basic Java thread APIs.
- [x] Verification: structural checks, data-flow type state, and `StackMapTable` consistency checks.
- [x] GC: configurable mark-and-sweep with manual triggering and stats.
- [x] Standard library: practical subset through built-ins, Rust natives, and selected JDK bytecode for `java.lang`, `java.util`, regex/time/text/io/nio/concurrent/reflection.
- [x] JIT: Cranelift backend, method compilation, OSR, helper-backed calls/fields/arrays/type checks/synchronization/exceptions, deopt, and interpreter fallback.
- [x] Tests: `tests/jit.rs` contains 146 `jit_` cases covering many interpreter-vs-JIT differential scenarios.

## HotSpot Gap Summary

| Area | HotSpot | jvm-rs Today | Direction |
|---|---|---|---|
| Standard library | Full JDK modules, JNI, reflection, Unsafe, VarHandle, ServiceLoader | Common classes stitched together from built-ins, natives, and partial JDK bytecode; many natives are simplified or stubbed | Run real workloads first, then expand by observed failures |
| Execution engine | Template interpreter plus C1/C2 tiered compilation, profiling, speculative optimization | Interpreter works; JIT compiles many bytecodes, but project-level inlining/escape analysis is not real yet | Stabilize correctness, then add profiling and optimization |
| Deoptimization | Precise safepoints, OopMaps, debug info, mature stack reconstruction | Snapshots and fallback exist, but compiled exception tables, complex stack reconstruction, and safepoints are incomplete | Make deopt a testable runtime contract |
| GC | Serial/Parallel/G1/ZGC/Shenandoah, generational and concurrent collectors | Single mark-and-sweep collector, global heap lock, non-moving objects | Start with young generation and TLABs |
| Memory layout | Compressed oops, object headers, class pointers, field layout, code cache/metaspace engineering | `Reference::Heap(usize)`, map-heavy object fields, HashMap-heavy metadata | Flatten objects and class metadata |
| Java memory model | Mature volatile, park/unpark, interrupt, monitor, Unsafe/VarHandle semantics | Basic threads and monitors; Unsafe/VarHandle are lite or stubbed | Prioritize volatile/CAS/park correctness |
| Diagnostics | `jcmd`, JFR, `jstack`, `jmap`, structured GC/JIT logs, perf integration | Mostly `-Xtrace` and test counters | Add structured events and logs |

## Compatibility

### P0: Make Real Programs Run

- [x] Build a compatibility sample set: tests cover collections/stream-heavy (`collections_stream_heavy`, `collections_map_reduce`, `collections_nested_lists`), multithreaded (`multithreaded_*`), parsing (`parsing_*`), CLI (`cli_*`).
- [x] Track for each sample: infrastructure exists via `Vm::get_stub_stats()` — callers can now assert zero dangerous stub hits after execution.
- [x] Add counters for unknown native/stub hits so default-return stubs cannot hide compatibility failures.
- [x] Classify `jdk/internal/misc/Unsafe` behavior as real semantics, conservative stub, or dangerous stub.
- [x] Add a runtime fail-fast mode for dangerous native stubs.
- [x] Extend `invokedynamic` bootstrap support beyond lambda and StringConcat based on real workload failures. (Infrastructure added: `BootstrapMethodHandle` variant in `InvokeDynamicKind`, JIT and interpreter paths for custom bootstrap methods. Full resolution of method handle arguments still needed for complete CallSite bootstraps.)
- [x] Add at least one end-to-end real JAR test, not only tests that compile tiny Java sources dynamically.

### P1: JDK Surface Area

- [ ] Create a built-in/native compatibility table: class, method signature, implementation type, semantic completeness, test coverage.
- [ ] Improve `java.lang.Class` and reflection: annotations, modifiers, constructors, primitive/array class metadata, and method invocation error semantics.
- [ ] Improve `java.lang.Thread`: interrupt, daemon flag, name, priority, context class loader, uncaught exception handler.
- [ ] Implement `ServiceLoader`, resource loading, system properties, and environment access behavior needed by common libraries.
- [ ] Move `java.io` and `java.nio.file` from stubs toward real file IO, error handling, and path normalization.
- [ ] Expand `java.util.stream`: map/filter/reduce/collect pipelines instead of only selected native collector/stream shortcuts.

### P3: Specification And Ecosystem

- [ ] Run a feasible OpenJDK jtreg subset and track pass/fail/unsupported.
- [ ] Clarify the Java target: README says JVMS 21, so document classfile version support, JDK API coverage, and module boundaries.
- [ ] Keep classpath applications as the near-term target; defer full JPMS unless real workloads require it.
- [ ] Keep JNI/JVMTI out of scope for now; if real samples need JNI, write a minimal native library loading design first.
- [ ] Maintain explicit non-goals: finalization, full JMX, Attach/JVMTI agent ABI compatibility, desktop modules.

## JIT

### P0: Correctness And Support Matrix

- [ ] Publish a JIT opcode matrix: native-lowered, helper-backed, deopt fallback, or interpreter-only.
- [ ] Audit suspicious opcode lowering mappings and add regression tests, especially int bitwise `iand`/`ior`/`ixor`.
- [ ] Add JIT helper ABI property tests for many args, wide values, floats/doubles, references, void returns, primitive returns, and reference returns.
- [ ] Add JIT cache invalidation tests for site fallback, interpreter-only marking, OSR keys, and normal method keys.
- [ ] Make deopt snapshot invariants explicit and tested: pc, locals, operand stack, reference kinds, and pending exception object.

### P1: Deoptimization And OSR

- [ ] Remove the temporary OSR restriction where `method.max_locals > 5` skips OSR.
- [ ] Generalize OSR locals/stack mapping for arbitrary local counts and mixed primitive/reference values.
- [ ] Support exception tables inside compiled methods so `athrow` can find compiled-frame handlers before falling back.
- [ ] Add safepoint and GC-root visibility for compiled frames.
- [ ] Make deopt metadata robust enough for inlined frames before implementing aggressive inlining.

### P2: Profiling And Optimization

- [ ] Add profiling: invocation counts, backedge counts, receiver type profiles, and branch profiles.
- [ ] Implement small-method inlining, initially for `invokestatic`, final methods, and private methods.
- [ ] Add minimal DCE, constant propagation, and redundant null/bounds check elimination after inlining.
- [ ] Add code cache stats and a reclamation policy so compiled code cannot grow without bound.
- [ ] Add JIT dumps for bytecode, Cranelift IR, machine code size, and deopt sites.

## GC

### P1: Observability First

- [ ] Emit GC pause time, freed bytes, live bytes, total heap bytes, and allocation rate.
- [ ] Add tests that assert GC keeps interpreter and JIT-visible references alive.
- [ ] Verify compiled frames expose roots before enabling more aggressive JIT execution across collections.

### P2: Generational Collector

- [ ] Implement a young generation with bump allocation.
- [ ] Add minor GC and promotion into old generation.
- [ ] Design and implement write barriers for generational and future concurrent collectors.
- [ ] Add TLABs to remove the global heap lock from the common allocation path.
- [ ] Add optional compaction or handle indirection to address long-running fragmentation.

## Memory

### P1: Runtime Layout

- [ ] Replace map-style object field storage with class-layout-based flat slots.
- [ ] Build class metadata arenas to reduce repeated HashMap and String allocation.
- [ ] Add a symbol/string interner shared by class metadata and runtime strings where safe.
- [ ] Document the compressed-reference plan: current `Reference::Heap(usize)` behavior, HotSpot compressed oops differences, and migration path.

### P2: Startup And Footprint

- [ ] Build a class-data-sharing analogue: pre-parsed class metadata blobs loaded with mmap.
- [ ] Lazily parse method `Code` and selected attributes only on first execution or reflection demand.
- [ ] Quicken resolved constant-pool entries to reduce repeated interpreter resolution.
- [ ] Drop or compact unused constant-pool data after resolution where reflection does not need it.

## Concurrency

### P0: Java Memory Model Basics

- [ ] Define memory ordering for `volatile` field loads/stores.
- [ ] Use the same volatile semantics in the interpreter, JIT helpers, Unsafe, and VarHandle.
- [ ] Implement real CAS behavior for Unsafe/VarHandle instead of broad success stubs.
- [ ] Implement `LockSupport.park/unpark`.

### P1: Threads And Monitors

- [ ] Add monitor wait/notify tests for timeout, interrupt, and spurious wakeup tolerance.
- [ ] Replace yield-based monitor waiting with Condvar/parking.
- [ ] Add stress tests for Atomic classes, ConcurrentHashMap, Executor, CompletableFuture, and wait/notify.
- [ ] Improve thread state tracking for diagnostics and uncaught exceptions.

## Performance

### P0: Baselines

- [ ] Create fixed benchmarks: hello, class loading, collections, numeric loop, allocation loop, and multithreaded workload.
- [ ] Run each benchmark on both `java` and `jvm-rs`.
- [ ] Record cold start, warm throughput, RSS, allocation rate, JIT compilation count, and GC count.
- [ ] Add a script or `cargo bench` harness and publish results as CI artifacts.

### P2: HotSpot Comparison Targets

- [ ] Beat HotSpot on cold start for small classpath programs.
- [ ] Keep RSS below HotSpot on matched simple workloads.
- [ ] Reach within 2x of HotSpot steady-state throughput on selected numeric/allocation-heavy loops.
- [ ] Add regression gates for startup time and memory footprint once measurements are stable.

## Tooling

### P1: Developer Switches

- [ ] Add `-Xint`.
- [ ] Add `-Xjit:off`.
- [ ] Add `-Xjit:threshold=...`.
- [ ] Add `-Xverify:all` and `-Xverify:none`.
- [ ] Add a fail-fast option for unsupported native methods and dangerous stubs.

### P2: Diagnostics

- [ ] Add `-Xlog:class+load` structured logging.
- [ ] Add `-Xlog:gc` structured logging.
- [ ] Add `-Xlog:jit` structured logging.
- [ ] Add a thread dump with Java thread id, state, monitor owner/waiters, and stack frames.
- [ ] Add a heap dump, first in a jvm-rs-native format, then evaluate hprof compatibility.
- [ ] Add runtime counters API for tests and benchmark collection.

## Suggested Near-Term Order

1. Build the compatibility sample set and native stub fail-fast mode.
2. Publish the JIT support matrix and add deopt snapshot invariant tests.
3. Flatten object layout and add the benchmark harness.
4. Implement young generation, TLABs, and profile-guided JIT improvements.
