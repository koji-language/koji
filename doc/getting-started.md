# Getting Started

This guide shows how to build Koji, write a first program and run it.

## Requirements

The Koji compiler, `kojic`, is written in Koji, and compiles programs to C. Building it needs:

-   a C compiler (`cc`, such as gcc or clang), which `kojic` also uses to build programs;
-   `gzip` and `ar`.

The runtime library that compiled programs are linked with is written in Koji too; its C is in `bootstrap/libkoji.c.gz`, next to the seed.

## Building

Clone the repository and run the build script:

```sh
git clone https://github.com/koji-language/koji.git
cd koji
bootstrap/build.sh
```

The compiler is written in Koji, so it needs a compiler to be built: the script starts from the C that the compiler generated for itself (`bootstrap/kojic.c.gz`, the seed), compiles it with the C compiler, and with it builds the compiler of `compiler/`.

The compiler is created at `build/kojic`, and the runtime library at `build/libkoji_runtime.a`. `kojic` finds the runtime library next to itself; another one can be given in the environment variable `KOJI_RUNTIME`. `KOJI_CC` chooses the C compiler (default: `cc`). To call it as `kojic` from anywhere, add `build` to your `PATH`:

```sh
export PATH=$PWD/build:$PATH
```

After a change to the compiler, `bootstrap/build.sh --seed` also writes the seed again from the compiler it built, so that the next build starts from it.

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

`kojic build` compiles a program to a native executable, through C. `kojic run` does the same in a temporary file, runs it with the arguments and the input of `kojic`, and ends with its exit code.

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

If you want to contribute, run the test suite with [Rust](https://www.rust-lang.org/tools/install)'s `cargo` (the tests are written in Rust; building Koji does not need it):

```sh
cargo test
```

The tests build the compiler from the seed, like `bootstrap/build.sh`, and run it on their programs. `test/crypto.rs` tests the cryptography of the runtime by C programs linked with it, and the test of `https://` uses the `openssl` command when it is there. `cargo test --test bootstrap -- --ignored` also checks that the compiler compiles itself to the same C (the fixed point). The stages of the compiler are compared with snapshots (`test/snapshots/`): after a change of a stage, `KOJI_UPDATE_SNAPSHOTS=1 cargo test --test stages` writes them again.

See the [contributing guide](../CONTRIBUTING.md) for details.
