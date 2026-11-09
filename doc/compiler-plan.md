# Compiler Plan

This document plans the path from the current implementation to a **self-hosted compiler**: a Koji compiler written in Koji that produces native binaries. It records the language decisions already made and the ones still open.

> Status: the compiler is self-hosted (phases 1 to 5 are done): `kojic` is written in Koji and built from a seed in C. The compiler and the interpreter in Rust were removed; the next phase is [optimization](#optimizations).

---

## Where we are

The compiler, `kojic`, is written in Koji (`compiler/`, one file per stage). `kojic build <file>` runs:

1. **Lexer** (`compiler/lexer.kj`): source text → tokens.
2. **Parser** (`compiler/parser.kj`): tokens → AST.
3. **Type checker** (`compiler/checker.kj`, `compiler/types.kj`): checks the whole program, including imported modules, and records what it finds about each node (types, moves, drops).
4. **Lowering** (`compiler/lower.kj`, `compiler/ir.kj`): AST → IR, a typed intermediate representation with basic blocks.
5. **C generator** (`compiler/cgen.kj`, with the declarations of the runtime in `compiler/prelude.kj`): IR → one C file.
6. **C compiler** (`cc`): C file + the runtime library → executable.

`kojic run <file>` does the same in a temporary file and runs it; there is no interpreter.

The **runtime** (`runtime/`) is a Rust library with a C interface (declared in `runtime/prelude.h`): strings, lists, maps, output, panics, the standard library and the operators that need more than a C expression. It is compiled once as a static library and linked into every program.

The compiler is built from the **seed** (`bootstrap/kojic.c.gz`), the C it generated for itself: `bootstrap/build.sh` compiles the seed (stage 1), and stage 1 builds the compiler of `compiler/`. The tests (`test/`, run by `cargo test`) build it the same way and run it as a command.

The compiler was first written in Rust (phases 1 and 2), then in Koji, compared stage by stage with the one in Rust (phase 3), until it compiled itself (phase 4) and the one in Rust was removed (phase 5). The sections below keep the record of those phases.

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

The decisions of the language, with their reasons and the alternatives considered, are in [Design Decisions](design.md). The way the compiler implements them is in the documents of each topic ([Values and Memory](memory.md), [Error Handling](error-handling.md), [Modules and Imports](modules-and-imports.md)...).

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
-   **Memory management**: ownership with value semantics, copy-on-write and drop (see [Memory management](design.md#ownership-and-value-semantics)).

---

## The compiler today

`kojic build main.kj` writes the executable `main` (`-o` chooses another name). `--emit c` writes the generated C instead, and `--emit ir` the IR as text. The C compiler is `cc`, or the one named by the `KOJI_CC` environment variable.

It compiles a first subset of the language:

-   `int`, `float`, `boolean`, `char` and `string` values (also indexing and slicing strings, `for` over the characters of a string, `chars`, `code` and `char(n)`), `mut` and `immut` variables, top-level variables used by functions;
-   object types and sum types (also generic, such as `Option[int]`, and recursive), with object literals, constructors, fields (reading and assigning, also `people[0].age += 1`), `==`, `match` (variants, literals, bindings, guards, as a statement or an expression), `is` and the narrowing it gives (`if (x is Some) { x.value }`);
-   `Option` and `Result`: `?` (with the automatic wrapping of the error, and stopping the program at the top level), `mapErr` with a variant (`mapErr(Parse)`), and the list methods that give them (`pop`, `removeAt`, `indexOf`);
-   closures: lambdas, which capture a copy of the local variables they use (top-level variables are read directly), named functions used as values, function types (`(int) => int`), calls of any expression that gives a function (`makeAdder(5)(1)`), `sortBy` and `mapErr` with a function;
-   generic functions used as values where a function type is expected (`immut f: (int[]) => Option[int] = first`);
-   imports of other files, also inside a function or a block (the module runs the first time the block runs) (`import calc from 'calc.kj'` and `import { add } from 'calc.kj'`), with their functions, variables (read as they change), types, methods and generics; each module runs once, before the module that imports it, as in the interpreter;
-   `Map` and `Set`: literals (`['Ana': 30]`, `[:]`), `Set.from`, reading and assigning by key (also `groups['even'].push(n)`), `in`, `==`, `for` over the keys or items, `get`, `has`, `remove`, `add`, `keys`, `values` and `length`; any equatable value can be a key, kept in the order it was added;
-   lists of all those values: literals, indexing and assigning items (also `grid[0][1] = 5`), slices, `+`, `==`, `in`, `for` over a list, `length`, `push`, `insert`, `sort` and `join`;
-   every operator on those types, with the same panics as the interpreter (overflow, division by zero, `float` overflow, invalid shifts and powers);
-   string interpolation, `print`, `string(x)`, `int(x)` and `float(x)` (also from a string, as a `Result`), `input`, `panic`, `in` on strings, and the string methods (`trim`, `upper`, `lower`, the `is...` tests, `split`, `replace`, `startsWith`, `endsWith`, `indexOf`, `length`);
-   every module of the standard library: `math`, `time`, `path`, `system`, `file` (files are move-only: closed by `close()` or when dropped), `datetime`, `json`, `regex` and `http`, imported whole or by name; their errors show the same readable messages as in the interpreter;
-   functions with parameters and a return value, recursion, `mut` parameters (`reset(mut count)`) and `take` parameters;
-   move-only values and `drop`: a value is dropped at the end of its block (also when leaving it with `return`, `break`, `continue` or `?`), when its variable gets a new value, after a temporary is used, and at the end of the program, last declared first; a move leaves the variable empty, so it is not dropped twice; dropping runs the type's `drop` method, then drops what the value holds (fields, items of lists);
-   methods, `mut self` and associated functions (`Point.origin()`);
-   generic functions and methods of generic types (also with their own type parameters, `Pair.mapFirst[C]`), with one instance for each set of type arguments they are called with (monomorphization), and the constraints `equatable` and `orderable`; a type's `display` method in `print`, interpolation and `string(x)`, and the built-in `display()` and `debug()`;
-   `if` (also as an expression), `while`, `for` over `range`, `break`, `continue` and `return`.

The whole language compiles; a construct the compiler could not handle would be reported as `compile error: ... is not supported by 'kojic build' yet`. The tests in `test/compiler.rs` compile programs and check that the executable behaves exactly like the interpreter: same output, same panic message and the same exit code.

**Memory**: strings, lists and records (the values of declared types) are reference-counted. A variable owns one reference, and copying a value only adds a reference (value semantics with copy-on-write); changing a list that is shared copies it first, so the other variables never see the change. Assigning a variable releases its old value, and a function releases its values when it returns. Each item of a list and each field of a record takes an 8-byte slot, and the generated code describes their types to the runtime (how to retain, release, show, compare and order them). A generic type gets one description for each set of type arguments (`Option[int]`, `Option[string]`). Maps find their keys with a hash, which the description of each type also gives.

A `mut` parameter is passed by value and its final value is written back to the caller's variable when the function returns, as in the interpreter: the C function gets a pointer for it (`out<n>`).

A closure is the function's code and a record with the values it captured; its code takes that record as a first, hidden argument. A named function used as a value is called through a small function that takes the (empty) record.

The tests run every compiled program under valgrind when it is installed, so a memory error fails them even when the output is right.

The checker records the type arguments it infers for each call of a generic function; inside a generic body, a type parameter is replaced by the instance's type argument.

Each module has its own names: two files can declare a function with the same name. A function takes the file of its module for the messages of its panics. Function signatures come from the types the checker gave them, so a type is never looked up by its name.

Dropping a type is a generated function: it calls the `drop` method (`mut self`, so it may change the value), then the functions that drop each field or item that needs it. The checker says where values are declared, moved, replaced and used as temporaries; the compiler emits the drops at those points.

The standard library is in the runtime: the generated code calls a Rust function for each library function, giving it the description of the `Option`, `Result` or error it must build. The messages of errors, the conversions of text (`int('42')`), dates, the JSON parser, regular expressions and HTTP requests are the same code in the interpreter and in the runtime.

---

## The compiler in Koji (phase 3)

The compiler written in Koji is in `compiler/`, one file per stage; `compiler/main.kj` is its entry point, with the commands of `kojic` (`run`, `check`, `build`, `dump`, `help`); there is no interpreter in Koji, so `run` compiles the program to a temporary executable and runs it. It runs with `kojic run compiler/main.kj ...`, and `kojic build` compiles it to an executable (the first step of the bootstrap). To build an executable, it needs the runtime library, next to `kojic` or in `KOJI_RUNTIME`, and a C compiler (`KOJI_CC`, by default `cc`).

Each stage prints the same text as a `kojic dump <stage>` command of the compiler in Rust, and `test/bootstrap.rs` compares them on the examples, the compiler's own files, every code block of the documentation and edge cases (`test/lexer/`, the syntax errors listed in `test/bootstrap.rs`, the programs and modules of `test/checker/`, and the programs of `test/ir/cases.txt`). The compiled Koji compiler is checked on all of them, and the interpreted one on a few.

| Stage | File | Compared with | Status |
| --- | --- | --- | --- |
| Lexer | `compiler/lexer.kj` | `kojic dump tokens` (`1:5 Identifier count`) | done |
| Parser | `compiler/parser.kj` | `kojic dump ast` (`(at 1 (immut x _ (int 1)))`; an error is `error: syntax error: ...`) | done |
| Checker | `compiler/types.kj`, `compiler/checker.kj` | `kojic dump types` (the syntax tree with the type of each node and what the compiler needs to know about it: `(int 1):int`, `!move`, `!method=Person.greet`...) | done |
| IR | `compiler/ir.kj`, `compiler/lower.kj` | `kojic dump ir` (the text of `kojic build --emit ir`; what `kojic build` does not support yet is the same `compile error`) | done |
| C generator | `compiler/cgen.kj` (and `compiler/prelude.kj`, the lines of `src/backend/c/prelude.h`) | `kojic dump c` (the text of `kojic build --emit c`) | done |

Notes for writing the compiler in Koji:

-   Koji has no `else if`: a chain of conditions is a `match` with guards, or an early `if` with `return` or `break`.
-   Characters are classified through strings (`string(c).isAlpha()`), and `c >= '0'` compares chars.
-   There are no tuples: a function that gives several values returns a small record (`IndexParts` in the parser).
-   The syntax tree in Koji gives each node an `id`: the checker records what it finds about a node by its id (the checker in Rust uses the node's address).
-   Errors stop the whole check, so a function that fails with `?` does not need to undo what it changed (a scope it pushed, for instance).
-   Messages name the main file as it was given and a module as its `import` wrote it (`lib/math.kj`), in type errors, panics and compiled code alike: the compiler in Koji has no way to find a file's full path.
-   `function`, `type`, `import`, `from` and the other keywords cannot name a variable or a parameter (`runner`, `ownerName`...), and `display` and `debug` are reserved method names.
-   Importing a sum type imports its variants too (`import { Token } from 'lexer.kj'` brings `Identifier`, `End`...); naming them as well is an error.

---

## Phases

| Phase | Delivers | Validation |
| --- | --- | --- |
| **0. Decisions** | The open decisions below, and the documentation updated | Documentation reviewed |
| **1. Language** | The decisions above and the features in [What the language needs](#what-the-language-needs-to-support-the-compiler), implemented in the Rust checker and interpreter | Tests like the current ones |
| **2. Compiler in Rust** | A typed IR in `src/ir/` (monomorphization, closure conversion, `match` compiled to decision trees), a C code generator, a runtime in Rust (strings, lists, maps, memory management) and a `kojic build` command | Every test runs both interpreted and compiled, with identical output |
| **3. `kojic` in Koji** | Lexer → parser → checker → IR → C generator, in this order | At each step, compare with the Rust implementation: same tokens, same AST, same errors, same generated C |
| **4. Bootstrap** | Stages 1 → 2 → 3 | Stage 2 output identical to stage 3: reached, checked by `cargo test --test bootstrap -- --ignored` (stage 1 is the compiler in Koji built by `kojic build`, stage 2 the C it generates for itself compiled by `cc`) |
| **5. Maintenance** | A bootstrap policy (decided: a seed in C, see below): done, and the compiler and the interpreter in Rust are removed | The tests build the compiler from the seed; `cargo test --test bootstrap -- --ignored` checks the fixed point |
| **6. Optimization** | The optimizations below, in the compiler written in Koji | A benchmark (the compiler checking and lowering its own files) measures each one; every test still gives the same output |

Notes:

-   **Compiled standard library**: `json` and `regex` can be rewritten in Koji. `http` with TLS is heavy: it can go through FFI (e.g. libcurl) or stay outside the core.
-   **Rule for `kojic`**: it may only use features supported by the previous compiler version, otherwise the bootstrap breaks.

### The seed (phase 5)

Decided: **a seed in C**. The C that the compiler in Koji generates for itself is committed (compressed, `bootstrap/kojic.c.gz`), and building Koji needs only a C compiler and the runtime: `cc` builds stage 1 from the seed, and stage 1 builds the compiler from `compiler/`. There is then one implementation of the language, the one in Koji, which can use any feature of the language once the seed is regenerated with it (a feature is implemented first, the seed regenerated, and only then can the compiler use it). The runtime stays in Rust, so building it still needs `cargo`.

Steps:

1.  **`kojic run` in the compiler in Koji**: it compiles to a temporary executable and runs it with the terminal of `kojic` (`system.run`), ending with the program's exit code. Done; `test/bootstrap.rs` checks that every example runs like the interpreter.
2.  **The tests use the compiler in Koji**: `test/language.rs` and `test/compiler.rs` compile and run their programs with it, instead of the interpreter in Rust inside the test process. Done: the compiler in Koji is built once (`test/koji/mod.rs`, kept in the target directory while its C and the runtime stay the same), and each test runs it as a command (`kojic run`, `kojic build`, `kojic check`). What `test/compiler.rs` compared with the interpreter is now written in the tests, and the outputs of the examples are in `test/examples/`.
3.  **The seed**: `bootstrap/kojic.c.gz` and a script that builds `kojic` from it; a test checks that the seed regenerated reaches the fixed point. Done: `bootstrap/build.sh` builds the runtime, stage 1 from the seed (without optimizations: it only builds stage 2) and `build/kojic` from `compiler/`; `--seed` writes the seed again from it. The tests build the compiler the same way (`test/koji/mod.rs`), and the ignored `the_compiler_in_koji_compiles_itself` checks that the compiler built by stage 2 generates the same C as stage 2. The seed is written again when the compiler needs a feature the seed does not compile, and from time to time; it need not follow every change of `compiler/`. It must be written again when the C interface of the runtime changes (a function or a structure the generated code uses): the seed's C, compiled with the new runtime, would not agree with it. Then a compiler built before the change (with the runtime it was built with, from the commit before) compiles the new `compiler/` into a compiler that still runs on the old runtime but generates the new C, and that one writes the seed.
4.  **The compiler and the interpreter in Rust are removed** (`src/`); what stays in Rust is the runtime and the code that runs the tests. Done: the comparisons of the stages with the compiler in Rust became snapshots (`test/snapshots/`, `test/stages.rs`, written again with `KOJI_UPDATE_SNAPSHOTS=1`), and the C declarations of the runtime moved to `runtime/prelude.h`.

### Optimizations

Profiles of the compiled checker (callgrind) show most of the time in reference counting (`retain` and `release`), `malloc`/`free` and tiny calls: `Lexer.at`, which only reads `self.chars[i]`, costs about 100 instructions (it retains and releases `self`, calls `koji_record_get` and `koji_list_get` in the Rust library, which C cannot inline, and retains then releases the item it read). Koji has value semantics and explicit `mut`, so the compiler knows who can see each value: the same model as Koka, whose Perceus reference counting is the reference for most of these. In order of benefit for their cost:

1.  **Inline runtime accessors**: `koji_record_get`, `koji_list_get`, `retain` and `release` as `static inline` C (or link-time optimization across C and Rust). A change of the runtime only, possible at any time. Done: records and lists have a C layout (a record and its fields in one allocation), and reading a field, an item or a length, the tag, and counting references are inline in the prelude; only the last release calls the runtime. The runtime still exports the same functions, so a seed that calls them keeps working. `check` of the compiler went from 4.37 to 2.64 billion instructions (0.72 to 0.35 s), `dump c` from 1.65 to 1.02 s. `malloc` and `free` are now the largest cost (about a third), for steps 2 and 3.
2.  **Borrowed parameters**: the caller keeps the value alive during the call, so the function does not retain and release its parameters (except values read from a global, which the call may change). Done (`compiler/borrow.kj`): a parameter the function never assigns, changes in place, moves out or drops is borrowed; a caller that passes a global to one keeps a reference during the call; a function used as a closure keeps owned parameters, since its callers do not know it. `check` of the compiler: 2.67 to 2.54 billion instructions (−5%, retaining and releasing being already inline).
3.  **Reference counting elision and reuse** (the rest of Perceus): read a field without retaining it when it is only inspected; move instead of copying at the last use of a variable; reuse the memory of a value that dies to build the next one (`substitute`, `namedParts`). Started in the runtime: a record or a short string that dies gives its memory to a pool (one free list per size, `runtime/src/pool.rs`) for the next value of the same size, instead of `free` and `malloc`; a string is one allocation with its bytes (laid out for C, its references counted inline). `check` of the compiler: 2.54 to 1.86 billion instructions. In the compiler (`compiler/liveness.kj`, the locals read after each instruction): a copy that reads a value for the last time moves its reference, as does a `return` of a local; and `text + more` that reads `text` for the last time adds to it in place when no one else sees it (`koji_string_append`; a string grows by doubling, so `text += c` in a loop costs a constant time per piece instead of copying the text). `check`: 1.86 to 1.77 billion instructions. A record is built and its fields set in place, in the generated C (the field written at its word, a value read for the last time moved into it; the record copied first only when shared), and the small functions of the C prelude are always inlined (`KOJI_INLINE`: left to its heuristics, gcc took 20 minutes to build the compiler, weighing them in its large functions; now under 3). `check`: 1.77 to 1.36 billion instructions. A local only given fields of a borrowed parameter (or of such a local) is borrowed too: the parameter keeps the value alive, so it is neither retained nor released (`compiler/borrow.kj`; `self.chars[...]` in the lexer). `check`: 1.36 to 1.30 billion instructions. The pool keeps its free blocks in lists the generated code reads and writes inline, refilled a chunk at a time (one `malloc` for many blocks); a record is built inline (`koji_record_make`, its size known from its variant) and freed by a function generated for its type, which releases each field where it is (`koji_free_<type>`, called by `koji_record_release`) instead of looking at the description of the type field by field. `check`: 1.30 to 1.02 billion instructions. The memory of a record that dies is reused by the next one of its size: the pool gives the last block it got back first. A map finds a key in a table of positions of its own (open addressing, the hash of each key kept with it), instead of a `HashMap` of lists (a second hash of the hash, and a list allocated for each key); keys hash with a fast function (a multiply and a rotation for each 8 bytes) instead of SipHash. `check`: 1.02 to 0.86 billion instructions. The header of a list or a map is a block of the pool too (its items are still allocated by the system). `check`: 0.71 to 0.67 billion instructions (after step 4).
4.  **Inlining of small functions** in the IR (`Lexer.at`, `Parser.isSymbol`): the call and the reference counting of its parameters disappear. A panic location is the function's file and the instruction's line, so an instruction inlined from another module must carry its own file. Done in C instead, which keeps the locations as they are: a function of at most 4 blocks that calls no other one but such functions, of at most 16 instructions counting those of the functions it calls, is always inlined (`KOJI_INLINE`; at first, 12 instructions and no call); the C compiler did not inline them in the large functions by itself. Before it, a field is read at an offset known when the C is generated (every field of a type takes one word, or the type has one variant) instead of the runtime's description of the variant. `check` of the compiler: 0.86 to 0.75 billion instructions (offsets), then 0.71 (inlining); building the compiler with `-O2` takes about 3 and a half minutes instead of 3.
5.  **Escape analysis** to put records that never leave their function on the stack, without `malloc` or reference counting. Less useful than the others, and storing such a value elsewhere must copy it to the heap. Done (`compiler/escape.kj`): a record built by one instruction of a function, and never copied, stored, returned, captured, changed, given to the runtime (but to print it) or to a parameter that escapes (computed for all the functions at once, from the parameters that escape by themselves), is built in memory on the function's stack, with its fields released when it is built again and when the function returns; a local it is copied into right after it is built is an alias of it. 206 records of the compiler (`token == Symbol(text)` in `Parser.isSymbol`, for one). With it, the lines that release the locals at each `return` are computed once per function. `check` of the compiler: 0.68 to 0.67 billion instructions (−2.6%), `dump c` 3.23 to 2.92 (−9%, mostly the release lines). After the steps: a `push` writes in place when the list has room and is not shared, and `==` with a variant built on the spot (`token == Symbol(text)`, on the stack) compares its tag and its fields inline when they are ints, bools, chars or strings. `check`: −7%. A `mut` parameter moves back to the caller instead of being shared and released, a field of a variable is set at its known offset, and `text += string(c)` appends the char without a string for it (`koji_string_append_char`). `check`: −5%. Liveness summarizes each block once (the locals it reads first and those it writes) instead of going through its instructions at each iteration, and keeps for each instruction only the locals it reads for the last time: `dump c`: −18%. From it, a value is released right after the instruction that reads it for the last time, instead of when the function returns (but a borrowed local, a record on the stack, or a `mut` parameter): a list or a map read and then changed (`self.ids.get(name)`, then `self.ids[name] = id`) is no longer shared when it changes, and is no longer copied. `check`: −11%, `dump c`: −17%. A local is also borrowed when it only takes values read from something that stays as it is during the call (a borrowed parameter, such a local, or a global given its value once and never changed): a field, an item (`for word in words`), the value itself; and such a global given to a borrowed parameter needs no reference of the caller. `check`: −7%, `dump c`: −4%. The pool is also the allocator of the runtime's Rust code (the items of lists, the entries of maps, `String`s): blocks of up to 16 words come from it, each thread with lists of its own (the main thread's are `koji_pool`); `KOJI_POOL=0` turns it off, so that valgrind sees each block (the tests run their programs under valgrind so). `check`: −2%, `dump c`: −9%. The lexer tries the long symbols only for a character that starts one (`(` and `,` start none): `check`: −6.5%. In the C generator: a C string literal escapes only what needs it, and the escaped file of a function is computed once for its panic locations; liveness works on arrays of the function's locals (from each block that reads a local first, back through the blocks before it), without sets; `items[i] = value` writes in place when the list is not shared and the index is in it. `dump c`: −18%.

The benchmark is `bench/compiler.sh`: the compiler checking and generating its own source, the best of five runs of each stage, and the instructions of `check` (exact) when valgrind is installed.

Before them, measured: a `mut` parameter changing a local variable's value in place (it copied the whole value at each call), and caches in the checker written in Koji (`isMoveOnly`).

### A runtime in Koji (phase 7)

The runtime is in Rust (`runtime/`, about 5,600 lines, with the `regex` and `ureq` libraries); it moves to Koji, module by module, with the same behavior, the same tests and no loss of speed (see [A runtime of its own](design.md#a-runtime-of-its-own-written-in-koji)):

1.  **Modules of the standard library written in Koji**, which the compiler carries: `std/*.kj`, embedded in `compiler/stdlib.kj` (generated; `test/stages.rs` checks it, `KOJI_UPDATE_STDLIB=1` writes it again); `import json` compiles `std/json.kj` as the module `std:json`. First the modules that need nothing new. Done: `json` (parser, writer and helpers in Koji; `runtime/src/json.rs` is gone, and `Json` and `JsonError` are no longer built into the checker). Then `path` (the same rules as before: repeated separators and `.` after the first part count for nothing). When a module the compiler itself uses moves, the seed is written in two steps: the C that the previous compiler generates still calls the runtime, so it is linked with the previous runtime once, and the compiler it builds generates the new seed. Then `regex`, an engine of its own in Koji (a Pike machine: no backtracking), with `Regex` an opaque type; the `regex` crate and `runtime/src/regex.rs` are gone. Its results are those of the crate on a battery of patterns (but the Unicode properties beyond letters, cases, numbers and spaces, now `Unsupported`); it is about 1.4 times slower on a benchmark of searches and replacements (a first-character prefilter skips most positions). Then `datetime` (on `time.now()`; `DateTime` is a record with a `display` method, shown as its fields inside other values like any type). `time` and `math` wait for step 2: `time` only calls the system, and `math` needs the C library for floats.
2.  **`koji:unsafe`**: an address type, reads and writes of bytes and words, unsigned and wrapping arithmetic, and `extern function` for the C library of the system; for the standard library only. Done (see [`koji:unsafe`](design.md#kojiunsafe-the-lowest-level-of-the-standard-library)): `Address`, `CInt`, `allocate`, `free`, `read` and `write` (generic: the size follows the type, a `byte` too), and `extern function` (the C prototype is generated, an `Address` passed as a `void *`); unsigned and wrapping arithmetic wait for `decimal`. With it, `time` is in Koji (`clock_gettime`, `nanosleep`), with the epoch of Koji, 0000-01-01 (see [the decision](design.md#the-epoch-of-koji-is-0000-01-01)). Then `math` (`std/math.kj`): the floats with the C library (`sqrt`, `pow`, `floor`...), the decimals still with the runtime (`extern` functions take a `decimal` and a `Location`, the place of the call, for their panics), `MathError`, `pi` and `e` declared in Koji; the compiler calls the function for the types of the arguments (see [the decision](design.md#mathabs-of-several-types-the-compiler-chooses-the-function)). Its results and panics are those of before on a battery of cases.
3.  **The rest of the runtime**: strings, lists, maps and the pool; `decimal`; `file` and `system`; the equality, hash and display of records generated by the compiler for each type. Done: the methods of strings `trim`, `split`, `replace`, `startsWith`, `endsWith` and `indexOf`, in `koji:core` (see [the decision](design.md#built-in-methods-written-in-koji-kojicore)), with the same results on a battery of texts (Unicode white space, multibyte characters) and a little faster (`memmem` of the C library). Then `system` (`std/system.kj`): `posix_spawnp` for `run` and `capture` (the outputs of `capture` read with `poll`, so that a program filling one pipe does not block), `getenv`, `uname` for `os`; the runtime keeps only what C does not give in the same way on every system (`runtime/src/system.rs`: the arguments of `main`, `errno`, the environment, the path of the executable). The messages of `SystemError` and `MathError` are `display` methods in Koji (the runtime chose them by the name of the type, which a type of a program could have too).
4.  **TLS in Koji** for `http`: hashes, ciphers, key exchange, certificates.
5.  **Rust leaves the runtime**, and cargo is no longer needed to build Koji (but for what may have to stay in Rust).

---

## Open decisions

None for now.

Decided:

-   **FFI** (`extern function`, to call C libraries from Koji code) is postponed until the language needs it: the compiler compiles itself without it, and the standard library is in the runtime, in Rust.

-   **Generic functions as values**: allowed where a function type is expected (an annotation, a parameter), which gives the type arguments; without it, an error. A value that stays generic is not planned.

-   **`Ok()`**: a `Result[void, E]` succeeds with `Ok()`, without an argument (there is no `void` value); it is shown as `Ok(void)`.

-   **`int` to `float`**: only a single value is converted; an `int[]` where a `float[]` is expected is an error, since converting it would copy the list (see [Primitive Types](primitive-types.md#numeric-types)).
-   **Backend**: C, with the runtime in Rust (see [Bootstrap strategy](#bootstrap-strategy)).

-   **Mutating methods**: `mut self`, with no marker at the call (see [Methods](design.md#methods-and-self)).
-   **Functions without `self`**: associated functions, `Person.create('John')`.
-   **Move-only types**: implicit, from `drop` (see [Value semantics details](design.md#move-only-types-and-drop)).
-   **Display**: `display()` is readable (a `display(self): string` method of the type, when declared) and is what `print` and interpolation use; `debug()` shows the structure (see [Custom Types](custom-types.md#display)).
-   **`regex.compile`**, with `RegexError = { InvalidPattern(pattern, message) }` (see [Typed errors in the standard library](design.md#typed-errors-in-the-standard-library)).
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
-   **Opaque types**: `export opaque type` (see [Opaque Types](modules-and-imports.md#opaque-types)): other modules use the type, but not its fields, literals or variants; it is shown by its `display` method, also inside other values, or by its name.
-   **No anonymous objects**: an object literal needs a known type (see [No `interface`](design.md#nominal-types-no-interface-no-anonymous-objects)). Values are built with a literal or with an associated function (`Person.create(...)`); the language has no special constructors.
