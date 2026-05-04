use super::{CompiledCode, DeoptimizationInfo, GuardCheck, StackSlot, TrapInfo};

pub struct CodeEmitter;

impl CodeEmitter {
    pub fn new() -> Self {
        Self
    }

    pub fn emit(&self, code_buffer: Vec<u8>) -> CompiledCode {
        CompiledCode {
            code_buffer,
            frame_size: 0,
            stack_slots: Vec::new(),
            deopt_info: DeoptimizationInfo {
                guard_checks: Vec::new(),
                trap_info: Vec::new(),
                local_kinds: Vec::new(),
                stack_kinds_by_pc: std::collections::HashMap::new(),
                max_stack_depth: 0,
            },
        }
    }
}

impl Default for CodeEmitter {
    fn default() -> Self {
        Self::new()
    }
}

pub fn emit_to_memory(code_buffer: Vec<u8>) -> CompiledCode {
    let emitter = CodeEmitter::new();
    emitter.emit(code_buffer)
}
