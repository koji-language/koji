# Contributing to Koji

Thank you for your interest in Koji. Bug reports, ideas, documentation, examples and code are all
welcome. This guide shows how to build Koji, find your way in it, and send a change.

-   **A bug**: open an [issue](https://github.com/koji-language/koji/issues) with the version
    (`kojic version`), your system, and the smallest program that shows it.
-   **An idea or a question about the language**: open an issue to talk about it before writing
    code. A change of the language is a decision of design (see below), and it is easier to agree
    on it first.
-   **A vulnerability**: do not open an issue; see [SECURITY.md](SECURITY.md).
-   **A fix, or a small improvement**: send a pull request.

## Building Koji

Koji builds itself from a seed (its own executables, in `bootstrap/seed/`): you need `git` and
`gzip`, and nothing else. No C compiler, no LLVM, no other language.

```sh
git clone https://github.com/koji-language/koji
cd koji
sh bootstrap/build.sh                # bootstrap\build.cmd on Windows (PowerShell)
build/kojic run examples/hello.kj
```

`bootstrap/build.sh` writes `build/kojic` and the runtime library next to it,
`build/libkoji_runtime.a`, in less than a minute. Build again after each change of `compiler/` or
`std/`.

Then run the tests (about a minute; `valgrind`, where it is installed, checks the memory of the
programs they build):

```sh
build/kojic test test/*.kj
```

## How the repository is organized

| Path | What it is |
| --- | --- |
| `compiler/` | The compiler, `kojic`, written in Koji. |
| `std/` | The standard library, written in Koji. `std/koji/` is the runtime and the internal modules, which programs cannot import. |
| `compiler/stdlib.kj` | The sources of `std/`, which the compiler carries. Generated: do not edit it (see below). |
| `test/` | The tests, written in Koji: `test/*.kj`, and the cases they run (`test/cases/`, `test/programs/`...). |
| `doc/` | The documentation, which the [site](https://koji-language.github.io/koji-website/) shows. `doc/design.md` keeps the reasons of the language, `doc/internals.md` those of the compiler. |
| `bootstrap/` | The seed, the scripts that build and pack Koji, and the generators of the tables of the library (`bootstrap/generate/`). |
| `examples/` | Small programs, all checked by the tests. |
| `editors/textmate/` | The grammar for editors (the extension of VS Code is [koji-vscode](https://github.com/koji-language/koji-vscode)). |
| `bench/` | `bench/compiler.sh`: the speed of the compiler on its own source. |

### The stages of the compiler

A program goes through these files of `compiler/`, in order:

1.  `lexer.kj`: the text to tokens.
2.  `parser.kj`: the tokens to a syntax tree.
3.  `checker.kj` (with `types.kj`, `borrow.kj`, `foreign.kj`, `packages.kj`): the types, the
    moves, the modules; every error a user sees comes from here.
4.  `lower.kj`: the tree to the intermediate representation (`ir.kj`), then `inline.kj`.
5.  `mir.kj`, `mirlower.kj`, `mirregs.kj`...: the machine IR, which knows no processor, and the
    allocation of its registers.
6.  `x64.kj` and `a64.kj`: the machine code of each processor; `native.kj` drives them.
7.  `macho.kj`, `coff.kj` (and the ELF of `native.kj`): the object files; `linker.kj`,
    `machlink.kj` and `pelink.kj`: the linkers of Linux, macOS and Windows.

`main.kj` holds the commands (`run`, `build`, `test`, `fmt`...), `lsp.kj` the language server and
`format.kj` the formatter. [doc/internals.md](doc/internals.md) explains why each part is the way
it is.

`kojic dump tokens|ast|types|ir|inlined <file.kj>` shows what each stage makes of a file, which
is the fastest way to see where a bug starts.

## Sending a change

1.  Fork the repository, work on a branch of your fork made from `main`, and open the pull
    request against `main`. The repository keeps `main` and the tags of the releases only.
2.  Keep a pull request to one change: a fix, a feature, a cleanup. A large change goes easier as
    several small ones.
3.  Before you send it:
    -   `sh bootstrap/build.sh`, then `build/kojic test test/*.kj`: everything passes;
    -   `build/kojic fmt` on the files you changed: CI checks that the code of Koji is in its
        style;
    -   a test for what you fixed or added (see below);
    -   the documentation of what users see, in `doc/`;
    -   a line in the section `## Unreleased` of [CHANGELOG.md](CHANGELOG.md), for a change that
        users notice.
4.  CI builds and tests Koji on Linux (x86-64 and arm64), macOS (arm64 and x86-64) and Windows,
    and every job must pass.

### Tests

Most tests are cases in files of text, in `test/cases/`: a program, and what it must do.

```
==== a_name_for_the_case
---- source
print(1 + 2)
---- output
3
```

Besides `---- output`, a case can have `---- input` (what the program reads), `---- error` or
`---- error containing` (the error that must end it, from the compiler or a panic), `---- exit 3`,
and `---- file lib.kj` (another file of the program). They run in `test/cases.kj`, built by
`kojic run` and `kojic build`.

What a case cannot show (servers, processes, the language server, the platforms) is a `test`
block in `test/*.kj`. The helpers they share are in `test/koji.kj`.

Variables that change how the tests run:

| Variable | What it does |
| --- | --- |
| `KOJI_UPDATE_SNAPSHOTS=1` | Writes again the snapshots of the stages (`test/snapshots/`) when a change of the compiler changes them on purpose. Read the diff before committing it. |
| `KOJI_TEST_WINDOWS=1` | Builds the programs of the cases for Windows and runs them under Wine. |
| `KOJI_TEST_JOBS=4` | How many programs run at the same time (8 by default). |
| `KOJI_COMPILER=path` | The `kojic` to test, instead of `build/kojic`. |

### Changing the standard library

The compiler carries the sources of `std/` in `compiler/stdlib.kj`. After a change of `std/`,
write it again and build:

```sh
build/kojic run bootstrap/generate/stdlib.kj > compiler/stdlib.kj
sh bootstrap/build.sh
```

To try a change of `std/` without building, run the compiler with `KOJI_STD=std`: it then reads
the library from the directory. The tests check that `compiler/stdlib.kj` is up to date.

### The seed

The seed is written again only when the compiler needs a feature that the seed cannot compile, and
`bootstrap/build.sh --seed` does it. Leave it out of a pull request, unless that is what the
pull request is about.

### Debugging

-   `kojic dump ...`: each stage, as above.
-   `KOJI_MIR_DUMP=name`: the machine IR of the function `name`, as it is compiled.
-   `KOJI_LINKER=system`: links with the linker of the system rather than Koji's, to tell a bug
    of the linker from a bug of the code; `KOJI_LINKER=koji` makes a failure of Koji's linker an
    error rather than a fallback.
-   `KOJI_TARGET=windows-x64` (or `arm64`, `macos-x64`, `macos-arm64`): builds for another
    platform, from any of them.

## Style

-   `kojic fmt` decides the spaces, the indentation, the quotes and the blank lines. The rest is
    in [doc/conventions.md](doc/conventions.md): names, the special cases first, `immut` unless a
    variable changes.
-   Code, comments, documentation and commit messages are in English.
-   A function, a type or a module starts with a comment that says what it is or does; the editor
    shows it as its documentation. A comment explains why, not what the code says already.
-   A commit message is a short summary, with the area first when it helps (`checker: ...`,
    `regex: ...`, `docs: ...`), then, after a blank line, what changed and why.

## Changing the language

Koji is a preview, and its design can still change, but each decision is kept in
[doc/design.md](doc/design.md): what Koji does, why, and what was considered instead. A change of
the language, or of the way the standard library is shaped, adds or changes a decision there, in
the same pull request. Open an issue first to talk about it: the reasons matter as much as the code.

## License

Koji is licensed under the [Apache License 2.0 with LLVM Exceptions](LICENSE). By contributing, you
agree that your contribution is licensed the same way.
