//! Shared test utilities for jvm-rs integration tests.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use jvm_rs::launcher::LaunchOptions;
use jvm_rs::vm::ExecutionResult;

pub fn temp_dir(test_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("jvm-rs-{test_name}-{nanos}"));
    fs::create_dir_all(&path).unwrap();
    path
}

pub fn compile_and_run_with_javac_args(
    test_name: &str,
    javac_args: &[&str],
    files: &[(&str, &str)],
) -> (ExecutionResult, Vec<String>) {
    let root = temp_dir(test_name);
    for (name, source) in files {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, source).unwrap();
    }

    let source_files: Vec<PathBuf> = files.iter().map(|(name, _)| root.join(name)).collect();
    let mut cmd = Command::new("javac");
    cmd.args(javac_args).arg("-d").arg(&root);
    for source in &source_files {
        cmd.arg(source);
    }
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "javac failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let main_file = files
        .iter()
        .find(|(_, source)| source.contains("public static void main(String[] args)"))
        .or_else(|| files.iter().find(|(name, _)| name.ends_with("/Main.java")))
        .map(|(name, _)| *name)
        .unwrap_or(files[0].0);
    let main_class = main_file.trim_end_matches(".java").replace('/', ".");

    let options = LaunchOptions::new(&root, &main_class, vec![]);
    let mut vm = jvm_rs::vm::Vm::new().expect("failed to create VM");
    vm.set_class_path(options.class_path.clone());
    let source = jvm_rs::launcher::resolve_class_path(&options.class_path, &main_class).unwrap();
    let method = jvm_rs::launcher::load_main_method(&source, &main_class, &[], &mut vm).unwrap();
    let result = vm.execute(method).unwrap();
    let output = vm.take_output();
    (result, output)
}

pub fn compile_and_run(
    test_name: &str,
    files: &[(&str, &str)],
) -> (ExecutionResult, Vec<String>) {
    compile_and_run_with_javac_args(test_name, &["--release", "8"], files)
}
