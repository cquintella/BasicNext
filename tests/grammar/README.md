# Basic Next 0.1 grammar fixtures

These are source-level conformance fixtures for `docs/language/0.1.ebnf`.
They are intentionally framework-free: Sprint 2 and Sprint 3 must make `bn
check` accept every file under `valid/` and reject every file under `invalid/`
with a source-spanned lexical, syntactic, or semantic diagnostic as noted below.

| Fixture | Required result | Main coverage |
| --- | --- | --- |
| `valid/all-constructs.bn` | Accept | Module order, declarations, types, `NEW`, calls, assignments, every block, `END` pairs, expressions and casts. |
| `valid/comments-and-blanks.bn` | Accept | Blank lines, line comments, non-nesting block comments and the newline within one. |
| `valid/final-newline-optional.bn` | Accept with its final line ending removed | Synthetic `NEWLINE` before EOF. |
| `valid/cast-precedence.bn` | Accept | Postfix numeric and `BOOLEAN` casts, member access, indexing, calls, exponentiation and unary minus. |
| `valid/type-tests-and-stop.bn` | Accept | Primitive alternative type test and program termination with `STOP`. |
| `valid/start-exit-code.bn` | Accept | `Start` returning an operating-system exit code. |
| `valid/multidimensional-vectors.bn` | Accept | Multidimensional vector type, nested literal, and chained indexing. |
| `invalid/import-after-declaration.bn` | Reject (syntax) | Import ordering. |
| `invalid/mismatched-end.bn` | Reject (syntax) | Exact `END <KEYWORD>` matching. |
| `invalid/untyped-let.bn` | Reject (syntax) | Mandatory `AS` on a binding. |
| `invalid/invalid-lvalue.bn` | Reject (syntax) | Assignment target cannot be an expression. |
| `invalid/cross-type-equality.bn` | Reject (semantic) | Equality does not coerce `STRING` to `BOOLEAN`. |
| `invalid/invalid-stop-code.bn` | Reject (semantic) | `STOP` requires an `INTEGER` exit code from `0` through `255`. |
| `invalid/out-of-range-exit-code.bn` | Reject (semantic) | `Start` exit codes are limited to `0` through `255`. |
| `invalid/bare-stop.bn` | Reject (syntax) | `STOP` always requires an exit-code expression. |
| `invalid/void-vector.bn` | Reject (syntax) | `VOID` cannot be a vector element. |
| `invalid/ragged-vector-literal.bn` | Reject (semantic) | A nested vector literal must match every declared dimension. |
| `invalid/bad-input-call.bn` | Reject (syntax) | `INPUT` accepts no arguments. |
| `invalid/malformed-number.bn` | Reject (lexical) | Unsupported float form. |
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
