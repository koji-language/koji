# file

The `file` module provides functionality for working with files.

## Functions

-   `open(path: string, mode: Mode) -> Result[File, FileError]`  
    Opens the file at `path` in the specified `mode`:

    ```koji
    type Mode = {
        Read // read
        Write // write (overwrites existing content)
        Append // write at the end
    }
```

    With `import file`, the modes are `file.Read`, `file.Write` and `file.Append`. With `import { Mode } from 'file'`, they can be written `Read`, `Write` and `Append`. Since the mode is a sum type, an invalid mode does not compile.

## Methods of `File`

-   `read() -> Result[string, FileError]`  
    Reads the entire contents of the file. The file must have been opened with `Read`.

-   `write(text: string) -> Result[void, FileError]`  
    Writes `text` to the file. The file must have been opened with `Write` or `Append`.

-   `close() -> void`  
    Closes the file. It consumes the variable: using it afterwards is a compile error.

A `File` is move-only: it cannot be copied, only moved, and it is closed automatically when its owner goes out of scope (see [Values and Memory](../memory.md#files)). So `close()` is only needed to close a file earlier.

Reading a file opened for writing, or writing to a file opened for reading, is a bug: it panics.

## Errors

```koji
type FileError = {
    NotFound(path: string)
    PermissionDenied(path: string)
    IsDirectory(path: string)
    InvalidUtf8(path: string) // the content is not valid text
    Io(path: string, message: string) // any other failure of the operating system
}
```

Printing an error shows a readable message, also available as `error.display()`:

```koji
match (file.open('missing.txt', file.Read)) {
    Ok(f) => print(f.read())
    Err(error) => print(error) // file 'missing.txt' not found
}
```

## Example

```koji
import file

match (file.open('notes.txt', file.Write)) {
    Ok(f) => {
        f.write('Hello world!')

        f.close()
    }
    Err(error) => print(error)
}

match (file.open('notes.txt', file.Read)) {
    Ok(f) => {
        match (f.read()) {
            Ok(content) => print(content)
            Err(error) => print(error)
        }

        f.close()
    }
    Err(NotFound(path)) => print(`${path} does not exist`)
    Err(error) => print(error)
}
```

With the `?` operator (see [Error Handling](../error-handling.md)), inside a function:

```koji
import file

function readNotes(): Result[string, file.FileError] {
    immut f = file.open('notes.txt', file.Read)?
    immut content = f.read()?

    f.close()

    return Ok(content)
}
```
