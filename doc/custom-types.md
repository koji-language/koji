# Custom Types

Custom types are declared with `type`. This page covers shape types (named fields), intersections, and the methods of a type. Types with variants are covered in [Sum Types](sum-types.md).

## Shape Types

Types with fixed named fields, for values with a known structure.

```koji
type Person = {
    name: string
    age: int
}

immut person: Person = {
    name: 'John',
    age: 20
}
```

Shape types can be nested, and used for variables, function parameters and return types:

```koji
type Address = {
    street: string
    city: string
}

type User = {
    id: int
    name: string
    address: Address
}

immut user: User = {
    id: 42,
    name: 'Bob',
    address: {
        street: '123 Main St',
        city: 'Exampleville'
    }
}

function printUser(user: User) {
    print(`${user.name} lives in ${user.address.city}`)
}
```

All fields are required, and an object literal cannot have extra fields. Fields can be separated by commas or new lines.

Shape types are **nominal**: two types with the same fields are still different types. An object literal is a value of the type expected where it is written: the type of the variable, of the parameter, of the return value or of the list.

```koji
type Celsius = { value: decimal }

type Fahrenheit = { value: decimal }

immut celsius: Celsius = { value: 20.0 }
mut fahrenheit: Fahrenheit = { value: 68.0 } // Ok

fahrenheit = celsius // Error! Celsius is not Fahrenheit
```

So an object literal always needs a known type. Without one, it is an error:

```koji
immut point = { x: 1, y: 2 } // Error! this object literal needs a type

type Point = {
    x: int
    y: int
}

immut origin: Point = { x: 0, y: 0 } // Ok

function move(point: Point) { ... }

move({ x: 1, y: 2 }) // Ok: the parameter gives the type
```

## Dictionaries

Koji has no index signatures (`{ [key: string]: string }`): a dictionary is a [`Map`](composite-types.md#map), which accepts any equatable key type. A type can name a map:

```koji
type Dictionary = Map[string, string]

immut translations: Dictionary = [

    'hello': 'olá',
    'world': 'mundo'
]
```

## Type Union

Koji does not support untagged type unions (e.g. `string | int`).

When a value can be one of several types, use a sum type (tagged union). Each variant is explicitly tagged, so the compiler can verify exhaustiveness and the programmer always knows which variant they are handling.

```koji
type Value = string | int // Error! Untagged union is not allowed

// Tagged union: each variant is named
type Value = {
    Text(value: string)
    Num(value: int)
}
```

This ensures that "it can be A or B" always comes with a tag you can match on. There is no "it's one of them, but I don't know which": the type system forbids it.

## Type Intersection

A type defined with intersection (`&`) merges the fields of its parts into a new type:

```koji
type Animal = {
    name: string
}

type Dog = Animal & {
    breed: string
}

immut rex: Dog = { name: 'Rex', breed: 'Beagle' }
```

## No Subtyping

Koji has no subtyping. A type is not accepted where another type is expected, even when it has all of its fields: `Dog` above has the fields of `Animal`, but it is not an `Animal`.

```koji
function greet(animal: Animal) {
    print(animal.name)
}

greet(rex) // Error! argument 1 must be Animal, found Dog
greet({ name: rex.name }) // Ok
```

## Methods

A function declared as `function Type.name(...)` belongs to `Type`. With `self` as the first parameter, it is a **method**, called on a value:

```koji
type Person = {
    name: string
    age: int
}

function Person.greet(self) {
    print(`Hello, I'm ${self.name}!`)
}

immut person: Person = {
    name: 'John',
    age: 20
}

person.greet() // Hello, I'm John!
```

```koji
type Rect = {
    width: int
    height: int
}

function Rect.area(self): int {
    return self.width * self.height
}

immut rect: Rect = {
    width: 3,
    height: 4
}

print(rect.area()) // 12
```

`self` has no type annotation (it is always the type) and, like every parameter, it is read-only.

### Associated Functions

Without `self`, the function is an **associated function**, called on the type: `Type.name(...)`. It is useful for constructors:

```koji
function Rect.square(side: int): Rect {
    return { width: side, height: side }
}

print(Rect.square(3).area()) // 9
```

### Methods of Sum Types

Sum types can have methods too:

```koji
type Shape = {
    Circle(radius: decimal)
    Square(side: decimal)
}

function Shape.area(self): decimal {
    return match (self) {
        Circle(r) => 3.14 * r * r
        Square(s) => s * s
    }
}

print(Square(2.0).area()) // 4
```

### Methods That Change the Value

A method that changes its value declares `mut self`. The call needs no marker, but the value must be a `mut` variable (or a field or element of one), not an `immut`:

```koji
type Counter = {
    count: int
}

function Counter.increment(mut self) {
    self.count += 1
}

mut counter: Counter = { count: 0 }

counter.increment()

print(counter.count) // 1
```

`mut self` can also give the value a new value, which is useful for a sum type that changes state:

```koji
type Light = {
    On
    Off
}

function Light.toggle(mut self) {
    self = match (self) {
        On => Off
        Off => On
    }
}

mut light: Light = Off

light.toggle()

print(light) // On
```

### `drop`

A type can declare `function Type.drop(mut self)`, which runs by itself when a value of the type goes out of scope (to close a connection, for example). Such a type is move-only: its values are moved, not copied. See [Values and Memory](memory.md#drop).

### Rules

-   Methods are declared at the top level of the file that declares the type, and take no `export`: they are available wherever the type is (`import { Point } from 'geometry.kj'` then `Point.origin()`, or `geometry.Point.origin()`).
-   A method cannot have the same name as a field of its type.
-   A method with `self` is called on a value (`person.greet()`), and an associated function on the type (`Person.create()`); the other way around is an error.

## Display

`print` and string interpolation show a value with its type's `display` method, when there is one. It is declared as `function Type.display(self): string`:

```koji
type Person = {
    name: string
    age: int
}

function Person.display(self): string {
    return `${self.name} (${self.age})`
}

immut ana: Person = { name: 'Ana', age: 30 }

print(ana) // Ana (30)
print(`Hi, ${ana}!`) // Hi, Ana (30)!
print(ana.debug()) // { name: 'Ana', age: 30 }
```

-   Every value has `display()` (the text `print` shows) and `debug()` (the structure of the value, as written in code). Without a `display` method, both show the structure, except for strings and standard library errors, whose `display()` is readable (`'abc' is not a valid number`).
-   Inside a list or another value, the structure is shown: `print([ana])` gives `[{ name: 'Ana', age: 30 }]`.
-   `debug` cannot be declared: it always shows the structure.
