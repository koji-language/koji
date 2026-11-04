# Primitive Types

## Numeric Types

-   `int`: 64-bit integer numbers. They are written in base 10 or, after `0x`, in base 16.

```koji
immut age: int = 23
immut mask = 0xff // 255
```

-   `decimal`: numbers as in everyday arithmetic, with 34 significant digits. A literal with a point or an exponent is a `decimal`, exactly the number written: `0.1` is one tenth, so `0.1 + 0.2 == 0.3`.

```koji
immut price = 19.99 // a decimal
immut tax: decimal = 0.07

print(price * tax) // 1.3993
print(0.1 + 0.2) // 0.3
print(decimal(1) / 3) // 0.3333333333333333333333333333333333
print(6.022e23) // 602200000000000000000000
```

A decimal literal must hold exactly: one with more than 34 significant digits, or out of the range of a decimal (about 1e-6176 to 1e6145), is a compile-time error. An operation rounds its exact result to 34 significant digits, to the nearest (half to even): `decimal(2) / 3` is `0.6666666666666666666666666666666667`. A decimal is shown in full, without an exponent or trailing zeros: `1.50` and `1.5` are equal and both are shown as `1.5`, and `decimal(10) ** 20` is `100000000000000000000`.

-   `float`: 64-bit binary floating-point numbers, computed by the processor: fast, but most fractions written in base 10 have no exact value in base 2. A `float` is written and shown as it is stored, in base 16: a mantissa after `0x` and a power of 2 after `p`. `0x1.8p1` is 1.5 × 2 = 3.

```koji
immut half = 0x1p-1 // 0.5, exactly
immut tenth = float(0.1) // the float nearest to 0.1

print(tenth) // 0x1.999999999999ap-4
print(tenth + float(0.2)) // 0x1.3333333333334p-2
print(decimal(tenth + float(0.2))) // 0.30000000000000004
```

A float literal is exact: one with more digits than a float holds (53 bits), or out of its range, is a compile-time error. `float(x)` gives the float nearest to a decimal, and `decimal(x)` the shortest decimal that is the same float again, so a float goes to a decimal and back without losing anything.

A `float` and a `decimal` never mix: one of them is converted with `float(x)` or `decimal(x)`, which shows where a value is rounded. A decimal literal is not a float either: `immut x: float = 0.5` is an error, `float(0.5)` or `0x1p-1` is the float.

```koji
immut rate: float = 0x1p-2
immut amount = 10.0 // a decimal

print(amount * decimal(rate)) // 2.5
print(amount * rate) // Error! operator '*' cannot be applied to decimal and float: convert one of them with float(x) or decimal(x)
```

A `decimal` is for the numbers people write and read, the `float` for heavy computations (simulations, science, graphics); see [Conventions](conventions.md#numbers).

An `int` can be used where a `decimal` or a `float` is expected, and it is converted. This applies to a single value, and to the items of a list literal; a list that already exists is not converted, since that would copy it:

```koji
immut ratio: decimal = 1 // 1
immut weights: decimal[] = [1, 2.5] // [1, 2.5]
immut counts = [1, 2]
immut values: decimal[] = counts // Error! cannot assign a value of type int[] to 'values' of type decimal[]
```

`int / int` is an `int` (truncated toward zero); with a `decimal`, the result is a `decimal`. `**` raises a decimal to any decimal power; an `int` exponent is exact.

Neither a `decimal` nor a `float` has NaN or infinity: when a result does not fit in an `int`, a `decimal` or a `float` (a representation limit), the program panics. For example, `9223372036854775807 + 1` and `float(1e308) * float(10)` stop the program. A `decimal` also stops the program when a result is too small to be anything but zero (`decimal underflow`). Results that do not exist in the real numbers (a domain problem, like the square root of a negative number) are reported by the `math` functions as a `MathError` (see [math](standard-library/math.md)).

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
