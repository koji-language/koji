# Getting Started

This guide shows how to build Koji, write a first program and run it.

## Requirements

The Koji compiler, `kojic`, is written in Koji, and compiles programs to machine code (Linux on x86-64 and arm64, macOS on x86-64 and arm64, Windows on x86-64). Building it needs Linux on x86-64 or arm64 and `gzip`, or Windows on x86-64: no C compiler.

## Building

Clone the repository and run the build script:

```sh
git clone https://github.com/koji-language/koji.git
cd koji
bootstrap/build.sh
```

The compiler is written in Koji, so it needs a compiler to be built: the script starts from an executable of the compiler for your platform (`bootstrap/seed/kojic-<platform>.gz`, the seed), which builds the runtime library and the compiler of `compiler/`, machine code and linking included.

On Windows (x86-64), run `bootstrap\build.cmd` instead: it unpacks the seed for Windows with PowerShell and builds `build\kojic.exe` the same way.

The compiler is created at `build/kojic`, and the runtime library at `build/libkoji_runtime.a`. `kojic` finds the runtime library next to itself; another one can be given in the environment variable `KOJI_RUNTIME`. To call it as `kojic` from anywhere, add `build` to your `PATH`:

```sh
export PATH=$PWD/build:$PATH
```

When the compiler starts to use something the seed cannot compile, `bootstrap/build.sh --seed` also writes the seeds of every platform again from the compiler it built, so that the next build starts from them (they are not written again after every change: each is megabytes).

A compiled program keeps the memory of the small values it drops for the next ones, which is faster than asking the system each time. To check a program with valgrind, run it with `KOJI_POOL=0`: each value then gets its own memory from the system, and valgrind sees the use of a value after it was freed.

## Your First Program

Create a file named `hello.kj`:

```koji
function greet(name: string): string {
    return `Hello, ${name}!`
}

print(greet('World'))
```

Run it:

```sh
kojic run hello.kj
```

```
Hello, World!
```

## Checking a Program

Every program is checked before it runs: syntax, types, `match` arms that are not handled, assignments to `immut` values, names that were never declared and so on. A program with an error does not run at all.

To only check a program, without running it:

```sh
kojic check hello.kj
```

It prints nothing when the program is correct. If `hello.kj` called `greet(42)`, both `kojic run` and `kojic check` would report:

```
type error: hello.kj:5:13: argument 1 of 'greet' must be string, found int
    5 | print(greet(42))
      |             ^^
```

The message gives the file, the line and the column of the error, and shows the line with the part in error marked. The check goes on after an error, so that every error of the program is reported at once (up to 20).

## Commands

| Command | Description |
| --- | --- |
| `kojic run <file.kj> [arguments]` | Checks and runs a program; the arguments after the file are for the program (`system.args()`) |
| `kojic check <file.kj>` | Only checks syntax and types |
| `kojic build <file.kj> [-o <output>]` | Compiles a program to an executable |
| `kojic test [file.kj \| directory]...` | Runs the tests of files, the `test` blocks (see [Testing](testing.md)) |
| `kojic lsp` | The language server, for editors (on the standard input and output) |
| `kojic help` | Shows the available commands |

`kojic build` compiles a program to a native executable, through C. `kojic run` does the same in a temporary file, runs it with the arguments and the input of `kojic`, and ends with its exit code.

```sh
kojic build hello.kj
./hello
```

## Editor Support

`editors/vscode` is an extension of Visual Studio Code with the syntax highlighting of Koji and its language server: see its [README](../editors/vscode/README.md) to install it. Its grammar is a TextMate grammar, which other editors read too.

`kojic lsp` is the language server, for any editor that speaks the Language Server Protocol (VS Code, Neovim, Helix, Zed, Emacs...): it shows every error of a file as it is edited, the type of what is under the cursor, and goes to where a name is declared, in the file or in the module it comes from.

## Examples

The [`examples`](../examples) folder has small programs to try:

```sh
kojic run examples/functions.kj
kojic run examples/shapes.kj
```

## Running the Tests

If you want to contribute, run the test suite: it is written in Koji, and runs with the compiler built by `bootstrap/build.sh`, from the root of the repository:

```sh
sh bootstrap/build.sh
build/kojic test test/*.kj
```

The programs of `test/cases/` are run by `kojic run`, or built by `kojic build` and run (under valgrind when it is installed), against what they must print, several at a time (`KOJI_TEST_JOBS`, 8 by default); `test/*.kj` test the rest: the commands of the compiler, servers, the cryptography of the runtime, the platforms Koji builds for, and that the compiler compiles itself to the same executable, byte for byte (the fixed point). The test of `https://` uses the `openssl` command when it is there, and those of other platforms qemu, lld or Wine when they are there. `KOJI_COMPILER` names another compiler to test; `KOJI_TEST_WINDOWS=1` builds the programs of the cases for Windows and runs them with Wine (in a prefix of its own, `build/test/wine`, or `KOJI_WINEPREFIX`, with one wineserver for all of them).

The stages of the compiler are compared with snapshots (`test/snapshots/`): after a change of a stage, `KOJI_UPDATE_SNAPSHOTS=1 build/kojic test test/stages.kj` writes them again. After a change of `std/`, `build/kojic run bootstrap/generate/stdlib.kj > compiler/stdlib.kj` writes the sources the compiler carries again (then build it again).

See the [contributing guide](../CONTRIBUTING.md) for details.
