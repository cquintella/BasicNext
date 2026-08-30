# Native IDE Tooling: Language Server (LSP) and Debug Adapter (DAP)

**Target:** Version 0.3 or 0.4
**Status:** Proposed

## Motivation

Currently, IDE integration (such as the VS Code extension) relies on invoking the `bn check` CLI tool every time a file is saved, and launching terminals for execution without proper breakpoints. While this provides basic linting, it lacks the interactive, real-time feedback and execution control that modern developers expect from a programming language ecosystem.

O ideal, numa versão 0.3 ou 0.4, é construir ferramentas nativas em Rust:
1. **LSP:** Que analise a AST em tempo real enquanto o programador digita (sem precisar salvar), além de habilitar recursos como Go to Definition, Find References e Autocompletion inteligente (IntelliSense).
2. **DAP:** Implementar o Debug Adapter Protocol nativamente no interpretador em Rust para suportar breakpoints interativos, passo a passo (Step Over/Into) e inspeção de variáveis diretamente no painel da IDE.

## Features

Implementing native LSP and DAP in the Rust interpreter will enable advanced IDE capabilities, including:

- **Real-time Diagnostics:** Syntax and semantic errors reported instantly during typing.
- **Intelligent Navigation:** Go to Definition and Find References.
- **IntelliSense (Autocompletion):** Context-aware autocompletion for object methods, properties, and constructs.
- **Interactive Debugging (DAP):** Set breakpoints in the editor, pause execution, and step through Basic Next statements.
- **Variable Inspection:** Inspect local variables (`LET`), state, and memory during a paused debug session.

## Implementation Considerations

The LSP should leverage the existing parser and semantic analysis pipeline. Since Basic Next is designed for low cognitive load and explicit types, the analyzer is well-positioned to provide fast and accurate semantic resolution to the language server.
