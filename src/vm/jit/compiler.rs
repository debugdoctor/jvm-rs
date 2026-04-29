use cranelift::prelude::*;
use cranelift::codegen::ir::Function;
use cranelift_module::Module;

use crate::vm::types::Method;
use super::{CompiledCode, JitError, StackSlot, DeoptimizationInfo, GuardCheck};

pub struct BytecodeCompiler<'a> {
    method: &'a Method,
    builder: &'a mut FunctionBuilder<'a>,
    value_stack: Vec<Value>,
    frame_size: usize,
    stack_slots: Vec<StackSlot>,
    guard_checks: Vec<GuardCheck>,
    pc_offset: usize,
}

impl<'a> BytecodeCompiler<'a> {
    pub fn new(method: &'a Method, builder: &'a mut FunctionBuilder<'a>) -> Self {
        Self {
            method,
            builder,
            value_stack: Vec::new(),
            frame_size: 0,
            stack_slots: Vec::new(),
            guard_checks: Vec::new(),
            pc_offset: 0,
        }
    }

    pub fn lower(&mut self) -> Result<(), JitError> {
        let code = &self.method.code;
        let mut pc = 0;

        while pc < code.len() {
            self.pc_offset = pc;
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
            0xac => self.lower_ireturn(),
            0xad => self.lower_lreturn(),
            0xae => self.lower_freturn(),
            0xaf => self.lower_dreturn(),
            0xb0 => self.lower_areturn(),
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
                crate::vm::types::Value::Reference(_) => {
                    let null = self.builder.ins().iconst(types::I64, 0);
                    self.push(null);
                }
                _ => return Err(JitError::CompilationFailed("Unsupported constant type".into())),
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

    fn load_local(&mut self, index: usize, ty: Type) -> Result<(), JitError> {
        let param_value = self.builder.block_params(self.builder.current_block().unwrap())[index];
        self.push(param_value);
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
        let elem_size = self.builder.ins().iconst(types::I64, 4);
        let offset = self.builder.ins().imul(index, elem_size);
        let addr = self.builder.ins().iadd(array_ref, offset);
        let val = self.builder.ins().load(types::I32, MemFlags::new(), addr, 0);
        self.push(val);
        Ok(())
    }

    fn lower_istore(&mut self) -> Result<(), JitError> {
        let index = self.method.code[self.pc_offset + 1] as usize;
        self.store_local(index)
    }

    fn lower_lstore(&mut self) -> Result<(), JitError> {
        let index = self.method.code[self.pc_offset + 1] as usize;
        self.store_local(index)
    }

    fn lower_fstore(&mut self) -> Result<(), JitError> {
        let index = self.method.code[self.pc_offset + 1] as usize;
        self.store_local(index)
    }

    fn lower_dstore(&mut self) -> Result<(), JitError> {
        let index = self.method.code[self.pc_offset + 1] as usize;
        self.store_local(index)
    }

    fn lower_astore(&mut self) -> Result<(), JitError> {
        let index = self.method.code[self.pc_offset + 1] as usize;
        self.store_local(index)
    }

    fn store_local(&mut self, _index: usize) -> Result<(), JitError> {
        Ok(())
    }

    fn lower_istore_n(&mut self, n: usize) -> Result<(), JitError> {
        self.store_local(n)
    }

    fn lower_lstore_n(&mut self, n: usize) -> Result<(), JitError> {
        self.store_local(n)
    }

    fn lower_fstore_n(&mut self, n: usize) -> Result<(), JitError> {
        self.store_local(n)
    }

    fn lower_dstore_n(&mut self, n: usize) -> Result<(), JitError> {
        self.store_local(n)
    }

    fn lower_astore_n(&mut self, n: usize) -> Result<(), JitError> {
        self.store_local(n)
    }

    fn lower_iastore(&mut self) -> Result<(), JitError> {
        let value = self.pop();
        let index = self.pop();
        let array_ref = self.pop();
        let elem_size = self.builder.ins().iconst(types::I64, 4);
        let offset = self.builder.ins().imul(index, elem_size);
        let addr = self.builder.ins().iadd(array_ref, offset);
        self.builder.ins().store(MemFlags::new(), value, addr, 0);
        Ok(())
    }

    fn lower_lastore(&mut self) -> Result<(), JitError> {
        let value = self.pop();
        let index = self.pop();
        let array_ref = self.pop();
        let elem_size = self.builder.ins().iconst(types::I64, 8);
        let offset = self.builder.ins().imul(index, elem_size);
        let addr = self.builder.ins().iadd(array_ref, offset);
        self.builder.ins().store(MemFlags::new(), value, addr, 0);
        Ok(())
    }

    fn lower_fastore(&mut self) -> Result<(), JitError> {
        let value = self.pop();
        let index = self.pop();
        let array_ref = self.pop();
        let elem_size = self.builder.ins().iconst(types::I64, 4);
        let offset = self.builder.ins().imul(index, elem_size);
        let addr = self.builder.ins().iadd(array_ref, offset);
        self.builder.ins().store(MemFlags::new(), value, addr, 0);
        Ok(())
    }

    fn lower_dastore(&mut self) -> Result<(), JitError> {
        let value = self.pop();
        let index = self.pop();
        let array_ref = self.pop();
        let elem_size = self.builder.ins().iconst(types::I64, 8);
        let offset = self.builder.ins().imul(index, elem_size);
        let addr = self.builder.ins().iadd(array_ref, offset);
        self.builder.ins().store(MemFlags::new(), value, addr, 0);
        Ok(())
    }

    fn lower_aastore(&mut self) -> Result<(), JitError> {
        let value = self.pop();
        let index = self.pop();
        let array_ref = self.pop();
        let elem_size = self.builder.ins().iconst(types::I64, 8);
        let offset = self.builder.ins().imul(index, elem_size);
        let addr = self.builder.ins().iadd(array_ref, offset);
        self.builder.ins().store(MemFlags::new(), value, addr, 0);
        Ok(())
    }

    fn lower_bastore(&mut self) -> Result<(), JitError> {
        let value = self.pop();
        let index = self.pop();
        let array_ref = self.pop();
        let elem_size = self.builder.ins().iconst(types::I64, 1);
        let offset = self.builder.ins().imul(index, elem_size);
        let addr = self.builder.ins().iadd(array_ref, offset);
        self.builder.ins().store(MemFlags::new(), value, addr, 0);
        Ok(())
    }

    fn lower_castore(&mut self) -> Result<(), JitError> {
        let value = self.pop();
        let index = self.pop();
        let array_ref = self.pop();
        let elem_size = self.builder.ins().iconst(types::I64, 2);
        let offset = self.builder.ins().imul(index, elem_size);
        let addr = self.builder.ins().iadd(array_ref, offset);
        self.builder.ins().store(MemFlags::new(), value, addr, 0);
        Ok(())
    }

    fn lower_sastore(&mut self) -> Result<(), JitError> {
        let value = self.pop();
        let index = self.pop();
        let array_ref = self.pop();
        let elem_size = self.builder.ins().iconst(types::I64, 2);
        let offset = self.builder.ins().imul(index, elem_size);
        let addr = self.builder.ins().iadd(array_ref, offset);
        self.builder.ins().store(MemFlags::new(), value, addr, 0);
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

    fn lower_iinc(&mut self) -> Result<(), JitError> {
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
        let result = self.builder.ins().bint(types::I32, cmp);
        self.push(result);
        Ok(())
    }

    fn lower_fcmpl(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let cmp = self.builder.ins().fcmp(FloatCC::LessThan, lhs, rhs);
        let result = self.builder.ins().bint(types::I32, cmp);
        self.push(result);
        Ok(())
    }

    fn lower_fcmpg(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let cmp = self.builder.ins().fcmp(FloatCC::GreaterThan, lhs, rhs);
        let result = self.builder.ins().bint(types::I32, cmp);
        self.push(result);
        Ok(())
    }

    fn lower_dcmpl(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let cmp = self.builder.ins().fcmp(FloatCC::LessThan, lhs, rhs);
        let result = self.builder.ins().bint(types::I32, cmp);
        self.push(result);
        Ok(())
    }

    fn lower_dcmpg(&mut self) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();
        let cmp = self.builder.ins().fcmp(FloatCC::GreaterThan, lhs, rhs);
        let result = self.builder.ins().bint(types::I32, cmp);
        self.push(result);
        Ok(())
    }

    fn lower_if_icmp(&mut self, opcode: u8) -> Result<(), JitError> {
        let rhs = self.pop();
        let lhs = self.pop();

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
            0xa5 => IntCC::UnsignedLessThan,
            0xa6 => IntCC::UnsignedGreaterThanOrEqual,
            _ => return Err(JitError::CompilationFailed(format!("Invalid if_icmp opcode: 0x{:02x}", opcode))),
        };

        let cmp = self.builder.ins().icmp(cond, lhs, rhs);
        self.push(cmp);
        Ok(())
    }

    fn lower_goto(&mut self) -> Result<(), JitError> {
        Ok(())
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
        let null = self.builder.ins().iconst(types::I64, 0);
        self.push(null);
        Ok(())
    }

    fn lower_getfield(&mut self) -> Result<(), JitError> {
        let obj = self.pop();
        let field_val = self.pop();
        let zero = self.builder.ins().iconst(types::I64, 0);
        self.push(zero);
        Ok(())
    }

    fn lower_putfield(&mut self) -> Result<(), JitError> {
        let val = self.pop();
        let obj = self.pop();
        Ok(())
    }

    fn lower_invokevirtual(&mut self) -> Result<(), JitError> {
        let argc = (self.method.code[self.pc_offset + 3] & 0xFF) as usize;
        for _ in 0..argc {
            self.pop();
        }
        let this_ref = self.pop();
        let null = self.builder.ins().iconst(types::I64, 0);
        self.push(null);
        Ok(())
    }

    fn lower_invokespecial(&mut self) -> Result<(), JitError> {
        let argc = (self.method.code[self.pc_offset + 3] & 0xFF) as usize;
        for _ in 0..argc {
            self.pop();
        }
        let this_ref = self.pop();
        let null = self.builder.ins().iconst(types::I64, 0);
        self.push(null);
        Ok(())
    }

    fn lower_invokestatic(&mut self) -> Result<(), JitError> {
        let argc = (self.method.code[self.pc_offset + 3] & 0xFF) as usize;
        for _ in 0..argc {
            self.pop();
        }
        let null = self.builder.ins().iconst(types::I64, 0);
        self.push(null);
        Ok(())
    }

    fn lower_invokeinterface(&mut self) -> Result<(), JitError> {
        let argc = (self.method.code[self.pc_offset + 3] & 0xFF) as usize;
        for _ in 0..argc {
            self.pop();
        }
        let this_ref = self.pop();
        let null = self.builder.ins().iconst(types::I64, 0);
        self.push(null);
        Ok(())
    }

    fn lower_invokedynamic(&mut self) -> Result<(), JitError> {
        let null = self.builder.ins().iconst(types::I64, 0);
        self.push(null);
        Ok(())
    }

    fn lower_new(&mut self) -> Result<(), JitError> {
        let null = self.builder.ins().iconst(types::I64, 0);
        self.push(null);
        Ok(())
    }

    fn lower_newarray(&mut self) -> Result<(), JitError> {
        let count = self.pop();
        let null = self.builder.ins().iconst(types::I64, 0);
        self.push(null);
        Ok(())
    }

    fn lower_anewarray(&mut self) -> Result<(), JitError> {
        let count = self.pop();
        let null = self.builder.ins().iconst(types::I64, 0);
        self.push(null);
        Ok(())
    }
    fn lower_arraylength(&mut self) -> Result<(), JitError> {
        let array_ref = self.pop();
        let len = self.builder.ins().load(types::I32, MemFlags::new(), array_ref, 0);
        self.push(len);
        Ok(())
    }
    fn lower_athrow(&mut self) -> Result<(), JitError> {
        let exception_ref = self.pop();
        let null = self.builder.ins().iconst(types::I64, 0);
        self.push(null);
        Ok(())
    }

    fn lower_checkcast(&mut self) -> Result<(), JitError> {
        let obj_ref = self.pop();
        self.push(obj_ref);
        Ok(())
    }

    fn lower_instanceof(&mut self) -> Result<(), JitError> {
        let obj_ref = self.pop();
        let zero = self.builder.ins().iconst(types::I32, 0);
        self.push(zero);
        Ok(())
    }

    fn lower_monitorenter(&mut self) -> Result<(), JitError> {
        let obj_ref = self.pop();
        Ok(())
    }

    fn lower_monitorexit(&mut self) -> Result<(), JitError> {
        let obj_ref = self.pop();
        Ok(())
    }

    fn lower_invokenative(&mut self) -> Result<(), JitError> {
        let null = self.builder.ins().iconst(types::I64, 0);
        self.push(null);
        Ok(())
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

pub fn compile_method<'a>(
    method: &'a Method,
    builder: &'a mut FunctionBuilder<'a>,
) -> Result<CompiledCode, JitError> {
    let mut compiler = BytecodeCompiler::new(method, builder);
    compiler.lower()?;

    Ok(CompiledCode {
        code_buffer: Vec::new(),
        frame_size: compiler.frame_size(),
        stack_slots: compiler.stack_slots(),
        deopt_info: compiler.deopt_info(),
    })
}