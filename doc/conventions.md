# Conventions

-   Variables and functions use `camelCase`: `userName`, `parseAge`.
-   Types and variants use `PascalCase`: `Person`, `FileError`, `NotFound`.
-   Parentheses are required around conditions (`if (x > 0)`, `while (running)`, `match (value)`) and in function calls.
-   Strings can use single (`'`) or double (`"`) quotes, but **the convention is to prefer single quotes**. Backticks are for [interpolation](string-interpolation.md).
-   Imports are written at the top of the file.
-   A variable is annotated only when its type is not evident from its value: `immut person = Person.create('Ana')`, but `immut person: Person = { name: 'Ana', age: 30 }` (an object literal needs the type).
-   Variables are `immut` unless they need to change.
-   A blank line separates statements of different kinds: declarations, assignments, calls of different functions, `return`, and blocks (`if`, `for`, `while`, `match`). Consecutive statements of the same kind, such as a few declarations or several `print` calls, stay together. Functions, types and multi-line statements are always separated from their neighbors, and a comment on its own line goes with the statement below it:

    ```koji
    function factorial(x: int): int {
        if (x <= 1) { return 1 }

        return x * factorial(x - 1)
    }

    mut count = 0
    immut name = 'Ana'

    count += 1

    // Show the result.
    print(name)
    print(count)
    ```

-   Special cases come first, with an early `return` (or `break` and `continue` in a loop), so the main code is not nested inside `if` and `else`:

    ```koji
    function describe(age: int): string {
        if (age < 0) { return 'invalid' }

        if (age < 18) { return 'minor' }

        return 'adult'
    }
    ```

-   A comment after code on the same line is separated from it by exactly one space. Comments are not aligned into columns:

    ```koji
    immut count = 0 // number of items
    immut total = 0.0 // sum of the prices
    ```

-   A file ends with exactly one line break: the last line is terminated, and no blank lines follow it.
