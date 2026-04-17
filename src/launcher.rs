use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::classfile::{
    AttributeInfo, ClassFile, ClassFileError, ConstantPoolEntry, MemberInfo, StackMapFrame,
};
use crate::vm::{
    ClassMethod, ExceptionHandler, ExecutionResult, FieldRef, InvokeDynamicKind,
    InvokeDynamicSite, Method, MethodRef, RuntimeClass, Value, Vm, VmError,
};
use zip::ZipArchive;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchOptions {
    pub class_path: Vec<PathBuf>,
    pub main_class: String,
    pub args: Vec<String>,
    pub trace: bool,
}

impl LaunchOptions {
    pub fn new(
        class_path: impl Into<PathBuf>,
        main_class: impl Into<String>,
        args: Vec<String>,
    ) -> Self {
        Self {
            class_path: vec![class_path.into()],
            main_class: main_class.into(),
            args,
            trace: false,
        }
    }

    pub fn with_class_path(
        class_path: Vec<PathBuf>,
        main_class: impl Into<String>,
        args: Vec<String>,
    ) -> Self {
        Self {
            class_path,
            main_class: main_class.into(),
            args,
            trace: false,
        }
    }
}

#[derive(Debug)]
pub enum LaunchError {
    MissingMainClassArgument,
    MissingClassPathValue,
    UnsupportedOption(String),
    MainArgumentsNotSupported {
        count: usize,
    },
    MainClassNotFound {
        main_class: String,
        path: PathBuf,
    },
    ClassFileParse {
        path: PathBuf,
        source: ClassFileError,
    },
    MainMethodNotFound {
        class_name: String,
    },
    InvalidMainMethod {
        class_name: String,
        reason: String,
    },
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Vm(VmError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassSource {
    File(PathBuf),
    JarEntry {
        jar_path: PathBuf,
        entry_name: String,
    },
}

impl fmt::Display for LaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingMainClassArgument => write!(f, "missing main class name"),
            Self::MissingClassPathValue => write!(f, "missing value for -cp/-classpath"),
            Self::UnsupportedOption(option) => write!(f, "unsupported option: {option}"),
            Self::MainArgumentsNotSupported { count } => {
                write!(f, "main arguments are not supported yet (received {count})")
            }
            Self::MainClassNotFound { main_class, path } => write!(
                f,
                "error: could not find or load main class {main_class}\n\
                 Caused by: {} does not exist",
                path.display()
            ),
            Self::ClassFileParse { path, source } => {
                write!(f, "failed to parse class file {}: {source}", path.display())
            }
            Self::MainMethodNotFound { class_name } => {
                write!(
                    f,
                    "error: could not find or load main class {class_name}\n\
                     Caused by: the class does not contain a public static void main(String[]) method"
                )
            }
            Self::InvalidMainMethod { class_name, reason } => {
                write!(f, "invalid main method in class {class_name}: {reason}")
            }
            Self::Io { path, source } => {
                write!(f, "failed to read {}: {source}", path.display())
            }
            Self::Vm(error) => write!(f, "vm execution failed: {error}"),
        }
    }
}

impl std::error::Error for LaunchError {}

impl From<VmError> for LaunchError {
    fn from(value: VmError) -> Self {
        Self::Vm(value)
    }
}

pub fn parse_launch_options(args: &[String]) -> Result<LaunchOptions, LaunchError> {
    let mut class_path: Vec<PathBuf> = vec![PathBuf::from(".")];
    let mut main_class = None;
    let mut program_args = Vec::new();
    let mut trace = false;

    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "-cp" | "-classpath" => {
                index += 1;
                let value = args.get(index).ok_or(LaunchError::MissingClassPathValue)?;
                class_path = value.split(':').map(PathBuf::from).collect();
            }
            "-Xtrace" => {
                trace = true;
            }
            "-h" | "--help" | "help" => {
                return Err(LaunchError::UnsupportedOption(arg.clone()));
            }
            option if option.starts_with('-') => {
                return Err(LaunchError::UnsupportedOption(option.to_string()));
            }
            class_name => {
                main_class = Some(class_name.to_string());
                program_args.extend_from_slice(&args[index + 1..]);
                break;
            }
        }
        index += 1;
    }

    let main_class = main_class.ok_or(LaunchError::MissingMainClassArgument)?;

    let mut options = LaunchOptions::with_class_path(class_path, main_class, program_args);
    options.trace = trace;
    Ok(options)
}

pub fn launch(options: &LaunchOptions) -> Result<ExecutionResult, LaunchError> {
    let source = resolve_class_path(&options.class_path, &options.main_class)
        .ok_or_else(|| LaunchError::MainClassNotFound {
            main_class: options.main_class.clone(),
            path: class_relative_path(&options.main_class),
        })?;
    let mut vm = Vm::new();
    vm.set_class_path(options.class_path.clone());
    vm.set_trace(options.trace);
    let method = load_main_method(&source, &options.main_class, &options.args, &mut vm)?;
    vm.execute(method).map_err(LaunchError::from)
}

/// Build the relative `.class` file path for a fully-qualified class name.
///
/// Accepts both `demo.Main` (Java-style) and `demo/Main` (internal-style).
pub fn class_relative_path(class_name: &str) -> PathBuf {
    let normalized = class_name.replace('.', "/");
    let mut relative = PathBuf::new();
    for segment in normalized.split('/') {
        relative.push(segment);
    }
    relative.set_extension("class");
    relative
}

/// Search classpath entries for a `.class` file matching the given class name.
pub fn resolve_class_path(class_path: &[PathBuf], class_name: &str) -> Option<ClassSource> {
    let relative = class_relative_path(class_name);
    let relative_name = relative.to_string_lossy().replace('\\', "/");
    for entry in class_path {
        if is_jar_path(entry) {
            if jar_contains_class(entry, &relative_name) {
                return Some(ClassSource::JarEntry {
                    jar_path: entry.clone(),
                    entry_name: relative_name.clone(),
                });
            }
        } else {
            let candidate = entry.join(&relative);
            if candidate.exists() {
                return Some(ClassSource::File(candidate));
            }
        }
    }
    None
}

/// Compatibility wrapper: build a class file path from a single classpath entry.
pub fn main_class_path(class_path: &Path, main_class: &str) -> PathBuf {
    class_path.join(class_relative_path(main_class))
}

pub fn load_class_file(source: &ClassSource, main_class: &str) -> Result<ClassFile, LaunchError> {
    let (display_path, bytes) = read_class_source(source, main_class)?;
    ClassFile::parse(&bytes).map_err(|source| LaunchError::ClassFileParse {
        path: display_path,
        source,
    })
}

pub fn load_main_method(
    source: &ClassSource,
    main_class: &str,
    args: &[String],
    vm: &mut Vm,
) -> Result<Method, LaunchError> {
    let class_file = load_class_file(source, main_class)?;
    let class_name = class_file.class_name().unwrap_or(main_class);

    // Register the full class (all methods + fields) so that invokevirtual,
    // invokestatic, getfield, etc. can resolve members at runtime.
    register_class(class_name, &class_file, vm)?;

    let method = select_main_method(&class_file, class_name)?;
    method_to_runtime_method(&class_file, method, class_name, args, vm)
}

/// Load a `.class` file from disk and register it with the VM.
pub fn load_and_register_class(
    class_path: &Path,
    class_name: &str,
    vm: &mut Vm,
) -> Result<(), LaunchError> {
    let source = if is_jar_path(class_path) {
        ClassSource::JarEntry {
            jar_path: class_path.to_path_buf(),
            entry_name: class_relative_path(class_name)
                .to_string_lossy()
                .replace('\\', "/"),
        }
    } else {
        ClassSource::File(main_class_path(class_path, class_name))
    };
    load_and_register_class_from(&source, class_name, vm)
}

/// Load and register a class from an already-resolved file path.
pub fn load_and_register_class_from(
    source: &ClassSource,
    class_name: &str,
    vm: &mut Vm,
) -> Result<(), LaunchError> {
    let class_file = load_class_file(source, class_name)?;
    let resolved_name = class_file.class_name().unwrap_or(class_name);
    register_class(resolved_name, &class_file, vm)
}

fn is_jar_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("jar"))
        .unwrap_or(false)
}

fn jar_contains_class(jar_path: &Path, entry_name: &str) -> bool {
    let Ok(file) = fs::File::open(jar_path) else {
        return false;
    };
    let Ok(mut archive) = ZipArchive::new(file) else {
        return false;
    };
    archive.by_name(entry_name).is_ok()
}

fn read_class_source(
    source: &ClassSource,
    main_class: &str,
) -> Result<(PathBuf, Vec<u8>), LaunchError> {
    match source {
        ClassSource::File(path) => {
            let bytes = fs::read(path).map_err(|source| {
                if source.kind() == std::io::ErrorKind::NotFound {
                    LaunchError::MainClassNotFound {
                        main_class: main_class.to_string(),
                        path: path.to_path_buf(),
                    }
                } else {
                    LaunchError::Io {
                        path: path.to_path_buf(),
                        source,
                    }
                }
            })?;
            Ok((path.clone(), bytes))
        }
        ClassSource::JarEntry {
            jar_path,
            entry_name,
        } => {
            let file = fs::File::open(jar_path).map_err(|source| LaunchError::Io {
                path: jar_path.clone(),
                source,
            })?;
            let mut archive = ZipArchive::new(file).map_err(|source| LaunchError::Io {
                path: jar_path.clone(),
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, source.to_string()),
            })?;
            let mut entry = archive.by_name(entry_name).map_err(|_| LaunchError::MainClassNotFound {
                main_class: main_class.to_string(),
                path: PathBuf::from(format!("{}!/{entry_name}", jar_path.display())),
            })?;
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(|source| LaunchError::Io {
                path: PathBuf::from(format!("{}!/{entry_name}", jar_path.display())),
                source,
            })?;
            Ok((PathBuf::from(format!("{}!/{entry_name}", jar_path.display())), bytes))
        }
    }
}

fn register_class(
    class_name: &str,
    class_file: &ClassFile,
    vm: &mut Vm,
) -> Result<(), LaunchError> {
    let mut methods = std::collections::BTreeMap::new();

    for member in &class_file.methods {
        let name = member
            .name(&class_file.constant_pool)
            .map_err(|e| LaunchError::ClassFileParse {
                path: PathBuf::new(),
                source: e,
            })?
            .to_string();
        let descriptor = member
            .descriptor(&class_file.constant_pool)
            .map_err(|e| LaunchError::ClassFileParse {
                path: PathBuf::new(),
                source: e,
            })?
            .to_string();

        if let Some(code) = member.code() {
            let method = Method::with_constant_pool(
                code.code.clone(),
                code.max_locals as usize,
                code.max_stack as usize,
                extract_runtime_constants(class_file, vm),
            )
            .with_metadata(class_name, &name, &descriptor, member.access_flags)
            .with_reference_classes(extract_reference_classes(class_file))
            .with_field_refs(extract_field_refs(class_file))
            .with_method_refs(extract_method_refs(class_file))
            .with_exception_handlers(extract_exception_handlers(class_file, code))
            .with_line_numbers(extract_line_numbers(code))
            .with_stack_map_frames(extract_stack_map_frames(code))
            .with_invoke_dynamic_sites(extract_invoke_dynamic_sites(class_file));

            // Best-effort verification: log but don't fail on verification errors
            // since some javac output may use patterns the verifier doesn't handle yet.
            if let Err(e) = Vm::verify_method(&method) {
                eprintln!("warning: bytecode verification failed for {class_name}.{name}{descriptor}: {e}");
            }

            methods.insert((name, descriptor), ClassMethod::Bytecode(method));
        }
    }

    // Extract instance field definitions (name, descriptor) from the class file.
    let mut instance_fields = Vec::new();
    for field in &class_file.fields {
        let is_static = field.access_flags & 0x0008 != 0;
        if !is_static {
            let name = field
                .name(&class_file.constant_pool)
                .map_err(|e| LaunchError::ClassFileParse {
                    path: PathBuf::new(),
                    source: e,
                })?
                .to_string();
            let descriptor = field
                .descriptor(&class_file.constant_pool)
                .map_err(|e| LaunchError::ClassFileParse {
                    path: PathBuf::new(),
                    source: e,
                })?
                .to_string();
            instance_fields.push((name, descriptor));
        }
    }

    let super_class = class_file
        .super_class_name()
        .ok()
        .flatten()
        .map(str::to_string);

    let interfaces = class_file
        .interface_names()
        .unwrap_or_default()
        .into_iter()
        .map(str::to_string)
        .collect();

    vm.register_class(RuntimeClass {
        name: class_name.to_string(),
        super_class,
        methods,
        static_fields: std::collections::BTreeMap::new(),
        instance_fields,
        interfaces,
    });

    Ok(())
}

fn select_main_method<'a>(
    class_file: &'a ClassFile,
    class_name: &str,
) -> Result<&'a MemberInfo, LaunchError> {
    if let Some(method) = class_file
        .find_method("main", "([Ljava/lang/String;)V")
        .map_err(|error| invalid_main_method(class_name, error.to_string()))?
    {
        return Ok(method);
    }
    if let Some(method) = class_file
        .find_method("main", "()I")
        .map_err(|error| invalid_main_method(class_name, error.to_string()))?
    {
        return Ok(method);
    }
    if let Some(method) = class_file
        .find_method("main", "()V")
        .map_err(|error| invalid_main_method(class_name, error.to_string()))?
    {
        return Ok(method);
    }
    Err(LaunchError::MainMethodNotFound {
        class_name: class_name.to_string(),
    })
}

fn method_to_runtime_method(
    class_file: &ClassFile,
    method: &MemberInfo,
    class_name: &str,
    args: &[String],
    vm: &mut Vm,
) -> Result<Method, LaunchError> {
    if method.access_flags & 0x0008 == 0 {
        return Err(invalid_main_method(
            class_name,
            "main method must be static",
        ));
    }
    if method.access_flags & 0x0001 == 0 {
        return Err(invalid_main_method(
            class_name,
            "main method must be public",
        ));
    }
    let descriptor = method
        .descriptor(&class_file.constant_pool)
        .map_err(|error| invalid_main_method(class_name, error.to_string()))?;

    let code = method.code().ok_or_else(|| {
        invalid_main_method(class_name, "main method is missing a Code attribute")
    })?;

    let method = Method::with_constant_pool(
        code.code.clone(),
        code.max_locals as usize,
        code.max_stack as usize,
        extract_runtime_constants(class_file, vm),
    )
    .with_metadata(class_name, "main", descriptor, method.access_flags)
    .with_reference_classes(extract_reference_classes(class_file))
    .with_field_refs(extract_field_refs(class_file))
    .with_method_refs(extract_method_refs(class_file))
    .with_exception_handlers(extract_exception_handlers(class_file, code))
    .with_stack_map_frames(extract_stack_map_frames(code))
    .with_invoke_dynamic_sites(extract_invoke_dynamic_sites(class_file));

    match descriptor {
        "([Ljava/lang/String;)V" => {
            let args_array = vm.new_string_array(args);
            Ok(method.with_initial_locals([Some(args_array)]))
        }
        "()I" | "()V" => {
            if args.is_empty() {
                Ok(method)
            } else {
                Err(LaunchError::MainArgumentsNotSupported { count: args.len() })
            }
        }
        other => Err(invalid_main_method(
            class_name,
            format!("unsupported main descriptor {other}"),
        )),
    }
}

fn extract_runtime_constants(class_file: &ClassFile, vm: &mut Vm) -> Vec<Option<Value>> {
    let mut constants = Vec::with_capacity(class_file.constant_pool.len());
    constants.push(None);

    for index in 1..class_file.constant_pool.len() {
        let value = match class_file.constant_pool.get(index as u16) {
            Ok(ConstantPoolEntry::Integer(value)) => Some(Value::Int(*value)),
            Ok(ConstantPoolEntry::Long(value)) => Some(Value::Long(*value)),
            Ok(ConstantPoolEntry::Float(value)) => Some(Value::Float(*value)),
            Ok(ConstantPoolEntry::Double(value)) => Some(Value::Double(*value)),
            Ok(ConstantPoolEntry::String { string_index }) => class_file
                .constant_pool
                .utf8(*string_index)
                .ok()
                .map(|value| vm.new_string(value.to_string())),
            _ => None,
        };
        constants.push(value);
    }

    constants
}

fn extract_reference_classes(class_file: &ClassFile) -> Vec<Option<String>> {
    let mut classes = Vec::with_capacity(class_file.constant_pool.len());
    classes.push(None);

    for index in 1..class_file.constant_pool.len() {
        let value = match class_file.constant_pool.get(index as u16) {
            Ok(ConstantPoolEntry::Class { name_index }) => class_file
                .constant_pool
                .utf8(*name_index)
                .ok()
                .map(str::to_string),
            _ => None,
        };
        classes.push(value);
    }

    classes
}

fn extract_field_refs(class_file: &ClassFile) -> Vec<Option<FieldRef>> {
    let mut field_refs = Vec::with_capacity(class_file.constant_pool.len());
    field_refs.push(None);

    for index in 1..class_file.constant_pool.len() {
        let value = resolve_field_ref(class_file, index as u16).ok();
        field_refs.push(value);
    }

    field_refs
}

fn extract_method_refs(class_file: &ClassFile) -> Vec<Option<MethodRef>> {
    let mut method_refs = Vec::with_capacity(class_file.constant_pool.len());
    method_refs.push(None);

    for index in 1..class_file.constant_pool.len() {
        let value = resolve_method_ref(class_file, index as u16).ok();
        method_refs.push(value);
    }

    method_refs
}

fn extract_exception_handlers(
    class_file: &ClassFile,
    code: &crate::classfile::CodeAttribute,
) -> Vec<ExceptionHandler> {
    code.exception_table
        .iter()
        .map(|entry| {
            let catch_class = if entry.catch_type == 0 {
                None
            } else {
                class_file
                    .constant_pool
                    .class_name(entry.catch_type)
                    .ok()
                    .map(str::to_string)
            };
            ExceptionHandler {
                start_pc: entry.start_pc,
                end_pc: entry.end_pc,
                handler_pc: entry.handler_pc,
                catch_class,
            }
        })
        .collect()
}

fn extract_invoke_dynamic_sites(class_file: &ClassFile) -> Vec<Option<InvokeDynamicSite>> {
    let bootstrap_methods = class_file.bootstrap_methods();
    let mut sites = Vec::with_capacity(class_file.constant_pool.len());
    sites.push(None);

    for index in 1..class_file.constant_pool.len() {
        let site = match class_file.constant_pool.get(index as u16) {
            Ok(ConstantPoolEntry::InvokeDynamic {
                bootstrap_method_attr_index,
                name_and_type_index,
            }) => {
                let (name, descriptor) =
                    resolve_name_and_type(class_file, *name_and_type_index)
                        .unwrap_or_default();
                let kind = bootstrap_methods
                    .get(*bootstrap_method_attr_index as usize)
                    .map(|bm| resolve_invoke_dynamic_kind(class_file, bm))
                    .unwrap_or(InvokeDynamicKind::Unknown);

                Some(InvokeDynamicSite {
                    name,
                    descriptor,
                    bootstrap_method_index: *bootstrap_method_attr_index,
                    kind,
                })
            }
            _ => None,
        };
        sites.push(site);
    }

    sites
}

fn resolve_invoke_dynamic_kind(
    class_file: &ClassFile,
    bootstrap_method: &crate::classfile::BootstrapMethod,
) -> InvokeDynamicKind {
    let Ok((bootstrap_class, bootstrap_name, _)) =
        resolve_bootstrap_method(class_file, bootstrap_method.method_ref)
    else {
        return InvokeDynamicKind::Unknown;
    };

    match (bootstrap_class.as_str(), bootstrap_name.as_str()) {
        ("java/lang/invoke/LambdaMetafactory", "metafactory")
        | ("java/lang/invoke/LambdaMetafactory", "altMetafactory") => bootstrap_method
            .arguments
            .get(1)
            .and_then(|mh_idx| resolve_method_handle_target(class_file, *mh_idx))
            .map(|mr| InvokeDynamicKind::LambdaProxy {
                target_class: mr.class_name,
                target_method: mr.method_name,
                target_descriptor: mr.descriptor,
            })
            .unwrap_or(InvokeDynamicKind::Unknown),
        ("java/lang/invoke/StringConcatFactory", "makeConcat")
        | ("java/lang/invoke/StringConcatFactory", "makeConcatWithConstants") => {
            let recipe = bootstrap_method
                .arguments
                .first()
                .and_then(|index| constant_string_value(class_file, *index));
            let constants = bootstrap_method
                .arguments
                .iter()
                .skip(1)
                .filter_map(|index| constant_string_value(class_file, *index))
                .collect();
            InvokeDynamicKind::StringConcat { recipe, constants }
        }
        _ => InvokeDynamicKind::Unknown,
    }
}

fn resolve_bootstrap_method(
    class_file: &ClassFile,
    method_handle_index: u16,
) -> Result<(String, String, String), ClassFileError> {
    let ConstantPoolEntry::MethodHandle {
        reference_index, ..
    } = class_file.constant_pool.get(method_handle_index)?
    else {
        return Err(ClassFileError::UnexpectedConstantType {
            index: method_handle_index,
            expected: "MethodHandle",
            actual: class_file.constant_pool.get(method_handle_index)?.kind_name(),
        });
    };
    let method_ref = resolve_method_ref(class_file, *reference_index)?;
    Ok((
        method_ref.class_name,
        method_ref.method_name,
        method_ref.descriptor,
    ))
}

fn resolve_method_handle_target(class_file: &ClassFile, method_handle_index: u16) -> Option<MethodRef> {
    let Ok(ConstantPoolEntry::MethodHandle {
        reference_index, ..
    }) = class_file.constant_pool.get(method_handle_index)
    else {
        return None;
    };
    resolve_method_ref(class_file, *reference_index).ok()
}

fn constant_string_value(class_file: &ClassFile, index: u16) -> Option<String> {
    match class_file.constant_pool.get(index).ok()? {
        ConstantPoolEntry::String { string_index } => {
            class_file.constant_pool.utf8(*string_index).ok().map(str::to_string)
        }
        ConstantPoolEntry::Utf8(value) => Some(value.clone()),
        ConstantPoolEntry::Integer(value) => Some(value.to_string()),
        ConstantPoolEntry::Long(value) => Some(value.to_string()),
        ConstantPoolEntry::Float(value) => Some(value.to_string()),
        ConstantPoolEntry::Double(value) => Some(value.to_string()),
        ConstantPoolEntry::Class { name_index } => {
            class_file.constant_pool.utf8(*name_index).ok().map(str::to_string)
        }
        _ => None,
    }
}

fn extract_line_numbers(code: &crate::classfile::CodeAttribute) -> Vec<(u16, u16)> {
    for attr in &code.attributes {
        if let AttributeInfo::LineNumberTable(entries) = attr {
            return entries.iter().map(|e| (e.start_pc, e.line_number)).collect();
        }
    }
    Vec::new()
}

fn extract_stack_map_frames(code: &crate::classfile::CodeAttribute) -> Vec<StackMapFrame> {
    for attr in &code.attributes {
        if let AttributeInfo::StackMapTable(frames) = attr {
            return frames.clone();
        }
    }
    Vec::new()
}

fn resolve_field_ref(class_file: &ClassFile, index: u16) -> Result<FieldRef, ClassFileError> {
    match class_file.constant_pool.get(index)? {
        ConstantPoolEntry::Fieldref {
            class_index,
            name_and_type_index,
        } => {
            let class_name = class_file.constant_pool.class_name(*class_index)?.to_string();
            let (field_name, descriptor) =
                resolve_name_and_type(class_file, *name_and_type_index)?;
            Ok(FieldRef {
                class_name,
                field_name,
                descriptor,
            })
        }
        entry => Err(ClassFileError::UnexpectedConstantType {
            index,
            expected: "Fieldref",
            actual: entry.kind_name(),
        }),
    }
}

fn resolve_method_ref(class_file: &ClassFile, index: u16) -> Result<MethodRef, ClassFileError> {
    match class_file.constant_pool.get(index)? {
        ConstantPoolEntry::Methodref {
            class_index,
            name_and_type_index,
        }
        | ConstantPoolEntry::InterfaceMethodref {
            class_index,
            name_and_type_index,
        } => {
            let class_name = class_file.constant_pool.class_name(*class_index)?.to_string();
            let (method_name, descriptor) =
                resolve_name_and_type(class_file, *name_and_type_index)?;
            Ok(MethodRef {
                class_name,
                method_name,
                descriptor,
            })
        }
        entry => Err(ClassFileError::UnexpectedConstantType {
            index,
            expected: "Methodref or InterfaceMethodref",
            actual: entry.kind_name(),
        }),
    }
}

fn resolve_name_and_type(
    class_file: &ClassFile,
    index: u16,
) -> Result<(String, String), ClassFileError> {
    match class_file.constant_pool.get(index)? {
        ConstantPoolEntry::NameAndType {
            name_index,
            descriptor_index,
        } => Ok((
            class_file.constant_pool.utf8(*name_index)?.to_string(),
            class_file.constant_pool.utf8(*descriptor_index)?.to_string(),
        )),
        entry => Err(ClassFileError::UnexpectedConstantType {
            index,
            expected: "NameAndType",
            actual: entry.kind_name(),
        }),
    }
}

fn invalid_main_method(class_name: &str, reason: impl Into<String>) -> LaunchError {
    LaunchError::InvalidMainMethod {
        class_name: class_name.to_string(),
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::vm::{ExecutionResult, Value, Vm};

    use super::{
        LaunchError, LaunchOptions, launch, load_main_method, main_class_path,
        parse_launch_options, resolve_class_path,
    };

    #[test]
    fn parses_java_like_launch_options() {
        let args = vec![
            "-cp".to_string(),
            "examples".to_string(),
            "demo.Main".to_string(),
        ];

        let options = parse_launch_options(&args).unwrap();
        assert_eq!(options.class_path, vec![PathBuf::from("examples")]);
        assert_eq!(options.main_class, "demo.Main");
        assert!(options.args.is_empty());
    }

    #[test]
    fn launches_main_class_from_real_class_file() {
        let root = temp_dir("launches_main_class_from_real_class_file");
        let class_file = main_class_path(&root, "demo.Main");
        fs::create_dir_all(class_file.parent().unwrap()).unwrap();
        fs::write(&class_file, demo_main_class_bytes()).unwrap();

        let options = LaunchOptions::new(&root, "demo.Main", vec![]);
        let result = launch(&options).unwrap();

        assert_eq!(result, ExecutionResult::Value(Value::Int(60)));
    }

    #[test]
    fn launches_standard_main_with_string_args() {
        let root = temp_dir("launches_standard_main_with_string_args");
        let class_file = main_class_path(&root, "demo.Main");
        fs::create_dir_all(class_file.parent().unwrap()).unwrap();
        fs::write(&class_file, standard_main_class_bytes()).unwrap();

        let options =
            LaunchOptions::new(&root, "demo.Main", vec!["a".to_string(), "b".to_string()]);
        let result = launch(&options).unwrap();
        assert_eq!(result, ExecutionResult::Void);
    }

    #[test]
    fn loads_real_class_that_prints_ints_and_strings() {
        let root = temp_dir("loads_real_class_that_prints_ints_and_strings");
        let source_dir = root.join("demo");
        fs::create_dir_all(&source_dir).unwrap();
        let source_file = source_dir.join("Main.java");
        fs::write(
            &source_file,
            r#"package demo;

public class Main {
    public static void main(String[] args) {
        System.out.println(123);
        System.out.println("hi");
    }
}
"#,
        )
        .unwrap();

        let output = Command::new("javac")
            .arg("--release")
            .arg("8")
            .arg("-d")
            .arg(&root)
            .arg(&source_file)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "javac failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let source = resolve_class_path(&[root.clone()], "demo.Main").unwrap();
        let mut vm = Vm::new();
        let method = load_main_method(&source, "demo.Main", &[], &mut vm).unwrap();
        let result = vm.execute(method).unwrap();

        assert_eq!(result, ExecutionResult::Void);
        assert_eq!(vm.take_output(), vec!["123".to_string(), "hi".to_string()]);
    }

    #[test]
    fn loads_main_class_from_jar_classpath() {
        let root = temp_dir("loads_main_class_from_jar_classpath");
        let source_dir = root.join("demo");
        fs::create_dir_all(&source_dir).unwrap();
        let source_file = source_dir.join("Main.java");
        fs::write(
            &source_file,
            r#"package demo;

public class Main {
    public static void main(String[] args) {
        System.out.println(7);
    }
}
"#,
        )
        .unwrap();

        let output = Command::new("javac")
            .arg("--release")
            .arg("8")
            .arg("-d")
            .arg(&root)
            .arg(&source_file)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "javac failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let jar_path = root.join("demo.jar");
        let output = Command::new("jar")
            .arg("--create")
            .arg("--file")
            .arg(&jar_path)
            .arg("-C")
            .arg(&root)
            .arg("demo")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "jar failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let options = LaunchOptions::new(&jar_path, "demo.Main", vec![]);
        let result = launch(&options).unwrap();
        assert_eq!(result, ExecutionResult::Void);
    }

    #[test]
    fn parses_classes_from_jar_source() {
        let root = temp_dir("parses_classes_from_jar_source");
        let source_dir = root.join("demo");
        fs::create_dir_all(&source_dir).unwrap();
        let source_file = source_dir.join("Main.java");
        fs::write(
            &source_file,
            r#"package demo;

public class Main {
    public static void main(String[] args) {
        System.out.println("jar");
    }
}
"#,
        )
        .unwrap();

        let output = Command::new("javac")
            .arg("--release")
            .arg("8")
            .arg("-d")
            .arg(&root)
            .arg(&source_file)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "javac failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let jar_path = root.join("demo.jar");
        let output = Command::new("jar")
            .arg("--create")
            .arg("--file")
            .arg(&jar_path)
            .arg("-C")
            .arg(&root)
            .arg("demo")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "jar failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let source = resolve_class_path(&[jar_path], "demo.Main").unwrap();
        let mut vm = Vm::new();
        let method = load_main_method(&source, "demo.Main", &[], &mut vm).unwrap();
        let result = vm.execute(method).unwrap();
        assert_eq!(result, ExecutionResult::Void);
    }

    #[test]
    fn rejects_program_arguments_until_reference_types_exist() {
        let root = temp_dir("rejects_program_arguments_until_reference_types_exist");
        let class_file = main_class_path(&root, "Main");
        fs::write(&class_file, demo_main_class_bytes()).unwrap();

        let options = LaunchOptions::new(&root, "Main", vec!["hello".to_string()]);
        let error = launch(&options).unwrap_err();

        assert!(matches!(
            error,
            LaunchError::MainArgumentsNotSupported { count: 1 }
        ));
    }

    #[test]
    fn runs_class_with_try_catch() {
        let root = temp_dir("runs_class_with_try_catch");
        let source_dir = root.join("demo");
        fs::create_dir_all(&source_dir).unwrap();
        let source_file = source_dir.join("TryCatch.java");
        fs::write(
            &source_file,
            r#"package demo;

public class TryCatch {
    public static void main(String[] args) {
        try {
            int x = 1 / 0;
            System.out.println("unreachable");
        } catch (ArithmeticException e) {
            System.out.println("caught");
        }
        System.out.println("done");
    }
}
"#,
        )
        .unwrap();

        let output = Command::new("javac")
            .arg("--release")
            .arg("8")
            .arg("-d")
            .arg(&root)
            .arg(&source_file)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "javac failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let options = LaunchOptions::new(&root, "demo.TryCatch", vec![]);
        let result = launch(&options).unwrap();

        assert_eq!(result, ExecutionResult::Void);
    }

    #[test]
    fn runs_class_with_static_initializer() {
        let root = temp_dir("runs_class_with_static_initializer");
        let source_dir = root.join("demo");
        fs::create_dir_all(&source_dir).unwrap();
        let source_file = source_dir.join("StaticInit.java");
        fs::write(
            &source_file,
            r#"package demo;

public class StaticInit {
    static int BASE = 100;
    static int OFFSET;

    static {
        OFFSET = BASE + 23;
    }

    public static void main(String[] args) {
        System.out.println(OFFSET);
    }
}
"#,
        )
        .unwrap();

        let output = Command::new("javac")
            .arg("--release")
            .arg("8")
            .arg("-d")
            .arg(&root)
            .arg(&source_file)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "javac failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let options = LaunchOptions::new(&root, "demo.StaticInit", vec![]);
        let result = launch(&options).unwrap();
        assert_eq!(result, ExecutionResult::Void);
    }

    #[test]
    fn runs_class_with_string_methods() {
        let root = temp_dir("runs_class_with_string_methods");
        let source_dir = root.join("demo");
        fs::create_dir_all(&source_dir).unwrap();
        let source_file = source_dir.join("StringDemo.java");
        fs::write(
            &source_file,
            r#"package demo;

public class StringDemo {
    public static void main(String[] args) {
        String s = "hello";
        System.out.println(s.length());
        System.out.println(s.equals("hello"));
        System.out.println(s.equals("world"));
    }
}
"#,
        )
        .unwrap();

        let output = Command::new("javac")
            .arg("--release")
            .arg("8")
            .arg("-d")
            .arg(&root)
            .arg(&source_file)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "javac failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let options = LaunchOptions::new(&root, "demo.StringDemo", vec![]);
        let result = launch(&options).unwrap();
        assert_eq!(result, ExecutionResult::Void);
    }

    #[test]
    fn runs_class_with_string_concatenation() {
        let root = temp_dir("runs_class_with_string_concatenation");
        let source_dir = root.join("demo");
        fs::create_dir_all(&source_dir).unwrap();
        let source_file = source_dir.join("Concat.java");
        fs::write(
            &source_file,
            r#"package demo;

public class Concat {
    public static void main(String[] args) {
        String name = "world";
        System.out.println("hello " + name + "!");
        int x = 42;
        System.out.println("x=" + x);
    }
}
"#,
        )
        .unwrap();

        let output = Command::new("javac")
            .arg("--release")
            .arg("8")
            .arg("-d")
            .arg(&root)
            .arg(&source_file)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "javac failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let options = LaunchOptions::new(&root, "demo.Concat", vec![]);
        let result = launch(&options).unwrap();
        assert_eq!(result, ExecutionResult::Void);
    }

    #[test]
    fn runs_class_with_math_and_inheritance() {
        let root = temp_dir("runs_class_with_math_and_inheritance");
        let source_dir = root.join("demo");
        fs::create_dir_all(&source_dir).unwrap();

        // Base class
        fs::write(
            source_dir.join("Shape.java"),
            r#"package demo;
public class Shape {
    public String name() { return "shape"; }
    public int area() { return 0; }
}
"#,
        )
        .unwrap();

        // Subclass
        fs::write(
            source_dir.join("Rect.java"),
            r#"package demo;
public class Rect extends Shape {
    private int w;
    private int h;
    public Rect(int w, int h) { this.w = w; this.h = h; }
    public String name() { return "rect"; }
    public int area() { return w * h; }
}
"#,
        )
        .unwrap();

        // Main
        fs::write(
            source_dir.join("MathDemo.java"),
            r#"package demo;
public class MathDemo {
    public static void main(String[] args) {
        Rect r = new Rect(3, 7);
        System.out.println(r.name());
        System.out.println(r.area());
        System.out.println(Math.max(10, 20));
        System.out.println(Math.min(10, 20));
        System.out.println(Math.abs(-5));
    }
}
"#,
        )
        .unwrap();

        let output = Command::new("javac")
            .arg("--release")
            .arg("8")
            .arg("-d")
            .arg(&root)
            .arg(source_dir.join("Shape.java"))
            .arg(source_dir.join("Rect.java"))
            .arg(source_dir.join("MathDemo.java"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "javac failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let options = LaunchOptions::new(&root, "demo.MathDemo", vec![]);
        let result = launch(&options).unwrap();
        assert_eq!(result, ExecutionResult::Void);
    }

    #[test]
    fn runs_class_with_multidim_arrays_and_double_math() {
        let root = temp_dir("runs_class_with_multidim_arrays_and_double_math");
        let source_dir = root.join("demo");
        fs::create_dir_all(&source_dir).unwrap();
        let source_file = source_dir.join("Arrays.java");
        fs::write(
            &source_file,
            r#"package demo;

public class Arrays {
    public static void main(String[] args) {
        // Multi-dimensional array
        int[][] matrix = new int[2][3];
        matrix[0][0] = 1;
        matrix[1][2] = 42;
        System.out.println(matrix[1][2]);

        // Long array
        long[] longs = new long[3];
        longs[0] = 1000000000L;
        longs[1] = 2000000000L;
        longs[2] = longs[0] + longs[1];
        System.out.println(longs[2]);

        // Double array
        double[] doubles = new double[2];
        doubles[0] = 3.14;
        doubles[1] = Math.sqrt(2.0);
        System.out.println(doubles[0]);
    }
}
"#,
        )
        .unwrap();

        let output = Command::new("javac")
            .arg("--release")
            .arg("8")
            .arg("-d")
            .arg(&root)
            .arg(&source_file)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "javac failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let options = LaunchOptions::new(&root, "demo.Arrays", vec![]);
        let result = launch(&options).unwrap();
        assert_eq!(result, ExecutionResult::Void);
    }

    fn temp_dir(test_name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("jvm-rs-{test_name}-{nanos}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn runs_class_with_fields_constructor_and_method_calls() {
        let root = temp_dir("runs_class_with_fields_constructor_and_method_calls");
        let source_dir = root.join("demo");
        fs::create_dir_all(&source_dir).unwrap();
        let source_file = source_dir.join("Counter.java");
        fs::write(
            &source_file,
            r#"package demo;

public class Counter {
    private int count;

    public Counter() {
        this.count = 0;
    }

    public void increment() {
        this.count = this.count + 1;
    }

    public int getCount() {
        return this.count;
    }

    public static void main(String[] args) {
        Counter c = new Counter();
        c.increment();
        c.increment();
        c.increment();
        System.out.println(c.getCount());
    }
}
"#,
        )
        .unwrap();

        let output = Command::new("javac")
            .arg("--release")
            .arg("8")
            .arg("-d")
            .arg(&root)
            .arg(&source_file)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "javac failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let options = LaunchOptions::new(&root, "demo.Counter", vec![]);
        let result = launch(&options).unwrap();

        assert_eq!(result, ExecutionResult::Void);
    }

    #[test]
    fn runs_class_with_static_methods_and_switch() {
        let root = temp_dir("runs_class_with_static_methods_and_switch");
        let source_dir = root.join("demo");
        fs::create_dir_all(&source_dir).unwrap();
        let source_file = source_dir.join("Calc.java");
        fs::write(
            &source_file,
            r#"package demo;

public class Calc {
    public static int add(int a, int b) {
        return a + b;
    }

    public static int factorial(int n) {
        int result = 1;
        for (int i = 2; i <= n; i++) {
            result = result * i;
        }
        return result;
    }

    public static String dayName(int day) {
        switch (day) {
            case 1: return "Mon";
            case 2: return "Tue";
            case 3: return "Wed";
            default: return "Other";
        }
    }

    public static void main(String[] args) {
        System.out.println(add(10, 20));
        System.out.println(factorial(5));
        System.out.println(dayName(2));
    }
}
"#,
        )
        .unwrap();

        let output = Command::new("javac")
            .arg("--release")
            .arg("8")
            .arg("-d")
            .arg(&root)
            .arg(&source_file)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "javac failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let options = LaunchOptions::new(&root, "demo.Calc", vec![]);
        let result = launch(&options).unwrap();

        assert_eq!(result, ExecutionResult::Void);
    }

    fn demo_main_class_bytes() -> Vec<u8> {
        vec![
            0xca, 0xfe, 0xba, 0xbe, 0x00, 0x00, 0x00, 0x41, 0x00, 0x08, 0x01, 0x00, 0x04, b'M',
            b'a', b'i', b'n', 0x07, 0x00, 0x01, 0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/',
            b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j', b'e', b'c', b't', 0x07, 0x00, 0x03,
            0x01, 0x00, 0x04, b'm', b'a', b'i', b'n', 0x01, 0x00, 0x03, b'(', b')', b'I', 0x01,
            0x00, 0x04, b'C', b'o', b'd', b'e', 0x00, 0x21, 0x00, 0x02, 0x00, 0x04, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x01, 0x00, 0x09, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01, 0x00, 0x07,
            0x00, 0x00, 0x00, 0x14, 0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x08, 0x10, 0x0a,
            0x10, 0x14, 0x60, 0x05, 0x68, 0xac, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]
    }

    fn standard_main_class_bytes() -> Vec<u8> {
        vec![
            0xca, 0xfe, 0xba, 0xbe, 0x00, 0x00, 0x00, 0x41, 0x00, 0x08, 0x01, 0x00, 0x04, b'M',
            b'a', b'i', b'n', 0x07, 0x00, 0x01, 0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/',
            b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j', b'e', b'c', b't', 0x07, 0x00, 0x03,
            0x01, 0x00, 0x04, b'm', b'a', b'i', b'n', 0x01, 0x00, 0x16, b'(', b'[', b'L', b'j',
            b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'S', b't', b'r', b'i', b'n',
            b'g', b';', b')', b'V', 0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', 0x00, 0x21, 0x00,
            0x02, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x09, 0x00, 0x05, 0x00,
            0x06, 0x00, 0x01, 0x00, 0x07, 0x00, 0x00, 0x00, 0x16, 0x00, 0x02, 0x00, 0x01, 0x00,
            0x00, 0x00, 0x0a, 0x2a, 0xbe, 0x05, 0x9f, 0x00, 0x06, 0x04, 0x03, 0x6c, 0xb1, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00,
        ]
    }
}
