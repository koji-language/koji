//! Compiles programs with `kojic build` and checks that the executable behaves exactly like the
//! interpreter: same output, same panic message, same exit code.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct Output(Arc<Mutex<Vec<u8>>>);

impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// What a program did: its output, its error (the last line of the standard error) and
/// whether it succeeded.
#[derive(Debug, PartialEq)]
struct Run {
    output: String,
    error: String,
    success: bool,
}

/// The source file of a test, relative to the current directory (so panics show the same
/// file name in both runs).
fn source_file(name: &str, source: &str) -> PathBuf {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("compiler");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("{}.kj", name));
    std::fs::write(&path, source).unwrap();
    let cwd = std::env::current_dir().unwrap();
    path.strip_prefix(&cwd).map(Path::to_path_buf).unwrap_or(path)
}

fn interpret(path: &Path, source: &str) -> Run {
    let output = Output::default();
    let input: Box<dyn BufRead> = Box::new(std::io::empty());
    let result = kojic::run_source(source, path, Box::new(output.clone()), input);
    let output = String::from_utf8(output.0.lock().unwrap().clone()).unwrap();
    match result {
        Ok(()) => Run { output, error: String::new(), success: true },
        Err(error) => Run { output, error: error.to_string(), success: false },
    }
}

fn compile_and_run(path: &Path) -> Run {
    let executable = path.with_extension("");
    kojic::build_file(path, &executable, kojic::Emit::Executable).unwrap_or_else(|error| panic!("{}", error));
    let result = Command::new(&executable).output().unwrap();
    let stderr = String::from_utf8(result.stderr).unwrap();
    Run {
        output: String::from_utf8(result.stdout).unwrap(),
        error: stderr.lines().last().unwrap_or("").to_string(),
        success: result.status.success(),
    }
}

/// Compiles `source`, runs it, and checks that it prints `expected`, like the interpreter.
fn assert_same(name: &str, source: &str, expected: &str) {
    let path = source_file(name, source);
    let interpreted = interpret(&path, source);
    let compiled = compile_and_run(&path);
    assert_eq!(compiled, interpreted, "the compiled program differs from the interpreter");
    assert_eq!(compiled.output, expected);
}

/// Like `assert_same`, for a program that panics with `message` (after the location).
fn assert_same_panic(name: &str, source: &str, message: &str) {
    let path = source_file(name, source);
    let interpreted = interpret(&path, source);
    let compiled = compile_and_run(&path);
    assert_eq!(compiled, interpreted, "the compiled program differs from the interpreter");
    assert!(!compiled.success);
    assert!(compiled.error.ends_with(message), "unexpected error: {}", compiled.error);
}

#[test]
fn arithmetic() {
    assert_same(
        "arithmetic",
        "\
immut half: float = 1
print(1 + 2 * 3, 7 / 2, -7 / 2, -7 % 3, 2 ** 10, 0 ** 0)
print(half / 2, 7 / 2.0, 2.0 ** 0.5, 5.5 % 2, 1e20, 0.1 + 0.2, -half)
print(6 & 3, 6 | 3, 6 ^ 3, 1 << 4, -16 >> 2, ~0)
print(1 < 2, 2.5 >= 2, 3 == 3.0, 1 != 1)
",
        "7 3 -3 -1 1024 1\n0.5 3.5 1.4142135623730951 1.5 1e20 0.30000000000000004 -1.0\n2 7 5 16 -4 -1\ntrue true true false\n",
    );
}

#[test]
fn strings_and_interpolation() {
    assert_same(
        "strings",
        "\
immut name = 'Koji'
mut text = ''

for i in range(3) {
    text += string(i)
}

print(`Hello, ${name}! ${1 + 1} ${2.0} ${true}`)
print(text, 'ab' * 3, 3 * 'c', 'a' < 'b', text == '012', name != 'Koji')
print('quote \\' and backslash \\\\ and ünicode')
print()
print('a', 1, 2.5, false)
",
        "Hello, Koji! 2 2.0 true\n012 ababab ccc true true false\nquote ' and backslash \\ and ünicode\n\na 1 2.5 false\n",
    );
}

#[test]
fn functions_and_globals() {
    assert_same(
        "functions",
        "\
mut calls = 0

function factorial(n: int): int {
    calls += 1

    if (n <= 1) { return 1 }

    return n * factorial(n - 1)
}

function greet(name: string): string {
    return `Hello, ${name}!`
}

function half(x: float): float {
    return x / 2
}

function show(label: string, value: int) {
    print(label, value)
}

print(factorial(10), calls)
print(greet('World'), half(3))
show('answer', 42)
",
        "3628800 10\nHello, World! 1.5\nanswer 42\n",
    );
}

#[test]
fn control_flow() {
    assert_same(
        "control_flow",
        "\
mut n = 10

while (n > 0 and n % 7 != 0) {
    n -= 1
}

immut label = if (n == 7) { 'seven' } else { 'other' }
print(n, label, not true or false)

for i in range(10, 0, -3) {
    if (i == 4) { continue }

    print(i)
}

mut found = -1

for i in range(100) {
    if (i * i > 50) {
        found = i
        break
    }
}

print(found)
",
        "7 seven false\n10\n7\n1\n8\n",
    );
}

#[test]
fn panics() {
    assert_same_panic("division", "print(1)\nprint(10 / (5 - 5))\n", "division by zero");
    assert_same_panic("overflow", "immut big = 9223372036854775807\nprint(big + 1)\n", "integer overflow");
    assert_same_panic("negation", "immut small = -9223372036854775807 - 1\nprint(-small)\n", "integer overflow");
    assert_same_panic("float_overflow", "print(1e308 * 10)\n", "float overflow");
    assert_same_panic("shift", "print(1 << 64)\n", "invalid shift amount 64");
    assert_same_panic("power", "print(2 ** -1)\n", "negative exponent with int operands; use a float base");
    assert_same_panic("complex", "print((-8.0) ** 0.5)\n", "-8.0 ** 0.5 is not a real number (use math.pow to handle it)");
    assert_same_panic("range_step", "for i in range(0, 10, 0) { print(i) }\n", "range() step cannot be 0");
    assert_same_panic(
        "panic_in_function",
        "function divide(a: int, b: int): int {\n    return a / b\n}\n\nprint(divide(1, 0))\n",
        "division by zero",
    );
}

#[test]
fn lists() {
    assert_same(
        "lists",
        "\
mut numbers = [3, 1, 2]
numbers.push(5)
numbers.insert(0, 10)
print(numbers, numbers.length(), numbers[1])

immut copy = numbers
numbers[0] = 99
numbers.sort()
print(numbers, copy, copy == [10, 3, 1, 2, 5], copy != numbers, 3 in copy, 7 in copy)

mut grid: int[][] = [[1, 2], [3]]
immut row = grid[1]
grid[1].push(4)
grid[0][1] += 18
print(grid, row, grid[1][1])

mut names = []
names.push('Ana')
names.push('Bia')
names.sort()
print(names, names.join(', '), `${names}`, 'ab' in 'cab', string(['it\\'s']))

immut floats: float[] = [1, 2.5]
print(floats, floats + [3], [1, 2.5], numbers[1..3], numbers[..2], numbers[3..])

mut empty: string[] = []
print(empty, empty.length(), [[1], []], 'koji'.length())
",
        "[10, 3, 1, 2, 5] 5 3\n\
[1, 2, 3, 5, 99] [10, 3, 1, 2, 5] true true true false\n\
[[1, 20], [3, 4]] [3] 4\n\
['Ana', 'Bia'] Ana, Bia ['Ana', 'Bia'] true ['it\\'s']\n\
[1.0, 2.5] [1.0, 2.5, 3.0] [1.0, 2.5] [2, 3] [1, 2] [5, 99]\n\
[] 0 [[1], []] 4\n",
    );
}

#[test]
fn lists_in_functions_and_loops() {
    assert_same(
        "list_functions",
        "\
mut log: string[] = []

function total(items: int[]): int {
    mut sum = 0

    for item in items {
        sum += item
    }

    return sum
}

function doubled(items: int[]): int[] {
    mut result: int[] = []

    for item in items {
        result.push(item * 2)
    }

    log.push('doubled')
    return result
}

mut numbers = [1, 2, 3]

for n in numbers {
    // The loop goes through the list as it was.
    numbers.push(n * 10)

    if (n == 2) { break }
}

print(numbers, total(numbers), doubled(numbers), log)
",
        "[1, 2, 3, 10, 20] 36 [2, 4, 6, 20, 40] ['doubled']\n",
    );
}

#[test]
fn list_panics() {
    assert_same_panic("list_index", "immut items = [1, 2]\nprint(items[2])\n", "index 2 out of range (length 2)");
    assert_same_panic("list_negative", "mut items = [1]\nitems[-1] = 0\n", "index -1 out of range (length 1)");
    assert_same_panic("list_insert", "mut items = [1]\nitems.insert(3, 0)\n", "index 3 out of range (length 1)");
    assert_same_panic("list_slice", "immut items = [1, 2]\nprint(items[1..5])\n", "slice 1..5 out of range (length 2)");
}

#[test]
fn examples_that_compile() {
    for example in ["hello", "booleans", "comments", "condition", "interpolation", "length"] {
        let path = PathBuf::from(format!("examples/{}.kj", example));
        let source = std::fs::read_to_string(&path).unwrap();
        let interpreted = interpret(&path, &source);
        let executable = Path::new(env!("CARGO_TARGET_TMPDIR")).join(example);
        kojic::build_file(&path, &executable, kojic::Emit::Executable).unwrap();
        let result = Command::new(&executable).output().unwrap();
        assert_eq!(String::from_utf8(result.stdout).unwrap(), interpreted.output, "{}", example);
    }
}

#[test]
fn unsupported_constructs_are_reported() {
    let path = source_file("unsupported", "mut items = [1, 2, 3]\nprint(items.pop())\n");
    let error = kojic::build_file(&path, &path.with_extension(""), kojic::Emit::Executable).unwrap_err();
    assert!(
        error.to_string().ends_with("unsupported.kj:2: a value of a declared or built-in type (Option, Result, Map, ...) is not supported by 'kojic build' yet (use 'kojic run')"),
        "{}",
        error
    );
}
