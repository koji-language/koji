# Primitive Types

## Numeric Types

-   `int`: 64-bit integer numbers.

```koji
immut age: int = 23
```

-   `float`: 64-bit floating-point numbers. A `float` is always a finite real number: there is no NaN and no infinity.

```koji
immut pi: float = 3.14
immut avogadro: float = 6.022e23 // scientific notation
```

A float literal too large to be represented (`1e400`) is a compile-time error.

An `int` can be used where a `float` is expected, and it becomes a `float`. This applies to a single value, and to the items of a list literal; a list that already exists is not converted, since that would copy it:

```koji
immut ratio: float = 1 // 1.0
immut weights: float[] = [1, 2.5] // [1.0, 2.5]
immut counts = [1, 2]
immut values: float[] = counts // Error! cannot assign a value of type int[] to 'values' of type float[]
```

-   `decimal`: decimal numbers with 34 significant digits. A `decimal` holds exactly the numbers written in base 10 with up to 34 digits: `0.1` is exactly one tenth, so `0.1 + 0.2 == 0.3`. Like a `float`, it is always finite.

```koji
immut price: decimal = 19.99
immut tax: decimal = 0.07

print(price * tax) // 1.3993
print(decimal(1) / 3) // 0.3333333333333333333333333333333333
```

A literal with a point or an exponent is a `decimal` where a `decimal` is expected (a declared type, an argument, the other side of an operator): `price * 1.5` multiplies two decimals. Elsewhere it is a `float`. A decimal literal must hold exactly: a literal with more than 34 significant digits, or out of the range of a decimal (about 1e-6176 to 1e6145), is a compile-time error.

An operation rounds its exact result to 34 significant digits, to the nearest (half to even): `decimal(2) / 3` is `0.6666666666666666666666666666666667`. `1.50` and `1.5` are equal, and both are shown as `1.5`.

An `int` becomes a `decimal` like it becomes a `float`. A `float` and a `decimal` never mix: `price + 0.5` is a decimal when `price` is one, but a `float` variable must be converted with `decimal(x)` (and a decimal with `float(x)`). `**` takes an `int` exponent for a `decimal` (`price ** 2`).

```koji
immut ratio = 0.5 // a float
immut amount: decimal = 10

print(amount * decimal(ratio)) // 5.0
print(amount + ratio) // Error! operator '+' cannot be applied to decimal and float
```

When a numeric result does not fit in an `int`, a `float` or a `decimal` (a representation limit), the program panics. For example, `9223372036854775807 + 1` and `1e308 * 10` stop the program. A `decimal` also stops the program when a result is too small to be anything but zero (`decimal underflow`). Results that do not exist in the real numbers (a domain problem, like the square root of a negative number) are reported by the `math` functions as a `MathError` (see [math](standard-library/math.md)).

## Text

-   `string`: a sequence of characters.

```koji
immut name: string = 'Mary'
```

## Characters

-   `char`: a single Unicode character (a code point).

```koji
immut initial: char = 'K' // a one-character literal is a char where a char is expected

print(initial.code()) // 75
print(char(97)) // Some('a'): None for a code that is not a character
```

Indexing a string gives a `char`, and so does going through it with `for`:

```koji
immut name = 'Koji'

print(name[0]) // K

for c in name {
    print(c.code())
}

print(name[0] == 'K') // true
```

Characters can be compared (`<`, `>`), by their code. They are not numbers: `'a' + 1` is an error; use `c.code()` and `char(code)`.

## Boolean

-   `boolean`: logical values (`true`, `false`).

```koji
immut isStudent: boolean = true
```

## Missing Values

The Koji language does **not** include `null` or `undefined`.

To represent the absence of a value, use the `Option[T]` sum type:

```koji
immut name: Option[string] = None
```

A value that is only known later starts as `None` and is set afterwards:

```koji
mut x: Option[int] = None

// Later in the code
x = Some(42)
```
