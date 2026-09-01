# Basic Next 0.3 Planning Fixtures

These fixtures record accepted 0.3 syntax that is not yet part of the 0.2
implementation baseline. Move each case into `tests/grammar/` with its
implementation activity; until then it must not be included in the 0.2
conformance runner.

| Fixture | Required result | Coverage |
|---|---|---|
| `valid/single-line-if.bn` | Accept and print `equal` and `fallback` | Single-line `IF`, optional `ELSE`, equality, call-free branches, and no `END IF` |
| `invalid/single-line-if-end-if.bn` | Reject (syntax) | A single-line `IF` must not be followed by `END IF` |
| `invalid/single-line-if-split-else.bn` | Reject (syntax) | A compact `ELSE` cannot start on a later physical line |
