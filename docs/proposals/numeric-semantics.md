# Proposal: Checked Numeric Semantics

## Status

The rules prohibiting implicit assignment conversion between declared numeric
types, widening signed integers through `INT64`, unsigned integers through
`UINT64`, and `FLOAT32` to `FLOAT64` in numeric
expressions, requiring checked integral overflow, making `/` return `FLOAT`, using
Euclidean integer division and modulo, using `**` for both integral and
floating exponentiation, and contextual numeric literals are accepted for
Basic Next 0.1. Explicit floating-to-integer conversion truncates toward zero.
These decisions are recorded in `docs/language/0.1.md`. The rest of this document remains proposed and
non-normative until accepted. It changes semantics only; the grammar is
unchanged.

## Direction

Basic Next performs no implicit conversion between declared numeric types,
including widening conversion. An
integer literal is an exact, untyped integer while its expression is being
checked; a floating literal is an exact, untyped `FLOAT` value. A literal takes
the required type from its declaration, assignment target, or the other operand
of a numeric operation. If no context is available, integer literals are
`INTEGER` and floating literals are `FLOAT`.

```basic
LET port AS UINT32 = 65536
LET ratio AS FLOAT32 = 0.5
LET total AS INT32 = 2 + 3
LET mask AS UINT32 = 0x80000000
```

In numeric binary expressions, the signed widening sequence is `INT8 → INT16
→ INT32 → INT64`; `BYTE` also widens to `INT16`. The unsigned widening sequence
is `BYTE → UINT16 → UINT32 → UINT64`. The result has the widest integral type
present in its sequence. `FLOAT32` and `FLOAT64` combine as `FLOAT64`. `UINT16`,
`UINT32`, and `UINT64` do not combine implicitly with signed integral values;
`BYTE` is the sole exception because it widens safely to `INT16`. Integers do
not combine implicitly with floating values.
Contextual literals may adopt the required numeric type, but
assignment never widens or narrows declared values implicitly. A numeric binary
operation returns the selected operand type, except `/`, which returns
`FLOAT`. `**` applies integral widening and returns the widest type for integral
operands; for floating operands, it applies floating widening and returns the
promoted floating type.

## Checked operations

Every integral result is range-checked in the destination type. This includes
unary `-`, binary `+`, `-`, `*`, `**`, `SHL`, `+=`, `-=`, `*=`, `**=`, and the loop
binding update performed by `FOR`. Overflow raises the runtime `Error`
`NUMERIC_OVERFLOW`; it never wraps and must not panic the interpreter.

`/` performs IEEE 754 `FLOAT` division. A zero divisor produces the corresponding
IEEE result, including `INF`, `-INF`, or `NAN`. `%` accepts integral operands
only. A zero divisor raises `DIVISION_BY_ZERO`; otherwise its result follows the
existing Euclidean rule, including `INT32_MIN % -1 = 0`, without overflowing.

Integral `**` requires a non-negative exponent: a negative literal is a static
error and a negative computed exponent raises `INVALID_EXPONENT`. Floating
`**` permits a negative exponent. A shift count must be non-negative and less
than the width of the left operand. An invalid literal count is a static error;
an invalid computed count raises `INVALID_SHIFT_COUNT`. `SHR` is logical. `SHL`
is checked rather than masked.

`/=` is valid only when its left assignment target has type `FLOAT`, because
`/` returns `FLOAT`. `%=` is valid only for integral targets. The other compound
assignments require the ordinary operation result to be assignable to the left
target; no compound assignment silently narrows a value.

Floating operations use IEEE 754 binary arithmetic for their declared result
type with `roundTiesToEven`. `NAN`, signed zero, and signed infinities are BN
values and propagate according to IEEE 754; floating overflow and floating
division by zero produce the corresponding IEEE value rather than a BN runtime
error. BN 0.1 does not expose IEEE status flags, traps, NaN payloads, or a
program-selectable rounding direction. `NAN` and infinities cannot be converted
to an integer and raise `INVALID_NUMERIC_CONVERSION`.
`**` applies integral widening and returns the widest operand type for integral
operands. For floating operands, it applies floating widening and returns the
promoted floating type.

`NAN` is unordered: every ordered comparison involving it returns `FALSE`, as
does equality; inequality returns `TRUE`.

## Conversions and sizes

An explicit conversion is permitted between any integral types, including
narrowing conversions and signed/unsigned changes. The runtime checks that the
source value fits the target range and raises `INVALID_NUMERIC_CONVERSION`
otherwise. `INTEGER` follows `INT32`. Floating-to-integer conversion truncates
toward zero, then checks the target range. `NAN`, positive infinity, and
negative infinity raise
`INVALID_NUMERIC_CONVERSION`. Conversion to `FLOAT32` or `FLOAT` uses IEEE 754
rounding. A finite value that overflows the target floating type becomes the
appropriately signed infinity; `NAN` and infinities convert to their
corresponding target values.

Before allocating `TYPE[d1][d2]...`, the runtime validates each dimension and
multiplies dimensions with checked non-negative arithmetic. It then checks the
element count and byte size against the host allocation limit. Arithmetic
overflow raises `ALLOCATION_SIZE_OVERFLOW`; a valid size exceeding that limit
raises `ALLOCATION_TOO_LARGE`. No wrapped size may reach the allocator. The
same checks apply to `NEW TYPE[count]`.

## Alternatives considered

- Silent two's-complement wrapping was rejected: it hides ordinary mistakes
  and makes behavior differ from BN's safety-oriented runtime direction.
- Automatic numeric widening was rejected: it weakens explicit typing and
  makes assignment and compound assignment rules harder to predict.
- Saturating arithmetic was rejected: it silently changes calculations and
  loses the source of the error.

## Conformance work after acceptance

Add executable positive and negative fixtures for each integral boundary,
division/modulo by zero, exponentiation, shift counts, conversions from `NAN`
and infinity, IEEE special floating values, and multidimensional allocation-size overflow. The Rust runtime
must use checked arithmetic and translate every failure to the listed BN
`Error` codes.
