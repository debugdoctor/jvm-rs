//! Execution frame: per-method program counter, locals, operand stack,
//! and cached resolutions from the constant pool.

use super::types::{
    ClassMethod, ExceptionHandler, FieldRef, InvokeDynamicSite, Method, MethodRef, ResolvedMethod,
    Value, VmError,
};

#[derive(Debug)]
pub(super) struct Frame {
    pub(super) class_name: String,
    pub(super) method_name: String,
    pub(super) descriptor: String,
    pub(super) code: Vec<u8>,
    pub(super) pc: usize,
    pub(super) locals: Vec<Option<Value>>,
    pub(super) stack: Vec<Value>,
    pub(super) max_stack: usize,
    pub(super) constants: Vec<Option<Value>>,
    pub(super) reference_classes: Vec<Option<String>>,
    pub(super) field_refs: Vec<Option<FieldRef>>,
    pub(super) method_refs: Vec<Option<MethodRef>>,
    pub(super) exception_handlers: Vec<ExceptionHandler>,
    #[allow(dead_code)]
    pub(super) line_numbers: Vec<(u16, u16)>,
    pub(super) invoke_dynamic_sites: Vec<Option<InvokeDynamicSite>>,
    pub(super) invoke_cache: Vec<Option<ResolvedMethod>>,
}

impl Frame {
    /// Builds the initial execution frame for a method and seeds any preloaded locals
    /// such as launcher-provided `main` arguments.
    pub(super) fn new(method: Method) -> Self {
        let method_refs_len = method.method_refs.len();
        let mut locals = vec![None; method.max_locals];
        for (index, value) in method.initial_locals.into_iter().enumerate() {
            if index >= locals.len() {
                break;
            }
            locals[index] = value;
        }

        Self {
            class_name: method.class_name,
            method_name: method.name,
            descriptor: method.descriptor,
            code: method.code,
            pc: 0,
            locals,
            stack: Vec::with_capacity(method.max_stack),
            max_stack: method.max_stack,
            constants: method.constants,
            reference_classes: method.reference_classes,
            field_refs: method.field_refs,
            method_refs: method.method_refs,
            exception_handlers: method.exception_handlers,
            line_numbers: method.line_numbers,
            invoke_dynamic_sites: method.invoke_dynamic_sites,
            invoke_cache: vec![None; method_refs_len],
        }
    }

    /// Reads the next byte at the current program counter and advances `pc`.
    pub(super) fn read_u8(&mut self) -> Result<u8, VmError> {
        let byte = self
            .code
            .get(self.pc)
            .copied()
            .ok_or(VmError::UnexpectedEof { pc: self.pc })?;
        self.pc += 1;
        Ok(byte)
    }

    /// Reads a big-endian signed 16-bit immediate from bytecode.
    pub(super) fn read_i16(&mut self) -> Result<i16, VmError> {
        let high = self.read_u8()?;
        let low = self.read_u8()?;
        Ok(i16::from_be_bytes([high, low]))
    }

    /// Reads a big-endian signed 32-bit immediate from bytecode.
    pub(super) fn read_i32(&mut self) -> Result<i32, VmError> {
        let b0 = self.read_u8()?;
        let b1 = self.read_u8()?;
        let b2 = self.read_u8()?;
        let b3 = self.read_u8()?;
        Ok(i32::from_be_bytes([b0, b1, b2, b3]))
    }

    /// Reads a big-endian unsigned 16-bit immediate from bytecode.
    pub(super) fn read_u16(&mut self) -> Result<u16, VmError> {
        let high = self.read_u8()?;
        let low = self.read_u8()?;
        Ok(u16::from_be_bytes([high, low]))
    }

    /// Pushes a value onto the operand stack while enforcing the method's `max_stack`.
    pub(super) fn push(&mut self, value: Value) -> Result<(), VmError> {
        if self.stack.len() >= self.max_stack {
            return Err(VmError::StackOverflow {
                max_stack: self.max_stack,
            });
        }
        self.stack.push(value);
        Ok(())
    }

    /// Pops the top operand stack value, failing if the bytecode underflows the stack.
    pub(super) fn pop(&mut self) -> Result<Value, VmError> {
        self.stack.pop().ok_or(VmError::StackUnderflow)
    }

    /// Loads an initialized local variable slot.
    pub(super) fn load_local(&self, index: usize) -> Result<Value, VmError> {
        let slot = self.locals.get(index).ok_or(VmError::InvalidLocalIndex {
            index,
            max_locals: self.locals.len(),
        })?;
        slot.ok_or(VmError::UninitializedLocal { index })
    }

    /// Stores a value into a local variable slot.
    pub(super) fn store_local(&mut self, index: usize, value: Value) -> Result<(), VmError> {
        let max_locals = self.locals.len();
        let slot = self
            .locals
            .get_mut(index)
            .ok_or(VmError::InvalidLocalIndex { index, max_locals })?;
        *slot = Some(value);
        Ok(())
    }

    /// Resolves an execution-time constant that has already been projected into `Method.constants`.
    pub(super) fn load_constant(&self, index: usize) -> Result<Value, VmError> {
        self.constants
            .get(index)
            .and_then(|value| value.as_ref().copied())
            .ok_or(VmError::InvalidConstantIndex {
                index,
                constant_count: self.constants.len().saturating_sub(1),
            })
    }

    /// Resolves the reference component type used by instructions such as `anewarray`.
    pub(super) fn load_reference_class(&self, index: usize) -> Result<&str, VmError> {
        self.reference_classes
            .get(index)
            .and_then(|value| value.as_deref())
            .ok_or(VmError::InvalidClassConstantIndex {
                index,
                constant_count: self.reference_classes.len().saturating_sub(1),
            })
    }

    /// Resolves a field reference prepared from the class-file constant pool.
    pub(super) fn load_field_ref(&self, index: usize) -> Result<&FieldRef, VmError> {
        self.field_refs
            .get(index)
            .and_then(|value| value.as_ref())
            .ok_or(VmError::InvalidFieldRefIndex {
                index,
                constant_count: self.field_refs.len().saturating_sub(1),
            })
    }

    /// Resolves a method reference prepared from the class-file constant pool.
    pub(super) fn load_method_ref(&self, index: usize) -> Result<&MethodRef, VmError> {
        self.method_refs
            .get(index)
            .and_then(|value| value.as_ref())
            .ok_or(VmError::InvalidMethodRefIndex {
                index,
                constant_count: self.method_refs.len().saturating_sub(1),
            })
    }

    pub(super) fn get_cached_invoke(
        &self,
        index: usize,
        receiver_class: &str,
    ) -> Option<&ClassMethod> {
        let cached = self.invoke_cache.get(index)?;
        let cached = cached.as_ref()?;
        if cached.resolved_class == receiver_class {
            Some(&cached.class_method)
        } else {
            None
        }
    }

    pub(super) fn cache_invoke(
        &mut self,
        index: usize,
        resolved_class: String,
        class_method: ClassMethod,
    ) {
        if let Some(cache_entry) = self.invoke_cache.get_mut(index) {
            *cache_entry = Some(ResolvedMethod {
                resolved_class,
                class_method,
            });
        }
    }

    /// Applies a JVM-style relative branch offset from the current opcode position.
    pub(super) fn branch(&mut self, opcode_pc: usize, offset: i32) -> Result<(), VmError> {
        let target = opcode_pc as isize + offset as isize;
        if !(0..=self.code.len() as isize).contains(&target) {
            return Err(VmError::InvalidBranchTarget {
                target,
                code_len: self.code.len(),
            });
        }
        self.pc = target as usize;
        Ok(())
    }
}
