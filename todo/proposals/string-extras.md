# Proposal: BNString extras (stdlib) + string interpolation (language, deferred)

**Status:** Surface B (interpolation) remains deferred.  
**Preferred string helper (Carlos lock 2026-09-12):** **`Tokenizer`** (iterator `Next`), **not** Split→vector.  
Surface A vectorial (`Split`/`Join`/`Contains`/…) is **not** the preferred path.  
**Date:** 2026-09-07 (locks updated 2026-09-12)  
**Bucket link:** Track B helpers → [`ongoing/bucket-0.4.7.md`](../../ongoing/bucket-0.4.7.md) §7.5 / **G7.5-string** (G7.5 closed deferred; this proposal tracks the post-defer preferred API).  
**Interpolation** (`$"..."`) is **language DNA** and is **not** in the 0.4.7 critical path (Appendix E / Defer-until post-0.4.7 language bucket).

Nothing here is normative until accepted into `docs/library/` (and, for interpolation, `docs/language/`).

---

## Problem

BN already has character indexing and `LEN`, but everyday string work still needs
ad-hoc loops for:

- walking separator-delimited fields (split-like)
- substring search (`Contains` / `IndexOf`) — optional later
- trim of leading/trailing whitespace — optional later

Separately, modern languages reduce print/format noise with **interpolated**
string literals (e.g. `$"Total: {x} items"`). That is a **grammar + semantic**
change, not a stdlib method set.

---

## Split of concerns (planning)

| Surface | Kind | Status |
| --- | --- | --- |
| **Tokenizer (preferred)** | Example/class in repo **or** thin module — **not** core `STRING` inflation | **Carlos lock 2026-09-12** — primary direction for field walking |
| **A — BNString vectorial helpers** | `Split`→`STRING[]`, `Join`, `Contains`, … | **Not preferred**; deferred / demoted vs Tokenizer |
| **B — Interpolation `$"..."`** | Language DNA | **Deferred** |

**Non-goals:** changing `STRING` indexing/`LEN` rules; regex; locale-aware case folding beyond what the spec already allows; mutating strings in place if BN strings remain immutable values; stuffing Tokenizer into core language `STRING` methods.

---

## Preferred API — `Tokenizer` (Carlos lock 2026-09-12)

**Name:** `Tokenizer` (not Splitter).  
**Shape:** KISS iterator — not “allocate a vector of all parts.”

| API | Meaning |
| --- | --- |
| `Tokenizer.New(text AS STRING, sep AS STRING) AS Tokenizer OR Error` | Build tokenizer over `text` with separator `sep`. |
| `Next() AS STRING OR EOF` | Next field; `EOF` when exhausted. |
| `Reset()` (optional) | Rewind to the start of `text` with the same `sep`. |

**Rules (locked intent):**

- **Empty `sep` → `Error` on `New`** (fail closed; no undefined “split every char” surprise in MVP).
- Prefer shipping as an **example/class in the repo** or a **thin module** — do **not** inchar core `STRING`.
- Surface A vectorial (`Split` / `Join` / `Contains` / …) is **not** the preferred path for this need.

### Sketch

```basic
// thin module or example class — exact IMPORT id at library accept
LET tok AS Tokenizer OR Error = Tokenizer.New("a,b,c", ",")
IF tok IS Error THEN
    STOP 1
END IF
REPEAT
    LET part AS STRING OR EOF = tok.Next()
    IF part IS EOF THEN
        EXIT REPEAT
    END IF
    PRINT part
UNTIL FALSE
```

### Acceptance (when scheduled — not claiming done here)

1. Normative library or example doc names `Tokenizer` and the table above.
2. Interpret fixtures: empty sep → Error; walk fields; EOF after last; optional Reset.
3. No core `STRING` keyword/method growth for this feature.
4. Evidence with `bn run` (or agreed harness). **Docs lock in this commit only — no runtime yet.**

---

## Surface A — BNString vectorial helpers (demoted)

Earlier draft minimum set (`Split`→`STRING[]`, `Join`, `Contains`, `IndexOf`, `Trim`) remains recorded for history but is **not** Carlos’s preferred direction for delimiter walking. If a future bucket revisits vectorial helpers, it needs a **new** accept — do not treat G7.5 deferral as silent approval of Surface A over Tokenizer.

### Access (historical draft)

```basic
IMPORT BNString AS S
```

### Functions (historical draft — not preferred)

| API | Meaning (draft) |
| --- | --- |
| `S.Split(text AS STRING, separator AS STRING) AS STRING[]` | Split on separator into a vector. |
| `S.Join(parts AS STRING[], separator AS STRING) AS STRING` | Join parts with separator. |
| `S.Contains` / `S.IndexOf` / `S.Trim` | Search / trim helpers. |

---

## Surface B — Interpolation (deferred)

**Sketch only (not current accept):**

```basic
LET msg AS STRING = $"Total: {count} items"
```

Requires grammar productions, escape rules, type-checking of `{expr}`, and
lowering to concat/`PRINT` operands — **language DNA**.

**Defer-until:** post-0.4.7 language bucket / AQ when Carlos schedules DNA work.

---

## Relation to BNText

[`bntext-markdown.md`](bntext-markdown.md) is a different value type (Markdown
portability). Tokenizer / any future BNString helpers are plain `STRING`
processing. Do not merge modules without a separate accept.

---

## Open questions

1. ~~Empty separator~~ — **LOCKED:** `Tokenizer.New` → **Error** if `sep` is empty.
2. Module vs example-class packaging for `Tokenizer` (Carlos prefers either; lock path at implement).
3. Whitespace set for any future `Trim` (ASCII only vs Unicode Zs)?
4. Whether vectorial `Split`/`Join` ever return as a separate optional module after Tokenizer ships.
5. Interpolation scheduling (Surface B) — still deferred.

---

## History

- 2026-09-07 — Expanded from thin draft; split A/B; linked to bucket 0.4.7 G7.5.
- 2026-09-07 — Surface A deferred for post-0.4.7; normative semantics and
  runtime/LLVM contract remain unaccepted.
- 2026-09-08 — Carlos explicitly accepted deferring Surface A as a whole and
  closed G7.5. Earlier discussion of empty-separator and empty-needle behavior
  did not accept a normative API; semantic questions remained open for the
  future string-library bucket.
- 2026-09-12 — Carlos lock (via Quorra): prefer **`Tokenizer`** (iterator
  `Next`), not Split→vector. API: `New(text, sep) AS Tokenizer OR Error`,
  `Next() AS STRING OR EOF`, optional `Reset`; empty sep → Error on New;
  name Tokenizer (not Splitter); example/class or thin module — **not** core
  STRING; Surface A vectorial is not the preferred path. Docs only.
