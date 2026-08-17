# Proposal: Explicit Alternative Types and Error Values

## Status

Exploratory. This proposal is not part of Basic Next 0.1.

## Direction

`LET` declares a value with its explicit allowed types. `OR`, used after
`AS`, separates those alternatives; there is no separate declaration form.

```basic
LET result AS DATAFRAME OR DATA_ERROR = data.ReadCsv("sales.csv")
LET age AS INTEGER OR NA = customer.Age()
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
- `NA` is a candidate value/type for missing data, not an exception.
- APIs return a value or a typed error; `UNWRAP`-style termination, if adopted,
  must be explicit and exceptional.
- Generic classes and methods are outside this proposal.

## Open questions

1. Is `IS` limited to `NA` and allowed-type membership, or can it test arbitrary types?
2. Must every alternative be handled by a later `MATCH` form?
