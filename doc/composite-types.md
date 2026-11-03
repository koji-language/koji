# Composite Types

## List

An ordered list of elements of the same type. The type of a list of `T` is written `T[]`.

```koji
immut numbers: int[] = [1, 2, 3]

immut coordinates: int[][] = [[1, 2], [3, 4]]

immut positions: int[][][] = [[[1, 2], [3, 4]], [[5, 6], [7, 8]]]
```

Elements are accessed by their index, starting at `0`. An index out of range panics.

```koji
print(numbers[0]) // 1
print(coordinates[1][0]) // 3
print(numbers.length()) // 3
print(2 in numbers) // true

numbers[0] = 10 // changes the first element
```

Methods such as `push`, `pop`, `insert` and `sort` change a list (see [Built-in Methods](builtin-methods.md#list-methods)).

### Slices

`list[start..end]` is a copy of the items from `start` (included) to `end` (excluded). Either bound can be left out. Strings are sliced the same way, by characters:

```koji
immut numbers = [10, 20, 30, 40]

print(numbers[1..3]) // [20, 30]
print(numbers[..2]) // [10, 20]
print(numbers[2..]) // [30, 40]
print('Hello, Koji'[7..]) // Koji
```

Bounds out of range, or a start after the end, panic.

To go through a list, use a [`for` loop](control-flow.md#for-loop).

## Map

`Map[K, V]` maps keys of type `K` to values of type `V`. Keys must be [equatable](generics.md#constraints): numbers, strings, chars, booleans, and types made of them. A map literal is written with brackets, and `[:]` is the empty map:

```koji
mut ages: Map[string, int] = ['Ana': 30, 'Bia': 25]
immut names = [1: 'one', 2: 'two'] // Map[int, string]
mut empty: Map[string, int] = [:]

print(ages['Ana']) // 30: a missing key panics
print(ages.get('Carl')) // None
print('Bia' in ages) // true

ages['Carl'] = 41 // adds or replaces
ages['Ana'] += 1

ages.remove('Bia')

for name in ages {
    print(name, ages[name]) // in the order the keys were added
}
```

Like lists, maps are values: assigning a map gives an independent copy. See the [methods of maps](builtin-methods.md#map-and-set-methods).

## Set

`Set[T]` holds equatable items without repetition. It is created from a list:

```koji
mut seen: Set[string] = Set.from(['a', 'b', 'a'])

seen.add('c')

seen.remove('a')

print(seen.has('b'), 'c' in seen) // true true
print(seen) // Set(['b', 'c'])
```

`for item in seen` goes through the items, in the order they were added.
