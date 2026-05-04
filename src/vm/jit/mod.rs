pub mod compiler;
pub mod emitter;
pub mod optimizer;
pub mod runtime;

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::sync::RwLock;

use crate::vm::Frame;
use crate::vm::types::Method;
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

pub struct JitCompiler {
    compiled_code: RwLock<HashMap<String, CompiledCode>>,
    deopt_counts: RwLock<HashMap<String, HashMap<DeoptReason, u64>>>,
    deopt_site_counts: RwLock<HashMap<String, HashMap<usize, HashMap<DeoptReason, u64>>>>,
    interpreter_only: RwLock<HashMap<String, DeoptReason>>,
    invocation_threshold: u32,
    backedge_threshold: u32,
    isa: Arc<dyn TargetIsa>,
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
        })
    }

    pub fn should_compile(&self, frame: &Frame, cp_index: Option<usize>) -> bool {
        if let Some(index) = cp_index {
            let call_count = frame.call_counts.get(&index).copied().unwrap_or(0);
            call_count >= self.invocation_threshold
        } else {
            frame.invocation_count >= self.invocation_threshold
        }
    }

    pub fn set_thresholds(&mut self, invocation: u32, backedge: u32) {
        self.invocation_threshold = invocation;
        self.backedge_threshold = backedge;
    }

    pub fn invocation_threshold(&self) -> u32 {
        self.invocation_threshold
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
        matches!(reason, DeoptReason::ClassCast) && self.deopt_site_count(method_key, pc, reason) >= 1
    }

    pub fn site_fallbacks_for_method(&self, method_key: &str) -> HashMap<usize, DeoptReason> {
        let mut fallbacks = HashMap::new();
        if let Some(per_method) = self.deopt_site_counts.read().unwrap().get(method_key) {
            for (&pc, per_site) in per_method {
                if per_site
                    .get(&DeoptReason::ClassCast)
                    .copied()
                    .unwrap_or(0)
                    >= 1
                {
                    fallbacks.insert(pc, DeoptReason::ClassCast);
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
            DeoptReason::GuardFailure => self.deopt_site_count(method_key, pc, reason) >= 3,
            DeoptReason::ClassCast | DeoptReason::MonitorFailure => {
                self.deopt_site_count(method_key, pc, reason) >= 2
            }
            DeoptReason::NullCheck
            | DeoptReason::AllocationFailure
            | DeoptReason::Exception
            | DeoptReason::SiteFallback => false,
        }
    }

    pub fn should_abandon_jit(&self, method_key: &str, reason: DeoptReason) -> bool {
        match reason {
            DeoptReason::HelperUnsupported => true,
            DeoptReason::GuardFailure => self.deopt_count(method_key, reason) >= 3,
            DeoptReason::ClassCast | DeoptReason::MonitorFailure => {
                self.deopt_count(method_key, reason) >= 2
            }
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
        self.interpreter_only.read().unwrap().get(method_key).copied()
    }

    pub fn get_compiled_code(&self, method_key: &str) -> Option<CompiledCode> {
        self.compiled_code.read().unwrap().get(method_key).cloned()
    }

    pub fn isa(&self) -> &dyn TargetIsa {
        &*self.isa
    }

    pub fn compile(&self, method: &Method) -> Result<CompiledCode, String> {
        let method_key = format!("{}.{}{}", method.class_name, method.name, method.descriptor);
        compiler::compile_bytecode(method, self.isa(), self.site_fallbacks_for_method(&method_key))
            .map_err(|e| format!("JIT compilation failed: {:?}", e))
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
        let method = Method::new([0x04, 0xac], 0, 1)
            .with_metadata("jit/Test", "blacklisted", "()I", 0x0008);
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

        assert_eq!(compiler.record_deopt(key, DeoptReason::GuardFailure), 1);
        assert!(!compiler.should_abandon_jit(key, DeoptReason::GuardFailure));
        assert_eq!(compiler.record_deopt(key, DeoptReason::GuardFailure), 2);
        assert!(!compiler.should_abandon_jit(key, DeoptReason::GuardFailure));
        assert_eq!(compiler.record_deopt(key, DeoptReason::GuardFailure), 3);
        assert!(compiler.should_abandon_jit(key, DeoptReason::GuardFailure));

        assert_eq!(compiler.record_deopt(key, DeoptReason::NullCheck), 1);
        assert!(!compiler.should_abandon_jit(key, DeoptReason::NullCheck));
        assert_eq!(compiler.total_deopt_count(key), 4);
    }

    #[test]
    fn deopt_site_counts_track_hottest_pc() {
        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let key = "jit/Test.castCheck()I";

        assert_eq!(compiler.record_deopt_site(key, 7, DeoptReason::ClassCast), 1);
        assert_eq!(compiler.record_deopt_site(key, 7, DeoptReason::ClassCast), 2);
        assert_eq!(compiler.record_deopt_site(key, 12, DeoptReason::NullCheck), 1);

        assert_eq!(compiler.deopt_site_count(key, 7, DeoptReason::ClassCast), 2);
        assert_eq!(compiler.deopt_site_count(key, 12, DeoptReason::NullCheck), 1);
        assert_eq!(compiler.hottest_deopt_site(key), Some((7, 2)));
    }

    #[test]
    fn abandon_policy_prefers_repeated_same_site_failures() {
        let compiler = JitCompiler::new().expect("failed to create JIT compiler");
        let key = "jit/Test.guards()I";

        compiler.record_deopt(key, DeoptReason::GuardFailure);
        compiler.record_deopt_site(key, 10, DeoptReason::GuardFailure);
        assert!(!compiler.should_abandon_jit_at_site(key, 10, DeoptReason::GuardFailure));

        compiler.record_deopt(key, DeoptReason::GuardFailure);
        compiler.record_deopt_site(key, 20, DeoptReason::GuardFailure);
        assert!(!compiler.should_abandon_jit_at_site(key, 20, DeoptReason::GuardFailure));
        assert!(
            !compiler.should_abandon_jit(key, DeoptReason::GuardFailure),
            "method-wide total should no longer be the preferred trigger when failures are spread across sites"
        );

        compiler.record_deopt(key, DeoptReason::GuardFailure);
        compiler.record_deopt_site(key, 10, DeoptReason::GuardFailure);
        assert!(!compiler.should_abandon_jit_at_site(key, 10, DeoptReason::GuardFailure));

        compiler.record_deopt(key, DeoptReason::GuardFailure);
        compiler.record_deopt_site(key, 10, DeoptReason::GuardFailure);
        assert!(compiler.should_abandon_jit_at_site(key, 10, DeoptReason::GuardFailure));
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
        });
        vm.set_jit_thresholds(1, 1);
        let result = vm.execute(method).expect("top-level fallback should succeed");

        assert_eq!(result, ExecutionResult::Value(Value::Int(-1)));
        assert!(
            vm.jit_executions() >= 1,
            "expected synthetic handler method to attempt JIT execution"
        );
    }
}
