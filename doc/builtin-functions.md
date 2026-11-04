# Built-in Functions

These functions are available in every file, without `import`.

| Function | Description |
| --- | --- |
| `print(values...)` | Shows the values, separated by spaces, followed by a new line |
| `printError(values...)` | Like `print`, on the standard error output |
| `input(prompt)` | Reads a line typed by the user |
| `range(...)` | A list of consecutive integers |
| `int(x)`, `float(x)`, `decimal(x)`, `string(x)`, `char(code)` | Conversions |
| `panic(message)` | Stops the program |

## `print`

Shows its arguments, separated by spaces. Each value is shown as its [`display()`](custom-types.md#display) text: strings without quotes, standard library errors as a readable message.

```koji
immut name: string = 'John'

print(`Hello, ${name}!`) // Hello, John!
print('total:', 3, [1, 2]) // total: 3 [1, 2]
```

## `printError`

Like `print`, but writes to the **standard error output**, which is where error messages go: they stay visible when the standard output is redirected to a file or to another program.

```koji
immut name: string = 'notes.txt'

printError('cannot read', name) // cannot read notes.txt (on the standard error output)
```

## `input`

Shows the optional `prompt` and reads a line. It returns an `Option[string]`: `None` when there is nothing more to read (the end of the input).

```koji
immut name: Option[string] = input("What's your name? ")

match (name) {
	Some(line) => print(`Hello, ${line}!`)
	None => print('No input')
}
```

## `range`

`range(end)`, `range(start, end)` or `range(start, end, step)`: the integers from `start` (default `0`) up to, but not including, `end`. A negative `step` counts down; a `step` of `0` panics.

```koji
range(5) // [0, 1, 2, 3, 4]
range(2, 5) // [2, 3, 4]
range(10, 0, -3) // [10, 7, 4, 1]
```

In a `for` loop, the numbers are produced one at a time: `for i in range(1000000)` does not build a list of a million numbers.

## Conversions

| Function    | Description             | Example           |
| ----------- | ----------------------- | ----------------- |
| `int(x)`    | Converts to an integer  | `int(3.9) → 3`    |
| `float(x)`  | Converts to a float     | `float(2) → 2.0`  |
| `decimal(x)` | Converts to a decimal  | `decimal(2) / 4 → 0.5` |
| `string(x)` | Converts to a string    | `string(0) → '0'` |
| `char(code)` | The character with that code | `char(65) → Some('A')` |

`int` truncates toward zero. A float or a decimal too large for an `int` panics (see [Primitive Types](primitive-types.md)).

`decimal(x)` of a literal is the literal as written (`decimal(0.1)` is exactly one tenth); of a `float`, it is the shortest decimal that is the same float again, so nothing is lost either way: `decimal(0.1 + 0.2)` is `0.30000000000000004`, and `float(decimal(x)) == x`. `float(x)` of a decimal is the nearest float.

Converting a `string` can fail, so `int(x)`, `float(x)` and `decimal(x)` return a `Result[int, ParseError]` (or `Result[float, ParseError]`, `Result[decimal, ParseError]`) when `x` is a string. `decimal('0.1')` is exact; a text with more than 34 significant digits is rounded:

```koji
type ParseError = {
	InvalidNumber(text: string) // not a number: 'abc', 'nan', 'inf'
	OutOfRange(text: string) // too large: '99999999999999999999', '1e400'
}
```

```koji
int(3.9) // 3
int('0') // Ok(0)
int('abc') // Err(InvalidNumber('abc'))

match (int('42')) {
	Ok(value) => print(value + 1) // 43
	Err(InvalidNumber(text)) => print(`not a number: ${text}`)
	Err(OutOfRange(text)) => print(`too large: ${text}`)
}
```

`string(x)` gives the same text as `print(x)`.

## `panic`

Stops the program with an error message. Use it for states that should be impossible (bugs), not for expected failures, which are `Result` values.

```koji
function check(age: int): int {
	if (age >= 0) { return age }

	panic('age cannot be negative')
}
```

A panic cannot be caught: Koji has no `try` / `catch`. Index out of range, division by zero and overflow also panic (see [Error Handling](error-handling.md)).
