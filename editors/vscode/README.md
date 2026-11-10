# Koji for Visual Studio Code

-   Syntax highlighting for Koji (`.kj` files): keywords, declarations, strings and templates with
    their interpolations, numbers (decimal, hexadecimal and hexadecimal floats), comments, built-in
    functions and types, and `test` blocks. Also comments with `Ctrl+/`, matching brackets and
    quotes, and indentation after `{`, `(` and `[`.
-   The language server of Koji (`kojic lsp`): every error of a file as it is edited, the type of
    what is under the mouse, and where a name is declared (`F12`, or `Ctrl+click`), in the file or
    in the module it comes from.

The language server needs `kojic` (see [Getting Started](../../doc/getting-started.md)): on the
`PATH`, or named by the setting `koji.kojicPath`.

## Installing

Install its dependency (the client of the protocol), then copy (or link) this directory into the
extensions of VS Code, and restart it:

```sh
cd editors/vscode && npm install && cd -
ln -s "$PWD/editors/vscode" ~/.vscode/extensions/koji
```

Or package it as a `.vsix` file and install that:

```sh
cd editors/vscode
npm install
npx @vscode/vsce package
code --install-extension koji-0.1.0.vsix
```

The grammar (`syntaxes/koji.tmLanguage.json`) is a TextMate grammar: other editors that read them
(Sublime Text, Zed, the highlighters of GitHub and many sites) can use it as it is.
