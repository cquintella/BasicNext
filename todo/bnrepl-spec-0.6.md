# bnr — interactive Basic Next session specification proposal

Date: 2026-09-22. Target: Basic Next **0.6.0** toolchain; delivery bucket **0.6.9** (deferred).
Status: **Decision 0.1 accepted** (Carlos 2026-09-22); remaining BNR-0 locks open; execution deferred to [`todo/bucket-0.6.9.md`](bucket-0.6.9.md).
Internal planning document; do not force-add to Git or link from public documents.
Companions: [library-first drivers](../docs/architecture/library-first-drivers.md), [FrontendSession](../docs/architecture/frontend-session.md), [completion gates](../docs/architecture/completion-gates.md), [bucket 0.6.0](../ongoing/bucket-0.6.0.md).
Authoritative language surface: `language/0.6/`.

## Purpose

`bnr` is the **interactive, multi-submission persistent session** for Basic Next.

It is a distinct product from:

- `bni eval` — one-shot fragment evaluation (no session state across process invocations);
- `bnc` — Clang-like **compile only** (no interpret);
- notebook cells as documented today — still framed as complete programs with `FUNCTION Start()`, no top-level statements, no state between cells (and still naming host `bn` in places: **documentation debt**, not `bnr` behavior).

Bucket **0.6.9** will deliver escopo/recorte on top of claimed **0.6.0** libraries. Prefer precise version and recorte language; avoid “MVP” wording.

## Decision 0.1 (accepted — Carlos, 2026-09-22)

Interactive strategy vs `FUNCTION Start()`:

1. **Batch / notebook / `bni run` / `bnc`:** `FUNCTION Start()` remains the sole program entrypoint. Unchanged.
2. **`bnr`:** a live **session scope** holds bindings, heap, and code generations across submissions. The session controller executes each accepted submission; prior submissions are **not** replayed.
3. **Synthetic `Start`:** permitted **once at session open** only as an internal pipeline adaptation if a named entrypoint is still required by the interpret path. It must not be rebuilt per turn and must not re-execute history.
4. **Forbidden:** per-turn `Start` wrappers; accumulating history into a `Start` body and re-running it; removing batch `Start` to simplify the REPL.
5. **User-declared `Start` in interactive context:** ordinary function; **not** auto-invoked.
6. Grammar/semantics/fixtures under `language/0.6/` must still be updated before implementation claims interactive-language conformance.
7. Delivery of the executable work is **deferred to bucket 0.6.9**.

## Scope (0.6.9 on 0.6.0 libs)

### In scope

- Persistent interactive session: successive submissions, shared bindings/declarations until `:reset` or process exit.
- Library-first consumption of **0.6.0** crates already claimed: `bn_cli`, `bn_frontend` (including `prepare`), `bn_interpret_driver`, `bn_interp`, plus supporting `bn_ir`, `bn_diag`, `bn_source`, `bn_types`.
- Headless session engine (testable without TTY) plus a TTY front-end binary.
- Policy vocabulary aligned with `bni` / `bn_cli`; single `bn_rt::Policy`; `HostEnv` built by the interpretation driver; document effective policy at startup like `bni`.
- Meta-commands: `:help`, `:quit`, `:reset`, `:bindings`, `:type`.
- Acceptance gates **BNR-0 .. BNR-7** and the session state machine below.

### Out of scope (this bucket)

- LLVM JIT / any link to `bn_llvm` or `bn_compile_driver`.
- Silent hot reload of changed imported module files (this bucket: `:reset` required).
- Network REPL server.
- Notebook UI.
- Undo of HOST effects already performed.
- BNCrypto, BNSqlite.
- Replacing `bni eval`.
- Restoring removed binary name `bn` (D-060-01 revised 2026-09-22: binaries are `bni` and `bnc` only).

### Toolchain facts (authoritative, do not rediscover)

- Checkout context: `main` ahead of origin; version **0.6.0**; binaries **`bni`** and **`bnc`** only.
- Crates present: `bn_cli`, `bn_interpret_driver`, `bn_compile_driver`, `bn_lsp`, `bn_dap`, `bn_frontend`, `bn_interp`, `bn_ir`, `bn_diag`, `bn_source`, `bn_types`, `bni`, `bnc`. **No `bnr` crate yet.**
- `bni --help`: commands `eval|check|lex|run|lsp|dap`; shared opts include `--module-path`, `--no-filesystem`, `--sandbox`, `--read-root`, `--write-root`, warning policy, `--config`, `-V`, `-h`.
- `bnc`: compile only; no interpret.
- Companions in force: `docs/architecture/library-first-drivers.md`, `frontend-session.md`, `completion-gates.md`, `ongoing/bucket-0.6.0.md`. Prior 0.5 architecture-review companion is gone (historical note only).

## User interaction

- Line-oriented TTY with line editing (crate choice is a **BNR-0** lock; do not invent a version in this proposal).
- Prompt collects text until the frontend decides the submission is Complete, Incomplete, or Invalid (span-bearing; not delimiter counting alone).
- Meta-commands (non-language): `:help`, `:quit`, `:reset`, `:bindings`, `:type`.
- Ctrl-C while **Collecting**: discard partial buffer → Ready.
- Ctrl-C while **Executing**: abort current submission; return Ready or Invalidated per lifecycle rules (no silent half-publish of bindings).
- Startup: print version, brief help pointer, and **effective policy** (same spirit as `bni`).

Shared option vocabulary with `bni` where applicable (`--module-path`, `--no-filesystem`, `--sandbox`, `--read-root`, `--write-root`, warning policy, `--config`, `-V`, `-h`). `bnr` must not silently broaden the policy ceiling.

## Submission grammar

A **submission** is one analyze-then-(maybe)-execute unit.

1. **Meta-command** — leading `:` forms listed above.
2. **Language fragment** — source classified Complete / Incomplete / Invalid with spans; Incomplete continues collection.
3. **Proposed interactive context** — top-level session `LET` / `CONST`, assignment, expression display, persistent function/object declarations, imports.

### Authority note (BNR-0)

**Strategy accepted (Decision 0.1).** Current published 0.6 notebook/host rules still describe Start-only cells; interactive top-level LET/expressions are the accepted `bnr` context but are **not yet** reflected in grammar/semantics/fixtures. Those docs must be updated before conformance claims. Remaining open locks: 0.2–0.5.

## Pipeline (real crates)

Library-first drivers: executables own **no** reusable logic.

1. **I/O** — `bnr` binary (TTY) or headless caller owns prompts/byte streams only.
2. **Frontend** — `bn_frontend` / FrontendSession: completeness, `SubmissionId` / `SourceId` / `Revision`, diagnostics via `bn_diag` + `bn_source`.
3. **Prepare** — `bn_frontend::prepare`: lower + language validate + interpret-support check **before** any execution.
4. **Interpret** — `bn_interpret_driver` builds `HostEnv` under one `bn_rt::Policy`; execute with `bn_interp`.
5. **CLI/policy parsing** — reuse `bn_cli` patterns; document effective policy at startup.

**MUST** consume: `bn_cli` + `bn_frontend` (+ prepare) + `bn_interpret_driver` / `bn_interp`.
**MUST NOT** link: `bn_llvm` / `bn_compile_driver`.
Gate: `cargo tree -p bnr` excludes `bn_llvm`.

No LLVM JIT. No network REPL.

## Persistent state

| Kind | Behavior in 0.6.9 |
|---|---|
| Session scalars | `LET` / `CONST` and assignment persist across submissions; bare expressions display without inventing bindings. |
| Functions / objects | Declarations persist; no redeclare without `:reset`. |
| Imports | Module init once per session; **changed imported files require `:reset`** — no silent hot reload in this bucket. |
| Capture | Session `LET` is **non-capturing** w.r.t. function bodies (BNR-0 lock): functions do not close over later session mutations unless language authority revises this after acceptance. |
| Failure | Failed analysis → state untouched. Failed init → binding not published. |
| Reset | `:reset` disposes session env → empty Ready; does not undo external HOST effects already performed. |

## Errors

- Analysis/validation: diagnostics with spans; state unchanged; back to Ready (Incomplete stays Collecting).
- Execution: report error; do not publish bindings from the failed init.
- Policy: same classes as `bni`; never broaden silently.
- Interrupt: distinct messages for collection abort vs execution abort.

## Resource / policy

- Single `bn_rt::Policy`; interpretation driver builds `HostEnv`.
- `bnr` must not silently broaden the ceiling; startup documents effective policy like `bni`.
- Numeric limits (bytes, preview length, history depth, recursion/depth, deadlines) are **BNR-0** locks — no invented numbers in code before authority.
- Sandbox / roots / module-path follow `bni` / `bn_cli` vocabulary.

## Exit / diagnostics

- `:quit` / EOF: dispose session; exit 0 on clean shutdown.
- Fatal startup (bad config/policy): non-zero exit aligned with `bni` conventions.
- When shipping: man page `docs/man/bnr.1`; `--help` / `--version`; install and release assets **only if** Carlos includes `bnr` in 0.6.9 assets (pattern like `bni`/`bnc`).

## Session state machine

```
Ready → Collecting → Analyzing → Prepared → Executing → Ready
                                              ↘ Invalidated
Invalidated → (:reset | process exit) → Ready
```

- **Ready** — accept input or meta-command.
- **Collecting** — incomplete submission; Ctrl-C discards buffer → Ready.
- **Analyzing** — Complete submission; validate/prepare; failure → Ready, state untouched.
- **Prepared** — validated artifact; **no execution before this**.
- **Executing** — interpret; success → Ready; severe failure/interrupt may → Invalidated.
- **Invalidated** — refuse further language submissions until `:reset`.

## Acceptance gates (BNR-0 .. BNR-7)

| Gate | Substance | Blocks |
|---|---|---|
| **BNR-0** | Interactive scope acceptance (top-level LET/expr vs Start-only notebook rule); non-capturing functions w.r.t. session LET; numeric resource limits; packaging (`crates/bnr` vs `bni` subcommand; headless library name `bn_repl` or module under `bn_interpret_driver` — Carlos chooses); line-editing crate choice (do not invent version). | All implementation |
| **BNR-1** | Completeness Complete/Incomplete/Invalid + spans; SubmissionId/SourceId/Revision; headless `analyze_submission` API; unit tests on comments/strings/nested blocks (GC-FE oriented). | Execute path |
| **BNR-2** | Lower + language validate + interpret support check per submission; stable symbols; no exec before validation. | Persistence slice |
| **BNR-3** | Scalar persistence min: session LET/CONST + assignment + expression display; failed analysis untouched; failed init not published; headless transcript fixture matching the proposal example. | Functions/imports stretch |
| **BNR-4** | Lifecycle / policy / interrupt: Ctrl-C collection vs execution; reset/quit dispose; policy ceiling; HostEnv via `bn_interpret_driver`. | Delivery |
| **BNR-5** | Modularity: headless engine tests without TTY; `cargo tree -p bnr` excludes `bn_llvm`. | Release claim |
| **BNR-6** | Man page `docs/man/bnr.1` when shipping; `--help`/`--version`; install/assets only if Carlos includes the binary. | Close |
| **BNR-7** | Conformance transcript vs batch where overlap; soak bounds; release note `docs/releases/0.6.9.md`; move bucket to `done/`. | Done |

### Workspace quality gates (0.6.0 discipline)

Replace any legacy “existing Rust fmt/test” wording with workspace gates as used for **0.6.0**: `scripts/test-battery.sh`, `clippy -D warnings`, and the same battery/clippy bar that claimed 0.6.0. Run focused tests at the end of each sprint slice; full battery when closing the bucket. Activities are sequential. Quorra before Carlos at the bucket gate.

## Decisions still requiring authority

1. **Interactive scope (BNR-0)** — **accepted** as Decision 0.1 (session scope; synthetic Start at open only). Still required: update `language/0.6/` grammar/semantics/fixtures before conformance.
2. **Non-capturing session LET** — confirm for function bodies vs session bindings.
3. **Numeric resource limits** — concrete bytes / preview / history / depth / deadlines.
4. **Packaging** — prefer `crates/bnr` binary + optional `bn_repl` (or module under `bn_interpret_driver`) for headless API — **Carlos chooses**; alternative is a `bni` subcommand (not preferred).
5. **Line-editing crate** — choose an existing crate/version; do not invent.
6. **Asset inclusion** — whether 0.6.9 ships `bnr` beside `bni`/`bnc` (man page + assets pattern).
7. **Schedule** — execution is **0.6.9** (deferred); does not block 0.6.1–0.6.4 unless Carlos reprioritizes.

## Doc debt (not behavior)

- `language/0.6/` is mostly a 0.5 copy; `++`/`--` expressions (C convention) were added; notebook section still says cell = complete program with `FUNCTION Start()`, no top-level statements, no state between cells, and still names host `bn` — **doc debt**, not `bnr` semantics.
- Prior 0.5 architecture-review companion is gone; use companions in the header only (historical note).

## Relation to `bni eval`

| | `bni eval` | `bnr` |
|---|---|---|
| Lifetime | One-shot fragment | Multi-submission persistent session |
| State | None across invocations | Bindings/decls until `:reset` / exit |
| Product | Existing **0.6.0** | Deferred delivery **0.6.9** |

Distinct products; this bucket does not replace `bni eval`.

## Example transcript (headless acceptance sketch)

```
$ bnr
bnr 0.6.9 (policy: … effective ceiling documented …)
> LET x = 1
> x
1
> LET x = x + 1
> x
2
> :bindings
x : …
> :reset
> x
<error: unbound …>
> :quit
```

Failed analysis of a later submission must leave prior bindings intact; failed initialization of a new binding must not publish it. Transcript fixtures for BNR-3 should match this spirit.

## Out-of-scope detail (0.6.9)

The following are explicit non-goals for this delivery bucket. They may be revisited in later buckets under new authority; they must not creep into 0.6.9 implementation or tests.

1. **LLVM JIT** — no `bn_llvm`, no `bn_compile_driver`, no in-process native codegen for submissions.
2. **Hot reload** — changed imported module files require `:reset`; no silent re-init.
3. **Network REPL** — no TCP/WebSocket server front-end in this bucket.
4. **Notebook UI** — no cell UI; TTY + headless only.
5. **HOST effect undo** — `:reset` clears session env only; external side effects remain.
6. **BNCrypto / BNSqlite** — tracked under other version lanes (`0.6.1`, later).
7. **Replacing `bni eval`** — one-shot eval remains; `bnr` is the persistent session product.

## Packaging preference (open)

Preferred shape pending Carlos (BNR-0):

- Binary crate `crates/bnr` for the TTY executable.
- Optional headless library `bn_repl` **or** a module under `bn_interpret_driver` exposing the session engine without TTY.

Alternative (not preferred): `bni` subcommand. Decision is authority-gated; do not ship both shapes in 0.6.9 without an explicit call.
