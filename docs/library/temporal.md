# Temporal standard library 0.1

## Status

Accepted API contract. Parsing, UTC conversion, and identifier-only
`TIMEZONE` values are implemented in the 0.1 interpreter. Zone conversion is
post-0.1.

## Construction and interchange

| Function | Result |
| --- | --- |
| `Date.Parse(text AS STRING)` | `DATE` from ISO 8601 `YYYY-MM-DD`; invalid text raises `INVALID_DATE`. |
| `Time.Parse(text AS STRING)` | `TIME` from ISO 8601 `HH:MM:SS.mmm`; invalid text raises `INVALID_TIME`. |
| `TimeZone.Parse(text AS STRING)` | `TIMEZONE` from a canonical IANA TZDB identifier; invalid text raises `INVALID_TIMEZONE`. |
| `Timestamp.Parse(text AS STRING)` | `TIMESTAMP` from RFC 3339; invalid text raises `PARSE_ERROR`. |
| `Timestamp.Format(value AS TIMESTAMP)` | Canonical RFC 3339 `STRING`; an unrepresentable value raises `FORMAT_OUT_OF_RANGE`. |

`Timestamp.Parse` accepts `Z` and numeric ISO 8601 offsets. Fractions more
precise than milliseconds are valid only when all discarded digits are zero.
`Timestamp.Format` always emits UTC with `Z` and exactly three fractional
digits.

## UTC conversion

| Function | Result |
| --- | --- |
| `BNMath.TODATE(timestamp)` | UTC `DATE`. |
| `BNMath.TOTIME(timestamp)` | UTC `TIME`. |
| `BNMath.TOTIMESTAMP(date, time)` | UTC `TIMESTAMP`. |
| `BNMath.TOHOUR(timestamp)` | UTC hour as `INTEGER` from `0` through `23`. |
| `BNMath.TOWEEKDAY(timestamp)` | UTC ISO weekday as `INTEGER`, Monday `1` through Sunday `7`. |

Time-zone conversion is a separate operation because it depends on a versioned
IANA TZDB release. It is not silently performed by any UTC conversion.

In 0.1, `TIMEZONE` stores a canonical IANA identifier. `TimeZone.Parse`
accepts `UTC` and `Area/Location` spellings and rejects offsets such as
`-03:00`. The reference interpreter does not link a TZDB database; identifier
validation is spelling-only. Zone conversion remains post-0.1.
