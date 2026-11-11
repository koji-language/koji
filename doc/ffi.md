# Foreign Functions

A `foreign` block declares functions of a library of the system, written in another language (C, or any language that gives functions with the calling conventions of the system), which the module then calls as functions of Koji.

```koji
foreign 'sqlite3' {
    type Database = handle(sqlite3_close)

    function sqlite3_libversion(): string
    function sqlite3_open(path: string, mut database: Option[Database]): CInt
    function sqlite3_errmsg(database: Database): string
    blocking function sqlite3_exec(database: Database, sql: string, callback: int, argument: int, error: int): CInt
}

print(sqlite3_libversion()) // 3.45.1

mut opened: Option[Database] = None
sqlite3_open('notes.db', mut opened)

match (opened) {
    Some(db) => {
        if (sqlite3_exec(db, 'create table notes (text)', 0, 0, 0) != 0) {
            print(sqlite3_errmsg(db))
        }
    }
    None => print('no memory for a database')
} // the database is closed here: `sqlite3_close` runs when `db` is dropped
```

-   The string is the name of the library: the program is linked with it (`-lsqlite3`). `'c'` is the C library, which every program has.
-   A block is at the top level of a file, and its names are those of the module (to offer them to other modules, export functions of your own that call them).
-   A function has no body: it is the function of the library with the same name, or the one named by `from` (`function version(): string from 'sqlite3_libversion'`).

## Types

A foreign function only takes and returns values that cross safely, converted by Koji at each call:

| Type | In the library | Crosses as |
| --- | --- | --- |
| `int`, `CLong` | 64-bit integer (`int64_t`, `long`) | the value |
| `CInt` | `int` | the value, which must fit (from -2147483648 to 2147483647), or a panic |
| `CSize` | `size_t` | the value, which cannot be negative, or a panic |
| `float` | `double` | the value |
| `boolean`, `byte` | `bool`, `uint8_t` | the value |
| `string` | `const char *` | a copy ending with a zero byte, which the function only reads during the call (a string that holds a zero byte panics); a string returned is copied (`Option[string]`: `None` for NULL) |
| `byte[]` (a parameter) | `uint8_t *` | a copy of the bytes, valid during the call (pass its length as another argument) |
| a handle | a pointer the library gives | its address; a handle returned that is NULL panics, unless the result is an `Option` of the handle (`None`) |

The values are copies: a function of the library never keeps a pointer into the memory of Koji.

A `mut` parameter is one the function writes (a pointer for the library): an integer, a `float`, a `byte[]` (a buffer it fills, as long as the list), or an `Option` of a handle (a function that gives a handle through its arguments, as `sqlite3_open`; it becomes `None` when the library gives NULL).

```koji
foreign 'c' {
    function getentropy(mut buffer: byte[], size: CSize): CInt
}

mut key: byte[] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
getentropy(mut key, 16)
```

## Handles

`type Name = handle(destructor)` declares the type of a pointer that the library gives and takes back: an open database, a stream, a context. It is opaque (only the library looks inside) and move-only, like a `File`: it has one owner, cannot be copied, and when it is dropped the destructor of the library runs (`sqlite3_close(pointer)`), once. A handle cannot be made in Koji: only a foreign function returns one.

A task cannot capture a handle (a copy of it would be a second owner): a task that needs the database opens its own, or one task owns it and the others send it what to do through a [channel](standard-library/task.md#channels).

## Tasks

A foreign function runs on the thread of the program: while it runs, the other [tasks](standard-library/task.md) wait. A function marked `blocking` (one that waits for a disk, the network, a lock) runs on a thread that helps while other tasks are alive, and the task waits for it as for the network. In a block with a `blocking` function, the functions run one at a time, so a library that is not safe for threads stays safe.

## Safety

The checks above keep the memory of Koji safe from what crosses. What Koji cannot check is the declaration itself: it must say what the function of the library takes and returns (a wrong declaration is as wrong as a wrong call in C). Keep the declarations next to the documentation of the library, and the safe functions that use them in the same module.

Foreign blocks are for libraries that Koji does not have: the standard library is written in Koji, and depends on nothing but the system.
