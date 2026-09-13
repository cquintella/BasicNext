# Basic Next 0.5.0 — Corrective train (ARC DNA + typed dispatch returns)

**Status:** Active executable bucket (Tron audit 2026-09-12). **Tag policy locked by Carlos: ARC all-or-nothing on BOTH interpret (M2) and compile/LLVM (M4) + F1–F13 + typed AWAIT; not locks/dispatch-only; not interpret-only.**  
**Objective:** Make the **locked** 0.5.0 language DNA real: ARC compliance (no `DELETE`), typed `AWAIT → T OR Error`, fixtures F1–F13. Spec locks already exist; this file is the **implementation WBS**.

**Owner (tracker):** Tron until Carlos names implementer.  
**Gate:** Quorra ARC-compliance + no done-oco (fixtures + evidence) before Carlos.


## Carlos lock — tag 0.5.0 content (2026-09-12, via Quorra; **amplified same day**)

**Policy:** **All or nothing on ARC for both backends.** Version 0.5.0 must not ship as locks-only, typed-`AWAIT`-only, or **interpret-only ARC**.

**In the 0.5.0 claim (required):**
1. Full **interpret** ARC (`bn run`) as executable reference: strong/weak (`AS WEAK`), optional `RELEASE`, **`DELETE` keyword purged** from grammar/FE/fixtures/examples under the 0.5.0 surface. (**M2**)
2. Full **compile** ARC (`bn build` / LLVM): retain/release (or equivalent) so the same fixtures F1–F13 are green on the native path, or an honest matrix row is forbidden — **no silent native drift**. (**M4** — now **in** scope)
3. Conformance fixtures **F1–F13** with committed evidence on **interpret and compile** (no empty done).
4. Typed **`AWAIT → T OR Error`** (D0–D1 on interpret; **D2** native typed AWAIT **before or with** F13 native / M4) in the same release train. F13 native is mandatory; D2 is **in** the claim.
5. Book/migration alignment for 0.4.x → 0.5.0 memory model.

**Still out unless further expanded:** Unowned; HOST-as-ARC classes (capability `Close`/`*_close` stays).

**Extra sprints:** If M4 (or D2) cannot finish inside the first implementation cycle, **append extra sprints at the end of this bucket** as **calendar overflow only** — do not close 0.5.0 with only M2 green, and **never** reopen an out-of-minimum / slip-to-next-tag scope cut for compile ARC. Ordering tip: M2 reference first; **D2 before or with M4 F13 native**; D1 may parallelize with M2.

**Supersedes:** (1) Tron “0.5.0 = locks + D0–D1; M2 → next tag”; (2) prior Quorra note that compile ARC was outside the minimum tag.


## Inputs (locks — do not renegotiate here)

| Doc | Role |
| --- | --- |
| [`todo/proposals/bucket-0.5.0-corrective.md`](../todo/proposals/bucket-0.5.0-corrective.md) | Plan locks + waves M*/D* |
| [`todo/proposals/dispatch-typed-return.md`](../todo/proposals/dispatch-typed-return.md) | Typed AWAIT primary; ABI |
| [`docs/0.5.0/`](../docs/0.5.0/) | Normative grammar/semantics (`0.5.0.ebnf`, `language-0.5.0.md`, `keywords.md`) |
| [`docs/0.5.0/arc-conformance.md`](../docs/0.5.0/arc-conformance.md) | Fixtures F1–F13 |
| [`docs/0.5.0/memory-migration.md`](../docs/0.5.0/memory-migration.md) | 0.4 → 0.5 memory migration; book ch.7 superseded |

### Locked DNA (Quorra reject if violated)

- Automatic **strong** retain/release; **weak** = `AS WEAK ClassName`; dead → `NULL`
- **`DELETE` keyword purged** (grammar/book/fixtures) — never reintroduce; HOST stays `Close`/`*_close`
- **`RELEASE`** optional advanced (primary/vector/struct/object); never kill-all-aliases; no `RELEASE a[i]` as remove-middle
- Typed **`AWAIT` → T OR Error** primary; `Ticket.Result` not primary
- Tickets teach: Close per element, `RELEASE tickets` aggregate only

## Architecture locks

| Lock | Implication |
| --- | --- |
| Spec > interpret > compile | Interpret is ARC reference; LLVM must not claim ARC until M4 |
| One IR | Prefer semantic + runtime retain/release; new IR kinds only if lowering proves need |
| No force-dispose | Reject any “DELETE kills aliases” path |
| No done-oco | Wave not done without fixtures + committed evidence |
| Cloud Agents unavailable | Work on Andromeda checkout `/Users/caq/src/BasicNext` |

## Non-goals (this bucket)

- GC / COW / unowned MVP
- DataFrame/File as ARC classes in the same slice
- Closing 0.4.7 matrix compile sweep by pretending ARC fixes FOR/PRINT gaps
- Shipping both typed AWAIT and `Ticket.Result` as equal primary APIs
- Inventing `.bno` precompile pipeline (intent only; BNString already documented)

## Wave status (plan vs code)

### Track M — Memory / ARC

| Wave | Intent | Spec status | Code status (2026-09-12) |
| --- | --- | --- | --- |
| **M0** | Locks recorded | **DONE** (proposal + Quorra gates) | N/A |
| **M1** | Spec/grammar/book purge DELETE; WEAK; RELEASE; migration | **Mostly DONE** in `docs/0.5.0/` + book overlay; book ch.7 body still 0.3 manual text (superseded by overlay, not rewritten) | FE still parses `DELETE` (0.4 grammar live) |
| **M2** | Interpret strong/weak + F1–F13 | Spec ready | **NOT STARTED as language surface** — `bn_arc.rs` untracked private Rust helper only; heap still `USE_AFTER_DELETE` / `DOUBLE_DELETE` |
| **M3** | `value-memory-abi` object rows closed for interpret | Checklist still PARTIAL historically | **OPEN** |
| **M4** | LLVM retain/release + matrix | **In claim** (overflow sprints OK; not out-of-minimum) | **OPEN** |

### Track D — Typed dispatch

| Wave | Intent | Spec status | Code status (2026-09-12) |
| --- | --- | --- | --- |
| **D0** | Locks into language + BNDispatch module text | Spec in `docs/0.5.0` + proposal | **`modules/bn/BNDispatch.bn` still** `Async(work AS FUNCTION() AS VOID OR Error)` / Wait VOID — **not typed** |
| **D1** | FE + interpret ticket stores `Value`; AWAIT unboxes | Spec ready | **OPEN** (ABI `bn_rt_dispatch_await` already has result pointer) |
| **D2** | LLVM out-param + matrix | After D1 | **OPEN** |
| **D3** | `parallel_work` / `parallel_pi` honest | After D1 | **OPEN**; examples still DELETE-heavy elsewhere |

## File map (needed to function)

### Already present (spec / helpers)

| Path | Notes |
| --- | --- |
| `docs/0.5.0/*` | Authority for 0.5.0 surface |
| `todo/proposals/bucket-0.5.0-corrective.md` | Locks |
| `todo/proposals/dispatch-typed-return.md` | Dispatch returns |
| `crates/bn_runtime/src/bn_arc.rs` (+ test) | **Local untracked** — Rust `BnArc` test helper; **not** language ARC yet |
| `crates/bn_rt` dispatch await result | ABI ready for D1 |

### Must change / create (implementation)

| Area | Paths (expected) | Wave |
| --- | --- | --- |
| Grammar live | Frontend keyword/grammar still 0.4: purge `DELETE`, add `RELEASE`, `WEAK` type form | M1→M2 |
| Semantic ARC | Assign/param/return/scope retain-release; weak→NULL; use-after-release | M2 |
| Heap / runtime | Replace DELETE-centric diagnostics; strong counts; destructor on 0 | M2 |
| FE WEAK | `AS WEAK ClassName` parsing + typecheck | M2 |
| RELEASE | Statement lowering for all operand kinds | M2 |
| Fixtures | `docs/superpowers/evidence/arc-0.5.0/F01…` + `.bn` programs F1–F13 | M2 |
| ABI doc | `docs/architecture/value-memory-abi.md` object lifetime | M3 |
| BNDispatch | Typed worker / AWAIT result surface in `modules/bn/BNDispatch.bn` + docs | D0–D1 |
| Interpret await | Ticket result storage; typed unbox | D1 |
| Examples purge | `examples/**` still use `DELETE` (bndata_tour, linear_collections, …) — migrate or quarantine for 0.5 claim | M1/M2 + D3 |
| Book | Full rewrite ch.7 (optional if overlay+migration accepted as M1) | M1 residual |

## Gap register (Tron 2026-09-12)

| ID | Gap | Severity | Notes |
| --- | --- | --- | --- |
| G1 | No `ongoing/bucket-0.5.0.md` before this file | Fixed here | Plan was only in proposal |
| G2 | Live toolchain still 0.4 `DELETE` | **Critical** | Spec 0.5.0 ≠ running grammar |
| G3 | Interpret ARC not wired; heap DELETE diagnostics | **Critical** | `bn_arc` ≠ done |
| G4 | BNDispatch / AWAIT still completion-only VOID | **Critical** for dispatch claim | ABI ready, language not |
| G5 | F1–F13 fixtures | **In progress → committed paths** | `docs/superpowers/evidence/arc-0.5.0/F1…F13/` (red until M2/D1) |
| G6 | Examples + tests still teach `DELETE` | High | Blocks “no DELETE in surface” claim |
| G7 | Book ch.7 body not rewritten | Med | Overlay + migration exist |
| G8 | ~~Tag content~~ — **LOCKED:** all-or-nothing ARC (M2+M4+F1–F13+D0–D1+D2) | Closed | Carlos 2026-09-12 via Quorra (amplified) |
| G9 | ~~Replay-until-Close + STRING in MVP T~~ | Closed | Locked: replay until Close; MVP T includes STRING (with VOID/INTEGER/FLOAT/BOOLEAN) |
| G10 | ~~Unowned confirm deferred~~ | Closed | Unowned remains deferred / out of 0.5.0 claim (non-goal confirmed) |
| G11 | `bn_arc.rs` untracked / not integrated | Med | Commit when M2 starts or drop if unused |
| G12 | Duplicative docs trees `docs/language/0.5/` empty vs `docs/0.5.0/` | Low | Carlos path is `docs/0.5.0/` — keep |

**Wrong / out of scope if claimed done:** treating `bn_arc.rs` or BNString Unicode as ARC lifetime done; treating binaries CI green as 0.5.0 language complete.


## Fixtures-first rule (Carlos 2026-09-12 via Quorra)

**No M2/D1 runtime implementation wave starts** until F1–F13 (and typed-AWAIT F13)
are **committed** under `docs/superpowers/evidence/arc-0.5.0/` with `program.bn` +
`NOTES.md`. Expected red on current toolchain is OK. Green evidence is required
to close the wave.

## Critical path (executable order)

Waves remain the scope map; **execution = sprints** in SECTION 1.

1. **Sprint 1 (D0–D1)** — Typed AWAIT on interpret for **known** worker `T` (F13 INTEGER first). Opaque-ticket **D1-TYPE** is a narrowed residual only — does **not** freeze M1/M2.  
2. **S-M1** — Live grammar/FE purge `DELETE`; `RELEASE` / `WEAK` surface.  
3. **S-M2** — Interpret ARC + RELEASE + WEAK; F1–F13 evidence on `bn run` (may parallelize with Sprint 1).  
4. **S-M3** — `value-memory-abi` object lifetime rows closed for interpret.  
5. **S-D2** — Typed AWAIT on compile/LLVM (**before or with** M4 F13 native — S-M4 must not require F13 native green while D2 is still later).  
6. **S-M4** — LLVM retain/release + F1–F13 on `bn build` (F13 native needs S-D2).  
7. **S-D3 / S-purge** — honest parallel examples + finish G6 DELETE example migration (calendar overflow OK; scope stays in claim).

**Never** treat extra sprints as reopening an out-of-minimum cut or slipping compile ARC to the next tag.

## SECTION 1 — Sprint execution

Carlos confirmed on 2026-09-12: the execution unit is a **complete sprint**.
Tasks within a sprint are not separate delivery boundaries. The **waves** above
remain the scope map; **execution = sprints** below.

**Dependency order (summary):** Sprint 1 (D0–D1 known-T) ∥ S-M1 → S-M2 → S-M3;
then **S-D2 before/with S-M4 F13 native**; S-D3 / S-purge can trail as overflow
without cutting claim. Do not block M1/M2 on opaque-ticket D1-TYPE.

### Sprint 1 — D0–D1 typed dispatch on interpret (narrowed D1-TYPE)

- [ ] **Deliverables:** BNDispatch typed worker contract; FE argument/result
  checking; interpret ticket stores `Value` and typed `AWAIT` unbox; IR
  negatives + target support checks; regression tests; F13 interpret evidence.
- [ ] **Dependencies:** D0 locks; fixtures-first gate (F1–F13 `program.bn` +
  `NOTES.md` in `98af828`). **Does not depend on** closing opaque-ticket
  D1-TYPE for all tickets.
- [ ] **Fixtures:** F13 (INTEGER sum) on `bn run`; plus typed-dispatch coverage
  for FLOAT / STRING / BOOLEAN (and VOID) workers — **F13 INTEGER alone ≠ done**.
- [ ] **Gates:** Quorra no done-oco; W1–W5 and GC-IR/GC-SUP/GC-DEP; Rust
  `fmt` / `test` / `clippy` / `git diff --check`.
- [ ] **Definition of Ready:** 0.5.0 locks; known worker `T` path specified;
  D1-TYPE delimited (below) so Sprint 1 is not frozen.
- [ ] **D1-TYPE (delimited):** Applies **only** to opaque tickets whose worker
  result type `T` cannot be established statically (e.g. opaque ticket
  parameter). **No** generic syntax; **no** silent default to VOID. Sprint 1
  **proceeds** for known `T` (F13 INTEGER and other statically known workers).
  D1-TYPE does **not** block M1/M2 and does **not** freeze all of D0–D1.
- [ ] **Extra acceptance (beyond F13 INTEGER):** dispatch payloads for MVP
  return set **VOID / INTEGER / FLOAT / STRING / BOOLEAN**; **replay until
  Close**; timeout / cancel / task / closed-ticket errors; early exits on the
  await path; reject mismatched args/results. **ARC retain/destructor chains
  belong to S-M2/S-M4** — do not make Sprint 1 depend on ARC payload lifetime.
- [ ] **Definition of Done:** Deliverables + acceptance green on interpret;
  evidence committed; IDE plugin updates if surface-facing; review agrees.
  Native claims require S-D2 evidence separately.
- **Baseline:** [2026-09-12 execution baseline](#execution-baseline--2026-09-12).
  F13 currently fails `ASYNC_RETURN_TYPE`.

### S-M1 — Grammar / FE surface purge (M1 residual)

- [ ] **Deliverables:** Live grammar/FE reserved words match `docs/0.5.0`:
  purge `DELETE`; parse `RELEASE`; `AS WEAK ClassName`; book/migration residual
  as needed (overlay OK if M1 docs accepted).
- [ ] **Dependencies:** Spec locks in `docs/0.5.0/` (already mostly DONE).
  Independent of opaque D1-TYPE.
- [ ] **Fixtures / checks:** Grammar/negative tests that `DELETE` is rejected;
  `RELEASE` / `WEAK` parse; F12 NOTES (a) `program.bn`-only DELETE scan.
- [ ] **Gates:** Quorra rejects any DELETE return on 0.5.0 surface.
- [ ] **Definition of Ready:** Normative EBNF/keywords/language docs present.
- [ ] **Definition of Done:** Active toolchain grammar authority is 0.5.0 (or
  dual-track flag documented); FE no longer accepts `DELETE` as keyword sugar;
  evidence noted in sprint close.

### S-M2 — Interpret ARC + F1–F13 on `bn run`

- [ ] **Deliverables:** Semantic + runtime strong/weak retain/release; optional
  `RELEASE`; weak→NULL; use-after-release diagnostics; destructor on strong→0;
  committed green evidence for F1–F13 under `docs/superpowers/evidence/arc-0.5.0/`.
- [ ] **Dependencies:** S-M1 (grammar surface) or equivalent FE readiness; may
  **develop in parallel with Sprint 1**, but **closes only after** Sprint 1
  typed AWAIT interpret is green enough for **F11/F13** (ticket Close pattern +
  typed await observables). Not blocked by D1-TYPE opaque residual.
- [ ] **Fixtures:** F1–F13 on interpret (`bn run`); F12 split checks per NOTES.
- [ ] **Gates:** Quorra ARC-compliance per fixture wave; no done-oco.
- [ ] **Extra acceptance:** retain on **params/returns**; **destructor chains**;
  **early exits**; use-after-release / invalid binding; weak cycle break; no
  force-dispose. Treat `bn_arc.rs` helper alone as **not** done.
- [ ] **Definition of Ready:** Fixtures-first paths committed (red OK until green).
- [ ] **Definition of Done:** F1–F13 green on `bn run` with NOTES evidence;
  Rust gate on the diff; Quorra pass.

### S-M3 — value-memory-abi object rows (interpret)

- [ ] **Deliverables:** `docs/architecture/value-memory-abi.md` object lifetime
  rows closed for interpret ARC observables; checklist PARTIAL → closed.
- [ ] **Dependencies:** S-M2 interpret observables stable enough to document.
- [ ] **Fixtures:** Cross-links to F1–F13 / ABI examples as needed.
- [ ] **Gates:** Doc review; no silent contradiction with M2 evidence.
- [ ] **Definition of Ready:** M2 semantics known for strong/weak/RELEASE.
- [ ] **Definition of Done:** ABI doc object section matches interpret reality;
  historical PARTIAL rows closed or explicitly N/A.

### S-D2 — Typed AWAIT on compile / LLVM (in claim; before M4 F13 native)

- [ ] **Deliverables:** LLVM/native out-param wiring for typed await; matrix /
  support rows; F13 (and multi-T) green on `bn build`.
- [ ] **Dependencies:** Sprint 1 (D1 interpret reference). **Ordered before or
  with S-M4’s F13 native** — S-M4 must **not** require F13 native green while
  D2 is scheduled later. Remove any “D2 optional/deferred” hedging.
- [ ] **Fixtures:** F13 native; FLOAT/STRING/BOOLEAN + INTEGER dispatch;
  replay-until-Close; error paths — **F13 INTEGER alone ≠ D2 done**.
- [ ] **Gates:** Quorra; matrix honesty (no silent native drift).
- [ ] **Extra acceptance:** MVP return set VOID/INTEGER/FLOAT/STRING/BOOLEAN
  on native; early exits on the await path; replay until Close;
  timeout/cancel/task/closed errors. **ARC retain/destructor chains belong to
  S-M4** — do not make S-D2 depend on ARC object lifetime.
- [ ] **Definition of Ready:** D1 interpret green for known T; ABI out-param
  contract unchanged.
- [ ] **Definition of Done:** Native typed AWAIT evidence committed; F13 native
  mandatory for claim.

### S-M4 — Compile / LLVM ARC + F1–F13 on `bn build`

- [ ] **Deliverables:** LLVM retain/release (or equivalent) so F1–F13 match
  interpret observables on `bn build`; design note as needed.
- [ ] **Dependencies:** S-M2 interpret reference; **S-D2 before/with F13
  native** (do not gate “all F1–F13 native” on a later D2). M3 may overlap.
- [ ] **Fixtures:** F1–F13 on `bn build` with NOTES compile commands.
- [ ] **Gates:** Quorra ARC gate on native; no interpret-only claim.
- [ ] **Extra acceptance:** retain params/returns; destructor chains; early
  exits; same observables as interpret for in-scope fixtures.
- [ ] **Definition of Ready:** M2 green reference; D2 ready for F13 native path.
- [ ] **Definition of Done:** F1–F13 native green (or forbidden to claim row);
  evidence committed. Calendar overflow → append extra sprints **without**
  shrinking claim / without slipping compile ARC to the next tag.

### S-D3 — Honest parallel examples (`parallel_work` / `parallel_pi`)

- [ ] **Deliverables:** Examples/docs use typed aggregation; no PRINT-as-API;
  no DELETE-heavy teaching on 0.5.0 surface.
- [ ] **Dependencies:** Sprint 1 (D1) at minimum; prefer after S-D2 for native
  demo paths.
- [ ] **Fixtures / checks:** Example runs or documented harness; Quorra sample
  review.
- [ ] **Gates:** Honesty vs typed AWAIT locks.
- [ ] **Definition of Ready:** D1 contract stable.
- [ ] **Definition of Done:** Examples match DNA; DELETE purged or quarantined.

### S-purge — G6 example / test DELETE migration

- [ ] **Deliverables:** `examples/**` (and tests teaching DELETE) migrated or
  explicitly quarantined for 0.5.0 claim.
- [ ] **Dependencies:** S-M1 surface rules; can run parallel to S-M2+.
- [ ] **Fixtures / checks:** Repo scan for `\bDELETE\b` under claimed 0.5.0
  teaching paths; quarantine folder explicit if legacy kept.
- [ ] **Gates:** Quorra surface purity.
- [ ] **Definition of Ready:** G6 list known.
- [ ] **Definition of Done:** Quarantine empty or legacy folder explicit; claim
  paths DELETE-free.

## Hard acceptance gate (no done-oco)

Do **not** move to `done/`, do not claim “0.5.0 closed”, and do not tag a language-complete 0.5.0 until:

1. **No `DELETE`** in 0.5.0 grammar path + new ARC fixtures (F12). Quorra rejects DELETE return.  
2. **F1–F13** have committed `.bn` (or harness) + evidence under `docs/superpowers/evidence/arc-0.5.0/` for every wave claiming ARC done.  
2b. Same fixtures **green on `bn build` / native** (M4) before 0.5.0 claim — interpret-only is insufficient.  
3. **Typed AWAIT** fixture green on interpret (D1) and on compile (**D2** — in claim; before/with F13 native).  
4. Rust gate green on the committed diff (`fmt` / `test` / `clippy` / `git diff --check`) with pasted evidence.  
5. Working-tree-only checkboxes / untracked `bn_arc` alone ≠ acceptance.

## Tag content (Carlos lock — authoritative)

| In 0.5.0 claim | Out of claim |
| --- | --- |
| M0–M1 docs + **M2** interpret ARC + F1–F13 on `bn run` | Unowned (deferred); HOST-as-ARC |
| **M4** compile/LLVM ARC + F1–F13 on `bn build` (same observables) | Done-oco / locks-only / D1-only / **interpret-only ARC** |
| **D0–D1** typed AWAIT interpret + **D2** native typed AWAIT (F13 native mandatory) | — |
| Book/migration alignment | — |

**Tron:** prior Option A and any compile-ARC-out-of-minimum / next-tag slip are **superseded** and must not reappear as scope cuts. Parallelize D1 with M2; **D2 before or with M4 F13 native**; M4 after M2 reference (or overlap carefully). **Do not claim 0.5.0** until **M2, M4, D1, and D2** green. If calendar slips, **append extra sprints** — overflow only, never reopen an out-of-minimum cut.

## Related out-of-band (not this bucket’s success claim)

- BNString / TOLOWER / TOUPPER already on main — extras, not ARC lifetime.
- Binaries CI green (`dc6e820`) — release engineering, not 0.5.0 DNA done.


## Extra sprints (if needed)

Append at the **end** of this bucket when the first cycle cannot finish compile ARC / native typed AWAIT.
**Calendar overflow only** — never reopen an out-of-minimum cut, slip compile ARC to the next tag, or drop D2 from the claim.

| Sprint (suggested) | Focus | Done when | Ordering note |
| --- | --- | --- | --- |
| S-D2 (overflow slice) | Native typed AWAIT remaining matrix | F13 + multi-T + replay/errors on `bn build` | **Before/with** M4 F13 native — not after a deferred-D2 deferral |
| S-M4a | LLVM retain/release design + emission for class strong/weak | Design note + failing→passing subset of F1–F3 on `bn build` | After M2 reference |
| S-M4b | Full F1–F13 native green (deferred rows **forbidden** for in-scope items) | Evidence NOTES updated with compile commands | Needs S-D2 for F13 native |
| S-D3 | Honest `parallel_*` examples | Examples match typed AWAIT DNA | After D1; prefer after D2 |
| S-purge | Finish G6 example migration off `DELETE` | Quarantine empty or legacy folder explicit | Parallel OK |

Do not use extra sprints to shrink the Carlos claim — only to schedule overflow work **without** closing early.

## History

- 2026-09-12 — Spec locks + `docs/0.5.0/` + proposals (Quorra/Carlos).  
- 2026-09-12 — Tron creates this bucket: cross-check gaps; executable waves; tag recommendation.
- 2026-09-12 — Carlos (via Quorra): tag 0.5.0 **ARC all-or-nothing** (M2+F1–F13+D0–D1); Tron Option A superseded; residual reco section removed.
- 2026-09-12 — Carlos (via Quorra): **fixtures first** — F1–F13 + F13 typed AWAIT committed under `docs/superpowers/evidence/arc-0.5.0/`; G6 quarantine list for DELETE examples; G5 paths landed (red expected).

## Execution baseline — 2026-09-12

### Commands and scope

`cargo build --bin bn` completed successfully (exit 0). Each existing fixture
was then executed using the newly built binary:

```bash
target/debug/bn run docs/superpowers/evidence/arc-0.5.0/F1/program.bn
```

The same command was run individually for F2 through F13. These observations
describe the existing working tree, including pre-existing local changes;
they are not acceptance evidence for a committed implementation. No source
implementation was changed for this baseline. Fixture sources and notes are
already tracked in commit `98af828`.

### Observations

| Fixture | Exit | Actual observation | Acceptance implication |
| --- | --- | --- | --- |
| F1 | 0 | `PASS F1 strong alias n= 7` | Existing alias mutation works; does not establish ARC lifetime. |
| F2 | 0 | `IN_SCOPE`, `AFTER_SCOPE`; no `DEINIT` | Fails required scope destruction order. |
| F3 | 0 | `HELD  2`; no `DEINIT` | Fails required destruction on reassignment. |
| F4 | 1 | `UNKNOWN_TYPE`: `WEAK` is not declared/imported | Weak references unavailable. |
| F5 | 1 | `E0100` at `RELEASE c` | Syntax failure does not prove use-after-release checking. |
| F6 | 1 | `E0100` at `RELEASE n` | Syntax failure does not prove scalar binding lifetime checking. |
| F7 | 1 | `E0100` at `RELEASE w` | Aggregate release unavailable. |
| F8 | 1 | `E0100` at `RELEASE boxes` | Vector release unavailable. |
| F9 | 1 | `E0100` at `RELEASE boxes[0]` | Rejected before RELEASE support; does not establish element-specific validation. |
| F10 | 1 | `E0100` at `RELEASE a` | Cannot check survival of the other strong alias yet. |
| F11 | 1 | `E0100`: expected punctuation at `AWAIT tickets[i](60000)` | Await parser currently accepts a primary receiver without this index suffix. |
| F12 | 0 | `LIVE`; no `DEINIT` | Runtime lifetime observation fails; grammar purge is a separate check. |
| F13 | 1 | `ASYNC_RETURN_TYPE`: ASYNC FUNCTION must return VOID OR Error | First D0–D1 regression target. |

F2 and F12 also emitted `UNUSED_BINDING` warnings. No fixture was marked done
from its exit status alone. F12 NOTES are split: (a) no `DELETE` in
`program.bn` only; (b) `bn run` scope/lifetime test; (c) NOTES prose may mention
DELETE. Do not use a broken `rg && bn run` pipeline (zero-match `rg` exits 1).

### Implementation map for sprint 1

- `modules/bn/BNDispatch.bn`: `Queue.Async` currently accepts a zero-argument
  VOID/Error function; `Ticket.Wait` returns VOID/Error.
- `crates/bn_frontend/src/parser/expressions.rs`: keyword submission lowers to
  an ordinary `Async` call; await becomes `Wait`; indexed await needs parsing.
- `crates/bn_frontend/src/semantic/analyzer1.rs`: rejects typed ASYNC returns.
- `crates/bn_frontend/src/semantic/analyzer6.rs`: call checking currently uses
  the ordinary module signature, including submission arity.
- `crates/bn_frontend/src/lowering/builder/expressions.rs`: already emits
  `DispatchSubmit` and `DispatchAwait`.
- `crates/bn_ir/src/validate.rs`: dispatch-specific checking must accompany
  any new operand/result contracts, independently of target support.
- `src/runtime/executor/part2.rs`: dispatch IR operations route through calls.
- `src/runtime/executor/part3.rs`: submission requires queue plus function,
  renames the worker to Start, and records completion; wait discards payload.
- `src/dispatch.rs`: interpreter queue/ticket implementation. The existing
  `bn_rt` ABI result pointer does not by itself connect this execution path.
- `tests/runtime.rs`, `tests/ir.rs`, `tests/validated_ir.rs`: existing dispatch
  regressions and validation fixtures to extend alongside F13.

### Decision residual (does not freeze Sprint 1 / M1 / M2)

**D1-TYPE** is delimited to opaque tickets without static worker `T` only.
Sprint 1 proceeds for known `T` (F13 INTEGER). No generic syntax; no default
VOID. M1/M2 are not blocked.

- 2026-09-12 — Carlos (via Quorra): **amplify** — ARC required on interpret **and** compile (M4 in claim); extra sprints = calendar overflow only.
- 2026-09-12 — Carlos (via Quorra) review: complete sprints; D2 in claim before M4 F13 native; D1-TYPE narrowed; G9/G10 closed; F12 NOTES split.
- 2026-09-12 — Carlos review (4 localized): M4 no deferred in arc-conformance; ARC criteria off Sprint1/S-D2; S-M2 close-after D1 for F11/F13; F12 DELETE gate exit-code strict + tree+grammar scan.
