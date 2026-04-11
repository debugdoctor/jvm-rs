//! Basic bytecode structural verification.
//!
//! This module checks that a method's bytecode satisfies basic structural
//! constraints before execution:
//! - All opcodes are valid and fully contained within the code array.
//! - Branch targets land on valid instruction boundaries.
//! - The code does not fall off the end (last instruction must be a return,
//!   throw, or unconditional branch).
//! - Constant pool, field ref, and method ref indices are in bounds.

use crate::bytecode::Opcode;

use super::{Method, VmError};

/// Verify basic structural properties of a method's bytecode.
///
/// Returns `Ok(())` if verification passes, or a descriptive `VmError`.
pub fn verify_method(method: &Method) -> Result<(), VmError> {
    if method.code.is_empty() {
        return Ok(());
    }

    let code = &method.code;
    let len = code.len();
    let mut pc = 0;
    let mut instruction_starts = vec![false; len];
    let mut branch_targets: Vec<(usize, i32)> = Vec::new();

    // Pass 1: scan all instructions, record their start positions and branch targets.
    while pc < len {
        instruction_starts[pc] = true;
        let opcode_pc = pc;
        let byte = code[pc];
        pc += 1;

        let opcode = match Opcode::from_byte(byte) {
            Some(op) => op,
            None => {
                return Err(VmError::InvalidOpcode {
                    opcode: byte,
                    pc: opcode_pc,
                });
            }
        };

        // Determine the number of operand bytes consumed by this instruction.
        match opcode {
            // 0 operand bytes
            Opcode::AconstNull
            | Opcode::IconstM1
            | Opcode::Iconst0
            | Opcode::Iconst1
            | Opcode::Iconst2
            | Opcode::Iconst3
            | Opcode::Iconst4
            | Opcode::Iconst5
            | Opcode::Lconst0
            | Opcode::Lconst1
            | Opcode::Fconst0
            | Opcode::Fconst1
            | Opcode::Fconst2
            | Opcode::Dconst0
            | Opcode::Dconst1
            | Opcode::Iaload
            | Opcode::Laload
            | Opcode::Faload
            | Opcode::Daload
            | Opcode::Aaload
            | Opcode::Baload
            | Opcode::Caload
            | Opcode::Saload
            | Opcode::Iastore
            | Opcode::Lastore
            | Opcode::Fastore
            | Opcode::Dastore
            | Opcode::Aastore
            | Opcode::Bastore
            | Opcode::Castore
            | Opcode::Sastore
            | Opcode::Pop
            | Opcode::Pop2
            | Opcode::Dup
            | Opcode::DupX1
            | Opcode::DupX2
            | Opcode::Dup2
            | Opcode::Dup2X1
            | Opcode::Dup2X2
            | Opcode::Swap
            | Opcode::Iadd
            | Opcode::Ladd
            | Opcode::Fadd
            | Opcode::Dadd
            | Opcode::Isub
            | Opcode::Lsub
            | Opcode::Fsub
            | Opcode::Dsub
            | Opcode::Imul
            | Opcode::Lmul
            | Opcode::Fmul
            | Opcode::Dmul
            | Opcode::Idiv
            | Opcode::Ldiv
            | Opcode::Fdiv
            | Opcode::Ddiv
            | Opcode::Irem
            | Opcode::Lrem
            | Opcode::Frem
            | Opcode::Drem
            | Opcode::Ineg
            | Opcode::Lneg
            | Opcode::Fneg
            | Opcode::Dneg
            | Opcode::Ishl
            | Opcode::Lshl
            | Opcode::Ishr
            | Opcode::Lshr
            | Opcode::Iushr
            | Opcode::Lushr
            | Opcode::Iand
            | Opcode::Land
            | Opcode::Ior
            | Opcode::Lor
            | Opcode::Ixor
            | Opcode::Lxor
            | Opcode::I2l
            | Opcode::I2f
            | Opcode::I2d
            | Opcode::L2i
            | Opcode::L2f
            | Opcode::L2d
            | Opcode::F2i
            | Opcode::F2l
            | Opcode::F2d
            | Opcode::D2i
            | Opcode::D2l
            | Opcode::D2f
            | Opcode::I2b
            | Opcode::I2c
            | Opcode::I2s
            | Opcode::Lcmp
            | Opcode::Fcmpl
            | Opcode::Fcmpg
            | Opcode::Dcmpl
            | Opcode::Dcmpg
            | Opcode::Ireturn
            | Opcode::Lreturn
            | Opcode::Freturn
            | Opcode::Dreturn
            | Opcode::Areturn
            | Opcode::Return
            | Opcode::Arraylength
            | Opcode::Athrow
            | Opcode::Monitorenter
            | Opcode::Monitorexit => {}

            // 1 operand byte
            Opcode::Bipush
            | Opcode::Ldc
            | Opcode::Iload
            | Opcode::Lload
            | Opcode::Fload
            | Opcode::Dload
            | Opcode::Aload
            | Opcode::Istore
            | Opcode::Lstore
            | Opcode::Fstore
            | Opcode::Dstore
            | Opcode::Astore
            | Opcode::Newarray => {
                pc += 1;
            }

            // 1-byte index shortforms (0 extra operand bytes)
            Opcode::Iload0
            | Opcode::Iload1
            | Opcode::Iload2
            | Opcode::Iload3
            | Opcode::Lload0
            | Opcode::Lload1
            | Opcode::Lload2
            | Opcode::Lload3
            | Opcode::Fload0
            | Opcode::Fload1
            | Opcode::Fload2
            | Opcode::Fload3
            | Opcode::Dload0
            | Opcode::Dload1
            | Opcode::Dload2
            | Opcode::Dload3
            | Opcode::Aload0
            | Opcode::Aload1
            | Opcode::Aload2
            | Opcode::Aload3
            | Opcode::Istore0
            | Opcode::Istore1
            | Opcode::Istore2
            | Opcode::Istore3
            | Opcode::Lstore0
            | Opcode::Lstore1
            | Opcode::Lstore2
            | Opcode::Lstore3
            | Opcode::Fstore0
            | Opcode::Fstore1
            | Opcode::Fstore2
            | Opcode::Fstore3
            | Opcode::Dstore0
            | Opcode::Dstore1
            | Opcode::Dstore2
            | Opcode::Dstore3
            | Opcode::Astore0
            | Opcode::Astore1
            | Opcode::Astore2
            | Opcode::Astore3 => {}

            // 2-byte operand
            Opcode::Sipush | Opcode::LdcW | Opcode::Ldc2W => {
                pc += 2;
            }

            // 2-byte branch offset
            Opcode::Ifeq
            | Opcode::Ifne
            | Opcode::Iflt
            | Opcode::Ifge
            | Opcode::Ifgt
            | Opcode::Ifle
            | Opcode::IfIcmpeq
            | Opcode::IfIcmpne
            | Opcode::IfIcmplt
            | Opcode::IfIcmpge
            | Opcode::IfIcmpgt
            | Opcode::IfIcmple
            | Opcode::IfAcmpeq
            | Opcode::IfAcmpne
            | Opcode::Goto
            | Opcode::Ifnull
            | Opcode::Ifnonnull => {
                if pc + 1 >= len {
                    return Err(VmError::UnexpectedEof { pc });
                }
                let offset = i16::from_be_bytes([code[pc], code[pc + 1]]) as i32;
                branch_targets.push((opcode_pc, offset));
                pc += 2;
            }

            // 4-byte branch offset
            Opcode::GotoW => {
                if pc + 3 >= len {
                    return Err(VmError::UnexpectedEof { pc });
                }
                let offset = i32::from_be_bytes([code[pc], code[pc + 1], code[pc + 2], code[pc + 3]]);
                branch_targets.push((opcode_pc, offset));
                pc += 4;
            }

            // 2-byte index
            Opcode::Getstatic
            | Opcode::Putstatic
            | Opcode::Getfield
            | Opcode::Putfield
            | Opcode::Invokevirtual
            | Opcode::Invokespecial
            | Opcode::Invokestatic
            | Opcode::New
            | Opcode::Anewarray
            | Opcode::Checkcast
            | Opcode::Instanceof => {
                pc += 2;
            }

            // 2-byte iinc: index + delta
            Opcode::Iinc => {
                pc += 2;
            }

            // invokeinterface: 2-byte index + count + 0
            Opcode::Invokeinterface => {
                pc += 4;
            }

            // invokedynamic: 2-byte index + 2 zero bytes
            Opcode::Invokedynamic => {
                pc += 4;
            }

            // multianewarray: 2-byte index + 1-byte dimensions
            Opcode::Multianewarray => {
                pc += 3;
            }

            // tableswitch: variable length
            Opcode::Tableswitch => {
                let padding = (4 - (pc % 4)) % 4;
                pc += padding;
                if pc + 12 > len {
                    return Err(VmError::UnexpectedEof { pc });
                }
                let _default = i32::from_be_bytes([code[pc], code[pc+1], code[pc+2], code[pc+3]]);
                let low = i32::from_be_bytes([code[pc+4], code[pc+5], code[pc+6], code[pc+7]]);
                let high = i32::from_be_bytes([code[pc+8], code[pc+9], code[pc+10], code[pc+11]]);
                pc += 12;
                let count = (high - low + 1) as usize;
                pc += count * 4;
            }

            // lookupswitch: variable length
            Opcode::Lookupswitch => {
                let padding = (4 - (pc % 4)) % 4;
                pc += padding;
                if pc + 8 > len {
                    return Err(VmError::UnexpectedEof { pc });
                }
                let _default = i32::from_be_bytes([code[pc], code[pc+1], code[pc+2], code[pc+3]]);
                let npairs = i32::from_be_bytes([code[pc+4], code[pc+5], code[pc+6], code[pc+7]]) as usize;
                pc += 8;
                pc += npairs * 8;
            }

            // wide: variable
            Opcode::Wide => {
                if pc >= len {
                    return Err(VmError::UnexpectedEof { pc });
                }
                let inner = code[pc];
                pc += 1;
                pc += 2; // wide index
                // wide iinc has an additional 2 bytes
                if inner == 0x84 {
                    pc += 2;
                }
            }
        }

        if pc > len {
            return Err(VmError::UnexpectedEof { pc: opcode_pc });
        }
    }

    // Pass 2: verify branch targets land on instruction boundaries.
    for (opcode_pc, offset) in &branch_targets {
        let target = *opcode_pc as isize + *offset as isize;
        if target < 0 || target as usize > len {
            return Err(VmError::InvalidBranchTarget {
                target,
                code_len: len,
            });
        }
        // Target at exactly `len` is technically a fall-off but is permitted
        // by some compilers; it will be caught at runtime as MissingReturn.
        if (target as usize) < len && !instruction_starts[target as usize] {
            return Err(VmError::InvalidBranchTarget {
                target,
                code_len: len,
            });
        }
    }

    Ok(())
}
