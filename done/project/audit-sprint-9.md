# Sprint 9 audit

Status: Complete

| Requirement | Evidence | Result |
| --- | --- | --- |
| Typed BN IR lowering | `src/llvm.rs`; R10–R11 fixtures and CLI tests | pass |
| Native artifacts | `tests/test_compiler_parity.py` executes accepted artifacts against `bn run` | pass |
| Linked WebAssembly artifacts | `src/main.rs`, `bin/bn-wasm`, `tests/test_wasm_parity.py` | pass |
| Host capability diagnostics | wasm32 rejects filesystem and positioned console at build time | pass |
| Unsupported compiler surface remains explicit | `BUILD_LOWERING_UNAVAILABLE` names the unsupported instruction class | pass |

The accepted compiler surface is the typed scalar subset documented in
[`compiler-0.2.md`](compiler-0.2.md). Objects, vectors, filesystem, positioned
console, and other unlowered IR are not claimed as compiler-supported.

Direct evidence:

- 2026-08-29: `uv run --frozen python -m unittest -v
  tests/test_compiler_parity.py tests/test_wasm_parity.py`: pass (7 tests).
- Native parity covers scalar print, control flow, input, Euclidean integer
  operations, `HOST.Args`, and seeded `HOST.Random`.
- Linked wasm32 artifacts execute in the documented Node.js host and cover the
  accepted non-TTY and input surfaces.

Quality gates: `cargo fmt --check`, `cargo test`,
`cargo clippy -- -D warnings`, compiler parity, WebAssembly parity, and
`git diff --check` pass.

Open requirements: none within the accepted Sprint 9 compiler subset.

Completion decision: complete after R10, R11, and R12 closure; revalidated on
2026-08-29.
