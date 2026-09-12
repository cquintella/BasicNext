# ARC / lifetime conformance checklist (0.5.0)

**Status:** Normative checklist for Quorra gate and implementation waves.  
**Authority:** [`language-0.5.0.md`](language-0.5.0.md), [`0.5.0.ebnf`](0.5.0.ebnf),  
[`todo/proposals/bucket-0.5.0-corrective.md`](../../todo/proposals/bucket-0.5.0-corrective.md).

This document lists **required fixtures**. Executable `.bn` files are a **later
wave** (M2+). A wave may not claim ARC/lifetime “done” until every applicable
row has evidence (path + command + exit status).

## Done when (per wave)

| Claim | Done when |
| --- | --- |
| Spec lock (this docs train) | Checklist present; Quorra gate OK; no `DELETE` in 0.5.0 grammar |
| Interpret ARC (M2) | Each fixture below has a `.bn` (or equivalent harness) green on interpret |
| Native ARC (M4) | Same fixtures green on compile path, or honest deferred rows |

## Required fixtures

| ID | Scenario | Must observe | Forbidden |
| --- | --- | --- | --- |
| F1 | Strong aliasing | Two strong bindings share one object; both see mutations | Premature deinit while either strong lives |
| F2 | Scope deinit | Last strong leaves scope → destructor runs once | Double-deinit |
| F3 | Reassign drop | `x = NEW …` releases previous if no other strong | Leak of previous without other aliases; kill of other aliases |
| F4 | Cycle + weak | Mutual `AS WEAK … OR NULL` (or one weak edge) → no leak; after last strong drops, weak reads **`NULL`** (`IS NULL`) | Strong cycle leak presented as success |
| F5 | Use-after-release | `RELEASE` then use same binding → **error** | Silent reuse |
| F6 | `RELEASE` primary | Early end of scalar local; use-after-release error | Refcount story on primaries |
| F7 | `RELEASE` struct | Ends aggregate binding; nested class fields released as strong drops | Element-hole semantics |
| F8 | `RELEASE` vector (aggregate) | `RELEASE tickets` ends whole vector binding | — |
| F9 | No `RELEASE a[i]` as remove-middle | Fixed vector element release as “remove middle” rejected (compile or documented diagnostic) | Punching holes in fixed vectors |
| F10 | `RELEASE` object | Drop one strong; deinit only at count → 0 | Kill-all-aliases |
| F11 | Tickets Close + RELEASE | Loop `tickets[i].Close()` then `RELEASE tickets` (or scope exit without RELEASE) | `DELETE`; `RELEASE tickets[i]` as element remove |
| F12 | No `DELETE` | Grammar/fixtures under 0.5.0 contain **zero** `DELETE` keyword | Any `DELETE` statement |
| F13 | Typed AWAIT + args | `ASYNC queue PiPart(i)` (or `queue.Async(PiPart, i)`); `AWAIT` → `T OR Error` | Untyped discard of worker result as the only API |

## Evidence layout (when implementing)

Prefer:

```text
docs/superpowers/evidence/arc-0.5.0/<fixture-id>/
  program.bn
  NOTES.md          # command, backend, exit code, observation
```

Quorra **ARC compliance gate** on every M* wave: reject reintroduction of
`DELETE`, force-dispose, or semi-manual “must dispose to be correct” teaching.
