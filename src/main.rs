use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "\
kojic - the Koji programming language

Usage:
    kojic run <file.kj> [arguments]   check and run a program
    kojic check <file.kj>             only check syntax and types
    kojic build <file.kj> [options]   compile a program to an executable
        -o <output>                   the executable (default: the file name without .kj)
        --emit c                      write the generated C instead
        --emit ir                     write the intermediate representation instead
    kojic dump tokens <file.kj>       show the tokens of a file (for comparing compilers)
    kojic dump ast <file.kj>          show the syntax tree of a file
    kojic dump types <file.kj>        show what the checker finds (types, moves...)
    kojic help                        show this message";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("build") {
        return build(&args[1..]);
    }
    // `kojic dump tokens <file.kj>`: for comparing with the compiler written in Koji.
    if let [dump, stage, file] = args.as_slice() {
        if dump == "dump" && stage == "types" {
            print!("{}", kojic::dump_types(std::path::Path::new(file)));
            return ExitCode::SUCCESS;
        }
        if dump == "dump" && (stage == "tokens" || stage == "ast") {
            return match std::fs::read_to_string(file) {
                Ok(source) => {
                    let text = if stage == "tokens" { kojic::dump_tokens(&source) } else { kojic::dump_ast(&source) };
                    print!("{}", text);
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("cannot read '{}': {}", file, error);
                    ExitCode::FAILURE
                }
            };
        }
    }
    let (command, file, program_args) = match args.as_slice() {
        // Everything after the file is for the program (`system.args()`).
        [command, file, rest @ ..] if command == "run" => (command.as_str(), file, rest.to_vec()),
        [command, file] if command == "check" => (command.as_str(), file, Vec::new()),
        [help] if help == "help" || help == "--help" || help == "-h" => {
            println!("{}", USAGE);
            return ExitCode::SUCCESS;
        }
        [file] if file.ends_with(".kj") => {
            eprintln!("use 'kojic run {}' to run a program\n\n{}", file, USAGE);
            return ExitCode::from(2);
        }
        _ => {
            eprintln!("{}", USAGE);
            return ExitCode::from(2);
        }
    };

    let path = Path::new(file);
    let result = match command {
        "check" => kojic::check_file(path).map(|_| ()),
        _ => kojic::run_file(path, program_args),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(kojic::error::EzError::Exit(code)) => ExitCode::from(code.clamp(0, 255) as u8),
        Err(error) => {
            eprintln!("{}", error);
            ExitCode::FAILURE
        }
    }
}

/// `kojic build <file.kj> [-o <output>] [--emit c|ir]`
fn build(args: &[String]) -> ExitCode {
    let mut file = None;
    let mut output = None;
    let mut emit = kojic::Emit::Executable;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-o" => output = args.next().cloned(),
            "--emit" => {
                emit = match args.next().map(String::as_str) {
                    Some("c") => kojic::Emit::C,
                    Some("ir") => kojic::Emit::Ir,
                    _ => {
                        eprintln!("--emit takes 'c' or 'ir'\n\n{}", USAGE);
                        return ExitCode::from(2);
                    }
                }
            }
            _ if file.is_none() && !arg.starts_with('-') => file = Some(arg.clone()),
            _ => {
                eprintln!("{}", USAGE);
                return ExitCode::from(2);
            }
        }
    }
    let Some(file) = file else {
        eprintln!("{}", USAGE);
        return ExitCode::from(2);
    };
    let path = Path::new(&file);
    let output = output.unwrap_or_else(|| {
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        match emit {
            kojic::Emit::Executable => stem,
            kojic::Emit::C => format!("{}.c", stem),
            kojic::Emit::Ir => format!("{}.ir", stem),
        }
    });
    match kojic::build_file(path, Path::new(&output), emit) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", error);
            ExitCode::FAILURE
        }
    }
}
