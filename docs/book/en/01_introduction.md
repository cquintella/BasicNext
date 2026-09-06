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

Basic Next is an explicitly typed, object-oriented programming language designed for clarity, safety, and predictable execution. It bridges low-level memory control with modern object-oriented paradigms to provide a fully transparent development experience. By eliminating implicit conversions and hidden behaviors, Basic Next ensures that programs behave exactly as written.

### Predictability and Total Developer Control

Explicit code is easier to reason about than implicit conventions. Basic Next removes hidden runtime magic, automatic type coercion, and unexpected fallbacks. Variable types are stated clearly, memory management is transparent, and every function execution path is guaranteed to return a value.

### Shift-Left Safety

By catching potential issues before code ever runs, Basic Next helps you resolve design flaws early during compilation. The strict type system and mandatory return checks prevent common vulnerabilities such as invalid null references, unexpected type mismatches, and untracked resource leaks.

### Dual Execution Model

Basic Next provides both an interactive reference interpreter and ahead-of-time (AOT) compilation. Developers can use the interpreter for rapid experimentation and learning, then compile directly to native binaries or WebAssembly artifacts for efficient production deployment.

### Decoupled Modular Architecture

Under the hood, the Basic Next toolchain uses a modular pipeline: a Lexer, a Parser producing an Abstract Syntax Tree (AST), a Semantic Analyzer, an Intermediate Representation (BN IR), and dedicated backends. This structure keeps language rules clean, predictable, and maintainable.

### Systems and Engine Architecture

Combining an approachable syntax with deterministic low-level control, Basic Next gives programmers a clear view of how software interacts with computer memory and hardware resources. It serves as both a solid platform for systems development and an effective environment for learning software engineering.

>"Programming used to be fun for me. What I want with Basic Next is to go back in time—back to an era when making programs and creating games was genuinely fun. I want to write without having to overthink, turning ideas into programs with as little friction as possible. Basic Next takes the best elements from every language I've known and combines them into something as powerful as it is flexible, with an extremely low learning curve."
>
>— Carlos Alvaro Quintella

## Target Audience

Basic Next is built for learners and engineers who value explicit contracts, low cognitive load, and clean architecture without unnecessary boilerplate. It is well suited for beginners learning fundamental computer science concepts—thanks to readable syntax and helpful diagnostic messages—as well as experienced developers crafting predictable tools and applications.

## Philosophy

The design of Basic Next is guided by clarity, restraint, and deliberate choices:

- **Low cognitive load**: Code should be easy to follow, and the meaning of a statement should be obvious from its local context.
- **Readability first**: Source code is communication between humans, not just instructions for a machine.
- **Explicit contracts**: Types, function boundaries, and side effects should never be surprising.
- **Keep It Simple (KISS)**: Complexity must be justified by a real problem, not speculative requirements.
- **Object-oriented by default**: Related state and behavior belong together in cohesive structures with clear dependencies.
- **Small core, modular reach**: Core syntax stays compact, while rich functionality is provided through external modules and host capabilities.

---

Basic Next was created to restore fluidity, intuition, and enjoyment to programming, blending the clarity of classical languages with the rigor required for modern software:

- **Frictionless Writing**: Turning an idea into working code should be a smooth, continuous process with minimal syntactic obstacles.
- **Natural Structure**: System structure should grow organically with your program rather than requiring complex scaffolding up front.
- **Simplicity by Design**: A small, cohesive language core that fits comfortably in your head.
- **Modular Extensibility**: Capabilities expand cleanly through modules without bloating the core language specification.

---

## Installation and the `bn` CLI

Basic Next source files use the `.bn` extension and are UTF-8 encoded. The language comes with a command-line tool named `bn`.

You can download prebuilt binaries from GitHub or compile the toolchain directly from source using Rust:

```sh
cargo install --path .
```

The Unix manual page is available under `docs/man/bn.1`.

Basic commands:
- `bn check <file.bn>`: Checks syntax and semantic rules. Exits with code `0` on success, `1` on language errors, or `2` on tool usage errors.
- `bn run <file.bn> [-- args...]`: Validates, lowers to BN IR, and executes the program starting from `Start`.
- `bn build <file.bn>`: Compiles the source file into a native executable or WebAssembly artifact using the LLVM backend.
- `bn lex <file.bn>`: Prints the token stream produced by the lexer.

Basic Next diagnostics reject invalid code before execution starts, providing clear feedback on errors.

## Writing a Hello, World!

Every runnable Basic Next program requires an entry point. The simplest valid program consists of a `Start` function that writes text to the screen:

```basic
// A minimal Basic Next Program
FUNCTION Start() AS VOID
    PRINT "Hello, World!"
END FUNCTION
```

`PRINT` is a built-in statement that writes text to standard output, followed by a new line.

## Modules and the `Start` Function

Every source file in Basic Next represents a module. The main module executed by `bn run` must contain a function named `Start` that takes no arguments.

The `Start` function can return `VOID` or an `INTEGER`:

```basic
FUNCTION Start() AS INTEGER
    PRINT "Running successfully."
    RETURN 0
END FUNCTION
```

When `Start` returns an `INTEGER`, the returned value (from 0 to 255) is delivered to the host operating system as the process exit status code. When declared as `VOID`, the runtime automatically exits with code 0 on completion.

All executable statements in Basic Next must reside inside a function, class, or method. Statements are not allowed directly at the top level of a file.

Basic Next does not provide mutable global variables. Shared state should be passed explicitly as function arguments or stored in static class fields:

```basic
CLASS Library
    PUBLIC STATIC shared AS INTEGER = 0
    PUBLIC STATIC note AS STRING = "Ready"
END CLASS

FUNCTION lesser(num1 AS INTEGER, num2 AS INTEGER) AS BOOLEAN
    IF num1 < num2 THEN RETURN TRUE ELSE RETURN FALSE
END FUNCTION

FUNCTION Start() AS VOID
    Library.shared = 10
    PRINT lesser(Library.shared, 20)
    PRINT Library.note
END FUNCTION
```

## Ecosystem Tools

Basic Next provides integrations for standard development workflows:

- **Jupyter Kernel (`bn-kernel`)**: A kernel allowing interactive execution of Basic Next cells inside Jupyter notebooks.
- **VS Code Extension**: Located in `plugins/vscode/`, offering syntax highlighting and automatic diagnostic checks on save.

### Installing the VS Code Extension

To install the official extension for Visual Studio Code:

- Open your terminal and navigate to `plugins/vscode`.
- Package the extension into a `.vsix` file using `vsce`:
  `npx --yes @vscode/vsce package --allow-missing-repository`
- Install the file into VS Code:
  `code --install-extension basicnext-0.3.0.vsix`
- Restart VS Code to initialize language features.
