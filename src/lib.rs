pub mod lexer;
pub mod parser;
pub mod semantic;
pub mod ir;
pub mod backend;
pub mod runtime;

pub mod error;
pub mod utils;

use std::io::{BufRead, Write};
use std::path::Path;

use error::EzError;
use runtime::interpreter::Interpreter;
use semantic::type_checker::{Checker, Program};

/// Parses and type-checks the file at `path` and every module it imports.
pub fn check_file(path: &Path) -> Result<Program, EzError> {
    Checker::new().check_file(path)
}

/// Checks and runs the file at `path`, using the process' standard input and output. `args`
/// are the program's command-line arguments (`system.args()`).
pub fn run_file(path: &Path, args: Vec<String>) -> Result<(), EzError> {
    let program = check_file(path)?;
    let stdin = std::io::stdin();
    let mut interpreter = Interpreter::new(
        program,
        Box::new(std::io::stdout()),
        Box::new(stdin.lock()),
    );
    interpreter.args = args;
    interpreter.run()
}

/// Checks and runs `source` as if it were the file `path` (imports are resolved relative to it).
pub fn run_source(
    source: &str,
    path: &Path,
    out: Box<dyn Write>,
    input: Box<dyn BufRead>,
) -> Result<(), EzError> {
    let program = Checker::new().check_source(path, source)?;
    Interpreter::new(program, out, input).run()
}

/// What `kojic build` produces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Emit {
    /// An executable.
    Executable,
    /// The generated C file.
    C,
    /// The IR, as text.
    Ir,
}

/// The libraries the runtime (a Rust static library) needs from the system.
#[cfg(target_os = "macos")]
const SYSTEM_LIBRARIES: &[&str] = &["-lSystem", "-lc", "-lm"];
#[cfg(not(target_os = "macos"))]
const SYSTEM_LIBRARIES: &[&str] = &["-lpthread", "-ldl", "-lm", "-lrt", "-lutil"];

/// Compiles the file at `path` to `output`: checks it, lowers it to the IR, generates C and
/// links it with the runtime using the system's C compiler (`cc`, or the `KOJI_CC` variable).
pub fn build_file(path: &Path, output: &Path, emit: Emit) -> Result<(), EzError> {
    let program = check_file(path)?;
    let ir = ir::lower_program(&program)?;
    let write = |text: String| {
        std::fs::write(output, text).map_err(|e| {
            EzError::CompileError(format!("cannot write '{}': {}", output.display(), e))
        })
    };
    match emit {
        Emit::Ir => return write(ir.to_string()),
        Emit::C => return write(backend::c::generate(&ir)),
        Emit::Executable => {}
    }

    let source = backend::c::generate(&ir);
    let directory = std::env::temp_dir().join(format!(
        "kojic-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let io_error = |e: std::io::Error| EzError::CompileError(format!("cannot write the C file: {}", e));
    std::fs::create_dir_all(&directory).map_err(io_error)?;
    let c_file = directory.join("main.c");
    std::fs::write(&c_file, source).map_err(io_error)?;

    let compiler = std::env::var("KOJI_CC").unwrap_or_else(|_| "cc".to_string());
    let runtime = std::env::var("KOJI_RUNTIME").unwrap_or_else(|_| env!("KOJI_RUNTIME_LIB").to_string());
    let result = std::process::Command::new(&compiler)
        .arg("-O2")
        .arg("-o")
        .arg(output)
        .arg(&c_file)
        .arg(&runtime)
        .args(SYSTEM_LIBRARIES)
        .output();
    let _ = std::fs::remove_dir_all(&directory);
    match result {
        Ok(result) if result.status.success() => Ok(()),
        Ok(result) => Err(EzError::CompileError(format!(
            "the C compiler '{}' failed:\n{}",
            compiler,
            String::from_utf8_lossy(&result.stderr).trim_end()
        ))),
        Err(e) => Err(EzError::CompileError(format!(
            "cannot run the C compiler '{}': {} (set KOJI_CC to choose another one)",
            compiler, e
        ))),
    }
}
