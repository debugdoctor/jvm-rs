use crate::vm::types::Method;
use super::{CompiledCode, JitError, StackSlot, DeoptimizationInfo, GuardCheck, TrapInfo};

pub struct BytecodeCompiler<'a> {
    method: &'a Method,
    frame_size: usize,
    stack_slots: Vec<StackSlot>,
    guard_checks: Vec<GuardCheck>,
    trap_info: Vec<TrapInfo>,
}

impl<'a> BytecodeCompiler<'a> {
    pub fn new(method: &'a Method) -> Self {
        Self {
            method,
            frame_size: 0,
            stack_slots: Vec::new(),
            guard_checks: Vec::new(),
            trap_info: Vec::new(),
        }
    }

    pub fn lower(&mut self) -> Result<(), JitError> {
        let code = &self.method.code;
        let mut pc = 0;

        while pc < code.len() {
            let opcode = code[pc];
            self.lower_opcode(pc, opcode)?;
            pc = self.next_pc(pc, opcode);
        }

        Ok(())
    }

    fn lower_opcode(&mut self, pc: usize, opcode: u8) -> Result<(), JitError> {
        match opcode {
            0x00 => self.lower_aconst_null(),
            0x01..=0x08 => self.lower_iconst(-1i32 - (opcode - 0x01) as i32),
            0x09..=0x0a => self.lower_lconst((opcode - 0x09) as i64),
            0x0b..=0x0d => self.lower_fconst((opcode - 0x0b) as f32),
            0x0e..=0x0f => self.lower_dconst((opcode - 0x0e) as f64),
            0x10 => self.lower_bipush(),
            0x11 => self.lower_sipush(),
            0x12 => self.lower_ldc(),
            0x13 => self.lower_ldc_w(),
            0x14 => self.lower_ldc2_w(),
            0x15 => self.lower_iload(),
            0x16 => self.lower_lload(),
            0x17 => self.lower_fload(),
            0x18 => self.lower_dload(),
            0x19 => self.lower_aload(),
            0x1a..=0x1d => self.lower_iload_n((opcode - 0x1a) as usize),
            0x1e..=0x21 => self.lower_lload_n((opcode - 0x1e) as usize),
            0x22..=0x25 => self.lower_fload_n((opcode - 0x22) as usize),
            0x26..=0x29 => self.lower_dload_n((opcode - 0x26) as usize),
            0x2a..=0x2d => self.lower_aload_n((opcode - 0x2a) as usize),
            0x2e..=0x37 => self.lower_iaload(),
            0x3b => self.lower_istore(),
            0x3c => self.lower_lstore(),
            0x3d => self.lower_fstore(),
            0x3e => self.lower_dstore(),
            0x3f => self.lower_astore(),
            0x4b..=0x4f => self.lower_istore_n((opcode - 0x4b) as usize),
            0x50..=0x53 => self.lower_iastore(),
            0x60 => self.lower_iadd(),
            0x61 => self.lower_ladd(),
            0x62 => self.lower_fadd(),
            0x63 => self.lower_dadd(),
            0x64 => self.lower_isub(),
            0x68 => self.lower_imul(),
            0x6e => self.lower_idiv(),
            0x70 => self.lower_irem(),
            0x74 => self.lower_ineg(),
            0x79 => self.lower_ishl(),
            0x7a => self.lower_ishr(),
            0x7c => self.lower_iushr(),
            0x80 => self.lower_iand(),
            0x82 => self.lower_ior(),
            0x84 => self.lower_iinc(),
            0x99..=0x9e => self.lower_if_icmp(opcode),
            0x9f..=0xa6 => self.lower_if_icmp(opcode),
            0xa7 => self.lower_goto(),
            0xac => self.lower_ireturn(),
            0xb1 => self.lower_return(),
            0xb2 => self.lower_getstatic(),
            0xb4 => self.lower_getfield(),
            0xb5 => self.lower_putfield(),
            0xb6 => self.lower_invokevirtual(),
            0xb7 => self.lower_invokespecial(),
            0xb8 => self.lower_invokestatic(),
            0xb9 => self.lower_invokeinterface(),
            0xba => self.lower_invokedynamic(),
            0xbb => self.lower_new(),
            0xbc => self.lower_newarray(),
            0xbd => self.lower_anewarray(),
            0xbe => self.lower_arraylength(),
            0xbf => self.lower_athrow(),
            0xc0 => self.lower_checkcast(),
            0xc1 => self.lower_instanceof(),
            0xc2 => self.lower_monitorenter(),
            0xc3 => self.lower_monitorexit(),
            0xfe => self.lower_invokenative(),
            0xff => self.lower_athrow(),
            _ => Err(JitError::CompilationFailed(format!("Unsupported opcode: 0x{:02x}", opcode))),
        }
    }

    fn next_pc(&self, pc: usize, opcode: u8) -> usize {
        match opcode {
            0x10 | 0x11 | 0x12 | 0x13 | 0x14 => pc + 2,
            0xa7 | 0xc7 => {
                let offset = ((self.method.code[pc + 1] as i16) << 8) | (self.method.code[pc + 2] as i16);
                (pc as i32 + offset as i32) as usize
            }
            0xb6 | 0xb4 | 0xb2 | 0xb5 | 0xbb | 0xbc | 0xbd | 0xbe | 0xc0 | 0xc1 => pc + 3,
            0xb9 => pc + 5,
            0xba => pc + 5,
            0x84 => pc + 3,
            0x99..=0xa6 => {
                let offset = ((self.method.code[pc + 1] as i16) << 8) | (self.method.code[pc + 2] as i16);
                (pc as i32 + offset as i32) as usize
            }
            _ => pc + 1,
        }
    }

    fn lower_aconst_null(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_iconst(&mut self, _val: i32) -> Result<(), JitError> { Ok(()) }
    fn lower_lconst(&mut self, _val: i64) -> Result<(), JitError> { Ok(()) }
    fn lower_fconst(&mut self, _val: f32) -> Result<(), JitError> { Ok(()) }
    fn lower_dconst(&mut self, _val: f64) -> Result<(), JitError> { Ok(()) }
    fn lower_bipush(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_sipush(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_ldc(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_ldc_w(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_ldc2_w(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_iload(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_lload(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_fload(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_dload(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_aload(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_iload_n(&mut self, _n: usize) -> Result<(), JitError> { Ok(()) }
    fn lower_lload_n(&mut self, _n: usize) -> Result<(), JitError> { Ok(()) }
    fn lower_fload_n(&mut self, _n: usize) -> Result<(), JitError> { Ok(()) }
    fn lower_dload_n(&mut self, _n: usize) -> Result<(), JitError> { Ok(()) }
    fn lower_aload_n(&mut self, _n: usize) -> Result<(), JitError> { Ok(()) }
    fn lower_iaload(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_istore(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_lstore(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_fstore(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_dstore(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_astore(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_istore_n(&mut self, _n: usize) -> Result<(), JitError> { Ok(()) }
    fn lower_lstore_n(&mut self, _n: usize) -> Result<(), JitError> { Ok(()) }
    fn lower_fstore_n(&mut self, _n: usize) -> Result<(), JitError> { Ok(()) }
    fn lower_dstore_n(&mut self, _n: usize) -> Result<(), JitError> { Ok(()) }
    fn lower_astore_n(&mut self, _n: usize) -> Result<(), JitError> { Ok(()) }
    fn lower_iastore(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_lastore(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_fastore(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_dastore(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_aastore(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_bastore(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_castore(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_sastore(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_iadd(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_ladd(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_fadd(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_dadd(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_isub(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_lsub(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_fsub(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_dsub(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_imul(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_lmul(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_fmul(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_dmul(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_idiv(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_ldiv(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_fdiv(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_ddiv(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_irem(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_lrem(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_frem(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_drem(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_ineg(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_lneg(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_fneg(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_dneg(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_ishl(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_lshl(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_ishr(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_lshr(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_iushr(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_lushr(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_iand(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_land(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_ior(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_lor(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_ixor(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_lxor(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_iinc(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_if_icmp(&mut self, _opcode: u8) -> Result<(), JitError> { Ok(()) }
    fn lower_goto(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_ireturn(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_lreturn(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_freturn(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_dreturn(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_areturn(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_return(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_getstatic(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_getfield(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_putfield(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_invokevirtual(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_invokespecial(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_invokestatic(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_invokeinterface(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_invokedynamic(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_new(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_newarray(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_anewarray(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_arraylength(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_athrow(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_checkcast(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_instanceof(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_monitorenter(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_monitorexit(&mut self) -> Result<(), JitError> { Ok(()) }
    fn lower_invokenative(&mut self) -> Result<(), JitError> { Ok(()) }
}

pub fn compile_method(method: &Method) -> Result<CompiledCode, JitError> {
    let mut compiler = BytecodeCompiler::new(method);
    compiler.lower()?;
    Ok(CompiledCode {
        code_buffer: Vec::new(),
        frame_size: 0,
        stack_slots: Vec::new(),
        deopt_info: super::DeoptimizationInfo {
            guard_checks: Vec::new(),
            trap_info: Vec::new(),
        },
    })
}