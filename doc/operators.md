# Operators

## Arithmetic Operators

| Operator | Description                       |
| -------- | --------------------------------- |
| `+`      | Addition                          |
| `-`      | Subtraction                       |
| `*`      | Multiplication                    |
| `/`      | Division                          |
| `**`     | Exponentiation                    |
| `%`      | Modulus (remainder)               |

Division follows the type of the operands: dividing two `int` values gives an `int` (truncated toward zero), and dividing `decimal` values gives a `decimal` (`7 / 2.0` is `3.5`); an `int` operand takes the type of the other one. A `decimal` and a `float` do not mix: one is converted with `decimal(x)` or `float(x)`. The remainder `%` has the sign of the dividend.

```koji
7 / 2 // 3
-7 / 2 // -3
7.0 / 2.0 // 3.5
7 / 2.0 // 3.5
-7 % 2 // -1
```

`//` is always a comment (see [Comments](comments.md)).

Operators cannot return a `Result`, so an invalid arithmetic operation panics:

-   division by zero (`1 / 0`, `1.5 % 0.0`, `0.0 ** -1.0`);
-   results that do not fit in an `int`, a `decimal` or a `float` (overflow);
-   `**` with a result that is not a real number (`(-8.0) ** 0.5`).

`0 ** 0` is `1`.

To handle the `**` cases instead of panicking, use `math.pow`, which returns a `MathError` (`Complex` or `Infinite`; see [math](standard-library/math.md)).

## Comparison Operators

| Operator | Description                                                |
| -------- | ---------------------------------------------------------- |
| `==`     | Equality                                                   |
| `!=`     | Inequality                                                 |
| `>`      | Greater than                                               |
| `<`      | Less than                                                  |
| `>=`     | Greater than or equal                                      |
| `<=`     | Less than or equal                                         |
| `is`     | Variant check (whether a value is a given variant)         |
| `in`     | Membership check (whether an element is in a collection)   |

`==` compares values: two lists or objects are equal when their contents are equal. Values that hold functions or move-only values (such as files) cannot be compared (see [Constraints](generics.md#constraints)).

```koji
type Person = {
	name: string
	age: int
}

immut a: Person = { name: 'Ana', age: 30 }
immut b: Person = { name: 'Ana', age: 30 }

a == b // true: the fields are equal
```

`in` checks whether an element is in a list, or a text in a string:

```koji
'a' in 'banana' // true
2 in [1, 2, 3] // true
```

`is` checks the variant of a sum type value. Inside the `if`, the fields of the variant can be used:

```koji
immut result = Some(1)

if (result is Some) {
	print(result.value) // 1
}
```

## Logical Operators

| Operator | Description |
| -------- | ----------- |
| `and`    | Logical AND |
| `or`     | Logical OR  |
| `not`    | Logical NOT |

```koji
if (age >= 18 and not hasTicket) {
	print('buy a ticket first')
}
```

## Assignment Operators

| Operator | Description                                |
| -------- | ------------------------------------------ |
| `=`      | Assignment (assigns a value to a variable) |

## Compound Operators

| Operator | Description             | Equivalent |
| -------- | ----------------------- | ---------- |
| `+=`     | Add and assign          | x = x + y  |
| `-=`     | Subtract and assign     | x = x - y  |
| `*=`     | Multiply and assign     | x = x * y  |
| `/=`     | Divide and assign       | x = x / y  |
| `**=`    | Exponentiate and assign | x = x ** y |
| `%=`     | Modulus and assign      | x = x % y  |
| `&=`     | AND and assign          | x = x & y  |
| `\|=`    | OR and assign           | x = x \| y |
| `^=`     | XOR and assign          | x = x ^ y  |
| `<<=`    | Shift left and assign   | x = x << y |
| `>>=`    | Shift right and assign  | x = x >> y |

## String Operators

| Operator | Description                                  |
| -------- | -------------------------------------------- |
| `+`      | Concatenation (joins strings)                |
| `*`      | Repetition (repeats a string multiple times) |

## Bitwise Operators

Bitwise operators operate on the binary representations of integers.

| Operator | Description | Example (a = 5, b = 3) |
| -------- | ----------- | ---------------------- |
| `&`      | AND         | `a & b → 1`            |
| `\|`     | OR          | `a \| b → 7`           |
| `^`      | XOR         | `a ^ b → 6`            |
| `~`      | NOT         | `~a → -6`              |
| `<<`     | Shift left  | `a << 1 → 10`          |
| `>>`     | Shift right | `a >> 1 → 2`           |

Explanation:

-   `a = 5` → `0101` (binary), `b = 3` → `0011` (binary)
-   `a & b` → `0101 & 0011` = `0001` → `1`
-   `a | b` → `0101 | 0011` = `0111` → `7`
-   `a ^ b` → `0101 ^ 0011` = `0110` → `6`
-   `~a` → bitwise NOT of `0101` = `1010` (in two's complement, this is `-6`)
-   `a << 1` → shift `0101` one bit to the left = `1010` → `10`
-   `a >> 1` → shift `0101` one bit to the right = `0010` → `2`
