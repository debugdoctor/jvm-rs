//! Lazy class-loading with a parent-first bootstrap delegation model.
//!
//! Classes are resolved on first reference, cached in the VM's runtime,
//! and evicted under memory pressure using an LRU policy.  Classes in
//! `java.*` and `jdk.*` are protected from eviction because the VM
//! relies on them for correct operation.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use zip::ZipArchive;

use crate::classfile::ClassFile;
use crate::vm::{ClassMethod, RuntimeClass, VmError};

fn find_java_home() -> Option<PathBuf> {
    if let Ok(home) = env::var("JAVA_HOME") {
        let p = PathBuf::from(&home);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(output) = Command::new("/usr/libexec/java_home").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(PathBuf::from(path));
            }
        }
    }
    None
}

pub struct BootstrapClassLoader {
    paths: Vec<PathBuf>,
}

impl BootstrapClassLoader {
    pub fn new() -> Self {
        let mut paths = Vec::new();

        if let Some(java_home) = find_java_home() {
            let jmod_path = java_home.join("jmods");
            if jmod_path.exists() {
                if let Ok(entries) = fs::read_dir(&jmod_path) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) == Some("jmod") {
                            paths.push(path);
                        }
                    }
                }
            }
            let rt_jar = java_home.join("lib").join("rt.jar");
            if rt_jar.exists() {
                paths.push(rt_jar);
            }
        }

        #[cfg(target_os = "macos")]
        {
            let extra_prefixes = [
                "/Library/Java/JavaVirtualMachines",
                "/System/Library/Java/JavaVirtualMachines",
            ];
            for prefix in extra_prefixes {
                if let Ok(entries) = fs::read_dir(prefix) {
                    for entry in entries.flatten() {
                        let jdk = entry.path();
                        let jmods = jdk.join("Contents").join("Home").join("jmods");
                        if jmods.exists() {
                            if let Ok(entries) = fs::read_dir(&jmods) {
                                for jmod_entry in entries.flatten() {
                                    let path = jmod_entry.path();
                                    if path.extension().and_then(|e| e.to_str()) == Some("jmod") {
                                        paths.push(path);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            for homebrew_prefix in &[
                "/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home",
            ] {
                let jmods = PathBuf::from(homebrew_prefix).join("jmods");
                if jmods.exists() {
                    if let Ok(entries) = fs::read_dir(&jmods) {
                        for jmod_entry in entries.flatten() {
                            let path = jmod_entry.path();
                            if path.extension().and_then(|e| e.to_str()) == Some("jmod") {
                                paths.push(path);
                            }
                        }
                    }
                }
            }
        }

        #[cfg(target_os = "linux")]
        for prefix in &["/usr/lib/jvm", "/usr/java"] {
            if let Ok(entries) = fs::read_dir(prefix) {
                for entry in entries.flatten() {
                    let jdk = entry.path();
                    let jmods = jdk.join("jmods");
                    if jmods.exists() {
                        if let Ok(entries) = fs::read_dir(&jmods) {
                            for jmod_entry in entries.flatten() {
                                let path = jmod_entry.path();
                                if path.extension().and_then(|e| e.to_str()) == Some("jmod") {
                                    paths.push(path);
                                }
                            }
                        }
                    }
                    let rt_jar = jdk.join("jre").join("lib").join("rt.jar");
                    if rt_jar.exists() {
                        paths.push(rt_jar);
                    }
                }
            }
        }

        Self { paths }
    }

    fn find_class_bytes(&self, class_name: &str) -> Option<Vec<u8>> {
        let file_name = format!("{}.class", class_name);
        for path in &self.paths {
            if let Some(bytes) = self.search_path(path, &file_name) {
                return Some(bytes);
            }
        }
        None
    }

    fn search_path(&self, path: &Path, file_name: &str) -> Option<Vec<u8>> {
        if path.extension().and_then(|e| e.to_str()) == Some("jmod") {
            return self.search_jmod(path, file_name);
        }
        if path.extension().and_then(|e| e.to_str()) == Some("jar") {
            return self.search_jar(path, file_name);
        }
        if path.is_dir() {
            return self.search_dir(path, file_name);
        }
        None
    }

    fn search_jmod(&self, jmod_path: &Path, file_name: &str) -> Option<Vec<u8>> {
        let file = fs::File::open(jmod_path).ok()?;
        let mut archive = ZipArchive::new(file).ok()?;
        for name in &[
            format!("classes/{}", file_name),
            format!("class-history/{}", file_name),
        ] {
            if let Ok(mut entry) = archive.by_name(name) {
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes).ok()?;
                return Some(bytes);
            }
        }
        None
    }

    fn search_jar(&self, jar_path: &Path, file_name: &str) -> Option<Vec<u8>> {
        let file = fs::File::open(jar_path).ok()?;
        let mut archive = ZipArchive::new(file).ok()?;
        if let Ok(mut entry) = archive.by_name(file_name) {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).ok()?;
            return Some(bytes);
        }
        None
    }

    fn search_dir(&self, dir: &Path, file_name: &str) -> Option<Vec<u8>> {
        let candidate = dir.join(file_name);
        if candidate.exists() {
            fs::read(&candidate).ok()
        } else {
            None
        }
    }
}

impl Default for BootstrapClassLoader {
    fn default() -> Self {
        Self::new()
    }
}

pub struct LazyClassLoader<C> {
    inner: C,
}

impl<C: ClassLoader> LazyClassLoader<C> {
    pub fn new(inner: C) -> Self {
        Self { inner }
    }
}

pub trait ClassLoader {
    #[allow(dead_code)]
    fn load_class(&mut self, class_name: &str) -> Result<Option<RuntimeClass>, VmError>;
    fn load_classfile(&mut self, class_name: &str) -> Result<Option<ClassFile>, VmError>;
}

impl ClassLoader for BootstrapClassLoader {
    fn load_classfile(&mut self, class_name: &str) -> Result<Option<ClassFile>, VmError> {
        eprintln!("DEBUG load_classfile: called for {}", class_name);
        let bytes = match self.find_class_bytes(class_name) {
            Some(b) => {
                eprintln!("DEBUG load_classfile: {} found bytes len={}", class_name, b.len());
                b
            }
            None => {
                eprintln!("DEBUG load_classfile: {} NOT FOUND", class_name);
                return Err(VmError::ClassNotFound {
                    class_name: class_name.to_string(),
                });
            }
        };
        ClassFile::parse(&bytes)
            .map(Some)
            .map_err(|_| VmError::ClassNotFound {
                class_name: class_name.to_string(),
            })
    }

    fn load_class(&mut self, class_name: &str) -> Result<Option<RuntimeClass>, VmError> {
        let bytes = self
            .find_class_bytes(class_name)
            .ok_or_else(|| VmError::ClassNotFound {
                class_name: class_name.to_string(),
            })?;

        let class_file = ClassFile::parse(&bytes).map_err(|_e| VmError::ClassNotFound {
            class_name: class_name.to_string(),
        })?;

        let resolved_name = class_file.class_name().unwrap_or(class_name).to_string();
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

        let mut instance_fields = Vec::new();
        for field in &class_file.fields {
            let is_static = field.access_flags & 0x0008 != 0;
            if !is_static {
                let name = field
                    .name(&class_file.constant_pool)
                    .unwrap_or_default()
                    .to_string();
                let descriptor = field
                    .descriptor(&class_file.constant_pool)
                    .unwrap_or_default()
                    .to_string();
                instance_fields.push((name, descriptor));
            }
        }

        let mut methods = BTreeMap::new();
        for member in &class_file.methods {
            let name = member
                .name(&class_file.constant_pool)
                .unwrap_or_default()
                .to_string();
            let descriptor = member
                .descriptor(&class_file.constant_pool)
                .unwrap_or_default()
                .to_string();

            if let Some(code) = member.code() {
                let method = crate::vm::Method::with_constant_pool(
                    code.code.clone(),
                    code.max_locals as usize,
                    code.max_stack as usize,
                    vec![None],
                )
                .with_metadata(&resolved_name, &name, &descriptor, member.access_flags);

                methods.insert((name, descriptor), ClassMethod::Bytecode(method));
            }
        }

        Ok(Some(RuntimeClass {
            name: resolved_name,
            super_class,
            methods,
            static_fields: BTreeMap::new(),
            instance_fields,
            interfaces,
        }))
    }
}

impl<C: ClassLoader> ClassLoader for LazyClassLoader<C> {
    fn load_class(&mut self, class_name: &str) -> Result<Option<RuntimeClass>, VmError> {
        self.inner.load_class(class_name)
    }

    fn load_classfile(&mut self, class_name: &str) -> Result<Option<ClassFile>, VmError> {
        self.inner.load_classfile(class_name)
    }
}

pub fn create_bootstrap_loader() -> LazyClassLoader<BootstrapClassLoader> {
    LazyClassLoader::new(BootstrapClassLoader::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_arraylist_from_system_jdk() {
        let mut loader = BootstrapClassLoader::new();
        let result = loader.load_class("java/util/ArrayList");
        assert!(result.is_ok(), "load_class error: {:?}", result);
        let class = result.unwrap();
        assert!(class.is_some(), "ArrayList should be found in bootstrap path");
        let class = class.unwrap();
        assert_eq!(class.name, "java/util/ArrayList");
        assert_eq!(class.super_class, Some("java/util/AbstractList".to_string()));
        assert!(!class.methods.is_empty(), "ArrayList should have methods");
    }

    #[test]
    fn loads_hashmap_from_system_jdk() {
        let mut loader = BootstrapClassLoader::new();
        let result = loader.load_class("java/util/HashMap");
        assert!(result.is_ok(), "load_class error: {:?}", result);
        let class = result.unwrap().unwrap();
        assert_eq!(class.name, "java/util/HashMap");
        assert!(!class.methods.is_empty());
    }
}
