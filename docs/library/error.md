# Basic Next Error contract

`Error` is a standard-library object, not a keyword. It has public fields
`Code AS INTEGER` and `Message AS STRING`.

In 0.1, `TryParse` operations return their documented value type or `Error`
when failure is an expected result. `Parse` operations require success and
raise the runtime error `PARSE_ERROR` when parsing fails.

In 0.2, `Float.TryParse` is withdrawn. Numeric text conversion is
`BNMath.VAL`, which always returns `FLOAT` and does not use `Error`.
Temporal `Parse` operations still raise `PARSE_ERROR`. File and CSV
operations return `T OR Error`; they do not raise exceptions. Unterminated
CSV quotes, `WriteCSV` I/O failure, and `WriteBytes` I/O failure are
`Error` values, not runtime diagnostics. `ReadBytes` I/O, a closed file,
and a text-family file are `Error` on `INTEGER OR EOF OR Error`.

A user class must not `EXTENDS Error`.
