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

**What is Basic Next?**

Basic Next is an explicitly typed, object-oriented programming language designed for clarity, safety, and deterministic performance. It brings together low-level memory control and modern object-oriented paradigms to provide a fully transparent development experience. By eliminating implicit type coercions and hidden runtime behaviors, Basic Next ensures that software behaves exactly as written—delivering zero-overhead execution and absolute predictability from source code to final compilation.

**1. Core Philosophy: Predictability & Total Developer Control**
Built on the principle that explicit code is superior to implicit behavior, Basic Next completely eliminates hidden runtime magic, coercion rules, and default fallbacks. Variable types are strictly required, memory management is fully manual, and every function execution path is guaranteed to return a value—ensuring total developer control over resource allocation, system footprint, and execution flow.

**2. Shift-Left Safety & Strict Semantic Guarantees**
By enforcing rigorous static checks before code execution begins, Basic Next catches critical design and logic flaws at compile time rather than in production. The language’s strict type system and mandatory return-path checks prevent common runtime vulnerabilities, such as unhandled null references, implicit type conversions, or untracked memory leaks. This zero-surprise approach makes it ideal for high-reliability software, embedded systems, and environments where execution determinism is paramount.

**3. Dual Execution Model: Interpretation & AOT Compilation**
Unlike traditional scripting tools or heavily garbage-collected environments, Basic Next provides a versatile, dual execution pipeline. Developers benefit from an interactive reference interpreter for rapid testing and prototyping, alongside ahead-of-time (AOT) compilation for production deployment. By compiling directly ahead of time, Basic Next bypasses the runtime startup overhead and memory footprint typical of virtual machines, delivering near-instant execution and maximum hardware efficiency.

**4. Decoupled, Modular Pipeline Architecture**
At its structural core, Basic Next relies on a clean, modular pipeline consisting of a Lexer, a Parser producing an Abstract Syntax Tree (AST), a Semantic Analyzer, and a flexible backend interface. This decoupled architecture isolates frontend language analysis from execution engines. As a result, the language remains exceptionally maintainable, highly extensible, and future-proof—allowing seamless integration of new target backends (such as bytecode compilers or LLVM codegen) without altering the frontend parser or semantic rules.

**5. Purpose-Built for Systems and Engine Architecture**
Combining object-oriented structure with low-level determinism, Basic Next bridges the gap between modern language ergonomics and bare-metal programming. It is designed specifically for engineers who require explicit memory management and guaranteed execution semantics without sacrificing structural abstraction. Whether used for systems engineering, performance-critical tools, or as a reference platform for language design, Basic Next empowers developers to build transparent, reliable, and high-performance software.

>"Programming used to be fun for me. What I want with Basic Next is to go back in time—back to an era when making programs and creating games was genuinely fun. I want to write without having to overthink, turning ideas into programs with as little friction as possible. Basic Next takes the best elements from every language I've known and combines them into something as powerful as it is flexible, with an extremely low learning curve."
>
>— Carlos Alvaro Quintella

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

---

O Basic Next nasce para resgatar a fluidez, a diversão e a intuição no ato de programar, unindo a nostalgia das linguagens clássicas às exigências do desenvolvimento moderno. Sua arquitetura é regida por quatro pilares inegociáveis:

    Facilidade de Escrita: Escrever código deve ser um processo contínuo, reduzindo ao máximo o atrito cognitivo entre a ideia e a implementação.

    Estrutura Natural: A organização do código não deve ser forçada por cerimônias excessivas; a estrutura surge organicamente à medida que o sistema cresce.

    Filosofia Keep It Simple (KISS): A linguagem mantém um núcleo pequeno, coeso e previsível, inspirando-se na elegância do C tradicional.

    Expansibilidade por Módulos: O núcleo compacto é estendido através de um sistema de módulos, permitindo adicionar capacidades sem inflar a especificação base.

---

## Installation and the `bn` CLI

Basic Next source files use the `.bn` extension and are UTF-8 encoded. The language is distributed with a command-line tool, `bn`. 

You can download the latest binary from Github ou download the source code in Rust, explore and compile it your self.

Basic Next is mostly written in Rust, strongly using AI.

Install it with
`cargo install --path .` from this repository.

The Unix manual is available in:
 `bn(1)` (`docs/man/bn.1`).

Basic usage for `bn`
- `bn check <file.bn>`: lexer, parser, and semantics. Exit `0` if valid, `1`
  for a language diagnostic, `2` for invalid tool use or a tool failure.
- `bn run <file.bn> [-- args...]`: validate, lower to typed BN IR, and execute
  `Start`. On success the process exit code is `Start`'s result. Language
  errors exit `1`; tool failures exit `2`.
- `bn build <file.bn>`: compile to a native executable or WebAssembly artifact using the LLVM backend.
- `bn lex <file.bn>`: print the token stream and stop.

Basic Next diagnostics reject invalid source before execution. There are no warnings yet. 

Full command reference: [`bn(1)`](../../man/bn.1).

## Writng a Hello, World!

A complete Basic Next program requires an entry point. The simplest valid program consists of exactly one `Start` function that prints text to the screen:

```basic
// A minimal Basic Next Program
FUNCTION Start() AS VOID
    PRINT "Hello, World!"
END FUNCTION
```

`PRINT` is a built-in command that writes text to standard output, followed by a line ending.

## Modules and the `Start` Function

Every Basic Next file is a module. The executable module—the one passed to `bn run`—must contain exactly one function named `Start` taking no parameters.

The `Start` function can be declared with a `VOID` return type or an `INTEGER` return type:

```bn
FUNCTION Start() AS INTEGER
    PRINT "Running successfully."
    RETURN 0
END FUNCTION
```

When `Start` returns an `INTEGER`, it must return a value between `0` and `255`. This value is directly passed back to the host operating system as the process exit code. When `Start` is `VOID`, a successful completion automatically yields an exit code of `0`.

Basic Next requires all statements to be contained within functions, classes, interfaces, or structs. You cannot write executable statements at the top level of a module.

You don't have global variables, if you need one, you have to send it as parameter of a funtion. As alternative you can build a sttic object and store any information you need and pass it in your function.

```bn
CLASS Library
   PUBLIC STATIC shared AS INTEGER=0
   PUBLIC STATIC anotherShared AS STRING="That's all folks!"
END CLASS

FUNCTION lesser(num1 AS INTEGER, num2 AS INTEGER) AS BOOLEAN
   IF num1 < num2 THEN RETURN TRUE ELSE RETURN FALSE
END FUNCTION

FUNCTION Start() AS VOID
   Library.shared=10
   PRINT lesser(Libray.shared, 20)
   PRINT Library.anotherShared
END FUNCTION
```


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
