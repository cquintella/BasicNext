# BNString standard library

## Status

Official **extras** module (Carlos lock 2026-09-12 via Quorra). Source of truth:
`modules/bn/BNString.bn`. Implemented in Basic Next (LEN + index + loops) — not a
`bn_rt` native stub. Primary `STRING` remains unchanged.

Nothing here replaces core `STRING` indexing/`LEN`.

## Access

```basic
IMPORT BNString AS S

LET s AS S.String = NEW S.String("hello")
LET tok AS S.Tokenizer OR NULL = s.Tokenizer(",")
```

Logical import name: `BNString`. Path is not used in source.

## Types

| Type | Role |
| --- | --- |
| `S.String` | Object wrapper over a `STRING` value (`PRIVATE value`) |
| `S.Tokenizer` | Iterator over separator-delimited fields (`Next` → `STRING OR EOF`) |

## `S.String` methods

| Method | Meaning |
| --- | --- |
| `CONSTRUCTOR(s AS STRING)` | Store `s` |
| `Len() AS INTEGER` | `LEN(value)` |
| `CharAt(i AS INTEGER) AS STRING OR NULL` | One scalar at `i`, or `NULL` if OOB |
| `Concat(other AS STRING) AS S.String` | `NEW String(value + other)` |
| `Contains(needle AS STRING) AS BOOLEAN` | Substring test |
| `IndexOf(needle AS STRING) AS INTEGER OR NA` | First index, or `NA` |
| `Trim() AS S.String` | Strip ASCII space/tab/LF/CR (codes 32/9/10/13) |
| `Tokenizer(sep AS STRING) AS S.Tokenizer OR NULL` | Field iterator; empty `sep` → `NULL` |
| `ToString() AS STRING` | Underlying primary |

## `S.Tokenizer`

| API | Meaning |
| --- | --- |
| `Tokenizer.New(text, sep) AS Tokenizer OR NULL` | Empty `sep` → `NULL` (fail-closed) |
| `Next() AS STRING OR EOF` | Next field |
| `Reset() AS VOID` | Rewind |

Preferred over Split→`STRING[]` (see `todo/proposals/string-extras.md`).

## Precompiled objects

No `.bno` / module object pipeline in the toolchain yet. **Intent (0.5.x):** ship
precompiled module artifacts alongside source. Until then, distribute
`modules/bn/BNString.bn` source only (`modules/README.md`).

## Tour

`examples/bnstring_tour.bn` — `IMPORT BNString` smoke test (`bn run`).
