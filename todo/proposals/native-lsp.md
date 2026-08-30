# Native Language Server Protocol (LSP)

**Target:** Version 0.3 or 0.4
**Status:** Proposed

## Motivation

Currently, IDE integration (such as the VS Code extension) relies on invoking the `bn check` CLI tool every time a file is saved. While this provides useful diagnostic linting, it lacks the interactive, real-time feedback that modern developers expect from a programming language ecosystem.

O ideal, numa versão 0.3 ou 0.4, é construir um LSP nativo em Rust que analise a AST em tempo real enquanto o programador digita (sem precisar salvar), além de habilitar recursos como Go to Definition, Find References e Autocompletion inteligente (IntelliSense).

## Features

Implementing a native LSP in the Rust interpreter will enable advanced IDE capabilities, including:

- **Real-time Diagnostics:** Syntax and semantic errors reported instantly during typing, without requiring a file save.
- **Go to Definition:** Jump directly to the declaration of variables, functions, classes, and interfaces.
- **Find References:** Discover all usages of a symbol across the workspace.
- **IntelliSense (Autocompletion):** Context-aware intelligent autocompletion for object methods, properties, and standard language constructs.
- **Hover Information:** Display type signatures, explicit contracts, and documentation comments when hovering over symbols.

## Implementation Considerations

The LSP should leverage the existing parser and semantic analysis pipeline. Since Basic Next is designed for low cognitive load and explicit types, the analyzer is well-positioned to provide fast and accurate semantic resolution to the language server.
