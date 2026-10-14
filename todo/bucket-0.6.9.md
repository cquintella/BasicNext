# Bucket 0.6.9 — `bnr` (interactive Basic Next session)

**Status:** Deferred / scheduled in `todo/` (2026-09-22). Depends on **0.6.0** libraries claimed (`bni`, `bn_cli`, `bn_interpret_driver`, `bn_frontend::prepare`). Execution after **0.6.1–0.6.4** unless Carlos reprioritizes.
**Gate:** Quorra before Carlos.
**Tag policy:** version bucket **0.6.9**. Release note `docs/releases/0.6.9.md` only when closing.
**Authoritative proposal:** [`todo/bnr-spec-0.6.md`](../todo/bnr-spec-0.6.md)

**Not in this bucket:** LLVM JIT; hot reload; network REPL server; notebook UI; undo of HOST effects; BNCrypto; BNSqlite; replacing `bni eval`.

## Scope claim

| Claim | Evidence / gate |
| --- | --- |
| Escopo: interactive persistent session `bnr` on 0.6.0 libs | BNR-0 + proposal |
| Recorte: no LLVM / no network REPL / no hot reload | BNR-5 `cargo tree`; proposal non-goals |
| Versão delivery tag **0.6.9** (deferred) | Tag policy; was briefly planned as 0.6.1a |
| Policy ceiling = `bni` / `bn_cli` / `bn_rt::Policy` | BNR-4; startup documents effective policy |
| Headless engine + optional TTY binary | BNR-1 / BNR-5; packaging decision still open |
| Interactive model | **0.1 accepted** (Carlos 2026-09-22) — see below; grammar/fixtures still required before conformance |

## Decision 0.1 (accepted — Carlos, 2026-09-22)

Strategy for interactive vs `FUNCTION Start()`:

1. **Batch / notebook / `bni run` / `bnc`:** `FUNCTION Start()` remains the sole program entrypoint. Unchanged.
2. **`bnr`:** session **scope** holds bindings, heap, and code generations across submissions. The session controller executes each accepted submission; history is **not** replayed.
3. **Synthetic `Start`:** allowed **once at session open** only as an internal pipeline adaptation if the interpret driver still requires a named entrypoint. It must **not** be rebuilt per turn and must **not** re-execute prior submissions.
4. **Forbidden:** wrapping each turn in a new `Start`; accumulating user history into a `Start` body and re-running it; removing `Start` from batch programs to “help” the REPL.
5. **User-declared `Start` in interactive context:** ordinary function declaration; **not** auto-invoked.
6. Before implementation claims language conformance, update `language/0.6/` grammar/semantics/fixtures for the interactive context.

## EAP

### 0 Locks (BNR-0) — blocks code

- [x] **0.1** Interactive strategy (session scope + optional synthetic Start at open only) — **accepted Carlos 2026-09-22**; delivery deferred to **0.6.9**
- [ ] 0.2 Non-capturing functions w.r.t. session LET
- [ ] 0.3 Numeric resource limits (bytes, preview, history, depth, deadlines)
- [ ] 0.4 Packaging: `crates/bnr` vs subcommand of `bni`; headless library name
- [ ] 0.5 Line-editing crate choice (do not invent version)

### 1 Frontend foundation (BNR-1)

- [ ] 1.1 Completeness Complete/Incomplete/Invalid + spans (not delimiter counting)
- [ ] 1.2 SubmissionId / SourceId / Revision
- [ ] 1.3 Headless `analyze_submission` API
- [ ] Acceptance: unit tests on comments/strings/nested blocks; GC-FE oriented

### 2 Validate before execute (BNR-2)

- [ ] 2.1 Lower + language validate + interpret support check per submission
- [ ] 2.2 Stable symbols; no exec before validation

### 3 Scalar persistence slice (BNR-3 min)

- [ ] 3.1 Session LET/CONST + assignment + expression display
- [ ] 3.2 Failed analysis leaves state untouched; failed init does not publish binding
- [ ] Acceptance: headless transcript fixture matching the proposal example

### 4 Functions / objects / imports

- [ ] 4.1 Declarations persist; no redeclare; import init once; reset on changed module files

### 5 Lifecycle / policy / interrupt (BNR-4)

- [ ] 5.1 Ctrl-C collection vs execution; reset/quit dispose; policy ceiling; HostEnv via `bn_interpret_driver`

### 6 Modularity (BNR-5)

- [ ] 6.1 Headless engine tests without TTY; `cargo tree -p bnr` excludes `bn_llvm`

### 7 Delivery (BNR-6/7)

- [ ] 7.1 `docs/man/bnr.1`, `--help`/`--version`, install/release only if Carlos includes binary in assets
- [ ] 7.2 Conformance transcript vs batch where overlap; soak bounds
- [ ] 7.3 Close: release note `docs/releases/0.6.9.md`; move bucket to `done/`

## Testing policy

- End of each sprint slice: focused tests for that EAP item.
- When closing: `scripts/test-battery.sh`, `clippy -D warnings`, same discipline as **0.6.0**.
- Sequential activities; do not skip remaining BNR-0 locks.
- Quorra before Carlos at the bucket gate.

## History

- **2026-09-22** — Carlos: lock interactive strategy (session scope; synthetic Start only at open; no per-turn Start / no replay); **defer execution to 0.6.9**; leave `todo/` (remove brief `ongoing/bucket-0.6.1a.md`).
- **2026-09-22** — Retarget proposal from 0.5/`bn` to **0.6.0** toolchain; briefly opened as ongoing **0.6.1a** with EAP; companions → library-first / FrontendSession / completion-gates / bucket-0.6.0.
- **2026-09-09** — Earlier interactive-session planning under todo targeting 0.5-era `bn`; superseded by `todo/bnr-spec-0.6.md`.
