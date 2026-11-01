# Values and Memory

ez manages memory without a garbage collector and without manual frees. Two rules cover almost every program:

-   **Values are copied.** Assigning or passing a list or an object gives an independent copy.
-   **Resources are moved.** A value that holds a resource (a file, or a type with `drop`) has exactly one owner, and it is released when its owner goes out of scope.

## Value Semantics

Assigning a value, passing it to a function or storing it in a list gives a **copy**: changing one of them never changes the other.

```ez
a = [1, 2, 3]
b = a
b[0] = 9

print(a) // [1, 2, 3]
print(b) // [9, 2, 3]
```

Copies are cheap: the memory is shared until one of the copies changes (copy-on-write), and only then is the changed part copied.

There are no references, so there is no aliasing: a function can only change a variable passed with [`mut`](functions.md#mut-parameters), and a closure keeps the values it had when it was created. `==` compares values, and [`is`](operators.md#comparison-operators) only checks variants.

## Move-Only Values

Some values cannot be copied: a `File` (two copies would close the same file), and any type that declares a [`drop` method](#drop). A type that holds one of them (in a field, a list or a variant) cannot be copied either.

A move-only value has exactly one owner. Assigning it to another variable, returning it or storing it in another value **moves** it, and the variable it came from cannot be used anymore:

```ez
import file

f = file.open('notes.txt', file.Read)?
g = f        // the file moves to g
print(f.read()) // Error! 'f' cannot be used here: its value was moved (or closed) on line 4
```

A variable that was moved can get a new value (`f = file.open(...)?`), and a move inside a branch that ends with `return` does not count after the `if`.

### Borrowing

Parameters, `self`, the `for` variable and the bindings of a `match` on a variable **borrow** their value: they can use it, but they cannot move it. Passing a file to a function does not move it:

```ez
function show(f: file.File) {
    print(f.read())
}

f = file.open('notes.txt', file.Read)?
show(f)
show(f) // still usable: show only borrowed it
```

### Rules

The compiler checks that a move-only value is never copied:

-   A value cannot be moved out of a parameter, `self`, the `for` variable or a `match` binding on a variable.
-   A value cannot be copied out of a field or an element (`g = pair.first`).
-   A variable declared outside a loop or a function cannot be moved inside it: the next iteration, or the next call, would use it again.
-   A function cannot use a move-only variable of the functions around it (closures capture copies).
-   `for mut` cannot be used on a list of move-only values.

## Drop

When the owner of a move-only value goes out of scope, the value is **dropped**: a file is closed, and the `drop` method of the type runs. This happens at the end of the block, when the function returns (also through `?`), when the variable gets a new value, and at the end of the program. Variables are dropped in the reverse order of their declaration.

```ez
type Connection = {
    name: string
}

function Connection.drop(mut self) {
    print(`closing ${self.name}`)
}

function Connection.open(name: string): Connection {
    return { name: name }
}

if (true) {
    db = Connection.open('db')
    print('working')
} // closing db
```

-   `drop` is declared as `function Type.drop(mut self)`, and it cannot be called directly.
-   After the `drop` method, the fields of the value are dropped too (a `Connection` that holds a `File` closes it).
-   A move-only value that is created and not stored (a statement, or an argument such as `show(Connection.open('x'))`) is dropped right after it is used.

So there is no `defer`: cleanup belongs to the value that needs it.

## Files

A `File` is move-only, and it is closed when its owner goes out of scope, so most programs never call `close()`:

```ez
import file

function readNotes(): Result[string, file.FileError] {
    f = file.open('notes.txt', file.Read)?
    return f.read() // 'f' goes out of scope: the file is closed
}
```

`f.close()` closes the file earlier. It consumes `f`: using `f` afterwards is a compile error, not a runtime failure.

## Sharing

To share data between several parts of a program (a graph, a cache), keep the values in a list and store their indices ("arenas") instead of references.
