# Basic Next 0.3 grammar fixtures

These are source-level conformance fixtures for `docs/language/0.3/0.3.ebnf`.
They are intentionally framework-free: Sprint 2 and Sprint 3 must make `bn
check` accept every file under `valid/` and reject every file under `invalid/`
with a source-spanned lexical, syntactic, or semantic diagnostic as noted below.

| Fixture | Required result | Main coverage |
| --- | --- | --- |
| `valid/all-constructs.bn` | Accept | Module order, declarations, fixed-width integer types, IEEE special floating literals, `NEW`, calls, assignments, every block, `END` pairs, expressions and casts. |
| `valid/comments-and-blanks.bn` | Accept | Blank lines, line comments, non-nesting block comments and the newline within one. |
| `valid/final-newline-optional.bn` | Accept with its final line ending removed | Synthetic `NEWLINE` before EOF. |
| `valid/cast-precedence.bn` | Accept | Postfix numeric and `BOOLEAN` casts, member access, indexing, calls, exponentiation and unary minus. |
| `valid/type-tests-and-stop.bn` | Accept | Primitive alternative type test and program termination with `STOP`. |
| `valid/start-exit-code.bn` | Accept | `Start` returning an operating-system exit code. |
| `valid/multidimensional-vectors.bn` | Accept | Literal-dimension multidimensional vector type, nested literal, and chained indexing. |
| `valid/function-values-and-control-flow.bn` | Accept | Function value type, call through a binding, signed `STEP`, `CONTINUE FOR`, and bare `RETURN` from `VOID`. |
| `valid/integer-widening-conversion.bn` | Accept | `INT16 AS INT32` is a permitted integral widening conversion. |
| `valid/integer-narrowing-conversion.bn` | Accept | An explicit `INT32 AS INT16` conversion is valid; its range is checked at runtime. |
| `valid/host-capabilities.bn` | Accept | `HOST.Args` command-line arguments, `HOST.Clock`, and pure UTC timestamp conversions. |
| `valid/filesystem.bn` | Accept and run | `HOST.FileSystem` import, `FS.Open`, `FS.File.ReadLine`, `Close`, and `DELETE`. |
| `valid/filesystem-import-only.bn` | Accept; `bn run --no-filesystem` rejects | Capability is required from the `IMPORT`, even if `Start` does not use `FS`. |
| `valid/bndata-import.bn` | Accept and run | Logical `BNData` resolution, `DataFrame` construction and lifecycle. |
| `valid/bndata-csv.bn` | Accept and run | CSV header parsing, string columns, row/column counts. |
| `valid/bndata-write-csv.bn` | Accept and run | CSV serialization through `Data.WriteCSV`. |
| `valid/bndata-variable-length.bn` | Accept and run | DataFrame vector parameters accept arbitrary fixed lengths. |
| `valid/local-vector-expression-dimension.bn` | Accept and run | Local `LET` bindings may allocate fixed vectors from declaration-time expressions. |
| `valid/bndata-select-negative.bn` | Accept and run | `Select` with a negative index returns `Error`, not `INDEX_OUT_OF_BOUNDS`. |
| `valid/bndata-empty-stats.bn` | Accept and run | Empty-frame `Slice` is `Error`; empty float `Mean` is `NAN`; empty `ZScore` is a 0-row frame. |
| `valid/error-return-narrow.bn` | Accept and run | `IF file IS Error THEN RETURN` narrows `file` to `FS.File`. |
| `valid/temporal-types.bn` | Accept | Built-in temporal value type declarations. |
| `valid/len-and-sizeof.bn` | Accept and run | `LEN` counts numeric values, strings, vectors, and pointer regions; `SIZEOF` reports portable byte sizes. |
| `valid/pointer-named-type.bn` | Accept | Declared named pointer elements in bindings, signatures, and `IS` tests. |
| `valid/pointer-void.bn` | Accept and run | C-style typed-pointer round trip through opaque `POINTER TO VOID`. |
| `invalid/len-on-boolean.bn` | Reject (semantic) | `LEN` does not accept `BOOLEAN`. |
| `invalid/len-on-single-pointer.bn` | Reject (semantic) | `LEN` accepts pointer regions, not a single-value pointer. |
| `invalid/sizeof-function-value.bn` | Reject (semantic) | `SIZEOF` does not accept a function value. |
| `valid/cls-and-beep.bn` | Accept | `HOST.Console.Cls()` and `HOST.Console.Beep()` methods. |
| `valid/console-print-at.bn` | Accept; run on a TTY | In-bounds 1-based positioned console output. |
| `valid/console-print-at-column-oob.bn` | Accept; runtime error on a TTY | Column one past `NumCols()` is out of bounds. |
| `valid/console-print-at-row-oob.bn` | Accept; runtime error on a TTY | Row one past `NumRows()` is out of bounds. |
| `invalid/cls-without-operand.bn` | Reject (syntax) | `CLS` is an ordinary identifier in 0.2, not a statement; a bare name is not a call. |
| `invalid/beep-on-integer.bn` | Reject (semantic) | `BEEP` is an ordinary identifier; there is no `BEEP` statement. |
| `invalid/withdrawn-console-statements.bn` | Reject (semantic) | `CLS(HOST.Console)` / `BEEP(HOST.Console)` are not 0.2 statements. |
| `invalid/filesystem-unknown-mode.bn` | Reject (semantic) | `FS.Open` rejects literal modes outside `READ`, `WRITE`, `APPEND`. |
| `invalid/filesystem-seek.bn` | Reject (semantic) | File seeking is outside the 0.2 surface. |
| `invalid/filesystem-directory-api.bn` | Reject (semantic) | Directory APIs are outside the 0.2 surface. |
| `invalid/import-after-declaration.bn` | Reject (syntax) | Import ordering. |
| `invalid/import-host-bare.bn` | Reject (syntax) | `HOST` import requires a dotted capability name. |
| `invalid/mismatched-end.bn` | Reject (syntax) | Exact `END <KEYWORD>` matching. |
| `invalid/untyped-let.bn` | Reject (syntax) | Mandatory `AS` on a binding. |
| `invalid/invalid-lvalue.bn` | Reject (syntax) | Assignment target cannot be an expression. |
| `invalid/cross-type-equality.bn` | Reject (semantic) | Equality does not coerce `STRING` to `BOOLEAN`. |
| `invalid/invalid-stop-code.bn` | Reject (semantic) | `STOP` requires an `INTEGER` exit code from `0` through `255`. |
| `invalid/out-of-range-exit-code.bn` | Reject (semantic) | `Start` exit codes are limited to `0` through `255`. |
| `invalid/bare-stop.bn` | Reject (syntax) | `STOP` always requires an exit-code expression. |
| `invalid/bare-continue.bn` | Reject (syntax) | `CONTINUE` always names a loop kind. |
| `invalid/continue-outside-loop.bn` | Reject (semantic) | `CONTINUE FOR` requires an enclosing `FOR`. |
| `invalid/zero-for-step.bn` | Reject (semantic) | A literal `STEP 0` is invalid. |
| `invalid/void-return-value.bn` | Reject (semantic) | A `VOID` function cannot return a value. |
| `invalid/nonvoid-bare-return.bn` | Reject (semantic) | A value-producing function cannot use bare `RETURN`. |
| `invalid/uninitialized-pointer.bn` | Reject (semantic) | A non-defaultable `LET` type requires an initializer. |
| `invalid/pointer-unknown-named-type.bn` | Reject (semantic) | A named pointer element must resolve to a declared or imported type. |
| `invalid/pointer-void-index.bn` | Reject (semantic) | An opaque pointer must convert to a typed pointer before indexing. |
| `invalid/new-void.bn` | Reject (syntax) | `POINTER TO VOID` is opaque; `NEW VOID` is not an allocation form. |
| `invalid/void-vector.bn` | Reject (syntax) | `VOID` cannot be a vector element. |
| `invalid/signature-vector-expression-dimension.bn` | Reject (syntax) | Signature vector dimensions remain literal-only. |
| `invalid/ragged-vector-literal.bn` | Reject (semantic) | A nested vector literal must match every declared dimension. |
| `invalid/local-vector-negative-dimension.bn` | Reject (semantic) | Local vector dimensions must be non-negative at declaration time. |
| `invalid/removed-host-memory.bn` | Reject (semantic) | `HOST.Memory` is deferred and is not a 0.1 capability. |
| `invalid/host-capability-lowercase.bn` | Reject (syntax) | Capability names after `HOST.` must start with a capital letter. |
| `invalid/bad-input-call.bn` | Reject (syntax) | `INPUT` accepts no arguments. |
| `invalid/malformed-number.bn` | Reject (lexical) | Unsupported float form. |
| `invalid/caret-exponentiation.bn` | Reject (lexical) | `^` is not an exponentiation operator in 0.1. |
| `invalid/malformed-binary.bn` | Reject (lexical) | A binary literal cannot contain a digit other than `0` or `1`. |
| `invalid/malformed-hexadecimal.bn` | Reject (lexical) | A hexadecimal literal requires hexadecimal digits. |
| `invalid/bad-escape.bn` | Reject (lexical) | Only `\"` and `\\` string escapes exist. |
| `invalid/unterminated-string.bn` | Reject (lexical) | String opening delimiter span. |
| `invalid/unterminated-comment.bn` | Reject (lexical) | Block-comment opening span. |
| `invalid/nested-comment.bn` | Reject (syntax) | Block comments do not nest. |

`valid/final-newline-optional.bn` is stored with a normal final newline for
repository portability. The conformance runner must also feed the same bytes
without its final `LF` and expect acceptance.

The existing files in `examples/` are also normative parse examples. In
particular, `examples/rpn-calculator.bn` exercises method calls, alternative
types, vectors, `NEW`, `DELETE`, and nested `ELSE IF`.
