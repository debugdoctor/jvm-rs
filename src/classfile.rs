use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct ClassFile {
    pub minor_version: u16,
    pub major_version: u16,
    pub constant_pool: ConstantPool,
    pub access_flags: u16,
    pub this_class: u16,
    pub super_class: u16,
    pub interfaces: Vec<u16>,
    pub fields: Vec<MemberInfo>,
    pub methods: Vec<MemberInfo>,
    pub attributes: Vec<AttributeInfo>,
}

impl ClassFile {
    pub fn parse(bytes: &[u8]) -> Result<Self, ClassFileError> {
        let mut reader = ClassReader::new(bytes);
        let magic = reader.read_u4()?;
        if magic != 0xCAFEBABE {
            return Err(ClassFileError::InvalidMagic { actual: magic });
        }

        let minor_version = reader.read_u2()?;
        let major_version = reader.read_u2()?;
        let constant_pool = ConstantPool::parse(&mut reader)?;
        let access_flags = reader.read_u2()?;
        let this_class = reader.read_u2()?;
        let super_class = reader.read_u2()?;
        let interfaces = reader.read_many_u2()?;
        let fields = parse_members(&mut reader, &constant_pool)?;
        let methods = parse_members(&mut reader, &constant_pool)?;
        let attributes = parse_attributes(&mut reader, &constant_pool)?;

        if !reader.is_finished() {
            return Err(ClassFileError::TrailingBytes {
                remaining: reader.remaining(),
            });
        }

        Ok(Self {
            minor_version,
            major_version,
            constant_pool,
            access_flags,
            this_class,
            super_class,
            interfaces,
            fields,
            methods,
            attributes,
        })
    }

    pub fn class_name(&self) -> Result<&str, ClassFileError> {
        self.constant_pool.class_name(self.this_class)
    }

    pub fn super_class_name(&self) -> Result<Option<&str>, ClassFileError> {
        if self.super_class == 0 {
            return Ok(None);
        }
        self.constant_pool.class_name(self.super_class).map(Some)
    }

    pub fn bootstrap_methods(&self) -> &[BootstrapMethod] {
        for attr in &self.attributes {
            if let AttributeInfo::BootstrapMethods(methods) = attr {
                return methods;
            }
        }
        &[]
    }

    pub fn find_method(
        &self,
        name: &str,
        descriptor: &str,
    ) -> Result<Option<&MemberInfo>, ClassFileError> {
        for method in &self.methods {
            if method.name(&self.constant_pool)? == name
                && method.descriptor(&self.constant_pool)? == descriptor
            {
                return Ok(Some(method));
            }
        }
        Ok(None)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstantPool {
    entries: Vec<Option<ConstantPoolEntry>>,
}

impl ConstantPool {
    fn parse(reader: &mut ClassReader<'_>) -> Result<Self, ClassFileError> {
        let count = reader.read_u2()? as usize;
        let mut entries = Vec::with_capacity(count);
        entries.push(None);

        let mut index = 1;
        while index < count {
            let tag = reader.read_u1()?;
            let entry = match tag {
                1 => ConstantPoolEntry::Utf8(reader.read_modified_utf8()?),
                3 => ConstantPoolEntry::Integer(reader.read_u4()? as i32),
                4 => ConstantPoolEntry::Float(f32::from_bits(reader.read_u4()?)),
                5 => {
                    let value = reader.read_u8()? as i64;
                    entries.push(Some(ConstantPoolEntry::Long(value)));
                    entries.push(None);
                    index += 2;
                    continue;
                }
                6 => {
                    let value = f64::from_bits(reader.read_u8()?);
                    entries.push(Some(ConstantPoolEntry::Double(value)));
                    entries.push(None);
                    index += 2;
                    continue;
                }
                7 => ConstantPoolEntry::Class {
                    name_index: reader.read_u2()?,
                },
                8 => ConstantPoolEntry::String {
                    string_index: reader.read_u2()?,
                },
                9 => ConstantPoolEntry::Fieldref {
                    class_index: reader.read_u2()?,
                    name_and_type_index: reader.read_u2()?,
                },
                10 => ConstantPoolEntry::Methodref {
                    class_index: reader.read_u2()?,
                    name_and_type_index: reader.read_u2()?,
                },
                11 => ConstantPoolEntry::InterfaceMethodref {
                    class_index: reader.read_u2()?,
                    name_and_type_index: reader.read_u2()?,
                },
                12 => ConstantPoolEntry::NameAndType {
                    name_index: reader.read_u2()?,
                    descriptor_index: reader.read_u2()?,
                },
                15 => ConstantPoolEntry::MethodHandle {
                    reference_kind: reader.read_u1()?,
                    reference_index: reader.read_u2()?,
                },
                16 => ConstantPoolEntry::MethodType {
                    descriptor_index: reader.read_u2()?,
                },
                17 => ConstantPoolEntry::Dynamic {
                    bootstrap_method_attr_index: reader.read_u2()?,
                    name_and_type_index: reader.read_u2()?,
                },
                18 => ConstantPoolEntry::InvokeDynamic {
                    bootstrap_method_attr_index: reader.read_u2()?,
                    name_and_type_index: reader.read_u2()?,
                },
                19 => ConstantPoolEntry::Module {
                    name_index: reader.read_u2()?,
                },
                20 => ConstantPoolEntry::Package {
                    name_index: reader.read_u2()?,
                },
                _ => return Err(ClassFileError::UnsupportedConstantTag { tag }),
            };

            entries.push(Some(entry));
            index += 1;
        }

        Ok(Self { entries })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn get(&self, index: u16) -> Result<&ConstantPoolEntry, ClassFileError> {
        let index = index as usize;
        self.entries.get(index).and_then(Option::as_ref).ok_or(
            ClassFileError::InvalidConstantPoolIndex {
                index: index as u16,
            },
        )
    }

    pub fn utf8(&self, index: u16) -> Result<&str, ClassFileError> {
        match self.get(index)? {
            ConstantPoolEntry::Utf8(value) => Ok(value),
            entry => Err(ClassFileError::UnexpectedConstantType {
                index,
                expected: "Utf8",
                actual: entry.kind_name(),
            }),
        }
    }

    pub fn class_name(&self, index: u16) -> Result<&str, ClassFileError> {
        match self.get(index)? {
            ConstantPoolEntry::Class { name_index } => self.utf8(*name_index),
            entry => Err(ClassFileError::UnexpectedConstantType {
                index,
                expected: "Class",
                actual: entry.kind_name(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConstantPoolEntry {
    Utf8(String),
    Integer(i32),
    Float(f32),
    Long(i64),
    Double(f64),
    Class {
        name_index: u16,
    },
    String {
        string_index: u16,
    },
    Fieldref {
        class_index: u16,
        name_and_type_index: u16,
    },
    Methodref {
        class_index: u16,
        name_and_type_index: u16,
    },
    InterfaceMethodref {
        class_index: u16,
        name_and_type_index: u16,
    },
    NameAndType {
        name_index: u16,
        descriptor_index: u16,
    },
    MethodHandle {
        reference_kind: u8,
        reference_index: u16,
    },
    MethodType {
        descriptor_index: u16,
    },
    Dynamic {
        bootstrap_method_attr_index: u16,
        name_and_type_index: u16,
    },
    InvokeDynamic {
        bootstrap_method_attr_index: u16,
        name_and_type_index: u16,
    },
    Module {
        name_index: u16,
    },
    Package {
        name_index: u16,
    },
}

impl ConstantPoolEntry {
    pub(crate) fn kind_name(&self) -> &'static str {
        match self {
            Self::Utf8(_) => "Utf8",
            Self::Integer(_) => "Integer",
            Self::Float(_) => "Float",
            Self::Long(_) => "Long",
            Self::Double(_) => "Double",
            Self::Class { .. } => "Class",
            Self::String { .. } => "String",
            Self::Fieldref { .. } => "Fieldref",
            Self::Methodref { .. } => "Methodref",
            Self::InterfaceMethodref { .. } => "InterfaceMethodref",
            Self::NameAndType { .. } => "NameAndType",
            Self::MethodHandle { .. } => "MethodHandle",
            Self::MethodType { .. } => "MethodType",
            Self::Dynamic { .. } => "Dynamic",
            Self::InvokeDynamic { .. } => "InvokeDynamic",
            Self::Module { .. } => "Module",
            Self::Package { .. } => "Package",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberInfo {
    pub access_flags: u16,
    pub name_index: u16,
    pub descriptor_index: u16,
    pub attributes: Vec<AttributeInfo>,
}

impl MemberInfo {
    pub fn name<'a>(&self, constant_pool: &'a ConstantPool) -> Result<&'a str, ClassFileError> {
        constant_pool.utf8(self.name_index)
    }

    pub fn descriptor<'a>(
        &self,
        constant_pool: &'a ConstantPool,
    ) -> Result<&'a str, ClassFileError> {
        constant_pool.utf8(self.descriptor_index)
    }

    pub fn code(&self) -> Option<&CodeAttribute> {
        self.attributes
            .iter()
            .find_map(|attribute| match attribute {
                AttributeInfo::Code(code) => Some(code),
                _ => None,
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionTableEntry {
    pub start_pc: u16,
    pub end_pc: u16,
    pub handler_pc: u16,
    pub catch_type: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeAttribute {
    pub name_index: u16,
    pub max_stack: u16,
    pub max_locals: u16,
    pub code: Vec<u8>,
    pub exception_table: Vec<ExceptionTableEntry>,
    pub attributes: Vec<AttributeInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeInfo {
    Code(CodeAttribute),
    LineNumberTable(Vec<LineNumberEntry>),
    SourceFile(String),
    BootstrapMethods(Vec<BootstrapMethod>),
    Raw(RawAttribute),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapMethod {
    pub method_ref: u16,
    pub arguments: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineNumberEntry {
    pub start_pc: u16,
    pub line_number: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawAttribute {
    pub name_index: u16,
    pub name: String,
    pub info: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassFileError {
    InvalidMagic {
        actual: u32,
    },
    UnexpectedEof {
        position: usize,
        needed: usize,
    },
    TrailingBytes {
        remaining: usize,
    },
    UnsupportedConstantTag {
        tag: u8,
    },
    InvalidConstantPoolIndex {
        index: u16,
    },
    UnexpectedConstantType {
        index: u16,
        expected: &'static str,
        actual: &'static str,
    },
    InvalidModifiedUtf8,
}

impl fmt::Display for ClassFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMagic { actual } => write!(f, "invalid class file magic 0x{actual:08x}"),
            Self::UnexpectedEof { position, needed } => {
                write!(
                    f,
                    "unexpected end of class file at byte {position}, needed {needed} more bytes"
                )
            }
            Self::TrailingBytes { remaining } => {
                write!(f, "class file has {remaining} trailing bytes")
            }
            Self::UnsupportedConstantTag { tag } => {
                write!(f, "unsupported constant pool tag {tag}")
            }
            Self::InvalidConstantPoolIndex { index } => {
                write!(f, "invalid constant pool index {index}")
            }
            Self::UnexpectedConstantType {
                index,
                expected,
                actual,
            } => write!(
                f,
                "constant pool entry {index} has type {actual}, expected {expected}"
            ),
            Self::InvalidModifiedUtf8 => write!(f, "invalid modified UTF-8 in class file"),
        }
    }
}

impl std::error::Error for ClassFileError {}

fn parse_members(
    reader: &mut ClassReader<'_>,
    constant_pool: &ConstantPool,
) -> Result<Vec<MemberInfo>, ClassFileError> {
    let count = reader.read_u2()? as usize;
    let mut members = Vec::with_capacity(count);

    for _ in 0..count {
        members.push(MemberInfo {
            access_flags: reader.read_u2()?,
            name_index: reader.read_u2()?,
            descriptor_index: reader.read_u2()?,
            attributes: parse_attributes(reader, constant_pool)?,
        });
    }

    Ok(members)
}

fn parse_attributes(
    reader: &mut ClassReader<'_>,
    constant_pool: &ConstantPool,
) -> Result<Vec<AttributeInfo>, ClassFileError> {
    let count = reader.read_u2()? as usize;
    let mut attributes = Vec::with_capacity(count);

    for _ in 0..count {
        let name_index = reader.read_u2()?;
        let attribute_length = reader.read_u4()? as usize;
        let name = constant_pool.utf8(name_index)?.to_string();

        if name == "Code" {
            let bytes = reader.read_bytes(attribute_length)?.to_vec();
            let mut code_reader = ClassReader::new(&bytes);
            let max_stack = code_reader.read_u2()?;
            let max_locals = code_reader.read_u2()?;
            let code_length = code_reader.read_u4()? as usize;
            let code = code_reader.read_bytes(code_length)?.to_vec();

            let exception_table_count = code_reader.read_u2()? as usize;
            let mut exception_table = Vec::with_capacity(exception_table_count);
            for _ in 0..exception_table_count {
                exception_table.push(ExceptionTableEntry {
                    start_pc: code_reader.read_u2()?,
                    end_pc: code_reader.read_u2()?,
                    handler_pc: code_reader.read_u2()?,
                    catch_type: code_reader.read_u2()?,
                });
            }

            let nested_attributes = parse_attributes(&mut code_reader, constant_pool)?;
            if !code_reader.is_finished() {
                return Err(ClassFileError::TrailingBytes {
                    remaining: code_reader.remaining(),
                });
            }

            attributes.push(AttributeInfo::Code(CodeAttribute {
                name_index,
                max_stack,
                max_locals,
                code,
                exception_table,
                attributes: nested_attributes,
            }));
        } else if name == "LineNumberTable" {
            let bytes = reader.read_bytes(attribute_length)?.to_vec();
            let mut lnt_reader = ClassReader::new(&bytes);
            let table_length = lnt_reader.read_u2()? as usize;
            let mut entries = Vec::with_capacity(table_length);
            for _ in 0..table_length {
                entries.push(LineNumberEntry {
                    start_pc: lnt_reader.read_u2()?,
                    line_number: lnt_reader.read_u2()?,
                });
            }
            attributes.push(AttributeInfo::LineNumberTable(entries));
        } else if name == "BootstrapMethods" {
            let bytes = reader.read_bytes(attribute_length)?.to_vec();
            let mut bm_reader = ClassReader::new(&bytes);
            let num_bootstrap_methods = bm_reader.read_u2()? as usize;
            let mut bootstrap_methods = Vec::with_capacity(num_bootstrap_methods);
            for _ in 0..num_bootstrap_methods {
                let method_ref = bm_reader.read_u2()?;
                let num_arguments = bm_reader.read_u2()? as usize;
                let mut arguments = Vec::with_capacity(num_arguments);
                for _ in 0..num_arguments {
                    arguments.push(bm_reader.read_u2()?);
                }
                bootstrap_methods.push(BootstrapMethod {
                    method_ref,
                    arguments,
                });
            }
            attributes.push(AttributeInfo::BootstrapMethods(bootstrap_methods));
        } else if name == "SourceFile" {
            let bytes = reader.read_bytes(attribute_length)?.to_vec();
            let mut sf_reader = ClassReader::new(&bytes);
            let source_file_index = sf_reader.read_u2()?;
            let source_file = constant_pool.utf8(source_file_index)?.to_string();
            attributes.push(AttributeInfo::SourceFile(source_file));
        } else {
            attributes.push(AttributeInfo::Raw(RawAttribute {
                name_index,
                name,
                info: reader.read_bytes(attribute_length)?.to_vec(),
            }));
        }
    }

    Ok(attributes)
}

struct ClassReader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> ClassReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn read_u1(&mut self) -> Result<u8, ClassFileError> {
        self.read_bytes(1).map(|bytes| bytes[0])
    }

    fn read_u2(&mut self) -> Result<u16, ClassFileError> {
        let bytes = self.read_bytes(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn read_u4(&mut self) -> Result<u32, ClassFileError> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_u8(&mut self) -> Result<u64, ClassFileError> {
        let bytes = self.read_bytes(8)?;
        Ok(u64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn read_many_u2(&mut self) -> Result<Vec<u16>, ClassFileError> {
        let count = self.read_u2()? as usize;
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(self.read_u2()?);
        }
        Ok(values)
    }

    fn read_modified_utf8(&mut self) -> Result<String, ClassFileError> {
        let length = self.read_u2()? as usize;
        let bytes = self.read_bytes(length)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| ClassFileError::InvalidModifiedUtf8)
    }

    fn read_bytes(&mut self, len: usize) -> Result<&'a [u8], ClassFileError> {
        let end = self.position.saturating_add(len);
        if end > self.bytes.len() {
            return Err(ClassFileError::UnexpectedEof {
                position: self.position,
                needed: end - self.bytes.len(),
            });
        }

        let slice = &self.bytes[self.position..end];
        self.position = end;
        Ok(slice)
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }

    fn is_finished(&self) -> bool {
        self.position == self.bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::{AttributeInfo, ClassFile, ClassFileError, ConstantPoolEntry};

    #[test]
    fn parses_minimal_class_file_and_code_attribute() {
        let class_file = ClassFile::parse(&minimal_class_bytes()).unwrap();

        assert_eq!(class_file.major_version, 65);
        assert_eq!(class_file.minor_version, 0);
        assert_eq!(class_file.class_name().unwrap(), "Main");
        assert_eq!(
            class_file.super_class_name().unwrap(),
            Some("java/lang/Object")
        );
        assert_eq!(class_file.methods.len(), 1);

        let method = class_file.find_method("main", "()I").unwrap().unwrap();
        let code = method.code().unwrap();
        assert_eq!(code.max_stack, 1);
        assert_eq!(code.max_locals, 0);
        assert_eq!(code.code, vec![0x08, 0xac]);
    }

    #[test]
    fn parses_constant_pool_entries() {
        let class_file = ClassFile::parse(&minimal_class_bytes()).unwrap();

        assert_eq!(
            class_file.constant_pool.get(1).unwrap(),
            &ConstantPoolEntry::Utf8("Main".to_string())
        );
        assert_eq!(class_file.constant_pool.class_name(2).unwrap(), "Main");
        assert_eq!(
            class_file.constant_pool.class_name(4).unwrap(),
            "java/lang/Object"
        );
    }

    #[test]
    fn rejects_invalid_magic() {
        let mut bytes = minimal_class_bytes();
        bytes[0] = 0;

        let error = ClassFile::parse(&bytes).unwrap_err();
        assert_eq!(error, ClassFileError::InvalidMagic { actual: 0x00febabe });
    }

    #[test]
    fn preserves_unknown_attributes_as_raw() {
        let class_file = ClassFile::parse(&class_with_raw_attribute_bytes()).unwrap();
        let method = class_file.find_method("main", "()I").unwrap().unwrap();

        match &method.attributes[1] {
            AttributeInfo::Raw(attribute) => {
                assert_eq!(attribute.name, "Synthetic");
                assert_eq!(attribute.info, vec![]);
            }
            other => panic!("expected raw attribute, got {other:?}"),
        }
    }

    fn minimal_class_bytes() -> Vec<u8> {
        vec![
            0xca, 0xfe, 0xba, 0xbe, // magic
            0x00, 0x00, // minor
            0x00, 0x41, // major = 65 (Java 21)
            0x00, 0x08, // constant_pool_count = 8
            0x01, 0x00, 0x04, b'M', b'a', b'i', b'n', // #1 Utf8 Main
            0x07, 0x00, 0x01, // #2 Class #1
            0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O',
            b'b', b'j', b'e', b'c', b't', // #3 Utf8 java/lang/Object
            0x07, 0x00, 0x03, // #4 Class #3
            0x01, 0x00, 0x04, b'm', b'a', b'i', b'n', // #5 Utf8 main
            0x01, 0x00, 0x03, b'(', b')', b'I', // #6 Utf8 ()I
            0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #7 Utf8 Code
            0x00, 0x21, // access_flags public super
            0x00, 0x02, // this_class
            0x00, 0x04, // super_class
            0x00, 0x00, // interfaces_count
            0x00, 0x00, // fields_count
            0x00, 0x01, // methods_count
            0x00, 0x09, // method access_flags public static
            0x00, 0x05, // name_index main
            0x00, 0x06, // descriptor_index ()I
            0x00, 0x01, // attributes_count
            0x00, 0x07, // attribute_name_index Code
            0x00, 0x00, 0x00, 0x0e, // attribute_length 14
            0x00, 0x01, // max_stack
            0x00, 0x00, // max_locals
            0x00, 0x00, 0x00, 0x02, // code_length
            0x08, 0xac, // iconst_5, ireturn
            0x00, 0x00, // exception_table_length
            0x00, 0x00, // attributes_count
            0x00, 0x00, // class attributes_count
        ]
    }

    fn class_with_raw_attribute_bytes() -> Vec<u8> {
        vec![
            0xca, 0xfe, 0xba, 0xbe, 0x00, 0x00, 0x00, 0x41, 0x00, 0x09, 0x01, 0x00, 0x04, b'M',
            b'a', b'i', b'n', 0x07, 0x00, 0x01, 0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/',
            b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j', b'e', b'c', b't', 0x07, 0x00, 0x03,
            0x01, 0x00, 0x04, b'm', b'a', b'i', b'n', 0x01, 0x00, 0x03, b'(', b')', b'I', 0x01,
            0x00, 0x04, b'C', b'o', b'd', b'e', 0x01, 0x00, 0x09, b'S', b'y', b'n', b't', b'h',
            b'e', b't', b'i', b'c', 0x00, 0x21, 0x00, 0x02, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x01, 0x00, 0x09, 0x00, 0x05, 0x00, 0x06, 0x00, 0x02, 0x00, 0x07, 0x00, 0x00,
            0x00, 0x0e, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x08, 0xac, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]
    }
}
