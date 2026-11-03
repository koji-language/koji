# Variable Types

Every variable is declared with one of two keywords:

| Keyword | Change the variable | Change its contents |
| ------- | ------------------- | ------------------- |
| `mut`   | yes                 | yes                 |
| `immut` | no                  | no                  |

```koji
mut count = 0
immut name = 'Ana'
mut items: int[] = [] // with a type annotation

count += 1
name = 'Bia' // Error! cannot modify 'name': it is immut

items.push(2)
```

A declaration always has its keyword: `x = 1` only changes an existing `mut` variable. A name that was never declared is an error, so a typo (`cuont = 1`) is reported instead of creating a new variable:

```koji
mut count = 0

cuont = 1 // Error! 'cuont' is not declared: declare it with 'mut cuont = ...' (or 'immut' if it never changes)
```

`var`, `const` and `let`, which other languages use, are reserved words: using one gives an error that points to `mut` and `immut`.

## `mut`

A `mut` variable can be changed in any way: given a new value, or changed in its contents (a field, an element, `push`...).

```koji
type Person = {
    name: string
    age: int
}

mut person: Person = { name: 'Ana', age: 30 }

person.age = 31 // changes a field
person = { name: 'Bia', age: 25 } // gives it a new value
```

## `immut`

An `immut` variable never changes: giving it a new value is an error, and so is changing a field, an element or a nested value.

```koji
immut person: Person = { name: 'Ana', age: 30 }

person.age = 31 // Error! cannot modify 'person': it is immut

immut matrix = [[1, 2], [3, 4]]

matrix[0][0] = 5 // Error! cannot modify 'matrix': it is immut
```

Values are copied when they are assigned (see [Values and Memory](memory.md)), so changing a `mut` copy of an `immut` value is fine: the `immut` one does not change.

```koji
immut original = [1, 2]
mut copy = original

copy.push(3)

print(original, copy) // [1, 2] [1, 2, 3]
```

Prefer `immut` for values that do not need to change: it tells the reader that the value stays the same.

## Types

The type of a variable comes from its value, or from an annotation (`mut items: int[] = []`). Every variable must have a known type by the end of its block: an empty value needs an annotation, unless a later use tells its type.

```koji
mut items = []

items.push(1) // items is an int[]

mut names = [] // Error! the type of 'names' cannot be inferred (it is ?[]): annotate it, as in 'mut names: int[] = ...'

mut found: Option[string] = None // the annotation gives the type
```

## Scope

A variable belongs to the block where it is declared: the file, a function, or any `{ ... }` block such as the body of an `if` or a loop. It is visible in that block and in the blocks inside it.

```koji
mut total = 0 // in the file

function add(n: int) {
    immut doubled = n * 2 // in the function

    total += doubled
}

add(1)

print(total) // 2
print(doubled) // Error! 'doubled' is not declared
```

Assigning to a variable of an outer block changes it:

```koji
mut count = 0

if (true) {
    count = 1 // changes the outer 'count'
}

print(count) // 1
```

## Other Read-Only Variables

Some variables are read-only without a keyword:

-   function parameters, unless they are declared `mut` (see [Functions](functions.md#parameters-are-read-only));
-   the variable of a `for` loop, unless it is written `for mut` (see [Control Flow](control-flow.md#the-loop-variable));
-   the variables bound by a `match` pattern.
