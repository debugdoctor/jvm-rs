//! Data-flow bytecode verification.
//!
//! This verifier performs:
//! - structural opcode / instruction-boundary validation
//! - control-flow validation for branches, switches, and exception handlers
//! - local / operand stack type propagation
//! - basic constructor tracking for uninitialized values
//! - optional `StackMapTable` consistency checks when present

use std::collections::{BTreeMap, VecDeque};

use crate::bytecode::Opcode;
use crate::classfile::VerificationTypeInfo;

use super::{Method, Value, VmError};

#[derive(Debug, Clone, PartialEq, Eq)]
enum VerifyType {
    Top,
    Int,
    Float,
    Long,
    Double,
    Null,
    ReturnAddress(usize),
    Reference(Option<String>),
    UninitializedThis,
    Uninitialized(u16),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FrameState {
    locals: Vec<VerifyType>,
    stack: Vec<VerifyType>,
}

#[derive(Debug, Clone)]
struct DecodedInstruction {
    pc: usize,
    next_pc: usize,
    opcode: Opcode,
    local_index: Option<usize>,
    cp_index: Option<usize>,
    branch_targets: Vec<usize>,
    iinc_delta: Option<i32>,
    atype: Option<u8>,
    dimensions: Option<u8>,
}

pub fn verify_method(method: &Method) -> Result<(), VmError> {
    if method.code.is_empty() {
        return Ok(());
    }

    let decoded = scan_method(method)?;
    validate_exception_handlers(method, &decoded)?;

    let mut states = vec![None; method.code.len()];
    let entry = initial_frame(method)?;
    states[0] = Some(entry);

    let mut queue = VecDeque::from([0usize]);
    while let Some(pc) = queue.pop_front() {
        let Some(state) = states[pc].clone() else {
            continue;
        };
        let insn = decoded
            .get(&pc)
            .ok_or_else(|| verification_error(pc, "missing decoded instruction"))?;

        let mut after = state.clone();
        apply_instruction(method, insn, &mut after)?;

        for handler in method
            .exception_handlers
            .iter()
            .filter(|handler| pc >= handler.start_pc as usize && pc < handler.end_pc as usize)
        {
            let catch_type = match &handler.catch_class {
                Some(name) => VerifyType::Reference(Some(name.clone())),
                None => VerifyType::Reference(Some("java/lang/Throwable".to_string())),
            };
            let handler_state = FrameState {
                locals: state.locals.clone(),
                stack: vec![catch_type],
            };
            if merge_state(
                &mut states[handler.handler_pc as usize],
                handler_state,
                handler.handler_pc as usize,
            )? {
                queue.push_back(handler.handler_pc as usize);
            }
        }

        if has_fallthrough(insn.opcode) {
            if merge_state(&mut states[insn.next_pc], after.clone(), insn.next_pc)? {
                queue.push_back(insn.next_pc);
            }
        }
        if insn.opcode == Opcode::Ret {
            let target = ret_target(&after, insn)?;
            if merge_state(&mut states[target], after.clone(), target)? {
                queue.push_back(target);
            }
        }
        for &target in &insn.branch_targets {
            if merge_state(&mut states[target], after.clone(), target)? {
                queue.push_back(target);
            }
        }
    }

    validate_stack_map_frames(method, &states)?;
    Ok(())
}

fn scan_method(method: &Method) -> Result<BTreeMap<usize, DecodedInstruction>, VmError> {
    let code = &method.code;
    let len = code.len();
    let mut pc = 0usize;
    let mut starts = vec![false; len];
    let mut decoded = BTreeMap::new();

    while pc < len {
        starts[pc] = true;
        let insn = decode_instruction(code, pc)?;
        if insn.next_pc > len {
            return Err(VmError::UnexpectedEof { pc });
        }

        match insn.opcode {
            Opcode::Ldc | Opcode::LdcW | Opcode::Ldc2W => {
                validate_constant_index(method, insn.cp_index.unwrap(), pc)?;
            }
            Opcode::Getstatic | Opcode::Putstatic | Opcode::Getfield | Opcode::Putfield => {
                validate_field_ref_index(method, insn.cp_index.unwrap(), pc)?;
            }
            Opcode::Invokevirtual
            | Opcode::Invokespecial
            | Opcode::Invokestatic
            | Opcode::Invokeinterface => {
                validate_method_ref_index(method, insn.cp_index.unwrap(), pc)?;
            }
            Opcode::Invokedynamic => {
                validate_invoke_dynamic_index(method, insn.cp_index.unwrap(), pc)?;
            }
            Opcode::New
            | Opcode::Anewarray
            | Opcode::Checkcast
            | Opcode::Instanceof
            | Opcode::Multianewarray => {
                validate_class_index(method, insn.cp_index.unwrap(), pc)?;
            }
            _ => {}
        }

        decoded.insert(pc, insn.clone());
        pc = insn.next_pc;
    }

    for insn in decoded.values() {
        for &target in &insn.branch_targets {
            if target >= len || !starts[target] {
                return Err(VmError::InvalidBranchTarget {
                    target: target as isize,
                    code_len: len,
                });
            }
        }
    }

    Ok(decoded)
}

fn validate_exception_handlers(
    method: &Method,
    decoded: &BTreeMap<usize, DecodedInstruction>,
) -> Result<(), VmError> {
    let len = method.code.len();
    for handler in &method.exception_handlers {
        let start = handler.start_pc as usize;
        let end = handler.end_pc as usize;
        let target = handler.handler_pc as usize;
        if start > end || end > len {
            return Err(verification_error(
                target.min(len.saturating_sub(1)),
                "exception handler range is out of bounds",
            ));
        }
        if target >= len || !decoded.contains_key(&target) {
            return Err(verification_error(
                target.min(len.saturating_sub(1)),
                "exception handler target is not an instruction boundary",
            ));
        }
    }
    Ok(())
}

fn validate_stack_map_frames(
    method: &Method,
    states: &[Option<FrameState>],
) -> Result<(), VmError> {
    if method.stack_map_frames.is_empty() {
        return Ok(());
    }

    let mut previous = None::<usize>;
    for frame in &method.stack_map_frames {
        let pc = match previous {
            None => frame.offset_delta as usize,
            Some(prev) => prev + frame.offset_delta as usize + 1,
        };
        previous = Some(pc);
        let Some(state) = states.get(pc).and_then(|s| s.as_ref()) else {
            return Err(verification_error(
                pc,
                "StackMapTable frame points to unreachable or invalid code",
            ));
        };
        let expected_locals = frame
            .locals
            .iter()
            .map(|info| stack_map_type(method, info))
            .collect::<Result<Vec<_>, _>>()?;
        let expected_stack = frame
            .stack
            .iter()
            .map(|info| stack_map_type(method, info))
            .collect::<Result<Vec<_>, _>>()?;

        compare_stack_map(pc, &state.locals, &expected_locals, "locals")?;
        compare_stack_map(pc, &state.stack, &expected_stack, "stack")?;
    }

    Ok(())
}

fn compare_stack_map(
    pc: usize,
    actual: &[VerifyType],
    expected: &[VerifyType],
    what: &str,
) -> Result<(), VmError> {
    let actual = trimmed(actual);
    let expected = trimmed(expected);
    if what == "locals" && actual.len() >= expected.len() {
        for (idx, (actual_ty, expected_ty)) in actual.iter().zip(expected.iter()).enumerate() {
            if !type_compatible(actual_ty, expected_ty) {
                return Err(verification_error(
                    pc,
                    format!(
                        "StackMapTable {what}[{idx}] mismatch: expected {}, got {}",
                        type_name(expected_ty),
                        type_name(actual_ty)
                    ),
                ));
            }
        }
        return Ok(());
    }
    if actual.len() != expected.len() {
        return Err(verification_error(
            pc,
            format!(
                "StackMapTable {what} length mismatch: expected {}, got {}",
                expected.len(),
                actual.len()
            ),
        ));
    }
    for (idx, (actual_ty, expected_ty)) in actual.iter().zip(expected.iter()).enumerate() {
        if !type_compatible(actual_ty, expected_ty) {
            return Err(verification_error(
                pc,
                format!(
                    "StackMapTable {what}[{idx}] mismatch: expected {}, got {}",
                    type_name(expected_ty),
                    type_name(actual_ty)
                ),
            ));
        }
    }
    Ok(())
}

fn initial_frame(method: &Method) -> Result<FrameState, VmError> {
    let mut locals = vec![VerifyType::Top; method.max_locals];
    let mut index = 0usize;

    if method.access_flags & 0x0008 == 0 {
        let this_ty = if method.name == "<init>" {
            VerifyType::UninitializedThis
        } else {
            VerifyType::Reference(if method.class_name.is_empty() {
                None
            } else {
                Some(method.class_name.clone())
            })
        };
        if index < locals.len() {
            locals[index] = this_ty;
        }
        index += 1;
    }

    let (params, _) = parse_method_descriptor(&method.descriptor)?;
    for ty in params {
        if index >= locals.len() {
            return Err(verification_error(
                0,
                "descriptor requires more locals than max_locals",
            ));
        }
        locals[index] = ty;
        index += 1;
    }

    for (idx, value) in method.initial_locals.iter().enumerate() {
        if idx >= locals.len() {
            return Err(verification_error(
                0,
                format!("initial local {idx} exceeds max_locals {}", locals.len()),
            ));
        }
        if let Some(value) = value {
            locals[idx] = verify_type_from_value(*value);
        }
    }

    Ok(FrameState {
        locals,
        stack: Vec::new(),
    })
}

fn apply_instruction(
    method: &Method,
    insn: &DecodedInstruction,
    state: &mut FrameState,
) -> Result<(), VmError> {
    let ret_ty = parse_method_descriptor(&method.descriptor)?.1;

    match insn.opcode {
        Opcode::AconstNull => push(state, VerifyType::Null, insn.pc, method.max_stack)?,
        Opcode::IconstM1
        | Opcode::Iconst0
        | Opcode::Iconst1
        | Opcode::Iconst2
        | Opcode::Iconst3
        | Opcode::Iconst4
        | Opcode::Iconst5
        | Opcode::Bipush
        | Opcode::Sipush => push(state, VerifyType::Int, insn.pc, method.max_stack)?,
        Opcode::Lconst0 | Opcode::Lconst1 => {
            push(state, VerifyType::Long, insn.pc, method.max_stack)?
        }
        Opcode::Fconst0 | Opcode::Fconst1 | Opcode::Fconst2 => {
            push(state, VerifyType::Float, insn.pc, method.max_stack)?
        }
        Opcode::Dconst0 | Opcode::Dconst1 => {
            push(state, VerifyType::Double, insn.pc, method.max_stack)?
        }
        Opcode::Ldc | Opcode::LdcW | Opcode::Ldc2W => {
            let ty = constant_type(method, insn.cp_index.unwrap(), insn.pc)?;
            push(state, ty, insn.pc, method.max_stack)?;
        }

        Opcode::Iload | Opcode::Iload0 | Opcode::Iload1 | Opcode::Iload2 | Opcode::Iload3 => {
            let ty = load_local(state, insn.local_index.unwrap(), insn.pc)?;
            require_type(insn.pc, &ty, &VerifyType::Int)?;
            push(state, VerifyType::Int, insn.pc, method.max_stack)?;
        }
        Opcode::Lload | Opcode::Lload0 | Opcode::Lload1 | Opcode::Lload2 | Opcode::Lload3 => {
            let ty = load_local(state, insn.local_index.unwrap(), insn.pc)?;
            require_type(insn.pc, &ty, &VerifyType::Long)?;
            push(state, VerifyType::Long, insn.pc, method.max_stack)?;
        }
        Opcode::Fload | Opcode::Fload0 | Opcode::Fload1 | Opcode::Fload2 | Opcode::Fload3 => {
            let ty = load_local(state, insn.local_index.unwrap(), insn.pc)?;
            require_type(insn.pc, &ty, &VerifyType::Float)?;
            push(state, VerifyType::Float, insn.pc, method.max_stack)?;
        }
        Opcode::Dload | Opcode::Dload0 | Opcode::Dload1 | Opcode::Dload2 | Opcode::Dload3 => {
            let ty = load_local(state, insn.local_index.unwrap(), insn.pc)?;
            require_type(insn.pc, &ty, &VerifyType::Double)?;
            push(state, VerifyType::Double, insn.pc, method.max_stack)?;
        }
        Opcode::Aload | Opcode::Aload0 | Opcode::Aload1 | Opcode::Aload2 | Opcode::Aload3 => {
            let ty = load_local(state, insn.local_index.unwrap(), insn.pc)?;
            require_reference_like(insn.pc, &ty)?;
            push(state, ty, insn.pc, method.max_stack)?;
        }

        Opcode::Istore | Opcode::Istore0 | Opcode::Istore1 | Opcode::Istore2 | Opcode::Istore3 => {
            let ty = pop(state, insn.pc)?;
            require_type(insn.pc, &ty, &VerifyType::Int)?;
            store_local(state, insn.local_index.unwrap(), VerifyType::Int, insn.pc)?;
        }
        Opcode::Lstore | Opcode::Lstore0 | Opcode::Lstore1 | Opcode::Lstore2 | Opcode::Lstore3 => {
            let ty = pop(state, insn.pc)?;
            require_type(insn.pc, &ty, &VerifyType::Long)?;
            store_local(state, insn.local_index.unwrap(), VerifyType::Long, insn.pc)?;
        }
        Opcode::Fstore | Opcode::Fstore0 | Opcode::Fstore1 | Opcode::Fstore2 | Opcode::Fstore3 => {
            let ty = pop(state, insn.pc)?;
            require_type(insn.pc, &ty, &VerifyType::Float)?;
            store_local(state, insn.local_index.unwrap(), VerifyType::Float, insn.pc)?;
        }
        Opcode::Dstore | Opcode::Dstore0 | Opcode::Dstore1 | Opcode::Dstore2 | Opcode::Dstore3 => {
            let ty = pop(state, insn.pc)?;
            require_type(insn.pc, &ty, &VerifyType::Double)?;
            store_local(
                state,
                insn.local_index.unwrap(),
                VerifyType::Double,
                insn.pc,
            )?;
        }
        Opcode::Astore | Opcode::Astore0 | Opcode::Astore1 | Opcode::Astore2 | Opcode::Astore3 => {
            let ty = pop(state, insn.pc)?;
            require_astore_type(insn.pc, &ty)?;
            store_local(state, insn.local_index.unwrap(), ty, insn.pc)?;
        }

        Opcode::Iaload | Opcode::Baload | Opcode::Caload | Opcode::Saload => {
            pop_expect_int(state, insn.pc)?;
            pop_expect_reference(state, insn.pc)?;
            push(state, VerifyType::Int, insn.pc, method.max_stack)?;
        }
        Opcode::Laload => {
            pop_expect_int(state, insn.pc)?;
            pop_expect_reference(state, insn.pc)?;
            push(state, VerifyType::Long, insn.pc, method.max_stack)?;
        }
        Opcode::Faload => {
            pop_expect_int(state, insn.pc)?;
            pop_expect_reference(state, insn.pc)?;
            push(state, VerifyType::Float, insn.pc, method.max_stack)?;
        }
        Opcode::Daload => {
            pop_expect_int(state, insn.pc)?;
            pop_expect_reference(state, insn.pc)?;
            push(state, VerifyType::Double, insn.pc, method.max_stack)?;
        }
        Opcode::Aaload => {
            pop_expect_int(state, insn.pc)?;
            pop_expect_reference(state, insn.pc)?;
            push(
                state,
                VerifyType::Reference(None),
                insn.pc,
                method.max_stack,
            )?;
        }

        Opcode::Iastore | Opcode::Bastore | Opcode::Castore | Opcode::Sastore => {
            pop_expect_int(state, insn.pc)?;
            pop_expect_int(state, insn.pc)?;
            pop_expect_reference(state, insn.pc)?;
        }
        Opcode::Lastore => {
            pop_expect_long(state, insn.pc)?;
            pop_expect_int(state, insn.pc)?;
            pop_expect_reference(state, insn.pc)?;
        }
        Opcode::Fastore => {
            pop_expect_float(state, insn.pc)?;
            pop_expect_int(state, insn.pc)?;
            pop_expect_reference(state, insn.pc)?;
        }
        Opcode::Dastore => {
            pop_expect_double(state, insn.pc)?;
            pop_expect_int(state, insn.pc)?;
            pop_expect_reference(state, insn.pc)?;
        }
        Opcode::Aastore => {
            pop_expect_reference(state, insn.pc)?;
            pop_expect_int(state, insn.pc)?;
            pop_expect_reference(state, insn.pc)?;
        }

        Opcode::Pop => {
            let _ = pop(state, insn.pc)?;
        }
        Opcode::Pop2 => {
            let _ = pop(state, insn.pc)?;
            let _ = pop(state, insn.pc)?;
        }
        Opcode::Dup => {
            let value = pop(state, insn.pc)?;
            push(state, value.clone(), insn.pc, method.max_stack)?;
            push(state, value, insn.pc, method.max_stack)?;
        }
        Opcode::DupX1 => {
            let v1 = pop(state, insn.pc)?;
            let v2 = pop(state, insn.pc)?;
            push(state, v1.clone(), insn.pc, method.max_stack)?;
            push(state, v2, insn.pc, method.max_stack)?;
            push(state, v1, insn.pc, method.max_stack)?;
        }
        Opcode::Dup2 => {
            let v1 = pop(state, insn.pc)?;
            let v2 = pop(state, insn.pc)?;
            push(state, v2.clone(), insn.pc, method.max_stack)?;
            push(state, v1.clone(), insn.pc, method.max_stack)?;
            push(state, v2, insn.pc, method.max_stack)?;
            push(state, v1, insn.pc, method.max_stack)?;
        }
        Opcode::DupX2 => {
            let v1 = pop(state, insn.pc)?;
            let v2 = pop(state, insn.pc)?;
            let v3 = pop(state, insn.pc)?;
            push(state, v1.clone(), insn.pc, method.max_stack)?;
            push(state, v3, insn.pc, method.max_stack)?;
            push(state, v2, insn.pc, method.max_stack)?;
            push(state, v1, insn.pc, method.max_stack)?;
        }
        Opcode::Dup2X1 => {
            let v1 = pop(state, insn.pc)?;
            let v2 = pop(state, insn.pc)?;
            let v3 = pop(state, insn.pc)?;
            push(state, v2.clone(), insn.pc, method.max_stack)?;
            push(state, v1.clone(), insn.pc, method.max_stack)?;
            push(state, v3, insn.pc, method.max_stack)?;
            push(state, v2, insn.pc, method.max_stack)?;
            push(state, v1, insn.pc, method.max_stack)?;
        }
        Opcode::Dup2X2 => {
            let v1 = pop(state, insn.pc)?;
            let v2 = pop(state, insn.pc)?;
            let v3 = pop(state, insn.pc)?;
            let v4 = pop(state, insn.pc)?;
            push(state, v2.clone(), insn.pc, method.max_stack)?;
            push(state, v1.clone(), insn.pc, method.max_stack)?;
            push(state, v4, insn.pc, method.max_stack)?;
            push(state, v3, insn.pc, method.max_stack)?;
            push(state, v2, insn.pc, method.max_stack)?;
            push(state, v1, insn.pc, method.max_stack)?;
        }
        Opcode::Swap => {
            let v1 = pop(state, insn.pc)?;
            let v2 = pop(state, insn.pc)?;
            push(state, v1, insn.pc, method.max_stack)?;
            push(state, v2, insn.pc, method.max_stack)?;
        }

        Opcode::Iadd
        | Opcode::Isub
        | Opcode::Imul
        | Opcode::Idiv
        | Opcode::Irem
        | Opcode::Iand
        | Opcode::Ior
        | Opcode::Ixor
        | Opcode::Ishl
        | Opcode::Ishr
        | Opcode::Iushr => {
            pop_expect_int(state, insn.pc)?;
            pop_expect_int(state, insn.pc)?;
            push(state, VerifyType::Int, insn.pc, method.max_stack)?;
        }
        Opcode::Ladd
        | Opcode::Lsub
        | Opcode::Lmul
        | Opcode::Ldiv
        | Opcode::Lrem
        | Opcode::Land
        | Opcode::Lor
        | Opcode::Lxor
        | Opcode::Lshl
        | Opcode::Lshr
        | Opcode::Lushr => {
            if matches!(insn.opcode, Opcode::Lshl | Opcode::Lshr | Opcode::Lushr) {
                pop_expect_int(state, insn.pc)?;
                pop_expect_long(state, insn.pc)?;
            } else {
                pop_expect_long(state, insn.pc)?;
                pop_expect_long(state, insn.pc)?;
            }
            push(state, VerifyType::Long, insn.pc, method.max_stack)?;
        }
        Opcode::Fadd | Opcode::Fsub | Opcode::Fmul | Opcode::Fdiv | Opcode::Frem => {
            pop_expect_float(state, insn.pc)?;
            pop_expect_float(state, insn.pc)?;
            push(state, VerifyType::Float, insn.pc, method.max_stack)?;
        }
        Opcode::Dadd | Opcode::Dsub | Opcode::Dmul | Opcode::Ddiv | Opcode::Drem => {
            pop_expect_double(state, insn.pc)?;
            pop_expect_double(state, insn.pc)?;
            push(state, VerifyType::Double, insn.pc, method.max_stack)?;
        }
        Opcode::Ineg => {
            pop_expect_int(state, insn.pc)?;
            push(state, VerifyType::Int, insn.pc, method.max_stack)?;
        }
        Opcode::Lneg => {
            pop_expect_long(state, insn.pc)?;
            push(state, VerifyType::Long, insn.pc, method.max_stack)?;
        }
        Opcode::Fneg => {
            pop_expect_float(state, insn.pc)?;
            push(state, VerifyType::Float, insn.pc, method.max_stack)?;
        }
        Opcode::Dneg => {
            pop_expect_double(state, insn.pc)?;
            push(state, VerifyType::Double, insn.pc, method.max_stack)?;
        }
        Opcode::Iinc => {
            let idx = insn.local_index.unwrap();
            let ty = load_local(state, idx, insn.pc)?;
            require_type(insn.pc, &ty, &VerifyType::Int)?;
            let _ = insn.iinc_delta;
        }

        Opcode::I2l | Opcode::F2l | Opcode::D2l => {
            let src = pop(state, insn.pc)?;
            require_one_of(
                insn.pc,
                &src,
                &[VerifyType::Int, VerifyType::Float, VerifyType::Double],
            )?;
            push(state, VerifyType::Long, insn.pc, method.max_stack)?;
        }
        Opcode::I2f | Opcode::L2f | Opcode::D2f => {
            let src = pop(state, insn.pc)?;
            require_one_of(
                insn.pc,
                &src,
                &[VerifyType::Int, VerifyType::Long, VerifyType::Double],
            )?;
            push(state, VerifyType::Float, insn.pc, method.max_stack)?;
        }
        Opcode::I2d | Opcode::L2d | Opcode::F2d => {
            let src = pop(state, insn.pc)?;
            require_one_of(
                insn.pc,
                &src,
                &[VerifyType::Int, VerifyType::Long, VerifyType::Float],
            )?;
            push(state, VerifyType::Double, insn.pc, method.max_stack)?;
        }
        Opcode::L2i | Opcode::F2i | Opcode::D2i | Opcode::I2b | Opcode::I2c | Opcode::I2s => {
            let src = pop(state, insn.pc)?;
            require_one_of(
                insn.pc,
                &src,
                &[
                    VerifyType::Long,
                    VerifyType::Float,
                    VerifyType::Double,
                    VerifyType::Int,
                ],
            )?;
            push(state, VerifyType::Int, insn.pc, method.max_stack)?;
        }

        Opcode::Lcmp => {
            pop_expect_long(state, insn.pc)?;
            pop_expect_long(state, insn.pc)?;
            push(state, VerifyType::Int, insn.pc, method.max_stack)?;
        }
        Opcode::Fcmpl | Opcode::Fcmpg => {
            pop_expect_float(state, insn.pc)?;
            pop_expect_float(state, insn.pc)?;
            push(state, VerifyType::Int, insn.pc, method.max_stack)?;
        }
        Opcode::Dcmpl | Opcode::Dcmpg => {
            pop_expect_double(state, insn.pc)?;
            pop_expect_double(state, insn.pc)?;
            push(state, VerifyType::Int, insn.pc, method.max_stack)?;
        }

        Opcode::Ifeq | Opcode::Ifne | Opcode::Iflt | Opcode::Ifge | Opcode::Ifgt | Opcode::Ifle => {
            pop_expect_int(state, insn.pc)?;
        }
        Opcode::IfIcmpeq
        | Opcode::IfIcmpne
        | Opcode::IfIcmplt
        | Opcode::IfIcmpge
        | Opcode::IfIcmpgt
        | Opcode::IfIcmple => {
            pop_expect_int(state, insn.pc)?;
            pop_expect_int(state, insn.pc)?;
        }
        Opcode::IfAcmpeq | Opcode::IfAcmpne => {
            pop_expect_reference(state, insn.pc)?;
            pop_expect_reference(state, insn.pc)?;
        }
        Opcode::Ifnull | Opcode::Ifnonnull => {
            pop_expect_reference(state, insn.pc)?;
        }
        Opcode::Tableswitch | Opcode::Lookupswitch => {
            pop_expect_int(state, insn.pc)?;
        }
        Opcode::Goto | Opcode::GotoW => {}
        Opcode::Jsr | Opcode::JsrW => {
            push(
                state,
                VerifyType::ReturnAddress(insn.next_pc),
                insn.pc,
                method.max_stack,
            )?;
        }
        Opcode::Ret => {
            let ty = load_local(state, insn.local_index.unwrap(), insn.pc)?;
            match ty {
                VerifyType::ReturnAddress(_) => {}
                _ => {
                    return Err(verification_error(
                        insn.pc,
                        format!("ret expects returnAddress local, got {}", type_name(&ty)),
                    ));
                }
            }
        }

        Opcode::Getstatic => {
            let field = method.field_refs[insn.cp_index.unwrap()].as_ref().unwrap();
            push(
                state,
                parse_field_descriptor(&field.descriptor)?,
                insn.pc,
                method.max_stack,
            )?;
        }
        Opcode::Putstatic => {
            let field = method.field_refs[insn.cp_index.unwrap()].as_ref().unwrap();
            let expected = parse_field_descriptor(&field.descriptor)?;
            let actual = pop(state, insn.pc)?;
            require_assignable(insn.pc, &actual, &expected)?;
        }
        Opcode::Getfield => {
            let field = method.field_refs[insn.cp_index.unwrap()].as_ref().unwrap();
            pop_expect_reference(state, insn.pc)?;
            push(
                state,
                parse_field_descriptor(&field.descriptor)?,
                insn.pc,
                method.max_stack,
            )?;
        }
        Opcode::Putfield => {
            let field = method.field_refs[insn.cp_index.unwrap()].as_ref().unwrap();
            let expected = parse_field_descriptor(&field.descriptor)?;
            let actual = pop(state, insn.pc)?;
            require_assignable(insn.pc, &actual, &expected)?;
            pop_expect_reference(state, insn.pc)?;
        }

        Opcode::Invokevirtual
        | Opcode::Invokestatic
        | Opcode::Invokeinterface
        | Opcode::Invokespecial => {
            let method_ref = method.method_refs[insn.cp_index.unwrap()].as_ref().unwrap();
            let (args, ret) = parse_method_descriptor(&method_ref.descriptor)?;
            for expected in args.iter().rev() {
                let actual = pop(state, insn.pc)?;
                require_assignable(insn.pc, &actual, expected)?;
            }
            if insn.opcode != Opcode::Invokestatic {
                let receiver = pop(state, insn.pc)?;
                if insn.opcode == Opcode::Invokespecial && method_ref.method_name == "<init>" {
                    require_constructor_receiver(insn.pc, &receiver)?;
                    initialize_uninitialized(state, &receiver, &method_ref.class_name);
                } else {
                    require_reference_like(insn.pc, &receiver)?;
                }
            }
            if let Some(ret) = ret {
                push(state, ret, insn.pc, method.max_stack)?;
            }
        }
        Opcode::Invokedynamic => {
            let site = method
                .invoke_dynamic_sites
                .get(insn.cp_index.unwrap())
                .and_then(|x| x.as_ref())
                .ok_or_else(|| verification_error(insn.pc, "invalid invokedynamic site index"))?;
            let (args, ret) = parse_method_descriptor(&site.descriptor)?;
            for expected in args.iter().rev() {
                let actual = pop(state, insn.pc)?;
                require_assignable(insn.pc, &actual, expected)?;
            }
            if let Some(ret) = ret {
                push(state, ret, insn.pc, method.max_stack)?;
            }
        }

        Opcode::New => {
            push(
                state,
                VerifyType::Uninitialized(insn.pc as u16),
                insn.pc,
                method.max_stack,
            )?;
        }
        Opcode::Newarray | Opcode::Anewarray | Opcode::Multianewarray => {
            let dims = insn.dimensions.unwrap_or(1);
            for _ in 0..dims {
                pop_expect_int(state, insn.pc)?;
            }
            push(
                state,
                VerifyType::Reference(None),
                insn.pc,
                method.max_stack,
            )?;
        }
        Opcode::Arraylength => {
            pop_expect_reference(state, insn.pc)?;
            push(state, VerifyType::Int, insn.pc, method.max_stack)?;
        }
        Opcode::Athrow => {
            pop_expect_reference(state, insn.pc)?;
            state.stack.clear();
        }
        Opcode::Checkcast => {
            pop_expect_reference(state, insn.pc)?;
            let target = method.reference_classes[insn.cp_index.unwrap()].clone();
            push(
                state,
                VerifyType::Reference(target),
                insn.pc,
                method.max_stack,
            )?;
        }
        Opcode::Instanceof => {
            pop_expect_reference(state, insn.pc)?;
            push(state, VerifyType::Int, insn.pc, method.max_stack)?;
        }
        Opcode::Monitorenter | Opcode::Monitorexit => {
            pop_expect_reference(state, insn.pc)?;
        }

        Opcode::Ireturn => {
            pop_expect_int(state, insn.pc)?;
            require_return_type(insn.pc, &ret_ty, &VerifyType::Int)?;
            state.stack.clear();
        }
        Opcode::Lreturn => {
            pop_expect_long(state, insn.pc)?;
            require_return_type(insn.pc, &ret_ty, &VerifyType::Long)?;
            state.stack.clear();
        }
        Opcode::Freturn => {
            pop_expect_float(state, insn.pc)?;
            require_return_type(insn.pc, &ret_ty, &VerifyType::Float)?;
            state.stack.clear();
        }
        Opcode::Dreturn => {
            pop_expect_double(state, insn.pc)?;
            require_return_type(insn.pc, &ret_ty, &VerifyType::Double)?;
            state.stack.clear();
        }
        Opcode::Areturn => {
            let actual = pop(state, insn.pc)?;
            require_reference_like(insn.pc, &actual)?;
            let expected = ret_ty
                .as_ref()
                .ok_or_else(|| verification_error(insn.pc, "reference return in void method"))?;
            require_assignable(insn.pc, &actual, expected)?;
            state.stack.clear();
        }
        Opcode::Return => {
            if ret_ty.is_some() {
                return Err(verification_error(
                    insn.pc,
                    "void return in non-void method",
                ));
            }
            state.stack.clear();
        }

        Opcode::Wide => unreachable!(),
    }

    Ok(())
}

fn decode_instruction(code: &[u8], pc: usize) -> Result<DecodedInstruction, VmError> {
    let opcode_byte = *code.get(pc).ok_or(VmError::UnexpectedEof { pc })?;
    let opcode = Opcode::from_byte(opcode_byte).ok_or(VmError::InvalidOpcode {
        opcode: opcode_byte,
        pc,
    })?;
    let mut insn = DecodedInstruction {
        pc,
        next_pc: pc + 1,
        opcode,
        local_index: None,
        cp_index: None,
        branch_targets: Vec::new(),
        iinc_delta: None,
        atype: None,
        dimensions: None,
    };

    match opcode {
        Opcode::Bipush | Opcode::Newarray => {
            insn.next_pc += 1;
            insn.atype = Some(read_u8(code, pc + 1)?);
        }
        Opcode::Ldc
        | Opcode::Iload
        | Opcode::Lload
        | Opcode::Fload
        | Opcode::Dload
        | Opcode::Aload
        | Opcode::Istore
        | Opcode::Lstore
        | Opcode::Fstore
        | Opcode::Dstore
        | Opcode::Astore => {
            let operand = read_u8(code, pc + 1)?;
            insn.next_pc += 1;
            match opcode {
                Opcode::Ldc => insn.cp_index = Some(operand as usize),
                _ => insn.local_index = Some(operand as usize),
            }
        }
        Opcode::Iload0
        | Opcode::Lload0
        | Opcode::Fload0
        | Opcode::Dload0
        | Opcode::Aload0
        | Opcode::Istore0
        | Opcode::Lstore0
        | Opcode::Fstore0
        | Opcode::Dstore0
        | Opcode::Astore0 => insn.local_index = Some(0),
        Opcode::Iload1
        | Opcode::Lload1
        | Opcode::Fload1
        | Opcode::Dload1
        | Opcode::Aload1
        | Opcode::Istore1
        | Opcode::Lstore1
        | Opcode::Fstore1
        | Opcode::Dstore1
        | Opcode::Astore1 => insn.local_index = Some(1),
        Opcode::Iload2
        | Opcode::Lload2
        | Opcode::Fload2
        | Opcode::Dload2
        | Opcode::Aload2
        | Opcode::Istore2
        | Opcode::Lstore2
        | Opcode::Fstore2
        | Opcode::Dstore2
        | Opcode::Astore2 => insn.local_index = Some(2),
        Opcode::Iload3
        | Opcode::Lload3
        | Opcode::Fload3
        | Opcode::Dload3
        | Opcode::Aload3
        | Opcode::Istore3
        | Opcode::Lstore3
        | Opcode::Fstore3
        | Opcode::Dstore3
        | Opcode::Astore3 => insn.local_index = Some(3),
        Opcode::Sipush | Opcode::LdcW | Opcode::Ldc2W => {
            let idx = read_u16(code, pc + 1)? as usize;
            insn.next_pc += 2;
            if opcode != Opcode::Sipush {
                insn.cp_index = Some(idx);
            }
        }
        Opcode::Iinc => {
            insn.local_index = Some(read_u8(code, pc + 1)? as usize);
            insn.iinc_delta = Some(read_i8(code, pc + 2)? as i32);
            insn.next_pc += 2;
        }
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
        | Opcode::Jsr
        | Opcode::Ifnull
        | Opcode::Ifnonnull => {
            let offset = read_i16(code, pc + 1)? as isize;
            insn.next_pc += 2;
            insn.branch_targets
                .push(branch_target(pc, offset, code.len())?);
        }
        Opcode::GotoW | Opcode::JsrW => {
            let offset = read_i32(code, pc + 1)? as isize;
            insn.next_pc += 4;
            insn.branch_targets
                .push(branch_target(pc, offset, code.len())?);
        }
        Opcode::Ret => {
            insn.local_index = Some(read_u8(code, pc + 1)? as usize);
            insn.next_pc += 1;
        }
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
            insn.cp_index = Some(read_u16(code, pc + 1)? as usize);
            insn.next_pc += 2;
        }
        Opcode::Invokeinterface | Opcode::Invokedynamic => {
            insn.cp_index = Some(read_u16(code, pc + 1)? as usize);
            insn.next_pc += 4;
        }
        Opcode::Multianewarray => {
            insn.cp_index = Some(read_u16(code, pc + 1)? as usize);
            insn.dimensions = Some(read_u8(code, pc + 3)?);
            insn.next_pc += 3;
        }
        Opcode::Tableswitch => {
            let mut pos = pc + 1;
            pos += (4 - (pos % 4)) % 4;
            let default = read_i32(code, pos)? as isize;
            let low = read_i32(code, pos + 4)?;
            let high = read_i32(code, pos + 8)?;
            pos += 12;
            insn.branch_targets
                .push(branch_target(pc, default, code.len())?);
            for _ in low..=high {
                let offset = read_i32(code, pos)? as isize;
                insn.branch_targets
                    .push(branch_target(pc, offset, code.len())?);
                pos += 4;
            }
            insn.next_pc = pos;
        }
        Opcode::Lookupswitch => {
            let mut pos = pc + 1;
            pos += (4 - (pos % 4)) % 4;
            let default = read_i32(code, pos)? as isize;
            let npairs = read_i32(code, pos + 4)? as usize;
            pos += 8;
            insn.branch_targets
                .push(branch_target(pc, default, code.len())?);
            for _ in 0..npairs {
                let _key = read_i32(code, pos)?;
                let offset = read_i32(code, pos + 4)? as isize;
                insn.branch_targets
                    .push(branch_target(pc, offset, code.len())?);
                pos += 8;
            }
            insn.next_pc = pos;
        }
        Opcode::Wide => {
            let inner_pc = pc + 1;
            let inner_byte = read_u8(code, inner_pc)?;
            let inner = Opcode::from_byte(inner_byte).ok_or(VmError::InvalidOpcode {
                opcode: inner_byte,
                pc: inner_pc,
            })?;
            let index = read_u16(code, pc + 2)? as usize;
            insn.opcode = inner;
            insn.local_index = Some(index);
            insn.next_pc = pc + 4;
            if inner == Opcode::Iinc {
                insn.iinc_delta = Some(read_i16(code, pc + 4)? as i32);
                insn.next_pc += 2;
            }
        }
        _ => {}
    }

    Ok(insn)
}

fn has_fallthrough(opcode: Opcode) -> bool {
    !matches!(
        opcode,
        Opcode::Goto
            | Opcode::Jsr
            | Opcode::GotoW
            | Opcode::JsrW
            | Opcode::Tableswitch
            | Opcode::Lookupswitch
            | Opcode::Ret
            | Opcode::Ireturn
            | Opcode::Lreturn
            | Opcode::Freturn
            | Opcode::Dreturn
            | Opcode::Areturn
            | Opcode::Return
            | Opcode::Athrow
    )
}

fn merge_state(
    slot: &mut Option<FrameState>,
    mut incoming: FrameState,
    pc: usize,
) -> Result<bool, VmError> {
    match slot {
        None => {
            *slot = Some(incoming);
            Ok(true)
        }
        Some(existing) => {
            if existing.stack.len() != incoming.stack.len() {
                return Err(verification_error(
                    pc,
                    format!(
                        "stack height mismatch across control-flow merge: {} vs {}",
                        existing.stack.len(),
                        incoming.stack.len()
                    ),
                ));
            }
            let max_len = existing.locals.len().max(incoming.locals.len());
            existing.locals.resize(max_len, VerifyType::Top);
            incoming.locals.resize(max_len, VerifyType::Top);

            let mut changed = false;
            for i in 0..max_len {
                let merged =
                    merge_type(&existing.locals[i], &incoming.locals[i]).map_err(|reason| {
                        verification_error(pc, format!("local {i} merge failed: {reason}"))
                    })?;
                if merged != existing.locals[i] {
                    existing.locals[i] = merged;
                    changed = true;
                }
            }
            for i in 0..existing.stack.len() {
                let merged =
                    merge_type(&existing.stack[i], &incoming.stack[i]).map_err(|reason| {
                        verification_error(pc, format!("stack {i} merge failed: {reason}"))
                    })?;
                if merged != existing.stack[i] {
                    existing.stack[i] = merged;
                    changed = true;
                }
            }
            Ok(changed)
        }
    }
}

fn merge_type(left: &VerifyType, right: &VerifyType) -> Result<VerifyType, String> {
    use VerifyType::*;
    Ok(match (left, right) {
        (a, b) if a == b => a.clone(),
        (Top, _) | (_, Top) => Top,
        (ReturnAddress(a), ReturnAddress(b)) if a == b => ReturnAddress(*a),
        (Null, Reference(name)) | (Reference(name), Null) => Reference(name.clone()),
        (Reference(_), Reference(_)) => Reference(None),
        (UninitializedThis, UninitializedThis) => UninitializedThis,
        (Uninitialized(a), Uninitialized(b)) if a == b => Uninitialized(*a),
        (a, b) => {
            return Err(format!(
                "incompatible types {} and {}",
                type_name(a),
                type_name(b)
            ));
        }
    })
}

fn stack_map_type(method: &Method, info: &VerificationTypeInfo) -> Result<VerifyType, VmError> {
    Ok(match info {
        VerificationTypeInfo::Top => VerifyType::Top,
        VerificationTypeInfo::Integer => VerifyType::Int,
        VerificationTypeInfo::Float => VerifyType::Float,
        VerificationTypeInfo::Double => VerifyType::Double,
        VerificationTypeInfo::Long => VerifyType::Long,
        VerificationTypeInfo::Null => VerifyType::Null,
        VerificationTypeInfo::UninitializedThis => VerifyType::UninitializedThis,
        VerificationTypeInfo::Object(index) => {
            let class = method
                .reference_classes
                .get(*index as usize)
                .and_then(|value| value.clone())
                .ok_or_else(|| {
                    verification_error(0, format!("invalid StackMapTable class index {index}"))
                })?;
            VerifyType::Reference(Some(class))
        }
        VerificationTypeInfo::Uninitialized(offset) => VerifyType::Uninitialized(*offset),
    })
}

fn verify_type_from_value(value: Value) -> VerifyType {
    match value {
        Value::Int(_) => VerifyType::Int,
        Value::Long(_) => VerifyType::Long,
        Value::Float(_) => VerifyType::Float,
        Value::Double(_) => VerifyType::Double,
        Value::ReturnAddress(pc) => VerifyType::ReturnAddress(pc),
        Value::Reference(_) => VerifyType::Reference(None),
    }
}

fn constant_type(method: &Method, index: usize, _pc: usize) -> Result<VerifyType, VmError> {
    let value = method
        .constants
        .get(index)
        .ok_or(VmError::InvalidConstantIndex {
            index,
            constant_count: method.constants.len().saturating_sub(1),
        })?
        .ok_or(VmError::InvalidConstantIndex {
            index,
            constant_count: method.constants.len().saturating_sub(1),
        })?;
    Ok(verify_type_from_value(value))
}

fn validate_constant_index(method: &Method, index: usize, pc: usize) -> Result<(), VmError> {
    if method.constants.get(index).and_then(|v| *v).is_none() {
        return Err(verification_error(
            pc,
            format!("invalid constant pool index {index}"),
        ));
    }
    Ok(())
}

fn validate_class_index(method: &Method, index: usize, pc: usize) -> Result<(), VmError> {
    if method
        .reference_classes
        .get(index)
        .and_then(|v| v.as_ref())
        .is_none()
    {
        return Err(verification_error(
            pc,
            format!("invalid class reference index {index}"),
        ));
    }
    Ok(())
}

fn validate_field_ref_index(method: &Method, index: usize, pc: usize) -> Result<(), VmError> {
    if method
        .field_refs
        .get(index)
        .and_then(|v| v.as_ref())
        .is_none()
    {
        return Err(verification_error(
            pc,
            format!("invalid field reference index {index}"),
        ));
    }
    Ok(())
}

fn validate_method_ref_index(method: &Method, index: usize, pc: usize) -> Result<(), VmError> {
    if method
        .method_refs
        .get(index)
        .and_then(|v| v.as_ref())
        .is_none()
    {
        return Err(verification_error(
            pc,
            format!("invalid method reference index {index}"),
        ));
    }
    Ok(())
}

fn validate_invoke_dynamic_index(method: &Method, index: usize, pc: usize) -> Result<(), VmError> {
    if method
        .invoke_dynamic_sites
        .get(index)
        .and_then(|value| value.as_ref())
        .is_none()
    {
        return Err(verification_error(
            pc,
            format!("invalid invokedynamic index {index}"),
        ));
    }
    Ok(())
}

fn parse_method_descriptor(
    descriptor: &str,
) -> Result<(Vec<VerifyType>, Option<VerifyType>), VmError> {
    let bytes = descriptor.as_bytes();
    if bytes.first() != Some(&b'(') {
        return Err(VmError::InvalidDescriptor {
            descriptor: descriptor.to_string(),
        });
    }
    let mut index = 1usize;
    let mut args = Vec::new();
    while bytes.get(index) != Some(&b')') {
        let (ty, next) = parse_field_descriptor_at(descriptor, index)?;
        args.push(ty);
        index = next;
    }
    index += 1;
    if bytes.get(index) == Some(&b'V') {
        Ok((args, None))
    } else {
        let (ret, next) = parse_field_descriptor_at(descriptor, index)?;
        if next != descriptor.len() {
            return Err(VmError::InvalidDescriptor {
                descriptor: descriptor.to_string(),
            });
        }
        Ok((args, Some(ret)))
    }
}

fn parse_field_descriptor(descriptor: &str) -> Result<VerifyType, VmError> {
    let (ty, next) = parse_field_descriptor_at(descriptor, 0)?;
    if next != descriptor.len() {
        return Err(VmError::InvalidDescriptor {
            descriptor: descriptor.to_string(),
        });
    }
    Ok(ty)
}

fn parse_field_descriptor_at(
    descriptor: &str,
    start: usize,
) -> Result<(VerifyType, usize), VmError> {
    let bytes = descriptor.as_bytes();
    match bytes.get(start).copied() {
        Some(b'B' | b'C' | b'I' | b'S' | b'Z') => Ok((VerifyType::Int, start + 1)),
        Some(b'J') => Ok((VerifyType::Long, start + 1)),
        Some(b'F') => Ok((VerifyType::Float, start + 1)),
        Some(b'D') => Ok((VerifyType::Double, start + 1)),
        Some(b'L') => {
            let end = descriptor[start..]
                .find(';')
                .map(|offset| start + offset)
                .ok_or_else(|| VmError::InvalidDescriptor {
                    descriptor: descriptor.to_string(),
                })?;
            Ok((
                VerifyType::Reference(Some(descriptor[start + 1..end].to_string())),
                end + 1,
            ))
        }
        Some(b'[') => {
            let mut end = start + 1;
            while bytes.get(end) == Some(&b'[') {
                end += 1;
            }
            let (_, next) = parse_field_descriptor_at(descriptor, end)?;
            Ok((
                VerifyType::Reference(Some(descriptor[start..next].to_string())),
                next,
            ))
        }
        _ => Err(VmError::InvalidDescriptor {
            descriptor: descriptor.to_string(),
        }),
    }
}

fn require_return_type(
    pc: usize,
    actual: &Option<VerifyType>,
    expected: &VerifyType,
) -> Result<(), VmError> {
    let actual = actual
        .as_ref()
        .ok_or_else(|| verification_error(pc, "typed return in void method"))?;
    require_assignable(pc, expected, actual)
}

fn require_constructor_receiver(pc: usize, ty: &VerifyType) -> Result<(), VmError> {
    match ty {
        VerifyType::UninitializedThis | VerifyType::Uninitialized(_) | VerifyType::Reference(_) => {
            Ok(())
        }
        _ => Err(verification_error(
            pc,
            format!(
                "constructor receiver must be uninitialized or reference, got {}",
                type_name(ty)
            ),
        )),
    }
}

fn initialize_uninitialized(state: &mut FrameState, from: &VerifyType, class_name: &str) {
    let initialized = VerifyType::Reference(Some(class_name.to_string()));
    for local in &mut state.locals {
        if local == from {
            *local = initialized.clone();
        }
    }
    for value in &mut state.stack {
        if value == from {
            *value = initialized.clone();
        }
    }
}

fn require_assignable(
    pc: usize,
    actual: &VerifyType,
    expected: &VerifyType,
) -> Result<(), VmError> {
    if type_compatible(actual, expected) {
        Ok(())
    } else {
        Err(verification_error(
            pc,
            format!(
                "type mismatch: expected {}, got {}",
                type_name(expected),
                type_name(actual)
            ),
        ))
    }
}

fn require_one_of(pc: usize, actual: &VerifyType, expected: &[VerifyType]) -> Result<(), VmError> {
    if expected.iter().any(|candidate| actual == candidate) {
        Ok(())
    } else {
        Err(verification_error(
            pc,
            format!("unexpected type {}", type_name(actual)),
        ))
    }
}

fn require_type(pc: usize, actual: &VerifyType, expected: &VerifyType) -> Result<(), VmError> {
    require_assignable(pc, actual, expected)
}

fn require_reference_like(pc: usize, actual: &VerifyType) -> Result<(), VmError> {
    match actual {
        VerifyType::Null
        | VerifyType::Reference(_)
        | VerifyType::UninitializedThis
        | VerifyType::Uninitialized(_) => Ok(()),
        _ => Err(verification_error(
            pc,
            format!("expected reference, got {}", type_name(actual)),
        )),
    }
}

fn require_astore_type(pc: usize, actual: &VerifyType) -> Result<(), VmError> {
    match actual {
        VerifyType::ReturnAddress(_) => Ok(()),
        _ => require_reference_like(pc, actual),
    }
}

fn type_compatible(actual: &VerifyType, expected: &VerifyType) -> bool {
    match (actual, expected) {
        (a, b) if a == b => true,
        (VerifyType::ReturnAddress(a), VerifyType::ReturnAddress(b)) => a == b,
        (VerifyType::Null, VerifyType::Reference(_)) => true,
        (VerifyType::Reference(_), VerifyType::Reference(_)) => true,
        (VerifyType::UninitializedThis, VerifyType::Reference(_)) => true,
        (VerifyType::Uninitialized(_), VerifyType::Reference(_)) => true,
        _ => false,
    }
}

fn type_name(ty: &VerifyType) -> &str {
    match ty {
        VerifyType::Top => "top",
        VerifyType::Int => "int",
        VerifyType::Float => "float",
        VerifyType::Long => "long",
        VerifyType::Double => "double",
        VerifyType::Null => "null",
        VerifyType::ReturnAddress(_) => "returnAddress",
        VerifyType::Reference(_) => "reference",
        VerifyType::UninitializedThis => "uninitializedThis",
        VerifyType::Uninitialized(_) => "uninitialized",
    }
}

fn ret_target(state: &FrameState, insn: &DecodedInstruction) -> Result<usize, VmError> {
    let ty = load_local(state, insn.local_index.unwrap(), insn.pc)?;
    match ty {
        VerifyType::ReturnAddress(target) => Ok(target),
        _ => Err(verification_error(
            insn.pc,
            format!("ret expects returnAddress local, got {}", type_name(&ty)),
        )),
    }
}

fn load_local(state: &FrameState, index: usize, pc: usize) -> Result<VerifyType, VmError> {
    state
        .locals
        .get(index)
        .cloned()
        .ok_or_else(|| verification_error(pc, format!("local {index} out of bounds")))
        .and_then(|ty| {
            if ty == VerifyType::Top {
                Err(verification_error(
                    pc,
                    format!("local {index} is not initialized"),
                ))
            } else {
                Ok(ty)
            }
        })
}

fn store_local(
    state: &mut FrameState,
    index: usize,
    ty: VerifyType,
    pc: usize,
) -> Result<(), VmError> {
    let slot = state
        .locals
        .get_mut(index)
        .ok_or_else(|| verification_error(pc, format!("local {index} out of bounds")))?;
    *slot = ty;
    Ok(())
}

fn push(
    state: &mut FrameState,
    ty: VerifyType,
    pc: usize,
    max_stack: usize,
) -> Result<(), VmError> {
    if state.stack.len() >= max_stack {
        return Err(verification_error(
            pc,
            format!("stack overflow past max_stack {max_stack}"),
        ));
    }
    state.stack.push(ty);
    Ok(())
}

fn pop(state: &mut FrameState, pc: usize) -> Result<VerifyType, VmError> {
    state
        .stack
        .pop()
        .ok_or_else(|| verification_error(pc, "operand stack underflow"))
}

fn pop_expect_int(state: &mut FrameState, pc: usize) -> Result<(), VmError> {
    let ty = pop(state, pc)?;
    require_type(pc, &ty, &VerifyType::Int)
}

fn pop_expect_long(state: &mut FrameState, pc: usize) -> Result<(), VmError> {
    let ty = pop(state, pc)?;
    require_type(pc, &ty, &VerifyType::Long)
}

fn pop_expect_float(state: &mut FrameState, pc: usize) -> Result<(), VmError> {
    let ty = pop(state, pc)?;
    require_type(pc, &ty, &VerifyType::Float)
}

fn pop_expect_double(state: &mut FrameState, pc: usize) -> Result<(), VmError> {
    let ty = pop(state, pc)?;
    require_type(pc, &ty, &VerifyType::Double)
}

fn pop_expect_reference(state: &mut FrameState, pc: usize) -> Result<(), VmError> {
    let ty = pop(state, pc)?;
    require_reference_like(pc, &ty)
}

fn trimmed(values: &[VerifyType]) -> &[VerifyType] {
    let mut end = values.len();
    while end > 0 && values[end - 1] == VerifyType::Top {
        end -= 1;
    }
    &values[..end]
}

fn branch_target(pc: usize, offset: isize, code_len: usize) -> Result<usize, VmError> {
    let target = pc as isize + offset;
    if target < 0 || target as usize >= code_len {
        return Err(VmError::InvalidBranchTarget { target, code_len });
    }
    Ok(target as usize)
}

fn read_u8(code: &[u8], pos: usize) -> Result<u8, VmError> {
    code.get(pos)
        .copied()
        .ok_or(VmError::UnexpectedEof { pc: pos })
}

fn read_i8(code: &[u8], pos: usize) -> Result<i8, VmError> {
    Ok(read_u8(code, pos)? as i8)
}

fn read_u16(code: &[u8], pos: usize) -> Result<u16, VmError> {
    let b0 = read_u8(code, pos)?;
    let b1 = read_u8(code, pos + 1)?;
    Ok(u16::from_be_bytes([b0, b1]))
}

fn read_i16(code: &[u8], pos: usize) -> Result<i16, VmError> {
    Ok(read_u16(code, pos)? as i16)
}

fn read_i32(code: &[u8], pos: usize) -> Result<i32, VmError> {
    let b0 = read_u8(code, pos)?;
    let b1 = read_u8(code, pos + 1)?;
    let b2 = read_u8(code, pos + 2)?;
    let b3 = read_u8(code, pos + 3)?;
    Ok(i32::from_be_bytes([b0, b1, b2, b3]))
}

fn verification_error(pc: usize, reason: impl Into<String>) -> VmError {
    VmError::VerificationError {
        pc,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classfile::StackMapFrame;
    use crate::vm::Method;

    #[test]
    fn verifies_simple_integer_method() {
        let method = Method::new(vec![0x04, 0xac], 0, 1).with_metadata("Main", "f", "()I", 0x0009);
        verify_method(&method).unwrap();
    }

    #[test]
    fn rejects_type_mismatch_in_return() {
        let method = Method::new(vec![0x04, 0xb0], 0, 1).with_metadata("Main", "f", "()I", 0x0009);
        let error = verify_method(&method).unwrap_err();
        assert!(matches!(error, VmError::VerificationError { .. }));
    }

    #[test]
    fn checks_stack_map_frames() {
        let method = Method::new(vec![0x03, 0x99, 0x00, 0x05, 0x04, 0xac, 0x05, 0xac], 0, 1)
            .with_metadata("Main", "f", "()I", 0x0009)
            .with_stack_map_frames(vec![StackMapFrame {
                offset_delta: 6,
                locals: vec![],
                stack: vec![],
            }]);
        verify_method(&method).unwrap();
    }

    #[test]
    fn verifies_jsr_and_ret() {
        let method = Method::new(
            vec![
                0x08, 0x3b, 0xa8, 0x00, 0x05, 0x1a, 0xac, 0x4c, 0x84, 0x00, 0x01, 0xa9, 0x01,
            ],
            2,
            1,
        )
        .with_metadata("Main", "legacy", "()I", 0x0009);
        verify_method(&method).unwrap();
    }
}
