# Generics

A generic function or type works with values of any type. Its **type parameters** are written between brackets after its name, like the built-in `Option[T]` and `Result[T, E]`.

## Generic Functions

```koji
function first[T](items: T[]): Option[T] {
    if (items.length() == 0) {
        return None
    }
    return Some(items[0])
}

print(first([1, 2, 3])) // Some(1)
print(first(['a'])) // Some('a')
```

Type arguments are never written at the call: they come from the arguments, or from the type expected where the call is written.

```koji
function map[A, B](items: A[], f: (A) => B): B[] {
    var result: B[] = []
    for item in items {
        result = result + [f(item)]
    }
    return result
}

print(map([1, 2, 3], (x: int) => `#${x}`)) // ['#1', '#2', '#3']

const nothing: Option[int] = first([]) // T is int, from the expected type
```

A generic function can only be called: it cannot be stored in a variable (`g = first` is an error), because its type arguments are only known at a call.

## Generic Types

```koji
type Pair[A, B] = {
    first: A
    second: B
}

type Tree[T] = {
    Leaf
    Node(value: T, left: Tree[T], right: Tree[T])
}

const pair: Pair[string, int] = { first: 'x', second: 2 }
const tree: Tree[int] = Node(1, Leaf, Leaf)
```

Where a generic type is written, it needs its arguments: `Pair[string, int]`, not `Pair`. Aliases (`type Name = ...`) cannot have type parameters.

## Methods of Generic Types

The methods of a generic type see its type parameters, without repeating them:

```koji
function Pair.create(first: A, second: B): Pair[A, B] {
    return { first: first, second: second }
}

function Pair.swap(self): Pair[B, A] {
    return { first: self.second, second: self.first }
}

const p = Pair.create(1, 'one') // a Pair[int, string]
print(p.swap().first) // one
```

A method can have type parameters of its own, after its name:

```koji
function Pair.mapFirst[C](self, f: (A) => C): Pair[C, B] {
    return { first: f(self.first), second: self.second }
}

print(p.mapFirst((n: int) => n * 10).first) // 10
```

## What a Type Parameter Can Do

Inside a generic function or type, a value of type `T` can be stored, passed, returned, put in lists and other values, and printed (`print`, interpolation, `display()`, `debug()`). It has no fields and no operators: `a == b` or `a < b` on two `T` values is an error, because nothing says what `T` supports.

To give a generic function a behavior, pass it as a function, as `map` does with `f`.

Type parameters cannot be move-only values yet (a `File`, or a type with `drop`): `Box[file.File]` is an error. The built-in `Option` and `Result` accept them.
