use super::JitError;
use cranelift::codegen::ir::Function;
use cranelift::codegen::{Context, isa::TargetIsa};

/// Simple peephole constant-fold pass over raw bytecode.
/// Returns a new bytecode Vec with folded constants, or None if no change.
/// Folds: iconst_N iconst_M iadd/isub/imul -> single iconst (if result fits in [-1,5]).
pub fn constant_fold_bytecode(code: &[u8]) -> Option<Vec<u8>> {
    let mut result = code.to_vec();
    let mut changed = false;
    let mut i = 0;
    while i + 2 < result.len() {
        let v1 = iconst_value(result[i]);
        let v2 = iconst_value(result[i + 1]);
        if let (Some(a), Some(b)) = (v1, v2) {
            let folded: Option<i32> = match result[i + 2] {
                0x60 => Some(a + b), // iadd
                0x64 => Some(a - b), // isub
                0x68 => Some(a * b), // imul
                _ => None,
            };
            if let Some(val) = folded {
                if let Some(encoded) = iconst_encode(val) {
                    result[i] = encoded;
                    result[i + 1] = 0x00; // nop
                    result[i + 2] = 0x00; // nop
                    changed = true;
                    i += 3;
                    continue;
                }
            }
        }
        i += 1;
    }
    if changed { Some(result) } else { None }
}

fn iconst_value(byte: u8) -> Option<i32> {
    match byte {
        0x02 => Some(-1), // iconst_m1
        0x03 => Some(0),  // iconst_0
        0x04 => Some(1),  // iconst_1
        0x05 => Some(2),  // iconst_2
        0x06 => Some(3),  // iconst_3
        0x07 => Some(4),  // iconst_4
        0x08 => Some(5),  // iconst_5
        _ => None,
    }
}

fn iconst_encode(val: i32) -> Option<u8> {
    match val {
        -1 => Some(0x02), // iconst_m1
        0  => Some(0x03), // iconst_0
        1  => Some(0x04), // iconst_1
        2  => Some(0x05), // iconst_2
        3  => Some(0x06), // iconst_3
        4  => Some(0x07), // iconst_4
        5  => Some(0x08), // iconst_5
        _ => None,
    }
}

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
