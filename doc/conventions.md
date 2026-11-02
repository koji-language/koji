# Conventions

-   Variables and functions use `camelCase`: `userName`, `parseAge`.
-   Types and variants use `PascalCase`: `Person`, `FileError`, `NotFound`.
-   Parentheses are required around conditions (`if (x > 0)`, `while (running)`, `match (value)`) and in function calls.
-   Strings can use single (`'`) or double (`"`) quotes, but **the convention is to prefer single quotes**. Backticks are for [interpolation](string-interpolation.md).
-   Imports are written at the top of the file.
-   A variable is annotated only when its type is not evident from its value: `immut person = Person.create('Ana')`, but `immut person: Person = { name: 'Ana', age: 30 }` (an object literal needs the type).
-   Variables are `immut` unless they need to change.
-   A comment after code on the same line is separated from it by exactly one space. Comments are not aligned into columns:

    ```koji
    immut count = 0 // number of items
    immut total = 0.0 // sum of the prices
```
