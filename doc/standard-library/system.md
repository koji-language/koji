# system

The `system` module gives access to the command line, the environment, the exit code and other programs.

## Functions

-   `args() -> string[]`  
    The command-line arguments of the program: with `kojic run main.kj a b`, `['a', 'b']`.

-   `env(name: string) -> Option[string]`  
    The value of the environment variable `name`, or `None` if it is not set.

-   `exit(code: int) -> never`  
    Ends the program with the exit code `code` (`0` means success). The code after it does not run, and values are not dropped.

-   `run(command: string, args: string[]) -> Result[Output, SystemError]`  
    Runs the program `command` with the arguments `args`, waits for it to end, and returns what it printed. A program that ends with an error code is still `Ok`: check `status`.

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

match (system.run('git', ['--version'])) {
    Ok(output) => print(output.stdout.trim())
    Err(error) => print(error) // command 'git' not found
}
```
