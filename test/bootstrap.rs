//! Phase 3: the compiler written in Koji (`compiler/`) must agree with the one in Rust. Each
//! stage prints the same text as `kojic dump <stage>`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// The compiler written in Koji, compiled by `kojic build` (once for all the tests).
fn koji_compiler() -> &'static Path {
    static EXECUTABLE: OnceLock<PathBuf> = OnceLock::new();
    EXECUTABLE.get_or_init(|| {
        let executable = Path::new(env!("CARGO_TARGET_TMPDIR")).join("kojic-in-koji");
        kojic::build_file(Path::new("compiler/main.kj"), &executable, kojic::Emit::Executable)
            .unwrap_or_else(|error| panic!("the compiler written in Koji does not compile: {}", error));
        executable
    })
}

fn output(command: &mut Command) -> String {
    let result = command.output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    String::from_utf8(result.stdout).unwrap()
}

/// The files to compare: the examples, the compiler itself, the code blocks of the
/// documentation and the lexer's edge cases.
fn corpus() -> Vec<PathBuf> {
    let mut files = Vec::new();
    for directory in ["examples", "compiler", "test/lexer"] {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|extension| extension == "kj") {
                files.push(path);
            }
        }
    }
    let blocks = Path::new(env!("CARGO_TARGET_TMPDIR")).join("bootstrap-doc-blocks");
    std::fs::create_dir_all(&blocks).unwrap();
    let mut count = 0;
    let mut pending = vec![PathBuf::from("doc")];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "md") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            for block in text.split("```koji\n").skip(1) {
                let code = block.split("```").next().unwrap_or("");
                count += 1;
                let file = blocks.join(format!("block{}.kj", count));
                std::fs::write(&file, code).unwrap();
                files.push(file);
            }
        }
    }
    assert!(count > 100, "too few documentation blocks: {}", count);
    files
}

#[test]
fn the_lexer_in_koji_gives_the_same_tokens() {
    let compiler = koji_compiler();
    for file in corpus() {
        let expected = kojic::dump_tokens(&std::fs::read_to_string(&file).unwrap());
        let tokens = output(Command::new(compiler).arg("tokens").arg(&file));
        assert_eq!(tokens, expected, "the tokens of {} differ", file.display());
    }
}

#[test]
fn the_interpreted_lexer_agrees() {
    // The interpreter runs the compiler written in Koji too (slower: a few files).
    for file in ["examples/shapes.kj", "test/lexer/edge.kj", "compiler/lexer.kj"] {
        let expected = kojic::dump_tokens(&std::fs::read_to_string(file).unwrap());
        let tokens = output(Command::new(env!("CARGO_BIN_EXE_kojic")).args(["run", "compiler/main.kj", "tokens", file]));
        assert_eq!(tokens, expected, "the tokens of {} differ", file);
    }
}
