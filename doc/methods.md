# Built-in Methods

Methods that the language provides. To declare methods for your own types, see [Custom Types](custom-types.md#methods).

## Methods of Every Value

| Method        | Description                                                                      | Example                          |
| ------------- | -------------------------------------------------------------------------------- | -------------------------------- |
| `x.display()` | The text `print` shows (readable; see [Display](custom-types.md#display))         | `'hi'.display() → 'hi'`          |
| `x.debug()`   | The structure of the value, as it is written in code                             | `'hi'.debug() → "'hi'"`          |

For a standard library error, `display()` is a readable message (`'abc' is not a valid number`) and `debug()` its structure (`InvalidNumber('abc')`).

## String Methods

| Method                  | Description                                  | Example                                 |
| ----------------------- | -------------------------------------------- | --------------------------------------- |
| `str.length()`          | Number of characters                         | `'abc'.length() → 3`                    |
| `str.trim()`            | Removes whitespace from the start and end    | `'  hi  '.trim() → 'hi'`                |
| `str.upper()`           | Converts to uppercase                        | `'hello'.upper() → 'HELLO'`             |
| `str.lower()`           | Converts to lowercase                        | `'HELLO'.lower() → 'hello'`             |
| `str.isSpace()`         | Whether it has only whitespace               | `'   '.isSpace() → true`                |
| `str.isNumeric()`       | Whether it has only digits                   | `'123'.isNumeric() → true`              |
| `str.isAlpha()`         | Whether it has only letters                  | `'abc'.isAlpha() → true`                |
| `str.isAlphaNumeric()`  | Whether it has only letters and digits       | `'abc123'.isAlphaNumeric() → true`      |
| `str.isUpper()`         | Whether all its letters are uppercase        | `'ABC'.isUpper() → true`                |
| `str.isLower()`         | Whether all its letters are lowercase        | `'abc'.isLower() → true`                |
| `str.isTitle()`         | Whether every word starts with an uppercase letter followed by lowercase ones | `'Hello World'.isTitle() → true` |
| `str.split(sep)`        | Splits at every `sep`                        | `'a b'.split(' ') → ['a', 'b']`         |
| `str.replace(old, new)` | Replaces every `old` with `new`              | `'hello'.replace('l', 'L') → 'heLLo'`   |
| `str.startsWith(s)`     | Whether it starts with `s`                   | `'Koji'.startsWith('Ko') → true`        |
| `str.endsWith(s)`       | Whether it ends with `s`                     | `'Koji'.endsWith('ji') → true`          |
| `str.indexOf(s)`        | Position (in characters) of the first `s`    | `'Koji'.indexOf('j') → Some(2)`         |
| `str.chars()`           | Its characters                               | `'ab'.chars() → ['a', 'b']`             |

## Char Methods

| Method       | Description             | Example              |
| ------------ | ----------------------- | -------------------- |
| `c.code()`   | The code of the character | `'A'.code() → 65`  |

The `is...` methods return `false` for an empty string.

## List Methods

| Method                | Description                                                    | Example                              |
| --------------------- | -------------------------------------------------------------- | ------------------------------------ |
| `list.length()`       | Number of items                                                | `[1, 2, 3].length() → 3`             |
| `list.push(x)`        | Adds `x` at the end                                            | `[1, 2]` → `[1, 2, 3]`               |
| `list.insert(i, x)`   | Adds `x` at position `i` (`i` can be the length)               | `insert(0, 9)` → `[9, 1, 2]`         |
| `list.pop()`          | Removes the last item: `Some(item)`, or `None` if empty        | `[1, 2].pop() → Some(2)`             |
| `list.removeAt(i)`    | Removes the item at `i`: `Some(item)`, or `None` out of range  | `[1, 2].removeAt(0) → Some(1)`       |
| `list.sort()`         | Sorts the items, which must be orderable                       | `[3, 1, 2]` → `[1, 2, 3]`            |
| `list.sortBy(f)`      | Sorts with `f(a, b)`: whether `a` goes before `b` (stable)     | `sortBy((a, b) => a.age < b.age)`    |
| `list.indexOf(x)`     | Position of the first `x` (equatable items)                    | `[5, 6].indexOf(6) → Some(1)`        |
| `list.join(sep)`      | Joins a list of strings, with `sep` between them               | `['a', 'b'].join(', ') → 'a, b'`     |

`push`, `insert`, `pop`, `removeAt`, `sort` and `sortBy` change the list, like a `mut self` method: the variable must be a `var` or a `const`, not an `imut` or a parameter without `mut`. They change it in place; other copies of the list do not change. To check whether a list has an item, use `x in list`.

## Dictionary Methods

| Method       | Description       | Example |
| ------------ | ----------------- | ------- |
| `d.length()` | Number of keys    |         |

## Building Text

There is no string builder: text is built with `+=`, which appends in place when the variable is the only owner of its string (strings have [value semantics](memory.md)), or with `join`:

```koji
var line = ''
for i in range(3) {
    line += `${i};`
}
print(line) // 0;1;2;

print(['a', 'b', 'c'].join(', ')) // a, b, c
```
