# Proposal: HOST.Ui capability and BNUI module (v0)

**Status:** Proposed — design only (no IR / host / LLVM / interpreter work in this document).  
**Date:** 2026-09-07  
**Owner (tracker):** Doug until Carlos names implementer.  
**Motivation:** Teach interactive UI in Basic Next without growing language keywords, without an MVC framework, and without inventing jagged array literals. UI must sit on an explicit `HOST` capability plus an imported module, matching [PHILOSOPHY.md](../../PHILOSOPHY.md) (small core, KISS, explicit contracts, capabilities for reach, anti-framework).

**Related (do not conflate):**
- [PHILOSOPHY.md](../../PHILOSOPHY.md) — principles 3–7, 9; anti-goals (framework, kitchen-sink, implicit typing)
- [host-capabilities.md](host-capabilities.md) — exploratory HOST pattern (dom/gpu sketches); this proposal is the concrete UI v0 slice
- [c-ffi.md](c-ffi.md) — another HOST capability precedent (`HOST.c`); UI is not FFI
- Architecture host traits / execution policy — deny ≠ unimplemented; a missing UI host fails closed as unsupported capability, not as “language invalid”

Nothing here is normative until accepted into `docs/language/`, `docs/library/host.md` (or successor), and `modules/bn/` as appropriate.

---

## Problem

BN can already teach console I/O and HOST capabilities (Clock, Console, Net, …). There is **no** portable, teachable path to a minimal windowed UI:

| Gap | Today |
| --- | --- |
| Language | No UI keywords (correct) — and no accepted module/capability contract either |
| Teaching | Hello-world UI in other languages drags Window/MVC or DOM ceremony |
| Portability | Vendor GUI APIs must not become BN reserved words |
| Layout | Want a default that is obvious in a short example without absolute coordinates |

Carlos’s draft goal: a hello that imports UI, reacts to events through one exported handler, and lays out a few controls in reading order — **without** inventing `EVENT` syntax or a framework tax.

---

## Goals (v0)

1. Expose UI as **`HOST.Ui`** (capability) + **`IMPORT BNUI AS UI`** (typed module helpers). No new language keywords (`EVENT`, `WINDOW`, …).
2. Ship a **Flow-only** layout: ordered rows of controls via **typed builders** (`UI.Row` / `UI.Rows`, or a flat list plus an explicit row-break helper). Teachable in one screen.
3. Define a single exported entry contract: **`FUNCTION UI.EventHandler() AS VOID`**. The host owns the real OS/imgui loop; BN does not pretend to be the event pump.
4. Minimum controls: **Label**, **Text**, **Button** (each with a stable id for Get/Set).
5. Keep the announced surface small enough for 0.x production bar: explicit non-goals, no stub “coming soon” APIs in the happy path.
6. Document how this interacts with **`Start` / entry validation** so interpret and compile cannot diverge by silence.

## Non-goals (v0) — do not implement or advertise as done

Explicitly **out of v0** (candidates for a later **v0.1** proposal revision, not silent scope creep):

- Absolute positioning (`At(x,y)` / `Place`) and mixed flow+absolute packing.
- Scroll panes / automatic scrollbar when content exceeds client area.
- Rich controls (menus, trees, tables, canvases, dialogs beyond the three minimum).
- CSS-like styling systems, themes, or layout constraints solvers.
- Invented jagged matrix literals `[[a,b],[c]]` as language syntax for layout.
- A new `EVENT` / `ON` keyword or a second IR “UI dialect.”
- Claiming interpret↔compile UI parity before fixtures and support-matrix rows exist.

---

## Design principles (how this obeys PHILOSOPHY)

- **KISS / ideas into software** — Flow default states “controls in reading order.” Absolute and scroll earn their place when a real example needs them.
- **Small core, broad reach** — richness in `BNUI` + `HOST.Ui`, not reserved words.
- **Explicit contracts** — imports required; event alphabet closed and named in the module; types on declarations.
- **Capabilities, not vendors** — programs target `HOST.Ui`, not “imgui” or “Win32” in source.
- **Anti-framework** — no prescribed MVC/Window subclassing; one handler + Get/Set is the teaching model.
- **Deliberate evolution** — this file is the proposal; acceptance updates language/library docs together with fixtures.

---

## Naming

| Piece | Name | Role |
| --- | --- | --- |
| Capability | `HOST.Ui` | What the host *can* provide: surface, event pump, paint. Import form TBD to match existing `HOST.*` registry casing (`HOST.Ui` preferred in prose; implementers follow registry normalization). |
| Module | `BNUI` | Portable helpers: builders, control constructors, event CONST/types, Get/Set. |
| Import alias | `IMPORT BNUI AS UI` | Teaching surface in examples. |
| Handler | `UI.EventHandler` | Single exported `FUNCTION … AS VOID` the host calls per event. |

Implementation sketch under the hood (imgui or other) is a **host detail**, not a BN keyword and not part of the language contract.

---

## Proposed surface (v0)

### 1. Capability and module

```basic
IMPORT BNUI AS UI
' Host must expose HOST.Ui; missing capability → fail closed (unsupported / policy),
' not a silent no-op.
```

Programs that need UI **import the module**. Whether `BNUI` re-exports or requires a separate `IMPORT HOST.Ui` follows the same rule as other HOST-backed modules once library docs lock it — v0 requires the capability to be **present and policy-allowed** before `Run`.

### 2. Event handler contract (no new keyword)

Exactly one application-facing hook for v0 teaching examples:

```basic
FUNCTION UI.EventHandler() AS VOID
  LET e AS STRING = UI.Event()
  IF e = UI.EVENT_LOAD THEN
    LET app AS OBJECT = NEW(UI.App, "Hello", UI.Rows(
      UI.Row(NEW(UI.Label, "lbl", "Nome:"), NEW(UI.Text, "nome", "")),
      UI.Row(NEW(UI.Button, "go", "OK"))
    ))
    app.Run()
  ELSE IF e = "go" THEN
    ' button id; see Events section for alphabet vs control ids
    LET nome AS STRING = app.Get("nome")
    app.Set("lbl", "Olá, " + nome)
  END IF
END FUNCTION
```

Illustrative only: exact `NEW` / `Rows` signatures are fixed when the module stub is accepted into `modules/bn/`. The **shape** is normative for the proposal: builders, not jagged literals; one handler; `Run` only after construction on load.

**Host owns the loop.** `app.Run()` means: *hand the thread to the host until the UI session ends* (window closed / host teardown). It is a **boundary call**, not hidden MVC magic. Teaching materials must say in two lines:

1. BN exports `UI.EventHandler`.
2. The host calls it with the current event set; `Run` blocks in the host until the session finishes.

Re-entrancy rules (whether `EventHandler` may run nested while inside `Run`) are an open implementation item — see Open questions.

### 3. Layout: Flow only (builders)

- Default layout is **Flow**: a sequence of **rows**; each row is a sequence of controls in order; rows are centered as a teaching default; long rows may wrap *within the flow model* only if the module documents wrap as part of Flow (v0 may define wrap as “implementation may wrap; authors should keep rows short”).
- Authors build structure with **`UI.Row(...)`** and **`UI.Rows(...)`** (or equivalent typed helpers). Alternative acceptable v0 shape: a single flat `UI.Controls(...)` list plus an explicit **`UI.Break`** / row-end sentinel object — uglier, still explicit, still no new array syntax.
- **Forbidden in v0 examples and grammar claims:** presenting `[[a,b],[c]]` as if BN already had jagged matrix literals for layout.

Resize behavior in v0: if content does not fit, the host may clip or letterbox; **scrollbar / scroll pane is out of v0** (listed under Non-goals). Do not document scroll as available.

### 4. Controls (minimum)

| Control | Role | Id |
| --- | --- | --- |
| `UI.Label` | Read-only text | Required string id |
| `UI.Text` | Editable single-line text | Required string id |
| `UI.Button` | Clickable; click delivers its id as the current event (see below) | Required string id |

`app.Get(id)` / `app.Set(id, value)` read and write control content by id. Value types in v0 are the obvious scalars for these controls (`STRING` for Label/Text; Button has no value payload beyond the click event).

**Typing debt (recorded, not solved in v0):** string ids and Get/Set are the teaching MVP and will rot like early DOM APIs if left forever. Evolution should move toward typed handles / `OBJECT` identities or module-level id CONSTANTS — see Evolution.

### 5. Events: closed alphabet + module CONST

`UI.Event()` returns a `STRING` in v0 **only** under a **closed, documented alphabet** plus control ids:

| Kind | Representation | Notes |
| --- | --- | --- |
| Session load / first paint setup | `UI.EVENT_LOAD` (module `CONST` equal to `"load"`) | Prefer comparing to the CONST, not a magic literal scattered in user code |
| Button click | The button’s **id** string | Same string space as ids — document collision rule: reserved event names must not be used as control ids |
| Session end (optional in v0) | `UI.EVENT_CLOSE` if the host can deliver it before teardown | If not deliverable portably, omit from v0 alphabet rather than fake it |

No open-ended free-form event strings without updating the module contract and this proposal. Stronger enums / discriminated unions are **evolution**, not a language `ENUM` keyword requirement for v0.

### 6. Entry point and `Start` (provisional decision — must not stay implicit)

**Provisional decision for v0 design** (implementation must confirm against `validate_for` / support matrix):

1. Every UI program still has **`FUNCTION Start() AS VOID`** (or the existing executable entry rules) so language validate and both backends keep one entry story.
2. `Start` **registers** the UI session (or simply returns after ensuring `UI.EventHandler` is the exported handler) and **does not** duplicate the host loop. The preferred teaching pattern:

```basic
FUNCTION Start() AS VOID
  ' Entry exists for validate_for / run / build.
  ' Host discovers UI.EventHandler by module export convention and drives events.
  ' Start may be empty or perform non-UI setup only.
END FUNCTION

FUNCTION UI.EventHandler() AS VOID
  ' ... as above ...
END FUNCTION
```

3. **Hello-ui without calling UI from `Start`:** allowed if the host’s UI runner loads the module and calls `EventHandler` with `EVENT_LOAD` after process start. That runner is a **target/capability mode**, not a second language entry keyword.
4. If a backend cannot support UI, `validate_for` / capability checks fail with **support** diagnostics (`TARGET_UNSUPPORTED_*` / HOST policy), distinct from ill-formed IR.

**Open:** exact discovery rule (export name, HOST registration API, CLI `bn run --ui`). Marked open below; silence is not acceptance.

---

## Teaching sketch (normative shape, illustrative API)

```basic
IMPORT BNUI AS UI

FUNCTION Start() AS VOID
END FUNCTION

FUNCTION UI.EventHandler() AS VOID
  LET e AS STRING = UI.Event()
  IF e = UI.EVENT_LOAD THEN
    LET app AS OBJECT = NEW(UI.App, "Demo", UI.Rows(
      UI.Row(NEW(UI.Label, "msg", "Ready"), NEW(UI.Button, "ping", "Ping"))
    ))
    app.Run()
  ELSE IF e = "ping" THEN
    app.Set("msg", "Pong")
  END IF
END FUNCTION
```

Notes for authors of examples: keep ids distinct from reserved event CONST values; do not demonstrate absolute layout or scrolling in v0 samples.

---

## Implementation boundaries (when accepted — not this PR)

| Layer | Responsibility |
| --- | --- |
| Spec / library docs | Capability `HOST.Ui`, module `BNUI`, event alphabet, builders, non-goals |
| Frontend / semantic | Resolve imports; typecheck handler signature; no new keywords |
| IR | Likely HOST calls / externs only — no UI-specific IR kinds without a follow-on architecture note |
| `bn_rt` / host | Imgui (or other) backend behind `HOST.Ui`; policy deny; event pump; `Run` blocking semantics |
| Support matrix | Native UI host row; wasm/other may be unsupported until separately proposed |

This proposal does **not** authorize stubs that claim UI works.

---

## Evolution (after v0)

Ordered candidates — each needs its own acceptance, not drive-by expansion:

1. **v0.1 layout:** `At` / `Place` absolute controls excluded from flow pack; then scroll pane when content exceeds client area.
2. **Stronger events/ids:** typed event values or handles instead of overlapping string spaces; reduce Get/Set stringly debt.
3. **More controls** only with teaching examples and tests.
4. **CLI/IDE runners** that make `--ui` / debug story obvious without changing language DNA.

---

## Acceptance criteria (for later implementation work)

1. Spec + `BNUI` module text + positive/negative fixtures updated together.
2. At least one hello example using Flow builders only; CI/host gate fails closed without `HOST.Ui`.
3. No `EVENT` keyword; no jagged layout literal in grammar.
4. Support diagnostics distinct from language-invalid IR.
5. Interpret is reference where UI is supported; compile claims only with matrix evidence.

---

## Open questions

1. **Handler discovery:** How does the host bind `UI.EventHandler` — fixed export name, registration call from `Start`, or CLI flag? Must be locked before implementation.
2. **`Run` re-entrancy:** Can the host deliver further events (including nested) while `Run` is on the stack? Preferred teaching model: yes, host calls `EventHandler` re-entrantly; needs a clear stack/concurrency story for interpret vs native.
3. **`app` binding across events:** Illustrative code uses `app` in click branch — is `app` a module-level binding set on load, a thread-local host register, or must every branch `UI.App.Current`? Needs an explicit rule so examples are honest.
4. **Import shape:** Is `IMPORT BNUI` enough, or must programs also `IMPORT HOST.Ui` for policy visibility (prefer visible capability import if it matches other HOST modules).
5. **Wasm / headless:** v0 native-desktop only, or explicit unsupported rows from day one?
6. **Reserved id clash:** Formal list of reserved event CONST strings that may not be control ids.

---

## Decision summary

| Topic | v0 decision |
| --- | --- |
| Keywords | None new |
| Capability / module | `HOST.Ui` + `BNUI` (`IMPORT BNUI AS UI`) |
| Layout | Flow only via typed builders (or flat+Break) |
| Absolute / scroll | Out of v0 |
| Events | Closed alphabet + module CONST; button id clicks; stringly Get/Set with recorded debt |
| Loop | Host owns; `Run` = hand thread to host until session ends |
| Entry | Keep `Start` for validate/backends; UI driven through `EventHandler`; discovery rule still open |

