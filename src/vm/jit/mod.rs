pub mod compiler;
pub mod optimizer;
pub mod emitter;
pub mod runtime;

use std::collections::HashMap;
use std::fmt;
use std::sync::RwLock;

use crate::vm::types::Method;
use crate::vm::Frame;
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
    isa: Box<dyn TargetIsa>,
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
        if frame.code.len() > 200 {
            return false;
        }
        if let Some(index) = cp_index {
            let call_count = frame.call_counts.get(&index).copied().unwrap_or(0);
            call_count >= self.invocation_threshold
        } else {
            frame.invocation_count >= self.invocation_threshold
        }
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

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.compile(method)
        }));

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
}

pub fn initialize_jit() {
    println!("JIT Compiler initialized with Cranelift backend");
}