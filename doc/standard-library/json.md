# json

The `json` module provides functions to encode and decode data in JSON format.

## The `Json` Type

JSON data is represented by a sum type, so every kind of value is explicit and must be handled with `match`:

```koji
type Json = {
    Null
    Bool(value: boolean)
    Int(value: int)
    Decimal(value: decimal)
    String(value: string)
    Array(items: Json[])
    Object(fields: Map[string, Json])
}
```

-   A number without `.`, `e` or `E` is an `Int` (`2025`); any other number is a `Decimal` (`2025.0`, `1e3`), exactly as written (rounded only beyond 34 significant digits).
-   JSON `null` is the `Null` variant: Koji itself has no `null`.
-   An object is a `Map[string, Json]` (also named `json.JsonObject`), which keeps the order of its keys.
-   `Int(1) == Decimal(1.0)` is `false`: they are different variants.

## Functions

-   `parse(text: string) -> Result[Json, JsonError]`  
    Converts a JSON string into a `Json` value.

-   `stringify(value: Json) -> string`  
    Converts a `Json` value to a JSON string. A `Decimal` is written in full and always with a decimal point (`Decimal(3)` is written `3.0`, `Decimal(1e20)` is written `1e20`), so that parsing the text back gives a `Decimal` again.

-   `get(value: Json, key: string) -> Option[Json]`  
    The field `key` of an `Object`, or `None` if `value` is not an object or has no such field.

-   `asString(value: Json) -> Option[string]`, `asInt(value: Json) -> Option[int]`, `asBool(value: Json) -> Option[boolean]`  
    The payload of a `String`, `Int` or `Bool`, or `None` for any other variant.

-   `asDecimal(value: Json) -> Option[decimal]`  
    The payload of a `Decimal`, or of an `Int` converted to `decimal`. Many serializers write `3.0` as `3`, so this accepts both.

## Errors

```koji
type JsonError = {
    Syntax(message: string, line: int, column: int)
    NumberOutOfRange(text: string, line: int, column: int) // an integer too large for int
}
```

## Example

```koji
import json

immut text: string = '{"name": "koji", "year": 2025}'

match (json.parse(text)) {
    Ok(Object(fields)) => match (fields['name']) {
        String(name) => print(name) // koji
        _ => print('name is not a string')
    }
    Ok(_) => print('expected an object')
    Err(error) => print(`invalid JSON: ${error}`)
}
```

With the helper functions:

```koji
import json

match (json.parse('{"name": "koji", "year": 2025}')) {
    Ok(data) => {
        match (json.get(data, 'year')) {
            Some(year) => print(json.asInt(year)) // Some(2025)
            None => print('no year')
        }

        print(json.stringify(data)) // {"name":"koji","year":2025}
    }
    Err(error) => print(error)
}
```

Building a value to convert:

```koji
import json

print(json.stringify(json.Array([json.Int(1), json.Decimal(2.0), json.Null]))) // [1,2.0,null]
```
