# system

The `system` module gives access to the command line, the environment, the exit code, other programs and the operating system.

## Functions

-   `args() -> string[]`  
    The command-line arguments of the program: with `kojic run main.kj a b`, `['a', 'b']`.

-   `env(name: string) -> Option[string]`  
    The value of the environment variable `name`, or `None` if it is not set.

-   `executable() -> Option[string]`  
    The path of the executable of the running program (with `kojic run`, the temporary executable it built), or `None` where the system cannot tell it.

-   `exit(code: int) -> never`  
    Ends the program with the exit code `code` (`0` means success). The code after it does not run, and values are not dropped.

-   `run(command: string, args: string[]) -> Result[int, SystemError]`  
    Runs the program `command` with the arguments `args` and waits for it to end. It uses the terminal of this program: it reads the same input and writes to the same outputs, as if it were typed in the terminal. Gives its exit code (`-1` if it was stopped by a signal); a program that ends with an error code is still `Ok`.

-   `capture(command: string, args: string[]) -> Result[Output, SystemError]`  
    Like `run`, but keeps what the program prints instead of showing it, and gives it with the exit code as an `Output`.

## Constants

-   `os: OperatingSystem`  
    The operating system the program runs on:

    ```koji
    type OperatingSystem = {
        Linux
        MacOS
        Windows
        Other(name: string) // another system, by its name: 'freebsd'
    }
    ```

    Since it is a sum type, a `match` on it must handle every system, or have a `_` arm:

    ```koji
    import system

    immut separator = match (system.os) {
        Windows => ';'
        _ => ':'
    }

    print(separator)
    ```

## The `Output` Type

-   `status: int`: the exit code of the program (`-1` if it was stopped by a signal).
-   `stdout: string`: what it printed.
-   `stderr: string`: what it printed as errors.

## Errors

```koji
type SystemError = {
    NotFound(command: string) // there is no such program
    PermissionDenied(command: string) // it cannot be run
    Io(command: string, message: string) // any other failure of the operating system
}
```

## Example

```koji
import system

immut args = system.args()

if (args.length() == 0) {
    print('usage: kojic run greet.kj <name>')

    system.exit(2)
}

print(`Hello, ${args[0]}!`)

match (system.capture('git', ['--version'])) {
    Ok(output) => print(output.stdout.trim())
    Err(error) => print(error) // command 'git' not found
}

match (system.run('git', ['status'])) {
    Ok(0) => print('done')
    Ok(code) => print(`git ended with the code ${code}`)
    Err(error) => print(error)
}
```
