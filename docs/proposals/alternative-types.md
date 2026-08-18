# Proposal: Explicit Alternative Types and Error Values

## Status

Accepted for Basic Next 0.1. `OR` type alternatives, `IS`, `EOF`, and the
standard-library `Error` object define the initial error-value protocol.

## Direction

`LET` declares a value with its explicit allowed types. `OR`, used after
`AS`, separates those alternatives; there is no separate declaration form.

```basic
LET result AS DATAFRAME OR DATA_ERROR = data.ReadCsv("sales.csv")
LET line AS STRING OR EOF = INPUT()
LET value AS FLOAT OR Error = Float.TryParse("12.5")
```

`OR` remains the logical operator in expressions. An explicit membership test
narrows the value for each branch:

```basic
LET result AS DATAFRAME OR DATA_ERROR = data.ReadCsv("sales.csv")

IF result IS DATA_ERROR THEN
    PRINT result.Message()
ELSE
    PRINT result.RowCount()
END IF
```

Within the `THEN` branch, `result` is `DATA_ERROR`; within the `ELSE` branch,
it is `DATAFRAME`.

## Principles

- `LET` declarations retain explicit types through `AS`.
- Every allowed type is named explicitly; there is no implicit `ANY` type.
- `EOF` is the accepted end-of-input value; `NA` is the accepted missing-data
  value, not an exception.
- APIs return a value or the standard-library `Error` object; `UNWRAP`-style
  termination, if adopted, must be explicit and exceptional.
- Generic classes and methods are outside this proposal.

## Open questions

1. Can `IS` test arbitrary types beyond declared alternative membership?
2. Must every alternative be handled by a later `MATCH` form?
