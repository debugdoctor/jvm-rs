pub mod compiler;
pub mod emitter;
pub mod optimizer;
pub mod runtime;

pub use optimizer::constant_fold_bytecode;

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::sync::RwLock;

use crate::vm::Frame;
use crate::vm::types::{ClassMethod, Method, RuntimeClass};
use cranelift::codegen::isa::TargetIsa;
use cranelift_native;
use runtime::DeoptReason;
use runtime::JitContext;

#[derive(Clone)]
pub struct CompiledCode {
    pub code_buffer: Vec<u8>,
    pub frame_size: usize,
    pub stack_slots: Vec<StackSlot>,
    pub deopt_info: DeoptimizationInfo,
}

#[derive(Clone)]
pub struct StackSlot {
    pub size: usize,
    pub offset: i32,
}

#[derive(Clone)]
pub struct DeoptimizationInfo {
    pub guard_checks: Vec<GuardCheck>,
    pub trap_info: Vec<TrapInfo>,
    pub local_kinds: Vec<DeoptLocalKind>,
    pub stack_kinds_by_pc: HashMap<usize, Vec<DeoptLocalKind>>,
    pub max_stack_depth: usize,
}

#[derive(Clone)]
pub enum DeoptLocalKind {
    Int,
    Long,
    Float,
    Double,
    Reference,
    Top,
}

#[derive(Clone)]
pub struct GuardCheck {
    pub pc: usize,
    pub guard_type: GuardType,
}

#[derive(Clone)]
pub enum GuardType {
    NotNull,
    TypeCheck(String),
    BoundsCheck,
    DivideByZero,
}

#[derive(Clone)]
pub struct TrapInfo {
    pub pc: usize,
    pub trap_type: TrapType,
}

#[derive(Clone)]
pub enum TrapType {
    NullPointer,
    ArrayBounds,
    DivideByZero,
    InvalidCast,
    ClassCast,
}

/// Per-method invocation and backedge counters used by the JIT profiling
/// subsystem. These feed the tier-up threshold decision.
#[derive(Debug, Default, Clone)]
pub struct MethodProfile {
    pub invocations: u64,
    pub backedge_count: u64,
}

/// Describes a method that the JIT has identified as a candidate for inlining
/// at its call sites.
///
/// Candidate detection is implemented; bytecode-level inlining into the
/// Cranelift IR is pending (requires recursive compilation + deopt metadata
/// for inlined frames).
// TODO: bytecode-level inlining into Cranelift IR pending.
#[derive(Debug, Clone)]
pub struct InlineCandidate {
    /// Declaring class internal name (e.g. `com/example/Foo`).
    pub class: String,
    /// Simple method name.
    pub method: String,
    /// Method descriptor (e.g. `(I)V`).
    pub descriptor: String,
    /// Raw bytecodes of the callee (≤ 10 bytes).
    pub bytecodes: Vec<u8>,
}

pub struct JitCompiler {
    compiled_code: RwLock<HashMap<String, CompiledCode>>,
    deopt_counts: RwLock<HashMap<String, HashMap<DeoptReason, u64>>>,
    deopt_site_counts: RwLock<HashMap<String, HashMap<usize, HashMap<DeoptReason, u64>>>>,
    interpreter_only: RwLock<HashMap<String, DeoptReason>>,
    invocation_threshold: u32,
    backedge_threshold: u32,
    isa: Arc<dyn TargetIsa>,
    /// Per-method profiles keyed by (class_name, method_name, descriptor).
    profiles: RwLock<HashMap<(String, String, String), MethodProfile>>,
}

impl fmt::Debug for JitCompiler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JitCompiler")
            .field("invocation_threshold", &self.invocation_threshold)
            .field("backedge_threshold", &self.backedge_threshold)
            .finish()
    }
}

impl JitCompiler {
    pub fn new() -> Result<Self, String> {
        let isa = cranelift_native::builder()
            .map_err(|e| e.to_string())?
            .finish(cranelift::codegen::settings::Flags::new(
                cranelift::codegen::settings::builder(),
            ))
            .map_err(|e| format!("failed to build ISA: {}", e))?;

        Ok(Self {
            compiled_code: RwLock::new(HashMap::new()),
            deopt_counts: RwLock::new(HashMap::new()),
            deopt_site_counts: RwLock::new(HashMap::new()),
            interpreter_only: RwLock::new(HashMap::new()),
            invocation_threshold: 1000,
            backedge_threshold: 2000,
            isa,
            profiles: RwLock::new(HashMap::new()),
        })
    }

    pub(super) fn should_compile(&self, frame: &Frame, cp_index: Option<usize>) -> bool {
        if let Some(index) = cp_index {
            let call_count = frame.call_counts.get(&index).copied().unwrap_or(0);
            call_count >= self.invocation_threshold
        } else {
            frame.invocation_count >= self.invocation_threshold
        }
    }

    pub(super) fn should_osr(&self, frame: &Frame, backedge_pc: usize) -> bool {
        frame
            .backedge_counts
            .get(&backedge_pc)
            .copied()
            .unwrap_or(0)
            >= self.backedge_threshold
    }

    pub fn set_thresholds(&mut self, invocation: u32, backedge: u32) {
        self.invocation_threshold = invocation;
        self.backedge_threshold = backedge;
    }

    pub fn invocation_threshold(&self) -> u32 {
        self.invocation_threshold
    }

    /// Record one invocation of the given method. Returns the updated count.
    pub fn record_invocation(&self, class: &str, method: &str, desc: &str) -> u64 {
        let mut map = self.profiles.write().unwrap();
        let profile = map
            .entry((class.to_string(), method.to_string(), desc.to_string()))
            .or_default();
        profile.invocations += 1;
        profile.invocations
    }

    /// Record one backedge (loop iteration) in the given method. Returns the updated count.
    pub fn record_backedge(&self, class: &str, method: &str, desc: &str) -> u64 {
        let mut map = self.profiles.write().unwrap();
        let profile = map
            .entry((class.to_string(), method.to_string(), desc.to_string()))
            .or_default();
        profile.backedge_count += 1;
        profile.backedge_count
    }

    /// Return the current invocation count for a method (0 if not yet profiled).
    pub fn get_invocation_count(&self, class: &str, method: &str, desc: &str) -> u64 {
        self.profiles
            .read()
            .unwrap()
            .get(&(class.to_string(), method.to_string(), desc.to_string()))
            .map(|p| p.invocations)
            .unwrap_or(0)
    }

    /// Return the current backedge count for a method (0 if not yet profiled).
    pub fn get_backedge_count(&self, class: &str, method: &str, desc: &str) -> u64 {
        self.profiles
            .read()
            .unwrap()
            .get(&(class.to_string(), method.to_string(), desc.to_string()))
            .map(|p| p.backedge_count)
            .unwrap_or(0)
    }

    pub fn install_code(&self, method_key: String, code: CompiledCode) {
        self.compiled_code.write().unwrap().insert(method_key, code);
    }

    pub fn record_deopt(&self, method_key: &str, reason: DeoptReason) -> u64 {
        let mut counts = self.deopt_counts.write().unwrap();
        let per_method = counts.entry(method_key.to_string()).or_default();
        let entry = per_method.entry(reason).or_insert(0);
        *entry += 1;
        *entry
    }

    pub fn record_deopt_site(&self, method_key: &str, pc: usize, reason: DeoptReason) -> u64 {
        let mut counts = self.deopt_site_counts.write().unwrap();
        let per_method = counts.entry(method_key.to_string()).or_default();
        let per_site = per_method.entry(pc).or_default();
        let entry = per_site.entry(reason).or_insert(0);
        *entry += 1;
        *entry
    }

    pub fn deopt_count(&self, method_key: &str, reason: DeoptReason) -> u64 {
        self.deopt_counts
            .read()
            .unwrap()
            .get(method_key)
            .and_then(|per_method| per_method.get(&reason).copied())
            .unwrap_or(0)
    }

    pub fn total_deopt_count(&self, method_key: &str) -> u64 {
        self.deopt_counts
            .read()
            .unwrap()
            .get(method_key)
            .map(|per_method| per_method.values().copied().sum())
            .unwrap_or(0)
    }

    pub fn deopt_site_count(&self, method_key: &str, pc: usize, reason: DeoptReason) -> u64 {
        self.deopt_site_counts
            .read()
            .unwrap()
            .get(method_key)
            .and_then(|per_method| per_method.get(&pc))
            .and_then(|per_site| per_site.get(&reason).copied())
            .unwrap_or(0)
    }

    pub fn hottest_deopt_site(&self, method_key: &str) -> Option<(usize, u64)> {
        self.deopt_site_counts
            .read()
            .unwrap()
            .get(method_key)
            .and_then(|per_method| {
                per_method
                    .iter()
                    .map(|(pc, per_site)| (*pc, per_site.values().copied().sum()))
                    .max_by_key(|(_, count)| *count)
            })
    }

    pub fn invalidate_compiled_method(&self, method_key: &str) {
        self.compiled_code.write().unwrap().remove(method_key);
    }

    pub fn should_recompile_with_site_fallback(
        &self,
        method_key: &str,
        pc: usize,
        reason: DeoptReason,
    ) -> bool {
        matches!(reason, DeoptReason::ClassCast | DeoptReason::NullCheck)
            && self.deopt_site_count(method_key, pc, reason) >= 1
    }

    pub fn site_fallbacks_for_method(&self, method_key: &str) -> HashMap<usize, DeoptReason> {
        let mut fallbacks = HashMap::new();
        if let Some(per_method) = self.deopt_site_counts.read().unwrap().get(method_key) {
            for (&pc, per_site) in per_method {
                if per_site.get(&DeoptReason::ClassCast).copied().unwrap_or(0) >= 1 {
                    fallbacks.insert(pc, DeoptReason::ClassCast);
                    continue;
                }
                if per_site.get(&DeoptReason::NullCheck).copied().unwrap_or(0) >= 1 {
                    fallbacks.insert(pc, DeoptReason::NullCheck);
                    continue;
                }
            }
        }
        fallbacks
    }

    pub fn should_abandon_jit_at_site(
        &self,
        method_key: &str,
        pc: usize,
        reason: DeoptReason,
    ) -> bool {
        match reason {
            DeoptReason::HelperUnsupported => true,
            DeoptReason::ClassCast | DeoptReason::MonitorFailure => {
                self.deopt_site_count(method_key, pc, reason) >= 2
            }
            DeoptReason::GuardFailure => false,
            DeoptReason::NullCheck
            | DeoptReason::AllocationFailure
            | DeoptReason::Exception
            | DeoptReason::SiteFallback => false,
        }
    }

    pub fn should_abandon_jit(&self, method_key: &str, reason: DeoptReason) -> bool {
        match reason {
            DeoptReason::HelperUnsupported => true,
            DeoptReason::ClassCast | DeoptReason::MonitorFailure => {
                self.deopt_count(method_key, reason) >= 2
            }
            DeoptReason::GuardFailure => false,
            DeoptReason::NullCheck
            | DeoptReason::AllocationFailure
            | DeoptReason::Exception
            | DeoptReason::SiteFallback => false,
        }
    }

    pub fn mark_interpreter_only(&self, method_key: String, reason: DeoptReason) {
        self.compiled_code.write().unwrap().remove(&method_key);
        self.interpreter_only
            .write()
            .unwrap()
            .insert(method_key, reason);
    }

    pub fn interpreter_only_reason(&self, method_key: &str) -> Option<DeoptReason> {
        self.interpreter_only
            .read()
            .unwrap()
            .get(method_key)
            .copied()
    }

    /// Returns `(compiled_method_count, total_code_bytes, interpreter_only_count)`.
    pub fn code_cache_stats(&self) -> (usize, usize, usize) {
        let cache = self.compiled_code.read().unwrap();
        let count = cache.len();
        let bytes: usize = cache.values().map(|c| c.code_buffer.len()).sum();
        let io_count = self.interpreter_only.read().unwrap().len();
        (count, bytes, io_count)
    }

    pub fn get_compiled_code(&self, method_key: &str) -> Option<CompiledCode> {
        self.compiled_code.read().unwrap().get(method_key).cloned()
    }

    pub fn isa(&self) -> &dyn TargetIsa {
        &*self.isa
    }

    /// Maximum bytecode length for an inline candidate.
    const INLINE_BYTECODE_LIMIT: usize = 10;

    /// Access flags: ACC_STATIC, ACC_PRIVATE, ACC_FINAL.
    const ACC_STATIC: u16 = 0x0008;
    const ACC_PRIVATE: u16 = 0x0002;
    const ACC_FINAL: u16 = 0x0010;

    /// Try to locate `class::method desc` in `classes` and return an
    /// [`InlineCandidate`] if the method is eligible for inlining:
    ///   - has bytecode (not native)
    ///   - is static, private, or final
    ///   - has ≤ 10 bytecodes
    ///   - has no exception handlers
    pub fn find_inline_candidate(
        &self,
        classes: &std::collections::HashMap<String, RuntimeClass>,
        class: &str,
        method: &str,
        desc: &str,
    ) -> Option<InlineCandidate> {
        let rc = classes.get(class)?;
        let cm = rc.methods.get(&(method.to_string(), desc.to_string()))?;
        let m = match cm {
            ClassMethod::Bytecode(m) => m,
            ClassMethod::Native => return None,
        };
        let is_eligible = (m.access_flags & Self::ACC_STATIC != 0)
            || (m.access_flags & Self::ACC_PRIVATE != 0)
            || (m.access_flags & Self::ACC_FINAL != 0);
        if !is_eligible {
            return None;
        }
        if m.code.len() > Self::INLINE_BYTECODE_LIMIT {
            return None;
        }
        if !m.exception_handlers.is_empty() {
            return None;
        }
        Some(InlineCandidate {
            class: class.to_string(),
            method: method.to_string(),
            descriptor: desc.to_string(),
            bytecodes: m.code.clone(),
        })
    }

    /// Returns `true` when the named method is eligible for inlining.
    ///
    /// Delegates to [`Self::find_inline_candidate`]; see that function for the
    /// eligibility criteria.
    pub fn can_inline(
        &self,
        classes: &std::collections::HashMap<String, RuntimeClass>,
        class: &str,
        method: &str,
        desc: &str,
    ) -> bool {
        self.find_inline_candidate(classes, class, method, desc)
            .is_some()
    }

    pub fn compile(&self, method: &Method) -> Result<CompiledCode, String> {
        let method_key = format!("{}.{}{}", method.class_name, method.name, method.descriptor);
        compiler::compile_bytecode(
            method,
            self.isa(),
            self.site_fallbacks_for_method(&method_key),
        )
        .map_err(|e| format!("JIT compilation failed: {:?}", e))
    }

    pub fn compile_osr(&self, method: &Method, entry_pc: usize) -> Result<CompiledCode, String> {
        let method_key = Self::osr_method_key(method, entry_pc);
        compiler::compile_bytecode_osr(
            method,
            self.isa(),
            entry_pc,
            self.site_fallbacks_for_method(&method_key),
        )
        .map_err(|e| format!("JIT OSR compilation failed: {:?}", e))
    }

    pub fn osr_method_key(method: &Method, entry_pc: usize) -> String {
        format!(
            "{}.{}{}@osr:{}",
            method.class_name, method.name, method.descriptor, entry_pc
        )
    }

    pub fn get_or_compile(&self, method: &Method) -> Option<CompiledCode> {
        let key = format!("{}.{}{}", method.class_name, method.name, method.descriptor);
        if self.interpreter_only_reason(&key).is_some() {
            return None;
        }
        if let Some(code) = self.get_compiled_code(&key) {
            return Some(code);
        }

        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.compile(method)));

        match result {
            Ok(Ok(code)) => {
                self.install_code(key, code.clone());
                Some(code)
            }
            Ok(Err(e)) => {
                println!("JIT compilation error: {}", e);
                None
            }
            Err(_) => {
                println!("JIT compilation panicked for {}", key);
                None
            }
        }
    }

    pub fn get_or_compile_osr(&self, method: &Method, entry_pc: usize) -> Option<CompiledCode> {
        let key = Self::osr_method_key(method, entry_pc);
        if self.interpreter_only_reason(&key).is_some() {
            return None;
        }
        if let Some(code) = self.get_compiled_code(&key) {
            return Some(code);
        }

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.compile_osr(method, entry_pc)
        }));

        match result {
            Ok(Ok(code)) => {
                self.install_code(key, code.clone());
                Some(code)
            }
            Ok(Err(e)) => {
                println!("JIT OSR compilation error: {}", e);
                None
            }
            Err(_) => {
                println!("JIT OSR compilation panicked for {}", key);
                None
            }
        }
    }
}

impl Default for JitCompiler {
    fn default() -> Self {
        Self::new().expect("failed to create JIT compiler")
    }
}

#[derive(Debug)]
pub enum JitError {
    CompilationFailed(String),
    CodeGenerationFailed(String),
    LinkerError(String),
    UnsupportedOperation(String),
}

pub fn initialize_jit() {
    println!("JIT Compiler initialized with Cranelift backend");
}

#[cfg(test)]
mod tests {
    use super::JitCompiler;
    use crate::vm::jit::runtime::DeoptReason;
    use crate::vm::types::{ExceptionHandler, Method, Value};
    use crate::vm::{ExecutionResult, Vm};

    #[test]
    fn compiles_integer_bytecode_into_machine_code() {
        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let method = Method::new(
            [
                0x05, // iconst_2
                0x06, // iconst_3
                0x60, // iadd
                0x08, // iconst_5
                0x68, // imul
                0xac, // ireturn
            ],
            0,
            2,
        )
        .with_metadata("jit/Test", "constMath", "()I", 0);

        let code = compiler.compile(&method).expect("JIT compilation failed");
        assert!(
            !code.code_buffer.is_empty(),
            "JIT compilation should produce machine code"
        );
    }

    #[test]
    fn interpreter_only_methods_skip_recompilation() {
        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let method =
            Method::new([0x04, 0xac], 0, 1).with_metadata("jit/Test", "blacklisted", "()I", 0x0008);
        let key = format!("{}.{}{}", method.class_name, method.name, method.descriptor);

        compiler.mark_interpreter_only(key.clone(), DeoptReason::HelperUnsupported);

        assert_eq!(
            compiler.interpreter_only_reason(&key),
            Some(DeoptReason::HelperUnsupported)
        );
        assert!(
            compiler.get_or_compile(&method).is_none(),
            "interpreter-only methods should not re-enter JIT compilation"
        );
    }

    #[test]
    fn deopt_counts_drive_abandon_policy() {
        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let key = "jit/Test.guardy()I";

        assert_eq!(
            compiler.record_deopt(key, DeoptReason::HelperUnsupported),
            1
        );
        assert!(compiler.should_abandon_jit(key, DeoptReason::HelperUnsupported));

        assert_eq!(compiler.record_deopt(key, DeoptReason::NullCheck), 1);
        assert!(!compiler.should_abandon_jit(key, DeoptReason::NullCheck));
        assert_eq!(compiler.total_deopt_count(key), 2);
    }

    #[test]
    fn deopt_site_counts_track_hottest_pc() {
        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let key = "jit/Test.castCheck()I";

        assert_eq!(
            compiler.record_deopt_site(key, 7, DeoptReason::ClassCast),
            1
        );
        assert_eq!(
            compiler.record_deopt_site(key, 7, DeoptReason::ClassCast),
            2
        );
        assert_eq!(
            compiler.record_deopt_site(key, 12, DeoptReason::NullCheck),
            1
        );

        assert_eq!(compiler.deopt_site_count(key, 7, DeoptReason::ClassCast), 2);
        assert_eq!(
            compiler.deopt_site_count(key, 12, DeoptReason::NullCheck),
            1
        );
        assert_eq!(compiler.hottest_deopt_site(key), Some((7, 2)));
    }

    #[test]
    fn site_fallbacks_only_include_specific_reason_classes() {
        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let key = "jit/Test.siteFallbacks()I";

        compiler.record_deopt(key, DeoptReason::ClassCast);
        compiler.record_deopt_site(key, 10, DeoptReason::ClassCast);
        compiler.record_deopt(key, DeoptReason::NullCheck);
        compiler.record_deopt_site(key, 20, DeoptReason::NullCheck);
        compiler.record_deopt(key, DeoptReason::GuardFailure);
        compiler.record_deopt_site(key, 30, DeoptReason::GuardFailure);

        let fallbacks = compiler.site_fallbacks_for_method(key);
        assert_eq!(fallbacks.get(&10), Some(&DeoptReason::ClassCast));
        assert_eq!(fallbacks.get(&20), Some(&DeoptReason::NullCheck));
        assert_eq!(fallbacks.get(&30), None);
    }

    #[test]
    fn guardfailure_is_legacy_only_and_does_not_drive_new_policy() {
        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let key = "jit/Test.legacyGuard()I";

        compiler.record_deopt(key, DeoptReason::GuardFailure);
        compiler.record_deopt_site(key, 10, DeoptReason::GuardFailure);
        compiler.record_deopt(key, DeoptReason::GuardFailure);
        compiler.record_deopt_site(key, 10, DeoptReason::GuardFailure);
        compiler.record_deopt(key, DeoptReason::GuardFailure);
        compiler.record_deopt_site(key, 10, DeoptReason::GuardFailure);

        assert!(!compiler.should_recompile_with_site_fallback(key, 10, DeoptReason::GuardFailure));
        assert!(!compiler.should_abandon_jit_at_site(key, 10, DeoptReason::GuardFailure));
        assert!(!compiler.should_abandon_jit(key, DeoptReason::GuardFailure));
    }

    #[test]
    fn compiles_bytecode_with_arguments() {
        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let method = Method::new(
            [
                0x1a, // iload_0
                0x1b, // iload_1
                0x60, // iadd
                0x10, 0x07, // bipush 7
                0x68, // imul
                0xac, // ireturn
            ],
            2,
            3,
        )
        .with_metadata("jit/Test", "argMath", "(II)I", 0);

        let code = compiler.compile(&method).expect("JIT compilation failed");
        assert!(
            !code.code_buffer.is_empty(),
            "JIT compilation with arguments should produce machine code"
        );
    }

    #[test]
    fn get_or_compile_caches_real_compiled_code() {
        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let method = Method::new(
            [
                0x1a, // iload_0
                0x10, 0x07, // bipush 7
                0x68, // imul
                0xac, // ireturn
            ],
            1,
            2,
        )
        .with_metadata("jit/Test", "cached", "(I)I", 0);
        let method_key = "jit/Test.cached(I)I";

        let first = compiler
            .get_or_compile(&method)
            .expect("expected first compilation to succeed");
        let cached = compiler
            .get_compiled_code(method_key)
            .expect("compiled code should be cached");
        let second = compiler
            .get_or_compile(&method)
            .expect("expected cached compilation to succeed");

        assert!(!first.code_buffer.is_empty());
        assert_eq!(first.code_buffer, cached.code_buffer);
        assert_eq!(cached.code_buffer, second.code_buffer);
    }

    #[test]
    fn executes_compiled_integer_bytecode_end_to_end() {
        use super::runtime::JitContext;

        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let mut context = JitContext::new();
        let method = Method::new(
            [
                0x05, // iconst_2
                0x06, // iconst_3
                0x60, // iadd
                0x08, // iconst_5
                0x68, // imul
                0xac, // ireturn
            ],
            0,
            2,
        )
        .with_metadata("jit/Test", "constMath", "()I", 0);

        let code = compiler.compile(&method).expect("JIT compilation failed");
        assert!(
            context.add_method("jit/Test.constMath()I".to_string(), code),
            "failed to install compiled code"
        );

        let result = context
            .execute(0, "jit/Test.constMath()I", &[])
            .expect("missing JIT entry");
        assert_eq!(result, Value::Int(25));
    }

    #[test]
    fn executes_compiled_goto_w_end_to_end() {
        use super::runtime::JitContext;

        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let mut context = JitContext::new();
        let method = Method::new(
            [
                0xc8, 0x00, 0x00, 0x00, 0x08, // goto_w +8
                0x05, // iconst_2
                0xac, // ireturn
                0x00, // nop
                0x06, // iconst_3
                0xac, // ireturn
            ],
            0,
            1,
        )
        .with_metadata("jit/Test", "gotoWide", "()I", 0);

        let code = compiler.compile(&method).expect("JIT compilation failed");
        assert!(
            context.add_method("jit/Test.gotoWide()I".to_string(), code),
            "failed to install compiled code"
        );

        let result = context
            .execute(0, "jit/Test.gotoWide()I", &[])
            .expect("missing JIT entry");
        assert_eq!(result, Value::Int(3));
    }

    #[test]
    fn rejects_jsr_subroutines_for_now() {
        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let method = Method::new(
            [
                0x08, // iconst_5
                0x3b, // istore_0
                0xa8, 0x00, 0x05, // jsr +5
                0x1a, // iload_0
                0xac, // ireturn
                0x4c, // astore_1
                0x84, 0x00, 0x01, // iinc 0, 1
                0xa9, 0x01, // ret 1
            ],
            2,
            2,
        )
        .with_metadata("jit/Test", "legacySubroutine", "()I", 0);

        let err = match compiler.compile(&method) {
            Ok(_) => panic!("jsr/ret bytecode should stay on the interpreter for now"),
            Err(err) => err,
        };
        assert!(
            err.contains("return-address SSA"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn executes_compiled_wide_local_access_end_to_end() {
        use super::runtime::JitContext;

        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let mut context = JitContext::new();
        let method = Method::new(
            [
                0x10, 0x07, // bipush 7
                0xc4, 0x36, 0x01, 0x04, // wide istore 260
                0xc4, 0x84, 0x01, 0x04, 0x00, 0x05, // wide iinc 260 by 5
                0xc4, 0x15, 0x01, 0x04, // wide iload 260
                0xac, // ireturn
            ],
            0,
            1,
        )
        .with_metadata("jit/Test", "wideLocals", "()I", 0);
        let method = Method {
            max_locals: 261,
            ..method
        };

        let code = compiler.compile(&method).expect("JIT compilation failed");
        assert!(
            context.add_method("jit/Test.wideLocals()I".to_string(), code),
            "failed to install compiled code"
        );

        let result = context
            .execute(0, "jit/Test.wideLocals()I", &[])
            .expect("missing JIT entry");
        assert_eq!(result, Value::Int(12));
    }

    #[test]
    fn rejects_wide_ret_for_now() {
        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let method = Method::new(
            [
                0xc4, 0xa9, 0x01, 0x04, // wide ret 260
            ],
            261,
            0,
        )
        .with_metadata("jit/Test", "wideRet", "()V", 0);

        let err = match compiler.compile(&method) {
            Ok(_) => panic!("wide ret bytecode should stay on the interpreter for now"),
            Err(err) => err,
        };
        assert!(
            err.contains("wide ret stays on the interpreter"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn top_level_jit_exception_falls_back_to_interpreter_handler() {
        let method = Method::new(
            [
                0xbb, 0x00, 0x01, // new #1 demo/Thrown
                0xbf, // athrow
                0x57, // pop exception
                0x02, // iconst_m1
                0xac, // ireturn
            ],
            0,
            2,
        )
        .with_metadata("jit/Test", "catchTopLevel", "()I", 0x0008)
        .with_reference_classes([None, Some("demo/Thrown".to_string())])
        .with_exception_handlers([ExceptionHandler {
            start_pc: 0,
            end_pc: 4,
            handler_pc: 4,
            catch_class: Some("demo/Thrown".to_string()),
        }]);

        let mut vm = Vm::new().expect("failed to create VM");
        vm.register_class(crate::vm::RuntimeClass {
            name: "demo/Thrown".to_string(),
            super_class: Some("java/lang/RuntimeException".to_string()),
            methods: std::collections::HashMap::new(),
            static_fields: std::collections::HashMap::new(),
            instance_fields: vec![],
            interfaces: vec![],
            field_offsets: std::collections::HashMap::new(),
        });
        vm.set_jit_thresholds(1, 1);
        let result = vm
            .execute(method)
            .expect("top-level fallback should succeed");

        assert_eq!(result, ExecutionResult::Value(Value::Int(-1)));
        assert!(
            vm.jit_executions() >= 1,
            "expected synthetic handler method to attempt JIT execution"
        );
    }

    #[test]
    fn invalidate_compiled_method_removes_from_cache() {
        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let method = Method::new(
            [
                0x05, // iconst_2
                0x10, 0x03, // bipush 3
                0x60, // iadd
                0xac, // ireturn
            ],
            0,
            2,
        )
        .with_metadata("jit/Cache", "add", "()I", 0);

        let code = compiler.compile(&method).expect("JIT compilation failed");
        let key = "jit/Cache.add()I";
        compiler.invalidate_compiled_method(key);

        assert!(
            compiler.get_compiled_code(key).is_none(),
            "invalidated method should not be in cache"
        );
    }

    #[test]
    fn invalidate_compiled_method_allows_recompilation() {
        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let method = Method::new(
            [
                0x05, // iconst_2
                0x10, 0x03, // bipush 3
                0x60, // iadd
                0xac, // ireturn
            ],
            0,
            2,
        )
        .with_metadata("jit/ReCache", "add", "()I", 0);
        let key = "jit/ReCache.add()I";

        let code1 = compiler.compile(&method).expect("first compilation failed");
        compiler.install_code(key.to_string(), code1.clone());
        assert!(compiler.get_compiled_code(key).is_some());

        compiler.invalidate_compiled_method(key);
        assert!(compiler.get_compiled_code(key).is_none());

        let code2 = compiler.compile(&method).expect("recompilation failed");
        compiler.install_code(key.to_string(), code2.clone());
        assert!(compiler.get_compiled_code(key).is_some());
        assert_eq!(
            compiler.get_compiled_code(key).unwrap().code_buffer,
            code2.code_buffer
        );
    }

    #[test]
    fn osr_key_differs_from_normal_method_key() {
        let method = Method::new(
            [
                0x05, // iconst_2
                0xac, // ireturn
            ],
            0,
            1,
        )
        .with_metadata("jit/OSR", "hotLoop", "(I)I", 0);

        let normal_key = "jit/OSR.hotLoop(I)I";
        let osr_key = JitCompiler::osr_method_key(&method, 0);

        assert_ne!(normal_key, osr_key);
        assert!(osr_key.contains("@osr:0"));
        assert!(osr_key.starts_with(normal_key));
    }

    #[test]
    fn osr_key_and_normal_key_cache_independently() {
        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let method = Method::new(
            [
                0x05, // iconst_2
                0xac, // ireturn
            ],
            0,
            1,
        )
        .with_metadata("jit/IndyOSR", "run", "(I)I", 0);

        let normal_key = "jit/IndyOSR.run(I)I";
        let osr_key = JitCompiler::osr_method_key(&method, 0);

        let code = compiler.compile(&method).expect("compilation failed");
        compiler.install_code(normal_key.to_string(), code.clone());

        assert!(compiler.get_compiled_code(normal_key).is_some());
        assert!(compiler.get_compiled_code(&osr_key).is_none());

        compiler.invalidate_compiled_method(normal_key);

        assert!(compiler.get_compiled_code(normal_key).is_none());
        assert!(compiler.get_compiled_code(&osr_key).is_none());
    }

    #[test]
    fn mark_interpreter_only_prevents_get_or_compile() {
        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let method = Method::new(
            [
                0x05, // iconst_2
                0xac, // ireturn
            ],
            0,
            1,
        )
        .with_metadata("jit/NoJIT", "skipMe", "()I", 0x0008);
        let key = "jit/NoJIT.skipMe()I";

        compiler.mark_interpreter_only(key.to_string(), DeoptReason::HelperUnsupported);

        assert_eq!(
            compiler.interpreter_only_reason(key),
            Some(DeoptReason::HelperUnsupported)
        );
        assert!(
            compiler.get_or_compile(&method).is_none(),
            "interpreter-only method should not be JIT compiled"
        );
    }

    #[test]
    fn mark_interpreter_only_removes_cached_compiled_code() {
        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let method = Method::new(
            [
                0x05, // iconst_2
                0xac, // ireturn
            ],
            0,
            1,
        )
        .with_metadata("jit/CachedNoJIT", "removeMe", "()I", 0);
        let key = "jit/CachedNoJIT.removeMe()I";

        let code = compiler.compile(&method).expect("compilation failed");
        compiler.install_code(key.to_string(), code);

        assert!(compiler.get_compiled_code(key).is_some());

        compiler.mark_interpreter_only(key.to_string(), DeoptReason::HelperUnsupported);

        assert!(compiler.get_compiled_code(key).is_none());
        assert_eq!(
            compiler.interpreter_only_reason(key),
            Some(DeoptReason::HelperUnsupported)
        );
    }

    #[test]
    fn site_fallback_triggers_for_classcast_at_site() {
        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let key = "jit/SiteFallback/target(I)V";

        compiler.record_deopt_site(key, 10, DeoptReason::ClassCast);

        assert!(compiler.should_recompile_with_site_fallback(key, 10, DeoptReason::ClassCast));
        assert!(!compiler.should_recompile_with_site_fallback(key, 20, DeoptReason::ClassCast));
        assert!(!compiler.should_recompile_with_site_fallback(key, 10, DeoptReason::NullCheck));
    }

    #[test]
    fn site_fallback_triggers_for_nullcheck_at_site() {
        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let key = "jit/SiteFallback/nullRef(Ljava/lang/Object;)V";

        compiler.record_deopt_site(key, 5, DeoptReason::NullCheck);

        assert!(compiler.should_recompile_with_site_fallback(key, 5, DeoptReason::NullCheck));
        assert!(!compiler.should_recompile_with_site_fallback(key, 5, DeoptReason::ClassCast));
    }

    #[test]
    fn should_abandon_jit_at_site_for_helper_unsupported() {
        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let key = "jit/SiteFallback/unsupported()V";

        assert!(compiler.should_abandon_jit_at_site(key, 0, DeoptReason::HelperUnsupported));
        assert!(compiler.should_abandon_jit_at_site(key, 99, DeoptReason::HelperUnsupported));
    }

    #[test]
    fn should_abandon_jit_at_site_requires_two_classcast_failures() {
        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let key = "jit/SiteFallback/badCast(I)V";

        compiler.record_deopt_site(key, 10, DeoptReason::ClassCast);
        assert!(!compiler.should_abandon_jit_at_site(key, 10, DeoptReason::ClassCast));

        compiler.record_deopt_site(key, 10, DeoptReason::ClassCast);
        assert!(compiler.should_abandon_jit_at_site(key, 10, DeoptReason::ClassCast));

        assert!(!compiler.should_abandon_jit_at_site(key, 20, DeoptReason::ClassCast));
    }

    #[test]
    fn should_abandon_jit_requires_two_method_level_failures() {
        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let key = "jit/MethodLevel/badCast(I)V";

        assert!(!compiler.should_abandon_jit(key, DeoptReason::ClassCast));
        assert!(!compiler.should_abandon_jit(key, DeoptReason::ClassCast));

        compiler.record_deopt(key, DeoptReason::ClassCast);
        assert!(!compiler.should_abandon_jit(key, DeoptReason::ClassCast));

        compiler.record_deopt(key, DeoptReason::ClassCast);
        assert!(compiler.should_abandon_jit(key, DeoptReason::ClassCast));

        assert!(!compiler.should_abandon_jit(key, DeoptReason::NullCheck));
    }

    #[test]
    fn interpreter_only_reason_is_overwritten_on_second_call() {
        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let key = "jit/OverwriteReason/final()V";

        compiler.mark_interpreter_only(key.to_string(), DeoptReason::HelperUnsupported);
        assert_eq!(
            compiler.interpreter_only_reason(key),
            Some(DeoptReason::HelperUnsupported)
        );

        compiler.mark_interpreter_only(key.to_string(), DeoptReason::NullCheck);
        assert_eq!(
            compiler.interpreter_only_reason(key),
            Some(DeoptReason::NullCheck),
            "second call should overwrite the first reason"
        );
    }

    // ── constant_fold_bytecode tests ──────────────────────────────────────────

    #[test]
    fn constant_fold_no_change_returns_none() {
        use super::constant_fold_bytecode;
        // iconst_0, iadd — only one iconst before iadd, nothing to fold
        let no_fold = &[0x03u8, 0x60];
        assert!(constant_fold_bytecode(no_fold).is_none());
    }

    #[test]
    fn constant_fold_iconst_add() {
        use super::constant_fold_bytecode;
        // iconst_2 (0x05), iconst_3 (0x06), iadd (0x60) => iconst_5 (0x08), nop, nop
        let code = &[0x05u8, 0x06, 0x60];
        let result = constant_fold_bytecode(code).expect("should fold iconst_2 + iconst_3");
        assert_eq!(result[0], 0x08, "result should be iconst_5");
        assert_eq!(result[1], 0x00, "slot 1 should be nop");
        assert_eq!(result[2], 0x00, "slot 2 should be nop");
    }

    #[test]
    fn constant_fold_iconst_sub() {
        use super::constant_fold_bytecode;
        // iconst_5 (0x08), iconst_2 (0x05), isub (0x64) => iconst_3 (0x06), nop, nop
        let code = &[0x08u8, 0x05, 0x64];
        let result = constant_fold_bytecode(code).expect("should fold iconst_5 - iconst_2");
        assert_eq!(result[0], 0x06, "result should be iconst_3");
    }

    #[test]
    fn constant_fold_iconst_mul() {
        use super::constant_fold_bytecode;
        // iconst_1 (0x04), iconst_4 (0x07), imul (0x68) => iconst_4 (0x07), nop, nop
        let code = &[0x04u8, 0x07, 0x68];
        let result = constant_fold_bytecode(code).expect("should fold iconst_1 * iconst_4");
        assert_eq!(result[0], 0x07, "result should be iconst_4");
    }

    #[test]
    fn constant_fold_out_of_range_returns_none() {
        use super::constant_fold_bytecode;
        // iconst_5 (0x08), iconst_5 (0x08), imul (0x68) => 25, not in [-1,5], no fold
        let code = &[0x08u8, 0x08, 0x68];
        assert!(constant_fold_bytecode(code).is_none(), "25 is out of iconst range, should not fold");
    }
}
