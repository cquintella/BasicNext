# Basic Next 0.5.0 — Corrective train (ARC DNA + typed dispatch returns)

**Status:** Active executable bucket (Tron audit 2026-09-12). **Tag policy locked by Carlos: ARC all-or-nothing (interpret M2 + F1–F13 + typed AWAIT); not locks/dispatch-only.**  
**Objective:** Make the **locked** 0.5.0 language DNA real: ARC compliance (no `DELETE`), typed `AWAIT → T OR Error`, fixtures F1–F13. Spec locks already exist; this file is the **implementation WBS**.

**Owner (tracker):** Tron until Carlos names implementer.  
**Gate:** Quorra ARC-compliance + no done-oco (fixtures + evidence) before Carlos.


## Carlos lock — tag 0.5.0 content (2026-09-12, via Quorra)

**Policy:** **All or nothing on ARC.** Version 0.5.0 must not ship as locks-only or typed-`AWAIT`-only.

**In the 0.5.0 tag (minimum):**
1. Full **interpret** ARC as executable reference: strong/weak (`AS WEAK`), optional `RELEASE`, **`DELETE` keyword purged** from grammar/FE/fixtures/examples under the 0.5.0 surface.
2. Conformance fixtures **F1–F13** with committed evidence (no empty done).
3. Typed **`AWAIT → T OR Error`** (D0–D1) in the same release train.
4. Book/migration alignment for 0.4.x → 0.5.0 memory model.

**Out of minimum tag unless Carlos expands scope:** M4 LLVM retain/release native parity (may follow as 0.5.x). Unowned remains deferred. HOST stays `Close`/`*_close`.

**Supersedes:** Tron’s earlier recommendation “0.5.0 = locks + D0–D1; M2 → 0.5.1” for *tag content*. Wave *ordering* may still do D1 in parallel with M2, but **release claim 0.5.0 requires M2 green**, not D1 alone.


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
| **M4** | LLVM retain/release + matrix | Deferred OK if interpret-first | **OPEN** |

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
| G5 | F1–F13 `.bn` fixtures missing | **Critical** for M2 close | Checklist only |
| G6 | Examples + tests still teach `DELETE` | High | Blocks “no DELETE in surface” claim |
| G7 | Book ch.7 body not rewritten | Med | Overlay + migration exist |
| G8 | Tag content open (locks+D1 vs +M2) | Med | Carlos open #3 |
| G9 | Replay-until-Close + STRING in MVP T | Low/Med | Proposal open #6 |
| G10 | Unowned confirm deferred | Low | Open #4 |
| G11 | `bn_arc.rs` untracked / not integrated | Med | Commit when M2 starts or drop if unused |
| G12 | Duplicative docs trees `docs/language/0.5/` empty vs `docs/0.5.0/` | Low | Carlos path is `docs/0.5.0/` — keep |

**Wrong / out of scope if claimed done:** treating `bn_arc.rs` or BNString Unicode as ARC lifetime done; treating binaries CI green as 0.5.0 language complete.

## Critical path (executable order)

1. **D0** — Update BNDispatch + language docs already in 0.5.0 (module signatures for typed workers).  
2. **D1** — FE+interpret typed AWAIT (ABI exists). Evidence: parallel sum fixture.  
3. **M1 residual** — Switch active grammar authority to 0.5.0 (or dual-track flag); purge DELETE from FE reserved words.  
4. **M2** — Interpret ARC + RELEASE + WEAK; land F1–F13 with evidence. Quorra gate each.  
5. **M3 / D2 / D3 / M4** — as capacity; M4 may be 0.5.1.

## Hard acceptance gate (no done-oco)

Do **not** move to `done/`, do not claim “0.5.0 closed”, and do not tag a language-complete 0.5.0 until:

1. **No `DELETE`** in 0.5.0 grammar path + new ARC fixtures (F12). Quorra rejects DELETE return.  
2. **F1–F13** have committed `.bn` (or harness) + evidence under `docs/superpowers/evidence/arc-0.5.0/` for every wave claiming ARC done.  
3. **Typed AWAIT** fixture green on interpret if tag includes D1.  
4. Rust gate green on the committed diff (`fmt` / `test` / `clippy` / `git diff --check`) with pasted evidence.  
5. Working-tree-only checkboxes / untracked `bn_arc` alone ≠ acceptance.

## Tag recommendation (Tron → Carlos)

| Option | Contents | When |
| --- | --- | --- |
| **A — Prefer for first tag** | M0+M1 docs + **D0+D1** typed AWAIT interpret + evidence; ARC interpret **Deferred** to 0.5.1 with Owner | Staffing tight; ABI ready; unlocks parallel honesty |
| **B — Prefer if capacity** | A + **M2** interpret ARC + F1–F13 green | Matches proposal “prefer interpret ARC in 0.5.0” |
| **Avoid** | Tag claiming ARC or typed await without fixtures | Done-oco |

**Tron recommendation:** Tag **0.5.0 = Option A** (locks + typed dispatch interpret) **unless** Carlos staffs M2 immediately; then Option B. Do **not** wait for M4 LLVM ARC to tag 0.5.0.

## Related out-of-band (not this bucket’s success claim)

- BNString / TOLOWER / TOUPPER already on main — extras, not ARC lifetime.
- Binaries CI green (`dc6e820`) — release engineering, not 0.5.0 DNA done.

## History

- 2026-09-12 — Spec locks + `docs/0.5.0/` + proposals (Quorra/Carlos).  
- 2026-09-12 — Tron creates this bucket: cross-check gaps; executable waves; tag recommendation.
