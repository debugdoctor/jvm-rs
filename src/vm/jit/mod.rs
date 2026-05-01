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

    pub fn get_compiled_code(&self, method_key: &str) -> Option<CompiledCode> {
        self.compiled_code.read().unwrap().get(method_key).cloned()
    }

    pub fn isa(&self) -> &dyn TargetIsa {
        &*self.isa
    }

    pub fn compile(&self, method: &Method) -> Result<CompiledCode, String> {
        compiler::compile_bytecode(method, self.isa())
            .map_err(|e| format!("JIT compilation failed: {:?}", e))
    }

    pub fn get_or_compile(&self, method: &Method) -> Option<CompiledCode> {
        let key = format!("{}.{}{}", method.class_name, method.name, method.descriptor);
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
    use crate::vm::types::{Method, Value};

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
            .execute("jit/Test.constMath()I", &[])
            .expect("missing JIT entry");
        assert_eq!(result, Value::Int(25));
    }
}
