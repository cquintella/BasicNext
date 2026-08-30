# Sprint 5 audit

Status: Complete

Normative sources reviewed: `docs/language/0.2/0.2.ebnf`,
`docs/language/0.2/0.2.md`, `docs/language/0.2/keywords.md`,
`docs/library/host.md`, `ongoing/WBS-0.2.md`, `ongoing/bucket.md`.

| Requirement | Evidence | Result |
| --- | --- | --- |
| Explicit `HOST.FileSystem` import and complete surface | `tests/grammar/valid/filesystem.bn`; semantic member table; `filesystem_capability_reports_file_existence` | pass |
| Capability unavailable before `Start` | `filesystem_capability_is_checked_before_start`; `HostEnv::without_filesystem` | pass |
| `Open` modes and unknown literal rejection | `filesystem-unknown-mode.bn`; `filesystem_open_returns_error_for_unknown_mode` | pass |
| Text methods and UTF-8 behavior | `filesystem.bn`; `filesystem_file_opens_reads_and_closes`; `filesystem_file_reports_invalid_utf8_on_text_read` | pass |
| Byte methods and count validation | `filesystem_file_reads_lines_and_bytes`; `filesystem_file_writes_bytes_round_trip`; `filesystem_file_rejects_byte_count_outside_buffer` | pass |
| Closed state, family exclusion, idempotent close | `filesystem_new_file_starts_closed`; runtime `file_call` state checks | pass |
| `DELETE` closes resources and detects reuse | `filesystem_file_delete_closes_and_rejects_reuse` | pass |
| Unsupported directory and seek APIs | `filesystem-directory-api.bn`, `filesystem-seek.bn`; `filesystem_rejects_directory_open_and_missing_delete` | pass |

Direct CLI evidence:
- `cargo run --quiet -- check tests/grammar/valid/filesystem.bn`: pass
- `cargo run --quiet -- run tests/grammar/valid/filesystem.bn`: pass (`[package]`)

Quality gates:
- `cargo fmt --check`: pass
- `cargo test`: pass (60 runtime, 37 semantic, 13 module graph)
- `cargo clippy -- -D warnings`: pass
- `git diff --check`: pass

Open requirements: none.
Completion decision: complete
