# path

The `path` module works with paths as text: it never accesses the disk. It is written in Koji: `std/path.kj`. To see what exists on the disk, use [`file.exists` and `file.list`](file.md#directories).

## Functions

-   `join(base: string, part: string) -> string`  
    `base` followed by `part`, with a separator between them: `join('src', 'main.kj')` is `'src/main.kj'`. If `part` is absolute, it replaces `base`: `join('/usr', '/etc')` is `'/etc'`.

-   `parent(path: string) -> Option[string]`  
    The directory that contains `path`: `parent('src/main.kj')` is `Some('src')`, and `parent('main.kj')` is `Some('.')`. The root (`'/'`) has no parent.

-   `name(path: string) -> Option[string]`  
    The last part of `path`: `name('src/main.kj')` is `Some('main.kj')`.

-   `stem(path: string) -> Option[string]`  
    The name without its extension: `stem('src/main.kj')` is `Some('main')`.

-   `extension(path: string) -> Option[string]`  
    The extension, without the dot: `extension('main.kj')` is `Some('kj')`, and `extension('Makefile')` is `None`.

-   `isAbsolute(path: string) -> boolean`  
    Whether `path` starts at the root (`'/tmp'`), rather than at the current directory (`'src'`).

## Example

```koji
import path

immut source = 'src/main.kj'

match (path.stem(source)) {
    Some(stem) => print(path.join('build', `${stem}.c`)) // build/main.c
    None => print('no file name')
}
```
