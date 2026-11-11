# hash

The `hash` module computes the digests of texts with SHA-2, written in Koji (`std/hash.kj`, on the SHA-2 of the runtime that TLS uses).

## Functions

-   `sha256(text: string) -> string`  
    The SHA-256 of the UTF-8 bytes of `text`: 64 hexadecimal digits in lower case.

-   `sha512(text: string) -> string`  
    The SHA-512 of the UTF-8 bytes of `text`: 128 hexadecimal digits in lower case.

## Example

```koji
import hash

print(hash.sha256('abc')) // ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
```

A digest tells whether two texts are the same without comparing them, and whether a text changed (`koji.sum` records the digests of packages). To store passwords, use a function made for them, which SHA-2 is not.
