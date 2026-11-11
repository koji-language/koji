# Functions

Functions allow you to encapsulate logic, making your code reusable and easier to maintain.

## Basic Syntax

```koji
function sum(a: int, b: int): int {
	return a + b
}

print(sum(2, 2)) // 4

function factorial(x: int): int {
    if (x <= 1) { return 1 }

    return x * factorial(x - 1)
}

print(factorial(4)) // 24
```

Parameters always have a type. The return type comes after `)`; without it, the function returns nothing (`void`).

A function whose body is a block can return its last expression without `return`:

```koji
function max(a: int, b: int): int {
	if (a > b) { a } else { b }
}
```

## Anonymous Functions

```koji
immut double = (x: int): int => {
	return x * 2
}

print(double(10)) // 20
```

When the body is a single expression, the braces and `return` can be left out, and the return type is inferred:

```koji
immut triple = (x: int) => x * 3

print(triple(10)) // 30
```

The type of a function is written `(parameters) => return type`: `(int) => int`.

Where the type of the function is known (a variable with a type, the parameter of a function), its parameters need no types: they have the ones of that type.

```koji
immut half: (int) => int = (x) => x / 2

function twice(f: (int) => int, x: int): int {
    return f(f(x))
}

print(twice((x) => x + 1, 5)) // 7
```

## Capturing Variables

A function created inside another function can read the variables around it. It captures a **read-only copy**: it sees the values from the moment it was created, and cannot change them.

```koji
function makeAdder(amount: int): (int) => int {
	return (x: int) => x + amount
}

immut addFive = makeAdder(5)

print(addFive(1)) // 6

function count(): int {
	mut total = 0
	immut add = () => { total += 1 } // Error! cannot modify 'total' here: functions capture a read-only copy of the variables around them

	return total
}
```

Variables at the top level of a module are not captured: functions can read and change them.

```koji
mut total = 0

function add(n: int) {
	total += n
}

add(2)

print(total) // 2
```

## Parameters Are Read-Only

Arguments are copies (see [Values and Memory](memory.md)): a function cannot change the caller's variables, except through [`mut`](#mut-parameters). A function cannot change its parameters, or their contents either:

```koji
function clear(items: int[]) {
	items[0] = 0 // Error! cannot modify 'items': parameters can only be modified when declared 'mut'
}
```

## `mut` Parameters

A parameter marked `mut` can be changed, and the change is visible to the caller when the function returns. The call repeats `mut`, so it is clear where a variable may change:

```koji
function reset(mut n: int) {
	n = 0
}

function addFirst(mut items: int[], amount: int) {
	items[0] += amount
}

mut count = 5

reset(mut count)

print(count) // 0

mut numbers = [1, 2, 3]

addFirst(mut numbers, 10)

print(numbers) // [11, 2, 3]
```

-   Only a `mut` variable (not an `immut`), or a field or element of one, can be passed as `mut`: `reset(mut person.age)`, `reset(mut numbers[0])`.
-   The argument must have exactly the type of the parameter: an `int` variable cannot be passed as `mut x: float`.
-   A function with `mut` parameters can only be called, not stored in a variable, and anonymous functions cannot have `mut` parameters.
-   Methods use `mut self` instead, and their call has no marker (`counter.increment()`); see [Custom Types](custom-types.md#methods).

## `take` Parameters

A parameter marked `take` takes ownership of its argument, which moves into the function. It matters for move-only values, such as files: see [Values and Memory](memory.md#taking-ownership).

```koji
function Logger.create(take out: file.File): Logger {
    return { out: out }
}
```

## Methods and Associated Functions

A function prefixed with a type is part of that type. With `self` as the first parameter, it is a **method**, called on a value; without `self`, it is an **associated function**, called on the type (useful for constructors):

```koji
type Person = {
	name: string
	age: int
}

function Person.create(name: string): Person {
	return { name: name, age: 0 }
}

function Person.greet(self) {
	print(`Hello, I'm ${self.name}!`)
}

immut person = Person.create('John')

person.greet() // Hello, I'm John!
```

See [Custom Types](custom-types.md#methods) for the rules.
