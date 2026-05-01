use super::JitError;
use cranelift::codegen::ir::Function;
use cranelift::codegen::{Context, isa::TargetIsa};

pub struct Optimizer {
    inline_threshold: usize,
    max_inline_depth: usize,
}

impl Optimizer {
    pub fn new() -> Self {
        Self {
            inline_threshold: 100,
            max_inline_depth: 10,
        }
    }

    pub fn optimize(&mut self, func: &mut Function, isa: &dyn TargetIsa) -> Result<(), JitError> {
        let mut ctx = Context::new();
        ctx.func = func.clone();
        let mut ctrl_plane = cranelift::codegen::control::ControlPlane::default();
        ctx.optimize(isa, &mut ctrl_plane)
            .map_err(|e| JitError::CompilationFailed(format!("optimization failed: {}", e)))?;

        *func = ctx.func;
        Ok(())
    }

    pub fn should_inline(&self, code_len: usize) -> bool {
        code_len < self.inline_threshold
    }
}

impl Default for Optimizer {
    fn default() -> Self {
        Self::new()
    }
}

pub fn optimize_function(func: &mut Function, isa: &dyn TargetIsa) -> Result<(), JitError> {
    let mut optimizer = Optimizer::new();
    optimizer.optimize(func, isa)
}
