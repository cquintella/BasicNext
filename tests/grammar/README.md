# Basic Next 0.1 grammar fixtures

These are source-level conformance fixtures for `docs/language/0.1.ebnf`.
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
| `valid/multidimensional-vectors.bn` | Accept | Multidimensional vector type, nested literal, and chained indexing. |
| `valid/function-values-and-control-flow.bn` | Accept | Function value type, call through a binding, signed `STEP`, `CONTINUE FOR`, and bare `RETURN` from `VOID`. |
| `valid/integer-widening-conversion.bn` | Accept | `INT16 AS INT32` is a permitted integral widening conversion. |
| `valid/integer-narrowing-conversion.bn` | Accept | An explicit `INT32 AS INT16` conversion is valid; its range is checked at runtime. |
| `valid/host-capabilities.bn` | Accept | `HOST.main` command-line arguments, `HOST.clock`, and pure UTC timestamp conversions. |
| `valid/temporal-types.bn` | Accept | Built-in temporal value type declarations. |
| `invalid/import-after-declaration.bn` | Reject (syntax) | Import ordering. |
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
| `invalid/void-vector.bn` | Reject (syntax) | `VOID` cannot be a vector element. |
| `invalid/ragged-vector-literal.bn` | Reject (semantic) | A nested vector literal must match every declared dimension. |
| `invalid/removed-host-memory.bn` | Reject (semantic) | `HOST.memory` is deferred and is not a 0.1 capability. |
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
