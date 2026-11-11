# Roadmap

This document outlines the planned features and milestones for the Koji programming language.

---

## Short-term

-   [x] Define the core syntax
-   [x] Implement the basic type system (primitive and custom types)
-   [x] Built-in functions: `print()`, `input()`
-   [x] String interpolation
-   [x] `for` loop with `range()`
-   [x] Organize and write the initial documentation

---

## Mid-term

-   [x] Type-safe standard library
-   [x] Control flow: `if`, `match`, `while`, `break`, `continue`
-   [x] Methods on custom types (`Type.method()`)
-   [x] Error handling model: `Result`, the `?` operator and panics (no `try` / `catch`)
-   [x] Module system with mandatory `export`
-   [x] Value semantics with copy-on-write, move-only types and `drop`
-   [x] User-defined generics, with `equatable` and `orderable` constraints
-   [x] A compiler targeting native binaries (`kojic build`, through C)
-   [x] A self-hosted compiler, written in Koji, with its runtime written in Koji too

---

## Long-term

-   [ ] Package manager
-   [x] Built-in testing framework (`test` blocks, `assert`, `kojic test`)
-   [x] Concurrency: lightweight tasks in groups, channels and shared values (`task`), on one thread
-   [ ] An HTTP server (`http.serve`)
-   [ ] Tasks over several threads
-   [ ] WebAssembly (WASM) compilation
-   [ ] Editor support (syntax highlighting, LSP integration)
-   [ ] Runtime optimizations
-   [ ] Production-ready tooling

---

This roadmap is a living document and will evolve as the language and its community grow.
