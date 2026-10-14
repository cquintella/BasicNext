# String operations decision for Basic Next 0.5

Status: internal design note; proposal, not an accepted language change.

## Decision summary

Keep the core `STRING` surface deliberately small. The current language already
has the operations needed by the compiler examples, host APIs, and text
algorithms:

| Operation | Status | Rationale |
| --- | --- | --- |
| String literal and immutable value | Keep | Required data model and safe sharing between interpreter and compiler. |
| `a + b` concatenation | Keep | Fundamental composition operation; already specified and implemented by both backends. |
| `a = b`, `a <> b` equality | Keep | Needed for parsing, search algorithms, protocol fields, and deterministic tests. |
| `LEN(s)` | Keep | Defines the Unicode-scalar sequence boundary and makes loops/index checks explicit. |
| `s[i]` read-only indexing | Keep | Sufficient for scanners, KMP/Boyer–Moore/Rabin–Karp, codecs, and character processing. |
| `ASC(s)` / `CHAR(n)` | Keep | Explicit Unicode scalar ↔ integer bridge; avoids a separate `CHAR` type in the core. |
| `PRINT`/`INPUT` string transport | Keep as host/console APIs | I/O, not string transformation; must remain outside the string value abstraction. |

These seven language-level capabilities are the minimum viable core for 0.5.
They should receive cross-backend conformance fixtures, Unicode scalar tests,
and bounded-allocation checks before any expansion.

## Defer from the core

Do not add string methods or new keywords for `contains`, `indexOf`, `startsWith`,
`endsWith`, slicing, replacement, splitting, joining, trimming, case folding,
regular expressions, formatting, or interpolation in the 0.5 core. They add
surface area, Unicode policy decisions, allocation/error contracts, and LLVM/IR
work without being necessary for the current algorithms or host modules.

If demand appears, provide an imported `BNText` module with typed functions
instead of extending `STRING` syntax. Candidate order is:

1. `Contains` / `IndexOf` (non-allocating search semantics where possible);
2. `Slice` (scalar-indexed, bounds-checked, explicit allocation);
3. `StartsWith` / `EndsWith`;
4. `Trim` and explicit Unicode case operations;
5. `Replace`, `Split`, and `Join` with documented allocation limits;
6. regex and formatting only after a separate proposal and dependency review.

`BNText` must not be treated as a language builtin. Its functions should use
the same validated IR and host policy boundaries as every other external
module, and unsupported target behaviour must be reported by `validate_for`.

## Semantic constraints

- `STRING` is immutable and is a sequence of Unicode scalar values, not bytes
  and not grapheme clusters. `LEN` and indexing therefore count scalar values.
- Indexing is zero-based, returns a one-scalar `STRING`, and raises the stable
  `INDEX_OUT_OF_BOUNDS` error when invalid.
- `ASC` rejects an empty string; `CHAR` rejects negative, surrogate, and
  out-of-range scalar values.
- Concatenation must preserve exact scalar order and must have equivalent
  observable behaviour in interpreter and LLVM backends.
- No implicit numeric/boolean/string coercion is introduced by this decision.
- Allocation limits and cancellation remain runtime/host contracts, not hidden
  behaviour of a new string operator.

## Acceptance gates for implementation work

Before adding any operation, require: (1) a language-spec amendment, (2)
positive and negative fixtures, (3) one validated-IR representation, (4)
interpreter/LLVM parity, and (5) an explicit Unicode, allocation, error, and
cancellation contract. Until those gates are met, the operation remains
library-level or deferred.
