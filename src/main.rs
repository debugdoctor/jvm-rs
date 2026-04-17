use std::env;
use std::process;

use jvm_rs::launcher::{LaunchError, launch, parse_launch_options};
use jvm_rs::vm::{ExecutionResult, Value};

fn main() {
    if let Err(message) = run_cli(env::args().skip(1).collect()) {
        eprintln!("{message}");
        process::exit(1);
    }
}

fn run_cli(args: Vec<String>) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("help") | Some("--help") | Some("-h") => {
            print_usage();
            Ok(())
        }
        Some(_) => run_main_class(&args),
        None => {
            print_usage();
            Ok(())
        }
    }
}

fn run_main_class(args: &[String]) -> Result<(), String> {
    let options = parse_launch_options(args).map_err(format_launch_error)?;
    let result = launch(&options).map_err(format_launch_error)?;

    match result {
        ExecutionResult::Void => Ok(()),
        ExecutionResult::Value(Value::Int(value)) => {
            println!("{value}");
            Ok(())
        }
        ExecutionResult::Value(Value::Long(value)) => {
            println!("{value}");
            Ok(())
        }
        ExecutionResult::Value(Value::Float(value)) => {
            println!("{value}");
            Ok(())
        }
        ExecutionResult::Value(Value::Double(value)) => {
            println!("{value}");
            Ok(())
        }
        ExecutionResult::Value(Value::Reference(reference)) => {
            println!("{reference:?}");
            Ok(())
        }
        ExecutionResult::Value(Value::ReturnAddress(_)) => {
            Err("internal error: top-level execution returned a legacy returnAddress".to_string())
        }
    }
}

fn print_usage() {
    println!("{}", usage_text());
}

fn usage_text() -> &'static str {
    "Usage:
  jvm-rs [-cp <path>] <MainClass> [args...]

Options:
  -cp, -classpath <path>    Set the class path root.
  -h, --help                Show this help message.

Example:
  jvm-rs -cp examples demo.Main

Class Path Resolution:
  The launcher currently looks for <class_path>/<MainClass>.class
  using package segments as directories.

Current Execution Support:
  Real .class loading is enabled.
  The runtime currently supports main()I, main()V, and a minimal main([Ljava/lang/String;)V.
  String[] arguments can be passed in, but the VM still only implements a small JVMS 21 subset."
}

fn format_launch_error(error: LaunchError) -> String {
    match error {
        LaunchError::MissingMainClassArgument => {
            format!("{}\n\n{}", error, usage_text())
        }
        LaunchError::MissingClassPathValue => {
            format!("{}\n\n{}", error, usage_text())
        }
        LaunchError::UnsupportedOption(option)
            if option == "help" || option == "-h" || option == "--help" =>
        {
            usage_text().to_string()
        }
        other => other.to_string(),
    }
}
