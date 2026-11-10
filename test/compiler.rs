//! Compiles programs with `kojic build` (the compiler written in Koji) and checks what the
//! executable does: its output, its panic message, its exit code.

use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "koji/mod.rs"]
mod koji;

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

/// Builds `path` to `executable` with the compiler written in Koji.
fn build(path: &Path, executable: &Path) {
    let built = koji::finish(koji::kojic().arg("build").arg(path).arg("-o").arg(executable), "");
    assert_eq!(built.code, Some(0), "{} does not compile: {}", path.display(), built.stderr);
}

fn compile_and_run(path: &Path) -> Run {
    let executable = path.with_extension("");
    build(path, &executable);
    // Under valgrind when it is installed: a memory error (a double free, an invalid read)
    // fails the test even when the output is right. Without the pool, so that valgrind sees each
    // block given back.
    let result = if has_valgrind() {
        Command::new("valgrind")
            .env("KOJI_POOL", "0")
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

/// Compiles `source`, runs it, and checks that it succeeds and prints `expected`.
fn assert_same(name: &str, source: &str, expected: &str) {
    let path = source_file(name, source);
    let compiled = compile_and_run(&path);
    assert_eq!(compiled.output, expected);
    assert!(compiled.success, "the program failed: {}", compiled.error);
}

/// Like `assert_same`, for a program that panics with `message` (after the location).
fn assert_same_panic(name: &str, source: &str, message: &str) {
    let path = source_file(name, source);
    let compiled = compile_and_run(&path);
    assert!(!compiled.success, "the program succeeded: {}", compiled.output);
    assert!(compiled.error.ends_with(message), "unexpected error: {}", compiled.error);
}

#[test]
fn arithmetic() {
    assert_same(
        "arithmetic",
        "\
immut half: decimal = 1
print(1 + 2 * 3, 7 / 2, -7 / 2, -7 % 3, 2 ** 10, 0 ** 0)
print(half / 2, 7 / 2.0, 2.0 ** 0.5, 5.5 % 2, 1e20, 0.1 + 0.2, -half)
print(6 & 3, 6 | 3, 6 ^ 3, 1 << 4, -16 >> 2, ~0)
print(1 < 2, 2.5 >= 2, 3 == 3.0, 1 != 1)
",
        "7 3 -3 -1 1024 1\n\
0.5 3.5 1.414213562373095048801688724209698 1.5 100000000000000000000 0.3 -1\n\
2 7 5 16 -4 -1\n\
true true true false\n\
",
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
        "Hello, Koji! 2 2 true\n\
012 ababab ccc true true false\n\
quote ' and backslash \\ and ünicode\n\
\n\
a 1 2.5 false\n\
",
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

function half(x: decimal): decimal {
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
fn arms_that_leave() {
    // An arm of a `match` used as a value may leave with `return`, `break` or `continue`.
    assert_same(
        "arms_that_leave",
        "\
function next(r: Result[int, string]): Result[int, string] {
    immut n = match (r) {
        Ok(n) => n
        Err(message) => {
            immut text = `bad ${message}`
            return Err(text)
        }
    }
    return Ok(n + 1)
}

function first(items: string[]): string {
    immut item = match (items.length()) {
        0 => return 'none'
        _ => items[0]
    }
    return item
}

print(next(Ok(1)), next(Err('e')), first([]), first(['a']))
mut total = 0

for i in range(5) {
    immut value = match (i) {
        3 => break
        1 => continue
        other => other * 10
    }
    total += value
}

print(total)
",
        "Ok(2) Err('bad e') none a\n20\n",
    );
}

#[test]
fn mut_arguments_see_the_other_arguments() {
    // A `mut` argument (or `mut self`) is the variable itself: the function gets its value at
    // the call, after the other arguments ran.
    assert_same(
        "mut_arguments",
        "\
type Counter = { n: int, log: string[] }

function Counter.bump(mut self): int {
    self.n += 1
    return self.n
}

function Counter.note(mut self, value: int): string {
    self.log.push(`${value} at ${self.n}`)
    return `noted ${value}`
}

function add(mut items: int[], value: int) {
    items.push(value)
}

function grow(mut items: int[]): int {
    items.push(0)
    return items.length()
}

mut counter: Counter = { n: 0, log: [] }
print(counter.note(counter.bump()), counter.n, counter.log)
mut items = [1]
add(mut items, grow(mut items))
print(items)
",
        "noted 1 1 ['1 at 1']\n[1, 0, 2]\n",
    );
}

#[test]
fn mut_parameters_change_values_in_place() {
    // A local variable passed as `mut` (or `mut self`) gives its value to the function, which
    // changes it in place: copying the map at each call would make this loop quadratic.
    assert_same(
        "in_place",
        "\
type Tables = { types: Map[int, int] }
type Checker = { tables: Tables, calls: int }

function Checker.record(mut self, id: int) {
    self.tables.types[id] = id * 2
    self.calls += 1
}

function grow(mut items: int[], count: int) {
    for i in range(count) {
        items.push(i)
    }
}

function run(): string {
    mut checker: Checker = { tables: { types: [:] }, calls: 0 }

    for i in range(3000) {
        checker.record(i)
    }

    mut items: int[] = []

    for i in range(30) {
        grow(mut items, 100)
    }

    return `${checker.tables.types.length()} ${checker.calls} ${items.length()}`
}

print(run())
",
        "3000 3000 3000\n",
    );
}

#[test]
fn errors_files_and_the_operating_system() {
    // `printError`, `file.remove`, `system.os` and `system.run` (whose program writes to the same
    // outputs), compiled: what each writes to the standard output and the standard error output.
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("compiler-system");
    std::fs::create_dir_all(&directory).unwrap();
    let scratch = directory.join("scratch.txt");
    let folder = directory.join("folder");
    let _ = std::fs::create_dir_all(&folder);
    let source = format!(
        "\
import file
import system

type Point = {{ x: int }}

function Point.display(self): string {{
    return `(${{self.x}})`
}}

immut point: Point = {{ x: 1 }}
print('out', point)
printError('error', point, 2)
printError()
print('after')

match (file.open('{scratch}', file.Write)) {{
    Ok(output) => print(output.write('x'))
    Err(error) => print(error)
}}

print(file.exists('{scratch}'), file.remove('{scratch}'), file.exists('{scratch}'))
print(file.remove('{scratch}'))
print(file.remove('{folder}') is Err)

immut known = match (system.os) {{
    Linux => true
    MacOS => true
    Windows => true
    Other(_) => false
}}
print(known)
print(system.run('echo', ['from', 'echo']))
print(system.run('sh', ['-c', 'echo to errors >&2; exit 4']))
",
        scratch = scratch.display(),
        folder = folder.display()
    );
    let path = source_file("system", &source);
    let executable = path.with_extension("");
    build(&path, &executable);
    let compiled = Command::new(&executable).output().unwrap();
    assert!(compiled.status.success());
    let expected = format!(
        "out (1)\nafter\nOk(void)\ntrue Ok(void) false\nErr(NotFound('{}'))\ntrue\ntrue\nfrom echo\nOk(0)\nOk(4)\n",
        scratch.display()
    );
    assert_eq!(String::from_utf8_lossy(&compiled.stdout), expected);
    assert_eq!(String::from_utf8_lossy(&compiled.stderr), "error (1) 2\n\nto errors\n");
}

#[test]
fn decimals_in_records_lists_and_maps() {
    // A decimal takes 16 bytes: two words in a record, a list or a map.
    assert_same(
        "decimals",
        "\
type Item = { name: string, price: decimal, count: int, tax: decimal }
type Shape = { Circle(r: decimal) Square(side: decimal, label: string) }
immut a: Item = { name: 'pen', price: 1.25, count: 3, tax: 0.07 }
mut items: Item[] = [a, { name: 'ink', price: 10.5, count: 1, tax: 0.2 }]
items[0].price += 0.75
items.push({ name: 'cap', price: 0.05, count: 9, tax: 0 })
mut total: decimal = 0
for item in items {
    total += item.price * item.count * (1 + item.tax)
}
print(items, total)
immut shapes = [Circle(1.5), Square(2, 's')]
print(shapes, shapes[1] == Square(2.00, 's'))
mut ds: decimal[] = [3, 1.5, 2.25]
ds.sort()
ds.insert(1, 9.75)
print(ds, ds.pop(), ds.removeAt(0), ds, 2.25 in ds, ds.indexOf(9.75))
immut m: Map[decimal, decimal] = [1.5: 2.5, 3: 4]
print(m, m[1.50], m.keys(), m.values())
mut s = Set.from(ds)
s.add(1.5)
print(s, ds[0..1], ds + ds)
immut f = (x: decimal) => x * 2
print(f(1.25), [decimal(1), 2].length())
mut w = ds
w.sortBy((x: decimal, y: decimal) => x > y)
print(w)
",
        "[{ name: 'pen', price: 2, count: 3, tax: 0.07 }, { name: 'ink', price: 10.5, count: 1, tax: 0.2 }, { name: 'cap', price: 0.05, count: 9, tax: 0 }] 19.47\n\
[Circle(1.5), Square(2, 's')] true\n\
[1.5, 9.75, 2.25, 3] Some(3) Some(1.5) [9.75, 2.25] true Some(0)\n\
[1.5: 2.5, 3: 4] 2.5 [1.5, 3] [2.5, 4]\n\
Set([9.75, 2.25, 1.5]) [9.75] [9.75, 2.25, 9.75, 2.25]\n\
2.5 2\n\
[9.75, 2.25]\n\
",
    );
}

#[test]
fn borrowed_parameters() {
    // `items` is borrowed (the function only reads it); given a global that the call changes, the
    // caller keeps the old value alive until the call ends.
    assert_same(
        "borrowed",
        "\
mut names: string[] = ['a', 'b']

function reset() {
    names = ['x']
}

function show(items: string[]): string {
    reset()
    return items.join(',')
}

function longest(words: string[], fallback: string): string {
    mut best = fallback

    for word in words {
        if (word.length() > best.length()) { best = word }
    }

    return best
}

print(show(names), names)
print(longest(['ab', 'abc'], ''), longest([], 'none'))
",
        "a,b ['x']\nabc none\n",
    );
}

#[test]
fn borrowed_fields() {
    // A field of a borrowed parameter is read without a reference of its own: the parameter keeps
    // it alive, even when the call changes the global it came from; a copy changed is its own.
    assert_same(
        "borrowed fields",
        "\
type Inner = { words: string[] }
type Outer = { inner: Inner, name: string }

mut current: Outer = { inner: { words: ['a', 'b'] }, name: 'first' }

function reset() {
    current = { inner: { words: [] }, name: 'second' }
}

function words(outer: Outer): string[] {
    immut list = outer.inner.words
    reset()
    mut more = list
    more.push(outer.name)
    return [list.join(','), more.join(',')]
}

function last(outer: Outer): string {
    immut list = outer.inner.words
    return list[list.length() - 1]
}

print(words(current), current.name)
print(last({ inner: { words: ['x', 'y'] }, name: 'z' }))
",
        "['a,b', 'a,b,first'] second\ny\n",
    );
}

#[test]
fn records_on_the_stack() {
    // A record that does not leave its function is on its stack (escape.kj): built again in a
    // loop, given to functions that only read it, compared; one kept elsewhere is not.
    assert_same(
        "stack records",
        "\
type Point = { name: string, tags: string[] }
type Shape = {
    Dot(name: string)
    Line(from: string, to: string)
}

mut kept: Point[] = []

function describe(point: Point): string {
    return `${point.name}:${point.tags.join('+')}`
}

function keep(point: Point) {
    kept.push(point)
}

function deeper(point: Point): string {
    return describe(point)
}

mut texts: string[] = []

for i in range(3) {
    immut point: Point = { name: `p${i}`, tags: [string(i), 'x'] }
    texts.push(deeper(point))
    immut other: Point = { name: `k${i}`, tags: [] }
    keep(other)
    immut shape = Line(`a${i}`, 'b')

    if (shape == Line('a1', 'b')) { texts.push('found') }
}

print(texts, kept.length(), kept[2].name)
",
        "['p0:0+x', 'p1:1+x', 'found', 'p2:2+x'] 3 k2\n",
    );
    // Returned, captured, stored by the function it is given to: on the heap. Given to a
    // recursive call that only reads it: on the stack.
    assert_same(
        "stack records escaping",
        "\
type P = { name: string }

mut saved: P[] = []

function same(p: P): P {
    return p
}

function later(p: P): () => string {
    return () => p.name
}

function store(p: P) {
    saved.push(p)
}

function relay(p: P) {
    store(p)
}

function depth(p: P, n: int): int {
    if (n == 0) { return p.name.length() }

    immut inner: P = { name: p.name + '!' }
    return depth(inner, n - 1)
}

function run(): string[] {
    immut a: P = { name: 'a' }
    immut b = same(a)
    immut c: P = { name: 'c' }
    immut f = later(c)
    immut d: P = { name: 'd' }
    relay(d)
    immut e: P = { name: 'e' }
    return [b.name, f(), string(depth(e, 3))]
}

print(run(), saved)
",
        "['a', 'c', '4'] [{ name: 'd' }]\n",
    );
}

#[test]
fn pushes_and_comparisons_inline() {
    // A push writes in place when the list has room and is not shared (a bool or a char fills
    // its word with zeros); a comparison with a variant built on the spot checks its tag and
    // its fields, or asks the runtime (a decimal).
    assert_same(
        "inline",
        "\
type Token = {
    Word(text: string)
    Number(value: int, exact: boolean)
    Mark(c: char)
    Amount(value: decimal)
    End
}

mut flags: boolean[] = []
mut letters: char[] = []
mut amounts: decimal[] = []
mut words: string[] = []

for i in range(10) {
    flags.push(i % 3 == 0)
    letters.push('a')
    amounts.push(1.5)
    words.push(string(i))
}

immut before = words
words.push('x')

function kind(token: Token): string {
    if (token == Word('if')) { return 'if' }
    if (token != Number(1, true)) { if (token == Mark('+')) { return 'plus' } }
    if (token == Amount(2.50)) { return 'amount' }
    if (token == End) { return 'end' }
    return 'other'
}

print(flags, letters.length(), amounts[9], words.length(), before.length(), flags == [true, false, false, true, false, false, true, false, false, true])
print(kind(Word('if')), kind(Word('of')), kind(Number(1, true)), kind(Mark('+')), kind(Amount(2.5)), kind(End))
",
        "[true, false, false, true, false, false, true, false, false, true] 10 1.5 11 10 true\nif other other plus amount end\n",
    );
}

#[test]
fn chars_appended() {
    // `text += string(c)` appends the char in place (in UTF-8), without a string for it; a copy
    // keeps its value.
    assert_same(
        "chars appended",
        "\
mut text = ''
mut copies: string[] = []

for c in 'aé€😀b' {
    text += string(c)
    copies.push(text)
}

immut kept = text
text += string('!')
mut other = kept
other += string('?')
print(text, kept, other, copies, text.length())
",
        "aé€😀b! aé€😀b aé€😀b? ['a', 'aé', 'aé€', 'aé€😀', 'aé€😀b'] 6\n",
    );
}

#[test]
fn values_released_at_their_last_use() {
    // `self.ids.get(name)` no longer holds the map when `self.ids[name] = id` changes it, which
    // is then done in place; a copy keeps its value.
    assert_same(
        "last use",
        "\
type Table = { ids: Map[string, int], names: string[] }

function Table.add(mut self, name: string): int {
    match (self.ids.get(name)) {
        Some(id) => return id
        None => {}
    }

    self.names.push(name)
    immut id = self.names.length() - 1
    self.ids[name] = id
    return id
}

mut table: Table = { ids: [:], names: [] }
mut found: int[] = []

for name in ['a', 'b', 'a', 'c'] {
    found.push(table.add(name))
}

immut before = table
table.add('d')
print(found, table.names, before.names, before.ids.length(), table.ids['d'])
",
        "[0, 1, 0, 2] ['a', 'b', 'c', 'd'] ['a', 'b', 'c'] 3 3\n",
    );
}

#[test]
fn borrowed_items_and_globals() {
    // An item of a borrowed list, and a value read from a global that never changes, are read
    // without a reference; a global that changes keeps its old value alive for the call it was
    // given to, and an item read from it has a reference of its own.
    assert_same(
        "borrowed items",
        "\
immut fixed = ['x', 'y']
mut changing = ['a', 'b']

function joined(words: string[]): string {
    mut text = ''

    for word in words {
        changing = ['c']
        text += word
    }

    immut first = words[0]
    changing = ['q']
    return text + first
}

function fromGlobal(): string {
    immut item = changing[0]
    changing = ['z']
    immut copy = fixed
    return item + copy[1] + fixed[0]
}

print(joined(changing), changing, fromGlobal(), changing, joined(fixed))
",
        "aba ['q'] qyx ['z'] xyx\n",
    );
}

#[test]
fn items_set_in_place() {
    // `items[i] = value` writes in place when the list is not shared and `i` is in it; a copy
    // keeps its value, and an index out of range panics.
    assert_same(
        "items set",
        "\
mut words = ['a', 'b', 'c']
immut before = words
words[1] = 'x'
words[2] = words[0] + '!'
mut flags = [false, false]
flags[1] = true
mut amounts: decimal[] = [1.5, 2.5]
amounts[0] = 9.25
mut grid = [[1, 2], [3, 4]]
immut row = grid[0]
grid[0][1] = 7
mut names: string[] = []
for i in range(3) {
    names.push(string(i))
    names[i] = names[i] + '?'
}
print(words, before, flags, amounts, grid, row, names)
",
        "['a', 'x', 'a!'] ['a', 'b', 'c'] [false, true] [9.25, 2.5] [[1, 7], [3, 4]] [1, 2] ['0?', '1?', '2?']\n",
    );
    assert_same_panic("items set out of range", "mut words = ['a']\nwords[5] = 'z'\n", "index 5 out of range (length 1)");
}

#[test]
fn values_moved_at_their_last_use() {
    // `text += c` adds in place only when no one else sees the string: a copy keeps its value.
    assert_same(
        "moves",
        "\
function build(count: int): string {
    mut text = ''

    for i in range(count) {
        text += string(i % 10)
    }

    return text
}

function pair(first: string): string[] {
    immut second = first + '!'
    return [first, second]
}

mut text = build(5)
immut copy = text
text += 'x'
immut again = text
text += 'y'
print(text, copy, again, build(12).length())
print(pair('a'), text + text)

mut names: string[] = []

for name in ['ana', 'bia'] {
    mut shout = name
    shout += '!'
    names.push(shout)
}

print(names)
",
        "01234xy 01234 01234x 12\n['a', 'a!'] 01234xy01234xy\n['ana!', 'bia!']\n",
    );
}

#[test]
fn fields_written_in_place() {
    // A field is written in the record itself, unless someone else sees the record; the value
    // assigned is taken first, so `r.next = Some(r)` keeps the record as it was.
    assert_same(
        "fields",
        "\
type Node = { name: string, next: Option[Node], items: string[] }

function named(name: string): Node {
    immut items = [name]
    return { name: name, next: None, items: items }
}

mut r = named('a')
r.next = Some(r)
mut copy = r
copy.name = 'b' + copy.name
r.items.push('y')
mut nodes: Map[string, Node] = [:]
immut key = 'key'
nodes[key] = r
nodes[key].name = key
print(r)
print(copy)
print(nodes)
",
        "{ name: 'a', next: Some({ name: 'a', next: None, items: ['a'] }), items: ['a', 'y'] }\n\
{ name: 'ba', next: Some({ name: 'a', next: None, items: ['a'] }), items: ['a'] }\n\
['key': { name: 'key', next: Some({ name: 'a', next: None, items: ['a'] }), items: ['a', 'y'] }]\n",
    );
}

#[test]
fn compound_assignments() {
    assert_same(
        "compound",
        "\
mut x = 12
x &= 10
x |= 5
x ^= 3
x <<= 2
x >>= 1
mut items = [1, 2]
items[0] |= 6
mut total = 7.5
total -= 1
total *= 2
total /= 4
total **= 2
total %= 3
print(x, items, total)
",
        "28 [7, 2] 1.5625\n",
    );
    assert_same_panic("shift_assign", "mut x = 1\nx <<= 64\n", "invalid shift amount 64");
}

#[test]
fn nested_functions_ranges_sets_and_module_variants() {
    assert_same(
        "nested_functions",
        "\
import json

function make(): int[] {
    mut x = 1

    function twice(n: int): int {
        return n * 2 + x
    }

    x = 5
    return [twice(1), twice(x)]
}

mut seen = Set.from([])
seen.add('x')
print(make(), range(3), range(5, 0, -2), seen)
print(json.stringify(json.Array([json.Int(1), json.Null, json.Bool(true)])))
",
        "[3, 11] [0, 1, 2] [5, 3, 1] Set(['x'])\n[1,null,true]\n",
    );
    assert_same_panic("range_value", "print(range(0, 5, 0))\n", "range() step cannot be 0");
}

#[test]
fn panics() {
    assert_same_panic("division", "print(1)\nprint(10 / (5 - 5))\n", "division by zero");
    assert_same_panic("overflow", "immut big = 9223372036854775807\nprint(big + 1)\n", "integer overflow");
    assert_same_panic("negation", "immut small = -9223372036854775807 - 1\nprint(-small)\n", "integer overflow");
    assert_same_panic("float_overflow", "print(float(1e308) * float(10))\n", "float overflow");
    assert_same_panic("shift", "print(1 << 64)\n", "invalid shift amount 64");
    assert_same_panic("power", "print(2 ** -1)\n", "negative exponent with int operands; use a decimal base");
    assert_same_panic("complex", "print((-8.0) ** 0.5)\n", "-8 ** 0.5 is not a real number (use math.pow to handle it)");
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

immut floats: decimal[] = [1, 2.5]
print(floats, floats + [3], [1, 2.5], numbers[1..3], numbers[..2], numbers[3..])

mut empty: string[] = []
print(empty, empty.length(), [[1], []], 'koji'.length())
",
        "[10, 3, 1, 2, 5] 5 3\n\
[1, 2, 3, 5, 99] [10, 3, 1, 2, 5] true true true false\n\
[[1, 20], [3, 4]] [3] 4\n\
['Ana', 'Bia'] Ana, Bia ['Ana', 'Bia'] true ['it\\'s']\n\
[1, 2.5] [1, 2.5, 3] [1, 2.5] [2, 3] [1, 2] [5, 99]\n\
[] 0 [[1], []] 4\n\
",
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
    let compiled = compile_and_run(&path);
    assert!(!compiled.success && compiled.error.starts_with("error: ") && compiled.error.ends_with("records.kj:94: negative"), "{}", compiled.error);
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
    x: decimal\n\
    y: decimal\n\
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
function Point.move(mut self, dx: decimal, dy: decimal) {\n\
    self.x += dx\n\
    self.y += dy\n\
}\n\
\n\
function Point.length(self): decimal {\n\
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
        "(3, 4) (0, 0) 5 { x: 3, y: 4 } 'it\\'s' [{ x: 3, y: 4 }]\n\
at (3, 4) (3, 4) (3, 4)\n\
(1, 1) (0, 0)\n\
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
    let compiled = compile_and_run(&path);
    assert!(!compiled.success && compiled.error.starts_with("panic: ") && compiled.error.ends_with("maps.kj:53: key 'Zed' not found"), "{}", compiled.error);
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
fn many_keys() {
    // The index of a map grows, loses keys and is copied; ints that differ only in their high bits
    // are found as well as the others.
    assert_same(
        "many keys",
        "\
mut squares: Map[int, int] = [:]
mut names: Map[string, int] = [:]

for i in range(2000) {
    squares[i * 4294967296] = i * i
    names[`n${i}`] = i
}

immut copy = squares

for i in range(0, 2000, 2) {
    squares.remove(i * 4294967296)
    names.remove(`n${i}`)
}

mut total = 0

for i in range(2000) {
    match (squares.get(i * 4294967296)) {
        Some(value) => total += value
        None => {}
    }
}

print(squares.length(), copy.length(), names.length(), total)
print(names['n1999'], 'n1998' in names, copy[1998 * 4294967296], names.keys()[..3])
",
        "1000 2000 1000 1333333000\n1999 false 3992004 ['n1', 'n3', 'n5']\n",
    );
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
    Circle(radius: decimal)\n\
    Square(side: decimal)\n\
    Dot\n\
}\n\
\n\
function Shape.area(self): decimal {\n\
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
    let compiled = compile_and_run(&main);
    assert_eq!(compiled.error, "panic: lib/shapes.kj:32: division by zero");
    assert!(!compiled.success);
    assert_eq!(compiled.output, "counter loaded\n\
shapes loaded 1\n\
main starts 1\n\
2 7 c\n\
Circle(1) 3\n\
Square(2) 4\n\
Dot 0\n\
Square(1) 1\n\
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
match (system.capture('echo', ['hi', 'there'])) {\n\
    Ok(output) => print(output.status, output.stdout.trim(), output.stderr)\n\
    Err(error) => print(error)\n\
}\n\
\n\
immut answer = input('name? ')\n\
print(answer, input())\n\
print('bye')\n\
",
        "Ok(4) Err(Complex) Ok(1.5) Ok(1024) Err(Complex) Err(Infinite)\n\
3 2.5 2 3 3 3.141592653589793238462643383279503 2.718281828459045235360287471352662\n\
Ok(42) Ok(7) Err(InvalidNumber('x')) Err(OutOfRange('99999999999999999999')) Ok(0x1.4p1) Err(InvalidNumber('nan')) 3 1 0x0p0\n\
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
    let compiled = compile_and_run(&path);
    assert!(!compiled.success && compiled.error.starts_with("panic: ") && compiled.error.ends_with("chars.kj:27: index 20 out of range (length 9)"), "{}", compiled.error);
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
    out.write(text)?\n\
    return Ok()\n\
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
    let _ = std::fs::remove_dir_all(&data);
    let compiled = compile_and_run(&path);
    assert!(!compiled.success);
    assert!(compiled.output.contains("Ok('first line\\nsecond line\\n')"), "{}", compiled.output);
    assert!(compiled.error.ends_with("absent.txt' not found"), "{}", compiled.error);
}

#[test]
fn dates() {
    assert_same("dates", "import datetime\n\
import { DateTime } from 'datetime'\n\
\n\
immut now: DateTime = datetime.now()\n\
print(now.year >= 2026, now.month >= 1, datetime.today().length())\n\
print(datetime.format(0, 'yyyy-mm-dd hh:min:ss'), datetime.format(63867219200.7, 'dd/mm/yyyy'))\n\
print(datetime.parse('2024-02-29 12:30:00', 'yyyy-mm-dd hh:min:ss'), datetime.parse('2023-02-29', 'yyyy-mm-dd'), datetime.parse('2024/01/01', 'yyyy-mm-dd'))\n\
\n\
match (datetime.parse('31/12/1999', 'dd/mm/yyyy')) {\n\
    Ok(timestamp) => print(datetime.format(timestamp, 'yyyy-mm-dd'))\n\
    Err(error) => print(error)\n\
}\n\
\n\
match (datetime.parse('x', 'yyyy')) {\n\
    Ok(timestamp) => print(timestamp)\n\
    Err(error) => print('error:', error)\n\
}\n\
\n\
print(string(now).length())\n\
", "true true 10\n\
0000-01-01 00:00:00 14/11/2023\n\
Ok(63876429000) Err(InvalidDate('2023-02-29')) Err(PatternMismatch('2024/01/01', 'yyyy-mm-dd'))\n\
1999-12-31\n\
error: 'x' does not match the pattern 'yyyy'\n\
19\n\
");
}

#[test]
fn regular_expressions() {
    assert_same("regular_expressions", "import regex\n\
\n\
match (regex.compile('[0-9]+')) {\n\
    Ok(numbers) => {\n\
        immut text = 'olá 12, mundo 345 e 6'\n\
        print(numbers, numbers.test(text), numbers.test('abc'))\n\
        print(numbers.find(text), numbers.find('none'))\n\
        print(numbers.findAll(text))\n\
\n\
        for found in numbers.findAll(text) {\n\
            print(found.value, found.start, found.end)\n\
        }\n\
\n\
        print(numbers.replace(text, '#'), numbers.replaceAll(text, '#'), numbers.split('a1b22c'))\n\
    }\n\
    Err(error) => print(error)\n\
}\n\
\n\
print(regex.compile('('), regex.compile('a').debug())\n\
\n\
match (regex.compile('[')) {\n\
    Ok(r) => print(r)\n\
    Err(error) => print('error:', error)\n\
}\n\
", "<regex '[0-9]+'> true false\n\
Some({ value: '12', start: 4, end: 6 }) None\n\
[{ value: '12', start: 4, end: 6 }, { value: '345', start: 14, end: 17 }, { value: '6', start: 20, end: 21 }]\n\
12 4 6\n\
345 14 17\n\
6 20 21\n\
olá #, mundo 345 e 6 olá #, mundo # e # ['a', 'b', 'c']\n\
Err(InvalidPattern('(', 'unclosed group')) Ok(<regex 'a'>)\n\
error: invalid regular expression '[': unclosed character class\n\
");
}

#[test]
fn json_values() {
    assert_same("json_values", "import json\n\
import { Json } from 'json'\n\
\n\
immut text = '{\"name\": \"Koji\", \"version\": 1, \"ratio\": 2.50, \"tags\": [\"fast\", \"safe\"], \"nested\": {\"ok\": true, \"none\": null}, \"name\": \"Koji 2\", \"big\": 1e300, \"escaped\": \"a\\\\\"b\\\\nc\\\\u00e9\"}'\n\
\n\
match (json.parse(text)) {\n\
    Ok(value) => {\n\
        print(value)\n\
        print(json.stringify(value))\n\
        print(json.get(value, 'name'), json.get(value, 'missing'))\n\
\n\
        match (json.get(value, 'version')) {\n\
            Some(version) => print(json.asInt(version), json.asDecimal(version), json.asString(version))\n\
            None => print('no version')\n\
        }\n\
\n\
        match (json.get(value, 'tags')) {\n\
            Some(Array(items)) => {\n\
                for item in items {\n\
                    print(json.asString(item))\n\
                }\n\
            }\n\
            _ => print('no tags')\n\
        }\n\
    }\n\
    Err(error) => print(error)\n\
}\n\
\n\
immut built: Json = Object(['list': Array([Int(1), Decimal(2.0), Bool(false), Null]), 'text': String('x')])\n\
print(json.stringify(built))\n\
print(json.parse('[1, 2,'), json.parse('99999999999999999999'))\n\
\n\
match (json.parse('{\"a\": }')) {\n\
    Ok(value) => print(value)\n\
    Err(error) => print('error:', error)\n\
}\n\
", "Object(['name': String('Koji 2'), 'version': Int(1), 'ratio': Decimal(2.5), 'tags': Array([String('fast'), String('safe')]), 'nested': Object(['ok': Bool(true), 'none': Null]), 'big': Decimal(1e300), 'escaped': String('a\"b\\ncé')])\n\
{\"name\":\"Koji 2\",\"version\":1,\"ratio\":2.5,\"tags\":[\"fast\",\"safe\"],\"nested\":{\"ok\":true,\"none\":null},\"big\":1e300,\"escaped\":\"a\\\"b\\ncé\"}\n\
Some(String('Koji 2')) None\n\
Some(1) Some(1) None\n\
Some('fast')\n\
Some('safe')\n\
{\"list\":[1,2.0,false,null],\"text\":\"x\"}\n\
Err(Syntax('unexpected end of input', 1, 7)) Err(NumberOutOfRange('99999999999999999999', 1, 1))\n\
error: invalid JSON at line 1, column 7: unexpected character '}'\n\
");
}

#[test]
fn http_errors() {
    assert_same("http_errors", "import http\n\
\n\
print(http.get('not a url'))\n\
\n\
match (http.get('http://127.0.0.1:1/')) {\n\
    Ok(response) => print(response.status)\n\
    Err(ConnectionFailed(url, _)) => print('cannot connect to', url)\n\
    Err(error) => print(error)\n\
}\n\
", "Err(InvalidUrl('not a url'))\n\
cannot connect to http://127.0.0.1:1/\n\
");
}

#[test]
fn generic_functions_as_values() {
    assert_same("generic_values", "function first[T](items: T[]): Option[T] {\n\
    return if (items.length() > 0) { Some(items[0]) } else { None }\n\
}\n\
\n\
function apply(g: (string[]) => Option[string]): Option[string] {\n\
    return g(['x'])\n\
}\n\
\n\
function biggest[T: orderable](a: T, b: T): T {\n\
    return if (a > b) { a } else { b }\n\
}\n\
\n\
immut f: (int[]) => Option[int] = first\n\
immut pick: (string, string) => string = biggest\n\
immut many: ((int[]) => Option[int])[] = [first, first]\n\
print(f([1, 2]), f([]), apply(first), pick('a', 'b'), many[1]([7]), f)\n\
", "Some(1) None Some('x') b Some(7) <function first>\n\
");
}

#[test]
fn imports_inside_blocks() {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("compiler").join("nested");
    for (name, source) in [
        ("main.kj", "print('start')\n\
\n\
function greet(name: string): string {\n\
    import { shout, uses } from 'tools.kj'\n\
    import math\n\
\n\
    print('uses before', uses, math.floor(2.7))\n\
    return shout(name)\n\
}\n\
\n\
function area(radius: decimal): decimal {\n\
    import math\n\
\n\
    math.pi * radius ** 2\n\
}\n\
\n\
print(greet('ana'))\n\
print(greet('bia'))\n\
\n\
if (true) {\n\
    import tools from 'tools.kj'\n\
    print(tools.uses, area(1))\n\
}\n\
"),
        ("tools.kj", "print('tools loaded')\n\
\n\
export mut uses = 0\n\
\n\
export function shout(text: string): string {\n\
    uses += 1\n\
    return `${text.upper()}!`\n\
}\n\
"),
    ] {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join(name), source).unwrap();
    }
    let main = directory.join("main.kj");
    let main = main.strip_prefix(std::env::current_dir().unwrap()).unwrap().to_path_buf();
    let compiled = compile_and_run(&main);
    assert!(compiled.success, "the program failed: {}", compiled.error);
    assert_eq!(compiled.output, "start\n\
tools loaded\n\
uses before 0 2\n\
ANA!\n\
uses before 1 2\n\
BIA!\n\
2 3.141592653589793238462643383279503\n\
");
}

#[test]
fn examples_that_compile() {
    // Each example prints what `test/examples/<example>.txt` holds (without input).
    for entry in std::fs::read_dir("examples").unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|extension| extension != "kj") {
            continue;
        }
        let example = path.file_stem().unwrap().to_str().unwrap().to_string();
        let expected = std::fs::read_to_string(format!("test/examples/{}.txt", example)).unwrap();
        let executable = Path::new(env!("CARGO_TARGET_TMPDIR")).join(&example);
        build(&path, &executable);
        let result = Command::new(&executable).stdin(std::process::Stdio::null()).output().unwrap();
        assert!(result.status.success(), "{}: {}", example, String::from_utf8_lossy(&result.stderr));
        assert_eq!(String::from_utf8(result.stdout).unwrap(), expected, "{}", example);
    }
}

#[test]
fn kojic_finds_the_runtime_next_to_it() {
    // As `bootstrap/build.sh` leaves them: kojic and libkoji_runtime.a in the same directory, and
    // no KOJI_RUNTIME.
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("next-to-kojic");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::copy(koji::compiler(), directory.join("kojic")).unwrap();
    let path = source_file("next_to_kojic", "print('found it')\n");
    let executable = path.with_extension("");
    let build = |kojic: &Path| {
        koji::finish(Command::new(kojic).env_remove("KOJI_RUNTIME").arg("build").arg(&path).arg("-o").arg(&executable), "")
    };
    let missing = build(&directory.join("kojic"));
    assert_eq!(missing.code, Some(1));
    assert!(missing.stderr.contains("the runtime library (libkoji_runtime.a) is not next to kojic"), "{}", missing.stderr);
    std::fs::copy(koji::RUNTIME, directory.join("libkoji_runtime.a")).unwrap();
    let built = build(&directory.join("kojic"));
    assert_eq!(built.code, Some(0), "{}", built.stderr);
    assert_eq!(String::from_utf8(Command::new(&executable).output().unwrap().stdout).unwrap(), "found it\n");
}

#[test]
fn errors_are_reported_before_compiling() {
    let path = source_file("type_error", "immut n: int = 'text'\n");
    let built = koji::finish(koji::kojic().arg("build").arg(&path).arg("-o").arg(path.with_extension("")), "");
    assert_eq!(built.code, Some(1));
    assert!(built.stderr.starts_with("type error: "), "{}", built.stderr);
}

#[test]
fn assert_shows_the_condition_and_the_two_sides() {
    assert_same("assert_passes", "immut values = [3, 4]\nassert(values.length() == 2)\nassert(values[0] < values[1], 'in order')\nprint('ok')\n", "ok\n");
    let path = source_file("assert_fails", "immut names = ['ana', 'bia']\nassert(names[0] + '!' == names[1], 'the same')\n");
    let failed = koji::finish(koji::kojic().current_dir(path.parent().unwrap()).arg("run").arg(path.file_name().unwrap()), "");
    assert_eq!(failed.code, Some(1));
    assert_eq!(failed.stderr, "panic: assert_fails.kj:2: assertion failed: names[0] + '!' == names[1] (the same)\n  left:  'ana!'\n  right: 'bia'\n");
    assert_same_panic("assert_condition", "immut n = 3\nassert(n > 2 and n % 2 == 0)\n", "assert_condition.kj:2: assertion failed: n > 2 and n % 2 == 0");
}

/// `kojic test`: the tests of each file in a process of their own, the output of a failing one,
/// files without tests left out, and the exit code 1 when a test fails.
#[test]
fn kojic_test_runs_each_test_alone() {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("kojic-test");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(directory.join("sub")).unwrap();
    std::fs::write(
        directory.join("math.kj"),
        "mut calls = 0\n\nfunction gcd(a: int, b: int): int {\n    calls += 1\n    return if (b == 0) { a } else { gcd(b, a % b) }\n}\n\n\
         test 'gcd' {\n    assert(gcd(12, 18) == 6)\n}\n\n\
         test 'each test starts again' {\n    assert(calls == 0)\n}\n\n\
         test 'a wrong result' {\n    print('computing')\n    assert(gcd(4, 6) == 3, 'on purpose')\n}\n\n\
         test 'a panic' {\n    immut items = [1]\n    print(items[2])\n}\n",
    )
    .unwrap();
    std::fs::write(directory.join("sub/plain.kj"), "print('no tests')\n").unwrap();
    std::fs::write(directory.join("sub/top.kj"), "print('top level')\n\ntest 'it\\'s fine' {\n    assert(true)\n}\n").unwrap();
    let finished = koji::finish(koji::kojic().current_dir(&directory).arg("test"), "");
    assert_eq!(finished.code, Some(1), "{}", finished.stderr);
    let (report, time) = finished.stdout.trim_end().rsplit_once(" (").unwrap();
    assert!(time.ends_with(" s)"), "{}", finished.stdout);
    assert_eq!(
        report,
        "ok    ./math.kj: gcd\n\
         ok    ./math.kj: each test starts again\n\
         FAIL  ./math.kj: a wrong result\n    panic: ./math.kj:18: assertion failed: gcd(4, 6) == 3 (on purpose)\n      left:  2\n      right: 3\n    output:\n        computing\n\
         FAIL  ./math.kj: a panic\n    panic: ./math.kj:23: index 2 out of range (length 1)\n\
         ok    ./sub/top.kj: it's fine\n\n\
         3 passed, 2 failed"
    );
    // A file without tests, and a program run by `kojic run`, which leaves its tests out.
    let none = koji::finish(koji::kojic().current_dir(&directory).arg("test").arg("sub/plain.kj"), "");
    assert_eq!((none.code, none.stdout.starts_with("\nno tests found")), (Some(0), true), "{}", none.stdout);
    let run = koji::finish(koji::kojic().current_dir(&directory).arg("run").arg("sub/top.kj"), "");
    assert_eq!(run.stdout, "top level\n");
}

/// A compile error gives the column, then the line with the part at fault marked (the tabs of the
/// line kept, so that the marks fall under it).
#[test]
fn errors_show_the_column_and_the_line() {
    let check = |name: &str, source: &str| {
        let path = source_file(name, source);
        koji::finish(koji::kojic().current_dir(path.parent().unwrap()).arg("check").arg(path.file_name().unwrap()), "").stderr
    };
    assert_eq!(
        check("column_type", "function greet(name: string): string {\n    return name\n}\n\nprint(greet(42))\n"),
        "type error: column_type.kj:5:13: argument 1 of 'greet' must be string, found int\n    5 | print(greet(42))\n      |             ^^\n"
    );
    assert_eq!(
        check("column_tabs", "function f() {\n\tif (true) {\n\t\timmut s: string = 4\n\t}\n}\n"),
        "type error: column_tabs.kj:3:21: cannot assign a value of type int to 's' of type string\n    3 | \t\timmut s: string = 4\n      | \t\t                  ^\n"
    );
    assert_eq!(
        check("column_syntax", "immut x = (1 +\nprint(x)\n"),
        "syntax error: column_syntax.kj:3:1: expected ')', found the end of the file\n    3 | \n      | ^\n"
    );
}

/// The check goes on after an error, statement by statement: every error is reported, and a variable
/// whose declaration failed is declared all the same (with its written type, or unknown), so that
/// its uses are not errors too.
#[test]
fn several_errors_are_reported() {
    let path = source_file(
        "several_errors",
        "immut a: int = 'one'\nimmut b = a + 1\nimmut c = missing * 2\nprint(c.upper(), b)\n\nfunction f(): string {\n    return 3\n}\n",
    );
    let checked = koji::finish(koji::kojic().current_dir(path.parent().unwrap()).arg("check").arg(path.file_name().unwrap()), "");
    assert_eq!(checked.code, Some(1));
    assert_eq!(
        checked.stderr,
        "type error: several_errors.kj:1:16: cannot assign a value of type string to 'a' of type int\n    1 | immut a: int = 'one'\n      |                ^^^^^\n\n\
         type error: several_errors.kj:3:11: undefined variable 'missing'\n    3 | immut c = missing * 2\n      |           ^^^^^^^\n\n\
         type error: several_errors.kj:7:12: returning int, but the function returns string\n    7 |     return 3\n      |            ^\n\n\
         3 errors\n"
    );
}

/// `kojic lsp`: the language server answers on the standard output with the errors of a file, the
/// type under the cursor and the declaration of a name.
#[test]
fn the_language_server() {
    use std::io::{BufRead, BufReader, Read, Write as _};
    use std::process::Stdio;

    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("lsp");
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join("main.kj");
    let uri = format!("file://{}", file.display());
    let mut server = koji::kojic().arg("lsp").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    let mut input = server.stdin.take().unwrap();
    let mut output = BufReader::new(server.stdout.take().unwrap());
    let mut send = |message: String| {
        write!(input, "Content-Length: {}\r\n\r\n{}", message.len(), message).unwrap();
        input.flush().unwrap();
    };
    let mut receive = || {
        let mut length = 0;
        loop {
            let mut line = String::new();
            output.read_line(&mut line).unwrap();
            let line = line.trim();
            if line.is_empty() {
                break;
            }
            if let Some(value) = line.strip_prefix("Content-Length:") {
                length = value.trim().parse().unwrap();
            }
        }
        let mut body = vec![0; length];
        output.read_exact(&mut body).unwrap();
        String::from_utf8(body).unwrap()
    };
    send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#.to_string());
    assert!(receive().contains(r#""hoverProvider":true"#));
    let text = "function twice(n: int): int {\n    return n * 2\n}\n\nimmut count = twice(3)\nimmut wrong: string = count\n";
    send(format!(
        r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","text":{}}}}}}}"#,
        uri,
        format!("{:?}", text)
    ));
    assert_eq!(
        receive(),
        format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{{"uri":"{}","diagnostics":[{{"range":{{"start":{{"line":5,"character":22}},"end":{{"line":5,"character":27}}}},"severity":1,"source":"kojic","message":"type error: cannot assign a value of type int to 'wrong' of type string"}}]}}}}"#,
            uri
        )
    );
    let request = |id: i32, method: &str, line: i32, character: i32| {
        format!(
            r#"{{"jsonrpc":"2.0","id":{},"method":"textDocument/{}","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":{},"character":{}}}}}}}"#,
            id, method, uri, line, character
        )
    };
    // The type of `count` in its declaration, and where `twice` and `n` are declared.
    send(request(2, "hover", 4, 7));
    assert!(receive().contains(r#""value":"```koji\ncount: int\n```""#));
    send(request(3, "definition", 4, 15));
    assert!(receive().contains(r#""range":{"start":{"line":0,"character":0}"#));
    send(request(4, "definition", 1, 11));
    assert!(receive().contains(r#""range":{"start":{"line":0,"character":0}"#));
    send(r#"{"jsonrpc":"2.0","id":5,"method":"shutdown"}"#.to_string());
    assert_eq!(receive(), r#"{"jsonrpc":"2.0","id":5,"result":null}"#);
    send(r#"{"jsonrpc":"2.0","method":"exit"}"#.to_string());
    assert_eq!(server.wait().unwrap().code(), Some(0));
}
