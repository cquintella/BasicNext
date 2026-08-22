# Temporal value types for Basic Next 0.1

## Status

Accepted for Basic Next 0.1. Lexical, syntax, and exact semantic recognition are
Sprint 7 frontend work; parsing, formatting, calendar conversion, and IANA
time-zone evaluation are Sprint 12 runtime work.

## Motivation

`TIMESTAMP` is an instant. Research, records, schedules, and interchange also
need a calendar date, a time of day, and a named time-zone rule set. Using
strings for these values loses validation, ordering, and unambiguous exchange.

## Accepted model

| Type | Value semantics | Logical representation |
| --- | --- | --- |
| `TIMESTAMP` | UTC instant | `INT64` milliseconds since the Unix epoch |
| `DATE` | Gregorian calendar date | `INT32` days since the Unix epoch |
| `TIME` | Time of day without zone | `UINT32` milliseconds since midnight |
| `TIMEZONE` | Named time-zone rules | Canonical IANA TZDB identifier text |

`DATE`, `TIME`, and `TIMEZONE` are built-in value types, not classes and not
ordinary `STRUCT`s. Their representation is not writable by user code.
`TIMESTAMP` remains fully compatible with `INT64`.

The supported civil-date range is `0001-01-01` through `9999-12-31`.
`TIME` ranges from `00:00:00.000` through `23:59:59.999`; `24:00` and leap
seconds are outside 0.1. Defaults are `1970-01-01`, `00:00:00.000`, and `UTC`.

## Text interchange

`TIMESTAMP` uses the RFC 3339 profile of ISO 8601. Its canonical text is
`YYYY-MM-DDTHH:MM:SS.mmmZ`. Parsers accept `Z` or a numeric ISO 8601 offset and
normalize to UTC. Fractional seconds beyond milliseconds are accepted only
when every discarded digit is zero; BN never rounds or truncates implicitly.

`DATE` uses `YYYY-MM-DD`; `TIME` uses `HH:MM:SS.mmm`. `TIMEZONE` uses a
canonical IANA TZDB identifier such as `America/Sao_Paulo`; `-03:00` is an ISO
8601 UTC offset, not a time-zone identifier.

The runtime must report its TZDB release when a result depends on time-zone
rules. `TIMEZONE` conversion rules and parse/format entry points are implemented
with the Sprint 12 temporal runtime contract.

## Alternatives rejected

- Ordinary public `STRUCT`s: callers could create invalid dates and times.
- Classes: identity, heap allocation, aliasing, and `NULL` are inappropriate
  for small immutable temporal values.
- A fixed numeric `TIMEZONE`: an offset cannot represent daylight-saving or
  historical IANA rules.
- Expanded ISO years: they would prevent every `TIMESTAMP` from round-tripping
  through the required RFC 3339 interchange profile.
