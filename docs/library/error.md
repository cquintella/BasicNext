# Basic Next 0.1 Error contract

`Error` is a standard-library object, not a keyword. It has public fields
`Code AS INTEGER` and `Message AS STRING`.

`TryParse` operations return their documented value type or `Error` when
failure is an expected result. `Parse` operations require success and raise
the runtime error `PARSE_ERROR` when parsing fails.
