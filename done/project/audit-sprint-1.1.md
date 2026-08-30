# Sprint 1.1 Audit — Command-line environment

**Status:** Complete

| Requirement | Evidence | Result |
| --- | --- | --- |
| `HOST.Args` needs no import and works only in the executable module | Runtime and module-graph fixtures | pass |
| Only `LEN(HOST.Args)` and `HOST.Args[index]` are valid | Positive runtime and three negative fixtures | pass |
| Entries are immutable strings; invalid indices diagnose | Runtime tests | pass |
| Entry zero is an absolute executable path; later entries preserve order | `HostEnv` runtime test and CLI canonicalization | pass |
| `HOST.Main`, `SYSTEM`, `ArgumentCount`, and `Argument` are withdrawn | Negative semantic fixture and updated valid fixtures | pass |

## Quality gates

- `cargo fmt --check` — pass.
- `cargo test` — pass (including 47 runtime and 36 semantic tests).
- `cargo clippy -- -D warnings` — pass.
- `git diff --check` — pass.

**Completion decision:** Complete. The Sprint 1.1 completion gate is closed.
