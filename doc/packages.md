# Packages

A program can use the modules of others: **packages**, which are repositories of git, at versions that are tags of git. There is no central registry: any repository that `git` can clone is a package.

## A Module

A directory with a `koji.mod` is a **module**: the program in it (and in its subdirectories) and the packages it requires.

```
module github.com/ana/agenda

require github.com/bia/toml v1.4.0
require github.com/caio/colors v0.3.2
```

-   `module` names the module by the path of its repository (a host, then its path).
-   `require` names a module the program needs, and the least version it works with.

The commands write it:

```
kojic init github.com/ana/agenda     # a koji.mod in the current directory
kojic add github.com/bia/toml        # requires the latest version (or `github.com/bia/toml@v1.4.0`)
kojic update                         # requires the latest version of each module (or `kojic update <module>`)
```

`kojic build`, `run`, `check` and `test` download what is missing.

## Imports

A module of a package is imported by the path of its repository:

```koji
import toml from 'github.com/bia/toml'
import { color } from 'github.com/caio/colors/terminal'
```

-   The path of a module is its file named as the last part of the path: `toml.kj` at the root of the repository of `github.com/bia/toml`.
-   A path inside it is the file of that path: `github.com/caio/colors/terminal` is `terminal.kj`, `github.com/caio/colors/text/style` is `text/style.kj`.
-   Inside a package, its files import each other as files (`import { parse } from 'parser.kj'`).
-   A program imports its own files the same way, or by the path of its module (`github.com/ana/agenda/calendar`).

## Versions

A version is a tag of semver: `v1.4.0`, `v2.0.0-beta.1`. Each module (the program and the packages) asks for the least version of each module it needs, and the program builds with the greatest of them, its **minimal version selection** (as Go does): if the program asks for `toml v1.4.0` and a package for `toml v1.5.0`, both use `v1.5.0`. A version published later changes nothing until `kojic update` asks for it, so a build is the same for everyone, with no lock file.

A new major version changes what the module offers, so it is a module of its own: from `v2`, the path of the module ends with its major version.

```
module github.com/bia/toml/v2
```

```koji
import toml from 'github.com/bia/toml/v2' // still the file toml.kj
```

A program can use `v1` and `v2` at once, and an update never moves it to another major version.

## Integrity

`koji.sum`, next to `koji.mod`, keeps the digest (SHA-256) of the files of each version used (its `.kj` files and its `koji.mod`), added when it is first downloaded. A version whose files are not those of its digest (a tag moved to another commit, a copy changed in the cache) is refused. Keep `koji.sum` in the repository with `koji.mod`: everyone then builds with the same files.

The packages are downloaded once, into `~/.cache/koji/packages` (or `$XDG_CACHE_HOME/koji`, or the directory `KOJI_CACHE` names), and checked at each build.

## Foreign Functions

A package can call code that is not Koji through [foreign functions](ffi.md), which Koji cannot check. A program builds with such a package only if its `koji.mod` allows it:

```
allow foreign github.com/caio/sqlite
```

The program's own foreign blocks need no leave.

## Working on a Package

`replace` takes a module from a directory instead of its repository, for changing a package and the program that uses it together:

```
replace github.com/bia/toml => ../toml
```

Only the `koji.mod` of the program is read for `replace` and `allow foreign`: those of the packages do not change how a program is built.

## Publishing

A package is a repository with a `koji.mod`: a version is a tag.

```
git tag v1.0.0
git push origin v1.0.0
```

`git` downloads the packages, from `https://` and the path of the module (`KOJI_GIT_URL` gives another start, for a mirror): what `git` can reach, a private repository included, `kojic` can.
