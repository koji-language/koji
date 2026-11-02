# Compiler Plan

This document plans the path from the current implementation to a **self-hosted compiler**: a Koji compiler written in Koji that produces native binaries. It records the language decisions already made and the ones still open.

> Status: the [decisions](#decisions) below are implemented in the interpreter, except where noted. Phase 2 (the compiler in Rust) has started: `kojic build` compiles a first subset of the language (see [The compiler today](#the-compiler-today)).

---

## Where we are

Everything is written in Rust. `kojic run <file>` runs four stages:

1. **Lexer** (`src/lexer/`): source text → tokens.
2. **Parser** (`src/parser/`): tokens → AST.
3. **Type checker** (`src/semantic/`): checks the whole program, including imported modules, before anything runs. It records the type of every expression for the compiler.
4. **Interpreter** (`src/runtime/`): walks the AST and executes it.

`kojic build <file>` shares the first three stages, then:

4. **Lowering** (`src/ir/`): AST → IR, a typed intermediate representation with basic blocks.
5. **C generator** (`src/backend/c/`): IR → one C file.
6. **C compiler** (`cc`): C file + the runtime library → executable.

The **runtime** (`runtime/`) is a Rust library with a C interface: strings, output, panics and the operators that need more than a C expression. It is compiled once as a static library and linked into every program.

---

## Bootstrap strategy

A self-hosted compiler needs an existing compiler to be built the first time. The Rust implementation plays that role.

```
stage 0: kojic in Rust (current implementation + a code generator)
    │ compiles
    ▼
stage 1: kojic written in Koji, compiled by stage 0
    │ compiles its own source
    ▼
stage 2: kojic compiled by kojic
    │ compiles its own source again
    ▼
stage 3: must be identical to stage 2 (fixed point)
```

When stage 2 and stage 3 produce the same output, the compiler sustains itself.

**Backend:** generate C, with the runtime written in Rust (decided).

-   **C** needs no LLVM or Cranelift bindings, works on any platform with a C compiler, gets the optimizations of gcc and clang for free, and the generated code is readable, which makes debugging much easier. A compiler written in Koji only has to write text.
-   **The runtime in Rust** reuses the standard library already implemented for the interpreter (`http`, `regex`, `json`, ...) instead of rewriting it in C, and the interpreter and compiled programs share the same code (number formatting, `**`), so they produce the same output. It is distributed with `kojic` as a static library: users only need a C compiler, never Rust.
-   **The IR** is independent of C: a native backend can be added later, linking with the same runtime.

---

## Decisions

### No `any` type

`any` is removed from the language. Dynamic data is represented by sum types. As a consequence, no value needs to carry its type at runtime.

JSON becomes a sum type in the `json` module:

```koji
type Json = {
    Null
    Bool(value: boolean)
    Int(value: int)
    Float(value: float)
    String(value: string)
    Array(items: Json[])
    Object(fields: Map[string, Json])
}
```

-   `json.parse(text: string) -> Result[Json, JsonError]`
-   `json.stringify(value: Json) -> string`
-   `Int` and `Float` are separate variants:
    -   A number without `.`, `e` or `E` that fits in 64 bits is an `Int`.
    -   A number with `.`, `e` or `E` is a `Float`.
    -   An integer too large for 64 bits is `Err(NumberOutOfRange(...))`.
-   `json.stringify(Float(3.0))` writes `3.0`, so that parsing it back gives a `Float` again.
-   `Int(1) == Float(1.0)` is `false`: they are different variants.
-   Helpers such as `json.get(value, key) -> Option[Json]` and `json.asFloat(value) -> Option[float]` (accepting both `Int` and `Float`) reduce the verbosity of matching.
-   Later, with user-defined generics: `json.decode[T](text) -> Result[T, JsonError]`.

### Mandatory `export`

Everything in a module is private by default. Only declarations marked with `export` can be imported.

-   Exportable: `export function`, `export mut`, `export immut` and `export type`.
-   Exporting a sum type also exports its variants.
-   `export` is only allowed at the top level of a file.
-   **`export mut`** is read-only outside its module:
    -   Importers always see the **current** value, not a copy taken at import time.
    -   Outside its module, it cannot be changed at all: neither given a new value nor changed in its contents.

    ```koji
    // file: counter.kj
    export mut count = 0

    export function increment() {
        count += 1
    }
    ```

    ```koji
    import { count, increment } from 'counter.kj'

    increment()

    print(count) // 1

    count = 5 // Error! 'count' is read-only outside 'counter.kj'
    ```

-   Importing something that is not exported is an error: `'helper' is not exported by 'math.kj'`.
-   A public signature cannot use a private type (e.g. `export function f(): Helper` with a private `Helper`).
-   The standard library is unaffected: all of its members are public.
-   Later: re-exports (`export { add } from 'math.kj'`).

### Tooling: `kojic run` and `kojic build`

The interpreter stays: `kojic run` checks and interprets a program, for fast development. The compiler will be `kojic build`, producing a native binary. Both must give the same results (every test runs in both modes).

### The `?` operator

`?` propagates errors instead of writing a `match` for every fallible call:

-   on a `Result`, `x?` gives the `Ok` value, or returns the same `Err` from the function;
-   on an `Option`, `x?` gives the `Some` value, or returns `None` (only in functions returning `Option`);
-   at the top level of the main file, an `Err` stops the program with its message and exit code 1.

```koji
type ConfigError = {
    File(error: file.FileError)
    Json(error: json.JsonError)
}

function readConfig(path: string): Result[json.Json, ConfigError] {
    immut f = file.open(path, file.Read)? // FileError → File(...)
    immut text = f.read()?

    return Ok(json.parse(text)?) // JsonError → Json(...)
}
```

**Error conversion** is automatic, with a strict rule: when the error type differs from the function's error type, `?` wraps it in the variant of the function's error type that has **exactly one** field of that type. If there is no such variant, or more than one, it is a compile error, and `result.mapErr(f)` converts the error explicitly.

Implemented; see [Error Handling](error-handling.md).

### Methods

A method is a function prefixed with its type, with an explicit `self` parameter (not `this`):

```koji
type Person = {
    name: string
}

function Person.greet(self) {
    print(`Hello, I'm ${self.name}!`)
}

immut person: Person = {
    name: 'John'
}

person.greet()
```

A function prefixed with its type but without `self` is an **associated function**, called on the type: `function Person.create(name: string): Person`, called as `Person.create('John')` (e.g. for constructors).

A method that changes its value declares **`mut self`**. The call does not repeat the marker (`counter.increment()`), because the method is already called on the variable; the value must be a `mut` variable (or a field or element of one), never an `immut`. `mut self` can also replace the value (`self = Off`), which is useful for sum types that change state.

Implemented; see [Custom Types](custom-types.md#methods).

### Memory management: ownership, value semantics and drop

Memory is managed with **ownership**, without a garbage collector. Every value has exactly one owner, and when the owner goes out of scope the value is **dropped**: its memory is released and, for resources, the resource is closed. This is deterministic, so there is no `defer`.

```koji
function readNotes(): Result[string, file.FileError] {
    immut f = file.open('notes.txt', file.Read)?
    immut content = f.read()? // on an error, '?' returns and 'f' is closed

    return Ok(content) // 'f' goes out of scope: the file is closed
}
```

**Value semantics with copy-on-write**, as in Swift, Hylo and Mojo. Assigning or passing a value gives an independent copy: there is no aliasing, and no variable becomes unusable (unlike a move). Underneath, the memory is shared until one of the copies is modified, so copies are cheap.

```koji
immut a = [1, 2, 3]
mut b = a // nothing is copied yet

b[0] = 9 // the list is copied now, because b changes

print(a) // [1, 2, 3]
print(b) // [9, 2, 3]
```

Consequences:

-   **`is` only tests variants** (`result is Some`). There is no reference identity; `==` compares values.
-   **Two mutability levels**, as in Swift: `mut` (everything can change) and `immut` (neither the variable nor its contents can change). With value semantics, a third level that forbids replacing the value but allows changing its contents (the `const` of TypeScript) would mean little: both are ways of changing the variable's value. Every declaration has its keyword, and `x = 1` only changes an existing `mut` variable, so a typo is an error instead of a new variable (see [Variable Types](variable-types.md)). A separate keyword for compile-time constants is not needed: the compiler can recognize an `immut` with a literal value by itself.
-   **No `defer`** (decided): dropping releases resources. It is not planned; it would only be reconsidered if a real need appears, for cleanup that does not belong to any value.
-   **Resource types are move-only.** A `File` cannot be copied (two copies would close the same file): `g = f` moves it, and using `f` afterwards is a compile error. `f.close()` consumes `f`, so using a closed file is a compile error instead of a panic.

Implemented in the interpreter: assignments and `mut` write-backs go through a place (a variable with fields and indices) that copies every shared level before changing it. See [Values and Memory](memory.md).

### Value semantics details

-   **Closures capture a read-only copy** of the variables they use: assigning to a captured variable (or changing its contents) is a compile error, and the closure sees the value from the moment it was created. Module variables are not captured: functions read and change them directly. State that changes is kept another way (e.g. a type with methods). Implemented.
-   **`mut` marks a parameter the function may modify**, both in the declaration and in the call, so the change is visible where it happens:

    ```koji
    function addItem(mut list: int[], item: int) { ... }

    addItem(mut items, 2)
    ```

    An `immut` variable can never be passed as `mut`. Parameters without `mut` (and `self`) are read-only. Implemented; see [Functions](functions.md#mut-parameters).
-   **Move-only types**, including user types: values that cannot be copied (resources). Assigning moves them, and using the previous variable is a compile error.
-   **Only move-only types can have a `drop`** method, called automatically when the value is dropped. A copyable type with `drop` would run it once per copy.
-   **Move-only is implicit**: a type is move-only when it declares a `drop` method, or when it has a field of a move-only type (a type holding a `File` cannot be copied either). There is no separate marker.
-   **`for` and `match` variables are `immut`**: `for x in items` gives an immut `x`; `for mut x in items` allows changing it (a copy: the list does not change). Variables bound by `match` patterns are always immut. Implemented.
-   **Intentionally shared structures** (graphs, caches) use indices into a list ("arenas"). An explicit shared type may come later, only if arenas prove insufficient.

Move-only types and `drop` are implemented:

-   **Borrowing**: parameters (with or without `mut`), `self`, the `for` variable and the bindings of a `match` on a variable borrow their value, so passing a file to a function does not move it. A value is moved by assigning it, returning it, or storing it in another value (a field, a list, a variant).
-   **Checks**: using a variable after its value was moved is a compile error. The check follows branches (a move in a branch that returns does not count after it), and forbids moves that could happen twice: out of a variable declared outside the current loop or function. Values cannot be copied out of a field or an element, and closures cannot use a move-only variable around them.
-   **Drops**: at the end of a block (also when leaving it with `return`, `break` or `?`), when the variable gets a new value, after a temporary is used, and at the end of the program, in reverse order of declaration. `drop` is declared as `function Type.drop(mut self)` and cannot be called directly. `file.close()` consumes the file.
-   **`take` parameters** take ownership of their argument, which moves in at the call (no marker): the function can store it, return it, or let it be dropped when it returns. `take self` consumes the value a method is called on.
-   **Limitation**: parts of a temporary that a `match` does not bind are not dropped (a `File` is still closed when its memory is released).

### Standard library: file modes and readable errors

-   `file.open(path, mode)` takes a sum type, so an invalid mode does not compile:

    ```koji
    type Mode = { Read  Write  Append }

    immut f = file.open('notes.txt', Read)?
    ```

-   `print(error)` (and string interpolation) shows a human-readable message for standard library errors, e.g. `file 'notes.txt' not found` instead of `NotFound('notes.txt')`. The same text is available as `error.display()`.

Both are implemented.

### Block scope

A variable belongs to the block where it is declared (the file, a function, or any `{ ... }` block) and is visible in the blocks inside it. Assigning to a name that exists in an outer block changes that variable; otherwise the assignment declares a new variable in the current block.

### Imports apply to the whole block

`import` statements are processed before the rest of their block, so types and functions declared before an `import` can use it (like ES modules).

### `MathError.Indeterminate` stays

It is kept for future functions, even though no current function returns it (`0 ** 0` is `1`).

### No `interface`

`type` covers what interfaces did: shape types describe the fields of a value. Shape types are **nominal**: two types with the same fields are different types (`Celsius` and `Fahrenheit` do not mix). An object literal is a value of the type expected where it is written (an annotation, a parameter, a return type); without an expected type, it is a compile error, so there are no anonymous objects.

`interface` stays a reserved word, as a precaution. Generic constraints do not need it: `equatable` and `orderable` are names the compiler knows, deduced from the structure of each type, and any other behavior is passed as a function (e.g. `sort(items, (a, b) => a.age < b.age)`).

### No `try` / `catch`

Errors are values, not exceptions:

| Kind                     | Examples                                                       | Handling                                      |
| ------------------------ | -------------------------------------------------------------- | --------------------------------------------- |
| Expected (recoverable)   | missing file, invalid JSON, network failure, `int('abc')`      | `Result`                                      |
| Bug (unrecoverable)      | index out of range, division by zero, integer or float overflow | **panic**: the program stops, it cannot be caught |

A built-in `panic(message)` lets code report impossible states.

### Typed errors in the standard library

Every fallible standard library function returns a `Result` with an error type specific to its module:

```koji
// file
type FileError = {
    NotFound(path: string)
    PermissionDenied(path: string)
    IsDirectory(path: string)
    InvalidUtf8(path: string)
    Io(path: string, message: string)
}

// http (4xx and 5xx statuses are still Ok(Response): they are not transport failures)
type HttpError = {
    InvalidUrl(url: string)
    ConnectionFailed(url: string, message: string)
    Timeout(url: string)
    Tls(url: string, message: string)
    InvalidResponse(url: string, message: string)
}

// json
type JsonError = {
    Syntax(message: string, line: int, column: int)
    NumberOutOfRange(text: string, line: int, column: int)
}

// datetime.parse
type DateError = {
    PatternMismatch(text: string, pattern: string)
    InvalidDate(text: string)
}

// int(x) and float(x) with a string
type ParseError = {
    InvalidNumber(text: string)
    OutOfRange(text: string)
}

// math
type MathError = {
    Complex // the result exists only in the complex numbers: sqrt(-1), log(-1)
    Indeterminate // indeterminate form (no current function returns it)
    Infinite // the result is infinite (a pole): pow(0, -1), log(0)
}

// regex.compile
type RegexError = {
    InvalidPattern(pattern: string, message: string)
    Unsupported(pattern: string, message: string)
    TooLarge(pattern: string)
}
```

`time` has no error type. `input()` keeps returning `Option[string]`.

`regex.compile(pattern)` returns `Result[Regex, RegexError]`, and the methods of `Regex` (`test`, `find`, `findAll`, `replace`, `replaceAll`, `split`) cannot fail. Implemented.

### No NaN, no infinity

`float` is always a **finite real number**:

-   There is no NaN. Math functions whose result is not a real number return `Err(Complex)`, `Err(Indeterminate)` or `Err(Infinite)`.
-   There is no infinity. Float overflow (`1e308 * 10`) panics, like integer overflow.
-   Float literals that are too large (`1e400`) are compile-time errors.
-   `float('1e400')` returns `Err(OutOfRange(...))`.
-   `MathError` only describes **domain** problems (the result does not exist in the reals). Overflow is a **representation** limit (the result exists but does not fit), so it does not belong to `MathError`.

### Representation limits panic, domain problems are `MathError`

One rule decides how numeric failures are reported:

| Problem | Meaning | Handling | Examples |
| --- | --- | --- | --- |
| **Representation limit** | The result exists but does not fit in an `int` or a `float` | panic | `9223372036854775807 + 1`, `1e308 * 10`, `math.pow(10, 400)`, `math.round(1e300)` |
| **Domain problem** | The result does not exist in the real numbers | `Err(MathError)` | `math.sqrt(-1)` → `Complex`, `math.pow(0, -1)` → `Infinite` |

`0 ** 0` and `math.pow(0, 0)` are `1`: that is the value of the expression, as in algebra and combinatorics. The indeterminate form 0⁰ only concerns limits.

Operators cannot return a `Result`, so they always panic, including on domain problems such as division by zero.

In `math` functions, an infinite result (a pole, such as `math.pow(0, -1)`, which is `1 / 0`) is `Err(Infinite)`. It is not an overflow: a larger type would fit an overflowing result, but no type can represent an infinite one. `Infinite` has no sign field, because the sign is always determined by the function (`pow(0, -y)` is always `+∞`, a future `log(0)` always `-∞`).

Float literals support scientific notation (`6.022e23`).

### Division

Like Go and Rust: `int / int` gives an `int` truncated toward zero, a division with a `float` gives a `float`, and `%` has the sign of the dividend. `//` is always a comment.

---

## What the language needs to support the compiler

A compiler is a large program, with many data structures and many operations that can fail. These features are needed before writing `kojic` in Koji, in order of importance:

1.  **User-defined generics**: `type Map[K: equatable, V]`, `function find[T](...)`, using the same bracket syntax as `Option[T]`: implemented, with the `equatable` and `orderable` constraints (see [Generics](generics.md)).
2.  **Collections**: lists with `push`, `pop`, `insert`, `sort` and slicing; `Map[K, V]` with any equatable key; `Set[T]` (implemented).
3.  **Text processing**: the `char` type, slicing, `startsWith`, `endsWith`, `indexOf`, and `+=` in place instead of a string builder (implemented); efficient indexing (`s[i]` is O(n) today, a matter for the compiled strings).
4.  **Error propagation**: the `?` operator (implemented).
5.  **Methods on types** (`Type.method()`, `mut self`): implemented.
6.  **Complete type inference**: every variable must have a known type by the end of its block (implemented). An empty value (`[]`, `[:]`, `None`) gets its type from an annotation or from a later use (`push`, an assignment, `map[key] = value`, `set.add`); otherwise it is a compile error that suggests the annotation.
7.  **System access**: command-line arguments, the exit code, environment variables and running processes (implemented in the `system` module); `panic` (implemented); path manipulation and directories (implemented in the `path` and `file` modules).
8.  **FFI** (`extern function`), so the compiled standard library can call C functions.
9.  **Module visibility**: covered by the mandatory `export` decision.
10. **Testing framework** (on the roadmap): not required, but it speeds up compiler development a lot.

### Semantics to settle before compiling

-   **`int` to `float` coercion**: an `int` becomes a `float` as a single value (a variable, an argument, a field, the items of a list literal), never inside an existing value: an `int[]` is not a `float[]` (decided). The interpreter converts at the same places as the generated code.
-   **Memory management**: ownership with value semantics, copy-on-write and drop (see [Memory management](#memory-management-ownership-value-semantics-and-drop)).

---

## The compiler today

`kojic build main.kj` writes the executable `main` (`-o` chooses another name). `--emit c` writes the generated C instead, and `--emit ir` the IR as text. The C compiler is `cc`, or the one named by the `KOJI_CC` environment variable.

It compiles a first subset of the language:

-   `int`, `float`, `boolean`, `char` and `string` values (also indexing and slicing strings, `for` over the characters of a string, `chars`, `code` and `char(n)`), `mut` and `immut` variables, top-level variables used by functions;
-   object types and sum types (also generic, such as `Option[int]`, and recursive), with object literals, constructors, fields (reading and assigning, also `people[0].age += 1`), `==`, `match` (variants, literals, bindings, guards, as a statement or an expression), `is` and the narrowing it gives (`if (x is Some) { x.value }`);
-   `Option` and `Result`: `?` (with the automatic wrapping of the error, and stopping the program at the top level), `mapErr` with a variant (`mapErr(Parse)`), and the list methods that give them (`pop`, `removeAt`, `indexOf`);
-   closures: lambdas, which capture a copy of the local variables they use (top-level variables are read directly), named functions used as values, function types (`(int) => int`), calls of any expression that gives a function (`makeAdder(5)(1)`), `sortBy` and `mapErr` with a function;
-   imports of other files (`import calc from 'calc.kj'` and `import { add } from 'calc.kj'`), with their functions, variables (read as they change), types, methods and generics; each module runs once, before the module that imports it, as in the interpreter;
-   `Map` and `Set`: literals (`['Ana': 30]`, `[:]`), `Set.from`, reading and assigning by key (also `groups['even'].push(n)`), `in`, `==`, `for` over the keys or items, `get`, `has`, `remove`, `add`, `keys`, `values` and `length`; any equatable value can be a key, kept in the order it was added;
-   lists of all those values: literals, indexing and assigning items (also `grid[0][1] = 5`), slices, `+`, `==`, `in`, `for` over a list, `length`, `push`, `insert`, `sort` and `join`;
-   every operator on those types, with the same panics as the interpreter (overflow, division by zero, `float` overflow, invalid shifts and powers);
-   string interpolation, `print`, `string(x)`, `int(x)` and `float(x)` (also from a string, as a `Result`), `input`, `panic`, `in` on strings, and the string methods (`trim`, `upper`, `lower`, the `is...` tests, `split`, `replace`, `startsWith`, `endsWith`, `indexOf`, `length`);
-   the standard library modules `math`, `time`, `path`, `system` and `file` (files are move-only: closed by `close()` or when dropped), imported whole or by name; their errors show the same readable messages as in the interpreter;
-   functions with parameters and a return value, recursion, `mut` parameters (`reset(mut count)`) and `take` parameters;
-   move-only values and `drop`: a value is dropped at the end of its block (also when leaving it with `return`, `break`, `continue` or `?`), when its variable gets a new value, after a temporary is used, and at the end of the program, last declared first; a move leaves the variable empty, so it is not dropped twice; dropping runs the type's `drop` method, then drops what the value holds (fields, items of lists);
-   methods, `mut self` and associated functions (`Point.origin()`);
-   generic functions and methods of generic types (also with their own type parameters, `Pair.mapFirst[C]`), with one instance for each set of type arguments they are called with (monomorphization), and the constraints `equatable` and `orderable`; a type's `display` method in `print`, interpolation and `string(x)`, and the built-in `display()` and `debug()`;
-   `if` (also as an expression), `while`, `for` over `range`, `break`, `continue` and `return`.

Anything else (`char`, generic functions used as values, imports inside a block, and the modules `datetime`, `json`, `regex` and `http`) is reported as `compile error: ... is not supported by 'kojic build' yet`. The tests in `test/compiler.rs` compile programs and check that the executable behaves exactly like the interpreter: same output, same panic message and the same exit code.

**Memory**: strings, lists and records (the values of declared types) are reference-counted. A variable owns one reference, and copying a value only adds a reference (value semantics with copy-on-write); changing a list that is shared copies it first, so the other variables never see the change. Assigning a variable releases its old value, and a function releases its values when it returns. Each item of a list and each field of a record takes an 8-byte slot, and the generated code describes their types to the runtime (how to retain, release, show, compare and order them). A generic type gets one description for each set of type arguments (`Option[int]`, `Option[string]`). Maps find their keys with a hash, which the description of each type also gives.

A `mut` parameter is passed by value and its final value is written back to the caller's variable when the function returns, as in the interpreter: the C function gets a pointer for it (`out<n>`).

A closure is the function's code and a record with the values it captured; its code takes that record as a first, hidden argument. A named function used as a value is called through a small function that takes the (empty) record.

The tests run every compiled program under valgrind when it is installed, so a memory error fails them even when the output is right.

The checker records the type arguments it infers for each call of a generic function; inside a generic body, a type parameter is replaced by the instance's type argument.

Each module has its own names: two files can declare a function with the same name. A function takes the file of its module for the messages of its panics. Function signatures come from the types the checker gave them, so a type is never looked up by its name.

Dropping a type is a generated function: it calls the `drop` method (`mut self`, so it may change the value), then the functions that drop each field or item that needs it. The checker says where values are declared, moved, replaced and used as temporaries; the compiler emits the drops at those points.

The standard library is in the runtime: the generated code calls a Rust function for each library function, giving it the description of the `Option`, `Result` or error it must build. The messages of errors and the conversions of text (`int('42')`) are the same functions in the interpreter and in the runtime.

**Next steps**, in order: the modules `datetime`, `json`, `regex` and `http`.

---

## Phases

| Phase | Delivers | Validation |
| --- | --- | --- |
| **0. Decisions** | The open decisions below, and the documentation updated | Documentation reviewed |
| **1. Language** | The decisions above and the features in [What the language needs](#what-the-language-needs-to-support-the-compiler), implemented in the Rust checker and interpreter | Tests like the current ones |
| **2. Compiler in Rust** | A typed IR in `src/ir/` (monomorphization, closure conversion, `match` compiled to decision trees), a C code generator, a runtime in Rust (strings, lists, maps, memory management) and a `kojic build` command | Every test runs both interpreted and compiled, with identical output |
| **3. `kojic` in Koji** | Lexer → parser → checker → IR → C generator, in this order | At each step, compare with the Rust implementation: same tokens, same AST, same errors, same generated C |
| **4. Bootstrap** | Stages 1 → 2 → 3 | Stage 2 output identical to stage 3 |
| **5. Maintenance** | A bootstrap policy: freeze the Rust implementation as stage 0, or commit the generated C of `kojic` so that only a C compiler is needed to build everything | CI rebuilds every stage |

Notes:

-   **Compiled standard library**: `json` and `regex` can be rewritten in Koji. `http` with TLS is heavy: it can go through FFI (e.g. libcurl) or stay outside the core.
-   **Rule for `kojic`**: it may only use features supported by the previous compiler version, otherwise the bootstrap breaks.

---

## Open decisions

1.  **FFI** (`extern function`), so the compiled standard library can call C functions. With the runtime in Rust, the standard library itself does not need it; it is for calling C libraries from Koji code.

Decided:

-   **`Ok()`**: a `Result[void, E]` succeeds with `Ok()`, without an argument (there is no `void` value); it is shown as `Ok(void)`.

-   **`int` to `float`**: only a single value is converted; an `int[]` where a `float[]` is expected is an error, since converting it would copy the list (see [Primitive Types](primitive-types.md#numeric-types)).
-   **Backend**: C, with the runtime in Rust (see [Bootstrap strategy](#bootstrap-strategy)).

-   **Mutating methods**: `mut self`, with no marker at the call (see [Methods](#methods)).
-   **Functions without `self`**: associated functions, `Person.create('John')`.
-   **Move-only types**: implicit, from `drop` (see [Value semantics details](#value-semantics-details)).
-   **Display**: `display()` is readable (a `display(self): string` method of the type, when declared) and is what `print` and interpolation use; `debug()` shows the structure (see [Custom Types](custom-types.md#display)).
-   **`regex.compile`**, with `RegexError = { InvalidPattern(pattern, message) }` (see [Typed errors in the standard library](#typed-errors-in-the-standard-library)).
-   **`for` and `match` variables are immut**, with `for mut` to opt out.
-   **`mut` and `immut`** replace `var`, `const` and `imut`; every declaration has one of them. `var`, `const` and `let` stay reserved, for an error that points to `mut` and `immut`; `imut` is an ordinary name.
-   **`take` parameters** take ownership of a move-only argument (like `owned` in Mojo or `sink` in Hylo); see [Values and Memory](memory.md#taking-ownership).
-   **`RegexError`** has `InvalidPattern`, `Unsupported` and `TooLarge`.
-   **`range` is lazy** in a `for` loop.
-   **Constraints**: `[T: equatable]` and `[T: orderable]`, names the compiler knows; types never declare them, the compiler decides from their structure (see [Generics](generics.md#constraints)). There are no method requirements, and type parameters cannot be move-only values yet.
-   **Generics**: brackets after the name (`function first[T]`, `type Pair[A, B]`); type arguments are never written at a call, they are inferred from the arguments and the expected type; methods of a generic type see its type parameters (`function Pair.swap(self): Pair[B, A]`) and can add their own (`function Pair.mapFirst[C](...)`). See [Generics](generics.md).
-   **Maps and sets**: `Map[K: equatable, V]` with the literal `['Ana': 30]` (`[:]` when empty), and `Set[T: equatable]` from `Set.from(list)`. Index signatures are gone: a dictionary is a `Map`.
-   **`char`** is a primitive type: a Unicode code point. A one-character literal is a `char` where a `char` is expected; indexing a string and `for c in text` give chars.
-   **Lists**: `push`, `pop`, `insert`, `removeAt`, `sort` (orderable items) and `sortBy(comparison)` change the list like `mut self`; slices `items[1..3]` work on lists and strings.
-   **`system` module**: `args()`, `env(name)`, `exit(code)` and `run(command, args)`, with `SystemError`.
-   **No string builder**: text is built with `+=` (appending in place when the string is not shared, as in Swift) and `list.join(separator)`.
-   **Opaque types** (a type that other modules can use but not build with a literal) are postponed until they are needed.
-   **No anonymous objects**: an object literal needs a known type (see [No `interface`](#no-interface)). Values are built with a literal or with an associated function (`Person.create(...)`); the language has no special constructors.
