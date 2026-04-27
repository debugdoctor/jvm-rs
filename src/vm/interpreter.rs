use crate::vm::{Vm, VmError, Thread, Opcode, Value, Reference, ExecutionResult};

#[inline(always)]
pub fn execute_aconst_null(thread: &mut Thread) -> Result<(), VmError> {
    thread.current_frame_mut().push(Value::Reference(Reference::Null))
}

#[inline(always)]
pub fn execute_iconst(thread: &mut Thread, val: i32) -> Result<(), VmError> {
    thread.current_frame_mut().push(Value::Int(val))
}

#[inline(always)]
pub fn execute_lconst(thread: &mut Thread, val: i64) -> Result<(), VmError> {
    thread.current_frame_mut().push(Value::Long(val))
}

#[inline(always)]
pub fn execute_fconst(thread: &mut Thread, val: f32) -> Result<(), VmError> {
    thread.current_frame_mut().push(Value::Float(val))
}

#[inline(always)]
pub fn execute_dconst(thread: &mut Thread, val: f64) -> Result<(), VmError> {
    thread.current_frame_mut().push(Value::Double(val))
}

#[inline(always)]
pub fn execute_bipush(thread: &mut Thread) -> Result<(), VmError> {
    let value = thread.current_frame_mut().read_u8()? as i8 as i32;
    thread.current_frame_mut().push(Value::Int(value))
}

#[inline(always)]
pub fn execute_sipush(thread: &mut Thread) -> Result<(), VmError> {
    let value = thread.current_frame_mut().read_i16()? as i32;
    thread.current_frame_mut().push(Value::Int(value))
}

#[inline(always)]
pub fn execute_iload(thread: &mut Thread, index: usize) -> Result<(), VmError> {
    let value = thread.current_frame().load_local(index)?;
    thread.current_frame_mut().push(value)
}

#[inline(always)]
pub fn execute_lload(thread: &mut Thread, index: usize) -> Result<(), VmError> {
    let value = thread.current_frame().load_local(index)?;
    thread.current_frame_mut().push(value)
}

#[inline(always)]
pub fn execute_fload(thread: &mut Thread, index: usize) -> Result<(), VmError> {
    let value = thread.current_frame().load_local(index)?;
    thread.current_frame_mut().push(value)
}

#[inline(always)]
pub fn execute_dload(thread: &mut Thread, index: usize) -> Result<(), VmError> {
    let value = thread.current_frame().load_local(index)?;
    thread.current_frame_mut().push(value)
}

#[inline(always)]
pub fn execute_aload(thread: &mut Thread, index: usize) -> Result<(), VmError> {
    let value = thread.current_frame().load_local(index)?;
    thread.current_frame_mut().push(value)
}

#[inline(always)]
pub fn execute_istore(thread: &mut Thread, index: usize) -> Result<(), VmError> {
    let value = thread.current_frame_mut().pop()?;
    thread.current_frame_mut().store_local(index, value)
}

#[inline(always)]
pub fn execute_lstore(thread: &mut Thread, index: usize) -> Result<(), VmError> {
    let value = thread.current_frame_mut().pop()?;
    thread.current_frame_mut().store_local(index, value)
}

#[inline(always)]
pub fn execute_fstore(thread: &mut Thread, index: usize) -> Result<(), VmError> {
    let value = thread.current_frame_mut().pop()?;
    thread.current_frame_mut().store_local(index, value)
}

#[inline(always)]
pub fn execute_dstore(thread: &mut Thread, index: usize) -> Result<(), VmError> {
    let value = thread.current_frame_mut().pop()?;
    thread.current_frame_mut().store_local(index, value)
}

#[inline(always)]
pub fn execute_astore(thread: &mut Thread, index: usize) -> Result<(), VmError> {
    let value = thread.current_frame_mut().pop()?;
    thread.current_frame_mut().store_local(index, value)
}

#[inline(always)]
pub fn execute_iadd(thread: &mut Thread) -> Result<(), VmError> {
    let b = thread.current_frame_mut().pop()?.as_int()?;
    let a = thread.current_frame_mut().pop()?.as_int()?;
    thread.current_frame_mut().push(Value::Int(a.wrapping_add(b)))
}

#[inline(always)]
pub fn execute_isub(thread: &mut Thread) -> Result<(), VmError> {
    let b = thread.current_frame_mut().pop()?.as_int()?;
    let a = thread.current_frame_mut().pop()?.as_int()?;
    thread.current_frame_mut().push(Value::Int(a.wrapping_sub(b)))
}

#[inline(always)]
pub fn execute_imul(thread: &mut Thread) -> Result<(), VmError> {
    let b = thread.current_frame_mut().pop()?.as_int()?;
    let a = thread.current_frame_mut().pop()?.as_int()?;
    thread.current_frame_mut().push(Value::Int(a.wrapping_mul(b)))
}

#[inline(always)]
pub fn execute_ireturn_full(thread: &mut Thread) -> Result<Option<ExecutionResult>, VmError> {
    let value = thread.current_frame_mut().pop()?;
    if thread.depth() == 1 {
        return Ok(Some(ExecutionResult::Value(value)));
    }
    thread.pop_frame();
    thread.current_frame_mut().push(value)?;
    Ok(None)
}

#[inline(always)]
pub fn execute_lreturn_full(thread: &mut Thread) -> Result<Option<ExecutionResult>, VmError> {
    let value = thread.current_frame_mut().pop()?;
    if thread.depth() == 1 {
        return Ok(Some(ExecutionResult::Value(value)));
    }
    thread.pop_frame();
    thread.current_frame_mut().push(value)?;
    Ok(None)
}

#[inline(always)]
pub fn execute_areturn_full(thread: &mut Thread) -> Result<Option<ExecutionResult>, VmError> {
    let value = thread.current_frame_mut().pop()?;
    if thread.depth() == 1 {
        return Ok(Some(ExecutionResult::Value(value)));
    }
    thread.pop_frame();
    thread.current_frame_mut().push(value)?;
    Ok(None)
}

#[inline(always)]
pub fn execute_return_full(thread: &mut Thread) -> Result<Option<ExecutionResult>, VmError> {
    if thread.depth() == 1 {
        return Ok(Some(ExecutionResult::Void));
    }
    thread.pop_frame();
    Ok(None)
}

#[inline(always)]
pub fn execute_pop(thread: &mut Thread) -> Result<(), VmError> {
    thread.current_frame_mut().pop()?;
    Ok(())
}

#[inline(always)]
pub fn execute_dup(thread: &mut Thread) -> Result<(), VmError> {
    let v = thread.current_frame_mut().pop()?;
    thread.current_frame_mut().push(v.clone())?;
    thread.current_frame_mut().push(v)
}

#[inline(always)]
pub fn execute_ldc(thread: &mut Thread) -> Result<(), VmError> {
    let index = thread.current_frame_mut().read_u8()? as usize;
    let value = thread.current_frame().load_constant(index)?;
    thread.current_frame_mut().push(value)
}

#[inline(always)]
pub fn execute_ldc_w(thread: &mut Thread) -> Result<(), VmError> {
    let index = thread.current_frame_mut().read_u16()? as usize;
    let value = thread.current_frame().load_constant(index)?;
    thread.current_frame_mut().push(value)
}

#[cfg(test)]
mod tests {
    #[test]
    fn opcode_handlers_compile() {}
}