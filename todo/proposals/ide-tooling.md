# Native IDE Tooling: Language Server (LSP) and Debug Adapter (DAP)

**Target:** 0.3 (mandatory surface). Remaining holes tracked in 0.4.3 sprint 7.
**Status:** Implemented in the Rust frontend; VS Code client still has open
items. Audit 2026-09-03 against `src/lsp.rs`, `src/dap.rs`, and
`plugins/vscode/`.

Normative 0.3 boundary: `docs/language/0.3/ide-tooling.md`. This file is the
original proposal plus an implementation checklist. `[X]` is in the tree.
`[ ]` is not. Out-of-scope 0.3 items stay unchecked and are not claimed.

## Motivation

IDE integration must not depend on `bn check` on save or a terminal with no
breakpoints. Native Rust LSP and DAP over stdio, driven by the same lexer /
parser / semantics / interpreter as `bn`.

## LSP (Rust `bn lsp`)

- [X] Native stdio language server in the `bn` crate (`src/lsp.rs`)
- [X] Lifecycle: `initialize`, `initialized`, `shutdown`, `exit`
- [X] `textDocument/didOpen`, `didChange`, `didClose` with full-document
      replacement
- [X] Diagnostics from the buffer without a file save (`publishDiagnostics`)
- [X] Go to definition (`textDocument/definition`), including sibling
      `file://` module load
- [X] Find references (`textDocument/references`) in the server
- [X] Context-aware completion (`textDocument/completion`, trigger `.`)
- [X] Capabilities advertised match implemented methods (hover and document
      symbols are advertised; rename/format are not)
- [X] UTF-16 positions at the protocol boundary (`len_utf16` in prefix/range)
- [X] Unknown methods return protocol "method not implemented"
- [X] Server does not execute the program
- [X] `bn --help` / usage list the `lsp` command
- [X] Find references wired in the VS Code client (`registerReferenceProvider`)

## DAP (Rust `bn dap`)

- [X] Native stdio debug adapter (`src/dap.rs`)
- [X] `initialize`, `launch`, `configurationDone`, `disconnect`, `terminate`
- [X] `setBreakpoints` mapped to executable statement lines; non-executable
      lines are not verified (`line is not an executable statement`)
- [X] `continue`, `pause`, `next` (step over), `stepIn`, `stepOut`
- [X] `threads`, `stackTrace`, `scopes`, `variables` (in-scope locals)
- [X] Interpreter debug hook / dedicated execution thread
- [X] Expression `evaluate` resolves paused-frame locals; unsupported optional
      requests still return the protocol error
- [X] Unknown optional requests get the unsupported response
- [X] VS Code `plugins/vscode/debugAdapter.js` spawns `bn dap`
- [X] `bn --help` / usage list the `dap` command

## VS Code client

- [X] Extension starts `bn lsp` and syncs open/change/close
- [X] Definition provider talks to the language server
- [X] Completion provider talks to the language server
- [X] Debugger type `basicnext` launches through `debugAdapter.js` → `bn dap`
- [X] Plugin tests exist (`plugins/vscode/test/`)
- [X] Client does not duplicate parsing: diagnostics come only from the LSP
      `textDocument/publishDiagnostics` stream
- [X] `package.json` `basicnext.executable` describes the language server and
      debugger, not linting

## Outside 0.3 (do not treat as this proposal)

Hover, rename, format, DAP `evaluate` / `setVariable` / completions, compiler
or wasm or Jupyter debugging. Those belong to `ongoing/bucket-0.4.3.md`
sprint 7 if they ship.

## Remaining to close this proposal

All checklist items are now closed. `bn --help` lists `lsp` and `dap`; the
VS Code client delegates diagnostics and references to the native LSP.
