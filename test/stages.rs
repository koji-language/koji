//! What each stage of the compiler gives (`kojic dump tokens|ast|types|ir`) for the cases of the
//! tests, against the snapshots in `test/snapshots/`. A change of a stage shows as a change of its
//! snapshot: run the tests with `KOJI_UPDATE_SNAPSHOTS=1` to write them again, and read the diff.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[path = "koji/mod.rs"]
mod koji;

/// A program of the tests: its file, and the directory `kojic` runs in (the messages name the file
/// as it is given, relative to that directory).
#[derive(Clone)]
struct Case {
    name: String,
    directory: PathBuf,
    file: String,
}

/// The `.kj` files of `directory`, in order, run from the root of the repository.
fn files(directory: &str) -> Vec<Case> {
    let mut files: Vec<String> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "kj"))
        .map(|path| path.to_str().unwrap().to_string())
        .collect();
    files.sort();
    files.into_iter().map(|file| Case { name: file.clone(), directory: PathBuf::from("."), file }).collect()
}

/// `sources` as files of a directory of their own, `name` followed by their number.
fn written(name: &str, sources: Vec<String>) -> Vec<Case> {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("stages").join(name.replace('/', "-"));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    sources
        .into_iter()
        .enumerate()
        .map(|(index, source)| {
            let file = format!("case{}.kj", index);
            std::fs::write(directory.join(&file), format!("{}\n", source.trim_end_matches('\n'))).unwrap();
            Case { name: format!("{} #{}", name, index), directory: directory.clone(), file }
        })
        .collect()
}

/// The programs of a file of cases (after its header, separated by lines `----`).
fn cases(path: &str) -> Vec<Case> {
    // Written once: the tests run at the same time, and one would read a file another rewrites.
    static WRITTEN: OnceLock<std::sync::Mutex<std::collections::HashMap<String, Vec<Case>>>> = OnceLock::new();
    let mut written_cases = WRITTEN.get_or_init(Default::default).lock().unwrap();
    written_cases
        .entry(path.to_string())
        .or_insert_with(|| {
            let text = std::fs::read_to_string(path).unwrap();
            let (_, cases) = text.split_once("----\n").expect("the cases start after a line '----'");
            written(path, cases.split("\n----\n").map(str::to_string).collect())
        })
        .clone()
}

/// Syntax errors (and a few tricky constructs) for the parser: each one is a file.
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

fn parser_cases() -> Vec<Case> {
    static CASES: OnceLock<Vec<Case>> = OnceLock::new();
    CASES.get_or_init(|| written("parser cases", PARSER_CASES.iter().map(|case| case.to_string()).collect())).clone()
}

/// Checks the snapshot `test/snapshots/<stage>.txt`: what `kojic dump <stage>` prints for each
/// case, after a line `==== <case>`.
fn check_snapshot(stage: &str, cases: Vec<Case>) {
    let mut sections = Vec::new();
    for case in cases {
        let compiler = std::fs::canonicalize(koji::compiler()).unwrap();
        let finished = koji::finish(std::process::Command::new(compiler).args(["dump", stage, &case.file]).current_dir(&case.directory), "");
        assert_eq!(finished.code, Some(0), "kojic dump {} {} failed: {}", stage, case.name, finished.stderr);
        sections.push((case.name, finished.stdout));
    }
    let text: String = sections.iter().map(|(name, output)| format!("==== {}\n{}", name, output)).collect();
    let path = format!("test/snapshots/{}.txt", stage);
    if std::env::var("KOJI_UPDATE_SNAPSHOTS").is_ok() {
        std::fs::create_dir_all("test/snapshots").unwrap();
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_default();
    if text == expected {
        return;
    }
    // The first case that changed.
    let before: Vec<&str> = expected.split("==== ").skip(1).collect();
    for (index, (name, output)) in sections.iter().enumerate() {
        let found = format!("{}\n{}", name, output);
        if before.get(index) != Some(&found.as_str()) {
            panic!(
                "{} differs from the snapshot {} (KOJI_UPDATE_SNAPSHOTS=1 writes it again).\nexpected:\n{}\nfound:\n{}",
                name,
                path,
                before.get(index).unwrap_or(&""),
                found
            );
        }
    }
    panic!("{} has cases the tests do not have (KOJI_UPDATE_SNAPSHOTS=1 writes it again)", path);
}

#[test]
fn tokens() {
    check_snapshot("tokens", [files("examples"), files("test/lexer")].concat());
}

#[test]
fn syntax_trees() {
    check_snapshot("ast", [files("examples"), files("test/lexer"), parser_cases()].concat());
}

#[test]
fn types() {
    check_snapshot("types", [files("examples"), parser_cases(), cases("test/checker/cases.txt"), files("test/checker")].concat());
}

#[test]
fn intermediate_representation() {
    check_snapshot("ir", [files("examples"), files("test/checker"), cases("test/ir/cases.txt")].concat());
}

/// `compiler/prelude.kj` as it must be: the lines of `runtime/prelude.h`, the declarations of the
/// runtime that start every C file the compiler generates.
fn prelude_in_koji() -> String {
    let prelude = std::fs::read_to_string("runtime/prelude.h").unwrap();
    let mut text = String::from(
        "// The start of every C file the compiler generates: the lines of runtime/prelude.h, the\n\
         // declarations of the runtime. Generated: test/stages.rs checks that it is up to date, and\n\
         // rewrites it when run with KOJI_UPDATE_PRELUDE=1.\n\nexport immut preludeLines = [\n",
    );
    for line in prelude.lines() {
        text.push_str(&format!("    '{}',\n", line.replace('\\', "\\\\").replace('\'', "\\'")));
    }
    text.push_str("]\n");
    text
}

/// `compiler/stdlib.kj` as it must be: the sources of the modules of the standard library written
/// in Koji (`std/*.kj`), which the compiler carries.
fn stdlib_in_koji() -> String {
    let mut names: Vec<String> = std::fs::read_dir("std")
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "kj"))
        .map(|path| path.file_stem().unwrap().to_str().unwrap().to_string())
        .collect();
    names.sort();
    let mut text = String::from(
        "// The modules of the standard library written in Koji (std/), which the compiler carries: `import\n\
         // json` compiles std/json.kj. Generated: test/stages.rs checks that it is up to date, and rewrites\n\
         // it when run with KOJI_UPDATE_STDLIB=1.\n\n",
    );
    let quoted: Vec<String> = names.iter().map(|name| format!("'{}'", name)).collect();
    text.push_str(&format!("export immut kojiModules = [{}]\n\n", quoted.join(", ")));
    text.push_str("// The source of the module `name` of the standard library.\nexport function stdSource(name: string): Option[string] {\n");
    for name in &names {
        let source = std::fs::read_to_string(format!("std/{}.kj", name)).unwrap();
        text.push_str(&format!("    if (name == '{}') {{\n        return Some([\n", name));
        for line in source.lines() {
            text.push_str(&format!("            '{}',\n", line.replace('\\', "\\\\").replace('\'', "\\'")));
        }
        text.push_str("        ].join('\\n'))\n    }\n\n");
    }
    text.push_str("    return None\n}\n");
    text
}

#[test]
fn the_standard_library_is_up_to_date() {
    let expected = stdlib_in_koji();
    if std::env::var("KOJI_UPDATE_STDLIB").is_ok() {
        std::fs::write("compiler/stdlib.kj", &expected).unwrap();
    }
    let found = std::fs::read_to_string("compiler/stdlib.kj").unwrap_or_default();
    assert!(found == expected, "compiler/stdlib.kj is out of date: run the tests with KOJI_UPDATE_STDLIB=1");
}

#[test]
fn the_c_prelude_is_up_to_date() {
    let expected = prelude_in_koji();
    if std::env::var("KOJI_UPDATE_PRELUDE").is_ok() {
        std::fs::write("compiler/prelude.kj", &expected).unwrap();
    }
    let found = std::fs::read_to_string("compiler/prelude.kj").unwrap_or_default();
    assert!(found == expected, "compiler/prelude.kj is out of date: run the tests with KOJI_UPDATE_PRELUDE=1");
}
