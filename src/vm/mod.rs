mod builtin;
pub mod verify;

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};

use crate::bytecode::Opcode;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    Reference(Reference),
    ReturnAddress(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reference {
    Null,
    Heap(usize),
}

impl Value {
    fn as_int(self) -> Result<i32, VmError> {
        match self {
            Self::Int(value) => Ok(value),
            other => Err(VmError::TypeMismatch {
                expected: "int",
                actual: other.type_name(),
            }),
        }
    }

    fn as_long(self) -> Result<i64, VmError> {
        match self {
            Self::Long(value) => Ok(value),
            other => Err(VmError::TypeMismatch {
                expected: "long",
                actual: other.type_name(),
            }),
        }
    }

    fn as_float(self) -> Result<f32, VmError> {
        match self {
            Self::Float(value) => Ok(value),
            other => Err(VmError::TypeMismatch {
                expected: "float",
                actual: other.type_name(),
            }),
        }
    }

    fn as_double(self) -> Result<f64, VmError> {
        match self {
            Self::Double(value) => Ok(value),
            other => Err(VmError::TypeMismatch {
                expected: "double",
                actual: other.type_name(),
            }),
        }
    }

    fn as_reference(self) -> Result<Reference, VmError> {
        match self {
            Self::Reference(reference) => Ok(reference),
            other => Err(VmError::TypeMismatch {
                expected: "reference",
                actual: other.type_name(),
            }),
        }
    }

    fn as_return_address(self) -> Result<usize, VmError> {
        match self {
            Self::ReturnAddress(address) => Ok(address),
            other => Err(VmError::TypeMismatch {
                expected: "returnAddress",
                actual: other.type_name(),
            }),
        }
    }

    fn type_name(self) -> &'static str {
        match self {
            Self::Int(_) => "int",
            Self::Long(_) => "long",
            Self::Float(_) => "float",
            Self::Double(_) => "double",
            Self::Reference(_) => "reference",
            Self::ReturnAddress(_) => "returnAddress",
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(v) => write!(f, "{v}"),
            Self::Long(v) => write!(f, "{v}L"),
            Self::Float(v) => write!(f, "{v}f"),
            Self::Double(v) => write!(f, "{v}d"),
            Self::Reference(Reference::Null) => write!(f, "null"),
            Self::Reference(Reference::Heap(i)) => write!(f, "ref@{i}"),
            Self::ReturnAddress(pc) => write!(f, "ret@{pc}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Method {
    pub class_name: String,
    pub name: String,
    pub descriptor: String,
    pub access_flags: u16,
    pub code: Vec<u8>,
    pub max_locals: usize,
    pub max_stack: usize,
    pub constants: Vec<Option<Value>>,
    pub reference_classes: Vec<Option<String>>,
    pub field_refs: Vec<Option<FieldRef>>,
    pub method_refs: Vec<Option<MethodRef>>,
    pub exception_handlers: Vec<ExceptionHandler>,
    pub line_numbers: Vec<(u16, u16)>,
    pub stack_map_frames: Vec<crate::classfile::StackMapFrame>,
    /// InvokeDynamic call site info: `(name, descriptor, bootstrap_index)` keyed by constant pool index.
    pub invoke_dynamic_sites: Vec<Option<InvokeDynamicSite>>,
    pub initial_locals: Vec<Option<Value>>,
}

/// Resolved info for an `invokedynamic` constant pool entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvokeDynamicSite {
    pub name: String,
    pub descriptor: String,
    pub bootstrap_method_index: u16,
    pub kind: InvokeDynamicKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvokeDynamicKind {
    Unknown,
    LambdaProxy {
        target_class: String,
        target_method: String,
        target_descriptor: String,
    },
    StringConcat {
        recipe: Option<String>,
        constants: Vec<String>,
    },
}

/// A single entry from the Code attribute's exception_table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionHandler {
    pub start_pc: u16,
    pub end_pc: u16,
    pub handler_pc: u16,
    /// Class name of the caught exception, or `None` for catch-all (`finally`).
    pub catch_class: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldRef {
    pub class_name: String,
    pub field_name: String,
    pub descriptor: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodRef {
    pub class_name: String,
    pub method_name: String,
    pub descriptor: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ClassInitializationState {
    Initializing(u64),
    Initialized,
}

#[derive(Debug, Default)]
struct RuntimeState {
    classes: BTreeMap<String, RuntimeClass>,
    initialized_classes: BTreeMap<String, ClassInitializationState>,
}

#[derive(Debug, Default)]
struct SharedMonitors {
    states: Mutex<BTreeMap<usize, MonitorState>>,
    changed: Condvar,
}

#[derive(Default)]
struct SharedThreads {
    states: Mutex<BTreeMap<usize, JavaThreadState>>,
}

struct JavaThreadState {
    started: bool,
    handle: Option<JvmThread>,
}

impl fmt::Debug for SharedThreads {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.states.lock().unwrap().len();
        f.debug_struct("SharedThreads")
            .field("thread_count", &count)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeClass {
    pub name: String,
    pub super_class: Option<String>,
    pub methods: BTreeMap<(String, String), ClassMethod>,
    pub static_fields: BTreeMap<String, Value>,
    /// Instance field definitions: (name, descriptor).
    pub instance_fields: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub enum ClassMethod {
    Bytecode(Method),
    Native,
}

impl Method {
    pub fn new(code: impl Into<Vec<u8>>, max_locals: usize, max_stack: usize) -> Self {
        Self::with_constants(code, max_locals, max_stack, [])
    }

    pub fn with_constants(
        code: impl Into<Vec<u8>>,
        max_locals: usize,
        max_stack: usize,
        constants: impl Into<Vec<Value>>,
    ) -> Self {
        let mut constant_pool = vec![None];
        constant_pool.extend(constants.into().into_iter().map(Some));
        Self::with_constant_pool(code, max_locals, max_stack, constant_pool)
    }

    pub fn with_constant_pool(
        code: impl Into<Vec<u8>>,
        max_locals: usize,
        max_stack: usize,
        constants: impl Into<Vec<Option<Value>>>,
    ) -> Self {
        Self {
            class_name: String::new(),
            name: String::new(),
            descriptor: String::new(),
            access_flags: 0,
            code: code.into(),
            max_locals,
            max_stack,
            constants: constants.into(),
            reference_classes: Vec::new(),
            field_refs: Vec::new(),
            method_refs: Vec::new(),
            exception_handlers: Vec::new(),
            line_numbers: Vec::new(),
            stack_map_frames: Vec::new(),
            invoke_dynamic_sites: Vec::new(),
            initial_locals: Vec::new(),
        }
    }

    pub fn with_metadata(
        mut self,
        class_name: impl Into<String>,
        name: impl Into<String>,
        descriptor: impl Into<String>,
        access_flags: u16,
    ) -> Self {
        self.class_name = class_name.into();
        self.name = name.into();
        self.descriptor = descriptor.into();
        self.access_flags = access_flags;
        self
    }

    pub fn with_initial_locals(mut self, locals: impl Into<Vec<Option<Value>>>) -> Self {
        self.initial_locals = locals.into();
        self
    }

    pub fn with_reference_classes(mut self, classes: impl Into<Vec<Option<String>>>) -> Self {
        self.reference_classes = classes.into();
        self
    }

    pub fn with_field_refs(mut self, field_refs: impl Into<Vec<Option<FieldRef>>>) -> Self {
        self.field_refs = field_refs.into();
        self
    }

    pub fn with_method_refs(mut self, method_refs: impl Into<Vec<Option<MethodRef>>>) -> Self {
        self.method_refs = method_refs.into();
        self
    }

    pub fn with_line_numbers(mut self, line_numbers: impl Into<Vec<(u16, u16)>>) -> Self {
        self.line_numbers = line_numbers.into();
        self
    }

    pub fn with_stack_map_frames(
        mut self,
        frames: impl Into<Vec<crate::classfile::StackMapFrame>>,
    ) -> Self {
        self.stack_map_frames = frames.into();
        self
    }

    pub fn with_invoke_dynamic_sites(
        mut self,
        sites: impl Into<Vec<Option<InvokeDynamicSite>>>,
    ) -> Self {
        self.invoke_dynamic_sites = sites.into();
        self
    }

    pub fn with_exception_handlers(
        mut self,
        handlers: impl Into<Vec<ExceptionHandler>>,
    ) -> Self {
        self.exception_handlers = handlers.into();
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionResult {
    Void,
    Value(Value),
}

#[derive(Debug, Clone, PartialEq)]
pub enum VmError {
    StackUnderflow,
    StackOverflow {
        max_stack: usize,
    },
    InvalidLocalIndex {
        index: usize,
        max_locals: usize,
    },
    UninitializedLocal {
        index: usize,
    },
    InvalidOpcode {
        opcode: u8,
        pc: usize,
    },
    UnexpectedEof {
        pc: usize,
    },
    InvalidConstantIndex {
        index: usize,
        constant_count: usize,
    },
    InvalidClassConstantIndex {
        index: usize,
        constant_count: usize,
    },
    InvalidFieldRefIndex {
        index: usize,
        constant_count: usize,
    },
    InvalidMethodRefIndex {
        index: usize,
        constant_count: usize,
    },
    UnsupportedNewArrayType {
        atype: u8,
    },
    ClassNotFound {
        class_name: String,
    },
    FieldNotFound {
        class_name: String,
        field_name: String,
    },
    MethodNotFound {
        class_name: String,
        method_name: String,
        descriptor: String,
    },
    UnsupportedNativeMethod {
        class_name: String,
        method_name: String,
        descriptor: String,
    },
    InvalidDescriptor {
        descriptor: String,
    },
    ClassCastError {
        from: String,
        to: String,
    },
    UnhandledException {
        class_name: String,
    },
    TypeMismatch {
        expected: &'static str,
        actual: &'static str,
    },
    NegativeArraySize {
        size: i32,
    },
    NullReference,
    InvalidHeapReference {
        reference: usize,
    },
    ArrayIndexOutOfBounds {
        index: i32,
        len: usize,
    },
    InvalidHeapValue {
        expected: &'static str,
        actual: &'static str,
    },
    DivisionByZero,
    InvalidBranchTarget {
        target: isize,
        code_len: usize,
    },
    MissingReturn,
    VerificationError {
        pc: usize,
        reason: String,
    },
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StackUnderflow => write!(f, "operand stack underflow"),
            Self::StackOverflow { max_stack } => {
                write!(f, "operand stack overflow (max_stack = {max_stack})")
            }
            Self::InvalidLocalIndex { index, max_locals } => {
                write!(
                    f,
                    "invalid local variable index {index} (max_locals = {max_locals})"
                )
            }
            Self::UninitializedLocal { index } => {
                write!(f, "local variable {index} has not been initialized")
            }
            Self::InvalidOpcode { opcode, pc } => {
                write!(f, "invalid opcode 0x{opcode:02x} at pc {pc}")
            }
            Self::UnexpectedEof { pc } => write!(f, "unexpected end of bytecode at pc {pc}"),
            Self::InvalidConstantIndex {
                index,
                constant_count,
            } => write!(
                f,
                "invalid constant pool index {index} (constant_count = {constant_count})"
            ),
            Self::InvalidClassConstantIndex {
                index,
                constant_count,
            } => write!(
                f,
                "invalid class constant index {index} (constant_count = {constant_count})"
            ),
            Self::InvalidFieldRefIndex {
                index,
                constant_count,
            } => write!(
                f,
                "invalid field reference index {index} (constant_count = {constant_count})"
            ),
            Self::InvalidMethodRefIndex {
                index,
                constant_count,
            } => write!(
                f,
                "invalid method reference index {index} (constant_count = {constant_count})"
            ),
            Self::UnsupportedNewArrayType { atype } => {
                write!(f, "unsupported newarray atype {atype}")
            }
            Self::ClassNotFound { class_name } => {
                write!(f, "class not found: {class_name}")
            }
            Self::FieldNotFound {
                class_name,
                field_name,
            } => write!(f, "field not found: {class_name}.{field_name}"),
            Self::MethodNotFound {
                class_name,
                method_name,
                descriptor,
            } => write!(
                f,
                "method not found: {class_name}.{method_name}{descriptor}"
            ),
            Self::UnsupportedNativeMethod {
                class_name,
                method_name,
                descriptor,
            } => write!(
                f,
                "unsupported native method: {class_name}.{method_name}{descriptor}"
            ),
            Self::InvalidDescriptor { descriptor } => {
                write!(f, "invalid method descriptor: {descriptor}")
            }
            Self::ClassCastError { from, to } => {
                write!(f, "class cast error: {from} cannot be cast to {to}")
            }
            Self::UnhandledException { class_name } => {
                write!(f, "unhandled exception: {class_name}")
            }
            Self::TypeMismatch { expected, actual } => {
                write!(f, "type mismatch: expected {expected}, got {actual}")
            }
            Self::NegativeArraySize { size } => write!(f, "negative array size {size}"),
            Self::NullReference => write!(f, "null reference"),
            Self::InvalidHeapReference { reference } => {
                write!(f, "invalid heap reference {reference}")
            }
            Self::ArrayIndexOutOfBounds { index, len } => {
                write!(f, "array index out of bounds: index {index}, len {len}")
            }
            Self::InvalidHeapValue { expected, actual } => {
                write!(f, "heap value mismatch: expected {expected}, got {actual}")
            }
            Self::DivisionByZero => write!(f, "division by zero"),
            Self::InvalidBranchTarget { target, code_len } => write!(
                f,
                "invalid branch target {target} for bytecode length {code_len}"
            ),
            Self::MissingReturn => write!(f, "method completed without return instruction"),
            Self::VerificationError { pc, reason } => {
                write!(f, "verification failed at pc {pc}: {reason}")
            }
        }
    }
}

impl std::error::Error for VmError {}

#[derive(Debug)]
struct Frame {
    code: Vec<u8>,
    pc: usize,
    locals: Vec<Option<Value>>,
    stack: Vec<Value>,
    max_stack: usize,
    constants: Vec<Option<Value>>,
    reference_classes: Vec<Option<String>>,
    field_refs: Vec<Option<FieldRef>>,
    method_refs: Vec<Option<MethodRef>>,
    exception_handlers: Vec<ExceptionHandler>,
    line_numbers: Vec<(u16, u16)>,
    invoke_dynamic_sites: Vec<Option<InvokeDynamicSite>>,
}

impl Frame {
    /// Builds the initial execution frame for a method and seeds any preloaded locals
    /// such as launcher-provided `main` arguments.
    fn new(method: Method) -> Self {
        let mut locals = vec![None; method.max_locals];
        for (index, value) in method.initial_locals.into_iter().enumerate() {
            if index >= locals.len() {
                break;
            }
            locals[index] = value;
        }

        Self {
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
        }
    }

    /// Reads the next byte at the current program counter and advances `pc`.
    fn read_u8(&mut self) -> Result<u8, VmError> {
        let byte = self
            .code
            .get(self.pc)
            .copied()
            .ok_or(VmError::UnexpectedEof { pc: self.pc })?;
        self.pc += 1;
        Ok(byte)
    }

    /// Reads a big-endian signed 16-bit immediate from bytecode.
    fn read_i16(&mut self) -> Result<i16, VmError> {
        let high = self.read_u8()?;
        let low = self.read_u8()?;
        Ok(i16::from_be_bytes([high, low]))
    }

    /// Reads a big-endian signed 32-bit immediate from bytecode.
    fn read_i32(&mut self) -> Result<i32, VmError> {
        let b0 = self.read_u8()?;
        let b1 = self.read_u8()?;
        let b2 = self.read_u8()?;
        let b3 = self.read_u8()?;
        Ok(i32::from_be_bytes([b0, b1, b2, b3]))
    }

    /// Reads a big-endian unsigned 16-bit immediate from bytecode.
    fn read_u16(&mut self) -> Result<u16, VmError> {
        let high = self.read_u8()?;
        let low = self.read_u8()?;
        Ok(u16::from_be_bytes([high, low]))
    }

    /// Pushes a value onto the operand stack while enforcing the method's `max_stack`.
    fn push(&mut self, value: Value) -> Result<(), VmError> {
        if self.stack.len() >= self.max_stack {
            return Err(VmError::StackOverflow {
                max_stack: self.max_stack,
            });
        }
        self.stack.push(value);
        Ok(())
    }

    /// Pops the top operand stack value, failing if the bytecode underflows the stack.
    fn pop(&mut self) -> Result<Value, VmError> {
        self.stack.pop().ok_or(VmError::StackUnderflow)
    }

    /// Loads an initialized local variable slot.
    fn load_local(&self, index: usize) -> Result<Value, VmError> {
        let slot = self.locals.get(index).ok_or(VmError::InvalidLocalIndex {
            index,
            max_locals: self.locals.len(),
        })?;
        slot.ok_or(VmError::UninitializedLocal { index })
    }

    /// Stores a value into a local variable slot.
    fn store_local(&mut self, index: usize, value: Value) -> Result<(), VmError> {
        let max_locals = self.locals.len();
        let slot = self
            .locals
            .get_mut(index)
            .ok_or(VmError::InvalidLocalIndex { index, max_locals })?;
        *slot = Some(value);
        Ok(())
    }

    /// Resolves an execution-time constant that has already been projected into `Method.constants`.
    fn load_constant(&self, index: usize) -> Result<Value, VmError> {
        self.constants
            .get(index)
            .and_then(|value| value.as_ref().copied())
            .ok_or(VmError::InvalidConstantIndex {
                index,
                constant_count: self.constants.len().saturating_sub(1),
            })
    }

    /// Resolves the reference component type used by instructions such as `anewarray`.
    fn load_reference_class(&self, index: usize) -> Result<&str, VmError> {
        self.reference_classes
            .get(index)
            .and_then(|value| value.as_deref())
            .ok_or(VmError::InvalidClassConstantIndex {
                index,
                constant_count: self.reference_classes.len().saturating_sub(1),
            })
    }

    /// Resolves a field reference prepared from the class-file constant pool.
    fn load_field_ref(&self, index: usize) -> Result<&FieldRef, VmError> {
        self.field_refs
            .get(index)
            .and_then(|value| value.as_ref())
            .ok_or(VmError::InvalidFieldRefIndex {
                index,
                constant_count: self.field_refs.len().saturating_sub(1),
            })
    }

    /// Resolves a method reference prepared from the class-file constant pool.
    fn load_method_ref(&self, index: usize) -> Result<&MethodRef, VmError> {
        self.method_refs
            .get(index)
            .and_then(|value| value.as_ref())
            .ok_or(VmError::InvalidMethodRefIndex {
                index,
                constant_count: self.method_refs.len().saturating_sub(1),
            })
    }

    /// Applies a JVM-style relative branch offset from the current opcode position.
    fn branch(&mut self, opcode_pc: usize, offset: i32) -> Result<(), VmError> {
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

#[derive(Debug, Clone)]
enum HeapValue {
    IntArray {
        values: Vec<i32>,
    },
    ReferenceArray {
        component_type: String,
        values: Vec<Reference>,
    },
    String(String),
    LongArray {
        values: Vec<i64>,
    },
    FloatArray {
        values: Vec<f32>,
    },
    DoubleArray {
        values: Vec<f64>,
    },
    Object {
        class_name: String,
        fields: BTreeMap<String, Value>,
    },
    StringBuilder(std::string::String),
}

impl HeapValue {
    fn kind_name(&self) -> &'static str {
        match self {
            Self::IntArray { .. } => "int-array",
            Self::LongArray { .. } => "long-array",
            Self::FloatArray { .. } => "float-array",
            Self::DoubleArray { .. } => "double-array",
            Self::ReferenceArray { .. } => "reference-array",
            Self::String(_) => "string",
            Self::Object { .. } => "object",
            Self::StringBuilder(_) => "string-builder",
        }
    }
}

#[derive(Debug, Default, Clone)]
struct Heap {
    values: Vec<Option<HeapValue>>,
    /// Number of live objects (approximate, updated by GC).
    live_count: usize,
    /// Number of allocations since last GC.
    allocs_since_gc: usize,
}

impl Heap {
    fn allocate_int_array(&mut self, values: Vec<i32>) -> Reference {
        self.allocate(HeapValue::IntArray { values })
    }

    fn allocate(&mut self, value: HeapValue) -> Reference {
        self.allocs_since_gc += 1;
        // Try to reuse a freed slot.
        for (i, slot) in self.values.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(value);
                return Reference::Heap(i);
            }
        }
        let reference = self.values.len();
        self.values.push(Some(value));
        Reference::Heap(reference)
    }

    fn allocate_string(&mut self, value: impl Into<String>) -> Reference {
        self.allocate(HeapValue::String(value.into()))
    }

    fn allocate_reference_array(
        &mut self,
        component_type: impl Into<String>,
        values: Vec<Reference>,
    ) -> Reference {
        self.allocate(HeapValue::ReferenceArray {
            component_type: component_type.into(),
            values,
        })
    }

    fn get(&self, reference: Reference) -> Result<&HeapValue, VmError> {
        match reference {
            Reference::Null => Err(VmError::NullReference),
            Reference::Heap(index) => self
                .values
                .get(index)
                .and_then(|v| v.as_ref())
                .ok_or(VmError::InvalidHeapReference { reference: index }),
        }
    }

    /// Returns the number of heap slots currently in use.
    fn len(&self) -> usize {
        self.values.iter().filter(|v| v.is_some()).count()
    }

    fn array_length(&self, reference: Reference) -> Result<usize, VmError> {
        match self.get(reference)? {
            HeapValue::IntArray { values } => Ok(values.len()),
            HeapValue::LongArray { values } => Ok(values.len()),
            HeapValue::FloatArray { values } => Ok(values.len()),
            HeapValue::DoubleArray { values } => Ok(values.len()),
            HeapValue::ReferenceArray { values, .. } => Ok(values.len()),
            value => Err(VmError::InvalidHeapValue {
                expected: "array",
                actual: value.kind_name(),
            }),
        }
    }

    fn load_int_array_element(&self, reference: Reference, index: i32) -> Result<i32, VmError> {
        let values = match self.get(reference)? {
            HeapValue::IntArray { values } => values,
            value => {
                return Err(VmError::InvalidHeapValue {
                    expected: "int-array",
                    actual: value.kind_name(),
                });
            }
        };

        let index = usize::try_from(index).map_err(|_| VmError::ArrayIndexOutOfBounds {
            index,
            len: values.len(),
        })?;

        values
            .get(index)
            .copied()
            .ok_or(VmError::ArrayIndexOutOfBounds {
                index: index as i32,
                len: values.len(),
            })
    }

    fn load_reference_array_element(
        &self,
        reference: Reference,
        index: i32,
    ) -> Result<Reference, VmError> {
        let values = match self.get(reference)? {
            HeapValue::ReferenceArray { values, .. } => values,
            value => {
                return Err(VmError::InvalidHeapValue {
                    expected: "reference-array",
                    actual: value.kind_name(),
                });
            }
        };

        let index = usize::try_from(index).map_err(|_| VmError::ArrayIndexOutOfBounds {
            index,
            len: values.len(),
        })?;

        values
            .get(index)
            .copied()
            .ok_or(VmError::ArrayIndexOutOfBounds {
                index: index as i32,
                len: values.len(),
            })
    }

    fn store_reference_array_element(
        &mut self,
        reference: Reference,
        index: i32,
        value: Reference,
    ) -> Result<(), VmError> {
        let values = match self.get_mut(reference)? {
            HeapValue::ReferenceArray { values, .. } => values,
            value => {
                return Err(VmError::InvalidHeapValue {
                    expected: "reference-array",
                    actual: value.kind_name(),
                });
            }
        };

        let index = usize::try_from(index).map_err(|_| VmError::ArrayIndexOutOfBounds {
            index,
            len: values.len(),
        })?;

        let len = values.len();
        let slot = values
            .get_mut(index)
            .ok_or(VmError::ArrayIndexOutOfBounds {
                index: index as i32,
                len,
            })?;
        *slot = value;
        Ok(())
    }

    fn store_int_array_element(
        &mut self,
        reference: Reference,
        index: i32,
        value: i32,
    ) -> Result<(), VmError> {
        let values = match self.get_mut(reference)? {
            HeapValue::IntArray { values } => values,
            value => {
                return Err(VmError::InvalidHeapValue {
                    expected: "int-array",
                    actual: value.kind_name(),
                });
            }
        };

        let index = usize::try_from(index).map_err(|_| VmError::ArrayIndexOutOfBounds {
            index,
            len: values.len(),
        })?;

        let len = values.len();
        let slot = values
            .get_mut(index)
            .ok_or(VmError::ArrayIndexOutOfBounds {
                index: index as i32,
                len,
            })?;
        *slot = value;
        Ok(())
    }

    /// Generic typed array element load.
    fn load_typed_array_element(&self, reference: Reference, index: i32) -> Result<Value, VmError> {
        let heap_val = self.get(reference)?;
        let (value, len) = match heap_val {
            HeapValue::LongArray { values } => {
                let i = Self::check_array_index(index, values.len())?;
                (Value::Long(values[i]), values.len())
            }
            HeapValue::FloatArray { values } => {
                let i = Self::check_array_index(index, values.len())?;
                (Value::Float(values[i]), values.len())
            }
            HeapValue::DoubleArray { values } => {
                let i = Self::check_array_index(index, values.len())?;
                (Value::Double(values[i]), values.len())
            }
            _ => {
                return Err(VmError::InvalidHeapValue {
                    expected: "typed-array",
                    actual: heap_val.kind_name(),
                });
            }
        };
        let _ = len;
        Ok(value)
    }

    /// Generic typed array element store.
    fn store_typed_array_element(
        &mut self,
        reference: Reference,
        index: i32,
        value: Value,
    ) -> Result<(), VmError> {
        let heap_val = self.get_mut(reference)?;
        match (heap_val, value) {
            (HeapValue::LongArray { values }, Value::Long(v)) => {
                let i = Self::check_array_index(index, values.len())?;
                values[i] = v;
            }
            (HeapValue::FloatArray { values }, Value::Float(v)) => {
                let i = Self::check_array_index(index, values.len())?;
                values[i] = v;
            }
            (HeapValue::DoubleArray { values }, Value::Double(v)) => {
                let i = Self::check_array_index(index, values.len())?;
                values[i] = v;
            }
            _ => {
                return Err(VmError::TypeMismatch {
                    expected: "matching array/value type",
                    actual: "mismatched",
                });
            }
        }
        Ok(())
    }

    fn check_array_index(index: i32, len: usize) -> Result<usize, VmError> {
        let i = usize::try_from(index).map_err(|_| VmError::ArrayIndexOutOfBounds {
            index,
            len,
        })?;
        if i >= len {
            return Err(VmError::ArrayIndexOutOfBounds {
                index,
                len,
            });
        }
        Ok(i)
    }

    fn get_mut(&mut self, reference: Reference) -> Result<&mut HeapValue, VmError> {
        match reference {
            Reference::Null => Err(VmError::NullReference),
            Reference::Heap(index) => self
                .values
                .get_mut(index)
                .and_then(|v| v.as_mut())
                .ok_or(VmError::InvalidHeapReference { reference: index }),
        }
    }

    /// Mark-and-sweep garbage collection.
    ///
    /// `roots` must contain every `Reference` reachable from the thread stacks,
    /// static fields, and any other GC roots.
    fn gc(&mut self, roots: &[Reference]) {
        let mut marked = vec![false; self.values.len()];

        // Worklist-based marking.
        let mut worklist: Vec<usize> = roots
            .iter()
            .filter_map(|r| match r {
                Reference::Heap(i) => Some(*i),
                Reference::Null => None,
            })
            .collect();

        while let Some(index) = worklist.pop() {
            if index >= marked.len() || marked[index] {
                continue;
            }
            marked[index] = true;

            // Trace child references.
            if let Some(Some(value)) = self.values.get(index) {
                match value {
                    HeapValue::ReferenceArray { values, .. } => {
                        for r in values {
                            if let Reference::Heap(i) = r {
                                if !marked[*i] {
                                    worklist.push(*i);
                                }
                            }
                        }
                    }
                    HeapValue::Object { fields, .. } => {
                        for v in fields.values() {
                            if let Value::Reference(Reference::Heap(i)) = v {
                                if !marked[*i] {
                                    worklist.push(*i);
                                }
                            }
                        }
                    }
                    HeapValue::IntArray { .. }
                    | HeapValue::LongArray { .. }
                    | HeapValue::FloatArray { .. }
                    | HeapValue::DoubleArray { .. }
                    | HeapValue::String(_)
                    | HeapValue::StringBuilder(_) => {}
                }
            }
        }

        // Sweep: free unmarked objects.
        let mut freed = 0;
        for (i, slot) in self.values.iter_mut().enumerate() {
            if slot.is_some() && !marked[i] {
                *slot = None;
                freed += 1;
            }
        }
        self.live_count = self.values.iter().filter(|v| v.is_some()).count();
        self.allocs_since_gc = 0;

        // Trim trailing None slots.
        while self.values.last().map_or(false, |v| v.is_none()) {
            self.values.pop();
        }

        let _ = freed; // available for tracing if needed
    }
}

#[derive(Debug)]
struct Thread {
    frames: Vec<Frame>,
}

impl Thread {
    fn new(method: Method) -> Self {
        Self {
            frames: vec![Frame::new(method)],
        }
    }

    fn current_frame(&self) -> &Frame {
        self.frames.last().expect("call stack is empty")
    }

    fn current_frame_mut(&mut self) -> &mut Frame {
        self.frames.last_mut().expect("call stack is empty")
    }

    fn push_frame(&mut self, frame: Frame) {
        self.frames.push(frame);
    }

    fn pop_frame(&mut self) -> Frame {
        self.frames.pop().expect("call stack is empty")
    }

    fn depth(&self) -> usize {
        self.frames.len()
    }
}

/// Per-object monitor state for `monitorenter` / `monitorexit`.
#[derive(Debug, Clone, Default)]
struct MonitorState {
    /// Number of times the owning thread has entered this monitor.
    /// Zero means the monitor is free.
    lock_count: usize,
    /// Thread ID of the owner (0 = unowned).
    owner_thread: u64,
    /// Number of threads waiting in `Object.wait()`.
    waiting_threads: usize,
    /// Number of pending notifications that have not yet been consumed by a waiter.
    pending_notifies: usize,
}

/// Handle to a spawned VM thread, allowing the caller to wait for completion.
pub struct JvmThread {
    handle: Option<std::thread::JoinHandle<Result<ExecutionResult, VmError>>>,
}

impl JvmThread {
    /// Block until the thread finishes and return its result.
    pub fn join(mut self) -> Result<ExecutionResult, VmError> {
        self.handle
            .take()
            .expect("thread already joined")
            .join()
            .unwrap_or(Err(VmError::MissingReturn))
    }
}

static NEXT_THREAD_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[derive(Debug, Clone)]
pub struct Vm {
    heap: Arc<Mutex<Heap>>,
    runtime: Arc<Mutex<RuntimeState>>,
    /// Object monitors keyed by heap index.
    monitors: Arc<SharedMonitors>,
    threads: Arc<SharedThreads>,
    class_path: Vec<PathBuf>,
    trace: bool,
    thread_id: u64,
    output: Arc<Mutex<Vec<String>>>,
}

impl Vm {
    pub fn new() -> Self {
        let mut vm = Self {
            heap: Arc::new(Mutex::new(Heap::default())),
            runtime: Arc::new(Mutex::new(RuntimeState::default())),
            monitors: Arc::new(SharedMonitors::default()),
            threads: Arc::new(SharedThreads::default()),
            class_path: Vec::new(),
            trace: false,
            thread_id: NEXT_THREAD_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            output: Arc::new(Mutex::new(Vec::new())),
        };
        vm.bootstrap();
        vm
    }

    /// Enable or disable execution tracing (prints pc, opcode, stack to stderr).
    /// Spawn a new thread that executes the given method.
    ///
    /// The new thread shares heap/monitor/output state with the parent VM,
    /// while method-local execution state remains isolated per thread.
    pub fn spawn(&self, method: Method) -> JvmThread {
        let mut child_vm = self.clone();
        child_vm.thread_id =
            NEXT_THREAD_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let handle = std::thread::spawn(move || child_vm.execute(method));
        JvmThread {
            handle: Some(handle),
        }
    }

    fn spawn_invocation(
        &self,
        start_class: &str,
        method_name: &str,
        descriptor: &str,
        args: Vec<Value>,
    ) -> Result<JvmThread, VmError> {
        let mut child_vm = self.clone();
        child_vm.thread_id =
            NEXT_THREAD_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let start_class = start_class.to_string();
        let method_name = method_name.to_string();
        let descriptor = descriptor.to_string();

        let handle = std::thread::spawn(move || {
            let (resolved_class, class_method) =
                child_vm.resolve_method(&start_class, &method_name, &descriptor)?;
            match class_method {
                ClassMethod::Native => {
                    let result =
                        child_vm.invoke_native(&resolved_class, &method_name, &descriptor, &args)?;
                    Ok(result.map_or(ExecutionResult::Void, ExecutionResult::Value))
                }
                ClassMethod::Bytecode(method) => {
                    let callee = method.with_initial_locals(args.into_iter().map(Some).collect::<Vec<_>>());
                    child_vm.execute(callee)
                }
            }
        });

        Ok(JvmThread {
            handle: Some(handle),
        })
    }

    /// Run garbage collection, freeing unreachable heap objects.
    fn collect_garbage(&mut self, thread: &Thread) {
        let mut roots = Vec::new();

        // Roots from thread frames: stack + locals.
        for frame in &thread.frames {
            for value in &frame.stack {
                if let Value::Reference(r @ Reference::Heap(_)) = value {
                    roots.push(*r);
                }
            }
            for local in &frame.locals {
                if let Some(Value::Reference(r @ Reference::Heap(_))) = local {
                    roots.push(*r);
                }
            }
            // Constants may hold string references.
            for constant in &frame.constants {
                if let Some(Value::Reference(r @ Reference::Heap(_))) = constant {
                    roots.push(*r);
                }
            }
        }

        // Roots from static fields of all loaded classes.
        let runtime = self.runtime.lock().unwrap();
        for class in runtime.classes.values() {
            for value in class.static_fields.values() {
                if let Value::Reference(r @ Reference::Heap(_)) = value {
                    roots.push(*r);
                }
            }
        }

        self.heap.lock().unwrap().gc(&roots);
    }

    pub fn set_trace(&mut self, enabled: bool) {
        self.trace = enabled;
    }

    /// Set the classpath entries used for on-demand class loading.
    pub fn set_class_path(&mut self, paths: Vec<PathBuf>) {
        self.class_path = paths;
    }

    /// Register a class loaded from a `.class` file.
    pub fn register_class(&mut self, class: RuntimeClass) {
        self.runtime
            .lock()
            .unwrap()
            .classes
            .insert(class.name.clone(), class);
    }

    /// Ensure a class is loaded, loading it from the classpath on demand.
    fn ensure_class_loaded(&mut self, class_name: &str) -> Result<(), VmError> {
        if self
            .runtime
            .lock()
            .unwrap()
            .classes
            .contains_key(class_name)
        {
            return Ok(());
        }
        if self.class_path.is_empty() {
            return Err(VmError::ClassNotFound {
                class_name: class_name.to_string(),
            });
        }
        let class_path = self.class_path.clone();
        let source = crate::launcher::resolve_class_path(&class_path, class_name).ok_or_else(
            || VmError::ClassNotFound {
                class_name: class_name.to_string(),
            },
        )?;
        crate::launcher::load_and_register_class_from(&source, class_name, self).map_err(|_| {
            VmError::ClassNotFound {
                class_name: class_name.to_string(),
            }
        })
    }

    /// Run `<clinit>` for a class if it hasn't been initialized yet.
    fn ensure_class_initialized(&mut self, class_name: &str) -> Result<(), VmError> {
        loop {
            enum InitializationAction {
                Wait,
                Run(Option<Method>),
                Done,
            }

            let action = {
                let mut runtime = self.runtime.lock().unwrap();
                match runtime.initialized_classes.get(class_name) {
                    Some(ClassInitializationState::Initialized) => InitializationAction::Done,
                    Some(ClassInitializationState::Initializing(owner))
                        if *owner == self.thread_id =>
                    {
                        InitializationAction::Done
                    }
                    Some(ClassInitializationState::Initializing(_)) => InitializationAction::Wait,
                    None => {
                        runtime.initialized_classes.insert(
                            class_name.to_string(),
                            ClassInitializationState::Initializing(self.thread_id),
                        );
                        let clinit = runtime.classes.get(class_name).and_then(|class| {
                            class
                                .methods
                                .get(&("<clinit>".to_string(), "()V".to_string()))
                                .cloned()
                        });
                        match clinit {
                            Some(ClassMethod::Bytecode(method)) => {
                                InitializationAction::Run(Some(method))
                            }
                            _ => InitializationAction::Run(None),
                        }
                    }
                }
            };

            match action {
                InitializationAction::Done => return Ok(()),
                InitializationAction::Wait => std::thread::yield_now(),
                InitializationAction::Run(clinit) => {
                    let result = if let Some(method) = clinit {
                        self.execute(method).map(|_| ())
                    } else {
                        Ok(())
                    };

                    let mut runtime = self.runtime.lock().unwrap();
                    match result {
                        Ok(()) => {
                            runtime.initialized_classes.insert(
                                class_name.to_string(),
                                ClassInitializationState::Initialized,
                            );
                            return Ok(());
                        }
                        Err(error) => {
                            runtime.initialized_classes.remove(class_name);
                            return Err(error);
                        }
                    }
                }
            }
        }
    }

    fn get_class(&self, class_name: &str) -> Result<RuntimeClass, VmError> {
        self.runtime
            .lock()
            .unwrap()
            .classes
            .get(class_name)
            .cloned()
            .ok_or_else(|| VmError::ClassNotFound {
                class_name: class_name.to_string(),
            })
    }

    fn get_static_field(&self, class_name: &str, field_name: &str) -> Result<Value, VmError> {
        let runtime = self.runtime.lock().unwrap();
        let class = runtime
            .classes
            .get(class_name)
            .ok_or_else(|| VmError::ClassNotFound {
                class_name: class_name.to_string(),
            })?;
        class
            .static_fields
            .get(field_name)
            .copied()
            .ok_or_else(|| VmError::FieldNotFound {
                class_name: class_name.to_string(),
                field_name: field_name.to_string(),
            })
    }

    fn put_static_field(
        &mut self,
        class_name: &str,
        field_name: &str,
        value: Value,
    ) -> Result<(), VmError> {
        let mut runtime = self.runtime.lock().unwrap();
        let class = runtime
            .classes
            .get_mut(class_name)
            .ok_or_else(|| VmError::ClassNotFound {
                class_name: class_name.to_string(),
            })?;
        class.static_fields.insert(field_name.to_string(), value);
        Ok(())
    }

    fn get_object_field(&self, reference: Reference, field_name: &str) -> Result<Value, VmError> {
        let heap = self.heap.lock().unwrap();
        match heap.get(reference)? {
            HeapValue::Object { fields, .. } => {
                Ok(*fields.get(field_name).unwrap_or(&Value::Reference(Reference::Null)))
            }
            value => Err(VmError::InvalidHeapValue {
                expected: "object",
                actual: value.kind_name(),
            }),
        }
    }

    fn set_object_field(
        &mut self,
        reference: Reference,
        field_name: &str,
        value: Value,
    ) -> Result<(), VmError> {
        let mut heap = self.heap.lock().unwrap();
        match heap.get_mut(reference)? {
            HeapValue::Object { fields, .. } => {
                fields.insert(field_name.to_string(), value);
                Ok(())
            }
            value => Err(VmError::InvalidHeapValue {
                expected: "object",
                actual: value.kind_name(),
            }),
        }
    }

    fn start_java_thread(
        &mut self,
        thread_ref: Reference,
        start_class: &str,
        method_name: &str,
        descriptor: &str,
        args: Vec<Value>,
    ) -> Result<(), VmError> {
        let index = match thread_ref {
            Reference::Null => return Err(VmError::NullReference),
            Reference::Heap(index) => index,
        };

        {
            let threads = self.threads.states.lock().unwrap();
            if threads.get(&index).is_some_and(|state| state.started) {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/IllegalThreadStateException".to_string(),
                });
            }
        }

        let handle = self.spawn_invocation(start_class, method_name, descriptor, args)?;
        self.threads.states.lock().unwrap().insert(
            index,
            JavaThreadState {
                started: true,
                handle: Some(handle),
            },
        );
        Ok(())
    }

    fn join_java_thread(&mut self, thread_ref: Reference) -> Result<(), VmError> {
        let index = match thread_ref {
            Reference::Null => return Err(VmError::NullReference),
            Reference::Heap(index) => index,
        };
        let maybe_handle = self
            .threads
            .states
            .lock()
            .unwrap()
            .get_mut(&index)
            .and_then(|state| state.handle.take());
        if let Some(handle) = maybe_handle {
            let _ = handle.join()?;
        }
        Ok(())
    }

    fn stringify_value(&self, value: Value) -> Result<String, VmError> {
        match value {
            Value::Int(v) => Ok(v.to_string()),
            Value::Long(v) => Ok(v.to_string()),
            Value::Float(v) => Ok(format_vm_float(v as f64)),
            Value::Double(v) => Ok(format_vm_float(v)),
            Value::Reference(Reference::Null) => Ok("null".to_string()),
            Value::Reference(reference) => self
                .stringify_reference(reference)
                .or_else(|_| Ok(format!("Object@{reference:?}"))),
            Value::ReturnAddress(pc) => Ok(format!("ret@{pc}")),
        }
    }

    fn build_string_concat(
        &self,
        recipe: Option<&str>,
        constants: &[String],
        args: &[Value],
    ) -> Result<String, VmError> {
        if let Some(recipe) = recipe {
            let mut result = String::new();
            let mut arg_index = 0usize;
            let mut constant_index = 0usize;
            for ch in recipe.chars() {
                match ch {
                    '\u{0001}' => {
                        let value = args.get(arg_index).copied().ok_or_else(|| {
                            VmError::InvalidDescriptor {
                                descriptor: format!("missing invokedynamic concat arg at {arg_index}"),
                            }
                        })?;
                        result.push_str(&self.stringify_value(value)?);
                        arg_index += 1;
                    }
                    '\u{0002}' => {
                        let value = constants.get(constant_index).ok_or_else(|| {
                            VmError::InvalidDescriptor {
                                descriptor: format!(
                                    "missing invokedynamic concat constant at {constant_index}"
                                ),
                            }
                        })?;
                        result.push_str(value);
                        constant_index += 1;
                    }
                    other => result.push(other),
                }
            }
            return Ok(result);
        }

        let mut result = String::new();
        for value in args {
            result.push_str(&self.stringify_value(*value)?);
        }
        Ok(result)
    }

    fn enter_monitor(&self, reference: Reference) -> Result<(), VmError> {
        let index = match reference {
            Reference::Null => return Err(VmError::NullReference),
            Reference::Heap(index) => index,
        };
        let tid = self.thread_id;
        let mut states = self.monitors.states.lock().unwrap();
        loop {
            let monitor = states.entry(index).or_default();
            if monitor.lock_count == 0 || monitor.owner_thread == tid {
                monitor.owner_thread = tid;
                monitor.lock_count += 1;
                return Ok(());
            }
            states = self.monitors.changed.wait(states).unwrap();
        }
    }

    fn exit_monitor(&self, reference: Reference) -> Result<(), VmError> {
        let index = match reference {
            Reference::Null => return Err(VmError::NullReference),
            Reference::Heap(index) => index,
        };
        let tid = self.thread_id;
        let mut states = self.monitors.states.lock().unwrap();
        let monitor = states.entry(index).or_default();
        if monitor.lock_count == 0 || monitor.owner_thread != tid {
            return Err(VmError::UnhandledException {
                class_name: "java/lang/IllegalMonitorStateException".to_string(),
            });
        }
        monitor.lock_count -= 1;
        if monitor.lock_count == 0 {
            monitor.owner_thread = 0;
            if monitor.waiting_threads == 0 && monitor.pending_notifies == 0 {
                states.remove(&index);
            }
            self.monitors.changed.notify_all();
        }
        Ok(())
    }

    fn wait_on_monitor(&self, reference: Reference) -> Result<(), VmError> {
        let index = match reference {
            Reference::Null => return Err(VmError::NullReference),
            Reference::Heap(index) => index,
        };
        let tid = self.thread_id;
        let mut states = self.monitors.states.lock().unwrap();
        let saved_lock_count = {
            let monitor = states.entry(index).or_default();
            if monitor.lock_count == 0 || monitor.owner_thread != tid {
                return Err(VmError::UnhandledException {
                    class_name: "java/lang/IllegalMonitorStateException".to_string(),
                });
            }
            let saved_lock_count = monitor.lock_count;
            monitor.lock_count = 0;
            monitor.owner_thread = 0;
            monitor.waiting_threads += 1;
            saved_lock_count
        };
        self.monitors.changed.notify_all();

        loop {
            states = self.monitors.changed.wait(states).unwrap();
            let monitor = states.entry(index).or_default();
            if monitor.pending_notifies > 0
                && (monitor.lock_count == 0 || monitor.owner_thread == tid)
            {
                monitor.pending_notifies -= 1;
                monitor.waiting_threads -= 1;
                monitor.owner_thread = tid;
                monitor.lock_count = saved_lock_count;
                return Ok(());
            }
        }
    }

    fn notify_monitor(&self, reference: Reference, notify_all: bool) -> Result<(), VmError> {
        let index = match reference {
            Reference::Null => return Err(VmError::NullReference),
            Reference::Heap(index) => index,
        };
        let tid = self.thread_id;
        let mut states = self.monitors.states.lock().unwrap();
        let monitor = states.entry(index).or_default();
        if monitor.lock_count == 0 || monitor.owner_thread != tid {
            return Err(VmError::UnhandledException {
                class_name: "java/lang/IllegalMonitorStateException".to_string(),
            });
        }
        let newly_available = if notify_all {
            monitor.waiting_threads.saturating_sub(monitor.pending_notifies)
        } else if monitor.waiting_threads > monitor.pending_notifies {
            1
        } else {
            0
        };
        monitor.pending_notifies += newly_available;
        if newly_available > 0 {
            self.monitors.changed.notify_all();
        }
        Ok(())
    }

    pub fn new_string(&mut self, value: impl Into<String>) -> Value {
        Value::Reference(self.heap.lock().unwrap().allocate_string(value))
    }

    pub fn new_string_array(&mut self, values: &[String]) -> Value {
        let references = values
            .iter()
            .map(|value| match self.new_string(value.clone()) {
                Value::Reference(reference) => reference,
                _ => unreachable!(),
            })
            .collect();
        Value::Reference(
            self.heap
                .lock()
                .unwrap()
                .allocate_reference_array("java/lang/String", references),
        )
    }

    pub fn take_output(&mut self) -> Vec<String> {
        std::mem::take(&mut self.output.lock().unwrap())
    }

    /// Get the class name of a heap value.
    fn get_object_class(&self, reference: Reference) -> Result<String, VmError> {
        match self.heap.lock().unwrap().get(reference)? {
            HeapValue::Object { class_name, .. } => Ok(class_name.clone()),
            HeapValue::String(_) => Ok("java/lang/String".to_string()),
            HeapValue::StringBuilder(_) => Ok("java/lang/StringBuilder".to_string()),
            HeapValue::IntArray { .. } => Ok("[I".to_string()),
            HeapValue::LongArray { .. } => Ok("[J".to_string()),
            HeapValue::FloatArray { .. } => Ok("[F".to_string()),
            HeapValue::DoubleArray { .. } => Ok("[D".to_string()),
            HeapValue::ReferenceArray { component_type, .. } => {
                Ok(format!("[L{component_type};"))
            }
        }
    }

    /// Verify a method's bytecode structure before execution.
    pub fn verify_method(method: &Method) -> Result<(), VmError> {
        verify::verify_method(method)
    }

    pub fn execute(&mut self, method: Method) -> Result<ExecutionResult, VmError> {
        let mut thread = Thread::new(method);

        loop {
            // Trigger GC when allocation pressure is high.
            if self.heap.lock().unwrap().allocs_since_gc >= 1024 {
                self.collect_garbage(&thread);
            }

            let opcode_pc = thread.current_frame().pc;
            if opcode_pc >= thread.current_frame().code.len() {
                return Err(VmError::MissingReturn);
            }
            let opcode_byte = thread.current_frame_mut().read_u8()?;
            let opcode = Opcode::from_byte(opcode_byte).ok_or(VmError::InvalidOpcode {
                opcode: opcode_byte,
                pc: opcode_pc,
            })?;

            if self.trace {
                let stack_repr: Vec<_> = thread
                    .current_frame()
                    .stack
                    .iter()
                    .map(|v| format!("{v}"))
                    .collect();
                eprintln!(
                    "  pc={opcode_pc:<4} {opcode:?}  stack=[{}]  depth={}",
                    stack_repr.join(", "),
                    thread.depth(),
                );
            }

            match self.execute_opcode(&mut thread, opcode, opcode_pc) {
                Ok(Some(result)) => return Ok(result),
                Ok(None) => {}
                Err(VmError::NullReference) => {
                    self.throw_new_exception(
                        &mut thread,
                        "java/lang/NullPointerException",
                    )?;
                }
                Err(VmError::ArrayIndexOutOfBounds { .. }) => {
                    self.throw_new_exception(
                        &mut thread,
                        "java/lang/ArrayIndexOutOfBoundsException",
                    )?;
                }
                Err(VmError::NegativeArraySize { .. }) => {
                    self.throw_new_exception(
                        &mut thread,
                        "java/lang/NegativeArraySizeException",
                    )?;
                }
                Err(VmError::ClassCastError { .. }) => {
                    self.throw_new_exception(
                        &mut thread,
                        "java/lang/ClassCastException",
                    )?;
                }
                Err(err) => return Err(err),
            }
        }
    }

    /// Execute a single opcode.
    ///
    /// Returns `Ok(Some(result))` when a return instruction terminates the
    /// entry-point method, `Ok(None)` to continue the loop.
    fn execute_opcode(
        &mut self,
        mut thread: &mut Thread,
        opcode: Opcode,
        opcode_pc: usize,
    ) -> Result<Option<ExecutionResult>, VmError> {
            match opcode {
                Opcode::AconstNull => thread
                    .current_frame_mut()
                    .push(Value::Reference(Reference::Null))?,
                Opcode::IconstM1 => thread.current_frame_mut().push(Value::Int(-1))?,
                Opcode::Iconst0 => thread.current_frame_mut().push(Value::Int(0))?,
                Opcode::Iconst1 => thread.current_frame_mut().push(Value::Int(1))?,
                Opcode::Iconst2 => thread.current_frame_mut().push(Value::Int(2))?,
                Opcode::Iconst3 => thread.current_frame_mut().push(Value::Int(3))?,
                Opcode::Iconst4 => thread.current_frame_mut().push(Value::Int(4))?,
                Opcode::Iconst5 => thread.current_frame_mut().push(Value::Int(5))?,
                Opcode::Bipush => {
                    let value = thread.current_frame_mut().read_u8()? as i8 as i32;
                    thread.current_frame_mut().push(Value::Int(value))?;
                }
                Opcode::Sipush => {
                    let value = thread.current_frame_mut().read_i16()? as i32;
                    thread.current_frame_mut().push(Value::Int(value))?;
                }
                Opcode::Ldc => {
                    let index = thread.current_frame_mut().read_u8()? as usize;
                    let value = thread.current_frame().load_constant(index)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::LdcW => {
                    let index = thread.current_frame_mut().read_u16()? as usize;
                    let value = thread.current_frame().load_constant(index)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Ldc2W => {
                    let index = thread.current_frame_mut().read_u16()? as usize;
                    let value = thread.current_frame().load_constant(index)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Lconst0 => thread.current_frame_mut().push(Value::Long(0))?,
                Opcode::Lconst1 => thread.current_frame_mut().push(Value::Long(1))?,
                Opcode::Fconst0 => thread.current_frame_mut().push(Value::Float(0.0))?,
                Opcode::Fconst1 => thread.current_frame_mut().push(Value::Float(1.0))?,
                Opcode::Fconst2 => thread.current_frame_mut().push(Value::Float(2.0))?,
                Opcode::Dconst0 => thread.current_frame_mut().push(Value::Double(0.0))?,
                Opcode::Dconst1 => thread.current_frame_mut().push(Value::Double(1.0))?,
                Opcode::Newarray => {
                    let atype = thread.current_frame_mut().read_u8()?;
                    let count = thread.current_frame_mut().pop()?.as_int()?;
                    if count < 0 {
                        return Err(VmError::NegativeArraySize { size: count });
                    }
                    let n = count as usize;
                    let reference = match atype {
                        4 | 5 | 8 | 9 | 10 => {
                            // boolean(4), char(5), byte(8), short(9), int(10)
                            self.heap.lock().unwrap().allocate_int_array(vec![0; n])
                        }
                        6 => self
                            .heap
                            .lock()
                            .unwrap()
                            .allocate(HeapValue::FloatArray { values: vec![0.0; n] }),
                        7 => self
                            .heap
                            .lock()
                            .unwrap()
                            .allocate(HeapValue::DoubleArray { values: vec![0.0; n] }),
                        11 => self
                            .heap
                            .lock()
                            .unwrap()
                            .allocate(HeapValue::LongArray { values: vec![0; n] }),
                        _ => return Err(VmError::UnsupportedNewArrayType { atype }),
                    };
                    thread.current_frame_mut().push(Value::Reference(reference))?;
                }
                Opcode::Anewarray => {
                    let index = thread.current_frame_mut().read_u16()? as usize;
                    let component_type =
                        thread.current_frame().load_reference_class(index)?.to_string();
                    let count = thread.current_frame_mut().pop()?.as_int()?;
                    if count < 0 {
                        return Err(VmError::NegativeArraySize { size: count });
                    }
                    let values = vec![Reference::Null; count as usize];
                    let reference = self
                        .heap
                        .lock()
                        .unwrap()
                        .allocate_reference_array(component_type, values);
                    thread.current_frame_mut().push(Value::Reference(reference))?;
                }
                Opcode::Aload => {
                    let index = thread.current_frame_mut().read_u8()? as usize;
                    let value = thread.current_frame().load_local(index)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Iload | Opcode::Lload | Opcode::Fload | Opcode::Dload => {
                    let index = thread.current_frame_mut().read_u8()? as usize;
                    let value = thread.current_frame().load_local(index)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Iload0 | Opcode::Lload0 | Opcode::Fload0 | Opcode::Dload0 => {
                    let value = thread.current_frame().load_local(0)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Iload1 | Opcode::Lload1 | Opcode::Fload1 | Opcode::Dload1 => {
                    let value = thread.current_frame().load_local(1)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Iload2 | Opcode::Lload2 | Opcode::Fload2 | Opcode::Dload2 => {
                    let value = thread.current_frame().load_local(2)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Iload3 | Opcode::Lload3 | Opcode::Fload3 | Opcode::Dload3 => {
                    let value = thread.current_frame().load_local(3)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Aload0 => {
                    let value = thread.current_frame().load_local(0)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Aload1 => {
                    let value = thread.current_frame().load_local(1)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Aload2 => {
                    let value = thread.current_frame().load_local(2)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Aload3 => {
                    let value = thread.current_frame().load_local(3)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Iaload => {
                    let index = thread.current_frame_mut().pop()?.as_int()?;
                    let array_ref = thread.current_frame_mut().pop()?.as_reference()?;
                    let value = self
                        .heap
                        .lock()
                        .unwrap()
                        .load_int_array_element(array_ref, index)?;
                    thread.current_frame_mut().push(Value::Int(value))?;
                }
                Opcode::Laload | Opcode::Faload | Opcode::Daload => {
                    let index = thread.current_frame_mut().pop()?.as_int()?;
                    let array_ref = thread.current_frame_mut().pop()?.as_reference()?;
                    let value = self
                        .heap
                        .lock()
                        .unwrap()
                        .load_typed_array_element(array_ref, index)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Baload | Opcode::Caload | Opcode::Saload => {
                    let index = thread.current_frame_mut().pop()?.as_int()?;
                    let array_ref = thread.current_frame_mut().pop()?.as_reference()?;
                    let value = self
                        .heap
                        .lock()
                        .unwrap()
                        .load_int_array_element(array_ref, index)?;
                    thread.current_frame_mut().push(Value::Int(value))?;
                }
                Opcode::Aaload => {
                    let index = thread.current_frame_mut().pop()?.as_int()?;
                    let array_ref = thread.current_frame_mut().pop()?.as_reference()?;
                    let reference = self
                        .heap
                        .lock()
                        .unwrap()
                        .load_reference_array_element(array_ref, index)?;
                    thread.current_frame_mut().push(Value::Reference(reference))?;
                }
                Opcode::Lastore | Opcode::Fastore | Opcode::Dastore => {
                    let value = thread.current_frame_mut().pop()?;
                    let index = thread.current_frame_mut().pop()?.as_int()?;
                    let array_ref = thread.current_frame_mut().pop()?.as_reference()?;
                    self.heap
                        .lock()
                        .unwrap()
                        .store_typed_array_element(array_ref, index, value)?;
                }
                Opcode::Bastore | Opcode::Castore | Opcode::Sastore => {
                    let value = thread.current_frame_mut().pop()?.as_int()?;
                    let index = thread.current_frame_mut().pop()?.as_int()?;
                    let array_ref = thread.current_frame_mut().pop()?.as_reference()?;
                    self.heap
                        .lock()
                        .unwrap()
                        .store_int_array_element(array_ref, index, value)?;
                }
                Opcode::Aastore => {
                    let value = thread.current_frame_mut().pop()?.as_reference()?;
                    let index = thread.current_frame_mut().pop()?.as_int()?;
                    let array_ref = thread.current_frame_mut().pop()?.as_reference()?;
                    self.heap
                        .lock()
                        .unwrap()
                        .store_reference_array_element(array_ref, index, value)?;
                }
                Opcode::Astore => {
                    let index = thread.current_frame_mut().read_u8()? as usize;
                    let value = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().store_local(index, value)?;
                }
                Opcode::Istore | Opcode::Lstore | Opcode::Fstore | Opcode::Dstore => {
                    let index = thread.current_frame_mut().read_u8()? as usize;
                    let value = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().store_local(index, value)?;
                }
                Opcode::Istore0 | Opcode::Lstore0 | Opcode::Fstore0 | Opcode::Dstore0 => {
                    let value = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().store_local(0, value)?;
                }
                Opcode::Istore1 | Opcode::Lstore1 | Opcode::Fstore1 | Opcode::Dstore1 => {
                    let value = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().store_local(1, value)?;
                }
                Opcode::Istore2 | Opcode::Lstore2 | Opcode::Fstore2 | Opcode::Dstore2 => {
                    let value = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().store_local(2, value)?;
                }
                Opcode::Istore3 | Opcode::Lstore3 | Opcode::Fstore3 | Opcode::Dstore3 => {
                    let value = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().store_local(3, value)?;
                }
                Opcode::Astore0 => {
                    let value = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().store_local(0, value)?;
                }
                Opcode::Astore1 => {
                    let value = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().store_local(1, value)?;
                }
                Opcode::Astore2 => {
                    let value = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().store_local(2, value)?;
                }
                Opcode::Astore3 => {
                    let value = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().store_local(3, value)?;
                }
                Opcode::Iastore => {
                    let value = thread.current_frame_mut().pop()?.as_int()?;
                    let index = thread.current_frame_mut().pop()?.as_int()?;
                    let array_ref = thread.current_frame_mut().pop()?.as_reference()?;
                    self.heap
                        .lock()
                        .unwrap()
                        .store_int_array_element(array_ref, index, value)?;
                }
                Opcode::Pop => {
                    let _ = thread.current_frame_mut().pop()?;
                }
                Opcode::Pop2 => {
                    let _ = thread.current_frame_mut().pop()?;
                    let _ = thread.current_frame_mut().pop()?;
                }
                Opcode::Dup => {
                    let value = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().push(value)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::DupX1 => {
                    let top = thread.current_frame_mut().pop()?;
                    let below = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().push(top)?;
                    thread.current_frame_mut().push(below)?;
                    thread.current_frame_mut().push(top)?;
                }
                Opcode::Dup2 => {
                    let top = thread.current_frame_mut().pop()?;
                    let below = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().push(below)?;
                    thread.current_frame_mut().push(top)?;
                    thread.current_frame_mut().push(below)?;
                    thread.current_frame_mut().push(top)?;
                }
                Opcode::DupX2 => {
                    let v1 = thread.current_frame_mut().pop()?;
                    let v2 = thread.current_frame_mut().pop()?;
                    let v3 = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().push(v1)?;
                    thread.current_frame_mut().push(v3)?;
                    thread.current_frame_mut().push(v2)?;
                    thread.current_frame_mut().push(v1)?;
                }
                Opcode::Dup2X1 => {
                    let v1 = thread.current_frame_mut().pop()?;
                    let v2 = thread.current_frame_mut().pop()?;
                    let v3 = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().push(v2)?;
                    thread.current_frame_mut().push(v1)?;
                    thread.current_frame_mut().push(v3)?;
                    thread.current_frame_mut().push(v2)?;
                    thread.current_frame_mut().push(v1)?;
                }
                Opcode::Dup2X2 => {
                    let v1 = thread.current_frame_mut().pop()?;
                    let v2 = thread.current_frame_mut().pop()?;
                    let v3 = thread.current_frame_mut().pop()?;
                    let v4 = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().push(v2)?;
                    thread.current_frame_mut().push(v1)?;
                    thread.current_frame_mut().push(v4)?;
                    thread.current_frame_mut().push(v3)?;
                    thread.current_frame_mut().push(v2)?;
                    thread.current_frame_mut().push(v1)?;
                }
                Opcode::Swap => {
                    let top = thread.current_frame_mut().pop()?;
                    let below = thread.current_frame_mut().pop()?;
                    thread.current_frame_mut().push(top)?;
                    thread.current_frame_mut().push(below)?;
                }
                Opcode::Iadd => {
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    thread.current_frame_mut().push(Value::Int(lhs + rhs))?;
                }
                Opcode::Isub => {
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    thread.current_frame_mut().push(Value::Int(lhs - rhs))?;
                }
                Opcode::Imul => {
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    thread.current_frame_mut().push(Value::Int(lhs * rhs))?;
                }
                Opcode::Idiv => {
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    if rhs == 0 {
                        self.throw_new_exception(
                            &mut thread,
                            "java/lang/ArithmeticException",
                        )?;
                        return Ok(None);
                    }
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    thread.current_frame_mut().push(Value::Int(lhs / rhs))?;
                }
                Opcode::Irem => {
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    if rhs == 0 {
                        self.throw_new_exception(
                            &mut thread,
                            "java/lang/ArithmeticException",
                        )?;
                        return Ok(None);
                    }
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    thread.current_frame_mut().push(Value::Int(lhs % rhs))?;
                }
                Opcode::Ineg => {
                    let value = thread.current_frame_mut().pop()?.as_int()?;
                    thread.current_frame_mut().push(Value::Int(-value))?;
                }
                // --- Long arithmetic ---
                Opcode::Ladd => {
                    let rhs = thread.current_frame_mut().pop()?.as_long()?;
                    let lhs = thread.current_frame_mut().pop()?.as_long()?;
                    thread.current_frame_mut().push(Value::Long(lhs.wrapping_add(rhs)))?;
                }
                Opcode::Lsub => {
                    let rhs = thread.current_frame_mut().pop()?.as_long()?;
                    let lhs = thread.current_frame_mut().pop()?.as_long()?;
                    thread.current_frame_mut().push(Value::Long(lhs.wrapping_sub(rhs)))?;
                }
                Opcode::Lmul => {
                    let rhs = thread.current_frame_mut().pop()?.as_long()?;
                    let lhs = thread.current_frame_mut().pop()?.as_long()?;
                    thread.current_frame_mut().push(Value::Long(lhs.wrapping_mul(rhs)))?;
                }
                Opcode::Ldiv => {
                    let rhs = thread.current_frame_mut().pop()?.as_long()?;
                    if rhs == 0 {
                        self.throw_new_exception(
                            &mut thread,
                            "java/lang/ArithmeticException",
                        )?;
                        return Ok(None);
                    }
                    let lhs = thread.current_frame_mut().pop()?.as_long()?;
                    thread.current_frame_mut().push(Value::Long(lhs / rhs))?;
                }
                Opcode::Lrem => {
                    let rhs = thread.current_frame_mut().pop()?.as_long()?;
                    if rhs == 0 {
                        self.throw_new_exception(
                            &mut thread,
                            "java/lang/ArithmeticException",
                        )?;
                        return Ok(None);
                    }
                    let lhs = thread.current_frame_mut().pop()?.as_long()?;
                    thread.current_frame_mut().push(Value::Long(lhs % rhs))?;
                }
                Opcode::Lneg => {
                    let value = thread.current_frame_mut().pop()?.as_long()?;
                    thread.current_frame_mut().push(Value::Long(-value))?;
                }
                // --- Float arithmetic ---
                Opcode::Fadd => {
                    let rhs = thread.current_frame_mut().pop()?.as_float()?;
                    let lhs = thread.current_frame_mut().pop()?.as_float()?;
                    thread.current_frame_mut().push(Value::Float(lhs + rhs))?;
                }
                Opcode::Fsub => {
                    let rhs = thread.current_frame_mut().pop()?.as_float()?;
                    let lhs = thread.current_frame_mut().pop()?.as_float()?;
                    thread.current_frame_mut().push(Value::Float(lhs - rhs))?;
                }
                Opcode::Fmul => {
                    let rhs = thread.current_frame_mut().pop()?.as_float()?;
                    let lhs = thread.current_frame_mut().pop()?.as_float()?;
                    thread.current_frame_mut().push(Value::Float(lhs * rhs))?;
                }
                Opcode::Fdiv => {
                    let rhs = thread.current_frame_mut().pop()?.as_float()?;
                    let lhs = thread.current_frame_mut().pop()?.as_float()?;
                    thread.current_frame_mut().push(Value::Float(lhs / rhs))?;
                }
                Opcode::Frem => {
                    let rhs = thread.current_frame_mut().pop()?.as_float()?;
                    let lhs = thread.current_frame_mut().pop()?.as_float()?;
                    thread.current_frame_mut().push(Value::Float(lhs % rhs))?;
                }
                Opcode::Fneg => {
                    let value = thread.current_frame_mut().pop()?.as_float()?;
                    thread.current_frame_mut().push(Value::Float(-value))?;
                }
                // --- Double arithmetic ---
                Opcode::Dadd => {
                    let rhs = thread.current_frame_mut().pop()?.as_double()?;
                    let lhs = thread.current_frame_mut().pop()?.as_double()?;
                    thread.current_frame_mut().push(Value::Double(lhs + rhs))?;
                }
                Opcode::Dsub => {
                    let rhs = thread.current_frame_mut().pop()?.as_double()?;
                    let lhs = thread.current_frame_mut().pop()?.as_double()?;
                    thread.current_frame_mut().push(Value::Double(lhs - rhs))?;
                }
                Opcode::Dmul => {
                    let rhs = thread.current_frame_mut().pop()?.as_double()?;
                    let lhs = thread.current_frame_mut().pop()?.as_double()?;
                    thread.current_frame_mut().push(Value::Double(lhs * rhs))?;
                }
                Opcode::Ddiv => {
                    let rhs = thread.current_frame_mut().pop()?.as_double()?;
                    let lhs = thread.current_frame_mut().pop()?.as_double()?;
                    thread.current_frame_mut().push(Value::Double(lhs / rhs))?;
                }
                Opcode::Drem => {
                    let rhs = thread.current_frame_mut().pop()?.as_double()?;
                    let lhs = thread.current_frame_mut().pop()?.as_double()?;
                    thread.current_frame_mut().push(Value::Double(lhs % rhs))?;
                }
                Opcode::Dneg => {
                    let value = thread.current_frame_mut().pop()?.as_double()?;
                    thread.current_frame_mut().push(Value::Double(-value))?;
                }
                Opcode::Ishl => {
                    let rhs = thread.current_frame_mut().pop()?.as_int()? & 0x1f;
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    thread.current_frame_mut().push(Value::Int(lhs << rhs))?;
                }
                Opcode::Ishr => {
                    let rhs = thread.current_frame_mut().pop()?.as_int()? & 0x1f;
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    thread.current_frame_mut().push(Value::Int(lhs >> rhs))?;
                }
                Opcode::Iushr => {
                    let rhs = thread.current_frame_mut().pop()?.as_int()? & 0x1f;
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    thread
                        .current_frame_mut()
                        .push(Value::Int(((lhs as u32) >> rhs) as i32))?;
                }
                Opcode::Iand => {
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    thread.current_frame_mut().push(Value::Int(lhs & rhs))?;
                }
                Opcode::Ior => {
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    thread.current_frame_mut().push(Value::Int(lhs | rhs))?;
                }
                Opcode::Ixor => {
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    thread.current_frame_mut().push(Value::Int(lhs ^ rhs))?;
                }
                Opcode::Lshl => {
                    let rhs = thread.current_frame_mut().pop()?.as_int()? & 0x3f;
                    let lhs = thread.current_frame_mut().pop()?.as_long()?;
                    thread.current_frame_mut().push(Value::Long(lhs << rhs))?;
                }
                Opcode::Lshr => {
                    let rhs = thread.current_frame_mut().pop()?.as_int()? & 0x3f;
                    let lhs = thread.current_frame_mut().pop()?.as_long()?;
                    thread.current_frame_mut().push(Value::Long(lhs >> rhs))?;
                }
                Opcode::Lushr => {
                    let rhs = thread.current_frame_mut().pop()?.as_int()? & 0x3f;
                    let lhs = thread.current_frame_mut().pop()?.as_long()?;
                    thread.current_frame_mut().push(Value::Long(((lhs as u64) >> rhs) as i64))?;
                }
                Opcode::Land => {
                    let rhs = thread.current_frame_mut().pop()?.as_long()?;
                    let lhs = thread.current_frame_mut().pop()?.as_long()?;
                    thread.current_frame_mut().push(Value::Long(lhs & rhs))?;
                }
                Opcode::Lor => {
                    let rhs = thread.current_frame_mut().pop()?.as_long()?;
                    let lhs = thread.current_frame_mut().pop()?.as_long()?;
                    thread.current_frame_mut().push(Value::Long(lhs | rhs))?;
                }
                Opcode::Lxor => {
                    let rhs = thread.current_frame_mut().pop()?.as_long()?;
                    let lhs = thread.current_frame_mut().pop()?.as_long()?;
                    thread.current_frame_mut().push(Value::Long(lhs ^ rhs))?;
                }
                Opcode::Iinc => {
                    let index = thread.current_frame_mut().read_u8()? as usize;
                    let delta = thread.current_frame_mut().read_u8()? as i8 as i32;
                    let value = thread.current_frame().load_local(index)?.as_int()?;
                    thread
                        .current_frame_mut()
                        .store_local(index, Value::Int(value + delta))?;
                }
                Opcode::I2b => {
                    let value = thread.current_frame_mut().pop()?.as_int()?;
                    thread
                        .current_frame_mut()
                        .push(Value::Int(value as i8 as i32))?;
                }
                Opcode::I2c => {
                    let value = thread.current_frame_mut().pop()?.as_int()?;
                    thread
                        .current_frame_mut()
                        .push(Value::Int(value as u16 as i32))?;
                }
                Opcode::I2s => {
                    let value = thread.current_frame_mut().pop()?.as_int()?;
                    thread
                        .current_frame_mut()
                        .push(Value::Int(value as i16 as i32))?;
                }
                // --- Widening / narrowing conversions ---
                Opcode::I2l => {
                    let v = thread.current_frame_mut().pop()?.as_int()?;
                    thread.current_frame_mut().push(Value::Long(v as i64))?;
                }
                Opcode::I2f => {
                    let v = thread.current_frame_mut().pop()?.as_int()?;
                    thread.current_frame_mut().push(Value::Float(v as f32))?;
                }
                Opcode::I2d => {
                    let v = thread.current_frame_mut().pop()?.as_int()?;
                    thread.current_frame_mut().push(Value::Double(v as f64))?;
                }
                Opcode::L2i => {
                    let v = thread.current_frame_mut().pop()?.as_long()?;
                    thread.current_frame_mut().push(Value::Int(v as i32))?;
                }
                Opcode::L2f => {
                    let v = thread.current_frame_mut().pop()?.as_long()?;
                    thread.current_frame_mut().push(Value::Float(v as f32))?;
                }
                Opcode::L2d => {
                    let v = thread.current_frame_mut().pop()?.as_long()?;
                    thread.current_frame_mut().push(Value::Double(v as f64))?;
                }
                Opcode::F2i => {
                    let v = thread.current_frame_mut().pop()?.as_float()?;
                    thread.current_frame_mut().push(Value::Int(v as i32))?;
                }
                Opcode::F2l => {
                    let v = thread.current_frame_mut().pop()?.as_float()?;
                    thread.current_frame_mut().push(Value::Long(v as i64))?;
                }
                Opcode::F2d => {
                    let v = thread.current_frame_mut().pop()?.as_float()?;
                    thread.current_frame_mut().push(Value::Double(v as f64))?;
                }
                Opcode::D2i => {
                    let v = thread.current_frame_mut().pop()?.as_double()?;
                    thread.current_frame_mut().push(Value::Int(v as i32))?;
                }
                Opcode::D2l => {
                    let v = thread.current_frame_mut().pop()?.as_double()?;
                    thread.current_frame_mut().push(Value::Long(v as i64))?;
                }
                Opcode::D2f => {
                    let v = thread.current_frame_mut().pop()?.as_double()?;
                    thread.current_frame_mut().push(Value::Float(v as f32))?;
                }
                // --- Long / float / double comparisons ---
                Opcode::Lcmp => {
                    let rhs = thread.current_frame_mut().pop()?.as_long()?;
                    let lhs = thread.current_frame_mut().pop()?.as_long()?;
                    let result = if lhs > rhs { 1 } else if lhs == rhs { 0 } else { -1 };
                    thread.current_frame_mut().push(Value::Int(result))?;
                }
                Opcode::Fcmpl => {
                    let rhs = thread.current_frame_mut().pop()?.as_float()?;
                    let lhs = thread.current_frame_mut().pop()?.as_float()?;
                    let result = if lhs > rhs { 1 } else if lhs == rhs { 0 } else { -1 };
                    thread.current_frame_mut().push(Value::Int(result))?;
                }
                Opcode::Fcmpg => {
                    let rhs = thread.current_frame_mut().pop()?.as_float()?;
                    let lhs = thread.current_frame_mut().pop()?.as_float()?;
                    let result = if lhs < rhs { -1 } else if lhs == rhs { 0 } else { 1 };
                    thread.current_frame_mut().push(Value::Int(result))?;
                }
                Opcode::Dcmpl => {
                    let rhs = thread.current_frame_mut().pop()?.as_double()?;
                    let lhs = thread.current_frame_mut().pop()?.as_double()?;
                    let result = if lhs > rhs { 1 } else if lhs == rhs { 0 } else { -1 };
                    thread.current_frame_mut().push(Value::Int(result))?;
                }
                Opcode::Dcmpg => {
                    let rhs = thread.current_frame_mut().pop()?.as_double()?;
                    let lhs = thread.current_frame_mut().pop()?.as_double()?;
                    let result = if lhs < rhs { -1 } else if lhs == rhs { 0 } else { 1 };
                    thread.current_frame_mut().push(Value::Int(result))?;
                }
                Opcode::Ifeq => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let value = thread.current_frame_mut().pop()?.as_int()?;
                    if value == 0 {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::Ifne => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let value = thread.current_frame_mut().pop()?.as_int()?;
                    if value != 0 {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::Iflt => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let value = thread.current_frame_mut().pop()?.as_int()?;
                    if value < 0 {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::Ifge => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let value = thread.current_frame_mut().pop()?.as_int()?;
                    if value >= 0 {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::Ifgt => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let value = thread.current_frame_mut().pop()?.as_int()?;
                    if value > 0 {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::Ifle => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let value = thread.current_frame_mut().pop()?.as_int()?;
                    if value <= 0 {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::IfIcmpeq => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    if lhs == rhs {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::IfIcmpne => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    if lhs != rhs {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::IfIcmplt => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    if lhs < rhs {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::IfIcmpge => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    if lhs >= rhs {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::IfIcmpgt => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    if lhs > rhs {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::IfIcmple => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let rhs = thread.current_frame_mut().pop()?.as_int()?;
                    let lhs = thread.current_frame_mut().pop()?.as_int()?;
                    if lhs <= rhs {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::IfAcmpeq => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let rhs = thread.current_frame_mut().pop()?.as_reference()?;
                    let lhs = thread.current_frame_mut().pop()?.as_reference()?;
                    if lhs == rhs {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::IfAcmpne => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let rhs = thread.current_frame_mut().pop()?.as_reference()?;
                    let lhs = thread.current_frame_mut().pop()?.as_reference()?;
                    if lhs != rhs {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::Tableswitch => {
                    // Align pc to a 4-byte boundary (relative to method start).
                    let padding = (4 - (thread.current_frame().pc % 4)) % 4;
                    for _ in 0..padding {
                        thread.current_frame_mut().read_u8()?;
                    }
                    let default = thread.current_frame_mut().read_i32()?;
                    let low = thread.current_frame_mut().read_i32()?;
                    let high = thread.current_frame_mut().read_i32()?;
                    let count = (high - low + 1) as usize;
                    let mut offsets = Vec::with_capacity(count);
                    for _ in 0..count {
                        offsets.push(thread.current_frame_mut().read_i32()?);
                    }
                    let index = thread.current_frame_mut().pop()?.as_int()?;
                    let offset = if index >= low && index <= high {
                        offsets[(index - low) as usize]
                    } else {
                        default
                    };
                    thread.current_frame_mut().branch(opcode_pc, offset)?;
                }
                Opcode::Lookupswitch => {
                    let padding = (4 - (thread.current_frame().pc % 4)) % 4;
                    for _ in 0..padding {
                        thread.current_frame_mut().read_u8()?;
                    }
                    let default = thread.current_frame_mut().read_i32()?;
                    let npairs = thread.current_frame_mut().read_i32()? as usize;
                    let mut pairs = Vec::with_capacity(npairs);
                    for _ in 0..npairs {
                        let key = thread.current_frame_mut().read_i32()?;
                        let offset = thread.current_frame_mut().read_i32()?;
                        pairs.push((key, offset));
                    }
                    let key = thread.current_frame_mut().pop()?.as_int()?;
                    let offset = pairs
                        .iter()
                        .find(|(k, _)| *k == key)
                        .map(|(_, o)| *o)
                        .unwrap_or(default);
                    thread.current_frame_mut().branch(opcode_pc, offset)?;
                }
                Opcode::Goto => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                }
                Opcode::Jsr => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let return_pc = thread.current_frame().pc;
                    thread
                        .current_frame_mut()
                        .push(Value::ReturnAddress(return_pc))?;
                    thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                }
                Opcode::Ret => {
                    let index = thread.current_frame_mut().read_u8()? as usize;
                    let target = thread.current_frame().load_local(index)?.as_return_address()?;
                    if target >= thread.current_frame().code.len() {
                        return Err(VmError::InvalidBranchTarget {
                            target: target as isize,
                            code_len: thread.current_frame().code.len(),
                        });
                    }
                    thread.current_frame_mut().pc = target;
                }
                Opcode::GotoW => {
                    let offset = thread.current_frame_mut().read_i32()?;
                    thread.current_frame_mut().branch(opcode_pc, offset)?;
                }
                Opcode::JsrW => {
                    let offset = thread.current_frame_mut().read_i32()?;
                    let return_pc = thread.current_frame().pc;
                    thread
                        .current_frame_mut()
                        .push(Value::ReturnAddress(return_pc))?;
                    thread.current_frame_mut().branch(opcode_pc, offset)?;
                }

                // --- References: field access ---

                Opcode::Getstatic => {
                    let index = thread.current_frame_mut().read_u16()? as usize;
                    let field_ref = thread.current_frame().load_field_ref(index)?.clone();
                    self.ensure_class_loaded(&field_ref.class_name)?;
                    self.ensure_class_initialized(&field_ref.class_name)?;
                    let value =
                        self.get_static_field(&field_ref.class_name, &field_ref.field_name)?;
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Putstatic => {
                    let index = thread.current_frame_mut().read_u16()? as usize;
                    let field_ref = thread.current_frame().load_field_ref(index)?.clone();
                    let value = thread.current_frame_mut().pop()?;
                    self.ensure_class_loaded(&field_ref.class_name)?;
                    self.ensure_class_initialized(&field_ref.class_name)?;
                    self.put_static_field(&field_ref.class_name, &field_ref.field_name, value)?;
                }
                Opcode::Getfield => {
                    let index = thread.current_frame_mut().read_u16()? as usize;
                    let field_ref = thread.current_frame().load_field_ref(index)?.clone();
                    let object_ref = thread.current_frame_mut().pop()?.as_reference()?;
                    let value = match self.heap.lock().unwrap().get(object_ref)? {
                        HeapValue::Object { fields, .. } => fields
                            .get(&field_ref.field_name)
                            .copied()
                            .ok_or_else(|| VmError::FieldNotFound {
                                class_name: field_ref.class_name.clone(),
                                field_name: field_ref.field_name.clone(),
                            })?,
                        value => {
                            return Err(VmError::InvalidHeapValue {
                                expected: "object",
                                actual: value.kind_name(),
                            })
                        }
                    };
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Putfield => {
                    let index = thread.current_frame_mut().read_u16()? as usize;
                    let field_ref = thread.current_frame().load_field_ref(index)?.clone();
                    let value = thread.current_frame_mut().pop()?;
                    let object_ref = thread.current_frame_mut().pop()?.as_reference()?;
                    match self.heap.lock().unwrap().get_mut(object_ref)? {
                        HeapValue::Object { fields, .. } => {
                            fields.insert(field_ref.field_name, value);
                        }
                        value => {
                            return Err(VmError::InvalidHeapValue {
                                expected: "object",
                                actual: value.kind_name(),
                            })
                        }
                    };
                }

                // --- References: method invocation ---

                Opcode::Invokevirtual => {
                    let index = thread.current_frame_mut().read_u16()? as usize;
                    let method_ref = thread.current_frame().load_method_ref(index)?.clone();
                    let arg_count = parse_arg_count(&method_ref.descriptor)?;

                    let mut args = Vec::with_capacity(arg_count);
                    for _ in 0..arg_count {
                        args.push(thread.current_frame_mut().pop()?);
                    }
                    args.reverse();
                    let receiver = thread.current_frame_mut().pop()?.as_reference()?;

                    let class_name = self.get_object_class(receiver)?;
                    self.dispatch_instance_method(
                        &mut thread,
                        &class_name,
                        &method_ref,
                        receiver,
                        args,
                    )?;
                }
                Opcode::Invokespecial => {
                    let index = thread.current_frame_mut().read_u16()? as usize;
                    let method_ref = thread.current_frame().load_method_ref(index)?.clone();
                    let arg_count = parse_arg_count(&method_ref.descriptor)?;

                    let mut args = Vec::with_capacity(arg_count);
                    for _ in 0..arg_count {
                        args.push(thread.current_frame_mut().pop()?);
                    }
                    args.reverse();
                    let receiver = thread.current_frame_mut().pop()?.as_reference()?;

                    // invokespecial uses the compile-time class, not the runtime class
                    self.dispatch_instance_method(
                        &mut thread,
                        &method_ref.class_name,
                        &method_ref,
                        receiver,
                        args,
                    )?;
                }
                Opcode::Invokestatic => {
                    let index = thread.current_frame_mut().read_u16()? as usize;
                    let method_ref = thread.current_frame().load_method_ref(index)?.clone();
                    let arg_count = parse_arg_count(&method_ref.descriptor)?;

                    let mut args = Vec::with_capacity(arg_count);
                    for _ in 0..arg_count {
                        args.push(thread.current_frame_mut().pop()?);
                    }
                    args.reverse();

                    let class_name = &method_ref.class_name;
                    self.ensure_class_loaded(class_name)?;
                    self.ensure_class_initialized(class_name)?;
                    let class = self.get_class(class_name)?;
                    let class_method = class
                        .methods
                        .get(&(
                            method_ref.method_name.clone(),
                            method_ref.descriptor.clone(),
                        ))
                        .cloned()
                        .ok_or_else(|| VmError::MethodNotFound {
                            class_name: class_name.clone(),
                            method_name: method_ref.method_name.clone(),
                            descriptor: method_ref.descriptor.clone(),
                        })?;

                    match class_method {
                        ClassMethod::Native => {
                            let result = self.invoke_native(
                                class_name,
                                &method_ref.method_name,
                                &method_ref.descriptor,
                                &args,
                            )?;
                            if let Some(value) = result {
                                thread.current_frame_mut().push(value)?;
                            }
                        }
                        ClassMethod::Bytecode(method) => {
                            let initial_locals: Vec<Option<Value>> =
                                args.into_iter().map(Some).collect();
                            let callee = method.with_initial_locals(initial_locals);
                            thread.push_frame(Frame::new(callee));
                        }
                    }
                }

                Opcode::Invokeinterface => {
                    let index = thread.current_frame_mut().read_u16()? as usize;
                    let _count = thread.current_frame_mut().read_u8()?;
                    let _zero = thread.current_frame_mut().read_u8()?;
                    let method_ref = thread.current_frame().load_method_ref(index)?.clone();
                    let arg_count = parse_arg_count(&method_ref.descriptor)?;

                    let mut args = Vec::with_capacity(arg_count);
                    for _ in 0..arg_count {
                        args.push(thread.current_frame_mut().pop()?);
                    }
                    args.reverse();
                    let receiver = thread.current_frame_mut().pop()?.as_reference()?;

                    let class_name = self.get_object_class(receiver)?;
                    self.dispatch_instance_method(
                        &mut thread,
                        &class_name,
                        &method_ref,
                        receiver,
                        args,
                    )?;
                }

                Opcode::Invokedynamic => {
                    let index = thread.current_frame_mut().read_u16()? as usize;
                    let _zero1 = thread.current_frame_mut().read_u8()?;
                    let _zero2 = thread.current_frame_mut().read_u8()?;

                    let site = thread
                        .current_frame()
                        .invoke_dynamic_sites
                        .get(index)
                        .and_then(|s| s.as_ref())
                        .cloned()
                        .ok_or_else(|| VmError::InvalidOpcode {
                            opcode: 0xba,
                            pc: opcode_pc,
                        })?;

                    let arg_count = parse_arg_count(&site.descriptor)?;
                    let mut args = Vec::with_capacity(arg_count);
                    for _ in 0..arg_count {
                        args.push(thread.current_frame_mut().pop()?);
                    }
                    args.reverse();

                    match &site.kind {
                        InvokeDynamicKind::LambdaProxy {
                            target_class,
                            target_method,
                            target_descriptor,
                        } => {
                            // LambdaMetafactory: create a lambda proxy object that stores the
                            // target method reference and any captured arguments.
                            let mut fields = BTreeMap::new();
                            fields.insert(
                                "__target_class".to_string(),
                                Value::Reference(
                                    self.heap
                                        .lock()
                                        .unwrap()
                                        .allocate_string(target_class.clone()),
                                ),
                            );
                            fields.insert(
                                "__target_method".to_string(),
                                Value::Reference(
                                    self.heap
                                        .lock()
                                        .unwrap()
                                        .allocate_string(target_method.clone()),
                                ),
                            );
                            fields.insert(
                                "__target_desc".to_string(),
                                Value::Reference(
                                    self.heap
                                        .lock()
                                        .unwrap()
                                        .allocate_string(target_descriptor.clone()),
                                ),
                            );
                            for (i, val) in args.into_iter().enumerate() {
                                fields.insert(format!("__capture_{i}"), val);
                            }

                            let proxy = self.heap.lock().unwrap().allocate(HeapValue::Object {
                                class_name: format!("__lambda_proxy_{}", site.name),
                                fields,
                            });
                            thread.current_frame_mut().push(Value::Reference(proxy))?;
                        }
                        InvokeDynamicKind::StringConcat { recipe, constants } => {
                            let concat = self.build_string_concat(recipe.as_deref(), constants, &args)?;
                            thread.current_frame_mut().push(self.new_string(concat))?;
                        }
                        InvokeDynamicKind::Unknown => {
                            // Unknown bootstrap method — push null as placeholder.
                            thread
                                .current_frame_mut()
                                .push(Value::Reference(Reference::Null))?;
                        }
                    }
                }

                // --- Monitors ---

                Opcode::Monitorenter => {
                    let obj_ref = thread.current_frame_mut().pop()?.as_reference()?;
                    self.enter_monitor(obj_ref)?;
                }
                Opcode::Monitorexit => {
                    let obj_ref = thread.current_frame_mut().pop()?.as_reference()?;
                    match self.exit_monitor(obj_ref) {
                        Ok(()) => {}
                        Err(VmError::UnhandledException { class_name }) => {
                            self.throw_new_exception(&mut thread, &class_name)?;
                            return Ok(None);
                        }
                        Err(error) => return Err(error),
                    }
                }

                // --- References: object creation ---

                Opcode::New => {
                    let index = thread.current_frame_mut().read_u16()? as usize;
                    let class_name =
                        thread.current_frame().load_reference_class(index)?.to_string();
                    self.ensure_class_loaded(&class_name)?;
                    self.ensure_class_initialized(&class_name)?;
                    let instance_fields = self.get_class(&class_name)?.instance_fields;
                    let mut fields = BTreeMap::new();
                    for (name, descriptor) in instance_fields {
                        fields.insert(name, default_value_for_descriptor(&descriptor));
                    }
                    let reference = self.heap.lock().unwrap().allocate(HeapValue::Object {
                        class_name,
                        fields,
                    });
                    thread.current_frame_mut().push(Value::Reference(reference))?;
                }
                Opcode::Athrow => {
                    let exception_ref = thread.current_frame_mut().pop()?.as_reference()?;
                    if exception_ref == Reference::Null {
                        return Err(VmError::NullReference);
                    }
                    self.throw_exception(&mut thread, exception_ref)?;
                }
                Opcode::Multianewarray => {
                    let index = thread.current_frame_mut().read_u16()? as usize;
                    let _class_name =
                        thread.current_frame().load_reference_class(index)?.to_string();
                    let dimensions = thread.current_frame_mut().read_u8()? as usize;
                    let mut counts = Vec::with_capacity(dimensions);
                    for _ in 0..dimensions {
                        counts.push(thread.current_frame_mut().pop()?.as_int()?);
                    }
                    counts.reverse();
                    let reference =
                        self.allocate_multi_array(&counts, 0)?;
                    thread.current_frame_mut().push(Value::Reference(reference))?;
                }
                Opcode::Wide => {
                    let inner_byte = thread.current_frame_mut().read_u8()?;
                    let inner = Opcode::from_byte(inner_byte).ok_or(VmError::InvalidOpcode {
                        opcode: inner_byte,
                        pc: opcode_pc,
                    })?;
                    let index = thread.current_frame_mut().read_u16()? as usize;
                    match inner {
                        Opcode::Iload | Opcode::Lload | Opcode::Fload
                        | Opcode::Dload | Opcode::Aload => {
                            let value = thread.current_frame().load_local(index)?;
                            thread.current_frame_mut().push(value)?;
                        }
                        Opcode::Istore | Opcode::Lstore | Opcode::Fstore
                        | Opcode::Dstore | Opcode::Astore => {
                            let value = thread.current_frame_mut().pop()?;
                            thread.current_frame_mut().store_local(index, value)?;
                        }
                        Opcode::Iinc => {
                            let delta = thread.current_frame_mut().read_i16()? as i32;
                            let value =
                                thread.current_frame().load_local(index)?.as_int()?;
                            thread
                                .current_frame_mut()
                                .store_local(index, Value::Int(value + delta))?;
                        }
                        Opcode::Ret => {
                            let target =
                                thread.current_frame().load_local(index)?.as_return_address()?;
                            if target >= thread.current_frame().code.len() {
                                return Err(VmError::InvalidBranchTarget {
                                    target: target as isize,
                                    code_len: thread.current_frame().code.len(),
                                });
                            }
                            thread.current_frame_mut().pc = target;
                        }
                        _ => {
                            return Err(VmError::InvalidOpcode {
                                opcode: inner_byte,
                                pc: opcode_pc,
                            });
                        }
                    }
                }
                Opcode::Checkcast => {
                    let index = thread.current_frame_mut().read_u16()? as usize;
                    let target =
                        thread.current_frame().load_reference_class(index)?.to_string();
                    let value = thread.current_frame_mut().pop()?;
                    let reference = value.as_reference()?;
                    if reference != Reference::Null {
                        let obj_class = self.get_object_class(reference)?;
                        if !self.is_instance_of(&obj_class, &target)? {
                            return Err(VmError::ClassCastError {
                                from: obj_class,
                                to: target,
                            });
                        }
                    }
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Instanceof => {
                    let index = thread.current_frame_mut().read_u16()? as usize;
                    let target =
                        thread.current_frame().load_reference_class(index)?.to_string();
                    let reference = thread.current_frame_mut().pop()?.as_reference()?;
                    let result = if reference == Reference::Null {
                        0
                    } else {
                        let obj_class = self.get_object_class(reference)?;
                        if self.is_instance_of(&obj_class, &target)? {
                            1
                        } else {
                            0
                        }
                    };
                    thread.current_frame_mut().push(Value::Int(result))?;
                }

                // --- Control: null checks ---

                Opcode::Ifnull => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let reference = thread.current_frame_mut().pop()?.as_reference()?;
                    if reference == Reference::Null {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }
                Opcode::Ifnonnull => {
                    let offset = thread.current_frame_mut().read_i16()?;
                    let reference = thread.current_frame_mut().pop()?.as_reference()?;
                    if reference != Reference::Null {
                        thread.current_frame_mut().branch(opcode_pc, offset.into())?;
                    }
                }

                // --- Control: returns ---

                Opcode::Areturn | Opcode::Ireturn | Opcode::Lreturn
                | Opcode::Freturn | Opcode::Dreturn => {
                    let value = thread.current_frame_mut().pop()?;
                    if thread.depth() == 1 {
                        return Ok(Some(ExecutionResult::Value(value)));
                    }
                    thread.pop_frame();
                    thread.current_frame_mut().push(value)?;
                }
                Opcode::Return => {
                    if thread.depth() == 1 {
                        return Ok(Some(ExecutionResult::Void));
                    }
                    thread.pop_frame();
                }

                Opcode::Arraylength => {
                    let reference = thread.current_frame_mut().pop()?.as_reference()?;
                    let length = self.heap.lock().unwrap().array_length(reference)?;
                    thread.current_frame_mut().push(Value::Int(length as i32))?;
                }
            }
            Ok(None)
    }

    /// Recursively allocate a multi-dimensional array.
    ///
    /// `depth` must be `< counts.len()` — guaranteed by the caller which reads
    /// `dimensions` (≥ 1 per JVMS) from bytecode and builds `counts` with
    /// exactly that many entries.  The recursion terminates at
    /// `depth + 1 == counts.len()`.
    fn allocate_multi_array(
        &mut self,
        counts: &[i32],
        depth: usize,
    ) -> Result<Reference, VmError> {
        let count = counts[depth];
        if count < 0 {
            return Err(VmError::NegativeArraySize { size: count });
        }
        let n = count as usize;
        if depth + 1 == counts.len() {
            // Innermost dimension — allocate an int array (common case for int[][])
            Ok(self.heap.lock().unwrap().allocate_int_array(vec![0; n]))
        } else {
            // Allocate sub-arrays recursively
            let mut elements = Vec::with_capacity(n);
            for _ in 0..n {
                let sub = self.allocate_multi_array(counts, depth + 1)?;
                elements.push(sub);
            }
            Ok(self
                .heap
                .lock()
                .unwrap()
                .allocate_reference_array("array", elements))
        }
    }

    /// Create a new exception object and throw it.
    fn throw_new_exception(
        &mut self,
        thread: &mut Thread,
        class_name: &str,
    ) -> Result<(), VmError> {
        let reference = self.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: class_name.to_string(),
            fields: BTreeMap::new(),
        });
        self.throw_exception(thread, reference)
    }

    /// Propagate an exception through the call stack, searching for a matching handler.
    ///
    /// If a handler is found the current frame's stack is cleared, the exception
    /// reference is pushed, and `pc` jumps to the handler.  If no handler matches
    /// in any frame, an `UnhandledException` error is returned.
    fn throw_exception(
        &mut self,
        thread: &mut Thread,
        exception_ref: Reference,
    ) -> Result<(), VmError> {
        let exception_class = self.get_object_class(exception_ref)?;

        loop {
            let pc = thread.current_frame().pc.saturating_sub(1); // pc of the athrow / throwing opcode
            let handler = thread
                .current_frame()
                .exception_handlers
                .iter()
                .find(|h| {
                    if pc < h.start_pc as usize || pc >= h.end_pc as usize {
                        return false;
                    }
                    match &h.catch_class {
                        None => true, // finally / catch-all
                        Some(cls) => {
                            // Check class hierarchy (best-effort: ignore load errors)
                            self.is_instance_of(&exception_class, cls).unwrap_or(false)
                        }
                    }
                })
                .cloned();

            if let Some(h) = handler {
                let frame = thread.current_frame_mut();
                frame.stack.clear();
                frame.push(Value::Reference(exception_ref))?;
                frame.pc = h.handler_pc as usize;
                return Ok(());
            }

            // No handler in this frame — pop and try the caller.
            if thread.depth() == 1 {
                return Err(VmError::UnhandledException {
                    class_name: exception_class,
                });
            }
            thread.pop_frame();
        }
    }

    /// Resolve a method by walking the class hierarchy from `start_class` upward.
    ///
    /// Returns `(resolved_class_name, class_method)`.
    fn resolve_method(
        &mut self,
        start_class: &str,
        method_name: &str,
        descriptor: &str,
    ) -> Result<(String, ClassMethod), VmError> {
        let mut current = start_class.to_string();
        loop {
            self.ensure_class_loaded(&current)?;
            let class = self.get_class(&current)?;
            if let Some(m) = class
                .methods
                .get(&(method_name.to_string(), descriptor.to_string()))
            {
                return Ok((current, m.clone()));
            }
            match &class.super_class {
                Some(parent) => current = parent.clone(),
                None => {
                    return Err(VmError::MethodNotFound {
                        class_name: start_class.to_string(),
                        method_name: method_name.to_string(),
                        descriptor: descriptor.to_string(),
                    });
                }
            }
        }
    }

    /// Check whether `class_name` is the same as, or a sub-class of, `target`.
    fn is_instance_of(&mut self, class_name: &str, target: &str) -> Result<bool, VmError> {
        let mut current = class_name.to_string();
        loop {
            if current == target {
                return Ok(true);
            }
            self.ensure_class_loaded(&current)?;
            let class = self.get_class(&current)?;
            match &class.super_class {
                Some(parent) => current = parent.clone(),
                None => return Ok(false),
            }
        }
    }

    /// Shared dispatch logic for `invokevirtual` and `invokespecial`.
    fn dispatch_instance_method(
        &mut self,
        thread: &mut Thread,
        class_name: &str,
        method_ref: &MethodRef,
        receiver: Reference,
        args: Vec<Value>,
    ) -> Result<(), VmError> {
        // Lambda proxy dispatch: redirect to the captured target method.
        if class_name.starts_with("__lambda_proxy_") {
            return self.dispatch_lambda_proxy(thread, receiver, args);
        }

        let (resolved_class, class_method) =
            self.resolve_method(class_name, &method_ref.method_name, &method_ref.descriptor)?;

        match class_method {
            ClassMethod::Native => {
                let mut all_args = vec![Value::Reference(receiver)];
                all_args.extend(args);
                let result = self.invoke_native(
                    &resolved_class,
                    &method_ref.method_name,
                    &method_ref.descriptor,
                    &all_args,
                )?;
                if let Some(value) = result {
                    thread.current_frame_mut().push(value)?;
                }
            }
            ClassMethod::Bytecode(method) => {
                let mut initial_locals = vec![Some(Value::Reference(receiver))];
                initial_locals.extend(args.into_iter().map(Some));
                let callee = method.with_initial_locals(initial_locals);
                thread.push_frame(Frame::new(callee));
            }
        }
        Ok(())
    }

    /// Dispatch a call on a lambda proxy object to its captured target method.
    fn dispatch_lambda_proxy(
        &mut self,
        thread: &mut Thread,
        receiver: Reference,
        args: Vec<Value>,
    ) -> Result<(), VmError> {
        let (target_class, target_method, target_desc, captures) = {
            let fields = match self.heap.lock().unwrap().get(receiver)? {
                HeapValue::Object { fields, .. } => fields.clone(),
                _ => return Err(VmError::NullReference),
            };

            let get_str = |key: &str| -> Result<std::string::String, VmError> {
                match fields.get(key) {
                    Some(Value::Reference(r)) => self.stringify_reference(*r),
                    _ => Ok(std::string::String::new()),
                }
            };

            let tc = get_str("__target_class")?;
            let tm = get_str("__target_method")?;
            let td = get_str("__target_desc")?;

            let mut captures = Vec::new();
            let mut i = 0;
            while let Some(val) = fields.get(&format!("__capture_{i}")) {
                captures.push(*val);
                i += 1;
            }

            (tc, tm, td, captures)
        };

        let mut all_args = captures;
        all_args.extend(args);

        self.ensure_class_loaded(&target_class)?;

        let (_, class_method) =
            self.resolve_method(&target_class, &target_method, &target_desc)?;

        match class_method {
            ClassMethod::Native => {
                let result =
                    self.invoke_native(&target_class, &target_method, &target_desc, &all_args)?;
                if let Some(value) = result {
                    thread.current_frame_mut().push(value)?;
                }
            }
            ClassMethod::Bytecode(method) => {
                let initial_locals: Vec<Option<Value>> =
                    all_args.into_iter().map(Some).collect();
                let callee = method.with_initial_locals(initial_locals);
                thread.push_frame(Frame::new(callee));
            }
        }
        Ok(())
    }
}

/// Count the number of arguments in a JVM method descriptor.
///
/// Parses the parameter section of a descriptor like `(ILjava/lang/String;)V`
/// and returns the number of parameters (2 in that example).
/// Return the JVM default zero-value for a field descriptor.
fn default_value_for_descriptor(descriptor: &str) -> Value {
    match descriptor.as_bytes().first() {
        Some(b'J') => Value::Long(0),
        Some(b'F') => Value::Float(0.0),
        Some(b'D') => Value::Double(0.0),
        Some(b'L') | Some(b'[') => Value::Reference(Reference::Null),
        _ => Value::Int(0),
    }
}

fn parse_arg_count(descriptor: &str) -> Result<usize, VmError> {
    let bytes = descriptor.as_bytes();
    if bytes.first() != Some(&b'(') {
        return Err(VmError::InvalidDescriptor {
            descriptor: descriptor.to_string(),
        });
    }
    let mut count = 0;
    let mut i = 1;
    while i < bytes.len() && bytes[i] != b')' {
        match bytes[i] {
            b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z' => {
                count += 1;
                i += 1;
            }
            b'L' => {
                count += 1;
                i += 1;
                while i < bytes.len() && bytes[i] != b';' {
                    i += 1;
                }
                i += 1;
            }
            b'[' => {
                while i < bytes.len() && bytes[i] == b'[' {
                    i += 1;
                }
                if i < bytes.len() && bytes[i] == b'L' {
                    i += 1;
                    while i < bytes.len() && bytes[i] != b';' {
                        i += 1;
                    }
                    i += 1;
                } else if i < bytes.len() {
                    i += 1;
                }
                count += 1;
            }
            _ => {
                return Err(VmError::InvalidDescriptor {
                    descriptor: descriptor.to_string(),
                });
            }
        }
    }
    Ok(count)
}

fn format_vm_float(v: f64) -> String {
    if v.is_nan() {
        "NaN".to_string()
    } else if v.is_infinite() {
        if v > 0.0 {
            "Infinity".to_string()
        } else {
            "-Infinity".to_string()
        }
    } else if v == 0.0 && v.is_sign_negative() {
        "-0.0".to_string()
    } else {
        let s = format!("{v}");
        if s.contains('.') {
            s
        } else {
            format!("{v}.0")
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::Ordering;
    use std::sync::mpsc;
    use std::time::Duration;

    use super::{
        ExecutionResult, FieldRef, HeapValue, Method, MethodRef, Reference, RuntimeClass, Value,
        Vm, VmError, NEXT_THREAD_ID,
    };

    #[test]
    fn executes_basic_integer_bytecode() {
        let method = Method::new(
            [
                0x05, // iconst_2
                0x06, // iconst_3
                0x60, // iadd
                0x3b, // istore_0
                0x1a, // iload_0
                0x08, // iconst_5
                0x68, // imul
                0xac, // ireturn
            ],
            1,
            2,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(25)));
    }

    #[test]
    fn supports_explicit_local_indexes_and_dup() {
        let method = Method::new(
            [
                0x10, 0x07, // bipush 7
                0x59, // dup
                0x36, 0x01, // istore 1
                0x15, 0x01, // iload 1
                0x60, // iadd
                0xac, // ireturn
            ],
            2,
            3,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(14)));
    }

    #[test]
    fn supports_dup_x1() {
        let method = Method::new(
            [
                0x04, // iconst_1
                0x05, // iconst_2
                0x5a, // dup_x1 => [2, 1, 2]
                0x60, // iadd => [2, 3]
                0x60, // iadd => [5]
                0xac, // ireturn
            ],
            0,
            3,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(5)));
    }

    #[test]
    fn supports_dup2() {
        let method = Method::new(
            [
                0x04, // iconst_1
                0x05, // iconst_2
                0x5c, // dup2 => [1, 2, 1, 2]
                0x60, // iadd => [1, 2, 3]
                0x60, // iadd => [1, 5]
                0x60, // iadd => [6]
                0xac, // ireturn
            ],
            0,
            4,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(6)));
    }

    #[test]
    fn supports_swap() {
        let method = Method::new(
            [
                0x10, 0x05, // bipush 5
                0x10, 0x03, // bipush 3
                0x5f, // swap => [3, 5]
                0x64, // isub => 3 - 5 = -2
                0xac, // ireturn
            ],
            0,
            2,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(-2)));
    }

    #[test]
    fn supports_reference_locals_and_arraylength() {
        let mut vm = Vm::new();
        let args = vm.new_string_array(&["a".to_string(), "b".to_string()]);
        let method = Method::new(
            [
                0x2a, // aload_0
                0xbe, // arraylength
                0xac, // ireturn
            ],
            1,
            1,
        )
        .with_initial_locals([Some(args)]);

        let result = vm.execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(2)));
    }

    #[test]
    fn supports_aconst_null_and_astore() {
        let method = Method::new(
            [
                0x01, // aconst_null
                0x4b, // astore_0
                0x2a, // aload_0
                0x57, // pop
                0xb1, // return
            ],
            1,
            1,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Void);
    }

    #[test]
    fn reports_null_reference_on_arraylength() {
        let method = Method::new(
            [
                0x01, // aconst_null
                0xbe, // arraylength
                0xac, // unreachable
            ],
            0,
            1,
        );

        let error = Vm::new().execute(method).unwrap_err();
        assert_eq!(
            error,
            VmError::UnhandledException {
                class_name: "java/lang/NullPointerException".to_string()
            }
        );
    }

    #[test]
    fn supports_aaload_and_areturn() {
        let mut vm = Vm::new();
        let args = vm.new_string_array(&["x".to_string(), "y".to_string()]);
        let method = Method::new(
            [
                0x2a, // aload_0
                0x04, // iconst_1
                0x32, // aaload
                0xb0, // areturn
            ],
            1,
            2,
        )
        .with_initial_locals([Some(args)]);

        let result = vm.execute(method).unwrap();
        match result {
            ExecutionResult::Value(Value::Reference(Reference::Heap(_))) => {}
            other => panic!("expected heap reference, got {other:?}"),
        }
    }

    #[test]
    fn supports_aastore() {
        let mut vm = Vm::new();
        let array = vm.new_string_array(&["x".to_string(), "y".to_string()]);
        let value = vm.new_string("z");
        let method = Method::new(
            [
                0x2a, // aload_0
                0x04, // iconst_1
                0x2b, // aload_1
                0x53, // aastore
                0x2a, // aload_0
                0x04, // iconst_1
                0x32, // aaload
                0xb0, // areturn
            ],
            2,
            3,
        )
        .with_initial_locals([Some(array), Some(value)]);

        let result = vm.execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(value));
    }

    #[test]
    fn supports_newarray_iaload_iastore_and_arraylength() {
        let method = Method::new(
            [
                0x06, // iconst_3
                0xbc, 0x0a, // newarray int
                0x4b, // astore_0
                0x2a, // aload_0
                0x04, // iconst_1
                0x10, 0x2a, // bipush 42
                0x4f, // iastore
                0x2a, // aload_0
                0x04, // iconst_1
                0x2e, // iaload
                0x2a, // aload_0
                0xbe, // arraylength
                0x68, // imul
                0xac, // ireturn
            ],
            1,
            3,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(126)));
    }

    #[test]
    fn supports_builtin_println_for_ints_and_strings() {
        let mut vm = Vm::new();
        let hello = vm.new_string("hello");
        let method = Method::with_constant_pool(
            [
                0xb2, 0x00, 0x01, // getstatic #1
                0x10, 0x2a, // bipush 42
                0xb6, 0x00, 0x01, // invokevirtual #1 println(int)
                0xb2, 0x00, 0x01, // getstatic #1
                0x12, 0x01, // ldc #1
                0xb6, 0x00, 0x02, // invokevirtual #2 println(String)
                0xb1, // return
            ],
            0,
            2,
            vec![None, Some(hello)],
        )
        .with_field_refs(vec![
            None,
            Some(FieldRef {
                class_name: "java/lang/System".to_string(),
                field_name: "out".to_string(),
                descriptor: "Ljava/io/PrintStream;".to_string(),
            }),
        ])
        .with_method_refs(vec![
            None,
            Some(MethodRef {
                class_name: "java/io/PrintStream".to_string(),
                method_name: "println".to_string(),
                descriptor: "(I)V".to_string(),
            }),
            Some(MethodRef {
                class_name: "java/io/PrintStream".to_string(),
                method_name: "println".to_string(),
                descriptor: "(Ljava/lang/String;)V".to_string(),
            }),
        ]);

        let result = vm.execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Void);
        assert_eq!(vm.take_output(), vec!["42".to_string(), "hello".to_string()]);
    }

    #[test]
    fn supports_ifnull_and_ifnonnull() {
        let method = Method::new(
            [
                0x01, // aconst_null
                0xc6, 0x00, 0x06, // ifnull +6
                0x10, 0x63, // bipush 99
                0xac, // ireturn
                0x10, 0x2a, // bipush 42
                0xac, // ireturn
            ],
            0,
            1,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(42)));

        let mut vm = Vm::new();
        let arg = vm.new_string("hello");
        let method = Method::new(
            [
                0x2a, // aload_0
                0xc7, 0x00, 0x06, // ifnonnull +6
                0x10, 0x0b, // bipush 11
                0xac, // ireturn
                0x10, 0x16, // bipush 22
                0xac, // ireturn
            ],
            1,
            1,
        )
        .with_initial_locals([Some(arg)]);

        let result = vm.execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(22)));
    }

    #[test]
    fn supports_if_acmpeq_and_if_acmpne() {
        let mut vm = Vm::new();
        let same = vm.new_string("same");
        let other = vm.new_string("other");

        let method = Method::new(
            [
                0x2a, // aload_0
                0x2b, // aload_1
                0xa5, 0x00, 0x06, // if_acmpeq +6
                0x10, 0x09, // bipush 9
                0xac, // ireturn
                0x10, 0x15, // bipush 21
                0xac, // ireturn
            ],
            2,
            2,
        )
        .with_initial_locals([Some(same), Some(same)]);

        let result = vm.execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(21)));

        let method = Method::new(
            [
                0x2a, // aload_0
                0x2b, // aload_1
                0xa6, 0x00, 0x06, // if_acmpne +6
                0x10, 0x0d, // bipush 13
                0xac, // ireturn
                0x10, 0x22, // bipush 34
                0xac, // ireturn
            ],
            2,
            2,
        )
        .with_initial_locals([Some(same), Some(other)]);

        let result = vm.execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(34)));
    }

    #[test]
    fn reports_array_index_out_of_bounds() {
        let mut vm = Vm::new();
        let args = vm.new_string_array(&["x".to_string()]);
        let method = Method::new(
            [
                0x2a, // aload_0
                0x04, // iconst_1
                0x32, // aaload
                0xb0, // areturn
            ],
            1,
            2,
        )
        .with_initial_locals([Some(args)]);

        let error = vm.execute(method).unwrap_err();
        assert_eq!(
            error,
            VmError::UnhandledException {
                class_name: "java/lang/ArrayIndexOutOfBoundsException".to_string()
            }
        );
    }

    #[test]
    fn supports_anewarray() {
        let method = Method::new(
            [
                0x05, // iconst_2
                0xbd, 0x00, 0x01, // anewarray #1
                0xbe, // arraylength
                0xac, // ireturn
            ],
            0,
            1,
        )
        .with_reference_classes(vec![None, Some("java/lang/String".to_string())]);

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(2)));
    }

    #[test]
    fn reports_negative_array_size_for_anewarray() {
        let method = Method::new(
            [
                0x02, // iconst_m1
                0xbd, 0x00, 0x01, // anewarray #1
                0xb0, // unreachable
            ],
            0,
            1,
        )
        .with_reference_classes(vec![None, Some("java/lang/String".to_string())]);

        let error = Vm::new().execute(method).unwrap_err();
        assert_eq!(
            error,
            VmError::UnhandledException {
                class_name: "java/lang/NegativeArraySizeException".to_string()
            }
        );
    }

    #[test]
    fn reports_invalid_class_constant_for_anewarray() {
        let method = Method::new(
            [
                0x04, // iconst_1
                0xbd, 0x00, 0x02, // anewarray #2
                0xb0, // unreachable
            ],
            0,
            1,
        )
        .with_reference_classes(vec![None, Some("java/lang/String".to_string())]);

        let error = Vm::new().execute(method).unwrap_err();
        assert_eq!(
            error,
            VmError::InvalidClassConstantIndex {
                index: 2,
                constant_count: 1,
            }
        );
    }

    #[test]
    fn reports_unsupported_newarray_type() {
        let method = Method::new(
            [
                0x04, // iconst_1
                0xbc, 0x03, // newarray with invalid atype 3
                0xb0, // unreachable
            ],
            0,
            1,
        );

        let error = Vm::new().execute(method).unwrap_err();
        assert_eq!(error, VmError::UnsupportedNewArrayType { atype: 3 });
    }

    #[test]
    fn reports_division_by_zero() {
        let method = Method::new(
            [
                0x08, // iconst_5
                0x03, // iconst_0
                0x6c, // idiv
                0xac, // ireturn
            ],
            0,
            2,
        );

        let error = Vm::new().execute(method).unwrap_err();
        assert_eq!(
            error,
            VmError::UnhandledException {
                class_name: "java/lang/ArithmeticException".to_string()
            }
        );
    }

    #[test]
    fn supports_sipush_ldc_and_ineg() {
        let method = Method::with_constants(
            [
                0x11, 0x01, 0x2c, // sipush 300
                0x12, 0x01, // ldc #1
                0x60, // iadd
                0x74, // ineg
                0xac, // ireturn
            ],
            0,
            2,
            [Value::Int(7)],
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(-307)));
    }

    #[test]
    fn supports_irem() {
        let method = Method::new(
            [
                0x10, 0x11, // bipush 17
                0x10, 0x05, // bipush 5
                0x70, // irem
                0xac, // ireturn
            ],
            0,
            2,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(2)));
    }

    #[test]
    fn supports_goto_and_ifeq() {
        let method = Method::new(
            [
                0x03, // iconst_0
                0x99, 0x00, 0x08, // ifeq +8
                0x10, 0x63, // bipush 99
                0xa7, 0x00, 0x05, // goto +5
                0x10, 0x2a, // bipush 42
                0xac, // ireturn
            ],
            0,
            2,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(42)));
    }

    #[test]
    fn supports_jsr_and_ret() {
        let method = Method::new(
            [
                0x08, // iconst_5
                0x3b, // istore_0
                0xa8, 0x00, 0x05, // jsr +5 -> pc 7
                0x1a, // iload_0
                0xac, // ireturn
                0x4c, // astore_1
                0x84, 0x00, 0x01, // iinc 0 by 1
                0xa9, 0x01, // ret 1
            ],
            2,
            1,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(6)));
    }

    #[test]
    fn shares_static_fields_across_spawned_threads() {
        let mut vm = Vm::new();
        vm.register_class(RuntimeClass {
            name: "demo/Counter".to_string(),
            super_class: Some("java/lang/Object".to_string()),
            methods: BTreeMap::new(),
            static_fields: BTreeMap::from([("value".to_string(), Value::Int(0))]),
            instance_fields: vec![],
        });

        let child_method = Method::new(
            [
                0x10, 0x2a, // bipush 42
                0xb3, 0x00, 0x01, // putstatic #1
                0xb1, // return
            ],
            0,
            1,
        )
        .with_field_refs(vec![
            None,
            Some(FieldRef {
                class_name: "demo/Counter".to_string(),
                field_name: "value".to_string(),
                descriptor: "I".to_string(),
            }),
        ]);

        vm.spawn(child_method).join().unwrap();

        let read_method = Method::new(
            [
                0xb2, 0x00, 0x01, // getstatic #1
                0xac, // ireturn
            ],
            0,
            1,
        )
        .with_field_refs(vec![
            None,
            Some(FieldRef {
                class_name: "demo/Counter".to_string(),
                field_name: "value".to_string(),
                descriptor: "I".to_string(),
            }),
        ]);

        let result = vm.execute(read_method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(42)));
    }

    #[test]
    fn blocks_monitorenter_until_owner_releases_monitor() {
        let vm = Vm::new();
        let monitor_ref = vm.heap.lock().unwrap().allocate(HeapValue::Object {
            class_name: "java/lang/Object".to_string(),
            fields: BTreeMap::new(),
        });
        vm.enter_monitor(monitor_ref).unwrap();

        let mut child_vm = vm.clone();
        child_vm.thread_id = NEXT_THREAD_ID.fetch_add(1, Ordering::Relaxed);

        let (started_tx, started_rx) = mpsc::channel();
        let (acquired_tx, acquired_rx) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            child_vm.enter_monitor(monitor_ref).unwrap();
            acquired_tx.send(()).unwrap();
            child_vm.exit_monitor(monitor_ref).unwrap();
        });

        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(acquired_rx.recv_timeout(Duration::from_millis(50)).is_err());

        vm.exit_monitor(monitor_ref).unwrap();

        acquired_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        handle.join().unwrap();
    }

    #[test]
    fn supports_iinc_with_positive_and_negative_deltas() {
        let method = Method::new(
            [
                0x10, 0x0a, // bipush 10
                0x3b, // istore_0
                0x84, 0x00, 0x05, // iinc 0 by 5
                0x84, 0x00, 0xfd, // iinc 0 by -3
                0x1a, // iload_0
                0xac, // ireturn
            ],
            1,
            1,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(12)));
    }

    #[test]
    fn supports_ifne_and_if_icmpne() {
        let method = Method::new(
            [
                0x04, // iconst_1
                0x9a, 0x00, 0x06, // ifne +6
                0x10, 0x64, // bipush 100
                0xac, // ireturn
                0x05, // iconst_2
                0x06, // iconst_3
                0xa0, 0x00, 0x06, // if_icmpne +6
                0x10, 0x37, // bipush 55
                0xac, // ireturn
                0x10, 0x58, // bipush 88
                0xac, // ireturn
            ],
            0,
            2,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(88)));
    }

    #[test]
    fn supports_iflt_ifge_ifgt_and_ifle() {
        let method = Method::new(
            [
                0x02, // iconst_m1
                0x9b, 0x00, 0x08, // iflt +8
                0x10, 0x63, // bipush 99
                0xa7, 0x00, 0x29, // goto +41
                0x03, // iconst_0
                0x9c, 0x00, 0x08, // ifge +8
                0x10, 0x62, // bipush 98
                0xa7, 0x00, 0x20, // goto +32
                0x04, // iconst_1
                0x9d, 0x00, 0x08, // ifgt +8
                0x10, 0x61, // bipush 97
                0xa7, 0x00, 0x17, // goto +23
                0x03, // iconst_0
                0x9e, 0x00, 0x08, // ifle +8
                0x10, 0x60, // bipush 96
                0xa7, 0x00, 0x0e, // goto +14
                0x10, 0x2c, // bipush 44
                0xac, // ireturn
                0x10, 0x0b, // bipush 11
                0xac, // ireturn
            ],
            0,
            1,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(44)));
    }

    #[test]
    fn supports_if_icmpeq() {
        let method = Method::new(
            [
                0x08, // iconst_5
                0x10, 0x05, // bipush 5
                0x9f, 0x00, 0x06, // if_icmpeq +6
                0x10, 0x09, // bipush 9
                0xac, // ireturn
                0x10, 0x21, // bipush 33
                0xac, // ireturn
            ],
            0,
            2,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(33)));
    }

    #[test]
    fn supports_if_icmplt_if_icmpge_if_icmpgt_and_if_icmple() {
        let method = Method::new(
            [
                0x04, // iconst_1
                0x05, // iconst_2
                0xa1, 0x00, 0x08, // if_icmplt +8
                0x10, 0x63, // bipush 99
                0xa7, 0x00, 0x32, // goto +50
                0x05, // iconst_2
                0x05, // iconst_2
                0xa2, 0x00, 0x08, // if_icmpge +8
                0x10, 0x62, // bipush 98
                0xa7, 0x00, 0x28, // goto +40
                0x06, // iconst_3
                0x05, // iconst_2
                0xa3, 0x00, 0x08, // if_icmpgt +8
                0x10, 0x61, // bipush 97
                0xa7, 0x00, 0x1e, // goto +30
                0x04, // iconst_1
                0x04, // iconst_1
                0xa4, 0x00, 0x08, // if_icmple +8
                0x10, 0x60, // bipush 96
                0xa7, 0x00, 0x14, // goto +20
                0x10, 0x4d, // bipush 77
                0xac, // ireturn
                0x10, 0x0c, // bipush 12
                0xac, // ireturn
            ],
            0,
            2,
        );

        let result = Vm::new().execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Value(Value::Int(77)));
    }

    #[test]
    fn reports_invalid_constant_index() {
        let method = Method::with_constants(
            [
                0x12, 0x02, // ldc #2
                0xac, // ireturn
            ],
            0,
            1,
            [Value::Int(1)],
        );

        let error = Vm::new().execute(method).unwrap_err();
        assert_eq!(
            error,
            VmError::InvalidConstantIndex {
                index: 2,
                constant_count: 1,
            }
        );
    }

    #[test]
    fn reports_invalid_branch_target() {
        let method = Method::new(
            [
                0xa7, 0x7f, 0xff, // goto far away
            ],
            0,
            0,
        );

        let error = Vm::new().execute(method).unwrap_err();
        assert_eq!(
            error,
            VmError::InvalidBranchTarget {
                target: 32767,
                code_len: 3,
            }
        );
    }
}
