//! The compiler written in Koji, as the tests use it: built from the seed like
//! `bootstrap/build.sh` does, kept in the target directory while its sources stay the same, and run as
//! a command (`kojic run`, `kojic build`, `kojic check`).

#![allow(dead_code)]

use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

/// The runtime library built from its C (bootstrap/libkoji.c.gz), for the programs of the C backend.
pub const RUNTIME: &str = env!("KOJI_RUNTIME_LIB");

/// The libraries a program needs from the system besides the C library.
const SYSTEM_LIBRARIES: &[&str] = &["-lm"];

/// The directory of the compilers the test binaries share.
fn compilers() -> PathBuf {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("koji-compilers");
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

/// A hash of these texts.
fn hash(texts: &[&[u8]]) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    texts.hash(&mut hasher);
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

/// The platform of the seed that runs here (`bootstrap/seed/kojic-<platform>.gz`).
pub fn platform() -> &'static str {
    if cfg!(all(target_os = "linux", target_arch = "aarch64")) { "linux-arm64" } else { "linux-x64" }
}

/// Stage 1: the seed (`bootstrap/seed/kojic-<platform>.gz`, an executable of an earlier compiler
/// written in Koji), and the runtime library it builds (`kojic library`), which it links the
/// compiler with: no C compiler, as `bootstrap/build.sh` does.
pub fn seed() -> &'static (PathBuf, PathBuf) {
    static SEED: OnceLock<(PathBuf, PathBuf)> = OnceLock::new();
    SEED.get_or_init(|| {
        let packed = format!("bootstrap/seed/kojic-{}.gz", platform());
        let bytes = std::fs::read(&packed).unwrap();
        let name = format!("stage1-{}", hash(&[&bytes]));
        let executable = made(&name, |executable| {
            let unpacked = Command::new("gzip").arg("-dc").arg(&packed).output().unwrap();
            assert!(unpacked.status.success(), "cannot unpack the seed");
            std::fs::write(executable, unpacked.stdout).unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        });
        let library = made(&format!("lib{}.a", name), |archive| {
            let built = finish(Command::new(&executable).arg("library").arg("-o").arg(archive), "");
            assert_eq!(built.code, Some(0), "the seed's library: {}", built.stderr);
        });
        (executable, library)
    })
}

/// The sources of the compiler and of the standard library, read in order.
fn sources() -> Vec<Vec<u8>> {
    fn walk(directory: &Path, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, found);
            } else if path.extension().is_some_and(|extension| extension == "kj") {
                found.push(path);
            }
        }
    }
    let mut paths = Vec::new();
    walk(Path::new("compiler"), &mut paths);
    walk(Path::new("std"), &mut paths);
    paths.sort();
    paths.iter().map(|path| [path.to_string_lossy().as_bytes(), &std::fs::read(path).unwrap()].concat()).collect()
}

/// The compiler written in Koji (`compiler/`), built by stage 1. Its name has a hash of its sources
/// and of the seed: the test binaries share it, and it is built again only when one of them
/// changes.
pub fn compiler() -> &'static Path {
    static EXECUTABLE: OnceLock<PathBuf> = OnceLock::new();
    EXECUTABLE.get_or_init(|| {
        let (stage1, library) = seed();
        let mut texts = sources();
        texts.push(std::fs::read(stage1).unwrap());
        let parts: Vec<&[u8]> = texts.iter().map(|text| text.as_slice()).collect();
        made(&format!("kojic-{}", hash(&parts)), |executable| {
            let built = finish(Command::new(stage1).env("KOJI_RUNTIME", library).args(["build", "compiler/main.kj", "-o"]).arg(executable), "");
            assert_eq!(built.code, Some(0), "the compiler written in Koji does not compile: {}", built.stderr);
        })
    })
}

/// The runtime library built natively by the compiler (`kojic library`), which the programs of the
/// tests link with.
pub fn library() -> &'static Path {
    static LIBRARY: OnceLock<PathBuf> = OnceLock::new();
    LIBRARY.get_or_init(|| {
        let compiler = compiler();
        let name = format!("libkoji-{}.a", compiler.file_name().unwrap().to_string_lossy());
        made(&name, |archive| {
            let built = finish(Command::new(compiler).arg("library").arg("-o").arg(archive), "");
            assert_eq!(built.code, Some(0), "kojic library: {}", built.stderr);
        })
    })
}

/// A command of the compiler written in Koji, with the runtime library.
pub fn kojic() -> Command {
    let mut command = Command::new(compiler());
    command.env("KOJI_RUNTIME", library());
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
