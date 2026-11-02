# Composite Types

## List

An ordered list of elements of the same type. The type of a list of `T` is written `T[]`.

```koji
numbers: int[] = [1, 2, 3]

coordinates: int[][] = [[1, 2], [3, 4]]

positions: int[][][] = [[[1, 2], [3, 4]], [[5, 6], [7, 8]]]
```

Elements are accessed by their index, starting at `0`. An index out of range panics.

```koji
print(numbers[0]) // 1
print(coordinates[1][0]) // 3
print(numbers.length()) // 3
print(2 in numbers) // true

numbers[0] = 10 // changes the first element
```

Methods such as `push`, `pop`, `insert` and `sort` change a list (see [Built-in Methods](methods.md#list-methods)).

### Slices

`list[start..end]` is a copy of the items from `start` (included) to `end` (excluded). Either bound can be left out. Strings are sliced the same way, by characters:

```koji
numbers = [10, 20, 30, 40]

print(numbers[1..3]) // [20, 30]
print(numbers[..2]) // [10, 20]
print(numbers[2..]) // [30, 40]
print('Hello, Koji'[7..]) // Koji
```

Bounds out of range, or a start after the end, panic.

To go through a list, use a [`for` loop](control-flow.md#for-loop).

Dictionaries (keys of type `string` mapped to values) are declared as [index signatures](custom-types.md#index-signatures).
