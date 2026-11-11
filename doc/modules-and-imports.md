# Modules and Imports

Modules organize code in separate files. Each file is a module: it can import a whole module, or specific items from it.

## Declaring a Module

```koji
// file: math.kj

export function add(a: int, b: int): int {
	return a + b
}

export function subtract(a: int, b: int): int {
	return a - b
}
```

Everything in a module is private by default. Only declarations marked with `export` (functions, `mut` and `immut` variables, and types) can be imported by other modules. Exporting a sum type also exports its variants, and the methods of an exported type are available wherever the type is.

```koji
// file: math.kj

function clamp(x: int): int { ... } // private: only usable inside math.kj

export immut pi: decimal = 3.14159
```

```koji
import { clamp } from 'math.kj' // Error! 'clamp' is not exported by 'math.kj'
```

`export` is only allowed at the top level of a file, and must be followed by `function`, `mut`, `immut` or `type`.

## Importing a Module

```koji
// file: main.kj

import math from 'math.kj'

immut result: int = math.add(5, 10)

print(result) // 15
```

The module is named after the file (`math`), and its items are used as `math.add`. To use them without the prefix, import specific items:

```koji
// file: main.kj

import { add, subtract } from 'math.kj'

immut result: int = add(5, 10)

print(result) // 15
```

Paths are relative to the importing file. Standard library modules are imported by name, without a path: `import math` (see [Standard Library](stdlib.md)).

Imports apply to the whole file, wherever they are written: types and functions declared before an `import` can already use it. The convention is to write them at the top of the file. An `import` inside a function or a block applies to that block only; the module runs the first time the block runs.

## Exported Variables

An exported `mut` variable is read-only outside its module: only the module itself can assign to it. Importers always see its **current** value.

```koji
// file: counter.kj

export mut count = 0

export function increment() {
	count += 1
}
```

```koji
// file: main.kj

import { count, increment } from 'counter.kj'

increment()

print(count) // 1

count = 5 // Error! 'count' is read-only outside 'counter.kj'
```

Outside its module, it cannot be changed in any way: neither given a new value nor changed in its contents.

## Private Types in Public Declarations

An exported declaration cannot use a private type, because other modules could not name it:

```koji
type Helper = {
	value: int
}

export function make(): Helper { ... } // Error! 'make' is exported, but its type uses the private type 'Helper'
```

## Opaque Types

`export opaque type` exports a type without its structure. Other modules use its values (they pass them, store them, call its methods), but they cannot read or change its fields, build it with a literal, or build or match its variants: only its module does, and the functions it exports are the way to get one.

```koji
// file: counter.kj

export opaque type Counter = {
    name: string
    count: int
}

export function counter(name: string): Counter {
    return { name: name, count: 0 }
}

function Counter.add(mut self) {
    self.count += 1
}

function Counter.display(self): string {
    return `<counter ${self.name}=${self.count}>`
}
```

```koji
// file: main.kj

import { Counter, counter } from 'counter.kj'

mut clicks = counter('clicks')
clicks.add()

print(clicks) // <counter clicks=1>
print([clicks]) // [<counter clicks=1>]
print(clicks.count) // Error! Counter is opaque: its fields are not visible outside its module
immut other: Counter = { name: 'x', count: 0 } // Error! Counter is opaque: it is built by the functions of its module, not with a literal
```

Since its structure is hidden, an opaque value is always shown by its type's `display` method, also inside other values and by `debug()`; without one, by its name (`<Counter>`). The structure of an opaque type may use the private types of its module. The `Regex` of the standard library is an opaque type.

## Packages

Modules of other projects are imported by the path of their repository (`import toml from 'github.com/bia/toml'`), once the program's `koji.mod` requires them: see [Packages](packages.md).
