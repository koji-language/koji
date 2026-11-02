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

fn has_valgrind() -> bool {
    static FOUND: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FOUND.get_or_init(|| Command::new("valgrind").arg("--version").output().is_ok_and(|o| o.status.success()))
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
    // Under valgrind when it is installed: a memory error (a double free, an invalid read)
    // fails the test even when the output is right.
    let result = if has_valgrind() {
        Command::new("valgrind")
            .args(["-q", "--error-exitcode=99"])
            .arg(&executable)
            .output()
            .unwrap()
    } else {
        Command::new(&executable).output().unwrap()
    };
    let stderr = String::from_utf8(result.stderr).unwrap();
    assert_ne!(result.status.code(), Some(99), "memory errors:\n{}", stderr);
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
fn objects_sum_types_and_match() {
    let source = "type Person = {\n\
    name: string\n\
    age: int\n\
    tags: string[]\n\
}\n\
\n\
type Tree = {\n\
    Leaf\n\
    Node(left: Tree, value: int, right: Tree)\n\
}\n\
\n\
type AppError = {\n\
    Parse(message: string)\n\
    Range(value: int)\n\
}\n\
\n\
function insert(tree: Tree, value: int): Tree {\n\
    return match (tree) {\n\
        Leaf => Node(Leaf, value, Leaf)\n\
        Node(left, v, right) => if (value < v) { Node(insert(left, value), v, right) } else { Node(left, v, insert(right, value)) }\n\
    }\n\
}\n\
\n\
function sum(tree: Tree): int {\n\
    match (tree) {\n\
        Leaf => return 0\n\
        Node(left, value, right) => return sum(left) + value + sum(right)\n\
    }\n\
}\n\
\n\
function check(n: int): Result[int, string] {\n\
    if (n < 0) { return Err('negative') }\n\
\n\
    return Ok(n * 2)\n\
}\n\
\n\
function twice(n: int): Result[int, AppError] {\n\
    immut a = check(n).mapErr(Parse)?\n\
    return Ok(a * 2)\n\
}\n\
\n\
function triple(n: int): Result[int, AppError] {\n\
    immut a = check(n)?\n\
    return Ok(a * 3)\n\
}\n\
\n\
function first(items: int[]): Option[int] {\n\
    if (items.length() == 0) { return None }\n\
\n\
    return Some(items[0])\n\
}\n\
\n\
function addFirst(items: int[]): Option[int] {\n\
    immut value = first(items)?\n\
    return Some(value + 1)\n\
}\n\
\n\
mut people: Person[] = [{ name: 'Ana', age: 30, tags: [] }]\n\
people.push({ age: 25, name: 'Bia', tags: ['x'] })\n\
people[0].age += 1\n\
people[1].tags.push('y')\n\
immut copy = people[1]\n\
people[1].name = 'Bea'\n\
print(people, copy.name, people[0] == people[0], people[0] != copy)\n\
\n\
mut tree = Leaf\n\
\n\
for n in [5, 2, 8, 1] {\n\
    tree = insert(tree, n)\n\
}\n\
\n\
print(tree, sum(tree))\n\
print(check(2), check(-1), addFirst([4]), addFirst([]))\n\
\n\
mut items = [1, 2, 3]\n\
print(items.pop(), items.removeAt(5), items.removeAt(0), items.indexOf(2), items.indexOf(9), items)\n\
\n\
immut maybe = first([7])\n\
\n\
if (maybe is Some) {\n\
    print('got', maybe.value)\n\
}\n\
\n\
for x in [0, 1, 2, 3] {\n\
    immut label = match (x) {\n\
        0 => 'zero'\n\
        n if (n % 2 == 1) => `odd ${n}`\n\
        _ => 'other'\n\
    }\n\
    print(label)\n\
}\n\
\n\
print(Range(3), Parse('bad') == Parse('bad'), twice(1), twice(-1), triple(1), triple(-1))\n\
immut value = check(-5)?\n\
print('unreachable')\n\
";
    let path = source_file("records", source);
    let interpreted = interpret(&path, source);
    let compiled = compile_and_run(&path);
    assert_eq!(compiled, interpreted, "the compiled program differs from the interpreter");
    assert_eq!(compiled.output, "[{ name: 'Ana', age: 31, tags: [] }, { name: 'Bea', age: 25, tags: ['x', 'y'] }] Bia true true\n\
Node(Node(Node(Leaf, 1, Leaf), 2, Leaf), 5, Node(Leaf, 8, Leaf)) 16\n\
Ok(4) Err('negative') Some(5) None\n\
Some(3) None Some(1) Some(0) None [2]\n\
got 7\n\
zero\n\
odd 1\n\
other\n\
odd 3\n\
Range(3) true Ok(4) Err(Parse('negative')) Ok(6) Err(Parse('negative'))\n\
");
    // `?` at the top level stops the program with the error.
    assert_eq!(compiled.error, "error: target/tmp/compiler/records.kj:94: negative");
}

#[test]
fn methods_and_display() {
    assert_same(
        "methods",
        "type Point = {\n\
    x: float\n\
    y: float\n\
}\n\
\n\
type Account = {\n\
    owner: string\n\
    history: int[]\n\
}\n\
\n\
function Point.origin(): Point {\n\
    return { x: 0, y: 0 }\n\
}\n\
\n\
function Point.move(mut self, dx: float, dy: float) {\n\
    self.x += dx\n\
    self.y += dy\n\
}\n\
\n\
function Point.length(self): float {\n\
    return (self.x ** 2 + self.y ** 2) ** 0.5\n\
}\n\
\n\
function Point.display(self): string {\n\
    return `(${self.x}, ${self.y})`\n\
}\n\
\n\
function Account.deposit(mut self, amount: int): int {\n\
    self.history.push(amount)\n\
    return self.history.length()\n\
}\n\
\n\
function reset(mut counter: int, mut names: string[]) {\n\
    counter = 0\n\
    names.push('reset')\n\
}\n\
\n\
mut origin = Point.origin()\n\
immut saved = origin\n\
origin.move(3, 4)\n\
print(origin, saved, origin.length(), origin.debug(), 'it\\'s'.debug(), [origin])\n\
print(`at ${origin}`, string(origin), origin.display())\n\
\n\
mut points = [Point.origin(), Point.origin()]\n\
points[1].move(1, 1)\n\
print(points[1], points[0])\n\
\n\
mut account: Account = { owner: 'Ana', history: [] }\n\
print(account.deposit(10), account.deposit(5), account)\n\
\n\
mut count = 7\n\
mut log = ['start']\n\
reset(mut count, mut log)\n\
print(count, log)\n\
",
        "(3.0, 4.0) (0.0, 0.0) 5.0 { x: 3.0, y: 4.0 } 'it\\'s' [{ x: 3.0, y: 4.0 }]\n\
at (3.0, 4.0) (3.0, 4.0) (3.0, 4.0)\n\
(1.0, 1.0) (0.0, 0.0)\n\
1 2 { owner: 'Ana', history: [10, 5] }\n\
0 ['start', 'reset']\n\
",
    );
}

#[test]
fn maps_and_sets() {
    let source = "mut ages: Map[string, int] = ['Ana': 30, 'Bia': 25]\n\
immut names = [1: 'one', 2: 'two']\n\
mut empty: Map[string, int] = [:]\n\
\n\
print(ages['Ana'], ages.get('Carl'), ages.get('Bia'), 'Bia' in ages, ages.has('Zed'))\n\
\n\
immut before = ages\n\
ages['Carl'] = 41\n\
ages['Ana'] += 1\n\
print(ages.remove('Bia'), ages.remove('Bia'), ages, before, names, empty, empty.length())\n\
\n\
for name in ages {\n\
    print(name, ages[name])\n\
}\n\
\n\
print(ages.keys(), ages.values(), ages == ['Carl': 41, 'Ana': 31], ages != before)\n\
\n\
mut groups: Map[string, int[]] = ['even': [], 'odd': []]\n\
\n\
for n in range(6) {\n\
    if (n % 2 == 0) {\n\
        groups['even'].push(n)\n\
    } else {\n\
        groups['odd'].push(n)\n\
    }\n\
}\n\
\n\
print(groups)\n\
\n\
mut seen: Set[string] = Set.from(['a', 'b', 'a'])\n\
seen.add('c')\n\
seen.add('a')\n\
print(seen.remove('a'), seen.remove('z'), seen.has('b'), 'c' in seen, seen, seen.length(), seen.values())\n\
\n\
for item in seen {\n\
    print(item)\n\
}\n\
\n\
immut points: Set[int[]] = Set.from([[1, 2], [1, 2], [3]])\n\
print(points, Set.from([1, 2]) == Set.from([2, 1]))\n\
\n\
type Point = {\n\
    x: int\n\
    y: int\n\
}\n\
\n\
mut visits: Map[Point, int] = [:]\n\
immut home: Point = { x: 0, y: 0 }\n\
visits[home] = 1\n\
immut same: Point = { x: 0, y: 0 }\n\
visits[same] += 1\n\
print(visits, visits[home])\n\
print(ages['Zed'])\n\
";
    let path = source_file("maps", source);
    let interpreted = interpret(&path, source);
    let compiled = compile_and_run(&path);
    assert_eq!(compiled, interpreted, "the compiled program differs from the interpreter");
    assert_eq!(compiled.output, "30 None Some(25) true false\n\
Some(25) None ['Ana': 31, 'Carl': 41] ['Ana': 30, 'Bia': 25] [1: 'one', 2: 'two'] [:] 0\n\
Ana 31\n\
Carl 41\n\
['Ana', 'Carl'] [31, 41] true true\n\
['even': [0, 2, 4], 'odd': [1, 3, 5]]\n\
true false true true Set(['b', 'c']) 2 ['b', 'c']\n\
b\n\
c\n\
Set([[1, 2], [3]]) true\n\
[{ x: 0, y: 0 }: 2] 2\n\
");
    assert!(compiled.error.ends_with("maps.kj:53: key 'Zed' not found"), "{}", compiled.error);
}

#[test]
fn closures() {
    assert_same(
        "closures",
        "type Person = {\n\
    name: string\n\
    age: int\n\
}\n\
\n\
mut calls = 0\n\
\n\
function makeAdder(amount: int): (int) => int {\n\
    return (x: int) => x + amount\n\
}\n\
\n\
function makeGreeter(greeting: string, names: string[]): () => string {\n\
    immut count = names.length()\n\
    return (): string => {\n\
        calls += 1\n\
        `${greeting}, ${names.join(' and ')} (${count})`\n\
    }\n\
}\n\
\n\
function apply(f: (int) => int, value: int): int {\n\
    return f(value)\n\
}\n\
\n\
function double(n: int): int {\n\
    return n * 2\n\
}\n\
\n\
function compose(f: (int) => int, g: (int) => int): (int) => int {\n\
    return (x: int) => g(f(x))\n\
}\n\
\n\
immut addFive = makeAdder(5)\n\
mut names = ['Ana', 'Bia']\n\
immut greet = makeGreeter('Hello', names)\n\
names.push('Caio')\n\
print(addFive(1), apply(addFive, 10), apply(double, 4), makeAdder(1)(1))\n\
print(greet(), greet(), calls, names)\n\
print(compose(double, addFive)(3), compose(addFive, double)(3))\n\
\n\
immut operations: ((int) => int)[] = [double, addFive, (n: int) => n * n]\n\
\n\
for operation in operations {\n\
    print(operation(3))\n\
}\n\
\n\
mut people: Person[] = [{ name: 'Bia', age: 30 }, { name: 'Ana', age: 30 }, { name: 'Caio', age: 20 }]\n\
people.sortBy((a: Person, b: Person) => a.age < b.age)\n\
print(people)\n\
\n\
mut words = ['pear', 'fig', 'apple']\n\
words.sortBy((a: string, b: string) => a.length() < b.length())\n\
print(words)\n\
\n\
function check(n: int): Result[int, int] {\n\
    if (n < 0) { return Err(n) }\n\
\n\
    return Ok(n)\n\
}\n\
\n\
function parse(n: int): Result[int, string] {\n\
    return check(n).mapErr((e: int) => `bad: ${e}`)\n\
}\n\
\n\
print(parse(12), parse(-1), double, (x: int) => x)\n\
\n\
function run() {\n\
    mut total = 10\n\
    immut show = (): int => total\n\
    total = 20\n\
    print(show(), total)\n\
}\n\
\n\
run()\n\
",
        "6 15 8 2\n\
Hello, Ana and Bia (2) Hello, Ana and Bia (2) 2 ['Ana', 'Bia', 'Caio']\n\
11 16\n\
6\n\
8\n\
9\n\
[{ name: 'Caio', age: 20 }, { name: 'Bia', age: 30 }, { name: 'Ana', age: 30 }]\n\
['fig', 'pear', 'apple']\n\
Ok(12) Err('bad: -1') <function double> <function anonymous function>\n\
10 20\n\
",
    );
}

#[test]
fn generics() {
    assert_same(
        "generics",
        "type Pair[A, B] = {\n\
    first: A\n\
    second: B\n\
}\n\
\n\
type Stack[T: orderable] = {\n\
    items: T[]\n\
}\n\
\n\
function largest[T: orderable](items: T[]): Option[T] {\n\
    if (items.length() == 0) { return None }\n\
\n\
    mut best = items[0]\n\
\n\
    for item in items[1..] {\n\
        if (item > best) { best = item }\n\
    }\n\
\n\
    return Some(best)\n\
}\n\
\n\
function swap[A, B](pair: Pair[A, B]): Pair[B, A] {\n\
    return { first: pair.second, second: pair.first }\n\
}\n\
\n\
function count[T: equatable](items: T[], wanted: T): int {\n\
    mut total = 0\n\
\n\
    for item in items {\n\
        if (item == wanted) { total += 1 }\n\
    }\n\
\n\
    return total\n\
}\n\
\n\
function repeat[T](value: T, times: int): T[] {\n\
    mut result: T[] = []\n\
\n\
    for i in range(times) {\n\
        result.push(value)\n\
    }\n\
\n\
    return result\n\
}\n\
\n\
function Pair.create(first: A, second: B): Pair[A, B] {\n\
    return { first: first, second: second }\n\
}\n\
\n\
function Pair.swapped(self): Pair[B, A] {\n\
    return swap(self)\n\
}\n\
\n\
function Pair.mapFirst[C](self, f: (A) => C): Pair[C, B] {\n\
    return { first: f(self.first), second: self.second }\n\
}\n\
\n\
function Pair.display(self): string {\n\
    return `<${self.first} | ${self.second}>`\n\
}\n\
\n\
function Stack.push(mut self, item: T) {\n\
    self.items.push(item)\n\
}\n\
\n\
function Stack.peek(self): Option[T] {\n\
    return largest(self.items)\n\
}\n\
\n\
immut pair: Pair[int, string] = { first: 1, second: 'one' }\n\
print(largest([3, 9, 2]), largest(['b', 'c', 'a']), largest([1.5]), count([1, 2, 1], 1), count(['a'], 'b'))\n\
print(swap(pair), pair.swapped(), Pair.create(true, 2.5), repeat('ab', 3), repeat([1], 2))\n\
print(pair.mapFirst((n: int) => n * 10), pair.mapFirst((n: int): string => `#${n}`))\n\
print(`${pair}`, swap(swap(pair)))\n\
\n\
mut stack: Stack[string] = { items: [] }\n\
stack.push('x')\n\
stack.push('z')\n\
print(stack, stack.peek())\n\
",
        "Some(9) Some('c') Some(1.5) 2 0\n\
<one | 1> <one | 1> <true | 2.5> ['ab', 'ab', 'ab'] [[1], [1]]\n\
<10 | one> <#1 | one>\n\
<1 | one> <1 | one>\n\
{ items: ['x', 'z'] } Some('z')\n\
",
    );
}

#[test]
fn modules() {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("compiler").join("modules");
    for (name, source) in [
        ("main.kj", "import { count, increment } from 'lib/counter.kj'\n\
import shapes from 'lib/shapes.kj'\n\
import { Shape, largest } from 'lib/shapes.kj'\n\
\n\
print('main starts', count)\n\
increment()\n\
print(count, shapes.largest([3, 7, 2]), largest(['a', 'c']))\n\
\n\
immut all: Shape[] = [Circle(1), shapes.Square(2), shapes.Dot, shapes.unit]\n\
\n\
for shape in all {\n\
    print(shape, shape.area())\n\
}\n\
\n\
function helper(): string {\n\
    return 'from main'\n\
}\n\
\n\
print(helper(), shapes.describe(), shapes.divide(7, 2))\n\
print(shapes.divide(1, 0))\n\
"),
        ("lib/counter.kj", "print('counter loaded')\n\
\n\
export mut count = 0\n\
\n\
export function increment(): int {\n\
    count += 1\n\
    return count\n\
}\n\
"),
        ("lib/shapes.kj", "import { increment } from 'counter.kj'\n\
\n\
print('shapes loaded', increment())\n\
\n\
export type Shape = {\n\
    Circle(radius: float)\n\
    Square(side: float)\n\
    Dot\n\
}\n\
\n\
function Shape.area(self): float {\n\
    return match (self) {\n\
        Circle(radius) => 3.0 * radius * radius\n\
        Square(side) => side * side\n\
        Dot => 0.0\n\
    }\n\
}\n\
\n\
export function largest[T: orderable](items: T[]): T {\n\
    mut best = items[0]\n\
\n\
    for item in items {\n\
        if (item > best) { best = item }\n\
    }\n\
\n\
    return best\n\
}\n\
\n\
export immut unit = Square(1)\n\
\n\
export function divide(a: int, b: int): int {\n\
    return a / b\n\
}\n\
\n\
function helper(): string {\n\
    return 'from shapes'\n\
}\n\
\n\
export function describe(): string {\n\
    return helper()\n\
}\n\
"),
    ] {
        let path = directory.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, source).unwrap();
    }
    let main = directory.join("main.kj");
    let main = main.strip_prefix(std::env::current_dir().unwrap()).unwrap().to_path_buf();
    let interpreted = interpret(&main, &std::fs::read_to_string(&main).unwrap());
    let compiled = compile_and_run(&main);
    assert_eq!(compiled, interpreted, "the compiled program differs from the interpreter");
    assert_eq!(compiled.output, "counter loaded\n\
shapes loaded 1\n\
main starts 1\n\
2 7 c\n\
Circle(1.0) 3.0\n\
Square(2.0) 4.0\n\
Dot 0.0\n\
Square(1.0) 1.0\n\
from main from shapes 3\n\
");
    // A panic in a module shows the module's file.
    assert!(compiled.error.ends_with("shapes.kj:32: division by zero"), "{}", compiled.error);
}

#[test]
fn drops() {
    assert_same(
        "drops",
        "type Conn = {\n\
    name: string\n\
}\n\
\n\
function Conn.drop(mut self) {\n\
    print(`close ${self.name}`)\n\
}\n\
\n\
function Conn.open(name: string): Conn {\n\
    print(`open ${name}`)\n\
    return { name: name }\n\
}\n\
\n\
function Conn.label(self): string {\n\
    return self.name\n\
}\n\
\n\
type Pool = {\n\
    main: Conn\n\
    size: int\n\
}\n\
\n\
function use(take c: Conn) {\n\
    print('using', c.name)\n\
}\n\
\n\
function peek(c: Conn): string {\n\
    return c.name\n\
}\n\
\n\
function make(name: string): Conn {\n\
    immut c = Conn.open(name)\n\
    return c\n\
}\n\
\n\
function early(stop: boolean) {\n\
    immut a = Conn.open('early-a')\n\
\n\
    if (stop) {\n\
        immut b = Conn.open('early-b')\n\
        return\n\
    }\n\
\n\
    print('not stopped')\n\
}\n\
\n\
immut first = Conn.open('first')\n\
\n\
if (true) {\n\
    immut inner = Conn.open('inner')\n\
    immut second = Conn.open('second')\n\
    print('in block')\n\
}\n\
\n\
mut replaced = Conn.open('old')\n\
replaced = Conn.open('new')\n\
print(peek(Conn.open('temp')), Conn.open('temp2').label())\n\
\n\
immut moved = Conn.open('moved')\n\
use(moved)\n\
immut kept = make('made')\n\
early(true)\n\
early(false)\n\
\n\
for i in range(3) {\n\
    immut loop = Conn.open(`loop ${i}`)\n\
\n\
    if (i == 1) { break }\n\
}\n\
\n\
immut pool: Pool = { main: Conn.open('pooled'), size: 1 }\n\
print('end')\n\
",
        "open first\n\
open inner\n\
open second\n\
in block\n\
close second\n\
close inner\n\
open old\n\
open new\n\
close old\n\
open temp\n\
close temp\n\
open temp2\n\
close temp2\n\
temp temp2\n\
open moved\n\
using moved\n\
close moved\n\
open made\n\
open early-a\n\
open early-b\n\
close early-b\n\
close early-a\n\
open early-a\n\
not stopped\n\
close early-a\n\
open loop 0\n\
close loop 0\n\
open loop 1\n\
close loop 1\n\
open pooled\n\
end\n\
close pooled\n\
close made\n\
close new\n\
close first\n\
",
    );
}

#[test]
fn drops_nested() {
    assert_same(
        "drops_nested",
        "type Conn = {\n\
    name: string\n\
}\n\
\n\
function Conn.drop(mut self) {\n\
    print(`close ${self.name}`)\n\
}\n\
\n\
type Session = {\n\
    Active(conn: Conn, user: string)\n\
    Closed\n\
}\n\
\n\
function Session.drop(mut self) {\n\
    print('session ends')\n\
}\n\
\n\
type Wrapper = {\n\
    inner: Conn\n\
}\n\
\n\
function Wrapper.drop(mut self) {\n\
    self.inner.name = `${self.inner.name} (renamed)`\n\
    print('wrapper ends')\n\
}\n\
\n\
function Conn.finish(take self): string {\n\
    return `finished ${self.name}`\n\
}\n\
\n\
function run() {\n\
    immut session: Session = Active({ name: 'db' }, 'ana')\n\
    immut idle: Session = Closed\n\
    mut pool: Conn[] = [{ name: 'a' }]\n\
    pool.push({ name: 'b' })\n\
    immut wrapper: Wrapper = { inner: { name: 'w' } }\n\
    immut solo: Conn = { name: 'solo' }\n\
    print(solo.finish())\n\
    print('run ends')\n\
}\n\
\n\
run()\n\
print('after run')\n\
",
        "close solo\n\
finished solo\n\
run ends\n\
wrapper ends\n\
close w (renamed)\n\
close a\n\
close b\n\
session ends\n\
session ends\n\
close db\n\
after run\n\
",
    );
}

#[test]
fn standard_library() {
    assert_same(
        "standard_library",
        "import math\n\
import path\n\
import system\n\
import { sqrt } from 'math'\n\
\n\
print(math.sqrt(16), math.sqrt(-1), sqrt(2.25), math.pow(2, 10), math.pow(-8, 0.5), math.pow(0, -1))\n\
print(math.abs(-3), math.abs(-2.5), math.floor(2.7), math.ceil(2.1), math.round(2.5), math.pi, math.e)\n\
print(int('42'), int(' 7 '), int('x'), int('99999999999999999999'), float('2.5'), float('nan'), int(3.9), int(true), float(false))\n\
\n\
match (int('abc')) {\n\
    Ok(n) => print(n)\n\
    Err(error) => print('error:', error, error.debug())\n\
}\n\
\n\
immut text = '  Hello, Koji World  '\n\
print(`[${text.trim()}]`, text.upper(), text.lower(), 'Hello World'.isTitle(), '123'.isNumeric(), ''.isSpace())\n\
print('a,b,,c'.split(','), 'banana'.replace('an', 'AN'), 'koji'.startsWith('ko'), 'koji'.endsWith('x'), 'olá mundo'.indexOf('mundo'), 'x'.indexOf('y'))\n\
print(path.join('src', 'main.kj'), path.parent('src/main.kj'), path.parent('main.kj'), path.name('a/b.kj'), path.stem('a/b.kj'), path.extension('Makefile'), path.isAbsolute('/tmp'))\n\
print(system.args(), system.env('KOJI_TEST_VARIABLE'), system.env('HOME') is Some)\n\
\n\
match (system.run('echo', ['hi', 'there'])) {\n\
    Ok(output) => print(output.status, output.stdout.trim(), output.stderr)\n\
    Err(error) => print(error)\n\
}\n\
\n\
immut answer = input('name? ')\n\
print(answer, input())\n\
print('bye')\n\
",
        "Ok(4.0) Err(Complex) Ok(1.5) Ok(1024.0) Err(Complex) Err(Infinite)\n\
3 2.5 2 3 3 3.141592653589793 2.718281828459045\n\
Ok(42) Ok(7) Err(InvalidNumber('x')) Err(OutOfRange('99999999999999999999')) Ok(2.5) Err(InvalidNumber('nan')) 3 1 0.0\n\
error: 'abc' is not a valid number InvalidNumber('abc')\n\
[Hello, Koji World]   HELLO, KOJI WORLD     hello, koji world   true true false\n\
['a', 'b', '', 'c'] bANANa true false Some(4) None\n\
src/main.kj Some('src') Some('.') Some('b.kj') Some('b') None true\n\
[] None true\n\
0 hi there \n\
name? None None\n\
bye\n\
",
    );
}

#[test]
fn chars() {
    let source = "immut initial: char = 'K'\n\
immut text = 'olá, Koji'\n\
immut letters: char[] = ['b', 'a', '\\'']\n\
mut sorted = letters\n\
sorted.sort()\n\
print(initial, initial.code(), char(97), char(-1), initial.debug(), letters, sorted)\n\
print(text[2], text[5..9], text[..3], text.chars(), 'K' in text, 'z' in text, initial == 'K', 'a' < 'b')\n\
\n\
mut vowels = 0\n\
\n\
for c in text {\n\
    if (c in 'aeiouá') { vowels += 1 }\n\
}\n\
\n\
print(vowels, `${initial}!`, string(initial))\n\
mut counts: Map[char, int] = [:]\n\
\n\
for c in 'banana' {\n\
    if (c in counts) {\n\
        counts[c] += 1\n\
    } else {\n\
        counts[c] = 1\n\
    }\n\
}\n\
\n\
print(counts)\n\
print(text[20])\n\
";
    let path = source_file("chars", source);
    let interpreted = interpret(&path, source);
    let compiled = compile_and_run(&path);
    assert_eq!(compiled, interpreted, "the compiled program differs from the interpreter");
    assert_eq!(compiled.output, "K 75 Some('a') None 'K' ['b', 'a', '\\''] ['\\'', 'a', 'b']\n\
á Koji olá ['o', 'l', 'á', ',', ' ', 'K', 'o', 'j', 'i'] true false true true\n\
4 K! K\n\
['b': 1, 'a': 3, 'n': 2]\n\
");
    assert!(compiled.error.ends_with("chars.kj:27: index 20 out of range (length 9)"), "{}", compiled.error);
}

#[test]
fn files() {
    // The program works in a directory of its own, emptied before each run.
    let data = Path::new(env!("CARGO_TARGET_TMPDIR")).join("compiler").join("files-data");
    let source = "import file\n\
import { Mode } from 'file'\n\
\n\
function save(path: string, text: string): Result[void, file.FileError] {\n\
    immut out = file.open(path, file.Write)?\n\
    return out.write(text)\n\
}\n\
\n\
function load(path: string): Result[string, file.FileError] {\n\
    immut input = file.open(path, Read)?\n\
    return input.read()\n\
}\n\
\n\
print(file.createDir('DATA/sub'), file.exists('DATA'), file.exists('nothing'))\n\
print(save('DATA/notes.txt', 'first line\\n'))\n\
\n\
match (file.open('DATA/notes.txt', Append)) {\n\
    Ok(f) => {\n\
        print(f.write('second line\\n'), f)\n\
        f.close()\n\
    }\n\
    Err(error) => print(error)\n\
}\n\
\n\
print(load('DATA/notes.txt'))\n\
print(load('DATA/missing.txt'), file.list('DATA'), file.list('nothing'))\n\
\n\
match (load('DATA')) {\n\
    Ok(text) => print(text)\n\
    Err(error) => print('error:', error)\n\
}\n\
\n\
immut text = load('DATA/notes.txt')?\n\
print(text.split('\\n').length())\n\
load('DATA/absent.txt')?\n\
print('not printed')\n\
".replace("DATA", &data.display().to_string());
    let path = source_file("files", &source);
    let _ = std::fs::remove_dir_all(&data);
    let interpreted = interpret(&path, &source);
    let _ = std::fs::remove_dir_all(&data);
    let compiled = compile_and_run(&path);
    assert_eq!(compiled, interpreted, "the compiled program differs from the interpreter");
    assert!(compiled.output.contains("Ok('first line\\nsecond line\\n')"), "{}", compiled.output);
    assert!(compiled.error.ends_with("absent.txt' not found"), "{}", compiled.error);
}

#[test]
fn examples_that_compile() {
    for example in ["hello", "booleans", "comments", "condition", "interpolation", "length", "shapes", "functions", "input", "modules", "types"] {
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
    let path = source_file("unsupported", "import json\nprint(json.stringify(json.Null))\n");
    let error = kojic::build_file(&path, &path.with_extension(""), kojic::Emit::Executable).unwrap_err();
    assert!(
        error.to_string().contains("is not supported by 'kojic build' yet (use 'kojic run')"),
        "{}",
        error
    );
}
