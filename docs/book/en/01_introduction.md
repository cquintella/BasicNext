# Introduction

[← Previous: Preface](00_preface.md) · [Contents](toc.md)

**Author:** Carlos Quintella
**License:** Mozilla Public License 2.0 (MPL-2.0)

![Basic Next Book Cover](../cover.jpg)

> **Note:** This book is the tutorial for the **0.6 line** of Basic Next, written against toolchain 0.6.0 (two executables, `bni` and `bnc`; the language is the 0.5.2 contract plus the 0.6.1 `++`/`--` expressions). It is not the normative language contract. When a chapter and the specification disagree, the specification governs: [`docs/language/0.6/0.6.md`](../../language/0.6/0.6.md), with the grammar in [`0.6.ebnf`](../../language/0.6/0.6.ebnf) and the reserved words in [`keywords.md`](../../language/0.6/keywords.md). Toolchain behaviour that is not part of the language — `bni eval`, the module search path, diagnostic configuration — is documented in [`usage.md`](../../project/usage.md).

This chapter installs the toolchain, runs a first program, and establishes the vocabulary the rest of the book uses: module, entry point, declaration, diagnostic. By the end of it you will have compiled and executed a Basic Next program in two different ways and will know what the tool reports when a program is wrong.

## What Basic Next Is

Basic Next is an explicitly typed, object-oriented language with a reference interpreter and an ahead-of-time compiler, both built on the same validated intermediate representation. A program states the type of every binding, parameter, and return value; the compiler checks those statements before the program runs; and the behaviour of the program at runtime is the behaviour its source describes, without implicit numeric conversion, without silent integer wraparound, and without values that are treated as conditions because they happen to be non-zero.

Three properties follow from that position and are worth stating concretely, because they shape how the language is used.

The first is that errors are reported by position in the source. A type mismatch, a missing return, a loop exit that names the wrong loop — each is a compile-time diagnostic carrying a stable code, a file, a line, and a column. The feedback loop for a beginner is therefore the compiler rather than a debugger, and the feedback loop for a maintainer is the same tool that builds the program.

The second is that the same source is both interpreted and compiled. `bni run` executes a program through the typed-IR interpreter, which is the executable reference for the language. `bnc` compiles the supported subset of that same validated IR to a native executable or a WebAssembly module. A program is not first written for one and then ported to the other, and where the compiler cannot express a valid program for a chosen target, it rejects it with a diagnostic naming the unsupported operation rather than emitting code that behaves differently.

The third is that the language surface is small on purpose. `HOST` is the only built-in interface to the operating system. Everything else — mathematics, JSON, logging, HTTP, tabular data, concurrency — is a module that a program imports by name. Nothing reaches a program's scope without a line of source that asks for it.

## Who the Language Is For

Two audiences, with one requirement in common.

Students meeting types, allocation, control flow, and memory for the first time need a language that makes the machine visible without making it cryptic, and that fails early enough for the failure to be connected to the decision that caused it. Practitioners building small deterministic tools need the same properties for a different reason: a program whose behaviour is determined by its source is a program that can be reasoned about six months later.

The book is written for both. It assumes no previous exposure to Basic Next and no specific prior language, and it explains each construct from its purpose rather than by analogy to another language's version of it.

## Design Principles

The principles below are stated as constraints, because that is what they are in practice. Each is enforced by the compiler or by the shape of the grammar rather than left to convention.

*Low cognitive load.* The meaning of a statement is determined by that statement and the declarations it names, not by a configuration file, a runtime setting, or a convention that must be known in advance.

*Readability before brevity.* Where a shorter form and a clearer form conflict, the language takes the clearer one. Blocks are closed by an explicit `END`, and imported names are reachable only through their local alias.

*Explicit contracts.* A function's signature states its parameter types, its return type, and, through alternative types such as `INTEGER OR NA`, whether it can fail to produce a value.

*Complexity must be justified.* A construct enters the language when a concrete problem requires it. The consequences are listed in the preface under what the language does not have.

*Small core, modular reach.* Capability grows by adding modules, not by adding keywords.

## Installing the Toolchain

Basic Next source files use the `.bn` extension and are UTF-8 encoded. The toolchain is a single command-line program named `bn`.

Prebuilt binaries are published on the [GitHub releases page](https://github.com/cquintella/BasicNext/releases/latest). To build from source, with a Rust toolchain installed, run the following in a clone of the repository:

```sh
cargo install --path .
```

Building from source requires Rust 1.97 or later. Compiling Basic Next programs to native code or to WebAssembly additionally requires Clang and LLVM 22; the WebAssembly target needs a Clang with a `wasm32` backend and `wasm-ld`, which Apple's Clang does not provide. Checking and running programs — everything in the first nine chapters of this book — needs neither.

Confirm the installation:

```sh
$ bn --version
bn 0.5.2
```

The Unix manual pages are [`bni(1)`](../../man/bni.1) and [`bnc(1)`](../../man/bnc.1) (the retiring [`bn(1)`](../../man/bn.1) dispatcher keeps `bn run`/`bn build` working through 0.6), and installation troubleshooting is in [`usage.md`](../../project/usage.md).

## The `bni` and `bnc` Commands

The toolchain exposes one pipeline through two executables: `bni`, the interpreter with the checking and editor commands, and `bnc`, the compiler. All of them run the same lexer, parser, semantic analysis, and IR validation; they differ in what they do afterwards.

| Command | What it does |
| --- | --- |
| `bni check <file.bn>` | Runs the full frontend and reports diagnostics without executing anything. |
| `bni run <file.bn> [-- args]` | Checks, lowers to BN IR, and executes `Start` in the interpreter. |
| `bnc <file.bn> [-o out]` | Compiles the supported IR subset. Without `-o` it writes LLVM IR to standard output; with `-o` it produces a native executable, or a WebAssembly module under `--target wasm32`. |
| `bni eval <source>` | Evaluates one source fragment, or a complete program with `--stdin`, without creating a file. |
| `bni lex <file.bn>` | Prints the token stream, which is useful when a syntax error is not obvious. |
| `bni lsp`, `bni dap` | Serve the Language Server and Debug Adapter protocols over standard input and output, for editor integration. |

Exit codes are stable and suitable for scripting: `0` on success, `1` when the program has language diagnostics, and `2` for invalid command-line use or unavailable build tooling.

`bni check` is the command to run most often. It is the fastest way to ask whether a program is well formed, and it is what an editor runs on save.

## A First Program

Create a file named `hello.bn`:

```basic
// A minimal Basic Next program.
FUNCTION Start() AS VOID
    PRINT "Hello, World!"
END FUNCTION
```

Check it, then run it:

```sh
$ bni check hello.bn
hello.bn: lexical, syntax, and semantic checks passed
$ bni run hello.bn
Hello, World!
```

Four things in three lines of source are worth naming now, because every later program repeats them.

`FUNCTION Start() AS VOID` declares the entry point. The name is fixed, the parameter list is empty, and the return type is stated like every other return type in the language. `PRINT` is a statement, not a function call; it writes its arguments to standard output followed by a newline. `END FUNCTION` closes the declaration, because block structure is written rather than indented. And the `//` comment runs to the end of the line; `/* ... */` spans several.

To compile the same file to a native executable:

```sh
$ bnc hello.bn -o hello
$ ./hello
Hello, World!
```

Without `-o`, `bnc` writes the LLVM intermediate representation to standard output, which is occasionally useful for inspection and is not otherwise part of the workflow.

For a fragment too small to justify a file, `bni eval` accepts source directly:

```sh
$ bni eval 'PRINT 2 + 3'
5
```

## Modules and the Entry Point

Every `.bn` file is a module. A module contains declarations — functions, classes, structs, interfaces, and constants — and nothing else; statements do not appear at the top level of a file. Writing one there is a syntax error:

```
error[E0100]: Syntax error: Expected expected IMPORT or a top-level declaration in source parser.
```

The module named on the `bni run` command line is the executable module, and it must declare a function named `Start` that takes no parameters. A module without one is rejected before execution:

```
error[START_NOT_FOUND]: Runtime or toolchain diagnostic: executable module requires FUNCTION Start
```

`Start` may return `VOID` or `INTEGER`. Returning `VOID` means the process exits with status `0` when the function completes. Returning `INTEGER` delivers the value, which must lie between `0` and `255`, to the operating system as the exit status:

```basic
FUNCTION Start() AS INTEGER
    PRINT "Running successfully."
    RETURN 0
END FUNCTION
```

The exit status is how a Basic Next program reports success or failure to a shell script, a build system, or a supervisor. Chapter three covers the related `STOP` statement, which terminates immediately and is reserved for failures from which there is nothing to return to.

### Splitting a Program Across Modules

A declaration is private to its module unless it is marked `EXPORT`, and another module reaches it by importing under a local alias. User modules resolve beneath a `modules/` directory next to the executable module. Given this layout:

```
project/
    main.bn
    modules/
        MathUtils.bn
```

with `modules/MathUtils.bn` containing

```basic
EXPORT FUNCTION Square(n AS INTEGER) AS INTEGER
    RETURN n * n
END FUNCTION
```

and `main.bn` containing

```basic
IMPORT MathUtils AS Math

FUNCTION Start() AS VOID
    PRINT Math.Square(5)
END FUNCTION
```

running `bni run main.bn` prints `25`. The alias is mandatory and is the only way to reach the imported names: `Square(5)` on its own does not resolve, because Basic Next never injects imported names into the importing module's scope. Two modules may therefore export functions of the same name without colliding. Chapter five covers module resolution, the standard-library path under `modules/bn/`, and the rejection of import cycles in full.

### Shared State

Basic Next has no mutable global variables. State that several functions need is passed as an argument, or held in a `STATIC` field of a class when it genuinely belongs to a type rather than to a call:

```basic
CLASS Library
    PUBLIC STATIC shared AS INTEGER = 0
    PUBLIC STATIC note AS STRING = "Ready"
END CLASS

FUNCTION IsLess(left AS INTEGER, right AS INTEGER) AS BOOLEAN
    RETURN left < right
END FUNCTION

FUNCTION Start() AS VOID
    Library.shared = 10
    PRINT IsLess(Library.shared, 20)
    PRINT Library.note
END FUNCTION
```

This program prints `TRUE` and then `Ready`. The absence of module-level mutable state is a deliberate constraint rather than an omission: it means that a function's effect on the rest of the program is bounded by its parameters, its return value, and the static fields it names explicitly.

## Reading a Diagnostic

Diagnostics are the primary teaching instrument of the language, so it is worth reading one closely before meeting them throughout the book. Consider a program that assigns an `INTEGER` to a `FLOAT` binding:

```basic
FUNCTION Start() AS VOID
    LET a AS INTEGER = 1
    LET b AS FLOAT = a
    PRINT b
END FUNCTION
```

```
error[TYPE_MISMATCH]: Type mismatch: Expected FLOAT(FLOAT64), but found INTEGER(INT32) in binding initializer.
 --> program.bn:3:22
  |
  3 |     LET b AS FLOAT = a
  |                      ^ incompatible value
```

The message has four parts. `error` is the severity. `TYPE_MISMATCH` is a stable code, which means it can be searched for, configured, and relied on across versions. The prose names both types and the context in which they met. The caret marks the column of the offending expression, not merely the line.

The fix is to state the conversion, using the `AS` operator covered in the next chapter:

```basic
LET b AS FLOAT = a AS FLOAT
```

Some diagnostics are warnings rather than errors. An unused binding or an unused import is reported while the program still runs:

```
warning[UNUSED_BINDING]: Unused binding: Binding 'unused' is never read.
```

Warnings can be configured per code with `--allow`, `--warn`, and `--deny`, and `--warnings errors` promotes all of them to errors, which is the setting to use in continuous integration.

## Editor and Notebook Integration

Two integrations are maintained alongside the language, each in its own repository.

The Jupyter kernel, [cquintella/basicnext-jupyter](https://github.com/cquintella/basicnext-jupyter), runs Basic Next cells in a notebook. A cell is a complete program with its own `FUNCTION Start`, not a fragment: there are no top-level statements and no state carried between cells, and the diagnostics are those of `bni run`. The kernel denies filesystem access, so a cell that imports `HOST.FileSystem` reports `HOST_CAPABILITY_UNAVAILABLE`.

The Visual Studio Code extension, [cquintella/basicnext-vscode](https://github.com/cquintella/basicnext-vscode), provides syntax highlighting and runs the checker on save, so the diagnostics described above appear in the editor's Problems panel with the same codes and positions. It also connects to `bni dap` for breakpoints, stepping, and inspection of variables. To install it from source:

```sh
git clone https://github.com/cquintella/basicnext-vscode
cd basicnext-vscode
npx --yes @vscode/vsce package --allow-missing-repository
code --install-extension basicnext-0.5.1.vsix
```

Restart the editor afterwards so the language features initialize.

## What Comes Next

The next chapter covers what a program is made of before it does anything: comments, variables and constants, the primitive types and their guarantees, operators, explicit conversion, and console input and output. From there the book proceeds to control flow, compound data and error handling, and functions and program structure — the four chapters that together cover everything needed to write a complete program.

---

[Next: Common Programming Concepts →](02_common_programming_concepts.md)
