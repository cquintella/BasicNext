# Math standard library 0.1

## Status

Accepted standard-library contract for Basic Next 0.1. The reference
implementation is planned; this document specifies the API rather than claiming
an existing implementation.

## Access

`Math` is a standard-library namespace available in every module without an
`IMPORT`. Its members are called with `Math.member(...)`. The namespace is not
a language keyword and adds no grammar production.

```basic
LET decay AS FLOAT = Math.EXP(-time / tau)
LET distance AS FLOAT = Math.HYPOT(dx, dy)
LET update AS FLOAT = Math.FMA(rate, delta, value)
```

`Math.*` is the canonical mathematics API. The former unqualified prelude
spellings are not aliases in 0.1.

## Numeric model

Each floating function accepts `FLOAT32` or `FLOAT` arguments. A unary function
returns its argument type; mixed `FLOAT32`/`FLOAT` arguments use the ordinary
floating widening rule and return `FLOAT`. Integral arguments require an
explicit conversion to a floating type.

Floating results follow the Basic Next IEEE 754 model: `NAN`, signed zero,
`INF`, and `-INF` are values, not runtime errors. Except for `FMA`, an operation
is evaluated as its mathematical function and rounded once to the result type
using `roundTiesToEven`. `FMA` evaluates the exact product plus exact addend and
rounds only the final result. Implementations must not substitute `a * b + c`
for `Math.FMA(a, b, c)`.

All functions propagate `NAN` unless a function-specific rule below says
otherwise. A finite argument outside a real function's domain yields `NAN`;
overflow yields the appropriately signed infinity. `Math` does not expose IEEE
status flags or traps in 0.1.

## Integer and general numeric functions

| Function | Signature and result |
| --- | --- |
| `Math.ABS(value)` | Any numeric type; returns the same type. A signed integer minimum raises `NUMERIC_OVERFLOW`. |
| `Math.MIN(left, right)` | Two compatible numeric operands; uses normal numeric promotion and returns that promoted type. `NAN` propagates. |
| `Math.MAX(left, right)` | Two compatible numeric operands; uses normal numeric promotion and returns that promoted type. `NAN` propagates. |
| `Math.SIGN(value)` | Any numeric type; returns the same type with `-1`, `0`, or `1`. `NAN` returns `NAN`; either signed zero returns zero with its original sign. |

`Math.ABS(-INF)` returns `INF`. `Math.MIN` and `Math.MAX` compare infinities in
their ordinary numeric order.

## Rounding functions

| Function | Signature and result |
| --- | --- |
| `Math.FLOOR(value)` | `FLOAT32` or `FLOAT`; same type, rounded toward negative infinity. |
| `Math.CEIL(value)` | `FLOAT32` or `FLOAT`; same type, rounded toward positive infinity. |
| `Math.TRUNC(value)` | `FLOAT32` or `FLOAT`; same type, rounded toward zero. |
| `Math.ROUND(value, digits)` | `FLOAT32` or `FLOAT`, plus `INTEGER`; same float type, decimal rounding with ties to even. `digits` may be negative, zero, or positive. |

For these functions, `NAN` propagates, infinities are unchanged, and a zero
result preserves the input zero's sign where IEEE 754 defines one.

## Exponential, logarithmic, and power functions

| Function | Signature and result |
| --- | --- |
| `Math.EXP(value)` | `FLOAT32` or `FLOAT`; same type. |
| `Math.LOG(value)` | Natural logarithm; `FLOAT32` or `FLOAT`; same type. |
| `Math.LOG10(value)` | Base-10 logarithm; `FLOAT32` or `FLOAT`; same type. |
| `Math.LOG2(value)` | Base-2 logarithm; `FLOAT32` or `FLOAT`; same type. |
| `Math.POW(base, exponent)` | Two compatible floating operands; promoted floating type. |

`Math.EXP(-INF)` is positive zero and `Math.EXP(INF)` is `INF`. The logarithms
of positive zero are `-INF`; logarithms of a negative finite value or `-INF`
are `NAN`; logarithms of `INF` are `INF`. `Math.POW` returns `NAN` when the
real-valued power is undefined, including a negative finite base with a
non-integral exponent.

## Trigonometric functions

| Function | Signature and result |
| --- | --- |
| `Math.SIN(value)`, `Math.COS(value)`, `Math.TAN(value)` | `FLOAT32` or `FLOAT`; same type. Angles are radians. |
| `Math.ASIN(value)`, `Math.ACOS(value)`, `Math.ATAN(value)` | `FLOAT32` or `FLOAT`; same type. Angles are radians. |
| `Math.ATAN2(y, x)` | Two compatible floating operands; promoted floating type, in radians. |

`SIN`, `COS`, and `TAN` of either infinity return `NAN`. `ASIN` and `ACOS`
return `NAN` for finite arguments outside `[-1, 1]`; `ATAN` accepts every
floating value. `ATAN2` uses the signs of zero to select the correct quadrant.

## Stable compound functions

| Function | Signature and result |
| --- | --- |
| `Math.SQRT(value)` | `FLOAT32` or `FLOAT`; same type. |
| `Math.HYPOT(x, y)` | Two compatible floating operands; promoted floating type. |
| `Math.FMA(a, b, c)` | Three compatible floating operands; promoted floating type. |

`Math.SQRT` returns `NAN` for negative finite input or `-INF`, `INF` for
`INF`, and preserves `-0.0`. `Math.HYPOT` computes `sqrt(x*x + y*y)` without
spurious intermediate overflow or underflow; its result is non-negative.
`Math.FMA` is the only fused multiply-add operation in 0.1.

## UTC timestamp conversions

`TIMESTAMP` is a signed count of milliseconds from the Unix epoch. These pure
functions do not read a clock:

| Function | Signature and result |
| --- | --- |
| `Math.TOHOUR(timestamp)` | `TIMESTAMP` to `INTEGER` from `0` through `23`, in UTC. |
| `Math.TOWEEKDAY(timestamp)` | `TIMESTAMP` to ISO 8601 `INTEGER`; Monday is `1` and Sunday is `7`, in UTC. |
| `Math.TODATE(timestamp)` | `TIMESTAMP` to UTC `DATE`. |
| `Math.TOTIME(timestamp)` | `TIMESTAMP` to UTC `TIME`. |
| `Math.TOTIMESTAMP(date, time)` | UTC `DATE` and `TIME` to `TIMESTAMP`. |

Negative timestamps use Euclidean day division, so instants before the Unix
epoch produce the corresponding UTC hour and weekday. The full parse, format,
and temporal-type contract is in [Temporal standard library 0.1](temporal.md).

## Reproducibility requirement

The reference interpreter and a future compiler must produce the same result
for every documented special-value case. Before implementation, add numeric
conformance vectors for signed zero, subnormal values, infinities, `NAN`,
domain boundaries, and `FMA` single-rounding cases.
