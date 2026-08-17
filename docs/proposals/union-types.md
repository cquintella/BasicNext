# Proposal: Explicit Union Types and Error Values

## Status

Exploratory. This proposal is not part of Basic Next 0.1.

## Motivation

Recoverable failures and missing data should be explicit values rather than
hidden exceptions. Basic Next should preserve explicit type declarations without
requiring generic syntax such as `<T>` or `RESULT OF T, E` in everyday code.

## Direction

A declaration may name a small set of alternatives with `OR` in a type context.

```basic
LET result AS DATAFRAME OR DATA_ERROR = data.ReadCsv("sales.csv")
LET age AS INTEGER OR NA = customer.Age()
```

`OR` remains the logical operator in expressions. After `AS`, it separates
members of a union type.

The compiler narrows a union after an explicit membership test:

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
- A union member must be named explicitly; there is no implicit `ANY` type.
- `NA` is a candidate value/type for missing data, not an exception.
- APIs return a value or a typed error; `UNWRAP`-style termination, if adopted,
  must be explicit and exceptional.
- Generic classes and methods are outside this proposal.

## Open questions

1. Is `IS` limited to `NA` and union membership, or can it test arbitrary types?
2. Must every union be exhaustively handled?
3. Is `DATAFRAME OR DATA_ERROR` a structural union, or should public APIs use
   named unions such as `DATA_RESULT`?
4. Should user-defined `UNION` declarations be included in the same release?
