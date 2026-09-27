# Basic Next 0.3 IDE Tooling

## Status

Accepted 0.3 delivery scope. This document defines the boundary for native
Language Server Protocol (LSP) and Debug Adapter Protocol (DAP) tooling. The
wire API, dependency set, and capability matrix require the WBS Phase 0
decision gate before implementation.

## Scope

The Rust reference frontend provides native, local LSP 3.18 and DAP services.
Both protocols use their published standard JSON message framing over stdio.
The existing VS Code extension remains a client; it must use these services
when available rather than duplicate parsing or semantic analysis.

Mandatory LSP features are:

- `initialize`, `initialized`, `shutdown`, and `exit` lifecycle handling;
- `textDocument/didOpen`, `textDocument/didChange`, and
  `textDocument/didClose` with full-document replacement in 0.3;
- diagnostics from incomplete source without requiring a file save;
- go to definition and find references for resolved symbols;
- context-aware completion using the parser and semantic model.

Mandatory DAP features are:

- `initialize`, `launch`, `configurationDone`, `disconnect`, and `terminate`;
- launch and breakpoints on executable source spans;
- continue, pause, step over, step into, and step out;
- stack frames and inspection of in-scope locals while execution is paused.

Both services run locally over standard input/output. They must preserve BN
source spans and use the lexer, parser/AST, semantic analysis, and interpreter
pipeline; neither service owns a second frontend or interpreter.

The reference runtime exposes a read-only `runtime::DebugHook` callback through
`execute_with_host_debug` and a terminate/continue control callback through
`execute_with_host_debug_control`. The DAP service runs the validated module in
a dedicated execution thread and maps `continue`, `pause`, and step requests to
that callback. It reports instruction source spans and read-only symbol/value
snapshots without evaluating user expressions; DAP `scopes` and `variables`
expose those snapshots.

## Boundaries

- LSP and DAP are tools, not BN syntax, keywords, `HOST` capabilities, or
  standard modules.
- The language server analyzes source supplied by the client; it must not
  execute it.
- The debug adapter controls the native interpreter only. Compiler, wasm, and
  Jupyter debugging are unavailable until each has executable provider
  evidence.
- Requests and produced data are bounded. A malformed request yields a
  protocol error and must not terminate the service.
- Unknown optional methods or events receive the protocol-defined
  unsupported response; they do not expand the 0.3 surface.
- LSP positions use UTF-16 code units as negotiated by LSP 3.18. Internal BN
  byte spans are converted only at the protocol boundary.
- Definition lookup may load an explicitly imported sibling module from a
  `file://` URI when it is not open in the client. The module name is restricted
  to identifier characters and the file is capped at 8 MiB; network and
  workspace discovery are not performed by the native service.
- Network transports, collaborative editing, remote debugging, language
  plugins, and a formatter are outside 0.3.

## Consistency requirements

The services must report the same source spans and diagnostics as `bn check`
for complete, unchanged source. A breakpoint maps to an executable statement
span; non-executable locations are rejected explicitly. Paused-variable
inspection must not evaluate arbitrary BN expressions or invoke user code.

The implementation may use approved LSP types and framing crates. DAP 0.3
uses the shared JSON/framing layer and implements only the mandatory messages
above; it does not depend on an alpha DAP framework. No dependency is added
before the WBS decision record states its version, features, license, and
security review.

The wire contracts are the published
[Language Server Protocol 3.18 specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.18/specification/)
and [Debug Adapter Protocol specification](https://microsoft.github.io/debug-adapter-protocol/specification).

## Verification

Conformance tests use local stdio clients. They cover malformed framing,
incremental document replacement, diagnostics, navigation, completion,
breakpoint mapping, stepping, scope visibility, termination, and source-span
parity with `bn check`. VS Code integration tests exercise the same services.
