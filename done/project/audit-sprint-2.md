# Sprint 2 audit

Status: Complete

Normative sources reviewed: `docs/language/0.2/0.2.ebnf`,
`docs/language/0.2/0.2.md`, `docs/language/0.2/keywords.md`,
`docs/library/math.md`, `ongoing/WBS-0.2.md`, `ongoing/bucket.md`.

| Requirement | Evidence | Result |
| --- | --- | --- |
| `BNMath.VAL` classic numeric-prefix conversion | `tests/grammar/valid/bnmath-02.bn`; runtime test `executes_bnmath_02_conversion_constants_and_statistics` | pass |
| Range constants and aliases | same fixture and runtime test | pass |
| Vector MIN/MAX and descriptive statistics | same fixture/runtime test; vector and pointer reduction paths | pass |
| `Float.TryParse` withdrawn | `tests/grammar/invalid/float-try-parse.bn`; semantic fixture sweep | pass |
| RPN example migrated to `BNMath.VAL` | `examples/rpn-calculator.bn`; `cargo test` | pass |

Direct CLI evidence:
- `cargo run --quiet -- check tests/grammar/valid/bnmath-02.bn`: pass
- `cargo run --quiet -- run tests/grammar/valid/bnmath-02.bn`: pass (`3.02.52147483647`)

Quality gates:
- `cargo fmt --check`: pass
- `cargo test`: pass (47 runtime, 36 semantic, 13 module graph)
- `cargo clippy -- -D warnings`: pass
- `git diff --check`: pass

Open requirements: none.
Completion decision: complete
