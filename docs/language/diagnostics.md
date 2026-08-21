# Basic Next 0.1 Diagnostics

This document defines the diagnostic presentation contract for the Basic Next
0.1 reference tool. It complements the language semantics in
[0.1.md](0.1.md).

## Contract

Basic Next 0.1 emits errors, not warnings. Every diagnostic has:

- a stable `UPPER_SNAKE_CASE` identifier;
- a primary source span with file, one-based line, and one-based column;
- the source line and a `^` marker at the primary span;
- a direct explanation of the violated rule; and
- the smallest useful correction when one is known.

The reference presentation is:

```text
main.bn:12:9: error[TYPE_MISMATCH]: cannot assign FLOAT to INTEGER
12 |     LET count AS INTEGER = 2.5
   |                            ^^^ expected INTEGER; write an explicit conversion
```

Messages must explain the language rule in plain terms. They must not merely
repeat a parser state, expose Rust implementation details, or use vague text
such as "invalid input" when the specific problem is known.

## Categories

Diagnostic identifiers are stable and use the semantic cause, for example:

```text
SYNTAX_ERROR
NAME_NOT_FOUND
TYPE_MISMATCH
MISSING_RETURN
NUMERIC_OVERFLOW
INVALID_NUMERIC_CONVERSION
NULL_POINTER_ACCESS
INDEX_OUT_OF_BOUNDS
USE_AFTER_DELETE
DOUBLE_DELETE
```

The core categories are:

| Identifier | When used |
| --- | --- |
| `SYNTAX_ERROR` | Source does not match the accepted grammar. |
| `NAME_NOT_FOUND`, `DUPLICATE_NAME`, `PRIVATE_ACCESS` | Name lookup or visibility fails. |
| `TYPE_MISMATCH`, `INVALID_NUMERIC_CONVERSION`, `INVALID_ALTERNATIVE_USE` | A static or checked value-type rule is violated. |
| `MISSING_RETURN`, `UNREACHABLE_CODE`, `INVALID_LOOP_CONTROL` | Flow analysis fails. |
| `NUMERIC_OVERFLOW`, `DIVISION_BY_ZERO`, `INVALID_SHIFT_COUNT`, `INVALID_EXIT_CODE` | A defined numeric or process-status runtime check fails. |
| `NULL_POINTER_ACCESS`, `INDEX_OUT_OF_BOUNDS`, `USE_AFTER_DELETE`, `DOUBLE_DELETE` | Checked-memory access fails. |
| `ALLOCATION_SIZE_INVALID`, `ALLOCATION_SIZE_OVERFLOW`, `ALLOCATION_TOO_LARGE` | Allocation validation fails. |
| `HOST_CAPABILITY_UNAVAILABLE`, `STATIC_INITIALIZATION_CYCLE`, `PARSE_ERROR` | Host, initialization, or standard-library execution fails. |

The tool may add a secondary labeled span when it materially explains a
conflict, such as a duplicate declaration's earlier location. Diagnostics do
not use warnings in 0.1.
