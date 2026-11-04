//! Phase 3: the compiler written in Koji (`compiler/`) must agree with the one in Rust. Each
//! stage prints the same text as `kojic dump <stage>`.

use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "koji/mod.rs"]
mod koji;

/// The compiler written in Koji, compiled by `kojic build` (once for all the tests).
fn koji_compiler() -> &'static Path {
    koji::compiler()
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
        let tokens = output(Command::new(compiler).args(["dump", "tokens"]).arg(&file));
        assert_eq!(tokens, expected, "the tokens of {} differ", file.display());
    }
}

#[test]
fn the_interpreted_lexer_agrees() {
    // The interpreter runs the compiler written in Koji too (slower: a few files).
    for file in ["examples/shapes.kj", "test/lexer/edge.kj", "compiler/lexer.kj"] {
        let expected = kojic::dump_tokens(&std::fs::read_to_string(file).unwrap());
        let tokens = output(Command::new(env!("CARGO_BIN_EXE_kojic")).args(["run", "compiler/main.kj", "dump", "tokens", file]));
        assert_eq!(tokens, expected, "the tokens of {} differ", file);
    }
}

/// Syntax errors (and a few tricky constructs) for comparing the parsers: each one is a file.
const PARSER_CASES: &[&str] = &[
    r#"immut x = 1 @ 2"#,
    r#"break"#,
    r#"while (true) { continue }
continue"#,
    r#"return 1"#,
    r#"interface A {}"#,
    r#"function f() { export immut x = 1 }"#,
    r#"var x = 1"#,
    r#"const x = 1"#,
    r#"let x = 1"#,
    r#"x: int = 1"#,
    r#"1 + 2 = 3"#,
    r#"f() += 1"#,
    r#"mut x int = 1"#,
    r#"mut interface = 1"#,
    r#"immut x: Map[string = 1"#,
    r#"immut f: (int, string) = 1"#,
    r#"immut f: (int) => = 1"#,
    r#"immut x: int[ = 1"#,
    r#"function f( { }"#,
    r#"function f(a: int { }"#,
    r#"function f(mut) {}"#,
    r#"function f() { print(1)"#,
    r#"if x > 1 { }"#,
    r#"while not x { }"#,
    r#"if (x) { } else if (y) { }"#,
    r#"for in items { }"#,
    r#"for x items { }"#,
    r#"export print(1)"#,
    r#"import { a, b from 'x.kj'"#,
    r#"import { a } 'x.kj'"#,
    r#"import { a } from x"#,
    r#"import math from 1"#,
    r#"type A = { a: int, a: string }"#,
    r#"type A = { B C B }"#,
    r#"type A = { a: int"#,
    r#"type A = { B(x: int) C"#,
    r#"type A = int | string"#,
    r#"type A[] = int"#,
    r#"type A[T, T] = int"#,
    r#"type A[T: 1] = int"#,
    r#"type A = { [k: string] int }"#,
    r#"type A = { [k: string]: int x }"#,
    r#"type A = { B(x int) }"#,
    r#"type A int"#,
    r#"immut x = match (y) { }"#,
    r#"immut x = match y { _ => 1 }"#,
    r#"immut x = match (y) _ => 1"#,
    r#"immut x = match (y) { 1 => 2"#,
    r#"immut x = match (y) { 1 if x => 2 }"#,
    r#"immut x = match (y) { 1 if (x => 2 }"#,
    r#"immut x = match (y) { 1 2 }"#,
    r#"immut x = match (y) { -a => 2 }"#,
    r#"immut x = match (y) { [1] => 2 }"#,
    r#"immut x = match (y) { A(1, => 2 }"#,
    r#"immut x = match (y) { 1e400 => 2 }"#,
    r#"immut x = 1e400"#,
    r#"immut x = { a: 1, a: 2 }"#,
    r#"immut x = { a 1 }"#,
    r#"immut x = { a: 1"#,
    r#"immut x = { 1: 2 }"#,
    r#"immut x = [1, 2: 3]"#,
    r#"immut x = [:"#,
    r#"immut x = [1, 2"#,
    r#"immut x = a[1"#,
    r#"immut x = a[1..2"#,
    r#"immut x = f(1, 2"#,
    r#"immut x = (1 + 2"#,
    r#"immut x = a."#,
    r#"immut x = a.1"#,
    r#"immut x = `a ${b`"#,
    r#"immut x = `a ${}`"#,
    r#"immut x = `a ${1 2}`"#,
    r#"immut x = `a ${(1}`"#,
    r#"immut x = `a ${`b ${c}`}`"#,
    r#"immut x = `${a} \n \t \0 \` ${b}${c}`"#,
    r#"immut f = (x: int) x"#,
    r#"immut f = (x: int): => x"#,
    r#"immut x = )"#,
    r#"immut x = "#,
    r#"print('a' 'b')"#,
    r#"function A.() {}"#,
    r#"function A.type(self) { return }"#,
    r#"function f[]() {}"#,
    r#"function f[T](x: T): T { return x }
immut y = 1 ** 2 ** 3 - -4 * ~5 and not x or !y"#,
    r#"immut x = a
(1)
immut y = b
[2]"#,
    r#"immut x = match (y) {
 0 => 1
 -1 => 2
}"#,
    r#""unterminated"#,
    r#"immut x = 'a' # comment?"#,
    r#"for mut x in [1] { x += 1 }"#,
    r#"immut x = 9223372036854775808"#,
    r#"immut x = match (y) { -9223372036854775808 => 1 }"#,
    r#"immut x = match (y) { -1e400 => 2 }"#,
];

/// The cases above as files (in a directory of their own).
fn parser_cases() -> Vec<PathBuf> {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("bootstrap-parser-cases");
    std::fs::create_dir_all(&directory).unwrap();
    PARSER_CASES
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let file = directory.join(format!("case{}.kj", index));
            std::fs::write(&file, format!("{}\n", source)).unwrap();
            file
        })
        .collect()
}

#[test]
fn the_parser_in_koji_gives_the_same_tree() {
    let compiler = koji_compiler();
    for file in corpus().into_iter().chain(parser_cases()) {
        let expected = kojic::dump_ast(&std::fs::read_to_string(&file).unwrap());
        let tree = output(Command::new(compiler).args(["dump", "ast"]).arg(&file));
        assert_eq!(tree, expected, "the syntax tree of {} differs", file.display());
    }
}

#[test]
fn the_interpreted_parser_agrees() {
    for file in ["examples/shapes.kj", "compiler/lexer.kj"] {
        let expected = kojic::dump_ast(&std::fs::read_to_string(file).unwrap());
        let tree = output(Command::new(env!("CARGO_BIN_EXE_kojic")).args(["run", "compiler/main.kj", "dump", "ast", file]));
        assert_eq!(tree, expected, "the syntax tree of {} differs", file);
    }
}

/// The programs of `test/checker/cases.txt` (after its header, separated by lines `----`), as
/// files in a directory of their own.
fn checker_cases() -> Vec<PathBuf> {
    let files = cases("test/checker/cases.txt", "bootstrap-checker-cases");
    assert!(files.len() > 200, "too few checker cases: {}", files.len());
    files
}

/// The programs of a file of cases (after its header, separated by lines `----`), as files in
/// the directory `name`.
fn cases(path: &str, name: &str) -> Vec<PathBuf> {
    let text = std::fs::read_to_string(path).unwrap();
    let (_, cases) = text.split_once("----\n").expect("the cases start after a line '----'");
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(&directory).unwrap();
    cases
        .split("\n----\n")
        .enumerate()
        .map(|(index, source)| {
            let file = directory.join(format!("case{}.kj", index));
            std::fs::write(&file, format!("{}\n", source.trim_end_matches('\n'))).unwrap();
            file
        })
        .collect()
}

/// The modules of `test/checker/` (imports of every kind, and their errors).
fn checker_modules() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir("test/checker")
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "kj"))
        .collect();
    files.sort();
    files
}

#[test]
fn the_checker_in_koji_finds_the_same() {
    let compiler = koji_compiler();
    let files = corpus().into_iter().chain(parser_cases()).chain(checker_cases()).chain(checker_modules());
    for file in files {
        let expected = kojic::dump_types(&file);
        let found = output(Command::new(compiler).args(["dump", "types"]).arg(&file));
        assert_eq!(found, expected, "what the checkers find in {} differs", file.display());
    }
}

#[test]
fn the_interpreted_checker_agrees() {
    for file in ["examples/shapes.kj", "test/checker/modules.kj", "test/checker/wrap.kj"] {
        let expected = kojic::dump_types(Path::new(file));
        let found = output(Command::new(env!("CARGO_BIN_EXE_kojic")).args(["run", "compiler/main.kj", "dump", "types", file]));
        assert_eq!(found, expected, "what the checkers find in {} differs", file);
    }
}

#[test]
fn the_lowering_in_koji_gives_the_same_ir() {
    let compiler = koji_compiler();
    let files = corpus()
        .into_iter()
        .chain(parser_cases())
        .chain(checker_cases())
        .chain(checker_modules())
        .chain(cases("test/ir/cases.txt", "bootstrap-ir-cases"));
    for file in files {
        let expected = kojic::dump_ir(&file);
        let found = output(Command::new(compiler).args(["dump", "ir"]).arg(&file));
        assert_eq!(found, expected, "the IR of {} differs", file.display());
    }
}

#[test]
fn the_interpreted_lowering_agrees() {
    for file in ["examples/shapes.kj", "examples/modules.kj"] {
        let expected = kojic::dump_ir(Path::new(file));
        let found = output(Command::new(env!("CARGO_BIN_EXE_kojic")).args(["run", "compiler/main.kj", "dump", "ir", file]));
        assert_eq!(found, expected, "the IR of {} differs", file);
    }
}

/// `compiler/prelude.kj` as it must be: the lines of `src/backend/c/prelude.h`, which the C
/// generator in Rust includes with `include_str!`.
fn prelude_in_koji() -> String {
    let prelude = std::fs::read_to_string("src/backend/c/prelude.h").unwrap();
    let mut text = String::from(
        "// The start of every C file the compiler generates: the lines of src/backend/c/prelude.h,\n\
         // which the C generator in Rust includes. Generated: test/bootstrap.rs checks that it is up\n\
         // to date, and rewrites it when run with KOJI_UPDATE_PRELUDE=1.\n\nexport immut preludeLines = [\n",
    );
    for line in prelude.lines() {
        text.push_str(&format!("    '{}',\n", line.replace('\\', "\\\\").replace('\'', "\\'")));
    }
    text.push_str("]\n");
    text
}

#[test]
fn the_c_prelude_in_koji_is_up_to_date() {
    let expected = prelude_in_koji();
    if std::env::var("KOJI_UPDATE_PRELUDE").is_ok() {
        std::fs::write("compiler/prelude.kj", &expected).unwrap();
    }
    let found = std::fs::read_to_string("compiler/prelude.kj").unwrap_or_default();
    assert!(found == expected, "compiler/prelude.kj is out of date: run the tests with KOJI_UPDATE_PRELUDE=1");
}

#[test]
fn the_c_generator_in_koji_gives_the_same_c() {
    let compiler = koji_compiler();
    let files = corpus()
        .into_iter()
        .chain(parser_cases())
        .chain(checker_cases())
        .chain(checker_modules())
        .chain(cases("test/ir/cases.txt", "bootstrap-c-cases"));
    for file in files {
        let expected = kojic::dump_c(&file);
        let found = output(Command::new(compiler).args(["dump", "c"]).arg(&file));
        assert!(found == expected, "the C of {} differs", file.display());
    }
}

#[test]
fn the_interpreted_c_generator_agrees() {
    let expected = kojic::dump_c(Path::new("examples/shapes.kj"));
    let found = output(Command::new(env!("CARGO_BIN_EXE_kojic")).args(["run", "compiler/main.kj", "dump", "c", "examples/shapes.kj"]));
    assert!(found == expected, "the C of examples/shapes.kj differs");
}

/// The output of a program run by the interpreter of the kojic written in Rust.
fn interpreted(file: &str) -> String {
    output(Command::new(env!("CARGO_BIN_EXE_kojic")).args(["run", file]))
}

#[test]
fn the_compiler_in_koji_builds_programs() {
    let compiler = koji_compiler();
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("bootstrap-builds");
    std::fs::create_dir_all(&directory).unwrap();
    for file in ["examples/shapes.kj", "examples/modules.kj", "examples/functions.kj", "test/checker/modules.kj"] {
        let executable = directory.join(Path::new(file).file_stem().unwrap());
        let built = Command::new(compiler)
            .args(["build", file, "-o"])
            .arg(&executable)
            .env("KOJI_RUNTIME", env!("KOJI_RUNTIME_LIB"))
            .output()
            .unwrap();
        match kojic::build_file(Path::new(file), &directory.join("rust-built"), kojic::Emit::Executable) {
            // Built by both: the programs give the same output as the interpreter.
            Ok(()) => {
                assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
                assert_eq!(output(&mut Command::new(&executable)), interpreted(file), "{} runs differently", file);
            }
            // Refused by both, with the same message.
            Err(error) => {
                assert_eq!(built.status.code(), Some(1));
                assert_eq!(String::from_utf8_lossy(&built.stderr).trim_end(), error.to_string());
            }
        }
    }
    // Errors go to the standard error output, with the code 1 (2 for a wrong command line).
    let checked = Command::new(compiler).args(["check", "test/checker/private.kj"]).output().unwrap();
    assert_eq!(checked.status.code(), Some(1));
    let expected = kojic::check_file(Path::new("test/checker/private.kj")).unwrap_err().to_string();
    assert_eq!(String::from_utf8_lossy(&checked.stderr).trim_end(), expected);
    assert!(checked.stdout.is_empty());
    assert_eq!(Command::new(compiler).arg("nonsense").output().unwrap().status.code(), Some(2));
    assert!(Command::new(compiler).args(["check", "examples/shapes.kj"]).status().unwrap().success());
}

/// Runs `command` with `input` as its standard input; gives its standard output, standard error
/// output and exit code.
fn run_with_input(command: &mut Command, input: &str) -> (String, String, Option<i32>) {
    use std::io::Write;
    let mut child = command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    // A program that does not read its input may end before it is written.
    let _ = child.stdin.take().unwrap().write_all(input.as_bytes());
    let result = child.wait_with_output().unwrap();
    (
        String::from_utf8_lossy(&result.stdout).to_string(),
        String::from_utf8_lossy(&result.stderr).to_string(),
        result.status.code(),
    )
}

#[test]
fn the_compiler_in_koji_runs_programs() {
    // `kojic run` compiles the program and runs it: the same outputs and exit code as the
    // interpreter of the kojic written in Rust, with the arguments and the input of kojic.
    let compiler = koji_compiler();
    let mut files: Vec<PathBuf> = std::fs::read_dir("examples")
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "kj"))
        .collect();
    files.sort();
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("bootstrap-runs");
    std::fs::create_dir_all(&directory).unwrap();
    let program = directory.join("program.kj");
    std::fs::write(
        &program,
        "import system\nimmut name = input('name? ')\nprint(name, system.args())\nprintError('to errors')\nprint(1 / (system.args().length() - 2))\n",
    )
    .unwrap();
    files.push(program);
    for file in files {
        let koji = run_with_input(
            Command::new(compiler).arg("run").arg(&file).args(["one", "two"]).env("KOJI_RUNTIME", env!("KOJI_RUNTIME_LIB")),
            "Ana\n",
        );
        let rust = run_with_input(Command::new(env!("CARGO_BIN_EXE_kojic")).arg("run").arg(&file).args(["one", "two"]), "Ana\n");
        assert_eq!(koji, rust, "{} runs differently", file.display());
    }
}

/// Phase 4: the compiler written in Koji compiles itself. Stage 1 is the compiler in Koji built by
/// the kojic written in Rust; stage 1 builds stage 2 from the same source, and the C that stage 2
/// generates for the compiler must be the same as stage 1's. Slow (the C compiler builds the
/// compiler once more): run it with `cargo test --test bootstrap -- --ignored`.
#[test]
#[ignore]
fn the_compiler_in_koji_compiles_itself() {
    let stage1 = koji_compiler();
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("bootstrap-stages");
    std::fs::create_dir_all(&directory).unwrap();
    let c_of = |compiler: &Path| output(Command::new(compiler).args(["dump", "c", "compiler/main.kj"]));
    let stage1_c = c_of(stage1);
    assert!(stage1_c == kojic::dump_c(Path::new("compiler/main.kj")), "stage 1 generates another C than kojic");
    let stage2 = directory.join("stage2");
    let built = Command::new(stage1)
        .args(["build", "compiler/main.kj", "-o"])
        .arg(&stage2)
        .env("KOJI_RUNTIME", env!("KOJI_RUNTIME_LIB"))
        .output()
        .unwrap();
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    assert!(c_of(&stage2) == stage1_c, "stage 2 generates another C than stage 1");
}
