//! Runs the examples from the documentation and checks their output.

use std::cell::RefCell;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Clone, Default)]
struct Output(Rc<RefCell<Vec<u8>>>);

impl Write for Output {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn execute(source: &str, input: &str, path: &Path) -> (String, Result<(), kojic::error::EzError>) {
    let output = Output::default();
    let result = kojic::run_source(
        source,
        path,
        Box::new(output.clone()),
        Box::new(std::io::Cursor::new(input.to_string().into_bytes())),
    );
    let text = String::from_utf8(output.0.borrow().clone()).unwrap();
    (text, result)
}

fn run_with_input(source: &str, input: &str) -> String {
    let (output, result) = execute(source, input, Path::new("test.kj"));
    if let Err(error) = result {
        panic!("program failed: {}\noutput so far:\n{}", error, output);
    }
    output
}

fn run(source: &str) -> String {
    run_with_input(source, "")
}

/// Runs a program that must fail and returns the error message.
fn run_err(source: &str) -> String {
    let (output, result) = execute(source, "", Path::new("test.kj"));
    match result {
        Ok(()) => panic!("program should have failed, output:\n{}", output),
        Err(error) => error.to_string(),
    }
}

fn assert_error(source: &str, fragment: &str) {
    let error = run_err(source);
    assert!(
        error.contains(fragment),
        "expected an error containing {:?}, found {:?}",
        fragment,
        error
    );
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("koji-test-{}-{}", name, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// ---------------------------------------------------------------------------
// Primitive types, variables and comments
// ---------------------------------------------------------------------------

#[test]
fn primitive_types() {
    let source = r#"
        immut age: int = 23
        immut pi: float = 3.14
        immut name: string = 'Mary'
        immut isStudent: boolean = true
        print(age, pi, name, isStudent)
    "#;
    assert_eq!(run(source), "23 3.14 Mary true\n");
}

#[test]
fn option_instead_of_null() {
    let source = r#"
        immut name: Option[string] = None
        mut x: Option[int] = None

        // Later in the code
        x = Some(42)
        print(name, x)
    "#;
    assert_eq!(run(source), "None Some(42)\n");
}

#[test]
fn global_and_local_variables() {
    let source = r#"
        immut globalVar = 1 // Global variable

        function test() {
            mut localVar = 2 // Local variable

            print(globalVar) // 1 (accesses the global variable)
            print(localVar) // 2 (accesses the local variable within the function)
        }

        test()

        print(globalVar) // 1 (global variable accessible outside the function)
    "#;
    assert_eq!(run(source), "1\n2\n1\n");
    assert_error(
        "function test() {\n mut localVar = 2\n}\nprint(localVar)",
        "undefined variable 'localVar'",
    );
}

#[test]
fn functions_can_update_globals() {
    let source = r#"
        mut count = 0
        function increment() {
            count += 1
        }
        increment()
        increment()
        print(count)
    "#;
    assert_eq!(run(source), "2\n");
}

#[test]
fn immut_cannot_change() {
    assert_error("immut pi = 3.14159\npi = 3.14", "cannot modify 'pi': it is immut");
    let source = r#"
        type Person = {
            name: string
            age: int
        }

        mut person: Person = {
            name: 'Ana',
            age: 30
        }

        person.age = 31
        person = { name: 'Bia', age: 25 }
        print(person.name, person.age)
    "#;
    assert_eq!(run(source), "Bia 25\n");
    assert_error(
        "type Person = { name: string }\nimmut person: Person = { name: 'Ana' }\nperson = { name: 'Bia' }",
        "cannot modify 'person': it is immut",
    );
}

#[test]
fn declarations_need_a_keyword() {
    assert_error("count = 1", "'count' is not declared: declare it with 'mut count = ...' (or 'immut' if it never changes)");
    assert_error("mut count = 0\ncuont = 1", "'cuont' is not declared");
    assert_error("name: string = 'Ana'", "declare 'name' with 'mut' or 'immut'");
    assert_error("var x = 1", "Koji has no 'var': declare with 'mut name = ...'");
    assert_error("const x = 1", "Koji has no 'const': declare with 'immut name = ...'");
    assert_error("imut x = 1", "Koji has no 'imut': declare with 'immut name = ...'");
}

#[test]
fn imut_is_deeply_immutable() {
    let source = r#"
        type Person = {
            name: string
            age: int
        }
        immut person: Person = { name: 'Ana', age: 30 }
        immut matrix = [[1, 2], [3, 4]]
        print(person.age, matrix[1][0])
    "#;
    assert_eq!(run(source), "30 3\n");
    assert_error(
        "type P = { age: int }\nimmut person: P = { age: 30 }\nperson.age = 31",
        "cannot modify 'person': it is immut",
    );
    assert_error("immut items = [1, 2]\nitems[0] = 5", "cannot modify 'items': it is immut");
    assert_error("immut matrix = [[1]]\nmatrix[0][0] = 5", "cannot modify 'matrix': it is immut");
    assert_error("immut x = 1\nx = 2", "cannot modify 'x': it is immut");
    assert_error("immut x = 1\nx += 2", "cannot modify 'x': it is immut");
    // const only forbids reassigning the variable.
    assert_eq!(run("mut items = [1, 2]\nitems[0] = 5\nprint(items)"), "[5, 2]\n");
}

#[test]
fn comments() {
    let source = "// Single-line comment\n/*\nMulti-line\ncomment\n*/\nprint('ok') // trailing";
    assert_eq!(run(source), "ok\n");
}

#[test]
fn lists() {
    let source = r#"
        immut numbers: int[] = [1, 2, 3]
        immut coordinates: int[][] = [[1, 2], [3, 4]]
        immut positions: int[][][] = [[[1, 2], [3, 4]], [[5, 6], [7, 8]]]
        print(numbers, coordinates[1][0], positions[1][1][1], numbers.length())
    "#;
    assert_eq!(run(source), "[1, 2, 3] 3 8 3\n");
    assert_error("immut numbers: int[] = [1, 'a']", "same type");
}

// ---------------------------------------------------------------------------
// Operators
// ---------------------------------------------------------------------------

#[test]
fn arithmetic_operators() {
    assert_eq!(
        run("print(1 + 2, 5 - 7, 3 * 4, 2 ** 10, 7 % 3, 2.5 * 2)"),
        "3 -2 12 1024 1 5.0\n"
    );
}

#[test]
fn division_follows_the_operand_types() {
    // Like Go and Rust: ints give a truncated int, floats give a float.
    assert_eq!(
        run("print(7 / 2, -7 / 2, 7.0 / 2.0, 7 / 2.0, -7 % 2, 7.5 % 2.0)"),
        "3 -3 3.5 3.5 -1 1.5\n"
    );
    assert_eq!(run("mut x: int = 17\nx /= 5\nprint(x)"), "3\n");
    assert_error("immut x: int = 7 / 2.0", "cannot assign a value of type float");
    assert_error("print(1 / 0)", "division by zero");
}

#[test]
fn double_slash_is_always_a_comment() {
    assert_eq!(run("immut x = 7 //2\nprint(x)"), "7\n");
}

#[test]
fn comparison_operators() {
    assert_eq!(
        run("print(1 == 1, 1 != 2, 3 > 2, 2 < 3, 3 >= 3, 2 <= 1, 'a' < 'b')"),
        "true true true true true false true\n"
    );
}

#[test]
fn equality_and_membership() {
    let source = r#"
        type Person = {
            name: string
            age: int
        }
        immut a: Person = { name: 'Ana', age: 30 }
        immut b: Person = { name: 'Ana', age: 30 }

        print(a == b) // true: the fields are equal
        print('a' in 'banana')
        print(2 in [1, 2, 3], 'x' in ['a'])
    "#;
    assert_eq!(run(source), "true\ntrue\ntrue false\n");
    // Values have no identity: `is` only checks variants.
    assert_error("immut a = [1]\nimmut b = a\nprint(a is b)", "'is' checks a variant");
    assert_error("immut x = 1\nprint(x is 1)", "'is' checks a variant");
}

#[test]
fn logical_operators() {
    let source = r#"
        immut age = 20
        if not (age > 18) { print('minor') } else { print('adult') }
        print(true and false, true or false, not true)
    "#;
    assert_eq!(run(source), "adult\nfalse true false\n");
}

#[test]
fn assignment_operators() {
    let source = r#"
        mut e = 10
        e += 1
        e -= 2
        e *= 3
        e /= 2
        e **= 2
        e %= 7
        mut f = 1.0
        f /= 4
        print(e, f)
    "#;
    assert_eq!(run(source), "1 0.25\n");
}

#[test]
fn string_operators() {
    assert_eq!(run("print('ab' + 'cd', 'ab' * 3)"), "abcd ababab\n");
}

#[test]
fn bitwise_operators() {
    let source = "immut a = 5\nimmut b = 3\nprint(a & b, a | b, a ^ b, ~a, a << 1, a >> 1)";
    assert_eq!(run(source), "1 7 6 -6 10 2\n");
}

#[test]
fn operator_type_errors() {
    assert_error("print(1 + 'a')", "cannot be applied to int and string");
    assert_error("print(true + 1)", "cannot be applied");
}

// ---------------------------------------------------------------------------
// Strings and conversions
// ---------------------------------------------------------------------------

#[test]
fn string_interpolation() {
    let source = r#"
        immut name: string = 'Emma'

        print(`Hello, ${name}!`) // Hello, Emma!
        print('Hello, ${name}!') // Hello, ${name}!
        print("Hello, ${name}!") // Hello, ${name}!
        print(`1 + 1 = ${1 + 1}`)
    "#;
    assert_eq!(
        run(source),
        "Hello, Emma!\nHello, ${name}!\nHello, ${name}!\n1 + 1 = 2\n"
    );
}

#[test]
fn conversion_methods() {
    assert_eq!(
        run("print(int('0'), int('abc'), float('0'), string(0), int(3.9), float(2))"),
        "Ok(0) Err(InvalidNumber('abc')) Ok(0.0) 0 3 2.0\n"
    );
    assert_eq!(
        run("print(int('99999999999999999999'), float('1e400'), float('nan'), float('inf'))"),
        "Err(OutOfRange('99999999999999999999')) Err(OutOfRange('1e400')) Err(InvalidNumber('nan')) Err(InvalidNumber('inf'))\n"
    );
    let source = r#"
        match (int('42')) {
            Ok(value) => print(value + 1)
            Err(InvalidNumber(text)) => print(`not a number: ${text}`)
            Err(OutOfRange(text)) => print(`too large: ${text}`)
        }
    "#;
    assert_eq!(run(source), "43\n");
}

#[test]
fn string_methods() {
    let source = r#"
        print('abc'.length(), '  hi  '.trim(), 'hello'.upper(), 'HELLO'.lower())
        print('   '.isSpace(), '123'.isNumeric(), 'abc'.isAlpha(), 'abc123'.isAlphaNumeric())
        print('ABC'.isUpper(), 'abc'.isLower(), 'Hello World'.isTitle(), 'Hello world'.isTitle(), '123'.isTitle())
        print('a b'.split(' '), 'hello'.replace('hello', 'hi'))
    "#;
    assert_eq!(
        run(source),
        "3 hi HELLO hello\ntrue true true true\ntrue true true false false\n['a', 'b'] hi\n"
    );
}

// ---------------------------------------------------------------------------
// Built-ins
// ---------------------------------------------------------------------------

#[test]
fn input_returns_an_option() {
    let source = r#"
        immut name: Option[string] = input("What's your name?")

        match (name) {
            Some(line) => print(`Hello, ${line}!`)
            None => print('No input')
        }
    "#;
    assert_eq!(run_with_input(source, "Ana\n"), "What's your name?Hello, Ana!\n");
    assert_eq!(run_with_input(source, ""), "What's your name?No input\n");
}

// ---------------------------------------------------------------------------
// Control flow
// ---------------------------------------------------------------------------

#[test]
fn if_else_statement_and_expression() {
    let source = r#"
        immut age = 20
        if (age > 18) {
            print('Adult')
        } else {
            print('Minor')
        }

        immut label: string = if (age > 18) { 'Adult' } else { 'Minor' }

        print(label)
    "#;
    assert_eq!(run(source), "Adult\nAdult\n");
    assert_error("immut x = if (true) { 1 }", "no value");
    assert_error("if (1) { print(1) }", "must be boolean");
    assert_error("if (true) { } else if (false) { }", "does not have 'else if'");
}

#[test]
fn for_loops() {
    let source = r#"
        print(range(0, 5))
        for i in range(0, 3) { print(i) }
        for i in range(2) { print(i) }
        for i in range(10, 0, -4) { print(i) }

        immut names: string[] = ['Anna', 'Claire', 'Leon']

        // By index
        for i in range(0, names.length()) {
            print(names[i])
        }

        // Direct iteration
        for name in names {
            print(name)
        }
    "#;
    assert_eq!(
        run(source),
        "[0, 1, 2, 3, 4]\n0\n1\n2\n0\n1\n10\n6\n2\nAnna\nClaire\nLeon\nAnna\nClaire\nLeon\n"
    );
}

#[test]
fn while_break_and_continue() {
    let source = r#"
        mut count = 0
        while (count < 3) {
            print(count)

            count += 1
        }

        for i in range(0, 10) {
            if (i == 5) {
                break
            }

            print(i)
        }

        for i in range(0, 10) {
            if (i % 2 == 0) {
                continue
            }

            print(i)
        }
    "#;
    assert_eq!(run(source), "0\n1\n2\n0\n1\n2\n3\n4\n1\n3\n5\n7\n9\n");
    assert_error("break", "can only be used inside a loop");
    assert_error("continue", "can only be used inside a loop");
}

#[test]
fn match_with_guards() {
    let source = r#"
        function classify(age: int): string {
            match (age) {
                x if (x > 18) => 'Adult'
                x if (x > 12) => 'Teen'
                _             => 'Minor'
            }
        }
        immut age: int = 14

        immut label: string = match (age) {
            x if (x > 18) => 'Adult'
            x if (x > 12) => 'Teen'
            _             => 'Minor'
        }

        print(label) // Teen
        print(classify(30), classify(5))
    "#;
    assert_eq!(run(source), "Teen\nAdult Minor\n");
}

#[test]
fn match_arms_must_agree_with_the_declared_type() {
    let source = r#"
        immut result = Ok(1)
        immut label: int = match (result) {
            Ok(value) => `Value: ${value}` // Error! string is not int
            Err(error) => `Error: ${error}`
        }
    "#;
    assert_error(source, "has type string, expected int");
}

#[test]
fn match_must_be_exhaustive() {
    assert_error(
        "immut option = Some(1)\nmatch (option) {\n Some(value) => print(value)\n}",
        "None is not handled",
    );
    assert_error("match (1) {\n 1 => print(1)\n}", "wildcard");
    let source = r#"
        immut option = Some(1)
        match (option) {
            Some(value) => print(value)
            _ => print('No value')
        }
    "#;
    assert_eq!(run(source), "1\n");
}

#[test]
fn match_arms_on_separate_lines() {
    let source = r#"
        function sign(n: int): string {
            match (n) {
                0 => 'zero'
                -1 => 'minus one'
                x if (x > 0) => 'positive'
                _ => 'negative'
            }
        }
        print(sign(0), sign(-1), sign(5), sign(-9))
    "#;
    assert_eq!(run(source), "zero minus one positive negative\n");
}

#[test]
fn match_literals_and_nested_patterns() {
    let source = r#"
        type Shape = {
            Circle(radius: float)
            Rectangle(width: float, height: float)
        }
        immut shapes: Option[Shape][] = [Some(Circle(2.0)), None, Some(Rectangle(1.0, 2.0))]
        for s in shapes {
            match (s) {
                Some(Circle(r)) => print(`circle ${r}`)
                Some(Rectangle(w, h)) => print(`rect ${w}x${h}`)
                None => print('nothing')
            }
        }
        match ('b') {
            'a' => print('A')
            'b' => print('B')
            _ => print('?')
        }
    "#;
    assert_eq!(run(source), "circle 2.0\nnothing\nrect 1.0x2.0\nB\n");
}

// ---------------------------------------------------------------------------
// Sum types and custom types
// ---------------------------------------------------------------------------

#[test]
fn sum_types() {
    let source = r#"
        type Result = {
            Ok(value: string)
            Err(message: string)
        }

        function handleResult(result: Result) {
            match (result) {
                Ok(value) => print(`Success: ${value}`)
                Err(message) => print(`Error: ${message}`)
            }
        }

        handleResult(Ok('saved'))
        handleResult(Err('disk full'))
    "#;
    assert_eq!(run(source), "Success: saved\nError: disk full\n");
}

#[test]
fn sum_type_destructuring() {
    let source = r#"
        type Shape = {
            Circle(radius: float)
            Rectangle(width: float, height: float)
        }

        function area(shape: Shape): float {
            match (shape) {
                Circle(radius) => 3.14 * radius * radius
                Rectangle(width, height) => width * height
            }
        }

        print(area(Circle(1.0)), area(Rectangle(2.0, 3.0)))
    "#;
    assert_eq!(run(source), "3.14 6.0\n");
}

#[test]
fn unit_variants() {
    let source = r#"
        type User = {
            Admin(name: string)
            Guest
        }
        immut users: User[] = [Admin('Ana'), Guest]
        for user in users {
            match (user) {
                Admin(name) => print(`Admin: ${name}`)
                Guest => print('Guest user')
            }
        }
    "#;
    assert_eq!(run(source), "Admin: Ana\nGuest user\n");
}

#[test]
fn recursive_sum_types() {
    let source = r#"
        type List = {
            Cons(head: int, tail: List)
            Nil
        }
        function total(list: List): int {
            match (list) {
                Cons(head, tail) => head + total(tail)
                Nil => 0
            }
        }
        print(total(Cons(1, Cons(2, Cons(3, Nil)))))
    "#;
    assert_eq!(run(source), "6\n");
}

#[test]
fn shape_types() {
    let source = r#"
        type Person = {
            name: string
            age: int
        }

        immut person: Person = {
            name: 'John',
            age: 20
        }
        print(person.name, person.age, person)
    "#;
    assert_eq!(run(source), "John 20 { name: 'John', age: 20 }\n");
    assert_error("type P = { a: int }\nimmut p: P = { a: 1, b: 2 }", "has no field 'b'");
    assert_error("type P = { a: int  b: int }\nimmut p: P = { a: 1 }", "missing field(s) b");
    assert_error("type P = { a: int }\nimmut p: P = { a: 'x' }", "must be int");
}

#[test]
fn maps_and_sets() {
    let source = r#"
        type Dictionary = Map[string, string]

        mut translations: Dictionary = [
            'hello': 'olá',
            'world': 'mundo'
        ]
        translations['bye'] = 'tchau'
        print(translations['hello'], 'world' in translations, translations.length())
        for key in translations { print(key) }

        immut ages: Map[string, int] = ['Ana': 30, 'Bia': 25]
        mut copy = ages
        copy['Ana'] += 1
        print(ages['Ana'], copy['Ana'], ages.get('Bia'), ages.get('X'), ages.has('Ana'))
        print(copy.remove('Bia'), copy.remove('X'), copy, copy.keys(), copy.values())
        immut empty: Map[int, string] = [:]
        print(empty, [1: 'um'])

        type Point = {
            x: int
            y: int
        }
        mut seen: Set[Point] = Set.from([])
        seen.add({ x: 1, y: 2 })
        seen.add({ x: 1, y: 2 })
        immut p: Point = { x: 1, y: 2 }
        print(seen.length(), seen.has(p), p in seen)
        immut s = Set.from([3, 1, 3, 2])
        mut t = s
        print(t.remove(3), t.remove(9))
        print(s, t, s.values(), s == Set.from([1, 2, 3]))

        mut counts: Map[char, int] = [:]
        for c in 'banana' {
            if (c in counts) {
                counts[c] += 1
            } else {
                counts[c] = 1
            }
        }
        print(counts)
    "#;
    assert_eq!(
        run(source),
        "olá true 3\nhello\nworld\nbye\n30 31 Some(25) None true\n\
         Some(25) None ['Ana': 31] ['Ana'] [31]\n[:] [1: 'um']\n1 true true\ntrue false\n\
         Set([3, 1, 2]) Set([1, 2]) [3, 1, 2] true\n['b': 1, 'a': 3, 'n': 2]\n"
    );
    let errors = [
        ("type D = { [key: string]: string }", "'D' cannot be an index signature: use a map"),
        ("immut d: Map[string, int] = { 'a': 1 }", "a map is written with brackets, not braces"),
        ("immut m = ['a': 1, 'b': 'x']", "map values must have the same type"),
        ("immut m = [(n: int) => n: 1]", "map keys must be equatable"),
        ("immut m = ['a': 1]\nm.remove('a')", "cannot call 'remove' on 'm', as it changes the value: it is immut"),
        ("immut m = ['a': 1, 2]", "a map literal needs 'key: value' for every entry"),
        ("immut s: Set[(int) => int] = Set.from([])", "is not equatable"),
    ];
    for (code, fragment) in errors {
        assert_error(code, fragment);
    }
}

#[test]
fn untagged_unions_are_rejected() {
    assert_error("type Result = string | int", "untagged union");
}

#[test]
fn type_intersection_and_no_subtyping() {
    let types = r#"
        type Animal = {
            name: string
        }

        type Dog = Animal & {
            breed: string
        }

        function greet(animal: Animal) {
            print(animal.name)
        }

        immut dog: Dog = {
            name: 'Rex',
            breed: 'Pug'
        }
    "#;
    let ok = format!("{}\ngreet({{ name: dog.name }})\nprint(dog.breed)", types);
    assert_eq!(run(&ok), "Rex\nPug\n");
    let error = format!("{}\ngreet(dog)", types);
    assert_error(&error, "Dog is not a subtype of Animal");
}

// ---------------------------------------------------------------------------
// Nested shapes and nominal typing
// ---------------------------------------------------------------------------

#[test]
fn nested_shapes() {
    let source = r#"
        type Address = {
            street: string
            city: string
        }

        type User = {
            id: int
            name: string
            address: Address
        }

        immut user: User = {
            id: 42
            name: 'Bob'
            address: {
                street: '123 Main St'
                city: 'Exampleville'
            }
        }

        type Product = {
            id: int
            name: string
            price: float
        }

        function printProduct(product: Product) {
            print(`Product: ${product.name} (${product.id}) - ${product.price}`)
        }

        print(user.address.city)
        printProduct({ id: 1, name: 'Pen', price: 2.5 })
    "#;
    assert_eq!(run(source), "Exampleville\nProduct: Pen (1) - 2.5\n");
    assert_error("type U = { id: int }\nimmut u: U = { id: 1, extra: true }", "has no field 'extra'");
}

#[test]
fn shapes_are_nominal() {
    // Two types with the same fields are still different types.
    let source = r#"
        type Celsius = { value: float }
        type Fahrenheit = { value: float }
        immut c: Celsius = { value: 20.0 }
        immut f: Fahrenheit = c
    "#;
    assert_error(source, "cannot assign a value of type Celsius to 'f' of type Fahrenheit");
}

#[test]
fn interface_is_reserved() {
    assert_error("interface User { id: int }", "'interface' is a reserved word");
    assert_error("immut interface = 1", "'interface' is a reserved word");
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

#[test]
fn functions() {
    let source = r#"
        function sum(a: int, b: int): int {
            return a + b
        }

        print(sum(2, 2)) // 4

        function factorial(x: int): int {
            if (x <= 1) { return 1 }

            return x * factorial(x - 1)
        }

        print(factorial(4)) // 24
    "#;
    assert_eq!(run(source), "4\n24\n");
    assert_error("function f(a: int): int { return a }\nf('x')", "must be int");
    assert_error("function f(): int {\n print(1)\n}", "must return a value of type int");
    assert_error("function f() { return 1 }", "cannot return a value");
}

#[test]
fn anonymous_functions_and_closures() {
    let source = r#"
        immut double = (x: int): int => {
            return x * 2
        }

        // or simply

        immut triple = (x: int) => x * 3

        print(double(10), triple(10)) // 20 30

        function makeAdder(amount: int): (int) => int {
            return (x: int) => x + amount
        }
        immut addFive = makeAdder(5)
        print(addFive(1))

        immut apply = (f: (int) => int, x: int) => f(x)
        print(apply(double, 21))
    "#;
    assert_eq!(run(source), "20 30\n6\n42\n");
}

#[test]
fn closures_capture_a_read_only_copy() {
    // The captured value is the one at the moment the function is created.
    let source = r#"
        function make(): int[] {
            mut results: int[] = []
            mut x = 1
            immut read = () => x
            x = 2
            results += [read()]

            mut fns: (() => int)[] = []
            for i in range(3) {
                fns += [() => i * 10]
            }
            for f in fns {
                results += [f()]
            }

            function inner(): int { return x }
            x = 3
            results += [inner()]
            return results
        }
        print(make())

        // Module variables are not captured: functions read and change them.
        mut total = 0
        function add(n: int) {
            total += n
        }
        add(2)
        add(3)
        print(total)
    "#;
    assert_eq!(run(source), "[1, 0, 10, 20, 2]\n5\n");
    let counter = r#"
        function makeCounter(): () => int {
            mut count = 0
            return () => {
                count += 1
                return count
            }
        }
    "#;
    assert_error(counter, "cannot modify 'count' here: functions capture a read-only copy");
    assert_error(
        "function f() {\n mut items = [1]\n immut g = () => { items[0] = 2 }\n}",
        "cannot modify 'items' here",
    );
}

#[test]
fn functions_are_hoisted() {
    assert_eq!(run("print(later(1))\nfunction later(x: int): int { return x + 1 }"), "2\n");
}

// ---------------------------------------------------------------------------
// Error propagation
// ---------------------------------------------------------------------------

#[test]
fn question_mark_operator() {
    let source = r#"
        function parseAge(text: string): Result[int, ParseError] {
            immut age = int(text)?
            return Ok(age)
        }
        print(parseAge('42'), parseAge('x'))

        function first(items: string[]): Option[string] {
            if (items.length() == 0) { return None }
            return Some(items[0])
        }
        function shout(items: string[]): Option[string] {
            immut item = first(items)?
            return Some(item.upper())
        }
        print(shout(['hey']), shout([]))
    "#;
    assert_eq!(run(source), "Ok(42) Err(InvalidNumber('x'))\nSome('HEY') None\n");
}

#[test]
fn question_mark_wraps_errors_automatically() {
    let source = r#"
        import json

        type AppError = {
            Parse(error: ParseError)
            Json(error: json.JsonError)
            Message(text: string)
        }

        function load(text: string, count: string): Result[int, AppError] {
            json.parse(text)?
            immut n = int(count)?
            return Ok(n)
        }
        print(load('[1]', '3'), load('[1', '3'), load('[1]', 'x'))
    "#;
    assert_eq!(
        run(source),
        "Ok(3) Err(Json(Syntax('expected \\',\\' or \\']\\'', 1, 3))) Err(Parse(InvalidNumber('x')))\n"
    );
    assert_error(
        "function f(): Result[int, string] {\n immut x = int('1')?\n return Ok(x)\n}",
        "use '.mapErr(...)'",
    );
    assert_error(
        "type E = { A(e: ParseError)  B(e: ParseError) }\nfunction f(): Result[int, E] {\n immut x = int('1')?\n return Ok(x)\n}",
        "the variants A, B all hold a ParseError",
    );
}

#[test]
fn readable_error_messages() {
    let source = r#"
        import file
        import json
        import math

        match (file.open('/nonexistent/notes.txt', file.Read)) {
            Ok(_) => print('opened?')
            Err(error) => {
                print(error)
                print(`failed: ${error}`, error.display().length() > 0)
            }
        }
        match (int('abc')) {
            Ok(_) => print('?')
            Err(error) => print(error)
        }
        match (json.parse('[1,')) {
            Ok(_) => print('?')
            Err(error) => print(error)
        }
        match (math.sqrt(-1)) {
            Ok(_) => print('?')
            Err(error) => print(error.display(), '|', error.debug())
        }
        print(int('abc')) // a Result keeps its structural form
    "#;
    assert_eq!(
        run(source),
        "file '/nonexistent/notes.txt' not found\nfailed: file '/nonexistent/notes.txt' not found true\n'abc' is not a valid number\ninvalid JSON at line 1, column 4: unexpected end of input\nthe result is not a real number (it is complex) | Complex\nErr(InvalidNumber('abc'))\n"
    );
    assert_error("print(int('a').message())", "has no method 'message'");
}

#[test]
fn map_err() {
    let source = r#"
        function parse(text: string): Result[int, string] {
            immut n = int(text).mapErr((error: ParseError) => `bad number: ${text}`)?
            return Ok(n)
        }
        type Wrapped = { Wrap(error: ParseError) }
        print(parse('1'), parse('x'), int('y').mapErr(Wrap))
    "#;
    assert_eq!(
        run(source),
        "Ok(1) Err('bad number: x') Err(Wrap(InvalidNumber('y')))\n"
    );
    assert_error("immut x = int('1').mapErr(5)", "'mapErr' expects a function");
}

#[test]
fn question_mark_rules() {
    assert_error("function f(): int {\n return int('1')?\n}", "needs the function to return a Result too");
    assert_error("immut x = 5?", "can only be used on a Result or an Option");
    assert_error("immut g = (s: string) => int(s)?", "needs the function to declare its return type");
    // At the top level of the main file, an error stops the program.
    let (output, result) = execute("print(int('1')?)\nimmut n = int('oops')?\nprint('never')", "", Path::new("test.kj"));
    assert_eq!(output, "1\n");
    assert_eq!(result.unwrap_err().to_string(), "error: test.kj:2: 'oops' is not a valid number");
}

#[test]
fn imports_are_hoisted() {
    let source = r#"
        type Failure = { Bad(error: math.MathError) }
        print(root(16.0))
        function root(x: float): Result[float, Failure] {
            return Ok(math.sqrt(x)?)
        }
        import math
    "#;
    assert_eq!(run(source), "Ok(4.0)\n");
}

// ---------------------------------------------------------------------------
// Modules
// ---------------------------------------------------------------------------

#[test]
fn modules_and_imports() {
    let dir = temp_dir("modules");
    std::fs::write(
        dir.join("math.kj"),
        r#"
        // file: math.kj

        export function add(a: int, b: int): int {
            return a + b
        }

        export function subtract(a: int, b: int): int {
            return a - b
        }

        function clamp(x: int): int {
            return x
        }

        export type Shape = {
            Circle(radius: float)
            Square(side: float)
        }

        export mut count = 0

        export immut origin = [0, 0]

        export function increment() {
            count += 1
        }
        "#,
    )
    .unwrap();
    let source = r#"
        // file: main.kj

        import math from 'math.kj'
        import { add, subtract, Shape, count, increment } from 'math.kj'

        immut result: int = math.add(5, 10)

        print(result) // 15
        print(add(5, 10), subtract(5, 10))
        immut s: Shape = Circle(1.0)
        immut t: math.Shape = math.Square(2.0)
        print(s, t)

        increment()
        math.increment()
        print(count, math.count) // always the current value
    "#;
    let (output, result) = execute(source, "", &dir.join("main.kj"));
    result.unwrap();
    assert_eq!(output, "15\n15 -5\nCircle(1.0) Square(2.0)\n2 2\n");

    let fails = |source: &str, fragment: &str| {
        let (_, result) = execute(source, "", &dir.join("main.kj"));
        let error = result.unwrap_err().to_string();
        assert!(error.contains(fragment), "expected {:?} in {:?}", fragment, error);
    };
    fails("import { nope } from 'math.kj'", "has no member 'nope'");
    fails("import { clamp } from 'math.kj'", "'clamp' is not exported by 'math.kj'");
    fails("import math from 'math.kj'\nprint(math.clamp(1))", "'clamp' is not exported by 'math.kj'");
    fails("import { count } from 'math.kj'\ncount = 5", "'count' is read-only outside 'math.kj'");
    fails("import math from 'math.kj'\nmath.count = 5", "'count' is read-only outside 'math.kj'");
    fails("import { origin } from 'math.kj'\norigin[0] = 1", "'origin' is read-only outside 'math.kj'");
    fails("import { count } from 'math.kj'\ncount += 1", "'count' is read-only outside 'math.kj'");
    fails("import math from 'math.kj'\nmath.origin[0] = 1", "'origin' is read-only outside 'math.kj'");
}

#[test]
fn export_rules() {
    assert_error("function f() {\n export immut x = 1\n}", "only allowed at the top level");
    assert_error("export x = 1", "expected 'function', 'mut', 'immut' or 'type' after 'export'");
    assert_error(
        "type Hidden = { value: int }\nexport function make(): Hidden { return { value: 1 } }",
        "uses the private type 'Hidden'",
    );
    // Exports of the main file are allowed (and simply unused).
    assert_eq!(run("export immut answer = 42\nprint(answer)"), "42\n");
}

// ---------------------------------------------------------------------------
// Standard library
// ---------------------------------------------------------------------------

#[test]
fn stdlib_time() {
    let source = r#"
        import time

        print('Waiting...')

        time.sleep(0.01)

        print('Done!')

        immut start = time.monotonic()
        // Do something
        immut end = time.monotonic()

        print(end - start >= 0.0, time.now() > 1700000000.0)
    "#;
    assert_eq!(run(source), "Waiting...\nDone!\ntrue true\n");
}

#[test]
fn stdlib_file() {
    let dir = temp_dir("file");
    let path = dir.join("notes.txt").display().to_string().replace('\\', "/");
    let source = format!(
        r#"
        import file

        match (file.open('{path}', file.Write)) {{
            Ok(f) => {{
                print(f.write('Hello world!'))
                f.close()
            }}
            Err(error) => print(error)
        }}

        match (file.open('{path}', file.Read)) {{
            Ok(f) => {{
                match (f.read()) {{
                    Ok(content) => print(content)
                    Err(error) => print(error)
                }}
                f.close()
            }}
            Err(error) => print(error)
        }}

        match (file.open('/nonexistent/notes.txt', file.Read)) {{
            Ok(_) => print('opened?')
            Err(NotFound(missing)) => print(`not found: ${{missing}}`)
            Err(error) => print(error)
        }}

        import {{ Mode }} from 'file'
        match (file.open('{path}', Append)) {{
            Ok(f) => f.write(' Bye!')
            Err(error) => Err(error)
        }}
        match (file.open('{path}', Read)) {{
            Ok(f) => print(f.read())
            Err(error) => print(error)
        }}
        "#
    );
    assert_eq!(
        run(&source),
        "Ok(void)\nHello world!\nnot found: /nonexistent/notes.txt\nOk('Hello world! Bye!')\n"
    );
    assert_error("import file\nimmut f = file.open('x', 'r')", "must be Mode, found string");
}

#[test]
fn stdlib_datetime() {
    let source = r#"
        import datetime

        immut date = datetime.today()

        print(date.length())

        match (datetime.parse('2025-04-20', 'YYYY-MM-DD')) {
            Ok(timestamp) => {
                print(timestamp)
                print(datetime.format(timestamp, 'DD/MM/YYYY'))
                print(datetime.format(timestamp, 'yyyy-mm-dd hh:min:ss'))
                print(datetime.format(timestamp + 3661, 'yyyy-mm-dd hh:min:ss'))
            }
            Err(error) => print(error)
        }
        print(datetime.parse('2025-02-30', 'YYYY-MM-DD'), datetime.parse('20/04', 'YYYY-MM-DD'))
        print(datetime.parse('2025/02/03', 'YYYY-MM-DD'))

        immut now = datetime.now()
        print(now.year >= 2025, now.month >= 1 and now.month <= 12)
    "#;
    assert_eq!(
        run(source),
        "10\n1745107200\n20/04/2025\n2025-04-20 00:00:00\n2025-04-20 01:01:01\n\
         Err(InvalidDate('2025-02-30')) Err(PatternMismatch('20/04', 'YYYY-MM-DD'))\n\
         Err(PatternMismatch('2025/02/03', 'YYYY-MM-DD'))\ntrue true\n"
    );
}

#[test]
fn stdlib_math() {
    let source = r#"
        import math

        match (math.pow(5, 2)) {
            Ok(squared) => print(math.pi * squared)
            Err(error) => print(error)
        }

        match (math.sqrt(-1)) {
            Ok(root) => print(root)
            Err(Complex) => print('no real square root')
            Err(Indeterminate) => print('indeterminate')
            Err(Infinite) => print('infinite')
        }
        print(math.sqrt(16), math.pow(-8, 0.5), math.pow(0, 0), math.pow(0, -1), math.pow(-2, 3))
        print(math.abs(-3), math.abs(-2.5), math.floor(2.7), math.ceil(2.1), math.round(2.5))
        print(math.e > 2.71)
    "#;
    assert_eq!(
        run(source),
        "78.53981633974483\nno real square root\nOk(4.0) Err(Complex) Ok(1.0) Err(Infinite) Ok(-8.0)\n3 2.5 2 3 3\ntrue\n"
    );
    // Representation limits panic: a larger type would fit the result, unlike Infinite.
    assert_error("import math\nprint(math.pow(10, 400))", "float overflow");
    assert_error("import math\nprint(math.round(1e300))", "integer overflow");
}

#[test]
fn floats_have_no_nan_or_infinity() {
    assert_error("immut x = 1e308 * 10.0", "float overflow");
    assert_error("immut x = 1.5 / 0.0", "division by zero");
    assert_error("immut x = (-8.0) ** 0.5", "not a real number");
    assert_error("immut x = 0.0 ** -1.0", "division by zero");
    assert_error("immut x = 1e400", "float literal is too large");
    assert_eq!(run("print(2.0 ** 0.5 > 1.41, 2 ** 3, (-8.0) ** 2.0)"), "true 8 64.0\n");
    // 0 ** 0 is 1: the value of the expression (the indeterminate form only concerns limits).
    assert_eq!(run("print(0 ** 0, 0.0 ** 0.0)"), "1 1.0\n");
    assert_eq!(run("print(6.022e23, 1e15, 2.5e-3)"), "6.022e23 1000000000000000.0 0.0025\n");
}

#[test]
fn panic_builtin() {
    assert_error("panic('impossible state')", "panic: test.kj:1: impossible state");
    let source = r#"
        function check(x: int): int {
            if (x > 0) { return x }
            panic('x must be positive')
        }
        print(check(2))
    "#;
    assert_eq!(run(source), "2\n");
}

#[test]
fn stdlib_json() {
    let source = r#"
        import json

        match (json.parse('{"name": "koji", "year": 2025, "tags": ["a"], "ok": true, "x": null, "pi": 3.14}')) {
            Ok(Object(fields)) => {
                match (fields['name']) {
                    String(name) => print(name) // koji
                    _ => print('name is not a string')
                }
                match (fields['x']) {
                    Null => print('null')
                    _ => print('not null')
                }
                print(fields['year'], fields['tags'], fields['pi'])
            }
            Ok(_) => print('expected an object')
            Err(error) => print(`invalid JSON: ${error}`)
        }

        match (json.parse('{"year": 2025}')) {
            Ok(data) => {
                print(json.get(data, 'year'), json.get(data, 'nope'))
                match (json.get(data, 'year')) {
                    Some(year) => print(json.asInt(year), json.asFloat(year), json.asString(year))
                    None => print('no year')
                }
                print(json.stringify(data))
            }
            Err(error) => print(error)
        }

        print(json.stringify(json.Array([json.Float(2.0), json.Int(2), json.String('a'), json.Null, json.Bool(false), json.Float(6.022e23)])))
        // parse and stringify give back the same text; floats keep their decimal point
        for text in ['{"name":"koji","year":2025,"tags":[1,2.5,null]}', '[2.0, 1e3]'] {
            match (json.parse(text)) {
                Ok(value) => print(json.stringify(value))
                Err(error) => print(error)
            }
        }
        print(json.parse('{"a": }'))
        print(json.parse('[1,\n 99999999999999999999]'))
    "#;
    assert_eq!(
        run(source),
        "koji\nnull\nInt(2025) Array([String('a')]) Float(3.14)\nSome(Int(2025)) None\nSome(2025) Some(2025.0) None\n{\"year\":2025}\n[2.0,2,\"a\",null,false,6.022e23]\n\
         {\"name\":\"koji\",\"year\":2025,\"tags\":[1,2.5,null]}\n[2.0,1000.0]\nErr(Syntax('unexpected character \\'}\\'', 1, 7))\nErr(NumberOutOfRange('99999999999999999999', 2, 2))\n"
    );
    assert_error("import json\nprint(json.stringify(1))", "must be Json");
}

#[test]
fn stdlib_regex() {
    let source = r#"
        import regex

        match (regex.compile('[0-9]+')) {
            Ok(digits) => {
                print(digits.test('abc123'), digits.test('abc'))
                match (digits.find('Order number: 12345')) {
                    Some(found) => print(found.value, found.start, found.end)
                    None => print('no number')
                }
                for found in digits.findAll('a1b22c333') {
                    print(found.value)
                }
                print(digits.replace('a1b2', '#'), digits.replaceAll('a1b2', '#'))
                print(digits.split('a1b22c'))
                print(digits.find('abc'))
            }
            Err(error) => print(error)
        }

        match (regex.compile('(abc')) {
            Ok(_) => print('valid?')
            Err(error) => print(error, '|', error.debug())
        }

        function countWords(text: string): Result[int, regex.RegexError] {
            immut word = regex.compile('[a-z]+')?
            return Ok(word.findAll(text).length())
        }

        print(countWords('hello big world'))
    "#;
    assert_eq!(
        run(source),
        "true false\n12345 14 19\n1\n22\n333\na#b2 a#b#\n['a', 'b', 'c']\nNone\n\
         invalid regular expression '(abc': unclosed group | InvalidPattern('(abc', 'unclosed group')\nOk(3)\n"
    );
    assert_error("import regex\nprint(regex.match('a', 'a'))", "has no member 'match'");
    let source = r#"
        import regex

        print(regex.compile('(?=a)b'))
        print(regex.compile('a{1000}{1000}'))
        match (regex.compile('(a)\\1')) {
            Ok(_) => print('valid?')
            Err(error) => print(error)
        }
    "#;
    assert_eq!(
        run(source),
        "Err(Unsupported('(?=a)b', 'look-around, including look-ahead and look-behind, is not supported'))\n\
         Err(TooLarge('a{1000}{1000}'))\n\
         unsupported regular expression '(a)\\1': backreferences are not supported\n"
    );
}

#[test]
fn stdlib_http() {
    use std::io::{Read, Write as _};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        for stream in listener.incoming().take(2) {
            let mut stream = stream.unwrap();
            let mut data = Vec::new();
            let mut buffer = [0u8; 4096];
            // Read the headers, then as many body bytes as Content-Length says.
            let request = loop {
                let read = stream.read(&mut buffer).unwrap();
                data.extend_from_slice(&buffer[..read]);
                let text = String::from_utf8_lossy(&data).to_string();
                if let Some((headers, body)) = text.split_once("\r\n\r\n") {
                    let length = headers
                        .lines()
                        .find_map(|l| l.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().to_string()))
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(0);
                    if body.len() >= length || read == 0 {
                        break text;
                    }
                }
                if read == 0 {
                    break text;
                }
            };
            let first_line = request.lines().next().unwrap_or("").to_string();
            let body = request.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
            let reply = format!("{} {}", first_line, body);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                reply.len(),
                reply
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });

    let source = format!(
        r#"
        import http

        match (http.get('http://127.0.0.1:{port}/api')) {{
            Ok(res) => if (res.status == 200) {{ print(res.body) }}
            Err(error) => print(error)
        }}
        match (http.post('http://127.0.0.1:{port}/items', 'data')) {{
            Ok(res) => print(res.body)
            Err(error) => print(error)
        }}
        match (http.get('http://127.0.0.1:1/closed')) {{
            Ok(_) => print('connected?')
            Err(ConnectionFailed(_, _)) => print('connection failed')
            Err(error) => print(error)
        }}
        "#
    );
    assert_eq!(
        run(&source),
        "GET /api HTTP/1.1 \nPOST /items HTTP/1.1 data\nconnection failed\n"
    );
    server.join().unwrap();
}

#[test]
fn all_examples_type_check() {
    for entry in std::fs::read_dir("examples").unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("Koji") {
            continue;
        }
        if let Err(error) = kojic::check_file(&path) {
            panic!("{}: {}", path.display(), error);
        }
    }
}

// ---------------------------------------------------------------------------
// Methods, associated functions and `mut` parameters
// ---------------------------------------------------------------------------

#[test]
fn methods_and_associated_functions() {
    let source = r#"
        type Person = {
            name: string
            age: int
        }

        function Person.create(name: string): Person {
            return { name: name, age: 0 }
        }

        function Person.greet(self) {
            print(`Hello, I'm ${self.name}!`)
        }

        function Person.olderThan(self, other: Person): boolean {
            return self.age > other.age
        }

        type Shape = {
            Circle(radius: float)
            Square(side: float)
        }

        function Shape.area(self): float {
            return match (self) {
                Circle(r) => 3.0 * r * r
                Square(s) => s * s
            }
        }

        immut person: Person = { name: 'John', age: 20 }
        person.greet()
        immut baby = Person.create('Ana')
        baby.greet()
        print(person.olderThan(baby), Square(2.0).area())
    "#;
    assert_eq!(run(source), "Hello, I'm John!\nHello, I'm Ana!\ntrue 4.0\n");

    let p = "type P = { name: string }\n";
    assert_error(&format!("{}function P.hi(self) {{ self.name = 'a' }}", p), "parameters can only be modified when declared 'mut'");
    assert_error(&format!("{}function P.debug(self) {{}}", p), "every type has 'debug()'");
    assert_error(&format!("{}function P.display(self): int {{ 1 }}", p), "must be declared as 'function P.display(self): string'");
    assert_error(&format!("{}function P.hi(self: P) {{}}", p), "'self' has no type annotation");
    assert_error(&format!("{}function P.name(self) {{}}", p), "same name as a field");
    assert_error(&format!("{}function Q.hi(self) {{}}", p), "cannot declare 'Q.hi'");
    assert_error(&format!("{}export function P.hi(self) {{}}", p), "does not take 'export'");
    assert_error(&format!("{}function f() {{ function P.hi(self) {{}} }}", p), "must be declared at the top level");
    assert_error(&format!("{}function P.hi(self) {{}}\nP.hi()", p), "takes 'self'");
    assert_error(
        &format!("{}function P.make(): P {{ return {{ name: 'a' }} }}\nimmut x: P = {{ name: 'b' }}\nx.make()", p),
        "has no 'self'",
    );
}

#[test]
fn methods_of_imported_types() {
    let dir = temp_dir("methods");
    std::fs::write(
        dir.join("geo.kj"),
        r#"
        export type Point = {
            x: int
            y: int
        }

        function Point.origin(): Point {
            return { x: 0, y: 0 }
        }

        function Point.sum(self): int {
            return self.x + self.y
        }

        export function make(x: int, y: int): Point {
            return { x: x, y: y }
        }

        export function double(mut n: int) {
            n = n * 2
        }
        "#,
    )
    .unwrap();
    let source = r#"
        import geo from 'geo.kj'
        import { Point, make, double } from 'geo.kj'

        print(geo.Point.origin().sum(), Point.origin().sum(), make(2, 3).sum())
        mut n = 3
        geo.double(mut n)
        double(mut n)
        print(n)
    "#;
    let (output, result) = execute(source, "", &dir.join("main.kj"));
    result.unwrap();
    assert_eq!(output, "0 0 5\n12\n");
}

#[test]
fn mut_parameters() {
    let source = r#"
        type Person = {
            name: string
            age: int
        }

        type Box = {
            inner: int
        }

        function reset(mut n: int) {
            n = 0
        }

        function replace(mut items: int[]) {
            items = [9, 9]
        }

        function birthday(mut p: Person) {
            p.age += 1
        }

        mut count = 5
        reset(mut count)
        mut list = [1, 2]
        replace(mut list)
        mut people: Person[] = [{ name: 'Ana', age: 30 }]
        birthday(mut people[0])
        mut data: Box = { inner: 3 }
        reset(mut data.inner)
        print(count, list, people[0].age, data.inner)
    "#;
    assert_eq!(run(source), "0 [9, 9] 31 0\n");

    let f = "function f(mut x: int) {}\n";
    assert_error("function f(x: int) { x = 1 }", "cannot modify 'x': parameters can only be modified when declared 'mut'");
    assert_error("function f(items: int[]) { items[0] = 1 }", "parameters can only be modified");
    assert_error(&format!("{}immut c = 1\nf(mut c)", f), "cannot pass 'c' as 'mut': it is immut");
    assert_error(&format!("{}immut c = [1]\nf(mut c[0])", f), "it is immut");
    assert_error(&format!("{}mut c = 1\nf(c)", f), "argument 1 is a 'mut' parameter");
    assert_error("function f(x: int) {}\nmut c = 1\nf(mut c)", "argument 1 is not a 'mut' parameter");
    assert_error("function f(mut x: float) {}\nmut c = 1\nf(mut c)", "must be exactly float, found int");
    assert_error(&format!("{}immut g = f", f), "can only be called, not used as a value");
    assert_error("immut g = (mut x: int) => x", "anonymous functions cannot have 'mut' parameters");
    assert_error(&format!("{}f(mut 1 + 2)", f), "only a variable, a field or an element");
    assert_error("mut x = 1\nprint(mut x)", "'mut' is only used before a call argument");
    assert_error(
        &format!("{}function outer() {{\nmut b = 1\nfunction inner() {{ f(mut b) }}\n}}", f),
        "functions capture a read-only copy",
    );
}

#[test]
fn mut_self() {
    let source = r#"
        type Counter = {
            count: int
        }

        function Counter.increment(mut self) {
            self.count += 1
        }

        function Counter.addTwice(mut self) {
            self.increment()
            self.increment()
        }

        mut counter: Counter = { count: 0 }
        counter.increment()
        mut fixed: Counter = { count: 10 }
        fixed.addTwice()
        mut counters: Counter[] = [{ count: 5 }]
        counters[0].increment()
        print(counter.count, fixed.count, counters[0].count)
    "#;
    assert_eq!(run(source), "1 12 6\n");

    let c = "type C = { count: int }\nfunction C.inc(mut self) { self.count += 1 }\n";
    assert_error(&format!("{}immut c: C = {{ count: 0 }}\nc.inc()", c), "cannot call 'C.inc' on 'c', as it changes the value: it is immut");
    assert_error(&format!("{}function f(c: C) {{ c.inc() }}", c), "parameters can only be modified");
    assert_error(&format!("{}function C.peek(self) {{ self.inc() }}", c), "cannot call 'C.inc' on 'self'");
    assert_error(
        &format!("{}function C.make(): C {{ return {{ count: 0 }} }}\nC.make().inc()", c),
        "call it on a variable, a field or an element",
    );
    // `mut self` can also replace the value: useful for sum types that change state.
    let source = r#"
        type Light = {
            On
            Off
        }

        function Light.toggle(mut self) {
            self = match (self) {
                On => Off
                Off => On
            }
        }

        mut light: Light = Off
        light.toggle()
        print(light)
        light.toggle()
        print(light)
    "#;
    assert_eq!(run(source), "On\nOff\n");
}

#[test]
fn display_and_debug() {
    let source = r#"
        type Person = {
            name: string
            age: int
        }

        function Person.display(self): string {
            return `${self.name} (${self.age})`
        }

        type Shape = {
            Circle(radius: float)
            Square(side: float)
        }

        function Shape.display(self): string {
            return match (self) {
                Circle(r) => `circle of radius ${r}`
                Square(s) => `square of side ${s}`
            }
        }

        immut ana: Person = { name: 'Ana', age: 30 }
        print(ana)
        print(`Hi, ${ana}!`)
        print(string(ana), ana.display(), ana.debug())
        print(Circle(1.0), Circle(1.0).debug())
        print([ana])
        print('text'.debug(), 'text'.display(), [1, 2].debug())
    "#;
    assert_eq!(
        run(source),
        "Ana (30)\nHi, Ana (30)!\nAna (30) Ana (30) { name: 'Ana', age: 30 }\n\
         circle of radius 1.0 Circle(1.0)\n[{ name: 'Ana', age: 30 }]\n'text' text [1, 2]\n"
    );
}

#[test]
fn loop_and_match_variables_are_imut() {
    assert_error("for x in [1, 2] { x = 3 }", "cannot modify 'x': the 'for' variable is immut (use 'for mut' to change it)");
    assert_error("for row in [[1]] { row[0] = 3 }", "the 'for' variable is immut");
    assert_error(
        "match (Some(1)) {\nSome(n) => { n = 2 }\nNone => {}\n}",
        "cannot modify 'n': variables bound by 'match' are immut",
    );
    // `for mut` changes a copy: the list keeps its values.
    let source = r#"
        mut rows = [[1, 2], [3, 4]]
        for mut row in rows {
            row[0] = 0
            print(row)
        }
        print(rows)
    "#;
    assert_eq!(run(source), "[0, 2]\n[0, 4]\n[[1, 2], [3, 4]]\n");
}

#[test]
fn object_literals_need_a_type() {
    let source = r#"
        type Person = {
            name: string
            age: int
        }

        function make(name: string): Person {
            return { name: name, age: 0 }
        }

        function show(person: Person) {
            print(person.name, person.age)
        }

        immut a: Person = { name: 'Ana', age: 30 }
        mut people: Person[] = [{ name: 'Bia', age: 20 }]
        people[0] = { name: 'Caio', age: 40 }
        immut maybe: Option[Person] = Some({ name: 'Duda', age: 10 })
        show(a)
        show(make('Eva'))
        show({ name: 'Fábio', age: 50 })
        show(people[0])
        match (maybe) {
            Some(person) => show(person)
            None => print('none')
        }
    "#;
    assert_eq!(run(source), "Ana 30\nEva 0\nFábio 50\nCaio 40\nDuda 10\n");
    assert_error("immut d = { name: 'Leo' }", "this object literal needs a type");
    assert_error("print({ a: 1 })", "this object literal needs a type");
    assert_error("immut x: int = { a: 1 }", "an object literal cannot be a value of type int");
    assert_error(
        "type S = { Circle(r: float) }\nimmut s: S = { r: 1.0 }",
        "an object literal cannot be a value of type S",
    );
}

#[test]
fn value_semantics() {
    let source = r#"
        type Person = {
            name: string
            age: int
        }

        immut a = [1, 2, 3]
        mut b = a
        b[0] = 9
        print(a, b)

        immut m = [[1, 2], [3, 4]]
        mut n = m
        n[1][0] = 0
        print(m, n)

        immut ana: Person = { name: 'Ana', age: 30 }
        mut people: Person[] = [ana]
        people[0].age = 31
        print(ana.age, people[0].age)

        function birthday(mut p: Person) {
            p.age += 1
        }
        birthday(mut people[0])
        print(ana.age, people[0].age)

        function same(items: int[]): int[] {
            return items
        }
        immut fixed = [1, 2]
        mut copy = same(fixed)
        copy[0] = 100
        print(fixed, copy)

        function makeReader(items: int[]): () => int[] {
            return () => items
        }
        mut source = [1]
        immut read = makeReader(source)
        source[0] = 2
        print(read(), source)
    "#;
    assert_eq!(
        run(source),
        "[1, 2, 3] [9, 2, 3]\n[[1, 2], [3, 4]] [[1, 2], [0, 4]]\n30 31\n30 32\n[1, 2] [100, 2]\n[1] [2]\n"
    );
}

// ---------------------------------------------------------------------------
// Move-only values and drop
// ---------------------------------------------------------------------------

const CONN: &str = r#"
    type Conn = {
        name: string
    }

    function Conn.drop(mut self) {
        print(`closing ${self.name}`)
    }

    function Conn.open(name: string): Conn {
        print(`opening ${name}`)
        return { name: name }
    }

    function use(conn: Conn) {
        print(`using ${conn.name}`)
    }
"#;

#[test]
fn drop_runs_when_the_owner_goes_out_of_scope() {
    let source = format!(
        "{}{}",
        CONN,
        r#"
        immut a = Conn.open('a')
        if (true) {
            immut b = Conn.open('b')
            use(b)
        }
        immut c = Conn.open('c')
        immut d = c
        use(Conn.open('temp'))
        Conn.open('statement')
        mut e = Conn.open('e1')
        e = Conn.open('e2')

        function make(): Conn {
            immut x = Conn.open('made')
            immut y = Conn.open('local')
            return x
        }
        immut m = make()
        print('end')
    "#
    );
    assert_eq!(
        run(&source),
        "opening a\nopening b\nusing b\nclosing b\nopening c\nopening temp\nusing temp\nclosing temp\n\
         opening statement\nclosing statement\nopening e1\nopening e2\nclosing e1\nopening made\n\
         opening local\nclosing local\nend\nclosing made\nclosing e2\nclosing c\nclosing a\n"
    );
}

#[test]
fn drop_runs_for_fields_after_the_owner() {
    let source = format!(
        "{}{}",
        CONN,
        r#"
        type Pool = {
            first: Conn
            second: Conn
        }

        function Pool.drop(mut self) {
            print('closing the pool')
        }

        type Maybe = {
            One(conn: Conn)
            Nothing
        }

        immut p: Pool = { first: Conn.open('p1'), second: Conn.open('p2') }
        immut list: Conn[] = [Conn.open('l1'), Conn.open('l2')]
        immut maybe: Maybe = One(Conn.open('v'))
    "#
    );
    assert_eq!(
        run(&source),
        "opening p1\nopening p2\nopening l1\nopening l2\nopening v\nclosing v\nclosing l1\nclosing l2\n\
         closing the pool\nclosing p1\nclosing p2\n"
    );
}

#[test]
fn files_are_closed_by_their_owner() {
    let dir = temp_dir("ownership");
    let path = dir.join("notes.txt").display().to_string().replace('\\', "/");
    let source = format!(
        r#"
        import file

        function save(path: string): Result[int, file.FileError] {{
            mut f = file.open(path, file.Write)?
            f.write('one ')?
            immut g = f
            g.write('two')?
            return Ok(2)
        }}

        function load(path: string): Result[string, file.FileError] {{
            immut f = file.open(path, file.Read)?
            return f.read()
        }}

        print(save('{path}'))
        print(load('{path}'))
        immut h = file.open('{path}', file.Read)?
        h.close()
    "#
    );
    let (output, result) = execute(&source, "", &dir.join("main.kj"));
    result.unwrap();
    assert_eq!(output, "Ok(2)\nOk('one two')\n");
}

#[test]
fn move_only_values_cannot_be_used_after_a_move() {
    let errors = [
        ("immut c = Conn.open('a')\nimmut d = c\nprint(c.name)", "'c' cannot be used here: its value was moved (or closed) on line"),
        ("immut c = Conn.open('a')\nif (true) { immut d = c }\nprint(c.name)", "its value was moved"),
        ("function f(c: Conn) { immut d = c }", "'c' cannot be moved: it is a parameter, which only borrows its value"),
        ("immut c = Conn.open('a')\nfor i in range(2) { immut d = c }", "cannot be moved inside a loop"),
        ("immut c = Conn.open('a')\nfunction g() { immut d = c }", "declared outside this function"),
        ("function g() {\nimmut c = Conn.open('a')\nimmut h = () => c.name\n}", "functions capture a copy"),
        ("immut c = Conn.open('a')\nimmut x = if (true) { c } else { c }", "cannot be moved out of a variable inside an 'if'"),
        ("type Pair = { a: Conn }\nimmut p: Pair = { a: Conn.open('x') }\nimmut q = p.a", "cannot be copied out of a field"),
        ("immut c = Conn.open('a')\nc.drop()", "'drop' cannot be called"),
        ("immut cs: Conn[] = [Conn.open('a')]\nfor mut c in cs { print(c.name) }", "'for mut' would copy each item"),
        ("type Other = { n: int }\nfunction Other.drop(self) {}", "'Other.drop' must be declared as 'function Other.drop(mut self)'"),
    ];
    for (code, fragment) in errors {
        assert_error(&format!("{}{}", CONN, code), fragment);
    }
    // A moved variable can get a new value, and a move in a branch that returns does not count.
    let source = format!(
        "{}{}",
        CONN,
        r#"
        mut c = Conn.open('a')
        immut d = c
        c = Conn.open('b')
        function pick(flag: boolean): Conn {
            immut x = Conn.open('x')
            if (flag) {
                return x
            }
            print(x.name)
            return x
        }
        immut p = pick(false)
        print(c.name, d.name, p.name)
    "#
    );
    assert_eq!(
        run(&source),
        "opening a\nopening b\nopening x\nx\nb a x\nclosing x\nclosing a\nclosing b\n"
    );
    assert_error(
        "import file\nimmut h = file.open('x', file.Read)?\nh.close()\nprint(h.read())",
        "'h' cannot be used here",
    );
}

#[test]
fn range_in_a_for_loop() {
    let source = r#"
        mut total = 0
        for i in range(100000) {
            total += i
        }
        print(total)
        for i in range(10, 0, -3) {
            print(i)
        }
        print(range(3))
    "#;
    assert_eq!(run(source), "4999950000\n10\n7\n4\n1\n[0, 1, 2]\n");
    assert_error("for i in range(0, 5, 0) { print(i) }", "range() step cannot be 0");
}

#[test]
fn take_parameters() {
    let source = format!(
        "{}{}",
        CONN,
        r#"
        type Pool = {
            conn: Conn
        }

        function Pool.create(take conn: Conn): Pool {
            return { conn: conn }
        }

        function consume(take conn: Conn) {
            print(`consuming ${conn.name}`)
        }

        function Conn.finish(take self) {
            print(`finishing ${self.name}`)
        }

        immut c = Conn.open('a')
        immut p = Pool.create(c)
        immut d = Conn.open('b')
        consume(d)
        consume(Conn.open('temp'))
        immut e = Conn.open('e')
        e.finish()
        print('end')
    "#
    );
    assert_eq!(
        run(&source),
        "opening a\nopening b\nconsuming b\nclosing b\nopening temp\nconsuming temp\nclosing temp\n\
         opening e\nfinishing e\nclosing e\nend\nclosing a\n"
    );
    let errors = [
        ("function consume(take conn: Conn) {}\nimmut c = Conn.open('a')\nconsume(c)\nprint(c.name)", "'c' cannot be used here"),
        ("function consume(take conn: Conn) {}\nfunction f(c: Conn) { consume(c) }", "'c' cannot be moved: it is a parameter"),
        ("function consume(take conn: Conn) {}\nimmut g = consume", "has 'mut' or 'take' parameters"),
        ("immut g = (take c: Conn) => c.name", "anonymous functions cannot have 'take' parameters"),
    ];
    for (code, fragment) in errors {
        assert_error(&format!("{}{}", CONN, code), fragment);
    }
}

// ---------------------------------------------------------------------------
// Generics
// ---------------------------------------------------------------------------

#[test]
fn generic_functions_and_types() {
    let source = r#"
        function first[T](items: T[]): Option[T] {
            if (items.length() == 0) {
                return None
            }
            return Some(items[0])
        }

        function map[A, B](items: A[], f: (A) => B): B[] {
            mut result: B[] = []
            for item in items {
                result = result + [f(item)]
            }
            return result
        }

        type Pair[A, B] = {
            first: A
            second: B
        }

        function Pair.create(first: A, second: B): Pair[A, B] {
            return { first: first, second: second }
        }

        function Pair.swap(self): Pair[B, A] {
            return { first: self.second, second: self.first }
        }

        function Pair.mapFirst[C](self, f: (A) => C): Pair[C, B] {
            return { first: f(self.first), second: self.second }
        }

        type Tree[T] = {
            Leaf
            Node(value: T, left: Tree[T], right: Tree[T])
        }

        function Tree.size(self): int {
            return match (self) {
                Leaf => 0
                Node(_, left, right) => 1 + left.size() + right.size()
            }
        }

        print(first([1, 2, 3]), first(['a']))
        immut nothing: Option[int] = first([])
        print(nothing)
        print(map([1, 2, 3], (x: int) => `#${x}`))
        immut p = Pair.create(1, 'one')
        immut q = p.swap()
        immut r = p.mapFirst((n: int) => n * 10)
        print(p.first, p.second, q.first, q.second, r.first, r.second)
        immut explicit: Pair[string, int] = { first: 'x', second: 2 }
        print(explicit.swap().first)
        immut tree: Tree[int] = Node(1, Node(2, Leaf, Leaf), Leaf)
        print(tree.size())
    "#;
    assert_eq!(
        run(source),
        "Some(1) Some('a')\nNone\n['#1', '#2', '#3']\n1 one one 1 10 one\n2\n2\n"
    );
}

#[test]
fn generic_errors() {
    let errors = [
        ("function eq[T](a: T, b: T): boolean { return a == b }", "operator '==' needs 'T' to be equatable: declare it as '[T: equatable]'"),
        ("function f[T](a: T): int { return a.length }", "a value of type T has no field 'length'"),
        ("function id[T](a: T): T { return a }\nimmut g = id", "'id' is generic and can only be called"),
        ("function id[T](a: T): T { return a }\nimmut x: int = id('a')", "argument 1 must be int, found string"),
        ("type Box[T] = { value: T }\nimmut b: Box = { value: 1 }", "type 'Box' needs 1 type argument(s)"),
        ("type Box[T] = { value: T }\nimmut b: Box[int, int] = { value: 1 }", "expects 1 type argument(s), found 2"),
        ("import file\ntype Box[T] = { value: T }\nfunction f(b: Box[file.File]) {}", "type parameters cannot be move-only values yet"),
        ("type Alias[T] = int", "the alias 'Alias' cannot have type parameters"),
        ("function f[T, T]() {}", "type parameter 'T' is declared twice"),
        ("type Box[T] = { value: T }\nfunction Box.make[T](v: T): Box[T] { return { value: v } }", "is already a type parameter of 'Box'"),
        ("function pair[A, B](a: A, b: B): A { return b }", "returning B, but the function returns A"),
    ];
    for (code, fragment) in errors {
        assert_error(code, fragment);
    }
}

#[test]
fn generics_across_modules() {
    let dir = temp_dir("generics");
    std::fs::write(
        dir.join("box.kj"),
        r#"
        export type Box[T] = {
            value: T
        }

        function Box.get(self): T {
            return self.value
        }

        export function wrap[T](value: T): Box[T] {
            return { value: value }
        }
        "#,
    )
    .unwrap();
    let source = r#"
        import box from 'box.kj'
        import { Box, wrap } from 'box.kj'

        immut b = wrap(5)
        immut c: Box[string] = { value: 'x' }
        immut d: box.Box[int] = box.wrap(2)
        print(b.get(), c.get(), box.wrap(true).get(), d.value)
    "#;
    let (output, result) = execute(source, "", &dir.join("main.kj"));
    result.unwrap();
    assert_eq!(output, "5 x true 2\n");
}

#[test]
fn generic_constraints() {
    let source = r#"
        type Point = {
            x: int
            y: int
        }

        function max[T: orderable](items: T[]): T {
            mut best = items[0]
            for item in items {
                if (item > best) {
                    best = item
                }
            }
            return best
        }

        function count[T: equatable](items: T[], wanted: T): int {
            mut n = 0
            for item in items {
                if (item == wanted) {
                    n += 1
                }
            }
            return n
        }

        type Set[T: equatable] = {
            items: T[]
        }

        function Set.has(self, item: T): boolean {
            return item in self.items
        }

        print(max([3, 1, 2]), max(['b', 'a']), max([1.5, 0.5]))
        immut p: Point = { x: 1, y: 2 }
        print(count([p, p], { x: 1, y: 2 }), count([1, 2, 1], 1))
        immut s: Set[string] = { items: ['a', 'b'] }
        print(s.has('b'), s.has('z'))
    "#;
    assert_eq!(run(source), "3 b 1.5\n2 2\ntrue false\n");

    let common = "type Person = { name: string }\n\
        function max[T: orderable](items: T[]): T { return items[0] }\n\
        function same[T: equatable](a: T, b: T): boolean { return a == b }\n\
        type Set[T: equatable] = { items: T[] }\n";
    let errors = [
        ("immut a: Person = { name: 'a' }\nimmut x = max([a])", "Person is not orderable, as 'max' requires"),
        ("immut x = same([(n: int) => n], [(n: int) => n])", "((int) => int)[] is not equatable, as 'same' requires"),
        ("immut f: (int) => int = (n: int) => n\nprint(f == f)", "values of type (int) => int cannot be compared with '=='"),
        ("immut s: Set[(int) => int] = { items: [] }", "is not equatable, as 'Set' requires"),
        ("function f[T: equatable](a: T, b: T): boolean { return a < b }", "operator '<' needs 'T' to be orderable: declare it as '[T: orderable]'"),
        ("function f[T](a: T, b: T): boolean { return a == b }", "operator '==' needs 'T' to be equatable"),
        ("function f[T: comparable](a: T): T { return a }", "unknown constraint 'comparable'"),
        ("function f[T: orderable](a: T, b: T): T { return a + b }", "operator '+' cannot be used on 'T'"),
        ("import file\nimmut f = file.open('x', file.Read)?\nimmut g = file.open('y', file.Read)?\nprint(f == g)", "values of type File cannot be compared"),
    ];
    for (code, fragment) in errors {
        assert_error(&format!("{}{}", common, code), fragment);
    }
    // A shape of equatable fields is equatable.
    assert_eq!(run(&format!("{}immut a: Person = {{ name: 'a' }}\nprint(same(a, a))", common)), "true\n");
}

#[test]
fn building_text() {
    let source = r#"
        mut text = ''
        for i in range(1000) {
            text += 'x'
        }
        print(text.length())
        immut a = 'ab'
        mut b = a
        b += 'c'
        print(a, b)
        print(['a', 'b', 'c'].join(', '), ['x'].join(''))
        immut names: string[] = ['Ana', 'Bia']
        print(names.join(' & '))
    "#;
    assert_eq!(run(source), "1000\nab abc\na, b, c x\nAna & Bia\n");
    assert_error("print([1, 2].join(','))", "'join' needs a list of strings, found int[]");
}

#[test]
fn chars() {
    let source = r#"
        immut text = 'Hello, Koji'
        immut c = text[0]
        print(c, c.code(), c.debug(), c == 'H', c < 'Z')
        immut d: char = 'k'
        immut letters: char[] = ['a', 'b']
        print(d, letters, char(75), char(-1))
        for ch in 'ab' {
            print(ch.code())
        }
        print(text.chars().length(), text.startsWith('Hello'), text.endsWith('Koji'), text.indexOf('Koji'), text.indexOf('x'))
        print('o' in text, `${c}!`, string(c) + 'i')
        match (text[7]) {
            'K' => print('K!')
            _ => print('other')
        }
    "#;
    assert_eq!(
        run(source),
        "H 72 'H' true true\nk ['a', 'b'] Some('K') None\n97\n98\n11 true true Some(7) None\ntrue H! Hi\nK!\n"
    );
    assert_error("immut c: char = 'ab'", "'ab' is not a char: a char literal has exactly one character");
    assert_error("immut x = 'a'[0] + 1", "operator '+' cannot be applied to char and int");
}

#[test]
fn list_methods_and_slices() {
    let source = r#"
        type Person = {
            name: string
            age: int
        }

        mut items = [3, 1, 2]
        items.push(4)
        items.insert(0, 9)
        print(items)
        print(items.pop(), items.removeAt(0), items.removeAt(10), items)
        items.sort()
        print(items, items.indexOf(3), items.indexOf(7))
        mut people: Person[] = [{ name: 'Ana', age: 30 }, { name: 'Bia', age: 25 }, { name: 'Caio', age: 30 }]
        people.sortBy((a: Person, b: Person) => a.age < b.age)
        print(people[0].name, people[1].name, people[2].name)
        mut copy = items
        copy.push(100)
        print(items, copy)
        mut empty = []
        empty.push('x')
        print(empty)

        immut list = [10, 20, 30, 40]
        print(list[1..3], list[..2], list[2..], list[..])
        immut text = 'Hello, Koji'
        print(text[0..5], text[7..])
    "#;
    assert_eq!(
        run(source),
        "[9, 3, 1, 2, 4]\nSome(4) Some(9) None [3, 1, 2]\n[1, 2, 3] Some(2) None\nBia Ana Caio\n\
         [1, 2, 3] [1, 2, 3, 100]\n['x']\n[20, 30] [10, 20] [30, 40] [10, 20, 30, 40]\nHello Koji\n"
    );
    let errors = [
        ("immut fixed = [1]\nfixed.push(2)", "cannot call 'push' on 'fixed', as it changes the value: it is immut"),
        ("type P = { n: int }\nmut ps: P[] = []\nps.sort()", "'sort' needs orderable items, and P is not: use 'sortBy'"),
        ("mut items = [1]\nitems.push('a')", "the item must be int, found string"),
        ("function f(items: int[]) { items.push(1) }", "parameters can only be modified when declared 'mut'"),
        ("immut items = [1, 2]\nprint(items[2..1])", "slice 2..1 out of range"),
    ];
    for (code, fragment) in errors {
        assert_error(code, fragment);
    }
}

#[test]
fn stdlib_system() {
    let source = r#"
        import system

        print(system.args(), system.env('KOJI_NOT_SET'))
        match (system.run('echo', ['hello', 'koji'])) {
            Ok(output) => print(output.status, output.stdout.trim())
            Err(error) => print(error)
        }
        match (system.run('kj-no-such-command', [])) {
            Ok(_) => print('?')
            Err(error) => print(error, '|', error.debug())
        }
        print('before exit')
        system.exit(3)
        print('not printed')
    "#;
    let (output, result) = execute(source, "", Path::new("test.kj"));
    assert_eq!(result, Err(kojic::error::EzError::Exit(3)));
    assert_eq!(
        output,
        "[] None\n0 hello koji\ncommand 'kj-no-such-command' not found | NotFound('kj-no-such-command')\nbefore exit\n"
    );
}
