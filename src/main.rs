use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "\
kojic - the Koji programming language

Usage:
    kojic run <file.kj> [arguments]   check and run a program
    kojic check <file.kj>             only check syntax and types
    kojic help                        show this message";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
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
