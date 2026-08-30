# Sprint 6 audit

Status: Complete

Normative sources reviewed: `docs/language/0.2/0.2.ebnf`,
`docs/language/0.2/0.2.md`, `docs/language/0.2/keywords.md`,
`docs/library/bndata.md`, `docs/library/host.md`, `ongoing/WBS-0.2.md`,
`ongoing/bucket.md`.

| Requirement | Evidence | Result |
| --- | --- | --- |
| Logical module location and explicit import | `modules/bn/BNData.bn`; `bndata-import.bn`; CLI check/run | pass |
| DataFrame ownership and lifecycle | `bndata_import_constructs_and_releases_empty_frame`; `bndata_frame_adds_columns_and_reports_counts` | pass |
| Variable fixed-length vector parameters | `bndata-variable-length.bn`; `bndata_accepts_variable_fixed_vector_lengths` | pass |
| CSV UTF-8, headers, quoting, separators, ragged rows | `bndata_read_csv_builds_string_columns`; `bndata_write_csv_serializes_headers_and_rows`; `parse_csv` and separator validation | pass |
| Add columns, counts, names, typed getters | `bndata_frame_adds_columns_and_reports_counts`; `bndata-csv.bn` | pass |
| Atomic integer/float conversion | `bndata-csv.bn`; conversion builds a replacement column before commit | pass |
| Documented reductions | `bndata-csv.bn`; Mean/Median/Min/Max and BNMath-backed reductions | pass |
| Copy-out interop and ownership | `bndata-csv.bn`; `CopyIntegerColumn`; pointer deleted by caller | pass |
| Select/Slice and bounds | `bndata-csv.bn`; `dataframe_call` bounds checks | pass |

Direct CLI evidence:
- `cargo run --quiet -- check tests/grammar/valid/bndata-import.bn`: pass
- `cargo run --quiet -- run tests/grammar/valid/bndata-import.bn`: pass (`00`)
- `cargo run --quiet -- check tests/grammar/valid/bndata-variable-length.bn`: pass
- `cargo run --quiet -- run tests/grammar/valid/bndata-variable-length.bn`: pass (`3`)

Quality gates:
- `cargo fmt --check`: pass
- `cargo test`: pass (65 runtime, 37 semantic, 13 module graph)
- `cargo clippy -- -D warnings`: pass
- `git diff --check`: pass

Open requirements: none.
Completion decision: complete
