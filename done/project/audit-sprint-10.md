# Sprint 10 audit

Status: Complete

Normative sources reviewed: `ongoing/bucket.md`, `ongoing/WBS-0.2.md`, and
`docs/library/basicnext.tmLanguage.json`.

| Requirement | Evidence | Result |
| --- | --- | --- |
| VS Code extension project exists in `plugins/vscode/` | `plugins/vscode/package.json`, `extension.js`, and README | pass |
| `.bn` files use TextMate highlighting | `package.json` contributes `source.bn`; bundled grammar is JSON-equivalent to `docs/library/basicnext.tmLanguage.json`; U1 grammar test covers keyword/special-literal union and lexical boundaries | pass |
| Save runs `bn check` and publishes diagnostics | `extension.js` parses every source-spanned diagnostic and ignores stale save results | pass |
| Run/build use saved source and missing `bn` is visible | R17 Node tests | pass |
| DAP lifecycle matches launch-only scope | No early `terminated`; README states no breakpoint/stepping support | pass |

Direct checks:

- `node plugins/vscode/test/test.js`: pass (diagnostics, save policy, and missing tool).
- `node plugins/vscode/test/grammar.js`: pass (TextMate union vs reserved words + `NAN`/`INF`, no `-INF` terminal, no exponent / extra escapes).
- `node plugins/vscode/test/debug-adapter.js`: pass (launch-only lifecycle).
- `node --check plugins/vscode/extension.js`: pass.
- Opened VS Code with `--extensionDevelopmentPath plugins/vscode`: local extension host starts.

Quality gates:

- `cargo fmt --check`: pass.
- `cargo test --locked`: pass (246 Rust tests on 2026-08-29).
- `cargo clippy -- -D warnings`: pass.
- `git diff --check`: pass.

Open requirements: none for Sprint 10.

Completion decision: complete.
