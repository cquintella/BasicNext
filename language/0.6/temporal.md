# Temporal standard library 0.1

## Status

Accepted API contract. Parsing, UTC conversion, and timezone conversion
are implemented across both backends (`bni` and `bnc`).

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

## Timezone conversion

| Function | Result |
| --- | --- |
| `TimeZone.Offset(tz AS TIMEZONE, ts AS TIMESTAMP)` | `INTEGER` offset in seconds from UTC at timestamp `ts` in timezone `tz`, accounting for daylight saving time. |
| `TimeZone.IsDST(tz AS TIMEZONE, ts AS TIMESTAMP)` | `BOOLEAN` indicating whether daylight saving time is active in `tz` at timestamp `ts`. |
| `Timestamp.AtZone(ts AS TIMESTAMP, tz AS TIMEZONE)` | Canonical RFC 3339 `STRING` in the local time of `tz` with its offset (`+HH:MM`, `-HH:MM`, or `Z` for UTC). |
| `Timestamp.FromLocal(date AS DATE, time AS TIME, tz AS TIMEZONE)` | `TIMESTAMP` (milliseconds since Unix epoch in UTC) corresponding to local civil date and time in `tz`. |

`TimeZone` stores a canonical IANA identifier. `TimeZone.Parse` accepts canonical
IANA identifiers (`UTC`, `Area/Location`, `Etc/GMT*`). `Timestamp.AtZone` formats
local civil components and timezone offset. `Timestamp.FromLocal` converts local
civil date and time in a timezone back into UTC `TIMESTAMP`.

