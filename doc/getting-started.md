# Getting Started

This guide shows how to build Koji, write a first program and run it.

## Requirements

Koji is implemented in Rust. To build it, install [Rust](https://www.rust-lang.org/tools/install) (which includes `cargo`).

## Building

Clone the repository and build it in release mode:

```sh
git clone https://github.com/koji-language/koji.git
cd koji
cargo build --release
```

The executable is created at `target/release/kojic`. To call it as `kojic` from anywhere, add that directory to your `PATH`, or install it with:

```sh
cargo install --path .
```

### From the Seed

Koji also has a compiler written in Koji (`compiler/`), which can be built without the compiler in Rust, from the C that it generates for itself (`bootstrap/kojic.c.gz`). It needs `cargo` only for the runtime library, `gzip` and a C compiler:

```sh
bootstrap/build.sh
```

The compiler is created at `build/kojic`, with the runtime library at `build/libkoji_runtime.a`. It has the same commands, and needs the path of the runtime library in `KOJI_RUNTIME` to build or run programs:

```sh
export KOJI_RUNTIME=$PWD/build/libkoji_runtime.a
build/kojic run hello.kj
```

`bootstrap/build.sh --seed` writes the seed again from the compiler it built: after a change to the compiler, so that the next build starts from it.

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
type error: hello.kj:5: argument 1 must be string, found int
```

The message gives the file and the line of the error.

## Commands

| Command | Description |
| --- | --- |
| `kojic run <file.kj> [arguments]` | Checks and runs a program; the arguments after the file are for the program (`system.args()`) |
| `kojic check <file.kj>` | Only checks syntax and types |
| `kojic build <file.kj> [-o <output>]` | Compiles a program to an executable (needs a C compiler) |
| `kojic help` | Shows the available commands |

`kojic run` runs programs on an interpreter. `kojic build` compiles them to native executables, through C: it needs a C compiler (`cc`, such as gcc or clang). Both run every program the same way.

```sh
kojic build hello.kj
./hello
```

## Examples

The [`examples`](../examples) folder has small programs to try:

```sh
kojic run examples/functions.kj
kojic run examples/shapes.kj
```

## Running the Tests

If you want to contribute, run the test suite with:

```sh
cargo test
```

See the [contributing guide](../CONTRIBUTING.md) for details.
