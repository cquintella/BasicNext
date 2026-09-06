# Protocol and Wasm reliability evidence

## DAP

`node plugins/vscode/test/debug-adapter.js` passed. The fixture exercises
initialize, exception-breakpoint response, stack trace, scopes, variables,
evaluate, continue, and termination framing. Rust unit coverage adds launch,
breakpoint mapping, and pause/resume checks (`cargo test --lib dap::`).

## LSP

`python3 -m unittest tests/test_lsp_protocol.py` passed the wire-level fixture
for initialize, didOpen, full-sync didChange, document symbols, completion,
hover, and shutdown/exit framing.

`cargo test --lib lsp::` passed all 12 tests, covering completion, definition,
references, sibling-module lookup, and protocol span conversion. The VS Code
extension registers completion, hover, definition, references, and full-text
`didChange`; its initialize request advertises only server capabilities that
are implemented in `src/lsp.rs`.

## Wasm

`python3 -m unittest tests/test_wasm_parity.py` passed. Existing CLI fixtures
confirm that Console is supported while Net and unavailable providers fail with
`BUILD_CAPABILITY_UNAVAILABLE`; this matches the current claimed target surface.
