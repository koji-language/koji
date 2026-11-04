//! The compiler written in Koji, as the tests use it: built once and kept in the target directory
//! while its C stays the same, then run as a command (`kojic run`, `kojic build`, `kojic check`).

#![allow(dead_code)]

use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

/// The runtime library every program is linked with.
pub const RUNTIME: &str = env!("KOJI_RUNTIME_LIB");

/// The executable of the compiler written in Koji. Its name has a hash of its C and of the
/// runtime: the test binaries share it, and it is built again only when one of them changes.
pub fn compiler() -> &'static Path {
    static EXECUTABLE: OnceLock<PathBuf> = OnceLock::new();
    EXECUTABLE.get_or_init(|| {
        let main = Path::new("compiler/main.kj");
        let c = kojic::dump_c(main);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        c.hash(&mut hasher);
        std::fs::read(RUNTIME).unwrap().hash(&mut hasher);
        let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("koji-compilers");
        std::fs::create_dir_all(&directory).unwrap();
        let executable = directory.join(format!("kojic-{:016x}", hasher.finish()));
        if !executable.exists() {
            // Built under a name of its own, then renamed: another test binary never sees half a
            // compiler.
            let building = directory.join(format!("building-{}", std::process::id()));
            kojic::build_file(main, &building, kojic::Emit::Executable)
                .unwrap_or_else(|error| panic!("the compiler written in Koji does not compile: {}", error));
            std::fs::rename(&building, &executable).unwrap();
        }
        executable
    })
}

/// A command of the compiler written in Koji, with the runtime library.
pub fn kojic() -> Command {
    let mut command = Command::new(compiler());
    command.env("KOJI_RUNTIME", RUNTIME);
    command
}

/// A new empty directory for one run.
pub fn scratch(name: &str) -> PathBuf {
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("runs")
        .join(format!("{}-{}-{}", name, std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

/// What a command did: its standard output, its standard error output and its exit code (`None`
/// when a signal stopped it).
#[derive(Debug, PartialEq)]
pub struct Finished {
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
}

/// Runs `command` with `input` as its standard input.
pub fn finish(command: &mut Command, input: &str) -> Finished {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // A program that does not read its input may end before it is written.
    let _ = child.stdin.take().unwrap().write_all(input.as_bytes());
    let result = child.wait_with_output().unwrap();
    Finished {
        stdout: String::from_utf8_lossy(&result.stdout).to_string(),
        stderr: String::from_utf8_lossy(&result.stderr).to_string(),
        code: result.status.code(),
    }
}

/// `kojic run <file>` in `directory` (the program's current directory, which also holds its
/// temporary executable), with `input` as the program's input.
pub fn run(file: &Path, directory: &Path, input: &str) -> Finished {
    finish(kojic().arg("run").arg(file).current_dir(directory).env("TMPDIR", directory), input)
}
