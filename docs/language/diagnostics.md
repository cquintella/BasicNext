# Basic Next 0.1 Diagnostics

This document defines the diagnostic presentation contract for the Basic Next
0.1 reference tool. It complements the language semantics in
[0.1.md](0.1.md).

## Contract

Basic Next 0.1 emits errors, not warnings. Every diagnostic has:

- a stable identifier: `E0001` and `E0100` for the current lexer and parser,
  and `UPPER_SNAKE_CASE` for named frontend and runtime diagnostics;
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

## Current frontend identifiers

The Rust frontend currently emits the following stable identifiers:

| Identifier | When used |
| --- | --- |
| `E0001` | Lexical analysis cannot produce a valid token. |
| `E0100` | Source does not match the accepted grammar. |
| `MODULE_NOT_FOUND`, `MODULE_LIMIT`, `IMPORT_CYCLE`, `MODULE_NOT_RESOLVED`, `HOST_IMPORT_SCOPE`, `IMPORTED_START` | Module loading, identity, host-import scope, or executable-module rules fail. |
| `NAME_NOT_FOUND`, `DUPLICATE_NAME`, `PRIVATE_ACCESS`, `UNKNOWN_TYPE`, `TYPE_NAME_AS_VALUE` | Name, declared-type, or visibility resolution fails. |
| `TYPE_MISMATCH`, `INVALID_ALTERNATIVE_USE`, `UNRESOLVED_TYPE`, `NOT_CALLABLE` | An expression has no valid resolved type or call target. |
| `INVALID_CONSTRUCTOR`, `INVALID_DESTRUCTOR`, `INVALID_DELETE_TARGET` | Object construction, destruction, or deletion violates the accepted object lifecycle. |
| `INVALID_POINTER_TYPE`, `POINTER_LENGTH_MISMATCH`, `ALLOCATION_SIZE_INVALID` | A pointer element, pointer shape, or allocation-size rule fails during semantic analysis. |
| `MISSING_RETURN`, `UNREACHABLE_CODE`, `INVALID_LOOP_CONTROL` | Flow analysis fails. |
| `INVALID_SHIFT_COUNT`, `INVALID_EXIT_CODE` | A statically checkable numeric or process-status rule fails. |

## Runtime identifiers

The accepted runtime contract additionally reserves these identifiers for
checks that cannot always be completed by the frontend:

| Identifier | When used |
| --- | --- |
| `INVALID_NUMERIC_CONVERSION`, `NUMERIC_OVERFLOW`, `DIVISION_BY_ZERO`, `INVALID_SHIFT_COUNT`, `INVALID_EXIT_CODE` | A defined numeric or process-status runtime check fails. |
| `NULL_POINTER_ACCESS`, `INDEX_OUT_OF_BOUNDS`, `USE_AFTER_DELETE`, `DOUBLE_DELETE` | Checked-memory access fails. |
| `POINTER_LENGTH_MISMATCH`, `ALLOCATION_SIZE_INVALID`, `ALLOCATION_SIZE_OVERFLOW`, `ALLOCATION_TOO_LARGE` | Dynamic pointer-shape or allocation validation fails. |
| `HOST_CAPABILITY_UNAVAILABLE`, `STATIC_INITIALIZATION_CYCLE`, `PARSE_ERROR`, `INVALID_DATE`, `INVALID_TIME`, `INVALID_TIMEZONE`, `FORMAT_OUT_OF_RANGE` | Host, initialization, or standard-library execution fails. |

The tool may add a secondary labeled span when it materially explains a
conflict, such as a duplicate declaration's earlier location. Diagnostics do
not use warnings in 0.1.
