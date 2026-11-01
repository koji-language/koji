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

Every program is checked before it runs: syntax, types, `match` arms that are not handled, assignments to `const` or `imut` values and so on. A program with an error does not run at all.

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
| `kojic run <file.kj>` | Checks and runs a program |
| `kojic check <file.kj>` | Only checks syntax and types |
| `kojic help` | Shows the available commands |

Programs currently run on an interpreter. Compiling to native binaries (`kojic build`) is planned; see the [compiler plan](compiler-plan.md).

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

## Next Steps

-   [Variable Types](variable-types.md): `var`, `const` and `imut`
-   [Primitive Types](primitive-types.md) and [Composite Types](composite-types.md)
-   [Functions](functions.md)
-   [Custom Types](custom-types.md) and [Sum Types](sum-types.md)
-   [Control Flow](control-flow.md)
-   [Error Handling](error-handling.md)
-   [Modules and Imports](modules-and-imports.md)
-   [Standard Library](stdlib.md)
-   The full [documentation](README.md)
