# Changelog

## 0.1.0 (preview)

The first release of Koji: a preview, to try the language and tell what is missing. Expect
breaking changes before 1.0.

### The language

-   Strong, static typing with inference: `int`, `float`, `decimal` (the default of literals with a
    point), `string`, `char`, `byte`, `boolean`, lists, maps, sets, records and sum types.
-   `match` with guards and exhaustiveness, `if` as an expression, no `null` (`Option`), errors as
    values (`Result` and `?`), panics for bugs, no exceptions.
-   Value semantics with copy-on-write, move-only types with `drop`, `mut` and `take` parameters.
-   Generics with the `equatable` and `orderable` constraints, methods, closures that capture a copy.
-   Modules with mandatory `export`, opaque types, packages from repositories of git (`koji.mod`,
    `koji.sum`, minimal version selection).
-   Tasks in groups, channels and shared values, on one thread, with no call blocking the others.
-   `test` blocks and `kojic test`; `foreign` blocks for the functions of libraries of the system.

### The standard library

`cli`, `datetime`, `file`, `hash`, `http` (a client with TLS 1.3 written in Koji, and a server),
`json`, `math`, `path`, `regex`, `system` (programs run with an input, a directory and an
environment), `task` and `time`.

### The compiler

-   `kojic` is written in Koji and builds itself from a seed of its own executables
    (`bootstrap/build.sh`, `bootstrap\build.cmd`): building Koji needs nothing but `gzip` (PowerShell
    on Windows), no C compiler.
-   It generates machine code itself (x86-64 and arm64) and links executables with its own linker
    (ELF, Mach-O and PE), the shared libraries of `foreign` blocks included on Linux and Windows.
-   `kojic run`, `build`, `check`, `test`, `lsp` (a language server), `init`, `add`, `update`,
    `version`.

### Platforms

| Platform | State |
| --- | --- |
| Linux, x86-64 | tested (the whole test suite) |
| Linux, arm64 | tested under qemu |
| Windows, x86-64 | tested under Wine (the programs of the test suite) |
| macOS, arm64 and x86-64 | built and linked, not yet run on a Mac |

### Known limits

-   Tasks run on one thread.
-   `regex` has no look-around.
-   On macOS, the libraries of `foreign` blocks are linked by the linker of the system.
-   The code is about 1.3 times as slow as C compiled by gcc on the compiler itself.
