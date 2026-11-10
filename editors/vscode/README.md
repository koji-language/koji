# Koji for Visual Studio Code

Syntax highlighting for Koji (`.kj` files): keywords, declarations, strings and templates with their
interpolations, numbers (decimal, hexadecimal and hexadecimal floats), comments, built-in functions
and types, and `test` blocks. Also comments with `Ctrl+/`, matching brackets and quotes, and
indentation after `{`, `(` and `[`.

## Installing

Copy (or link) this directory into the extensions of VS Code, then restart it:

```sh
ln -s "$PWD/editors/vscode" ~/.vscode/extensions/koji
```

Or package it as a `.vsix` file and install that:

```sh
cd editors/vscode
npx @vscode/vsce package
code --install-extension koji-0.1.0.vsix
```

The grammar (`syntaxes/koji.tmLanguage.json`) is a TextMate grammar: other editors that read them
(Sublime Text, Zed, the highlighters of GitHub and many sites) can use it as it is.
