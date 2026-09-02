# Introduction
**Author:** Carlos Quintella  
**Date:** August 29, 2026  
**License:** Mozilla Public License 2.0 (MPL-2.0)

![Basic Next Book Cover](../cover.jpg)

This document is the introductory tutorial for the Basic Next (BN) programming language.

> **Note:** This book is the **Version 0.3** tutorial. It is not the normative
> language contract. When a chapter and the specification disagree, follow
> [`docs/language/0.3/0.3.md`](../../language/0.3/0.3.md),
> [`docs/language/0.3/0.3.ebnf`](../../language/0.3/0.3.ebnf), and
> [`docs/language/0.3/keywords.md`](../../language/0.3/keywords.md).
> Features planned for later versions (packages, `MATCH`, generic classes,
> advanced concurrency) are excluded.

## What is Basic Next?

Basic Next is an explicitly typed, object-oriented language designed for clarity and predictable execution. It favors explicit declarations and strict types over implicit conversions or hidden behaviors. Everything in Basic Next is explicit: variable types are required, memory management is manual, and every execution path in a function must return a value.

The current reference implementation uses a straightforward pipeline comprising a lexer, a parser (producing an Abstract Syntax Tree), a semantic analyzer, and a reference interpreter. 

## Target Audience

Basic Next is built for developers who value explicit contracts, low cognitive load, and clean architecture without the burden of excessive boilerplates or imposing frameworks. It is suitable for both beginners learning fundamental computing concepts—thanks to its straightforward syntax and readable design—and experienced engineers looking for a predictable, transparent language to craft cross-platform tools, systems, and applications.

## Philosophy

The language's design is heavily informed by Zen principles: clarity, restraint, and deliberate choices over novelty. The core mission is to make modern programming more readable, predictable, pleasurable, and accessible. 

Basic Next follows several key design principles:
- **Low cognitive load**: Common code should be easy to understand and intent should be local.
- **Readability first**: Source code is communication, not merely an instruction to a machine.
- **Explicit contracts**: Types, boundaries, and effects should not be surprising. Every declaration states its type.
- **Keep it simple (KISS)**: Complexity must earn its place through a concrete problem, not anticipation.
- **Object-oriented by default**: Behavior belongs to cohesive objects, with explicit dependencies.
- **Small core, broad reach**: Richness belongs in external modules and host capabilities, keeping reserved words and built-in features to a minimum.

## Installation and the `bn` CLI

Basic Next source files use the `.bn` extension and are UTF-8 encoded. The
language is distributed with a command-line tool, `bn`. Install it with
`cargo install --path .` from this repository, or download a prebuilt binary
from GitHub Releases. The Unix manual is `bn(1)` (`docs/man/bn.1`).

- `bn check <file.bn>`: lexer, parser, and semantics. Exit `0` if valid, `1`
  for a language diagnostic, `2` for invalid tool use or a tool failure.
- `bn run <file.bn> [-- args...]`: validate, lower to typed BN IR, and execute
  `Start`. On success the process exit code is `Start`'s result. Language
  errors exit `1`; tool failures exit `2`.
- `bn build <file.bn>`: compile to a native executable or WebAssembly artifact using the LLVM backend.
- `bn lex <file.bn>`: print the token stream and stop.

`HOST.Args[0]` is the absolute executable entry given to `bn run` or `bn build`. Further program
arguments follow `--`. Frontend artifacts are available with `--emit tokens`,
`--emit ast`, `--emit typed-ast`, and `--emit ir`. `-v` prints pipeline
stages; `-vv` also prints tokens.

Basic Next diagnostics reject invalid source before execution. There are no
warnings. Full command reference: [`bn(1)`](../../man/bn.1).

## Hello, World!

A complete Basic Next program requires an entry point. The simplest valid program consists of exactly one `Start` function that prints text to the screen:

```basic
FUNCTION Start() AS VOID
    PRINT "Hello, World!"
END FUNCTION
```

`PRINT` is a built-in macro that writes text to standard output, followed by a line ending.

## Modules and the `Start` Function

Every Basic Next file is a module. The executable module—the one passed to `bn run`—must contain exactly one function named `Start` taking no parameters.

The `Start` function can be declared with a `VOID` return type or an `INTEGER` return type:

```basic
FUNCTION Start() AS INTEGER
    PRINT "Running successfully."
    RETURN 0
END FUNCTION
```

When `Start` returns an `INTEGER`, it must return a value between `0` and `255`. This value is directly passed back to the host operating system as the process exit code. When `Start` is `VOID`, a successful completion automatically yields an exit code of `0`.

Basic Next requires all statements to be contained within functions, classes, interfaces, or structs. You cannot write executable statements at the top level of a module.

## Ecosystem Tools

Basic Next provides tools for modern development workflows:

- **Jupyter Kernel (`bn-kernel`)**: A Python-based Jupyter kernel that evaluates Basic Next cells. Each cell is treated as a complete program with a `Start` function. To use it, install the `bn-kernel` Python package.
- **VS Code Extension**: Located in the `plugins/vscode/` directory, this extension provides syntax highlighting and on-save linting diagnostics powered by the `bn check` compiler.

### Installing the VS Code Extension

To get the best development experience with syntax highlighting and automatic error checking on save, you can install the official Basic Next VS Code extension directly from the repository.

1. Open your terminal and navigate to the VS Code plugin directory:
   ```sh
   cd plugins/vscode
   ```
2. Package the extension into a `.vsix` file using `vsce` (requires Node.js):
   ```sh
   npx --yes @vscode/vsce package --allow-missing-repository
   ```
3. Install the generated package into Visual Studio Code:
   ```sh
   code --install-extension basicnext-0.3.0.vsix
   ```
4. **Restart VS Code** completely after the installation to ensure the language server and debugger features load correctly.
