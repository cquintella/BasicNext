# Proposal: Scientific Standard-Library Surface

## Status

Proposed. This records the intended library model for research work. It does
not alter Basic Next 0.1 grammar or claim an implementation.

## Rationale

`Math` contains pure, deterministic numerical functions, including conversion
from a `TIMESTAMP` to calendar components. Hashing and pseudo-random generation
have different contracts and must not be hidden in `Math`: hashes operate on
byte/text representations, and a pseudo-random generator has mutable state.

The proposed standard namespaces are therefore `Math`, `Hash`, `Random`, and
`Statistics`.

## Timestamp conversion and clocks

Timestamp conversion is a pure `Math` operation; it does not read a clock and
does not introduce a nondeterministic program value. Clock access is an explicit
host capability, imported as `HOST.clock`.

```basic
IMPORT HOST.clock AS Clock

LET data_hora AS TIMESTAMP = Clock.GetTime()
LET startedAt AS UINT64 = Clock.MonotonicNanoseconds()
LET elapsed AS UINT64 = Clock.MonotonicNanoseconds() - startedAt
LET hour AS INTEGER = Math.ToHour(data_hora)
LET date AS Date = Math.ToDate(data_hora)
LET weekday AS INTEGER = Math.ToWeekday(data_hora)
```

`Clock.GetTime()` returns a `TIMESTAMP`: a signed `INT64` count of nanoseconds
from the UTC Unix epoch. `Clock.MonotonicNanoseconds()` is for measuring
duration. The future `Clock` contract must define clock resolution and its
availability.

The proposed `Math` conversions are:

| Function | Result |
| --- | --- |
| `Math.ToHour(timestamp)` | `INTEGER` from `0` through `23`, in UTC. |
| `Math.ToDate(timestamp)` | A `Date` value in UTC. `Date` is a future standard-library value with `Year`, `Month`, and `Day` fields; it is not locale-formatted text. |
| `Math.ToWeekday(timestamp)` | `INTEGER` using ISO 8601 numbering: Monday is `1`, Sunday is `7`, in UTC. |

Future unit conversions such as nanoseconds-to-seconds belong to `Math` too;
their integer rounding and overflow behavior must be defined when introduced.

## Hash

`Hash` is a pure standard-library namespace, distinct from cryptographic key
management. The minimum portable function should be:

```basic
LET digest AS STRING = Hash.SHA256(text)
```

`Hash.SHA256(value AS STRING) AS STRING` returns the lowercase hexadecimal
SHA-256 digest of the UTF-8 bytes of `value`. A future binary-data collection
type may add byte-oriented overloads. Non-cryptographic hashes are deferred:
their output width and collision guarantees are application-specific.

## Pseudo-random generation

`Random` is a stateful object created from an explicit `UINT64` seed:

```basic
LET generator AS Random = Random.Create(20260821)
LET sample AS FLOAT = generator.NextFloat()
LET word AS UINT64 = generator.NextUInt64()
```

The default generator must name and version its algorithm in the contract. The
recommended initial choice is `xoshiro256**`, seeded from `SplitMix64`; it is
fast and reproducible but is not cryptographically secure. The exact algorithm,
seed expansion, and every range-mapping rule are part of the public contract and
must not change within a language version.

The minimum methods are:

| Method | Result |
| --- | --- |
| `NextUInt64()` | Uniform unsigned 64-bit word. |
| `NextFloat()` | Uniform `FLOAT` in `[0, 1)`. |
| `NextFloat32()` | Uniform `FLOAT32` in `[0, 1)`. |
| `NextInteger(min, max)` | Uniform integer in the inclusive interval, rejecting an invalid interval. |

There is intentionally no implicit time seeding. A non-reproducible program may
explicitly obtain a `Clock` value and pass it as the seed.

## Statistics

Statistics needs a future variable-size numeric collection type. Fixed-size
vectors alone cannot give a library function one reusable parameter type for
arbitrarily shaped datasets. This proposal therefore defines the API model, not
an executable 0.1 signature.

```basic
LET average AS FLOAT = Statistics.Mean(samples)
LET median AS FLOAT = Statistics.Median(samples)
LET q2 AS FLOAT = Statistics.Q2(samples)
LET q3 AS FLOAT = Statistics.Q3(samples)
LET mode AS FLOAT OR NA = Statistics.Mode(samples)
```

The initial descriptive-statistics surface is:

| Function | Meaning |
| --- | --- |
| `Mean(values)` | Arithmetic mean. |
| `Median(values)` | Same result as `Q2(values)`. |
| `Q1(values)`, `Q2(values)`, `Q3(values)` | First, second, and third quartiles. |
| `Quantile(values, probability)` | General quantile. |
| `Mode(values)` | Unique most-frequent exact value, or `NA` if no unique mode exists. |
| `Modes(values)` | All tied modes; requires the future collection result type. |

`Q2` is the canonical name for the second quartile; `Median` is its readable
synonym. `2QRT` and `3QRT` are not proposed spellings.

Quantile algorithms are not universal. The default must be named rather than
implied: this proposal recommends Hyndman-Fan Type 7, the common default in R.
`Statistics.Quantile` must make the selected method explicit before the API is
accepted, so published research can reproduce results across tools.

For floating data, `NAN` is not an observation and is excluded only by an
explicit filtering operation; summary functions must otherwise propagate it.
`NA` must be handled explicitly before a collection reaches `Statistics`.

## Acceptance prerequisites

1. Define a variable-size collection and its ownership/copy model.
2. Define numeric stability requirements for `Mean` and the ordering algorithm
   for quantiles and modes.
3. Define empty-input results and errors for every statistic.
4. Add reproducibility fixtures for random streams, SHA-256 test vectors,
   clocks, and quartile datasets.
