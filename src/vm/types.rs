//! Public value, method, class, and error types plus descriptor helpers.
//!
//! Split from `mod.rs` to keep data-type definitions separate from the
//! runtime machinery (heap, frames, interpreter).

use std::collections::HashMap;
use std::fmt;

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
    pub(super) fn as_int(self) -> Result<i32, VmError> {
        match self {
            Self::Int(value) => Ok(value),
            other => Err(VmError::TypeMismatch {
                expected: "int",
                actual: other.type_name(),
            }),
        }
    }

    pub(super) fn as_long(self) -> Result<i64, VmError> {
        match self {
            Self::Long(value) => Ok(value),
            other => Err(VmError::TypeMismatch {
                expected: "long",
                actual: other.type_name(),
            }),
        }
    }

    pub(super) fn as_float(self) -> Result<f32, VmError> {
        match self {
            Self::Float(value) => Ok(value),
            other => Err(VmError::TypeMismatch {
                expected: "float",
                actual: other.type_name(),
            }),
        }
    }

    pub(super) fn as_double(self) -> Result<f64, VmError> {
        match self {
            Self::Double(value) => Ok(value),
            other => Err(VmError::TypeMismatch {
                expected: "double",
                actual: other.type_name(),
            }),
        }
    }

    pub(super) fn as_reference(self) -> Result<Reference, VmError> {
        match self {
            Self::Reference(reference) => Ok(reference),
            other => Err(VmError::TypeMismatch {
                expected: "reference",
                actual: other.type_name(),
            }),
        }
    }

    pub(super) fn as_return_address(self) -> Result<usize, VmError> {
        match self {
            Self::ReturnAddress(address) => Ok(address),
            other => Err(VmError::TypeMismatch {
                expected: "returnAddress",
                actual: other.type_name(),
            }),
        }
    }

    pub(super) fn type_name(self) -> &'static str {
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

#[derive(Debug, Clone)]
pub struct ResolvedMethod {
    pub resolved_class: String,
    pub class_method: ClassMethod,
}

#[derive(Debug, Clone)]
pub struct RuntimeClass {
    pub name: String,
    pub super_class: Option<String>,
    pub methods: HashMap<(String, String), ClassMethod>,
    pub static_fields: HashMap<String, Value>,
    /// Instance field definitions: (name, descriptor).
    pub instance_fields: Vec<(String, String)>,
    /// Names of directly implemented interfaces. Empty for built-in classes and for
    /// classes that do not declare any interface. Used by `resolve_method` to find
    /// interface `default` methods when no class-hierarchy method matches.
    #[doc(hidden)]
    pub interfaces: Vec<String>,
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

    pub fn with_exception_handlers(mut self, handlers: impl Into<Vec<ExceptionHandler>>) -> Self {
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

pub(super) fn default_value_for_descriptor(descriptor: &str) -> Value {
    match descriptor.as_bytes().first() {
        Some(b'J') => Value::Long(0),
        Some(b'F') => Value::Float(0.0),
        Some(b'D') => Value::Double(0.0),
        Some(b'L') | Some(b'[') => Value::Reference(Reference::Null),
        _ => Value::Int(0),
    }
}

/// Zero-valued return for a method descriptor, suitable for stubbing out
/// native methods. Returns `None` for void (`V`), matching the invoker's
/// "don't push anything on void" convention.
pub(super) fn stub_return_value(descriptor: &str) -> Option<Value> {
    let bytes = descriptor.as_bytes();
    let close = bytes.iter().position(|&b| b == b')')?;
    let ret = &descriptor[close + 1..];
    match ret.as_bytes().first()? {
        b'V' => None,
        b'J' => Some(Value::Long(0)),
        b'F' => Some(Value::Float(0.0)),
        b'D' => Some(Value::Double(0.0)),
        b'L' | b'[' => Some(Value::Reference(Reference::Null)),
        _ => Some(Value::Int(0)),
    }
}

/// Parse the leading descriptor byte for every parameter in a method descriptor.
/// Reference types collapse to `b'L'`, arrays to `b'['`; primitives keep their
/// descriptor letter. Returns `None` for malformed descriptors.
pub(super) fn parse_arg_types(descriptor: &str) -> Option<Vec<u8>> {
    let bytes = descriptor.as_bytes();
    if bytes.first() != Some(&b'(') {
        return None;
    }
    let mut out = Vec::new();
    let mut i = 1;
    while i < bytes.len() && bytes[i] != b')' {
        match bytes[i] {
            c @ (b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z') => {
                out.push(c);
                i += 1;
            }
            b'L' => {
                out.push(b'L');
                i += 1;
                while i < bytes.len() && bytes[i] != b';' {
                    i += 1;
                }
                i += 1;
            }
            b'[' => {
                out.push(b'[');
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
            }
            _ => return None,
        }
    }
    Some(out)
}

pub(super) fn parse_arg_count(descriptor: &str) -> Result<usize, VmError> {
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

pub(super) fn parse_return_type(descriptor: &str) -> Result<Option<u8>, VmError> {
    let bytes = descriptor.as_bytes();
    if bytes.first() != Some(&b'(') {
        return Err(VmError::InvalidDescriptor {
            descriptor: descriptor.to_string(),
        });
    }
    let mut i = 1;
    while i < bytes.len() && bytes[i] != b')' {
        match bytes[i] {
            b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z' => {
                i += 1;
            }
            b'L' => {
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
            }
            _ => {
                return Err(VmError::InvalidDescriptor {
                    descriptor: descriptor.to_string(),
                });
            }
        }
    }
    if i >= bytes.len() || bytes[i] != b')' {
        return Err(VmError::InvalidDescriptor {
            descriptor: descriptor.to_string(),
        });
    }
    i += 1;
    if i >= bytes.len() {
        Ok(None)
    } else {
        Ok(Some(bytes[i]))
    }
}

pub(super) fn format_vm_float(v: f64) -> String {
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
        if s.contains('.') { s } else { format!("{v}.0") }
    }
}
