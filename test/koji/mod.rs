//! The compiler written in Koji, as the tests use it: built from the seed like
//! `bootstrap/build.sh` does, kept in the target directory while its C stays the same, and run as
//! a command (`kojic run`, `kojic build`, `kojic check`).

#![allow(dead_code)]

use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

/// The runtime library every program is linked with.
pub const RUNTIME: &str = env!("KOJI_RUNTIME_LIB");

/// The libraries a program needs from the system besides the C library.
const SYSTEM_LIBRARIES: &[&str] = &["-lm"];

/// The directory of the compilers the test binaries share.
fn compilers() -> PathBuf {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("koji-compilers");
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

/// A hash of these texts and of the runtime.
fn hash(texts: &[&[u8]]) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    texts.hash(&mut hasher);
    std::fs::read(RUNTIME).unwrap().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// The executable `name` in the directory of the compilers, made by `make` (given the path to
/// write) when it is not there yet. It is made under a name of its own and then renamed: another
/// test binary never sees half a compiler.
fn made(name: &str, make: impl FnOnce(&Path)) -> PathBuf {
    let executable = compilers().join(name);
    if !executable.exists() {
        let making = compilers().join(format!("making-{}-{}", std::process::id(), name));
        make(&making);
        std::fs::rename(&making, &executable).unwrap();
    }
    executable
}

/// Stage 1: the seed (`bootstrap/kojic.c.gz`, the C of an earlier compiler written in Koji),
/// compiled without optimizations, which is faster, since it only builds the compiler.
pub fn seed() -> &'static Path {
    static EXECUTABLE: OnceLock<PathBuf> = OnceLock::new();
    EXECUTABLE.get_or_init(|| {
        let seed = std::fs::read("bootstrap/kojic.c.gz").unwrap();
        made(&format!("stage1-{}", hash(&[&seed])), |executable| {
            let c = executable.with_extension("c");
            let unpacked = Command::new("gzip").args(["-dc", "bootstrap/kojic.c.gz"]).output().unwrap();
            assert!(unpacked.status.success(), "cannot unpack the seed");
            std::fs::write(&c, unpacked.stdout).unwrap();
            let built = Command::new("cc").arg("-O0").arg("-o").arg(executable).arg(&c).arg(RUNTIME).args(SYSTEM_LIBRARIES).output().unwrap();
            assert!(built.status.success(), "the seed does not compile: {}", String::from_utf8_lossy(&built.stderr));
            std::fs::remove_file(&c).unwrap();
        })
    })
}

/// The compiler written in Koji (`compiler/`), built by stage 1. Its name has a hash of its C and
/// of the runtime: the test binaries share it, and it is built again only when one of them changes.
pub fn compiler() -> &'static Path {
    static EXECUTABLE: OnceLock<PathBuf> = OnceLock::new();
    EXECUTABLE.get_or_init(|| {
        let stage1 = seed();
        let c = Command::new(stage1).args(["dump", "c", "compiler/main.kj"]).output().unwrap();
        assert!(c.status.success(), "{}", String::from_utf8_lossy(&c.stderr));
        made(&format!("kojic-{}", hash(&[&c.stdout])), |executable| {
            let built = finish(Command::new(stage1).env("KOJI_RUNTIME", RUNTIME).args(["build", "compiler/main.kj", "-o"]).arg(executable), "");
            assert_eq!(built.code, Some(0), "the compiler written in Koji does not compile: {}", built.stderr);
        })
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
