use cranelift::codegen::ir::{AbiParam, Signature, TrapCode, UserFuncName};
use cranelift::codegen::{Context, isa::TargetIsa};
use cranelift::prelude::*;
use cranelift_frontend::{FunctionBuilderContext, Variable};

use super::{CompiledCode, DeoptimizationInfo, GuardCheck, GuardType, JitError, StackSlot};
use crate::vm::types::Method;

const X64_ALIGN: u16 = 16;
const JIT_TRAP_CODE: TrapCode = TrapCode::unwrap_user(1);

pub struct BytecodeCompiler<'a> {
    method: &'a Method,
    builder: &'a mut FunctionBuilder<'a>,
    value_stack: Vec<Value>,
    frame_size: usize,
    stack_slots: Vec<StackSlot>,
    guard_checks: Vec<GuardCheck>,
    pc_offset: usize,
    local_vars_initialized: bool,
    arg_types: Vec<u8>,
    context_var: Variable,
    local_vars: Vec<Variable>,
    block_map: std::collections::HashMap<usize, Block>,
    branch_targets: std::collections::HashSet<usize>,
}

impl<'a> BytecodeCompiler<'a> {
    pub fn new(
        method: &'a Method,
        builder: &'a mut FunctionBuilder<'a>,
        arg_types: Vec<u8>,
    ) -> Self {
        Self {
            method,
            builder,
            value_stack: Vec::new(),
            frame_size: 0,
            stack_slots: Vec::new(),
            guard_checks: Vec::new(),
            pc_offset: 0,
            local_vars_initialized: false,
            arg_types,
            context_var: Variable::new(0),
            local_vars: Vec::new(),
            block_map: std::collections::HashMap::new(),
            branch_targets: std::collections::HashSet::new(),
        }
    }

    pub fn lower(&mut self) -> Result<(), JitError> {
        self.collect_branch_targets();
        self.create_blocks()?;
        self.lower_with_blocks()
    }

    fn collect_branch_targets(&mut self) {
        self.branch_targets.clear();
        self.branch_targets.insert(0);
        let code = &self.method.code;
        let mut pc = 0;

        while pc < code.len() {
            let opcode = code[pc];
            self.collect_branch_target_for_opcode(pc, opcode);
            pc = self.next_pc(pc, opcode);
        }
    }

    fn collect_branch_target_for_opcode(&mut self, pc: usize, opcode: u8) {
        match opcode {
            0x99..=0x9e | 0x9f..=0xa6 => {
                let offset =
                    ((self.method.code[pc + 1] as i16) << 8) | (self.method.code[pc + 2] as i16);
                let target = (pc as i32 + offset as i32) as usize;
                self.branch_targets.insert(target);
                self.branch_targets.insert(pc + 3);
            }
            0xa7 | 0xa8 => {
                let offset =
                    ((self.method.code[pc + 1] as i16) << 8) | (self.method.code[pc + 2] as i16);
                let target = (pc as i32 + offset as i32) as usize;
                self.branch_targets.insert(target);
            }
            0xaa => {
                let default_offset = ((self.method.code[pc + 1] as i32) << 24)
                    | ((self.method.code[pc + 2] as i32) << 16)
                    | ((self.method.code[pc + 3] as i32) << 8)
                    | (self.method.code[pc + 4] as i32);
                let default_target = (pc as i32 + default_offset) as usize;
                self.branch_targets.insert(default_target);

                let mut new_pc = (pc + 4) & !3;
                let low = ((self.method.code[new_pc] as i32) << 24)
                    | ((self.method.code[new_pc + 1] as i32) << 16)
                    | ((self.method.code[new_pc + 2] as i32) << 8)
                    | (self.method.code[new_pc + 3] as i32);
                new_pc += 4;
                let high = ((self.method.code[new_pc] as i32) << 24)
                    | ((self.method.code[new_pc + 1] as i32) << 16)
                    | ((self.method.code[new_pc + 2] as i32) << 8)
                    | (self.method.code[new_pc + 3] as i32);
                new_pc += 4;
                for i in low..=high {
                    let val = ((self.method.code[new_pc] as i32) << 24)
                        | ((self.method.code[new_pc + 1] as i32) << 16)
                        | ((self.method.code[new_pc + 2] as i32) << 8)
                        | (self.method.code[new_pc + 3] as i32);
                    let target = (pc as i32 + val) as usize;
                    self.branch_targets.insert(target);
                    new_pc += 4;
                }
            }
            0xab => {
                let default_offset = ((self.method.code[pc + 1] as i32) << 24)
                    | ((self.method.code[pc + 2] as i32) << 16)
                    | ((self.method.code[pc + 3] as i32) << 8)
                    | (self.method.code[pc + 4] as i32);
                let default_target = (pc as i32 + default_offset) as usize;
                self.branch_targets.insert(default_target);

                let mut new_pc = (pc + 4) & !3;
                let num_pairs = ((self.method.code[new_pc] as i32) << 24)
                    | ((self.method.code[new_pc + 1] as i32) << 16)
                    | ((self.method.code[new_pc + 2] as i32) << 8)
                    | (self.method.code[new_pc + 3] as i32);
                new_pc += 4;
                for _ in 0..num_pairs {
                    let key = ((self.method.code[new_pc] as i32) << 24)
                        | ((self.method.code[new_pc + 1] as i32) << 16)
                        | ((self.method.code[new_pc + 2] as i32) << 8)
                        | (self.method.code[new_pc + 3] as i32);
                    let val = ((self.method.code[new_pc + 4] as i32) << 24)
                        | ((self.method.code[new_pc + 5] as i32) << 16)
                        | ((self.method.code[new_pc + 6] as i32) << 8)
                        | (self.method.code[new_pc + 7] as i32);
                    let target = (pc as i32 + val) as usize;
                    self.branch_targets.insert(target);
                    new_pc += 8;
                }
            }
            _ => {}
        }
    }

    fn create_blocks(&mut self) -> Result<(), JitError> {
        self.block_map.clear();
        for &target in &self.branch_targets {
            let block = self.builder.create_block();
            self.block_map.insert(target, block);
        }
        Ok(())
    }

    fn lower_with_blocks(&mut self) -> Result<(), JitError> {
        self.initialize_local_vars()?;

        let entry_block = *self.block_map.get(&0).unwrap();

        let code = &self.method.code;
        let mut pc = 0;
        let mut current_block = Some(entry_block);

        while pc < code.len() {
            if let Some(&block) = self.block_map.get(&pc) {
                if Some(block) != current_block {
                    current_block = Some(block);
                    self.builder.switch_to_block(block);
                }
            } else if current_block.is_none() {
                let opcode = code[pc];
                pc = self.next_pc(pc, opcode);
                continue;
            }

            self.pc_offset = pc;
            let opcode = code[pc];
            let next_pc = self.next_pc(pc, opcode);

            self.lower_opcode(pc, opcode)?;

            if self.opcode_terminates_block(opcode) {
                current_block = None;
            } else if next_pc < code.len() && self.branch_targets.contains(&next_pc) {
                let fall_block = *self.block_map.get(&next_pc).unwrap();
                self.builder.ins().jump(fall_block, &[]);
                current_block = None;
            }

            pc = next_pc;
        }

        for (&_target, &block) in &self.block_map {
            self.builder.seal_block(block);
        }

        Ok(())
    }

    fn opcode_terminates_block(&self, opcode: u8) -> bool {
        matches!(
            opcode,
            0x99..=0xa8 | 0xaa | 0xab | 0xac | 0xad | 0xae | 0xaf | 0xb0 | 0xb1 | 0xbf | 0xff
        )
    }

    fn read_branch_offset_for_pc(&self, pc: usize, code: &[u8]) -> i32 {
        let high = code[pc + 1] as i16;
        let low = code[pc + 2] as u16;
        i16::from_be_bytes([high as u8, low as u8]) as i32
    }

    fn get_switch_targets_for_pc(&self, pc: usize, code: &[u8]) -> Vec<usize> {
        let mut targets = Vec::new();
        let mut offset = (pc + 4) & !3;
        let low = ((code[offset] as i32) << 24)
            | ((code[offset + 1] as i32) << 16)
            | ((code[offset + 2] as i32) << 8)
            | (code[offset + 3] as i32);
        offset += 4;
        let high = ((code[offset] as i32) << 24)
            | ((code[offset + 1] as i32) << 16)
            | ((code[offset + 2] as i32) << 8)
            | (code[offset + 3] as i32);
        offset += 4;
        for _ in low..=high {
            let val = ((code[offset] as i32) << 24)
                | ((code[offset + 1] as i32) << 16)
                | ((code[offset + 2] as i32) << 8)
                | (code[offset + 3] as i32);
            targets.push((pc as i32 + val) as usize);
            offset += 4;
        }
        targets
    }

    fn lower_opcode(&mut self, pc: usize, opcode: u8) -> Result<(), JitError> {
        match opcode {
            0x00 => Ok(()), // nop
            0x01 => self.lower_aconst_null(),
            0x02..=0x08 => self.lower_iconst((opcode as i32) - 0x03),
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
            0x2e => self.lower_iaload(),
            0x2f => self.lower_laload(),
            0x30 => self.lower_faload(),
            0x31 => self.lower_daload(),
            0x32 => self.lower_aaload(),
            0x33 => self.lower_baload(),
            0x34 => self.lower_caload(),
            0x35 => self.lower_saload(),
            0x36 => self.lower_istore(),
            0x37 => self.lower_lstore(),
            0x38 => self.lower_fstore(),
            0x39 => self.lower_dstore(),
            0x3a => self.lower_astore(),
            0x3b..=0x3e => self.lower_istore_n((opcode - 0x3b) as usize),
            0x3f..=0x42 => self.lower_lstore_n((opcode - 0x3f) as usize),
            0x43..=0x46 => self.lower_fstore_n((opcode - 0x43) as usize),
            0x47..=0x4a => self.lower_dstore_n((opcode - 0x47) as usize),
            0x4b..=0x4e => self.lower_astore_n((opcode - 0x4b) as usize),
            0x4f => self.lower_iastore(),
            0x50 => self.lower_lastore(),
            0x51 => self.lower_fastore(),
            0x52 => self.lower_dastore(),
            0x53 => self.lower_aastore(),
            0x54 => self.lower_bastore(),
            0x55 => self.lower_castore(),
            0x56 => self.lower_sastore(),
            0x57 => self.lower_pop(),
            0x58 => self.lower_pop2(),
            0x59 => self.lower_dup(),
            0x5a => self.lower_dup_x1(),
            0x5b => self.lower_dup_x2(),
            0x5c => self.lower_dup2(),
            0x5d => self.lower_dup2_x1(),
            0x5e => self.lower_dup2_x2(),
            0x5f => self.lower_swap(),
            0x60 => self.lower_iadd(),
            0x61 => self.lower_ladd(),
            0x62 => self.lower_fadd(),
            0x63 => self.lower_dadd(),
            0x64 => self.lower_isub(),
            0x65 => self.lower_lsub(),
            0x66 => self.lower_fsub(),
            0x67 => self.lower_dsub(),
            0x68 => self.lower_imul(),
            0x69 => self.lower_lmul(),
            0x6a => self.lower_fmul(),
            0x6b => self.lower_dmul(),
            0x6c => self.lower_ldiv(),
            0x6d => self.lower_fdiv(),
            0x6e => self.lower_idiv(),
            0x6f => self.lower_ddiv(),
            0x70 => self.lower_lrem(),
            0x71 => self.lower_irem(),
            0x72 => self.lower_frem(),
            0x73 => self.lower_drem(),
            0x74 => self.lower_ineg(),
            0x75 => self.lower_lneg(),
            0x76 => self.lower_fneg(),
            0x77 => self.lower_dneg(),
            0x78 => self.lower_ishl(),
            0x79 => self.lower_lshl(),
            0x7a => self.lower_ishr(),
            0x7b => self.lower_lshr(),
            0x7c => self.lower_iushr(),
            0x7d => self.lower_lushr(),
            0x7e => self.lower_ixor(),
            0x7f => self.lower_land(),
            0x80 => self.lower_lor(),
            0x81 => self.lower_lxor(),
            0x82 => self.lower_ior(),
            0x83 => self.lower_ixor(),
            0x84 => self.lower_iinc(),
            0x85 => self.lower_i2l(),
            0x86 => self.lower_i2f(),
            0x87 => self.lower_i2d(),
            0x88 => self.lower_l2i(),
            0x89 => self.lower_l2f(),
            0x8a => self.lower_l2d(),
            0x8b => self.lower_f2i(),
            0x8c => self.lower_f2l(),
            0x8d => self.lower_f2d(),
            0x8e => self.lower_d2i(),
            0x8f => self.lower_d2l(),
            0x90 => self.lower_d2f(),
            0x91 => self.lower_i2b(),
            0x92 => self.lower_i2c(),
            0x93 => self.lower_i2s(),
            0x94 => self.lower_lcmp(),
            0x95 => self.lower_fcmpl(),
            0x96 => self.lower_fcmpg(),
            0x97 => self.lower_dcmpl(),
            0x98 => self.lower_dcmpg(),
            0x99..=0x9e => self.lower_if_icmp(opcode),
            0x9f..=0xa6 => self.lower_if_icmp(opcode),
            0xa7 => self.lower_goto(),
            0xa8 => self.lower_goto(),
            0xaa => self.lower_tableswitch(),
            0xab => self.lower_lookupswitch(),
            0xac => self.lower_ireturn(),
            0xad => self.lower_lreturn(),
            0xae => self.lower_freturn(),
            0xaf => self.lower_dreturn(),
            0xb0 => self.lower_areturn(),
            0xb1 => self.lower_return(),
            0xb2 => self.lower_getstatic(),
            0xb3 => self.lower_putstatic(),
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
            0xc5 => self.lower_multianewarray(),
            0xfe => self.lower_invokenative(),
            0xff => self.lower_athrow(),
            _ => Err(JitError::CompilationFailed(format!(
                "Unsupported opcode: 0x{:02x}",
                opcode
            ))),
        }
    }

    fn local_var(&self, index: usize) -> Result<Variable, JitError> {
        self.local_vars.get(index).copied().ok_or_else(|| {
            JitError::CompilationFailed(format!("local variable {} was not declared", index))
        })
    }

    fn next_pc(&self, pc: usize, opcode: u8) -> usize {
        match opcode {
            0x10 | 0x12 | 0x15..=0x19 | 0x36..=0x3a | 0xa9 | 0xbc => pc + 2,
            0x11 | 0x13 | 0x14 | 0x84 => pc + 3,
            0x99..=0xa8 | 0xc6 | 0xc7 => pc + 3,
            0xb2..=0xb8 | 0xbb | 0xbd | 0xc0 | 0xc1 | 0xfe => pc + 3,
            0xb9 | 0xba => pc + 5,
            0xc5 => pc + 4,
            0xaa => {
                let mut new_pc = (pc + 4) & !3;
                let low = ((self.method.code[new_pc] as i32) << 24)
                    | ((self.method.code[new_pc + 1] as i32) << 16)
                    | ((self.method.code[new_pc + 2] as i32) << 8)
                    | (self.method.code[new_pc + 3] as i32);
                new_pc += 4;
                let high = ((self.method.code[new_pc] as i32) << 24)
                    | ((self.method.code[new_pc + 1] as i32) << 16)
                    | ((self.method.code[new_pc + 2] as i32) << 8)
                    | (self.method.code[new_pc + 3] as i32);
                new_pc += 4 + ((high - low + 1) * 4) as usize;
                new_pc
            }
            0xab => {
                let mut new_pc = (pc + 4) & !3;
                new_pc += 4;
                let num_pairs = ((self.method.code[new_pc] as i32) << 24)
                    | ((self.method.code[new_pc + 1] as i32) << 16)
                    | ((self.method.code[new_pc + 2] as i32) << 8)
                    | (self.method.code[new_pc + 3] as i32);
                new_pc += 4 + (num_pairs * 8) as usize;
                new_pc
            }
            _ => pc + 1,
        }
    }

    fn push(&mut self, value: Value) {
        self.value_stack.push(value);
    }

    fn pop(&mut self) -> Value {
        self.value_stack.pop().expect("stack underflow")
    }

    fn iconst(&mut self, val: i32) -> Value {
        self.builder.ins().iconst(types::I32, val as i64)
    }

    fn lconst(&mut self, val: i64) -> Value {
        self.builder.ins().iconst(types::I64, val)
    }

    fn fconst(&mut self, val: f32) -> Value {
        self.builder.ins().f32const(val)
    }

    fn dconst(&mut self, val: f64) -> Value {
        self.builder.ins().f64const(val)
    }

    fn lower_aconst_null(&mut self) -> Result<(), JitError> {
        let null = self.builder.ins().iconst(types::I64, 0);
        self.push(null);
        Ok(())
    }

    fn lower_iconst(&mut self, val: i32) -> Result<(), JitError> {
        let c = self.iconst(val);
        self.push(c);
        Ok(())
    }

    fn lower_lconst(&mut self, val: i64) -> Result<(), JitError> {
        let c = self.lconst(val);
        self.push(c);
        Ok(())
    }

    fn lower_fconst(&mut self, val: f32) -> Result<(), JitError> {
        let c = self.fconst(val);
        self.push(c);
        Ok(())
    }

    fn lower_dconst(&mut self, val: f64) -> Result<(), JitError> {
        let c = self.dconst(val);
        self.push(c);
        Ok(())
    }

    fn lower_bipush(&mut self) -> Result<(), JitError> {
        let byte = self.method.code[self.pc_offset + 1] as i8 as i32;
        let c = self.iconst(byte);
        self.push(c);
        Ok(())
    }

    fn lower_sipush(&mut self) -> Result<(), JitError> {
        let high = self.method.code[self.pc_offset + 1] as i16;
        let low = self.method.code[self.pc_offset + 2] as u16;
        let val = i16::from_be_bytes([high as u8, low as u8]) as i32;
        let c = self.iconst(val);
        self.push(c);
        Ok(())
    }

    fn lower_ldc(&mut self) -> Result<(), JitError> {
        let index = self.method.code[self.pc_offset + 1] as usize;
        self.load_constant(index)
    }

    fn lower_ldc_w(&mut self) -> Result<(), JitError> {
        let index = ((self.method.code[self.pc_offset + 1] as usize) << 8)
            | (self.method.code[self.pc_offset + 2] as usize);
        self.load_constant(index)
    }

    fn lower_ldc2_w(&mut self) -> Result<(), JitError> {
        let index = ((self.method.code[self.pc_offset + 1] as usize) << 8)
            | (self.method.code[self.pc_offset + 2] as usize);
        self.load_constant(index)
    }

    fn load_constant(&mut self, index: usize) -> Result<(), JitError> {
        if let Some(Some(const_val)) = self.method.constants.get(index) {
            match const_val {
                crate::vm::types::Value::Int(i) => {
                    let c = self.iconst(*i);
                    self.push(c);
                }
                crate::vm::types::Value::Long(l) => {
                    let c = self.lconst(*l);
                    self.push(c);
                }
                crate::vm::types::Value::Float(f) => {
                    let c = self.fconst(*f);
                    self.push(c);
                }
                crate::vm::types::Value::Double(d) => {
                    let c = self.dconst(*d);
                    self.push(c);
                }
                crate::vm::types::Value::Reference(reference) => {
                    let raw = match reference {
                        crate::vm::types::Reference::Null => 0i64,
                        crate::vm::types::Reference::Heap(index) => (*index as i64) + 1,
                    };
                    let value = self.builder.ins().iconst(types::I64, raw);
                    self.push(value);
                }
                _ => {
                    return Err(JitError::CompilationFailed(
                        "Unsupported constant type".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    fn lower_iload(&mut self) -> Result<(), JitError> {
        let index = self.method.code[self.pc_offset + 1] as usize;
        self.load_local(index, types::I32)
    }

    fn lower_lload(&mut self) -> Result<(), JitError> {
        let index = self.method.code[self.pc_offset + 1] as usize;
        self.load_local(index, types::I64)
    }

    fn lower_fload(&mut self) -> Result<(), JitError> {
        let index = self.method.code[self.pc_offset + 1] as usize;
        self.load_local(index, types::F32)
    }

    fn lower_dload(&mut self) -> Result<(), JitError> {
        let index = self.method.code[self.pc_offset + 1] as usize;
        self.load_local(index, types::F64)
    }

    fn lower_aload(&mut self) -> Result<(), JitError> {
        let index = self.method.code[self.pc_offset + 1] as usize;
        self.load_local(index, types::I64)
    }

    fn load_local(&mut self, index: usize, _ty: Type) -> Result<(), JitError> {
        let var = self.local_var(index)?;
        let value = self.builder.use_var(var);
        let declared_ty = self.local_var_type(index);
        let requested_ty = _ty;
        let coerced = match (declared_ty, requested_ty) {
            (types::I64, types::I32) => self.builder.ins().ireduce(types::I32, value),
            (types::I32, types::I64) => self.builder.ins().sextend(types::I64, value),
            _ => value,
        };
        self.push(coerced);
        Ok(())
    }

    fn lower_iload_n(&mut self, n: usize) -> Result<(), JitError> {
        self.load_local(n, types::I32)
    }

    fn lower_lload_n(&mut self, n: usize) -> Result<(), JitError> {
        self.load_local(n, types::I64)
    }

    fn lower_fload_n(&mut self, n: usize) -> Result<(), JitError> {
        self.load_local(n, types::F32)
    }

    fn lower_dload_n(&mut self, n: usize) -> Result<(), JitError> {
        self.load_local(n, types::F64)
    }

    fn lower_aload_n(&mut self, n: usize) -> Result<(), JitError> {
        self.load_local(n, types::I64)
    }

    fn lower_iaload(&mut self) -> Result<(), JitError> {
        let index = self.pop();
        let array_ref = self.pop();
        let zero = self.builder.ins().iconst(types::I64, 0);
        let raw = self.emit_field_helper_call(
            crate::vm::jit::runtime::get_load_typed_array_element_ptr(),
            [array_ref, index, zero, zero, zero],
        )?;
        let val = self.coerce_raw_field_result(raw, "I")?;
        self.push(val);
        Ok(())
    }

    fn lower_laload(&mut self) -> Result<(), JitError> {
        let index = self.pop();
        let array_ref = self.pop();
        let zero = self.builder.ins().iconst(types::I64, 0);
        let raw = self.emit_field_helper_call(
            crate::vm::jit::runtime::get_load_typed_array_element_ptr(),
            [array_ref, index, zero, zero, zero],
        )?;
        let val = self.coerce_raw_field_result(raw, "J")?;
        self.push(val);
        Ok(())
    }

    fn lower_faload(&mut self) -> Result<(), JitError> {
        let index = self.pop();
        let array_ref = self.pop();
        let zero = self.builder.ins().iconst(types::I64, 0);
        let raw = self.emit_field_helper_call(
            crate::vm::jit::runtime::get_load_typed_array_element_ptr(),
            [array_ref, index, zero, zero, zero],
        )?;
        let val = self.coerce_raw_field_result(raw, "F")?;
        self.push(val);
        Ok(())
    }

    fn lower_daload(&mut self) -> Result<(), JitError> {
        let index = self.pop();
        let array_ref = self.pop();
        let zero = self.builder.ins().iconst(types::I64, 0);
        let raw = self.emit_field_helper_call(
            crate::vm::jit::runtime::get_load_typed_array_element_ptr(),
            [array_ref, index, zero, zero, zero],
        )?;
        let val = self.coerce_raw_field_result(raw, "D")?;
        self.push(val);
        Ok(())
    }

    fn lower_aaload(&mut self) -> Result<(), JitError> {
        let index = self.pop();
        let array_ref = self.pop();
        let zero = self.builder.ins().iconst(types::I64, 0);
        let raw = self.emit_field_helper_call(
            crate::vm::jit::runtime::get_load_reference_array_element_ptr(),
            [array_ref, index, zero, zero, zero],
        )?;
        self.push(raw);
        Ok(())
    }

    fn lower_baload(&mut self) -> Result<(), JitError> {
        let index = self.pop();
        let array_ref = self.pop();
        let zero = self.builder.ins().iconst(types::I64, 0);
        let raw = self.emit_field_helper_call(
            crate::vm::jit::runtime::get_load_typed_array_element_ptr(),
            [array_ref, index, zero, zero, zero],
        )?;
        let val = self.coerce_raw_field_result(raw, "I")?;
        self.push(val);
        Ok(())
    }

    fn lower_caload(&mut self) -> Result<(), JitError> {
        let index = self.pop();
        let array_ref = self.pop();
        let zero = self.builder.ins().iconst(types::I64, 0);
        let raw = self.emit_field_helper_call(
            crate::vm::jit::runtime::get_load_typed_array_element_ptr(),
            [array_ref, index, zero, zero, zero],
        )?;
        let val = self.coerce_raw_field_result(raw, "I")?;
        self.push(val);
        Ok(())
    }

    fn lower_saload(&mut self) -> Result<(), JitError> {
        let index = self.pop();
        let array_ref = self.pop();
        let zero = self.builder.ins().iconst(types::I64, 0);
        let raw = self.emit_field_helper_call(
            crate::vm::jit::runtime::get_load_typed_array_element_ptr(),
            [array_ref, index, zero, zero, zero],
        )?;
        let val = self.coerce_raw_field_result(raw, "I")?;
        self.push(val);
        Ok(())
    }

    fn lower_istore(&mut self) -> Result<(), JitError> {
        let index = self.method.code[self.pc_offset + 1] as usize;
        self.store_local(index, types::I32)
    }

    fn lower_lstore(&mut self) -> Result<(), JitError> {
        let index = self.method.code[self.pc_offset + 1] as usize;
        self.store_local(index, types::I64)
    }

    fn lower_fstore(&mut self) -> Result<(), JitError> {
        let index = self.method.code[self.pc_offset + 1] as usize;
        self.store_local(index, types::F32)
    }

    fn lower_dstore(&mut self) -> Result<(), JitError> {
        let index = self.method.code[self.pc_offset + 1] as usize;
        self.store_local(index, types::F64)
    }

    fn lower_astore(&mut self) -> Result<(), JitError> {
        let index = self.method.code[self.pc_offset + 1] as usize;
        self.store_local(index, types::I64)
    }

    fn store_local(&mut self, index: usize, _ty: Type) -> Result<(), JitError> {
        let value = self.pop();
        let var = self.local_var(index)?;
        let declared_ty = self.local_var_type(index);
        let stored = match (_ty, declared_ty) {
            (types::I32, types::I64) => self.builder.ins().sextend(types::I64, value),
            (types::I64, types::I32) => self.builder.ins().ireduce(types::I32, value),
            _ => value,
        };
        self.builder.def_var(var, stored);
        Ok(())
    }

    fn lower_istore_n(&mut self, n: usize) -> Result<(), JitError> {
        self.store_local(n, types::I32)
    }

    fn lower_lstore_n(&mut self, n: usize) -> Result<(), JitError> {
        self.store_local(n, types::I64)
    }

    fn lower_fstore_n(&mut self, n: usize) -> Result<(), JitError> {
        self.store_local(n, types::F32)
    }

    fn lower_dstore_n(&mut self, n: usize) -> Result<(), JitError> {
        self.store_local(n, types::F64)
    }

    fn lower_astore_n(&mut self, n: usize) -> Result<(), JitError> {
        self.store_local(n, types::I64)
    }

    fn lower_iastore(&mut self) -> Result<(), JitError> {
        let value = self.pop();
        let index = self.pop();
        let array_ref = self.pop();
        self.emit_typed_array_store(array_ref, index, value)?;
        Ok(())
    }

    fn lower_lastore(&mut self) -> Result<(), JitError> {
        let value = self.pop();
        let index = self.pop();
        let array_ref = self.pop();
        self.emit_typed_array_store(array_ref, index, value)?;
        Ok(())
    }

    fn lower_fastore(&mut self) -> Result<(), JitError> {
        let value = self.pop();
        let index = self.pop();
        let array_ref = self.pop();
        self.emit_typed_array_store(array_ref, index, value)?;
        Ok(())
    }

    fn lower_dastore(&mut self) -> Result<(), JitError> {
        let value = self.pop();
        let index = self.pop();
        let array_ref = self.pop();
        self.emit_typed_array_store(array_ref, index, value)?;
        Ok(())
    }

    fn lower_aastore(&mut self) -> Result<(), JitError> {
        let value = self.pop();
        let index = self.pop();
        let array_ref = self.pop();
        let zero = self.builder.ins().iconst(types::I64, 0);
        self.emit_field_helper_call(
            crate::vm::jit::runtime::get_store_reference_array_element_ptr(),
            [array_ref, index, value, zero, zero],
        )?;
        Ok(())
    }

    fn lower_bastore(&mut self) -> Result<(), JitError> {
        let value = self.pop();
        let index = self.pop();
        let array_ref = self.pop();
        self.emit_typed_array_store(array_ref, index, value)?;
        Ok(())
    }

    fn lower_castore(&mut self) -> Result<(), JitError> {
        let value = self.pop();
        let index = self.pop();
        let array_ref = self.pop();
        self.emit_typed_array_store(array_ref, index, value)?;
        Ok(())
    }

    fn lower_sastore(&mut self) -> Result<(), JitError> {
        let value = self.pop();
        let index = self.pop();
        let array_ref = self.pop();
        self.emit_typed_array_store(array_ref, index, value)?;
        Ok(())
    }

    fn lower_pop(&mut self) -> Result<(), JitError> {
        self.pop();
        Ok(())
    }

    fn lower_pop2(&mut self) -> Result<(), JitError> {
        self.pop();
        self.pop();
        Ok(())
    }

    fn lower_dup(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        self.push(val.clone());
        self.push(val);
        Ok(())
    }

    fn lower_dup_x1(&mut self) -> Result<(), JitError> {
        let val1 = self.pop();
        let val2 = self.pop();
        self.push(val1.clone());
        self.push(val2);
        self.push(val1);
        Ok(())
    }

    fn lower_dup_x2(&mut self) -> Result<(), JitError> {
        let val1 = self.pop();
        let val2 = self.pop();
        let val3 = self.pop();
        self.push(val1.clone());
        self.push(val3);
        self.push(val2);
        self.push(val1);
        Ok(())
    }

    fn lower_dup2(&mut self) -> Result<(), JitError> {
        let val1 = self.pop();
        let val2 = self.pop();
        self.push(val1.clone());
        self.push(val2.clone());
        self.push(val1);
        self.push(val2);
        Ok(())
    }

    fn lower_dup2_x1(&mut self) -> Result<(), JitError> {
        let val1 = self.pop();
        let val2 = self.pop();
        let val3 = self.pop();
        self.push(val1.clone());
        self.push(val2.clone());
        self.push(val3);
        self.push(val1);
        self.push(val2);
        Ok(())
    }

    fn lower_dup2_x2(&mut self) -> Result<(), JitError> {
        let val1 = self.pop();
        let val2 = self.pop();
        let val3 = self.pop();
        let val4 = self.pop();
        self.push(val1.clone());
        self.push(val2.clone());
        self.push(val4);
        self.push(val3);
        self.push(val1);
        self.push(val2);
        Ok(())
    }

    fn lower_swap(&mut self) -> Result<(), JitError> {
        let val1 = self.pop();
        let val2 = self.pop();
        self.push(val1);
        self.push(val2);
        Ok(())
    }

    fn lower_iadd(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let result = self.builder.ins().iadd(lhs, rhs);
        self.push(result);
        Ok(())
    }

    fn lower_ladd(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let result = self.builder.ins().iadd(lhs, rhs);
        self.push(result);
        Ok(())
    }

    fn lower_fadd(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let result = self.builder.ins().fadd(lhs, rhs);
        self.push(result);
        Ok(())
    }

    fn lower_dadd(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let result = self.builder.ins().fadd(lhs, rhs);
        self.push(result);
        Ok(())
    }

    fn lower_isub(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let result = self.builder.ins().isub(lhs, rhs);
        self.push(result);
        Ok(())
    }

    fn lower_imul(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let result = self.builder.ins().imul(lhs, rhs);
        self.push(result);
        Ok(())
    }

    fn lower_idiv(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let result = self.builder.ins().sdiv(lhs, rhs);
        self.push(result);
        Ok(())
    }

    fn lower_irem(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let quotient = self.builder.ins().sdiv(lhs, rhs);
        let product = self.builder.ins().imul(quotient, rhs);
        let remainder = self.builder.ins().isub(lhs, product);
        self.push(remainder);
        Ok(())
    }

    fn lower_ineg(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        let zero = self.iconst(0);
        let result = self.builder.ins().isub(zero, val);
        self.push(result);
        Ok(())
    }

    fn lower_ishl(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let mask = self.builder.ins().iconst(types::I64, 0x1f);
        let shifted = self.builder.ins().ushr(rhs, mask);
        let result = self.builder.ins().ishl(lhs, shifted);
        self.push(result);
        Ok(())
    }

    fn lower_ishr(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let mask = self.builder.ins().iconst(types::I64, 0x1f);
        let shifted = self.builder.ins().ushr(rhs, mask);
        let result = self.builder.ins().sshr(lhs, shifted);
        self.push(result);
        Ok(())
    }

    fn lower_iushr(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let mask = self.builder.ins().iconst(types::I64, 0x1f);
        let shifted = self.builder.ins().ushr(rhs, mask);
        let result = self.builder.ins().ushr(lhs, shifted);
        self.push(result);
        Ok(())
    }

    fn lower_iand(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let result = self.builder.ins().band(lhs, rhs);
        self.push(result);
        Ok(())
    }

    fn lower_ior(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let result = self.builder.ins().bor(lhs, rhs);
        self.push(result);
        Ok(())
    }

    fn lower_ixor(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let result = self.builder.ins().bxor(lhs, rhs);
        self.push(result);
        Ok(())
    }

    fn lower_lsub(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let result = self.builder.ins().isub(lhs, rhs);
        self.push(result);
        Ok(())
    }

    fn lower_fsub(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let result = self.builder.ins().fsub(lhs, rhs);
        self.push(result);
        Ok(())
    }

    fn lower_dsub(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let result = self.builder.ins().fsub(lhs, rhs);
        self.push(result);
        Ok(())
    }

    fn lower_lmul(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let result = self.builder.ins().imul(lhs, rhs);
        self.push(result);
        Ok(())
    }

    fn lower_fmul(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let result = self.builder.ins().fmul(lhs, rhs);
        self.push(result);
        Ok(())
    }

    fn lower_dmul(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let result = self.builder.ins().fmul(lhs, rhs);
        self.push(result);
        Ok(())
    }

    fn lower_ldiv(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let result = self.builder.ins().sdiv(lhs, rhs);
        self.push(result);
        Ok(())
    }

    fn lower_fdiv(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let result = self.builder.ins().fdiv(lhs, rhs);
        self.push(result);
        Ok(())
    }

    fn lower_ddiv(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let result = self.builder.ins().fdiv(lhs, rhs);
        self.push(result);
        Ok(())
    }

    fn lower_lrem(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let quotient = self.builder.ins().sdiv(lhs, rhs);
        let product = self.builder.ins().imul(quotient, rhs);
        let remainder = self.builder.ins().isub(lhs, product);
        self.push(remainder);
        Ok(())
    }

    fn lower_frem(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let quotient = self.builder.ins().fdiv(lhs, rhs);
        let floor = self.builder.ins().floor(quotient);
        let product = self.builder.ins().fmul(floor, rhs);
        let remainder = self.builder.ins().fsub(lhs, product);
        self.push(remainder);
        Ok(())
    }

    fn lower_drem(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let quotient = self.builder.ins().fdiv(lhs, rhs);
        let floor = self.builder.ins().floor(quotient);
        let product = self.builder.ins().fmul(floor, rhs);
        let remainder = self.builder.ins().fsub(lhs, product);
        self.push(remainder);
        Ok(())
    }

    fn lower_lneg(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        let zero = self.lconst(0);
        let result = self.builder.ins().isub(zero, val);
        self.push(result);
        Ok(())
    }

    fn lower_fneg(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        let neg = self.builder.ins().fneg(val);
        self.push(neg);
        Ok(())
    }

    fn lower_dneg(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        let neg = self.builder.ins().fneg(val);
        self.push(neg);
        Ok(())
    }

    fn lower_lshl(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let mask = self.builder.ins().iconst(types::I64, 0x3f);
        let shifted = self.builder.ins().band(rhs, mask);
        let result = self.builder.ins().ishl(lhs, shifted);
        self.push(result);
        Ok(())
    }

    fn lower_lshr(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let mask = self.builder.ins().iconst(types::I64, 0x3f);
        let shifted = self.builder.ins().band(rhs, mask);
        let result = self.builder.ins().sshr(lhs, shifted);
        self.push(result);
        Ok(())
    }

    fn lower_lushr(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let mask = self.builder.ins().iconst(types::I64, 0x3f);
        let shifted = self.builder.ins().band(rhs, mask);
        let result = self.builder.ins().ushr(lhs, shifted);
        self.push(result);
        Ok(())
    }

    fn lower_land(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let result = self.builder.ins().band(lhs, rhs);
        self.push(result);
        Ok(())
    }

    fn lower_lor(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let result = self.builder.ins().bor(lhs, rhs);
        self.push(result);
        Ok(())
    }

    fn lower_lxor(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let result = self.builder.ins().bxor(lhs, rhs);
        self.push(result);
        Ok(())
    }

    fn initialize_local_vars(&mut self) -> Result<(), JitError> {
        let num_locals = self.method.max_locals;
        let entry_block = *self
            .block_map
            .get(&0)
            .expect("Entry block at PC 0 must exist");
        self.builder.switch_to_block(entry_block);
        self.builder
            .append_block_params_for_function_params(entry_block);

        let block_params: Vec<Value> = self.builder.block_params(entry_block).to_vec();

        self.context_var = self.builder.declare_var(types::I64);
        self.builder.def_var(self.context_var, block_params[0]);

        let mut initialized_locals = vec![false; num_locals];
        self.local_vars.clear();
        self.local_vars.reserve(num_locals);

        for i in 0..num_locals {
            let var_type = self.local_var_type(i);
            let var = self.builder.declare_var(var_type);
            self.local_vars.push(var);
        }

        let mut local_index = 0;
        let arg_types = self.arg_types.clone();
        for (arg_index, arg_type) in arg_types.iter().enumerate() {
            if local_index >= num_locals {
                break;
            }
            let block_param_index = arg_index + 1;
            if let Some(&param) = block_params.get(block_param_index) {
                let param = self.coerce_entry_param(param, *arg_type);
                let var = self.local_var(local_index)?;
                self.builder.def_var(var, param);
                initialized_locals[local_index] = true;
            }
            local_index += if matches!(arg_type, b'J' | b'D') {
                2
            } else {
                1
            };
        }

        for i in 0..num_locals {
            if !initialized_locals[i] {
                let zero = match self.local_var_type(i) {
                    types::I32 => self.builder.ins().iconst(types::I32, 0),
                    types::I64 => self.builder.ins().iconst(types::I64, 0),
                    types::F32 => self.builder.ins().f32const(0.0),
                    types::F64 => self.builder.ins().f64const(0.0),
                    _ => self.builder.ins().iconst(types::I64, 0),
                };
                let var = self.local_var(i)?;
                self.builder.def_var(var, zero);
            }
        }

        self.local_vars_initialized = true;
        Ok(())
    }

    fn coerce_entry_param(&mut self, raw: Value, arg_type: u8) -> Value {
        match arg_type {
            b'B' | b'C' | b'I' | b'S' | b'Z' => self.builder.ins().ireduce(types::I32, raw),
            b'F' => {
                let bits = self.builder.ins().ireduce(types::I32, raw);
                let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                self.builder.ins().stack_store(bits, slot, 0);
                self.builder.ins().stack_load(types::F32, slot, 0)
            }
            b'D' => {
                let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                self.builder.ins().stack_store(raw, slot, 0);
                self.builder.ins().stack_load(types::F64, slot, 0)
            }
            _ => raw,
        }
    }

    fn local_var_type(&self, index: usize) -> Type {
        let mut local_index = 0;
        for arg_type in &self.arg_types {
            if index == local_index {
                return match arg_type {
                    b'B' | b'C' | b'I' | b'S' | b'Z' => types::I32,
                    b'J' => types::I64,
                    b'F' => types::F32,
                    b'D' => types::F64,
                    b'L' | b'[' => types::I64,
                    _ => types::I64,
                };
            }
            if matches!(arg_type, b'J' | b'D') && index == local_index + 1 {
                return types::I64;
            }
            local_index += if matches!(arg_type, b'J' | b'D') {
                2
            } else {
                1
            };
        }

        if let Some(ty) = self.infer_local_type_from_stores(index) {
            return ty;
        }

        types::I64
    }

    fn infer_local_type_from_stores(&self, index: usize) -> Option<Type> {
        let mut pc = 0;
        while pc < self.method.code.len() {
            let opcode = self.method.code[pc];
            let hit = match opcode {
                0x36 => self
                    .method
                    .code
                    .get(pc + 1)
                    .copied()
                    .map(|i| (i as usize, types::I32)),
                0x37 => self
                    .method
                    .code
                    .get(pc + 1)
                    .copied()
                    .map(|i| (i as usize, types::I64)),
                0x38 => self
                    .method
                    .code
                    .get(pc + 1)
                    .copied()
                    .map(|i| (i as usize, types::F32)),
                0x39 => self
                    .method
                    .code
                    .get(pc + 1)
                    .copied()
                    .map(|i| (i as usize, types::F64)),
                0x3a => self
                    .method
                    .code
                    .get(pc + 1)
                    .copied()
                    .map(|i| (i as usize, types::I64)),
                0x3b..=0x3e => Some(((opcode - 0x3b) as usize, types::I32)),
                0x3f..=0x42 => Some(((opcode - 0x3f) as usize, types::I64)),
                0x43..=0x46 => Some(((opcode - 0x43) as usize, types::F32)),
                0x47..=0x4a => Some(((opcode - 0x47) as usize, types::F64)),
                0x4b..=0x4e => Some(((opcode - 0x4b) as usize, types::I64)),
                0x84 => self
                    .method
                    .code
                    .get(pc + 1)
                    .copied()
                    .map(|i| (i as usize, types::I32)),
                _ => None,
            };
            if let Some((local, ty)) = hit {
                if local == index {
                    return Some(ty);
                }
            }
            pc = self.next_pc(pc, opcode);
        }
        None
    }

    fn emit_deoptimization(&mut self, reason: &str) -> Result<Value, JitError> {
        let zero = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().trap(JIT_TRAP_CODE);
        Ok(zero)
    }

    fn emit_helper_call(
        &mut self,
        _helper_index: usize,
        _args: &[Value],
        _return_type: Type,
    ) -> Result<Value, JitError> {
        let zero = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().trap(JIT_TRAP_CODE);
        Ok(zero)
    }

    fn lower_iinc(&mut self) -> Result<(), JitError> {
        let index = self.method.code[self.pc_offset + 1] as usize;
        let increment = self.method.code[self.pc_offset + 2] as i8 as i32;
        let var = self.local_var(index)?;
        let current = self.builder.use_var(var);
        let inc_val = self.builder.ins().iconst(types::I32, increment as i64);
        let result = self.builder.ins().iadd(current, inc_val);
        self.builder.def_var(var, result);
        Ok(())
    }

    fn lower_i2l(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        let result = self.builder.ins().sextend(types::I64, val);
        self.push(result);
        Ok(())
    }

    fn lower_i2f(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        let result = self.builder.ins().fcvt_from_sint(types::F32, val);
        self.push(result);
        Ok(())
    }

    fn lower_i2d(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        let result = self.builder.ins().fcvt_from_sint(types::F64, val);
        self.push(result);
        Ok(())
    }

    fn lower_l2i(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        let result = self.builder.ins().ireduce(types::I32, val);
        self.push(result);
        Ok(())
    }

    fn lower_l2f(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        let result = self.builder.ins().fcvt_from_sint(types::F32, val);
        self.push(result);
        Ok(())
    }

    fn lower_l2d(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        let result = self.builder.ins().fcvt_from_sint(types::F64, val);
        self.push(result);
        Ok(())
    }

    fn lower_f2i(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        let result = self.builder.ins().fcvt_to_sint(types::I32, val);
        self.push(result);
        Ok(())
    }

    fn lower_f2l(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        let result = self.builder.ins().fcvt_to_sint(types::I64, val);
        self.push(result);
        Ok(())
    }

    fn lower_f2d(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        let result = self.builder.ins().fpromote(types::F64, val);
        self.push(result);
        Ok(())
    }

    fn lower_d2i(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        let result = self.builder.ins().fcvt_to_sint(types::I32, val);
        self.push(result);
        Ok(())
    }

    fn lower_d2l(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        let result = self.builder.ins().fcvt_to_sint(types::I64, val);
        self.push(result);
        Ok(())
    }

    fn lower_d2f(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        let result = self.builder.ins().fdemote(types::F32, val);
        self.push(result);
        Ok(())
    }

    fn lower_i2b(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        let mask = self.builder.ins().iconst(types::I64, 0xFF);
        let masked = {
            let band_result = self.builder.ins().band(val, mask);
            band_result
        };
        let shift_val = self.builder.ins().iconst(types::I64, 56);
        let shifted = {
            let shl_result = self.builder.ins().ishl(masked, shift_val);
            shl_result
        };
        let shift_back = self.builder.ins().iconst(types::I64, 56);
        let arith = self.builder.ins().sshr(shifted, shift_back);
        self.push(arith);
        Ok(())
    }

    fn lower_i2c(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        let result = self.builder.ins().uextend(types::I32, val);
        self.push(result);
        Ok(())
    }

    fn lower_i2s(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        let result = self.builder.ins().sextend(types::I32, val);
        self.push(result);
        Ok(())
    }

    fn lower_lcmp(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let cmp = self.builder.ins().icmp(IntCC::SignedLessThan, lhs, rhs);
        let result = self.builder.ins().uextend(types::I32, cmp);
        self.push(result);
        Ok(())
    }

    fn lower_fcmpl(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let cmp = self.builder.ins().fcmp(FloatCC::LessThan, lhs, rhs);
        let result = self.builder.ins().uextend(types::I32, cmp);
        self.push(result);
        Ok(())
    }

    fn lower_fcmpg(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let cmp = self.builder.ins().fcmp(FloatCC::GreaterThan, lhs, rhs);
        let result = self.builder.ins().uextend(types::I32, cmp);
        self.push(result);
        Ok(())
    }

    fn lower_dcmpl(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let cmp = self.builder.ins().fcmp(FloatCC::LessThan, lhs, rhs);
        let result = self.builder.ins().uextend(types::I32, cmp);
        self.push(result);
        Ok(())
    }

    fn lower_dcmpg(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let cmp = self.builder.ins().fcmp(FloatCC::GreaterThan, lhs, rhs);
        let result = self.builder.ins().uextend(types::I32, cmp);
        self.push(result);
        Ok(())
    }

    fn lower_if_icmp(&mut self, opcode: u8) -> Result<(), JitError> {
        let rhs;
        let lhs;
        if (0x99..=0x9e).contains(&opcode) {
            rhs = self.builder.ins().iconst(types::I32, 0);
            lhs = self.pop();
        } else {
            rhs = self.pop();
            lhs = self.pop();
        }

        let cond = match opcode {
            0x99 => IntCC::Equal,
            0x9a => IntCC::NotEqual,
            0x9b => IntCC::SignedLessThan,
            0x9c => IntCC::SignedGreaterThanOrEqual,
            0x9d => IntCC::SignedGreaterThan,
            0x9e => IntCC::SignedLessThanOrEqual,
            0x9f => IntCC::Equal,
            0xa0 => IntCC::NotEqual,
            0xa1 => IntCC::SignedLessThan,
            0xa2 => IntCC::SignedGreaterThanOrEqual,
            0xa3 => IntCC::SignedGreaterThan,
            0xa4 => IntCC::SignedLessThanOrEqual,
            0xa5 => IntCC::Equal,
            0xa6 => IntCC::NotEqual,
            _ => {
                return Err(JitError::CompilationFailed(format!(
                    "Invalid if_icmp opcode: 0x{:02x}",
                    opcode
                )));
            }
        };

        let cmp = self.builder.ins().icmp(cond, lhs, rhs);
        let target = self.read_branch_offset();
        let target_pc = (self.pc_offset as i32 + target as i32) as usize;
        let fallthrough_pc = self.next_pc(self.pc_offset, opcode);

        let target_block = self.create_block_for_pc(target_pc);
        let fallthrough_block = self.create_block_for_pc(fallthrough_pc);
        self.builder
            .ins()
            .brif(cmp, target_block, &[], fallthrough_block, &[]);
        Ok(())
    }

    fn lower_goto(&mut self) -> Result<(), JitError> {
        let target = self.read_branch_offset();
        let target_pc = (self.pc_offset as i32 + target as i32) as usize;

        let jump_block = self.create_block_for_pc(target_pc);
        self.builder.ins().jump(jump_block, &[]);
        Ok(())
    }

    fn lower_tableswitch(&mut self) -> Result<(), JitError> {
        let default_offset = self.read_i32(1);
        let low = self.read_i32(self.pc_offset + 4 + ((4 - self.pc_offset % 4) % 4) as usize);
        let high = self.read_i32(self.pc_offset + 8 + ((4 - self.pc_offset % 4) % 4) as usize);
        let index = self.pop();
        let default_target = (self.pc_offset as i32 + default_offset) as usize;
        let jump_block = self.create_block_for_pc(default_target);
        self.builder.ins().jump(jump_block, &[]);
        Ok(())
    }

    fn lower_lookupswitch(&mut self) -> Result<(), JitError> {
        let default_offset = self.read_i32(1);
        let default_target = (self.pc_offset as i32 + default_offset) as usize;
        let jump_block = self.create_block_for_pc(default_target);
        self.builder.ins().jump(jump_block, &[]);
        Ok(())
    }

    fn read_i32(&self, offset: usize) -> i32 {
        let bytes = &self.method.code[offset..offset + 4];
        ((bytes[0] as i32) << 24)
            | ((bytes[1] as i32) << 16)
            | ((bytes[2] as i32) << 8)
            | (bytes[3] as i32)
    }

    fn create_block_for_pc(&mut self, pc: usize) -> Block {
        if let Some(&block) = self.block_map.get(&pc) {
            return block;
        }
        let block = self.builder.create_block();
        self.block_map.insert(pc, block);
        block
    }

    fn seal_block(&mut self, block: Block) {
        self.builder.seal_block(block);
    }

    fn read_branch_offset(&self) -> i32 {
        let high = self.method.code[self.pc_offset + 1] as i16;
        let low = self.method.code[self.pc_offset + 2] as u16;
        i16::from_be_bytes([high as u8, low as u8]) as i32
    }

    fn lower_ireturn(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        self.builder.ins().return_(&[val]);
        Ok(())
    }

    fn lower_lreturn(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        self.builder.ins().return_(&[val]);
        Ok(())
    }

    fn lower_freturn(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        self.builder.ins().return_(&[val]);
        Ok(())
    }

    fn lower_dreturn(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        self.builder.ins().return_(&[val]);
        Ok(())
    }

    fn lower_areturn(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        self.builder.ins().return_(&[val]);
        Ok(())
    }

    fn lower_return(&mut self) -> Result<(), JitError> {
        self.builder.ins().return_(&[]);
        Ok(())
    }

    fn lower_getstatic(&mut self) -> Result<(), JitError> {
        let cp_index = ((self.method.code[self.pc_offset + 1] as usize) << 8)
            | (self.method.code[self.pc_offset + 2] as usize);
        let field_ref = self
            .method
            .field_refs
            .get(cp_index)
            .and_then(|f| f.as_ref())
            .cloned()
            .ok_or_else(|| {
                JitError::CompilationFailed(format!("Invalid field ref index: {}", cp_index))
            })?;
        let field_desc = field_ref.descriptor.clone();
        self.stack_slots.push(StackSlot {
            size: 8,
            offset: cp_index as i32,
        });
        let field_value = self.load_static_field(field_ref, &field_desc)?;
        self.push(field_value);
        Ok(())
    }

    fn load_static_field(
        &mut self,
        field_ref: crate::vm::types::FieldRef,
        field_desc: &str,
    ) -> Result<Value, JitError> {
        let field_ref_id = crate::vm::jit::runtime::register_field_ref(field_ref);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let field_ref_id = self.builder.ins().iconst(types::I64, field_ref_id as i64);
        let raw = self.emit_field_helper_call(
            crate::vm::jit::runtime::get_get_static_field_ptr(),
            [field_ref_id, zero, zero, zero, zero],
        )?;
        let value = self.coerce_raw_field_result(raw, field_desc)?;
        Ok(value)
    }

    fn lower_putstatic(&mut self) -> Result<(), JitError> {
        let cp_index = ((self.method.code[self.pc_offset + 1] as usize) << 8)
            | (self.method.code[self.pc_offset + 2] as usize);
        let field_ref = self
            .method
            .field_refs
            .get(cp_index)
            .and_then(|f| f.as_ref())
            .cloned()
            .ok_or_else(|| {
                JitError::CompilationFailed(format!("Invalid field ref index: {}", cp_index))
            })?;
        let field_desc = field_ref.descriptor.clone();
        let value = self.pop();
        self.stack_slots.push(StackSlot {
            size: 8,
            offset: cp_index as i32,
        });
        self.store_static_field(field_ref, &field_desc, value)?;
        Ok(())
    }

    fn store_static_field(
        &mut self,
        field_ref: crate::vm::types::FieldRef,
        _field_desc: &str,
        value: Value,
    ) -> Result<(), JitError> {
        let field_ref_id = crate::vm::jit::runtime::register_field_ref(field_ref);
        let raw_value = self.coerce_helper_arg(value);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let field_ref_id = self.builder.ins().iconst(types::I64, field_ref_id as i64);
        self.emit_field_helper_call(
            crate::vm::jit::runtime::get_put_static_field_ptr(),
            [field_ref_id, raw_value, zero, zero, zero],
        )?;
        Ok(())
    }

    fn lower_getfield(&mut self) -> Result<(), JitError> {
        let cp_index = ((self.method.code[self.pc_offset + 1] as usize) << 8)
            | (self.method.code[self.pc_offset + 2] as usize);
        let field_ref = self
            .method
            .field_refs
            .get(cp_index)
            .and_then(|f| f.as_ref())
            .cloned()
            .ok_or_else(|| {
                JitError::CompilationFailed(format!("Invalid field ref index: {}", cp_index))
            })?;
        let field_desc = field_ref.descriptor.clone();
        let obj = self.pop();
        self.builder.ins().trapz(obj, JIT_TRAP_CODE);
        self.stack_slots.push(StackSlot {
            size: 8,
            offset: cp_index as i32,
        });
        let field_value = self.load_instance_field(obj, field_ref, &field_desc)?;
        self.push(field_value);
        Ok(())
    }

    fn load_instance_field(
        &mut self,
        obj: Value,
        field_ref: crate::vm::types::FieldRef,
        field_desc: &str,
    ) -> Result<Value, JitError> {
        let field_ref_id = crate::vm::jit::runtime::register_field_ref(field_ref);
        let obj = self.coerce_helper_arg(obj);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let field_ref_id = self.builder.ins().iconst(types::I64, field_ref_id as i64);
        let raw = self.emit_field_helper_call(
            crate::vm::jit::runtime::get_get_instance_field_ptr(),
            [obj, field_ref_id, zero, zero, zero],
        )?;
        self.coerce_raw_field_result(raw, field_desc)
    }

    fn lower_putfield(&mut self) -> Result<(), JitError> {
        let cp_index = ((self.method.code[self.pc_offset + 1] as usize) << 8)
            | (self.method.code[self.pc_offset + 2] as usize);
        let field_ref = self
            .method
            .field_refs
            .get(cp_index)
            .and_then(|f| f.as_ref())
            .cloned()
            .ok_or_else(|| {
                JitError::CompilationFailed(format!("Invalid field ref index: {}", cp_index))
            })?;
        let field_desc = field_ref.descriptor.clone();
        let value = self.pop();
        let obj = self.pop();
        self.builder.ins().trapz(obj, JIT_TRAP_CODE);
        self.stack_slots.push(StackSlot {
            size: 8,
            offset: cp_index as i32,
        });
        self.store_instance_field(obj, value, field_ref, &field_desc)?;
        Ok(())
    }

    fn store_instance_field(
        &mut self,
        obj: Value,
        value: Value,
        field_ref: crate::vm::types::FieldRef,
        _field_desc: &str,
    ) -> Result<(), JitError> {
        let field_ref_id = crate::vm::jit::runtime::register_field_ref(field_ref);
        let obj = self.coerce_helper_arg(obj);
        let raw_value = self.coerce_helper_arg(value);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let field_ref_id = self.builder.ins().iconst(types::I64, field_ref_id as i64);
        self.emit_field_helper_call(
            crate::vm::jit::runtime::get_put_instance_field_ptr(),
            [obj, field_ref_id, raw_value, zero, zero],
        )?;
        Ok(())
    }

    fn lower_invokevirtual(&mut self) -> Result<(), JitError> {
        let cp_index = ((self.method.code[self.pc_offset + 1] as usize) << 8)
            | (self.method.code[self.pc_offset + 2] as usize);
        let method_ref = self
            .method
            .method_refs
            .get(cp_index)
            .and_then(|m| m.as_ref())
            .ok_or_else(|| {
                JitError::CompilationFailed(format!("Invalid method ref index: {}", cp_index))
            })?;
        let method_desc = method_ref.descriptor.clone();
        let argc = crate::vm::types::parse_arg_count(&method_desc).map_err(|_| {
            JitError::CompilationFailed(format!("Invalid method descriptor: {}", method_desc))
        })?;
        let mut args = Vec::with_capacity(argc);
        for _ in 0..argc {
            args.push(self.pop());
        }
        args.reverse();
        let this_ref = self.pop();
        self.builder.ins().trapz(this_ref, JIT_TRAP_CODE);
        self.stack_slots.push(StackSlot {
            size: 8,
            offset: cp_index as i32,
        });
        self.guard_checks.push(GuardCheck {
            pc: self.pc_offset,
            guard_type: GuardType::NotNull,
        });
        let result = self.emit_invoke_virtual(method_ref.clone(), this_ref, &method_desc, args)?;
        if result {
            let raw = self.pop();
            let coerced = self.coerce_raw_helper_result(raw, &method_desc)?;
            self.push(coerced);
        }
        Ok(())
    }

    fn emit_invoke_virtual(
        &mut self,
        method_ref: crate::vm::types::MethodRef,
        receiver: Value,
        method_desc: &str,
        args: Vec<Value>,
    ) -> Result<bool, JitError> {
        self.emit_instance_invoke_helper(
            crate::vm::jit::runtime::get_invoke_virtual_ptr(),
            method_ref,
            receiver,
            method_desc,
            args,
            "invokevirtual",
        )
    }

    fn lower_invokespecial(&mut self) -> Result<(), JitError> {
        let cp_index = ((self.method.code[self.pc_offset + 1] as usize) << 8)
            | (self.method.code[self.pc_offset + 2] as usize);
        let method_ref = self
            .method
            .method_refs
            .get(cp_index)
            .and_then(|m| m.as_ref())
            .ok_or_else(|| {
                JitError::CompilationFailed(format!("Invalid method ref index: {}", cp_index))
            })?;
        let method_desc = method_ref.descriptor.clone();
        let argc = crate::vm::types::parse_arg_count(&method_desc).map_err(|_| {
            JitError::CompilationFailed(format!("Invalid method descriptor: {}", method_desc))
        })?;
        let mut args = Vec::with_capacity(argc);
        for _ in 0..argc {
            args.push(self.pop());
        }
        args.reverse();
        let this_ref = self.pop();
        self.builder.ins().trapz(this_ref, JIT_TRAP_CODE);
        self.stack_slots.push(StackSlot {
            size: 8,
            offset: cp_index as i32,
        });
        self.guard_checks.push(GuardCheck {
            pc: self.pc_offset,
            guard_type: GuardType::NotNull,
        });
        let result = self.emit_invoke_special(method_ref.clone(), this_ref, &method_desc, args)?;
        if result {
            let raw = self.pop();
            let coerced = self.coerce_raw_helper_result(raw, &method_desc)?;
            self.push(coerced);
        }
        Ok(())
    }

    fn emit_invoke_special(
        &mut self,
        method_ref: crate::vm::types::MethodRef,
        receiver: Value,
        method_desc: &str,
        args: Vec<Value>,
    ) -> Result<bool, JitError> {
        self.emit_instance_invoke_helper(
            crate::vm::jit::runtime::get_invoke_special_ptr(),
            method_ref,
            receiver,
            method_desc,
            args,
            "invokespecial",
        )
    }

    fn lower_invokestatic(&mut self) -> Result<(), JitError> {
        let cp_index = ((self.method.code[self.pc_offset + 1] as usize) << 8)
            | (self.method.code[self.pc_offset + 2] as usize);
        let method_ref = self
            .method
            .method_refs
            .get(cp_index)
            .and_then(|m| m.as_ref())
            .ok_or_else(|| {
                JitError::CompilationFailed(format!("Invalid method ref index: {}", cp_index))
            })?;
        let method_desc = method_ref.descriptor.clone();
        let argc = crate::vm::types::parse_arg_count(&method_desc).map_err(|_| {
            JitError::CompilationFailed(format!("Invalid method descriptor: {}", method_desc))
        })?;
        let mut args = Vec::with_capacity(argc);
        for _ in 0..argc {
            args.push(self.pop());
        }
        args.reverse();
        self.stack_slots.push(StackSlot {
            size: 8,
            offset: cp_index as i32,
        });
        let result = self.emit_invoke_static(method_ref.clone(), &method_desc, args)?;
        if result {
            let raw = self.pop();
            let coerced = self.coerce_raw_helper_result(raw, &method_desc)?;
            self.push(coerced);
        }
        Ok(())
    }

    fn emit_invoke_static(
        &mut self,
        method_ref: crate::vm::types::MethodRef,
        method_desc: &str,
        args: Vec<Value>,
    ) -> Result<bool, JitError> {
        let method_ref_id = crate::vm::jit::runtime::register_method_ref(method_ref);
        let (_argc_len, argc, arg0, arg1, arg2) = self.emit_helper_args(args, 3);
        let mut sig = Signature::new(self.builder.func.signature.call_conv);
        for _ in 0..6 {
            sig.params.push(AbiParam::new(types::I64));
        }
        sig.returns.push(AbiParam::new(types::I64));
        let sigref = self.builder.import_signature(sig);

        let helper = self.builder.ins().iconst(
            types::I64,
            crate::vm::jit::runtime::get_invoke_static_ptr() as i64,
        );
        let ctx = self.builder.ins().iconst(types::I64, 0);
        let method_ref_id = self.builder.ins().iconst(types::I64, method_ref_id as i64);
        let call_args = vec![ctx, method_ref_id, argc, arg0, arg1, arg2];

        let call = self.builder.ins().call_indirect(sigref, helper, &call_args);
        let result = self.builder.inst_results(call)[0];

        if crate::vm::types::parse_return_type(method_desc)
            .map_err(|e| JitError::CompilationFailed(format!("Invalid descriptor: {}", e)))?
            .is_some_and(|ret| ret != b'V')
        {
            self.push(result);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn emit_instance_invoke_helper(
        &mut self,
        helper_ptr: u64,
        method_ref: crate::vm::types::MethodRef,
        receiver: Value,
        method_desc: &str,
        args: Vec<Value>,
        _opcode_name: &str,
    ) -> Result<bool, JitError> {
        let method_ref_id = crate::vm::jit::runtime::register_method_ref(method_ref);
        let (_argc_len, argc, arg0, arg1, _) = self.emit_helper_args(args, 2);
        let mut sig = Signature::new(self.builder.func.signature.call_conv);
        for _ in 0..6 {
            sig.params.push(AbiParam::new(types::I64));
        }
        sig.returns.push(AbiParam::new(types::I64));
        let sigref = self.builder.import_signature(sig);

        let helper = self.builder.ins().iconst(types::I64, helper_ptr as i64);
        let ctx = self.builder.ins().iconst(types::I64, 0);
        let receiver = self.coerce_helper_arg(receiver);
        let method_ref_id = self.builder.ins().iconst(types::I64, method_ref_id as i64);
        let call_args = vec![ctx, receiver, method_ref_id, argc, arg0, arg1];

        let call = self.builder.ins().call_indirect(sigref, helper, &call_args);
        let result = self.builder.inst_results(call)[0];

        if crate::vm::types::parse_return_type(method_desc)
            .map_err(|e| JitError::CompilationFailed(format!("Invalid descriptor: {}", e)))?
            .is_some_and(|ret| ret != b'V')
        {
            self.push(result);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn emit_arg_buffer(&mut self, args: Vec<Value>) -> Value {
        if args.is_empty() {
            return self.builder.ins().iconst(types::I64, 0);
        }

        let size = (args.len() * 8) as u32;
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            size,
            3,
        ));
        for (index, arg) in args.into_iter().enumerate() {
            let stored = self.coerce_helper_arg(arg);
            self.builder
                .ins()
                .stack_store(stored, slot, (index * 8) as i32);
        }
        self.builder.ins().stack_addr(types::I64, slot, 0)
    }

    fn emit_helper_args(
        &mut self,
        args: Vec<Value>,
        inline_limit: usize,
    ) -> (usize, Value, Value, Value, Value) {
        const INLINE_ARG_MARKER: u64 = 1u64 << 63;
        let argc_len = args.len();
        if argc_len <= inline_limit {
            let argc = self
                .builder
                .ins()
                .iconst(types::I64, (INLINE_ARG_MARKER | argc_len as u64) as i64);
            let zero = self.builder.ins().iconst(types::I64, 0);
            let mut raw = Vec::with_capacity(3);
            for arg in args {
                raw.push(self.coerce_helper_arg(arg));
            }
            while raw.len() < 3 {
                raw.push(zero);
            }
            (argc_len, argc, raw[0], raw[1], raw[2])
        } else {
            let args_ptr = self.emit_arg_buffer(args);
            let argc = self.builder.ins().iconst(types::I64, argc_len as i64);
            let zero = self.builder.ins().iconst(types::I64, 0);
            (argc_len, argc, args_ptr, zero, zero)
        }
    }

    fn emit_field_helper_call(
        &mut self,
        helper_ptr: u64,
        args: [Value; 5],
    ) -> Result<Value, JitError> {
        let mut sig = Signature::new(self.builder.func.signature.call_conv);
        for _ in 0..6 {
            sig.params.push(AbiParam::new(types::I64));
        }
        sig.returns.push(AbiParam::new(types::I64));
        let sigref = self.builder.import_signature(sig);

        let helper = self.builder.ins().iconst(types::I64, helper_ptr as i64);
        let ctx = self.builder.ins().iconst(types::I64, 0);
        let call_args = [
            ctx,
            self.coerce_helper_arg(args[0]),
            self.coerce_helper_arg(args[1]),
            self.coerce_helper_arg(args[2]),
            self.coerce_helper_arg(args[3]),
            self.coerce_helper_arg(args[4]),
        ];
        let call = self.builder.ins().call_indirect(sigref, helper, &call_args);
        Ok(self.builder.inst_results(call)[0])
    }

    fn emit_typed_array_store(
        &mut self,
        array_ref: Value,
        index: Value,
        value: Value,
    ) -> Result<(), JitError> {
        let value = self.coerce_helper_arg(value);
        let zero = self.builder.ins().iconst(types::I64, 0);
        self.emit_field_helper_call(
            crate::vm::jit::runtime::get_store_typed_array_element_ptr(),
            [array_ref, index, value, zero, zero],
        )?;
        Ok(())
    }

    fn coerce_helper_arg(&mut self, value: Value) -> Value {
        match self.builder.func.dfg.value_type(value) {
            types::I64 => value,
            types::I32 | types::I16 | types::I8 => self.builder.ins().sextend(types::I64, value),
            types::F32 => {
                let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                self.builder.ins().stack_store(value, slot, 0);
                let bits = self.builder.ins().stack_load(types::I32, slot, 0);
                self.builder.ins().uextend(types::I64, bits)
            }
            types::F64 => {
                let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                self.builder.ins().stack_store(value, slot, 0);
                self.builder.ins().stack_load(types::I64, slot, 0)
            }
            _ => value,
        }
    }

    fn coerce_raw_field_result(&mut self, raw: Value, descriptor: &str) -> Result<Value, JitError> {
        let value = match descriptor.as_bytes().first() {
            Some(b'B' | b'C' | b'I' | b'S' | b'Z') => self.builder.ins().ireduce(types::I32, raw),
            Some(b'J') | Some(b'L') | Some(b'[') => raw,
            Some(b'F') => {
                let bits = self.builder.ins().ireduce(types::I32, raw);
                let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                self.builder.ins().stack_store(bits, slot, 0);
                self.builder.ins().stack_load(types::F32, slot, 0)
            }
            Some(b'D') => {
                let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                self.builder.ins().stack_store(raw, slot, 0);
                self.builder.ins().stack_load(types::F64, slot, 0)
            }
            Some(other) => {
                return Err(JitError::CompilationFailed(format!(
                    "Invalid field descriptor: {}",
                    *other as char
                )));
            }
            None => {
                return Err(JitError::CompilationFailed(
                    "Invalid empty field descriptor".to_string(),
                ));
            }
        };
        Ok(value)
    }

    fn coerce_raw_helper_result(
        &mut self,
        raw: Value,
        descriptor: &str,
    ) -> Result<Value, JitError> {
        let ret = crate::vm::types::parse_return_type(descriptor)
            .map_err(|e| JitError::CompilationFailed(format!("Invalid descriptor: {}", e)))?;
        let value = match ret {
            Some(b'B' | b'C' | b'I' | b'S' | b'Z') => self.builder.ins().ireduce(types::I32, raw),
            Some(b'J') | Some(b'L') | Some(b'[') => raw,
            Some(b'F') => {
                let bits = self.builder.ins().ireduce(types::I32, raw);
                let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                self.builder.ins().stack_store(bits, slot, 0);
                self.builder.ins().stack_load(types::F32, slot, 0)
            }
            Some(b'D') => {
                let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                self.builder.ins().stack_store(raw, slot, 0);
                self.builder.ins().stack_load(types::F64, slot, 0)
            }
            Some(_) => raw,
            None => raw,
        };
        Ok(value)
    }

    fn lower_invokeinterface(&mut self) -> Result<(), JitError> {
        let cp_index = ((self.method.code[self.pc_offset + 1] as usize) << 8)
            | (self.method.code[self.pc_offset + 2] as usize);
        let method_ref = self
            .method
            .method_refs
            .get(cp_index)
            .and_then(|m| m.as_ref())
            .ok_or_else(|| {
                JitError::CompilationFailed(format!("Invalid method ref index: {}", cp_index))
            })?;
        let method_desc = method_ref.descriptor.clone();
        let argc = crate::vm::types::parse_arg_count(&method_desc).map_err(|_| {
            JitError::CompilationFailed(format!("Invalid method descriptor: {}", method_desc))
        })?;
        let mut args = Vec::with_capacity(argc);
        for _ in 0..argc {
            args.push(self.pop());
        }
        args.reverse();
        let this_ref = self.pop();
        self.builder.ins().trapz(this_ref, JIT_TRAP_CODE);
        self.stack_slots.push(StackSlot {
            size: 8,
            offset: cp_index as i32,
        });
        self.guard_checks.push(GuardCheck {
            pc: self.pc_offset,
            guard_type: GuardType::NotNull,
        });
        let result =
            self.emit_invoke_interface(method_ref.clone(), this_ref, &method_desc, args)?;
        if result {
            let raw = self.pop();
            let coerced = self.coerce_raw_helper_result(raw, &method_desc)?;
            self.push(coerced);
        }
        Ok(())
    }

    fn emit_invoke_interface(
        &mut self,
        method_ref: crate::vm::types::MethodRef,
        receiver: Value,
        method_desc: &str,
        args: Vec<Value>,
    ) -> Result<bool, JitError> {
        self.emit_instance_invoke_helper(
            crate::vm::jit::runtime::get_invoke_interface_ptr(),
            method_ref,
            receiver,
            method_desc,
            args,
            "invokeinterface",
        )
    }

    fn lower_invokedynamic(&mut self) -> Result<(), JitError> {
        let cp_index = ((self.method.code[self.pc_offset + 1] as usize) << 8)
            | (self.method.code[self.pc_offset + 2] as usize);
        let site = self
            .method
            .invoke_dynamic_sites
            .get(cp_index)
            .and_then(|s| s.as_ref())
            .cloned()
            .ok_or_else(|| {
                JitError::CompilationFailed(format!(
                    "Invalid invokedynamic site index: {}",
                    cp_index
                ))
            })?;
        let arg_count = crate::vm::types::parse_arg_count(&site.descriptor).map_err(|_| {
            JitError::CompilationFailed(format!(
                "Invalid invokedynamic descriptor: {}",
                site.descriptor
            ))
        })?;
        let mut args = Vec::with_capacity(arg_count);
        for _ in 0..arg_count {
            args.push(self.pop());
        }
        args.reverse();
        self.stack_slots.push(StackSlot {
            size: 8,
            offset: cp_index as i32,
        });
        let descriptor = site.descriptor.clone();
        let result = self.emit_invokedynamic(site, args)?;
        if result {
            let raw = self.pop();
            let coerced = self.coerce_raw_helper_result(raw, &descriptor)?;
            self.push(coerced);
        }
        Ok(())
    }

    fn emit_invokedynamic(
        &mut self,
        site: crate::vm::types::InvokeDynamicSite,
        args: Vec<Value>,
    ) -> Result<bool, JitError> {
        let descriptor = site.descriptor.clone();
        let site_id = crate::vm::jit::runtime::register_invoke_dynamic_site(site);
        let (_argc_len, argc, arg0, arg1, arg2) = self.emit_helper_args(args, 3);
        let mut sig = Signature::new(self.builder.func.signature.call_conv);
        for _ in 0..6 {
            sig.params.push(AbiParam::new(types::I64));
        }
        sig.returns.push(AbiParam::new(types::I64));
        let sigref = self.builder.import_signature(sig);

        let helper = self.builder.ins().iconst(
            types::I64,
            crate::vm::jit::runtime::get_invoke_dynamic_ptr() as i64,
        );
        let ctx = self.builder.ins().iconst(types::I64, 0);
        let site_id = self.builder.ins().iconst(types::I64, site_id as i64);
        let call_args = vec![ctx, site_id, argc, arg0, arg1, arg2];

        let call = self.builder.ins().call_indirect(sigref, helper, &call_args);
        let result = self.builder.inst_results(call)[0];

        if crate::vm::types::parse_return_type(&descriptor)
            .map_err(|e| JitError::CompilationFailed(format!("Invalid descriptor: {}", e)))?
            .is_some_and(|ret| ret != b'V')
        {
            self.push(result);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn lower_new(&mut self) -> Result<(), JitError> {
        let cp_index = ((self.method.code[self.pc_offset + 1] as usize) << 8)
            | (self.method.code[self.pc_offset + 2] as usize);
        let class_name = self
            .method
            .reference_classes
            .get(cp_index)
            .and_then(|c| c.as_ref())
            .cloned()
            .ok_or_else(|| {
                JitError::CompilationFailed(format!("Invalid reference class index: {}", cp_index))
            })?;
        self.stack_slots.push(StackSlot {
            size: 8,
            offset: cp_index as i32,
        });
        let obj_ref = self.allocate_object(&class_name)?;
        self.push(obj_ref);
        Ok(())
    }

    fn allocate_object(&mut self, class_name: &str) -> Result<Value, JitError> {
        let class_id = crate::vm::jit::runtime::register_class_name(class_name.to_string());
        let class_id = self.builder.ins().iconst(types::I64, class_id as i64);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let obj_ref = self.emit_field_helper_call(
            crate::vm::jit::runtime::get_allocate_object_ptr(),
            [class_id, zero, zero, zero, zero],
        )?;
        self.builder.ins().trapz(obj_ref, JIT_TRAP_CODE);
        Ok(obj_ref)
    }

    fn lower_newarray(&mut self) -> Result<(), JitError> {
        let array_type = self.method.code[self.pc_offset + 1] as usize;
        let count = self.pop();
        self.stack_slots.push(StackSlot {
            size: 8,
            offset: array_type as i32,
        });
        let array_ref = self.allocate_array(1, array_type as u64, vec![count])?;
        self.push(array_ref);
        Ok(())
    }

    fn lower_anewarray(&mut self) -> Result<(), JitError> {
        let cp_index = ((self.method.code[self.pc_offset + 1] as usize) << 8)
            | (self.method.code[self.pc_offset + 2] as usize);
        let component_type = self
            .method
            .reference_classes
            .get(cp_index)
            .and_then(|c| c.as_ref())
            .cloned()
            .ok_or_else(|| {
                JitError::CompilationFailed(format!("Invalid reference class index: {}", cp_index))
            })?;
        let count = self.pop();
        self.stack_slots.push(StackSlot {
            size: 8,
            offset: cp_index as i32,
        });
        let descriptor_id = crate::vm::jit::runtime::register_array_descriptor(component_type);
        let array_ref = self.allocate_array(2, descriptor_id, vec![count])?;
        self.push(array_ref);
        Ok(())
    }

    fn lower_multianewarray(&mut self) -> Result<(), JitError> {
        let cp_index = ((self.method.code[self.pc_offset + 1] as usize) << 8)
            | (self.method.code[self.pc_offset + 2] as usize);
        let descriptor = self
            .method
            .reference_classes
            .get(cp_index)
            .and_then(|c| c.as_ref())
            .cloned()
            .ok_or_else(|| {
                JitError::CompilationFailed(format!("Invalid reference class index: {}", cp_index))
            })?;
        let dimensions = self.method.code[self.pc_offset + 3] as usize;
        let mut counts = Vec::with_capacity(dimensions);
        for _ in 0..dimensions {
            counts.push(self.pop());
        }
        counts.reverse();
        self.stack_slots.push(StackSlot {
            size: 8,
            offset: cp_index as i32,
        });
        let descriptor_id = crate::vm::jit::runtime::register_array_descriptor(descriptor);
        let array_ref = self.allocate_array(3, descriptor_id, counts)?;
        self.push(array_ref);
        Ok(())
    }

    fn allocate_array(
        &mut self,
        kind: u64,
        descriptor_or_atype: u64,
        counts: Vec<Value>,
    ) -> Result<Value, JitError> {
        let (argc, arg0, arg1) = self.emit_array_count_args(counts);
        let kind = self.builder.ins().iconst(types::I64, kind as i64);
        let descriptor_or_atype = self
            .builder
            .ins()
            .iconst(types::I64, descriptor_or_atype as i64);
        self.emit_field_helper_call(
            crate::vm::jit::runtime::get_allocate_array_ptr(),
            [kind, descriptor_or_atype, argc, arg0, arg1],
        )
    }

    fn emit_array_count_args(&mut self, counts: Vec<Value>) -> (Value, Value, Value) {
        const INLINE_ARG_MARKER: u64 = 1u64 << 63;
        if counts.len() <= 2 {
            let argc = self
                .builder
                .ins()
                .iconst(types::I64, (INLINE_ARG_MARKER | counts.len() as u64) as i64);
            let zero = self.builder.ins().iconst(types::I64, 0);
            let mut raw = Vec::with_capacity(2);
            for count in counts {
                raw.push(self.coerce_helper_arg(count));
            }
            while raw.len() < 2 {
                raw.push(zero);
            }
            (argc, raw[0], raw[1])
        } else {
            let count_len = counts.len();
            let args_ptr = self.emit_arg_buffer(counts);
            let argc = self.builder.ins().iconst(types::I64, count_len as i64);
            (argc, args_ptr, self.builder.ins().iconst(types::I64, 0))
        }
    }

    fn lower_arraylength(&mut self) -> Result<(), JitError> {
        let array_ref = self.pop();
        let zero = self.builder.ins().iconst(types::I64, 0);
        let raw = self.emit_field_helper_call(
            crate::vm::jit::runtime::get_array_length_ptr(),
            [array_ref, zero, zero, zero, zero],
        )?;
        let len = self.builder.ins().ireduce(types::I32, raw);
        self.push(len);
        Ok(())
    }
    fn lower_athrow(&mut self) -> Result<(), JitError> {
        let exception_ref = self.pop();
        self.builder.ins().trapz(exception_ref, JIT_TRAP_CODE);
        self.emit_throw(exception_ref)?;
        Ok(())
    }

    fn emit_throw(&mut self, exception_ref: Value) -> Result<(), JitError> {
        let zero = self.builder.ins().iconst(types::I64, 0);
        self.emit_field_helper_call(
            crate::vm::jit::runtime::get_athrow_ptr(),
            [exception_ref, zero, zero, zero, zero],
        )?;
        self.emit_default_return();
        Ok(())
    }

    fn emit_default_return(&mut self) {
        let Some(ret) = self
            .builder
            .func
            .signature
            .returns
            .first()
            .map(|param| param.value_type)
        else {
            self.builder.ins().return_(&[]);
            return;
        };

        let value = match ret {
            types::I32 => self.builder.ins().iconst(types::I32, 0),
            types::I64 => self.builder.ins().iconst(types::I64, 0),
            types::F32 => self.builder.ins().f32const(0.0),
            types::F64 => self.builder.ins().f64const(0.0),
            _ => self.builder.ins().iconst(ret, 0),
        };
        self.builder.ins().return_(&[value]);
    }

    fn lower_checkcast(&mut self) -> Result<(), JitError> {
        let cp_index = ((self.method.code[self.pc_offset + 1] as usize) << 8)
            | (self.method.code[self.pc_offset + 2] as usize);
        let target_class = self
            .method
            .reference_classes
            .get(cp_index)
            .and_then(|c| c.as_ref())
            .cloned()
            .ok_or_else(|| {
                JitError::CompilationFailed(format!("Invalid reference class index: {}", cp_index))
            })?;
        let obj_ref = self.pop();
        self.builder.ins().trapz(obj_ref, JIT_TRAP_CODE);
        self.stack_slots.push(StackSlot {
            size: 8,
            offset: cp_index as i32,
        });
        self.guard_checks.push(GuardCheck {
            pc: self.pc_offset,
            guard_type: GuardType::TypeCheck(target_class.clone()),
        });
        let class_id = crate::vm::jit::runtime::register_class_name(target_class);
        let class_id = self.builder.ins().iconst(types::I64, class_id as i64);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let ok = self.emit_field_helper_call(
            crate::vm::jit::runtime::get_checkcast_ptr(),
            [obj_ref, class_id, zero, zero, zero],
        )?;
        self.builder.ins().trapz(ok, JIT_TRAP_CODE);
        self.push(obj_ref);
        Ok(())
    }

    fn lower_instanceof(&mut self) -> Result<(), JitError> {
        let cp_index = ((self.method.code[self.pc_offset + 1] as usize) << 8)
            | (self.method.code[self.pc_offset + 2] as usize);
        let _target_class = self
            .method
            .reference_classes
            .get(cp_index)
            .and_then(|c| c.as_ref())
            .cloned()
            .ok_or_else(|| {
                JitError::CompilationFailed(format!("Invalid reference class index: {}", cp_index))
            })?;
        let obj_ref = self.pop();
        let is_null = self.builder.ins().icmp_imm(IntCC::Equal, obj_ref, 0);
        let result = self.builder.ins().icmp_imm(IntCC::Equal, is_null, 0);
        self.push(result);
        self.stack_slots.push(StackSlot {
            size: 4,
            offset: cp_index as i32,
        });
        Ok(())
    }

    fn lower_monitorenter(&mut self) -> Result<(), JitError> {
        let obj_ref = self.pop();
        self.builder.ins().trapz(obj_ref, JIT_TRAP_CODE);
        self.stack_slots.push(StackSlot {
            size: 8,
            offset: -1,
        });
        self.emit_monitor_enter(obj_ref)?;
        Ok(())
    }

    fn emit_monitor_enter(&mut self, obj_ref: Value) -> Result<(), JitError> {
        let zero = self.builder.ins().iconst(types::I64, 0);
        let ok = self.emit_field_helper_call(
            crate::vm::jit::runtime::get_monitor_enter_ptr(),
            [obj_ref, zero, zero, zero, zero],
        )?;
        self.builder.ins().trapz(ok, JIT_TRAP_CODE);
        Ok(())
    }

    fn lower_monitorexit(&mut self) -> Result<(), JitError> {
        let obj_ref = self.pop();
        self.builder.ins().trapz(obj_ref, JIT_TRAP_CODE);
        self.stack_slots.push(StackSlot {
            size: 8,
            offset: -1,
        });
        self.emit_monitor_exit(obj_ref)?;
        Ok(())
    }

    fn emit_monitor_exit(&mut self, obj_ref: Value) -> Result<(), JitError> {
        let zero = self.builder.ins().iconst(types::I64, 0);
        let ok = self.emit_field_helper_call(
            crate::vm::jit::runtime::get_monitor_exit_ptr(),
            [obj_ref, zero, zero, zero, zero],
        )?;
        self.builder.ins().trapz(ok, JIT_TRAP_CODE);
        Ok(())
    }

    fn lower_invokenative(&mut self) -> Result<(), JitError> {
        let cp_index = ((self.method.code[self.pc_offset + 1] as usize) << 8)
            | (self.method.code[self.pc_offset + 2] as usize);
        let method_ref = self
            .method
            .method_refs
            .get(cp_index)
            .and_then(|m| m.as_ref())
            .ok_or_else(|| {
                JitError::CompilationFailed(format!("Invalid method ref index: {}", cp_index))
            })?;
        let method_desc = method_ref.descriptor.clone();
        let argc = crate::vm::types::parse_arg_count(&method_desc).map_err(|_| {
            JitError::CompilationFailed(format!("Invalid method descriptor: {}", method_desc))
        })?;
        let mut args = Vec::with_capacity(argc);
        for _ in 0..argc {
            args.push(self.pop());
        }
        args.reverse();
        self.stack_slots.push(StackSlot {
            size: 8,
            offset: cp_index as i32,
        });
        let result = self.emit_invoke_native(method_ref.clone(), &method_desc, args)?;
        if result {
            let raw = self.pop();
            let coerced = self.coerce_raw_helper_result(raw, &method_desc)?;
            self.push(coerced);
        }
        Ok(())
    }

    fn emit_invoke_native(
        &mut self,
        method_ref: crate::vm::types::MethodRef,
        method_desc: &str,
        args: Vec<Value>,
    ) -> Result<bool, JitError> {
        let method_ref_id = crate::vm::jit::runtime::register_method_ref(method_ref);
        let (_argc_len, argc, arg0, arg1, arg2) = self.emit_helper_args(args, 3);
        let mut sig = Signature::new(self.builder.func.signature.call_conv);
        for _ in 0..6 {
            sig.params.push(AbiParam::new(types::I64));
        }
        sig.returns.push(AbiParam::new(types::I64));
        let sigref = self.builder.import_signature(sig);

        let helper = self.builder.ins().iconst(
            types::I64,
            crate::vm::jit::runtime::get_invoke_native_ptr() as i64,
        );
        let ctx = self.builder.ins().iconst(types::I64, 0);
        let method_ref_id = self.builder.ins().iconst(types::I64, method_ref_id as i64);
        let call_args = vec![ctx, method_ref_id, argc, arg0, arg1, arg2];

        let call = self.builder.ins().call_indirect(sigref, helper, &call_args);
        let result = self.builder.inst_results(call)[0];

        if crate::vm::types::parse_return_type(method_desc)
            .map_err(|e| JitError::CompilationFailed(format!("Invalid descriptor: {}", e)))?
            .is_some_and(|ret| ret != b'V')
        {
            self.push(result);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn frame_size(&self) -> usize {
        self.frame_size
    }

    pub fn stack_slots(&self) -> Vec<StackSlot> {
        self.stack_slots.clone()
    }

    pub fn deopt_info(&self) -> DeoptimizationInfo {
        DeoptimizationInfo {
            guard_checks: self.guard_checks.clone(),
            trap_info: Vec::new(),
        }
    }
}

pub fn compile_method(
    method: &Method,
    func: &mut cranelift::codegen::ir::Function,
    isa: &dyn TargetIsa,
) -> Result<CompiledCode, JitError> {
    let mut ctx = Context::new();
    ctx.func = func.clone();
    let mut ctrl_plane = cranelift::codegen::control::ControlPlane::default();
    ctx.optimize(isa, &mut ctrl_plane)
        .map_err(|e| JitError::CompilationFailed(format!("optimization failed: {}", e)))?;

    ctx.compile(isa, &mut ctrl_plane)
        .map_err(|e| JitError::CompilationFailed(format!("compile failed: {:?}", e)))?;

    let compiled = ctx
        .compiled_code()
        .ok_or_else(|| JitError::CompilationFailed("No compiled code produced".to_string()))?;

    let code_buffer = compiled.buffer.data().to_vec();

    Ok(CompiledCode {
        code_buffer,
        frame_size: 0,
        stack_slots: Vec::new(),
        deopt_info: DeoptimizationInfo {
            guard_checks: Vec::new(),
            trap_info: Vec::new(),
        },
    })
}

pub fn compile_bytecode(method: &Method, isa: &dyn TargetIsa) -> Result<CompiledCode, JitError> {
    reject_runtime_dependent_bytecode(method)?;

    let mut func = cranelift::codegen::ir::Function::new();
    func.name = UserFuncName::user(0, 0);

    let arg_types = crate::vm::types::parse_arg_types(&method.descriptor)
        .ok_or_else(|| JitError::CompilationFailed("Invalid method descriptor".to_string()))?;
    let return_type = crate::vm::types::parse_return_type(&method.descriptor)
        .map_err(|e| JitError::CompilationFailed(format!("Invalid descriptor: {}", e)))?;

    let mut signature = cranelift::codegen::ir::Signature::new(isa.default_call_conv());

    signature.params.insert(0, AbiParam::new(types::I64));

    for _ in &arg_types {
        signature.params.push(AbiParam::new(types::I64));
    }

    if let Some(ret) = return_type {
        let clif_type = match ret {
            b'B' | b'C' | b'I' | b'S' | b'Z' => types::I32,
            b'J' => types::I64,
            b'F' => types::F32,
            b'D' => types::F64,
            b'L' | b'[' => types::I64,
            b'V' => {
                func.signature = signature;
                let mut fn_ctx = FunctionBuilderContext::new();
                {
                    let mut builder = FunctionBuilder::new(&mut func, &mut fn_ctx);
                    let mut compiler =
                        BytecodeCompiler::new(method, &mut builder, arg_types.clone());
                    compiler.lower()?;
                }
                return compile_method(method, &mut func, isa);
            }
            _ => types::I64,
        };
        signature.returns.push(AbiParam::new(clif_type));
    }

    func.signature = signature;

    let mut fn_ctx = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut func, &mut fn_ctx);
        let mut compiler = BytecodeCompiler::new(method, &mut builder, arg_types);
        compiler.lower()?;
    }

    compile_method(method, &mut func, isa)
}

fn reject_runtime_dependent_bytecode(method: &Method) -> Result<(), JitError> {
    let mut pc = 0;
    while pc < method.code.len() {
        let opcode = method.code[pc];
        match opcode {
            0x99..=0xa8 => {
                let high = method.code[pc + 1] as i16;
                let low = method.code[pc + 2] as u16;
                let offset = i16::from_be_bytes([high as u8, low as u8]) as i32;
                if offset < 0 {
                    return Err(JitError::CompilationFailed(
                        "loop backedges stay on the interpreter until block-stack SSA merges are supported"
                            .to_string(),
                    ));
                }
            }
            0xb2 => {
                let cp_index = ((method.code[pc + 1] as usize) << 8) | method.code[pc + 2] as usize;
                if method
                    .field_refs
                    .get(cp_index)
                    .and_then(|field_ref| field_ref.as_ref())
                    .is_some_and(|field_ref| {
                        field_ref.class_name == "java/lang/System" && field_ref.field_name == "out"
                    })
                {
                    return Err(JitError::CompilationFailed(
                        "System.out access stays on the interpreter until println side effects are JIT-safe"
                            .to_string(),
                    ));
                }
            }
            0xba => {
                let cp_index = ((method.code[pc + 1] as usize) << 8) | method.code[pc + 2] as usize;
                if method
                    .invoke_dynamic_sites
                    .get(cp_index)
                    .and_then(|site| site.as_ref())
                    .is_some_and(|site| {
                        matches!(
                            site.kind,
                            crate::vm::types::InvokeDynamicKind::StringConcat { .. }
                        )
                    })
                {
                    return Err(JitError::CompilationFailed(
                        "invokedynamic string concat stays on the interpreter until the JIT helper ABI is verified"
                            .to_string(),
                    ));
                }
            }
            0xbf | 0xff => {
                if !method.exception_handlers.is_empty() {
                    return Err(JitError::CompilationFailed(format!(
                        "opcode 0x{opcode:02x} with an exception table stays on the interpreter until JIT frame unwinding is supported"
                    )));
                }
            }
            _ => {}
        }
        pc = bytecode_next_pc(&method.code, pc, opcode)?;
    }
    Ok(())
}

fn bytecode_next_pc(code: &[u8], pc: usize, opcode: u8) -> Result<usize, JitError> {
    let next = match opcode {
        0x10 | 0x12 | 0xbc => pc + 2,
        0x11 | 0x13 | 0x14 | 0x84 | 0x99..=0xa8 | 0xc6 | 0xc7 => pc + 3,
        0xb2..=0xb8 | 0xbb | 0xbd | 0xbe | 0xc0 | 0xc1 | 0xfe => pc + 3,
        0xb9 | 0xba => pc + 5,
        0xc5 => pc + 4,
        0xaa => {
            let mut cursor = (pc + 4) & !3;
            if cursor + 12 > code.len() {
                return Err(JitError::CompilationFailed("truncated tableswitch".into()));
            }
            cursor += 4;
            let low = i32::from_be_bytes(code[cursor..cursor + 4].try_into().unwrap());
            cursor += 4;
            let high = i32::from_be_bytes(code[cursor..cursor + 4].try_into().unwrap());
            cursor += 4;
            cursor + ((high - low + 1).max(0) as usize * 4)
        }
        0xab => {
            let mut cursor = (pc + 4) & !3;
            if cursor + 8 > code.len() {
                return Err(JitError::CompilationFailed("truncated lookupswitch".into()));
            }
            cursor += 4;
            let pairs = i32::from_be_bytes(code[cursor..cursor + 4].try_into().unwrap());
            cursor += 4;
            cursor + (pairs.max(0) as usize * 8)
        }
        _ => pc + 1,
    };

    if next > code.len() {
        return Err(JitError::CompilationFailed(format!(
            "opcode 0x{opcode:02x} at pc {pc} extends past bytecode end"
        )));
    }
    Ok(next)
}
