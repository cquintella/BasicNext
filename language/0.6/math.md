# BNMath standard library

## Status

Accepted standard-library contract for Basic Next 0.1, with the 0.2 surface
in [Conversion, constants, and statistics (0.2)](#conversion-constants-and-statistics-02).
The 0.1 interpreter implements the 0.1 surface. The 0.2 additions are
specified here and implemented in the 0.2 `BNMath` sprint.

## Access

`BNMath` is an external standard-library module under `modules/bn/BNMath.bn`.
Every use requires an import; the alias is used for member calls.

```basic
IMPORT BNMath AS Math
LET decay AS FLOAT = Math.EXP(-time / tau)
LET distance AS FLOAT = Math.HYPOT(dx, dy)
LET update AS FLOAT = Math.FMA(rate, delta, value)
```

`BNMath.*` is the canonical mathematics API. The former `Math.*` and
unqualified prelude spellings are not aliases in 0.1.

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
for `BNMath.FMA(a, b, c)`.

All functions propagate `NAN` unless a function-specific rule below says
otherwise. A finite argument outside a real function's domain yields `NAN`;
overflow yields the appropriately signed infinity. `BNMath` does not expose IEEE
status flags or traps in 0.1.

## Integer and general numeric functions

| Function | Signature and result |
| --- | --- |
| `BNMath.ABS(value)` | Any numeric type; returns the same type. A signed integer minimum raises `NUMERIC_OVERFLOW`. |
| `BNMath.MIN(left, right)` | Two compatible numeric operands; uses normal numeric promotion and returns that promoted type. `NAN` propagates. |
| `BNMath.MAX(left, right)` | Two compatible numeric operands; uses normal numeric promotion and returns that promoted type. `NAN` propagates. |
| `BNMath.SIGN(value)` | Any numeric type; returns the same type with `-1`, `0`, or `1`. `NAN` returns `NAN`; either signed zero returns zero with its original sign. |

`BNMath.ABS(-INF)` returns `INF`. `BNMath.MIN` and `BNMath.MAX` compare infinities in
their ordinary numeric order.

## Rounding functions

| Function | Signature and result |
| --- | --- |
| `BNMath.FLOOR(value)` | `FLOAT32` or `FLOAT`; same type, rounded toward negative infinity. |
| `BNMath.CEIL(value)` | `FLOAT32` or `FLOAT`; same type, rounded toward positive infinity. |
| `BNMath.TRUNC(value)` | `FLOAT32` or `FLOAT`; same type, rounded toward zero. |
| `BNMath.ROUND(value, digits)` | `FLOAT32` or `FLOAT`, plus `INTEGER`; same float type, decimal rounding with ties to even. `digits` may be negative, zero, or positive. |

For these functions, `NAN` propagates, infinities are unchanged, and a zero
result preserves the input zero's sign where IEEE 754 defines one.

## Exponential, logarithmic, and power functions

| Function | Signature and result |
| --- | --- |
| `BNMath.EXP(value)` | `FLOAT32` or `FLOAT`; same type. |
| `BNMath.LOG(value)` | Natural logarithm; `FLOAT32` or `FLOAT`; same type. |
| `BNMath.LOG10(value)` | Base-10 logarithm; `FLOAT32` or `FLOAT`; same type. |
| `BNMath.LOG2(value)` | Base-2 logarithm; `FLOAT32` or `FLOAT`; same type. |
| `BNMath.POW(base, exponent)` | Two compatible floating operands; promoted floating type. |

`BNMath.EXP(-INF)` is positive zero and `BNMath.EXP(INF)` is `INF`. The logarithms
of positive zero are `-INF`; logarithms of a negative finite value or `-INF`
are `NAN`; logarithms of `INF` are `INF`. `BNMath.POW` returns `NAN` when the
real-valued power is undefined, including a negative finite base with a
non-integral exponent.

## Trigonometric functions

| Function | Signature and result |
| --- | --- |
| `BNMath.SIN(value)`, `BNMath.COS(value)`, `BNMath.TAN(value)` | `FLOAT32` or `FLOAT`; same type. Angles are radians. |
| `BNMath.ASIN(value)`, `BNMath.ACOS(value)`, `BNMath.ATAN(value)` | `FLOAT32` or `FLOAT`; same type. Angles are radians. |
| `BNMath.ATAN2(y, x)` | Two compatible floating operands; promoted floating type, in radians. |

`SIN`, `COS`, and `TAN` of either infinity return `NAN`. `ASIN` and `ACOS`
return `NAN` for finite arguments outside `[-1, 1]`; `ATAN` accepts every
floating value. `ATAN2` uses the signs of zero to select the correct quadrant.

## Stable compound functions

| Function | Signature and result |
| --- | --- |
| `BNMath.SQRT(value)` | `FLOAT32` or `FLOAT`; same type. |
| `BNMath.HYPOT(x, y)` | Two compatible floating operands; promoted floating type. |
| `BNMath.FMA(a, b, c)` | Three compatible floating operands; promoted floating type. |

`BNMath.SQRT` returns `NAN` for negative finite input or `-INF`, `INF` for
`INF`, and preserves `-0.0`. `BNMath.HYPOT` computes `sqrt(x*x + y*y)` without
spurious intermediate overflow or underflow; its result is non-negative.
`BNMath.FMA` is the only fused multiply-add operation in 0.1.

## UTC timestamp conversions

`TIMESTAMP` is a signed count of milliseconds from the Unix epoch. These pure
functions do not read a clock:

| Function | Signature and result |
| --- | --- |
| `BNMath.TOHOUR(timestamp)` | `TIMESTAMP` to `INTEGER` from `0` through `23`, in UTC. |
| `BNMath.TOWEEKDAY(timestamp)` | `TIMESTAMP` to ISO 8601 `INTEGER`; Monday is `1` and Sunday is `7`, in UTC. |
| `BNMath.TODATE(timestamp)` | `TIMESTAMP` to UTC `DATE`. |
| `BNMath.TOTIME(timestamp)` | `TIMESTAMP` to UTC `TIME`. |
| `BNMath.TOTIMESTAMP(date, time)` | UTC `DATE` and `TIME` to `TIMESTAMP`. |

Negative timestamps use Euclidean day division, so instants before the Unix
epoch produce the corresponding UTC hour and weekday. The full parse, format,
and temporal-type contract is in [Temporal standard library 0.1](temporal.md).

## Conversion, constants, and statistics (0.2)

`Float.TryParse` is not part of Basic Next 0.2. Numeric text conversion is
`BNMath.VAL`. Temporal `Date.Parse`, `Time.Parse`, and `Timestamp.Parse`
are unchanged.

`BNMath` still has no Portuguese aliases. Teaching maps `SEN→SIN` and so on;
they are not names in the namespace. Angles remain radians. Randomness is
`HOST.Random`, not `BNMath`.

`BNMath.MIN` and `BNMath.MAX` keep their two-argument scalar forms from 0.1
and gain a one-argument vector form below. That arity split is a documented
`BNMath` exception; it is not language overloading.

### `BNMath.VAL`

```basic
LET x AS FLOAT = BNMath.VAL("  12abc")    // 12.0
LET y AS INTEGER = BNMath.VAL("3.9") AS INTEGER
```

`BNMath.VAL(text AS STRING) AS FLOAT` follows classic BASIC, not a strict
parse.

- Leading spaces are skipped.
- The longest numeric prefix is converted; the rest of the string is
  ignored.
- The decimal separator is `.` only: `VAL("3.14")` is `3.14`, `VAL("3,14")`
  is `3.0` (prefix `"3"`). `VAL("12abc")` is `12.0`.
- `VAL("")` and a string with no digits after the skipped spaces are `0.0`.
- The function does not return `Error` and does not raise `PARSE_ERROR`.
- A binary (`0b`) or hexadecimal (`0x`) prefix is not recognized as a base;
  `VAL("0x10")` reads the leading `0`.
- An optional sign may appear after the skipped spaces: `VAL("-2.5")` is
  `-2.5`.
- Integer results use an explicit `AS INTEGER` (range-checked as in 0.1).
- The numeric prefix is an optional sign, then decimal digits, then an
  optional fractional part `.` plus digits. There is no exponent (`e`/`E`
  ends the prefix). `INF` and `NAN` are not recognized as values.

| Input | `VAL` |
| --- | --- |
| `"  12abc"` | `12.0` |
| `"3,14"` | `3.0` |
| `"1e10"` | `1.0` |
| `"INF"` | `0.0` |
| `"NAN"` | `0.0` |
| `"-"` | `0.0` |
| `"+2"` | `2.0` |
| `"0.0.1"` | `0.0` |
| `""` | `0.0` |

### Range constants

These are namespace members, not calls. Their types match the named BN
type. Integer `MIN`/`MAX` are the inclusive representable endpoints.
Unsigned `MIN` is `0`. Floating `MAX` is the largest finite value; floating
`MIN` is the most-negative finite value; `MIN_POSITIVE_*` is the smallest
positive normal.

| Name | Type | Value |
| --- | --- | --- |
| `BNMath.MAX_INT8` | `INT8` | `127` |
| `BNMath.MIN_INT8` | `INT8` | `-128` |
| `BNMath.MAX_INT16` | `INT16` | `32767` |
| `BNMath.MIN_INT16` | `INT16` | `-32768` |
| `BNMath.MAX_INTEGER` / `BNMath.MAX_INT32` | `INTEGER` | `2147483647` |
| `BNMath.MIN_INTEGER` / `BNMath.MIN_INT32` | `INTEGER` | `-2147483648` |
| `BNMath.MAX_INT64` / `BNMath.MAX_TIMESTAMP` | `INT64` | `9223372036854775807` |
| `BNMath.MIN_INT64` / `BNMath.MIN_TIMESTAMP` | `INT64` | `-9223372036854775808` |
| `BNMath.MAX_BYTE` | `BYTE` | `255` |
| `BNMath.MIN_BYTE` | `BYTE` | `0` |
| `BNMath.MAX_UINT16` | `UINT16` | `65535` |
| `BNMath.MIN_UINT16` | `UINT16` | `0` |
| `BNMath.MAX_UINT32` | `UINT32` | `4294967295` |
| `BNMath.MIN_UINT32` | `UINT32` | `0` |
| `BNMath.MAX_UINT64` | `UINT64` | `18446744073709551615` |
| `BNMath.MIN_UINT64` | `UINT64` | `0` |
| `BNMath.MAX_FLOAT32` | `FLOAT32` | IEEE 754 binary32 largest finite |
| `BNMath.MIN_FLOAT32` | `FLOAT32` | IEEE 754 binary32 most-negative finite |
| `BNMath.MIN_POSITIVE_FLOAT32` | `FLOAT32` | IEEE 754 binary32 smallest positive normal |
| `BNMath.MAX_FLOAT` / `BNMath.MAX_FLOAT64` | `FLOAT` | IEEE 754 binary64 largest finite |
| `BNMath.MIN_FLOAT` / `BNMath.MIN_FLOAT64` | `FLOAT` | IEEE 754 binary64 most-negative finite |
| `BNMath.MIN_POSITIVE_FLOAT` / `BNMath.MIN_POSITIVE_FLOAT64` | `FLOAT` | IEEE 754 binary64 smallest positive normal |

`MAX_INTEGER` and `MAX_INT32` are the same member with two spellings of the
same `INTEGER`/`INT32` alias pair; likewise `FLOAT`/`FLOAT64` and
`INT64`/`TIMESTAMP`. Implementations must not expose two distinct storage
locations.

### Descriptive statistics

The operand is a fixed-size numeric vector of any declared length (`FLOAT[3]`,
`INTEGER[10]`, … — library vector-parameter rule) or a numeric region
`POINTER TO T[]` or `POINTER TO T[n]`, where `T` is an integer or floating
type. Integers used in `MEAN`, `MEDIAN`, `QUARTILE1`, `QUARTILE3`, `STDEV`,
`VARIANCE`, and `MODE` are converted to `FLOAT` elementwise before the
reduction. There is no `Statistics` namespace and no `QUARTILE2` name: the
second quartile is `MEDIAN`.

A null pointer raises the 0.1 pointer diagnostic. The element count `n` is
`LEN(values)`, using the 0.2 rule that `LEN` on a region pointer is that
region's element count.

| Function | Meaning |
| --- | --- |
| `BNMath.MEAN(values)` | Arithmetic mean as `FLOAT`. |
| `BNMath.MEDIAN(values)` | 50th percentile as `FLOAT`. |
| `BNMath.QUARTILE1(values)` | First quartile as `FLOAT`. |
| `BNMath.QUARTILE3(values)` | Third quartile as `FLOAT`. |
| `BNMath.MODE(values)` | Unique most-frequent value as `FLOAT OR NA`. Tie or empty → `NA`. |
| `BNMath.STDEV(values)` | Sample standard deviation (`n−1`) as `FLOAT`. |
| `BNMath.VARIANCE(values)` | Sample variance (`n−1`) as `FLOAT`. |
| `BNMath.RANGE(values)` | `MAX(values) − MIN(values)` as `FLOAT`. |
| `BNMath.MIN(values)` / `BNMath.MAX(values)` | One argument: min/max of the vector, element type. Two arguments: existing scalar `MIN(a, b)`. |

`NAN` in the data propagates. Empty `MEAN` / `MEDIAN` / `QUARTILE*` /
`RANGE` → `NAN`. Empty one-argument `MIN`/`MAX` raises
`INDEX_OUT_OF_BOUNDS`, regardless of the numeric element type. `STDEV` /
`VARIANCE` with `n<2` → `NAN`.

Quartile algorithm (Tukey hinges): sort a copy into non-decreasing order.
`MEDIAN` is the middle element when `n` is odd, or the mean of the two
central elements when `n` is even (that mean is `FLOAT`). `QUARTILE1` is
the median of the lower half; `QUARTILE3` is the median of the upper half.
When `n` is odd the overall median element is excluded from both halves.

`MODE` converts each element to `FLOAT` and counts exact `FLOAT` equality.
As with every descriptive reduction, any `NAN` input propagates to `NAN`;
mode ties without `NAN` return `NA`.

### Reproducibility requirement

The reference interpreter and a future compiler must produce the same result
for every documented special-value case. Before implementation, add numeric
conformance vectors for signed zero, subnormal values, infinities, `NAN`,
domain boundaries, `FMA` single-rounding cases, `VAL` prefixes, empty and
`n<2` statistics, and `MODE` ties.
