# Evidence: BNMath + BNData example tours

Date: 2026-09-07 06:54 -03 (America/Sao_Paulo)
Machine: Andromeda (`48de723a-4125-41e9-9fd7-ec92327dffc4`)
Repo: `/Users/caq/src/BasicNext`
bn: `/usr/local/bin/bn` (also `/Users/caq/.cargo/bin/bn`) version 0.4.5

## New files

- `/Users/caq/src/BasicNext/examples/bnmath_tour.bn`
- `/Users/caq/src/BasicNext/examples/bndata_tour.bn`

## Commands

```
cd /Users/caq/src/BasicNext
bn run examples/bnmath_tour.bn
bn run examples/bndata_tour.bn
```

Exit codes: **0** and **0**. FAIL lines: **0**. SKIP lines: **0**.

Full stdout (includes module unused-binding warnings from `modules/bn/BNMath.bn` / `BNData.bn`):

- `docs/superpowers/evidence/2026-09-07-bnmath-tour-stdout.txt` (89 PASS)
- `docs/superpowers/evidence/2026-09-07-bndata-tour-stdout.txt` (54 PASS)

## BNMath coverage checklist

Sources: `modules/bn/BNMath.bn` + `docs/library/math.md`. Limits accessed as namespace members (`Math.MAX_*`), same as `examples/type_test.bn` (not `Math.Limits.*`).

| API | Result |
| --- | --- |
| ABS | PASS |
| MIN (scalar) | PASS |
| MAX (scalar) | PASS |
| SIGN | PASS |
| FLOOR | PASS |
| CEIL | PASS |
| TRUNC | PASS |
| ROUND | PASS |
| EXP | PASS |
| LOG | PASS |
| LOG10 | PASS |
| LOG2 | PASS |
| POW | PASS |
| SIN | PASS |
| COS | PASS |
| TAN | PASS |
| ASIN | PASS |
| ACOS | PASS |
| ATAN | PASS |
| ATAN2 | PASS |
| SQRT | PASS |
| HYPOT | PASS |
| FMA | PASS |
| VAL | PASS |
| MEAN | PASS |
| MEDIAN | PASS |
| QUARTILE1 | PASS |
| QUARTILE3 | PASS |
| MODE | PASS |
| STDEV | PASS |
| VARIANCE | PASS |
| RANGE | PASS |
| MIN (vector) | PASS |
| MAX (vector) | PASS |
| TOHOUR | PASS |
| TOWEEKDAY | PASS |
| TODATE | PASS |
| TOTIME | PASS |
| TOTIMESTAMP | PASS |
| MAX_INT8 / MIN_INT8 | PASS |
| MAX_INT16 / MIN_INT16 | PASS |
| MAX_INTEGER / MIN_INTEGER | PASS |
| MAX_INT32 / MIN_INT32 | PASS |
| MAX_INT64 / MIN_INT64 | PASS |
| MAX_TIMESTAMP / MIN_TIMESTAMP | PASS |
| MAX_BYTE / MIN_BYTE | PASS |
| MAX_UINT16 / MIN_UINT16 | PASS |
| MAX_UINT32 / MIN_UINT32 | PASS |
| MAX_UINT64 / MIN_UINT64 | PASS |
| MAX_FLOAT32 / MIN_FLOAT32 / MIN_POSITIVE_FLOAT32 | PASS |
| MAX_FLOAT / MIN_FLOAT / MIN_POSITIVE_FLOAT | PASS |
| MAX_FLOAT64 / MIN_FLOAT64 / MIN_POSITIVE_FLOAT64 | PASS |

## BNData coverage checklist

Sources: `modules/bn/BNData.bn` + `docs/library/bndata.md`. CSV fixture: `tests/fixtures/bndata-sprint6.csv`. WriteCSV temp: `/tmp/basicnext-bndata-tour-write.csv`.

| API | Result |
| --- | --- |
| DataFrame CONSTRUCTOR / NEW | PASS |
| AddStringColumn | PASS |
| AddIntegerColumn | PASS |
| AddFloatColumn | PASS |
| AddBooleanColumn | PASS |
| RowCount | PASS |
| ColumnCount | PASS |
| ColumnName | PASS |
| SetLabel | PASS |
| GetString | PASS |
| GetInteger | PASS |
| GetFloat | PASS |
| GetBoolean | PASS |
| Mean | PASS |
| Median | PASS |
| Quartile1 | PASS |
| Quartile3 | PASS |
| Mode | PASS |
| Stdev | PASS |
| Variance | PASS |
| Range | PASS |
| Min | PASS |
| Max | PASS |
| ZScore | PASS |
| CopyIntegerColumn | PASS |
| CopyFloatColumn | PASS |
| Select | PASS |
| Slice | PASS |
| Transpose | PASS |
| AppendRows | PASS |
| AppendColumns | PASS |
| Join | PASS |
| LeftJoin | PASS |
| RightJoin | PASS |
| FullJoin | PASS |
| ConvertToInteger | PASS |
| ConvertToFloat | PASS |
| ReadCSV | PASS |
| WriteCSV | PASS |

## Explicitly left out

- `Math.Limits.MAX_*` class-qualified access: not used; runtime + `type_test` expose Limits as namespace members `Math.MAX_*` (documented in math.md). Module `EXPORT CLASS Limits` is the declaration site for those members.
- `CopyStringColumn`: docs mark **Not in 0.2** — not a public API to cover.
- No SKIP lines needed; every attempted public API succeeded on this runtime.
