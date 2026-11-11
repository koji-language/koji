# Design Decisions

Why Koji is the way it is. Each decision says what Koji does, why, and what was considered instead. The other documents describe how to use the language; this one keeps the reasons, so that later decisions stay consistent with them.

## Principles

Most decisions follow from a few ideas:

-   **Explicit over implicit**: what can change, what can fail and what a function takes over are visible where they happen (`mut`, `Result`, `?`, `take`).
-   **Errors at compile time**: a mistake the compiler can see (a typo, a missing case, a moved value) is a compile error, not a surprise when the program runs.
-   **Errors are values**: expected failures are `Result` values that the code handles; bugs stop the program.
-   **Sum types over strings and flags**: a closed set of cases is a sum type, so `match` checks that every case is handled.
-   **Values, not references**: assigning or passing a value gives an independent copy; nothing changes "behind the back" of a variable.
-   **One way to do each thing**: fewer constructs, each with a clear purpose.

## Variables

### `mut` and `immut`

**Decision**: every declaration starts with `mut` (the variable can change) or `immut` (neither the variable nor its contents can change). `x = 1` only changes an existing `mut` variable.

**Why**: the keyword makes the intent visible, and a typo (`cuont = 1`) is an error instead of a new variable. With value semantics, two levels are enough: changing a field and replacing the whole value are both ways of changing the variable.

**Alternatives**: `var`, `let` and `const`, as in TypeScript, have a third level (a variable that cannot be replaced but whose contents can change), which means little with value semantics. Declaring by assignment, as in Python, hides typos. `var`, `const` and `let` stay reserved words, so the error points to `mut` and `immut`.

### `for` and `match` variables are `immut`

**Decision**: `for x in items` and the variables of a `match` pattern are `immut`; `for mut x in items` allows changing `x` (a copy: the list does not change).

**Why**: changing them is rarely intended, and changing a copy can be mistaken for changing the list.

## Types

### No `any`

**Decision**: there is no `any` type. Dynamic data is a sum type (`json.Json`).

**Why**: every value has a known type at compile time, so no value needs to carry its type when the program runs, and every use of dynamic data is checked.

**Alternatives**: `any` with runtime checks, as in TypeScript, moves type errors to runtime.

### Nominal types, no `interface`, no anonymous objects

**Decision**: `type` declares every type. Shape types are nominal: two types with the same fields are different types. An object literal is a value of the type expected where it is written, and there is no subtyping.

**Why**: `Celsius` and `Fahrenheit` do not mix, and a value always has one known type. Generic constraints are deduced from the structure of each type (`equatable`, `orderable`), and other behavior is passed as a function, so interfaces are not needed.

**Alternatives**: structural types and interfaces, as in TypeScript and Go. `interface` stays a reserved word, as a precaution.

### `int`, `decimal` and `float`

**Decision**: an `int` becomes a `decimal` or a `float` only as a single value (a variable, an argument, a field, the items of a list literal), never inside an existing value: an `int[]` is not a `decimal[]`. `int / int` is an `int` truncated toward zero, and `%` has the sign of the dividend.

**Why**: converting a single value is free; converting a list would copy it without the code saying so. Integer division follows Go and Rust.

### Numbers are always finite

**Decision**: there is no NaN and no infinity, in a `decimal` or a `float`. A result that does not fit (`float(1e308) * float(10)`, `9223372036854775807 + 1`) panics, like a literal that does not fit (`0x1p1024`, `1e7000`) is a compile error. In `math`, a result that does not exist in the real numbers is an `Err(MathError)`: `sqrt(-1)` is `Complex`, `pow(0, -1)` is `Infinite`.

**Why**: NaN spreads silently through computations and breaks equality (`NaN != NaN`). The rule separates two kinds of problems: a **representation limit** (the result exists but does not fit: a larger type would hold it) panics, and a **domain problem** (the result does not exist in the reals) is an error the code can handle. Operators cannot return a `Result`, so they always panic, including on division by zero. `0 ** 0` is `1`, the value of the expression in algebra (only the limit form 0⁰ is indeterminate). `MathError.Indeterminate` is kept for future functions.

**Alternatives**: IEEE 754 infinities and NaN, as in most languages.

### `decimal`

**Decision**: `decimal` is a number with 34 significant digits in base 10 (the precision of IEEE 754 decimal128), computed by the runtime in software, in 16 bytes, and it is the type of a literal with a point or an exponent (`1.5`, `2e3`): exactly the number written. A literal that does not hold exactly (more than 34 significant digits, or out of range) is a compile error. An operation rounds its exact result to 34 digits, half to even. A decimal is shown in full, as in everyday writing: `10`, `1.5`, `2500000`, without trailing zeros (`1.50` equals `1.5`); a whole number is shown in full, like an `int`, up to the 34 digits a decimal holds (beyond them, in scientific notation: `1e300`), and a fraction that needs six or more zeros after the point before its first digit is shown in scientific notation, as science writes it (`1e-7`). It panics when a result does not fit, and also when it is too small to be anything but zero. `**` takes any decimal exponent: an `int` exponent is exact, another is computed with 64 digits. The standard library uses decimals where it does not compute heavily: seconds in `time`, the numbers of `json`, `math.pi` and `math.e`.

**Why**: Koji keeps the precision of the program above the influence of the hardware. A `float` is binary: `0.1` is not one tenth, and `0.1 + 0.2` is `0.30000000000000004` in a float. Precision is not the only reason:

-   **What is written is what is held.** A decimal read from text and written back gives the same text; a float only looks like `0.1` because its shortest decimal is shown.
-   **Equality works.** `0.1 + 0.2 == 0.3`, so comparisons, the keys of a map and the cases of a `match` behave as people expect.
-   **The same result on every machine.** All the arithmetic of a decimal, `sqrt` and `pow` included, is in the runtime of Koji; the functions of a float come from the system's `libm`, whose results can differ between systems.
-   **Data stays as it came.** Numbers from JSON, a file or the user are kept exactly, and a program that reads and writes them back does not change them.
-   **What some domains require.** Money, taxes and accounting round in base 10, often half to even, by rule.
-   **Fewer traps.** A whole kind of bug, common for beginners, disappears, and each conversion to a float is written in the code, like the rest of what Koji does not hide.

The cost is speed, and a `float` is there when it matters. 34 digits hold every `float` exactly (the shortest decimal of a float has at most 17 digits), so converting a float to a decimal and back loses nothing, and a decimal never looks as close as a float would: 16 digits would round `0.30000000000000004` to `0.3`. Measured, the common operations stay fast (an addition with the same exponent or an exact product in about 5 ns; a rounded product about 50 ns and a division about 70 ns, `runtime/bench/decimal.c`): a value is not normalized, so most operations need no division, and it is passed in two registers, never allocated. Half to even does not drift when many results are rounded. Equal values print the same; a number shown in full is the one people read (the exponent of a scientific notation is easy to miss), a whole number reads the same as an `int` of the same value, and scientific notation takes over for the small fractions science writes that way, where the zeros are only placeholders. A whole number with more than 34 digits would end in zeros the decimal does not keep, so its scientific notation shows exactly what is held, without a line of thousands of digits.

**Alternatives**: 16 digits (decimal64), which fits in 8 bytes but is about as precise as a `float`; precision without limit (Java's `BigDecimal`, Python's `Decimal`), whose division needs a precision chosen somewhere (in Python, a global context); rational numbers (Raku), exact for the four operations but whose denominators grow without limit; keeping only `float`.

### `float` is written as it is stored

**Decision**: `float` stays the 64-bit binary type of the processor, for complex computations. It is written and shown in base 16, as it is stored: a mantissa after `0x` and a power of 2 after `p` (`0x1.999999999999ap-4` is the float nearest to 0.1). A float literal must be exact (at most 53 bits, in range). A decimal literal is not a float: `float(0.1)` converts it (computed while compiling), and `decimal(x)` of a float is the shortest decimal that is the same float again, so `decimal(float(0.1) + float(0.2))` is `0.30000000000000004`. A `float` and a `decimal` never mix: `price * rate` with a decimal and a float is an error, and one of them is converted.

**Why**: the value a program shows is the value it holds. A float shown in base 10 looks like the decimal it approximates (`0.1`), hiding the rounding that makes `0.1 + 0.2 != 0.3`; in base 16 it is exact and short, and a hexadecimal literal is what C, Java and Go use for exact floats. Converting explicitly shows where a value is rounded. Floats keep the speed of the processor where it matters (simulations, graphics), and the conventions say which type to use.

**Alternatives**: renaming `float` to `binary` and writing it in base 2 (`0b1.1p1`), exact too but much longer; showing a float as its shortest decimal, as most languages do; letting a decimal literal become a float where a float is expected, which would round without the code saying so.

### `char`

**Decision**: `char` is a primitive type, a Unicode code point. A one-character literal is a `char` where a `char` is expected.

**Why**: text processing (a lexer, a parser) works on characters, and a `char` cannot be confused with a string of any length.

### Maps and sets

**Decision**: `Map[K: equatable, V]`, written `['Ana': 30]` (`[:]` when empty), and `Set[T: equatable]`, from `Set.from(list)`. There are no index signatures.

**Why**: a dictionary with keys of any equatable type, checked like any other generic type.

### Generics

**Decision**: type parameters in brackets after the name (`function first[T]`, `type Pair[A, B]`), with the constraints `equatable` and `orderable`, which the compiler deduces from the structure of each type. Type arguments are never written at a call: they come from the arguments and the expected type. A generic function used as a value needs an expected function type, which gives its type arguments.

**Why**: the same bracket syntax as `Option[T]`, and no declarations to keep in sync with what a type can do.

**Alternatives**: angle brackets, which are ambiguous with comparisons; constraints that types must declare (interfaces, traits).

### `Ok()`

**Decision**: a `Result[void, E]` succeeds with `Ok()`, without an argument; it is shown as `Ok(void)`.

**Why**: there is no `void` value to write.

## Control flow

### No `else if`

**Decision**: `if` has an optional `else` with a block; there is no `else if`. More than two branches are a `match` with guards, or special cases handled first with an early `return`.

**Why**: two rules of thumb, built into the language:

-   **When a condition needs a third branch, it is a case analysis**, and a case analysis reads better as `switch` / `case` than as an `if` / `else if` / `else` chain. Koji's `match` takes that role: with guards, it lays the cases side by side (`x if (x > 18) => 'Adult'`), and with patterns it also checks that every variant is handled.
-   **Early returns should be the natural way to write**: special cases first, each ending with `return` (or `break` and `continue`), and the main code after them, unnested (see [Conventions](conventions.md)). `else if` invites the opposite, a chain where every branch stays nested until the end; without it, the early return is the obvious way to handle a special case.

**Alternatives**: `else if`, as in most languages; a `switch` statement next to `match`, which would be a second construct for the same job. A `match` without a value (`match { x > 18 => ... }`) could make guard chains shorter later.

### Conditions in parentheses

**Decision**: conditions are written in parentheses: `if (x > 0)`, `while (running)`, `match (value)`.

**Why**: the condition is clearly delimited from the block, like the arguments of a call, and `not (a and b)` reads without doubt about what `not` applies to.

## Errors

### No `try` / `catch`

**Decision**: expected failures (a missing file, invalid JSON, `int('abc')`) are `Result` values; bugs (an index out of range, a division by zero, an overflow) **panic**: the program stops, and a panic cannot be caught. `panic(message)` reports impossible states.

**Why**: the signature of a function says whether it can fail, and the compiler checks that the error is handled. Exceptions are invisible in signatures, and catching them makes bugs look like expected failures.

### `?`

**Decision**: `x?` gives the `Ok` (or `Some`) value, or returns the `Err` (or `None`) from the function; at the top level of the main file, it stops the program with the message. When the error type differs from the function's, `?` wraps it in the one variant of the function's error type that holds exactly that type; if there is no such variant, or several, it is a compile error, and `mapErr` converts explicitly.

**Why**: propagating errors stays short without hiding them, and the conversion is automatic only when there is a single possible meaning.

### Typed errors in the standard library

**Decision**: every fallible function of the standard library returns a `Result` with an error type of its module (`FileError`, `HttpError`, `JsonError`...), and `print` shows them as readable messages (`file 'notes.txt' not found`).

**Why**: the code can `match` on what went wrong, and the message is ready for the user.

## Functions and methods

### Methods and `self`

**Decision**: a method is a function prefixed with its type, with an explicit `self`: `function Person.greet(self)`. Without `self`, it is an associated function, called on the type (`Person.create('Ana')`). A method that changes its value declares `mut self`; the call needs no marker. There are no special constructors.

**Why**: methods are ordinary functions, declared next to the type, and what a method may change is in its signature.

**Alternatives**: classes, `this`, constructors: more concepts for the same result.

### `mut` and `take` parameters

**Decision**: a parameter the function may change is `mut`, both in the declaration and at the call (`addItem(mut items, 2)`). A `take` parameter takes ownership of a move-only argument. Other parameters are read-only.

**Why**: with value semantics, a function cannot change the caller's variable unless the call says so, visibly. A `mut` argument is the variable itself: the function gets its value when the call happens, after the other arguments.

### Closures capture a copy

**Decision**: a closure captures a read-only copy of the variables it uses, from the moment it is created. Top-level variables are not captured.

**Why**: no hidden shared state between a closure and the code around it.

## Memory

### Ownership and value semantics

**Decision**: memory is managed with ownership, without a garbage collector. Assigning or passing a value gives an independent copy, which shares the memory until one of the copies changes (copy-on-write, as in Swift). When its owner goes out of scope, a value is dropped. `is` only tests variants; `==` compares values.

**Why**: no aliasing bugs, deterministic memory use, and cheap copies.

**Alternatives**: a garbage collector with shared references (Java, Go); explicit borrowing with lifetimes (Rust), which is precise but harder to learn. Intentionally shared structures (graphs, caches) keep their values in a list and store indices.

### Move-only types and `drop`

**Decision**: a type that declares `function Type.drop(mut self)` is move-only, and so is a type that holds a move-only value (a `File`). Assigning one moves it; using the previous variable is a compile error.

**Why**: a resource must be released exactly once; two copies of a file would close it twice. The property follows from the type's contents, so there is no separate marker to keep in sync.

### No `defer`

**Decision**: there is no `defer`.

**Why**: dropping a value releases its resources, at the end of its scope, deterministically. It would be reconsidered only for cleanup that does not belong to any value.

## Modules

### Mandatory `export`

**Decision**: everything in a module is private unless marked with `export`. An exported `mut` variable is read-only outside its module, and importers see its current value. A public signature cannot use a private type. Exporting a sum type exports its variants.

**Why**: a module decides what it offers, and only its own code changes its state.

### Imports belong to their block

**Decision**: an `import` applies to its whole block, before the other statements (like ES modules); an `import` inside a block is visible only there, and its module runs the first time the block runs.

**Why**: declarations can use an import written below them, and a module used only in one place is loaded only if that place runs.

## Standard library

### File modes

**Decision**: `file.open(path, mode)` takes a sum type, `Read`, `Write` or `Append`.

**Why**: an invalid mode does not compile, unlike a string such as `'rw'`.

### `printError`

**Decision**: a built-in `printError(...)` writes to the standard error output, with the same arguments as `print`: the values shown as their `display()` text, separated by spaces, and a line break.

**Why**: `print` and `printError` are a symmetric pair, and reporting an error is as basic as printing, so it needs no import. The name is in full words, like the rest of the language.

**Alternatives**: `system.printError(...)`, which needs an import for something as basic as `print`; values for the standard outputs (`file.stderr.write(...)`), which would be shared move-only values, and whose `write` returns a `Result` that `print` does not; the abbreviation `eprint`.

### `file.remove`

**Decision**: `file.remove(path)` removes a file and returns a `Result[void, FileError]`; it does not remove a directory (`Err(IsDirectory(path))`).

**Why**: removing a file is the common case, and a separate function for directories (a `removeDir`, when a program needs it) means a directory is never removed by mistake.

**Alternatives**: one `remove` for files and directories.

### `system.os`

**Decision**: `system.os` is a constant of the sum type `OperatingSystem = { Linux, MacOS, Windows, Other(name: string) }`.

**Why**: a `match` on it is checked: every system is handled, or there is a `_` arm. `Other` keeps the list open for systems without a variant. It is a constant, like `math.pi`, since it never changes while a program runs.

**Alternatives**: a string (`'linux'`), which a typo (`'macOS'`) gets past the compiler.

### `system.run` and `system.capture`

**Decision**: `system.run(command, args)` runs a program with the terminal of this one (the same input and outputs) and gives its exit code; `system.capture(command, args)` runs it and keeps what it prints, as an `Output`.

**Why**: the two differ in what happens to the program's output, and each name says it: `run` runs it as if it were typed in the terminal, the common case; `capture` keeps the output. They are verbs, like the other functions that do something (`open`, `remove`, `exit`), while the nouns of `system` (`args`, `env`, `os`) only read the state of the process.

**Alternatives**: `system.output(...)` for capturing (the name of the type it gives, and of Rust's `Command::output`), a noun that would hide that it runs a program; keeping `run` for capturing with a new `execute` or `call` for the terminal, names whose difference from `run` is not visible; a mode argument (`run(command, args, Terminal)`), which would make the result type depend on the mode.

### `system.executable`

**Decision**: `system.executable()` gives the path of the executable of the running program, as an `Option[string]` (`None` where the system cannot tell it). `kojic` uses it to find the runtime library next to itself.

**Why**: the name says what it gives, a file, like `path.parent` and `path.name`. It is a noun, like `args` and `env`, since it only reads the state of the process; a function, not a constant like `system.os`, since finding it asks the system.

**Alternatives**: `system.current()`, which reads as the current directory, as `current_dir` in Rust and `getcwd` in Python; `system.programPath()`, longer; `system.self()`, vague.

## Conventions

### Special cases first

**Decision**: the convention is to handle special cases first, with an early `return` (or `break` and `continue`), so the main code is not nested inside `if` and `else`.

**Why**: the main path reads from top to bottom at one level, and each special case is visible where it ends.

### One line break at the end of a file

**Decision**: a file ends with exactly one line break.

**Why**: a text file is a sequence of terminated lines, as Unix tools expect: without the final line break, `cat` joins the last line with the next output and `git diff` reports it; extra blank lines are noise.

## Tooling

### `kojic run` and `kojic build`

**Decision**: `kojic build` compiles a program to an executable, through C, linked with a runtime library (written in Koji: see below); `kojic run` compiles it to a temporary executable and runs it. There is no interpreter.

**Why**: one implementation of the language, so a program behaves the same in development and in production. Generating C needs no code generator library, works wherever there is a C compiler, gets the optimizations of gcc and clang, and the generated code is readable.

**Alternatives**: an interpreter for `kojic run`, which starts faster but is a second implementation of every feature that must give the same results (Koji had one, in Rust, until the compiler written in Koji replaced it); a native backend (LLVM, Cranelift, or machine code directly), which can come later from the same intermediate representation.

### The compiler is written in Koji, from a seed in C

**Decision**: `kojic` is written in Koji (`compiler/`). It is built from the seed, the C it generated for itself (`bootstrap/kojic.c.gz`): a C compiler builds stage 1 from the seed, and stage 1 builds the compiler of `compiler/` (`bootstrap/build.sh`). The seed is written again when the compiler needs a feature the seed does not compile.

**Why**: the compiler is the largest Koji program, and writing it in Koji tests the language and shows what it lacks. A seed in C needs only a C compiler to build Koji, on any system, and the compiler can use any feature of the language once the seed is written again with it.

**Alternatives**: keeping the compiler in Rust, a second implementation to maintain along with the one in Koji; a chain of earlier compilers to build from (like Go, which needs an earlier Go), which needs an executable for each system; a seed in the intermediate representation, which would need an interpreter of it.

### A runtime of its own, written in Koji

**Decision**: the runtime and the standard library are written in Koji, not in another language. What Koji cannot express yet (raw memory, calls to the operating system) comes from an internal module, `koji:unsafe`, with an address type, reads and writes of bytes and words, and `extern function` declarations of C functions of the system's library; only the standard library imports it at first. Its modules are in `std/`, and the compiler carries them (`compiler/stdlib.kj`, generated from `std/`): `import json` compiles `std/json.kj` like any module. HTTPS uses a TLS written in Koji. What stays outside Koji: the system's C library, as for nearly every language, and the C that the compiler generates (its target, not code written by hand); if a small part can only be written in another language, it is Rust. The runtime in Rust shrinks as modules move to Koji, each one with the same behavior, the same tests, and no loss of speed.

**Why**: a serious language should not need another one to run its programs: the standard library is then Koji's to read, change and optimize, and Koji is tested on it. The low-level part stays small and out of the users' sight, as `unsafe` does in Go, so the language stays safe.

**Alternatives**: the runtime in Rust (as it was), which needs cargo and libraries of others; the runtime in C, which is not Koji either; TLS from the system (OpenSSL), as Python or Node do, which depends on a library in C.

### `byte`

**Decision**: `byte` is a primitive type, an unsigned integer of 8 bits (0 to 255), for binary data (`byte[]`) and memory. An integer literal is a byte where a byte is expected; bytes go with bytes, and convert with `byte(n)` (which panics outside 0 to 255) and `int(b)`. Arithmetic panics when the result is not a byte, like that of ints; the bits stay in 8 bits.

**Why**: binary data (files, the network, memory) is made of bytes, and Koji had no type for them: an `int` is 8 bytes. As `char` is not a number but a character, `byte` is a number of its own size.

**Alternatives**: a byte as an `int` from 0 to 255 (no type: `readByte` and `writeByte` in `koji:unsafe`, and lists of ints 8 times as large); arithmetic that wraps around (`255 + 1` is `0`), which hides mistakes, as for ints.

### The epoch of Koji is 0000-01-01

**Decision**: a timestamp counts the seconds since 0000-01-01 00:00:00 UTC, in the proleptic Gregorian calendar (where the year before 1 is 0, as in ISO 8601): `time.now()`, `datetime.format` and `datetime.parse` use it. A timestamp of Unix (since 1970-01-01), which files, HTTP, most APIs and other languages use, is 62,167,219,200 seconds (719,528 days) less.

**Why**: the dates of history, from the year 0, have positive timestamps, and the epoch is the start of the calendar rather than an arbitrary date.

**Alternatives**: the epoch of Unix, which nearly every system uses, so that a timestamp needs no conversion when it is exchanged (the cost of this decision: every exchange adds or subtracts the difference); a date type without a timestamp.

### `koji:unsafe`: the lowest level of the standard library

**Decision**: a module of the standard library may import `koji:unsafe`, which has `Address` (a place in memory, of its own type), `CInt` (the 32-bit `int` of C, for `extern` declarations only: an `int` in Koji), `CSize` (the `size_t` of C, likewise), `U128` (an unsigned integer of 128 bits, `unsigned __int128` in C: `+`, `-` and `*` wrap around, `/`, `%` and the shifts beyond 127 panic, the comparisons are unsigned, a literal is one where a `U128` is expected; `wide(low, high)`, `lowWord`, `highWord` and `leadingZeros`; `decimalLow`, `decimalHigh` and `decimalOf` give the words of a decimal and back: what the decimals in Koji are made of), `Location` (the place of a call, `main.kj:12`, for panics: a last parameter of this type is given by the compiler, so `f.read()` calls `File.read(f, here)`), `Never` (the type of a function that does not return: `exit` of C, a function for C that ends the program), `locationAddress` (the text of a `Location`, as C has it), `offset`, `distance`, `nullAddress` and `isNull` (arithmetic of addresses, and the address 0), `stringBytes`, `stringSize` and `newString` (the UTF-8 bytes of a string, and a string from bytes; the address of the bytes does not keep the string alive, which is dropped after its last use, so a function that reads them takes the string), `blankString`, `stringReferences`, `stringCapacity` and `setStringSize` (a string to fill, and growing one in place), `reserve`, `listItems` and `setListLength` (room for more items in a list that is not shared, the address of its items, a word each, and how many of them were written), `allocate`, `free`, `read` and `write` (of an `int`, a `float`, a `boolean`, a `char`, a `byte` or an `Address`, with its size: `immut n: int = read(address, 8)`, the type read being the one expected) and `panicAt(here, message)` (a panic at that place); and it may declare `extern function name(...): type`, a function of the system's C library or of the runtime, with values of C only: `int`, `CInt`, `CSize`, `float`, `decimal`, `boolean`, `char`, `byte`, `Address` and `Location`, a `U128` or a `string` it reads (a `string` it returns is a new reference), and `Never` for a function that does not return. Other programs can use neither.

**Why**: the standard library is written in Koji, but some of it must reach the system (clocks, files, processes) or memory; this keeps that part small, visible and out of the users' sight, as `unsafe` does in Go.

**Alternatives**: these functions in the runtime in Rust (as they were); an FFI for every program, postponed until the language needs it.

### `math.abs` of several types: the compiler chooses the function

**Decision**: `abs`, `sqrt`, `pow`, `floor`, `ceil` and `round` of `math` take an `int`, a `decimal` or a `float` (as they always did). `math` is written in Koji (`std/math.kj`), with a private function for each type (`absInt`, `absDecimal`, `absFloat`...); the compiler checks a call by the rules of the function (the same types and messages as before: `math.pow` does not mix a float and a decimal) and calls the function for the types of its arguments, with the `Location` of the call as its last argument, so that its panics name the line of the program (`main.kj:3: integer overflow`), not one of `std/math.kj`.

`system.exit` is checked by the compiler in the same way, since a function of Koji cannot say that it never returns: it calls `leave` of `std/system.kj`, and the code after it is unreachable.

**Why**: the module moves to Koji without changing for its users, and overloading stays out of the language: six functions do not justify it.

**Alternatives**: overloading of functions in the language (rules for choosing, for errors and for functions as values, for six functions); generic functions with a constraint for numbers, which Koji does not have; one function for each type in the API (`math.absFloat`), heavier for every user.

### Built-in methods written in Koji: `koji:core`

**Decision**: methods of the built-in types can be written in Koji, in the internal module `koji:core` (`std/koji/core.kj`), which programs cannot import. The checker keeps their signatures; a call (`text.trim()`) is one of the function of the same name, with the value first (`trim(text)`) and, for a method that may panic, the `Location` of the call last. The module is checked only when a program uses one of them, and it has no top-level variables (it is never initialized). First the methods of strings that need no table of Unicode: `trim`, `split`, `replace`, `startsWith`, `endsWith` and `indexOf`, on the bytes of the string (`stringBytes`, `stringSize` and `newString` in `koji:unsafe`), with `memcmp` and `memmem` of the C library (linear searches).

The modules of the standard library may import `koji:core` too (`system` uses `textOf`, which reads bytes from the system as UTF-8, each invalid sequence becoming U+FFFD as Rust's `from_utf8_lossy` does).

**Why**: the runtime moves to Koji one function at a time, without changing the language or its messages: the same mechanism as `math`, for methods.

**Alternatives**: functions of Koji compiled under the C names of the runtime, which the generated code would call as it calls the runtime (the conventions of references of the runtime and of Koji would have to match at that border); a module of Koji imported by every program (compiled for each program, even one that never uses it).

### The library of the runtime written in Koji

**Decision**: the part of the runtime written in Koji that C calls (the prelude, the generated code) is a library: the internal modules of `std/koji/` but `koji:core`, compiled once (`kojic dump library` gives their C, kept in `bootstrap/libkoji.c.gz` next to the seed) into the archive of the runtime (`libkoji_runtime.a`), which every program links; since the runtime in Rust is gone, it is all of the runtime. Its functions are declared `extern function name(...) { ... }`: a function of Koji, given to C under its name (with values of C only, as for `extern` declarations). The library has no `main`: `koji_library_init` runs its modules, and the runtime calls it first. A program cannot import its modules: it calls their functions by their names of C. A function for C reads the strings it is given; one it declares `take` is given by C, and released after the call (a function of Koji does not take the reference of a string it is given, and shares the one it returns). `test/stages.rs` checks that `bootstrap/libkoji.c.gz` is up to date (`KOJI_UPDATE_LIBRARY=1` writes it again). First `koji_decimal_neg`, `koji_decimal_abs` and `koji_decimal_from_int`; the rest of the decimals follow. Its modules are compiled together, so one may import another (`koji:list` uses `mixed` of `koji:text`). A function for C may return `Never` (`koji_std_panic`): C sees it `_Noreturn`. The functions that C gives the library, such as those of the type of the items of a list (`KojiElementType`: retain, release, equal...), are called through inline functions of the prelude whose types are those that `extern` declarations of Koji give (`koji_element_equal(const void *, const void *, const void *)`). The tables of Unicode of `upper`, `lower` and the `is...` methods (`std/koji/unicode.kj`) are written by `bootstrap/unicode.rs` from the methods of `char` of Rust, so that the methods give what they gave when the runtime was in Rust (the alternative, reading the files of the Unicode Character Database, would need its own checks against Rust's results). Likewise, the tables of powers of 5 with which `koji:decimal` finds the shortest decimal of a float (Ryu, `std/koji/ryu.kj`) are written by `bootstrap/ryu.rs`.

**Why**: as the runtime of other languages, it is compiled once, not with each program, and the prelude calls it as the generated code does. Its C is in `bootstrap/` for the same reason as the seed: building Koji needs no Koji compiler.

**Alternatives**: the functions compiled into each program (as `koji:core`), which makes every compilation slower as the runtime moves; a library built by the tests and `bootstrap/build.sh` with the compiler just built (the seed would need a library before the compiler that makes it).

### Messages name files as they were written

**Decision**: type errors and panics name the main file as it was given on the command line, and a module as its `import` wrote it (`lib/math.kj:3: division by zero`).

**Why**: the same names everywhere, which the reader recognizes from the code.

### HTTPS: TLS 1.3 only, written in Koji

**Decision**: `https://` uses a client of TLS written in Koji (`koji:tls`, `koji:x509`, the cryptography of `std/koji/`), and only TLS 1.3. Its certificate authorities are those of the system (`SSL_CERT_FILE`, `SSL_CERT_DIR`, the usual files of Linux, macOS and the BSDs), with Mozilla's list built in for the systems that have none, as Go does. Its ciphers are AES-GCM with the instructions of the processor (AES-NI and PCLMULQDQ, or those of ARMv8, the only C of the cryptography, in the prelude) when it has them, and ChaCha20-Poly1305 otherwise, as Go, BoringSSL and rustls choose; never AES by tables. The end of the data is close_notify only: a connection closed without it is reported as maybe truncated, as rustls reports it, and `std/http.kj` ignores that after a body whose size it knows. A server without TLS 1.3 is refused with a message that says so.

**Why**: TLS 1.3 is what most servers speak, and what has the least to get wrong (no renegotiation, no CBC, no exchange of keys by RSA, the whole handshake after the ServerHello encrypted): one version, a smaller client and a smaller surface of attack. The cost is known: a few servers have only TLS 1.2 (in 2026, `registry.npmjs.org` and `registry.yarnpkg.com`), which the client in Rust (`ureq`) reached and which Koji does not.

**Alternatives**: TLS 1.2 too, in the modern subset rustls keeps (ECDHE, AEAD ciphers, Extended Master Secret, no renegotiation), which reaches every server but doubles the protocol to keep right; the TLS of the system (OpenSSL, Secure Transport), as Python and Node use it, which ties Koji to a library in C.

### Tests: `test` blocks next to the code, one `assert`

**Decision**: a test is a block `test 'name' { ... }` at the top level of a file, next to the code it tests, which it sees whole (private functions and types included). `test` is a word only there (followed by a string, at the top level): elsewhere it stays a name. A test is checked like a function and left out of a program; `kojic test` compiles each file with its tests and runs each test in a process of its own, after the top-level statements of the file, showing what a failing test printed. A test fails when it panics. The one function for checking is the built-in `assert(condition)` or `assert(condition, message)`: the compiler keeps the condition as written, and for a comparison the values of its two sides, shown when it fails (`assertion failed: total == 10`, `left: 9`, `right: 10`); a program's own `assert` takes its place.

**Why**: a test is written where its code is, with nothing to import or name (Zig's `test` blocks, Rust's tests in the same file), and can test what is private. One `assert` that explains its failure, as pytest does with Python's `assert`, says as much as a family of `assertEqual`, `assertTrue`... with nothing to learn. A process for each test gives isolation without catching panics, which Koji does not do: a test cannot break the next one, and the global state starts again. A contextual word breaks no program that uses `test` as a name, and the compiler itself does.

**Alternatives**: test functions in files of their own (`math_test.kj`, as Go), which see only what is exported; a `testing` module with `test(name, () => { ... })` (as Deno), whose closures could not change the variables around them; several assertion functions (`assertEqual`, as JUnit or unittest); all the tests of a file in one process, faster but with no isolation after a panic.


### Errors show their column and the line they are on

**Decision**: a compile error gives the file, the line and the column of what is wrong, then that line with it marked: `type error: hello.kj:5:13: argument 1 of 'greet' must be string, found int`, then `5 | print(greet(42))` and `^^` under `42`. The mark is under the value, the argument or the element at fault, not the whole statement (the parser gives each node where it starts and ends). A panic keeps its form, `panic: main.kj:12: message`: the program running has the line only.

**Why**: a beginner, and the editor, see at once where the error is, as with gcc, rustc and Elm; `file:line:column:` is what editors and terminals turn into a link.

**Alternatives**: the line only (as before), which leaves the reader to find the place in it; a long explanation with each error (as Elm), more work for each message, for later.

The check goes on after an error, statement by statement, and reports every error, up to 20 (`3 errors` at the end). A variable whose declaration failed is declared all the same, with its written type or an unknown one that no later use reports, so that one mistake gives one error, not a cascade; an imported module with errors stops the check, since every name taken from it would be an error too. Syntax errors stop at the first: the parser cannot know where the program goes on.

### The language server is a command of the compiler

**Decision**: `kojic lsp` is the language server (the Language Server Protocol, on the standard input and output), written in Koji in the compiler (`compiler/lsp.kj`): it checks the text the editor holds, saved or not, with the checker of the compiler, and gives every error as it is edited (the line, the column and the width of each), the type of what is under the cursor (from the types the checker kept, even when the file has errors elsewhere) and where a name is declared (a variable, a parameter, a function, a type, in the file or in the module it is imported from). The editor extension of VS Code (`editors/vscode`) runs it.

**Why**: the editor sees exactly what `kojic check` sees, with no second implementation of the language to keep in step; one program to install. Checking a whole file at each change is fast enough for the files of Koji (the compiler checks its own 15,000 lines in 0.08 s).

**Alternatives**: a server of its own, apart from the compiler (as rust-analyzer), which can be incremental but repeats the language; a server in another language (TypeScript), which would need the compiler anyway.


### Concurrency: light tasks in groups, on one thread

**Decision**: concurrency is the `task` module: a task runs a function on a stack of its own (1 MB the system gives as it is used, with a guard page below), and code in a task is ordinary code, with no `async` or `await`. In this first phase, the tasks take turns on the one thread of the program: a task runs until it waits (for a timer, the network, another task) or has run long enough, and the runtime (`koji:task`, written in Koji, with the switch between stacks and the poller of the system in the prelude: epoll on Linux, kqueue on macOS and the BSDs) then runs the next one, and waits for the timers and sockets when none is ready. Tasks are structured: each belongs to a group (`task.group`), which waits for all of its tasks before it returns, so no task outlives the code that started it. Tasks share data through channels (`task.channel`, with a capacity, 0 for a meeting) and shared values (`task.shared`, whose `update` no other task interleaves); everything else a task uses is a copy, as in any closure. A program where every task waits for another stops with a panic that says so.

**Why**: Koji is for scripts, CLIs, courses and small services: sending requests at the same time, and later serving them, is what they need, and lightweight tasks with sequential code do it without dividing functions into two colors (as `async` does in JavaScript, Rust and Python), as Go and Erlang show. One thread first means no data races at all, and the runtime stays as it is (counts of references that are not atomic, the caches of the library): code that does not wait is never interrupted. Structured groups (as Swift, Kotlin and Trio have them) make a task's life visible in the code: no task is lost or left running, and an error has a place to go. Shared values cover the simple case (a counter, a cache), channels the coordination of tasks.

**Alternatives**: `async` and `await` (JavaScript, Rust, Python), which color functions; threads of the system (Java before 21, C), heavy and with data races; tasks over several threads at once (Go), the second phase, which needs atomic counts of references and a runtime safe for threads; tasks without groups (Go's `go`), which can be forgotten.

The value of `Shared` is the one reference of Koji, which its copies share: the cells of `std/task.kj` are lists of one item that only `sharedGet` and `sharedSet` of `koji:unsafe` read and change in place. The library is compiled without the shadow stacks of x86-64 (CET), which forbid switching stacks.

### No call blocks the other tasks, and a task that computes is preempted

**Decision**: the calls that block (reading and writing files, directories, `input()`, waiting for a program, finding the addresses of a host, printing to a pipe that is full or a long text) are made by threads that help, while other tasks are alive: the task waits for its call as for a socket, and the threads run C only, never Koji, taking jobs from a pipe and giving them back through another that the poller watches (no locks). A task alone makes its calls itself, as before; with others alive, `print` writes at once a text of up to 4 KB to an output that takes it without waiting (`poll`, which a pipe then takes whole), and the tasks print one at a time, so their texts neither mix nor change order. A task that computes without waiting is preempted: the generated code checks a flag at the end of each turn of a loop and where a function that calls others starts, which a thread sets every 10 ms while several tasks are alive; the task then lets the others run (and the scheduler looks at the sockets and timers), and between two checks, nothing changes for it. Neither the library of the runtime nor the module `task` has the checks, so their steps stay whole. There is no `yield` for a program to call.

**Why**: a task never holds the others up, whatever it does: a server stays responsive with a handler that reads a large file or computes for a second, and nobody has to ask how often to give way, which is the runtime's job, as dropping values is. The checks are the cheapest way to preempt (a load and a branch, lost in the noise of the benchmarks), and they come where the generated code is in a known state, unlike preemption by signals. Threads that only run calls of C keep the runtime without data races (as libuv does for Node). The word `yield` stays free for generators, which produce values and have nothing to do with tasks.

**Alternatives**: `task.yield()` (as Go's `runtime.Gosched`, Kotlin's `yield`), which leaves the question to the programmer; preemption by signals at any instruction (Go since 1.14), which needs the stack maps of a garbage collector; asynchronous system calls (io_uring), for Linux only; a resolver of DNS written in Koji (as Go has), which misses what the system configures.

### Anonymous functions take their parameter types from where they are written

**Decision**: a parameter of an anonymous function written without a type has the one of the function type expected where it is written (`task.group((tasks) => ...)`, `immut f: (int) => int = (x) => x * 2`); with no such type, it needs one.

**Why**: the type is already written once, in the signature; writing it again at each call is noise, the more so for the functions given to tasks. It is what TypeScript, Kotlin and Swift do.

**Alternatives**: always writing the types (as before), clearer for a reader without an editor but heavy; inferring them from the body too (as ML), which gives errors far from their cause.

### The HTTP server: a handler from requests to responses, a task for each connection

**Decision**: `http.serve(port, handler)` serves HTTP/1.1: the handler is a function from a `Request` (method, decoded path, query and headers by lower-case name, body) to a `Response` (the type the client returns, with its headers), built by hand or by `http.text`, `http.html` and `http.json`. Each connection is a task of the group of `serve`, kept open for further requests (keep-alive, pipelining) until the client closes it, asks to, or leaves it idle 60 seconds; a request has 30 seconds to come. The server protects itself on its own: 400 for a request with both a length and chunks (smuggling), for white space before the colon of a header and for folded headers; 413, 431, 501 and 505; header values of the handler with a line break have it replaced, so they cannot add headers. It listens on every IPv4 address. A panic in a handler ends the program.

**Why**: a function from a request to a response is the smallest server to learn, and the whole of what small services need (as Go's `http.HandleFunc` or Python's WSGI, without the objects); with tasks, the handler is ordinary code that may wait without holding the others up. The protections and the limits are the defaults that a beginner would not know to add.

**Alternatives**: a router and middleware in the module (as Express), which belong in a library on top; a handler that writes to a stream (Go's `ResponseWriter`), for streaming bodies, later; recovering from a panic in a handler with a 500 (as Go), which Koji, without catching panics, does not do.
