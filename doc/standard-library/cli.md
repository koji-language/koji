# cli

The `cli` module reads the arguments of a program of the command line: its options and arguments are declared as a help shows them, and the help, the errors and `--version` come from the declarations. It is written in Koji (`std/cli.kj`).

```koji
import cli

immut command = cli.command('greet', 'Greets someone.')
    .flag('-l, --loud', 'Shout the greeting')
    .number('-t, --times <n>', 'How many times', 1)
    .option('-g, --greeting <text>', 'The greeting', 'hello')
    .argument('<name>', 'Who to greet')
    .version('1.0.0')

immut parsed = cli.parse(command)

for i in range(parsed.number('times')) {
    mut text = `${parsed.value('greeting')}, ${parsed.value('name')}`

    if (parsed.flag('loud')) {
        text = text.upper()
    }

    print(text)
}
```

```
$ greet -l --times 2 ana
HELLO, ANA
HELLO, ANA
$ greet --help
Greets someone.

Usage: greet [options] <name>

Arguments:
  <name>                 Who to greet

Options:
  -l, --loud             Shout the greeting
  -t, --times <n>        How many times (default: 1)
  -g, --greeting <text>  The greeting (default: hello)
  -h, --help             Show this help
      --version          Show the version
$ greet --lod ana
greet: unknown option '--lod' (did you mean '--loud'?)
Run 'greet --help' for help.
```

## Declaring

-   `command(name: string, description: string) -> Command`  
    A command: the name of the program (or of a subcommand) and what it does, for its help.

Each method returns the command with one more declaration, so they are chained:

-   `.flag(spec, help)`: an option without a value, `'-l, --loud'` (the short name may be left out: `'--loud'`).
-   `.option(spec, help, fallback: string)`: an option with a value, `'-o, --output <file>'`; `fallback` when it is not given (`''` for none).
-   `.number(spec, help, fallback: int)`: an option whose value is an integer, checked when the arguments are read.
-   `.argument(spec, help)`: an argument, in the order declared: `'<name>'` (required), `'[name]'` (may be left out), `'<names...>'` (the arguments left, at least one) or `'[names...]'` (any).
-   `.subcommand(command)`: a subcommand, as in `git add`: the first argument names it, and it reads the rest with its own options and arguments (the options of the command before it may come first).
-   `.version(text)`: what `--version` shows.

`-h`, `--help` and `--version` are made by the module.

## Reading

-   `parse(command: Command) -> Parsed`  
    Reads the arguments of the program. `--help` shows the help and `--version` the version, and the program ends; a mistake is shown on the standard error with how to get the help, and the program ends with the code 2.

-   `parseArgs(command: Command, args: string[]) -> Result[Parsed, CliError]`  
    Reads `args` (without the name of the program): for tests, or to handle the help and mistakes yourself. `CliError` is `Help(text)`, `Version(text)` or `Invalid(message)`.

The arguments are read as most programs read them: options anywhere among the arguments; `--name value` or `--name=value`; `-t 3` or `-t3`; flags together (`-lv`); `--` before arguments that start with `-`.

## The Values

-   `parsed.flag(name) -> boolean`: whether the flag was given (by its long name: `parsed.flag('loud')`).
-   `parsed.value(name) -> string`: the value of an option or an argument: what was given, or its fallback (`''` for an argument left out).
-   `parsed.number(name) -> int`: the value of an option declared with `number`.
-   `parsed.list(name) -> string[]`: the arguments of `<names...>` or `[names...]`.
-   `parsed.command() -> string`: the subcommand given (`''` for none).

A name that the command does not declare (with the kind asked for) panics: it is a mistake of the program, not of who runs it.

```koji
import cli

immut command = cli.command('todo', 'A list of things to do.')
    .subcommand(cli.command('add', 'Adds an item.').argument('<text...>', 'What to do'))
    .subcommand(cli.command('list', 'Shows the items.').flag('-a, --all', 'Also those done'))

immut parsed = cli.parse(command)

match (parsed.command()) {
    'add' => print(`adding: ${parsed.list('text').join(' ')}`)
    'list' => print(`listing (all: ${parsed.flag('all')})`)
    _ => {}
}
```
