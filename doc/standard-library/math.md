# math

The `math` module provides mathematical functions and constants. It is written in Koji (`std/math.kj`): the floats with the C library, the decimals with the runtime.

## Functions

Every function takes an `int`, a `decimal` or a `float`. A `decimal` (or an `int`) gives a `decimal` and a `float` gives a `float`; a function of two arguments does not mix a decimal and a float.

-   `abs(x: int) -> int`, `abs(x: float) -> float`, `abs(x: decimal) -> decimal`  
    Returns the absolute value of `x`, with the same type as `x`.

-   `sqrt(x: decimal) -> Result[decimal, MathError]`, `sqrt(x: float) -> Result[float, MathError]`  
    Returns the square root of `x` (for a decimal, correctly rounded to 34 digits; the root of an `int` is a decimal). Fails with `Complex` when `x` is negative.

-   `pow(x: decimal, y: decimal) -> Result[decimal, MathError]`, `pow(x: float, y: float) -> Result[float, MathError]`  
    Returns `x` raised to the power of `y`. For decimals, an `int` exponent gives the exact power rounded once, and another exponent is computed with 64 digits and rounded to 34. Fails with `Complex` when `x` is negative and `y` is not an integer (e.g. `pow(-8, 0.5)`), and with `Infinite` when `x` is `0` and `y` is negative (e.g. `pow(0, -1)`, which is `1 / 0`). `pow(0, 0)` is `1`: that is the value of the expression (only the limit form 0⁰ is indeterminate).

-   `floor(x: decimal) -> int`  
    Returns the largest integer less than or equal to `x`.

-   `ceil(x: decimal) -> int`  
    Returns the smallest integer greater than or equal to `x`.

-   `round(x: decimal) -> int`  
    Rounds `x` to the nearest integer (halfway cases away from zero: `round(2.5)` is `3`).

## Errors

The `math` module follows one rule:

-   **Domain problems return a `MathError`.** The result does not exist in the real numbers.
-   **Representation limits panic.** The result exists, but does not fit in an `int`, a `decimal` or a `float` (e.g. `math.pow(10, 7000)`, or `math.round(1e300)`, which does not fit in an `int`).

```koji
type MathError = {
    Complex // the result exists only in the complex numbers: sqrt(-1)
    Indeterminate // indeterminate form (no current function returns it)
    Infinite // the result is infinite: pow(0, -1)
}
```

`Infinite` is not an overflow. To tell them apart, ask whether a larger type would fit the result: `pow(10, 7000)` is a finite number too large for a `decimal` (overflow, it panics), while `pow(0, -1)` is infinite, so no type could represent it (`Err(Infinite)`).

Operators cannot return a `Result`, so they panic instead: `1 / 0` and `0.0 ** -1.0` are division by zero (see [Operators](../operators.md)).

`decimal` and `float` have no NaN and no infinity, so these cases cannot be represented as a value (see [Primitive Types](../primitive-types.md)).

## Constants

-   `pi`  
    The mathematical constant π, as a decimal of 34 digits: `3.141592653589793238462643383279503`.

-   `e`  
    Euler's number, as a decimal of 34 digits: `2.718281828459045235360287471352662`.

## Example

```koji
import math

match (math.pow(5, 2)) {
    Ok(squared) => print(math.pi * squared)
    Err(error) => print(error)
}

match (math.sqrt(-1)) {
    Ok(root) => print(root)
    Err(Complex) => print('no real square root')
    Err(Indeterminate) => print('indeterminate')
    Err(Infinite) => print('infinite')
}
```
