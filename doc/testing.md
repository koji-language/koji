# Testing

Tests are written in Koji, next to the code they test, and run with `kojic test`.

## Tests

A test is a block with a name, at the top level of a file:

```koji
function gcd(a: int, b: int): int {
    return if (b == 0) { a } else { gcd(b, a % b) }
}

export function lcm(a: int, b: int): int {
    return a / gcd(a, b) * b
}

test 'gcd of two numbers' {
    assert(gcd(12, 18) == 6)
    assert(gcd(7, 5) == 1, 'coprime numbers')
}

test 'lcm' {
    assert(lcm(4, 6) == 12)
}
```

-   The name is any text; two tests of a file cannot have the same one.
-   A test sees everything in its file, private functions and types included, like any function of the file. It takes no parameters and returns nothing (`return` ends it early).
-   `kojic run` and `kojic build` leave the tests out: they cost nothing to a program. `kojic check` checks them.
-   `test` is a word only there: elsewhere it is an ordinary name (`immut test = 3`).

A test passes when it ends, and fails when it panics: with [`assert`](builtin-functions.md#assert), or with any other panic (an index out of range, an overflow, `panic(...)`).

## `kojic test`

```sh
kojic test              # the tests of every .kj file here and in the directories below
kojic test math.kj      # the tests of a file
kojic test src lib      # those of some directories
```

Files without tests are left out, and so are files and directories whose names start with a dot. For each file:

-   the file is checked and compiled with its tests, as a program;
-   each test runs in a process of its own: the top-level statements of the file first (as an import runs them), then the test. A test that fails stops only itself, and each test starts from the same state, whatever the others did;
-   what a test prints is shown only when it fails.

If `lcm` had a mistake (returning `a * b`, after printing `computing...`), the report of the file above would be:

```
ok    math.kj: gcd of two numbers
FAIL  math.kj: lcm
    panic: math.kj:15: assertion failed: lcm(4, 6) == 12
      left:  24
      right: 12
    output:
        computing...

1 passed, 1 failed (0.12 s)
```

The exit code is `0` when every test passes, and `1` when one fails or a file cannot be built (a syntax or type error, also reported).

Only the tests of the files named are run, not those of the modules they import: each module is tested by its own file.
