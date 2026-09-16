# Proposal: `bn eval` mode (tooling surface for 0.5.1)

## Planning review addendum (2026-09-13)

Execution detail and acceptance are now in [bucket 0.5.1, SECTION 3](../../ongoing/bucket-0.5.1.md#section-3--t1-bn-eval-and-repl-infrastructure)
and [WBS 0.5.1](../../ongoing/WBS-0.5.1.md). The older options and JSON examples
below are design history where they differ from that concrete proposed contract;
neither version turns provisional choices into Carlos approval.

D-T1-01 is resolved by Carlos (2026-09-13), option A: 0.5.1 is oneshot only.
Each invocation evaluates one source and exits; `--session` is rejected. S0 and
S1 require a future proposal. T10 records the actual oneshot latency baseline.
D-T1-02 is resolved by Carlos (2026-09-13), option A: a syntactically recognized
top-level `FUNCTION Start` auto-promotes to program semantics with exactly one
structured warning under normal warning policy. Strings/comments do not trigger
it; explicit program mode emits no promotion warning. D-T1-03 is resolved by
Carlos (2026-09-13), option A: one JSON v1 envelope on process stdout, structured
`diagnostics[]`, captured program streams and empty process stderr after format
recognition. D-T1-04 is resolved by Carlos, option A: source comes from
`bn eval SOURCE` or `bn eval --stdin`, exactly one form. There is
no eval file option or path inference in 0.5.1; files remain under `bn run`.

Required tests are T01–T10 in the bucket, with T09 explicitly N/A for 0.5.1.
These supersede the eight-row smoke minimum:
they include source mapping through wrapping/imports, complete expression parse,
single evaluation of effects, validation barriers, policy, JSON failures,
runtime stdin ownership and isolated installation. Structured diagnostics share
DX1's model; no text parsing to reconstruct diagnostics for JSON.
Updating an external bnr repository is a separately authorized handoff.

---

**Status:** Proposed — **CLI / toolchain contract** (not a new language keyword).
**Date:** 2026-09-12
**Target release:** **0.5.1** (after 0.5.0 ARC + typed `AWAIT` claim).
**Owner (tracker):** Tron until Carlos names implementer.
**Gate:** Quorra before Carlos.
**Motivation:** The external **Basic Next REPL** (`bnr`; local companion under `~/src/Basic Next REPL`) (and similar hosts) must drive the **installed** `/usr/local/bin/bn` without rewriting Basic Next. Today `bn` is file-oriented (`bn run <file.bn>`). A documented **`bn eval`** mode lets a REPL (or scripts) submit source fragments and receive structured results without vendoring the language.

**Related (do not conflate):**
- Jupyter `bn-kernel` — already writes a temp `.bn` and calls `bn run --no-filesystem --jupyter-stdin`; **stateless between cells** ([`docs/project/kernel.md`](../../docs/project/kernel.md)).
- DAP stepping — IR step, **not** expression eval ([`docs/project/usage.md`](../../docs/project/usage.md)).
- 0.5.0 bucket — ARC + typed dispatch; **eval is explicitly 0.5.1**, not in the 0.5.0 claim.
- REPL architecture lock — thin shell around `/usr/local/bin/bn`; no `path` dependency on `~/src/BasicNext`.

Nothing here is normative until accepted into `docs/project/usage.md`, `docs/man/bn.1`, and implemented behind the **`bn eval`** subcommand.

---

## Problem

| Layer | Today |
| --- | --- |
| CLI | `bn check\|lex\|run\|build\|lsp\|dap` require a **file path** |
| Semantics | Executable programs need `FUNCTION Start()` |
| Hosts | REPL / notebooks invent temp files; no stable machine-readable result protocol for “what did this fragment mean?” |
| Session | No supported way to keep **bindings across** successive evals in one `bn` process |

So interactive and embedding hosts either wrap files ad hoc (Jupyter) or cannot offer honest multi-line session state without forking language semantics outside `bn`.

---

## Goals

1. Add a first-class **`bn eval`** subcommand that accepts Basic Next source from
   an argv string or stdin. Files remain the responsibility of `bn run`.
2. Define **fragment kinds** and how they map to existing frontend + interpret (reference) — prefer reuse of `check`/`run` pipelines over a second semantic engine.
3. Define a **stable result envelope** (stdout and/or exit codes) suitable for a REPL subprocess: success value presentation, diagnostics, exit status.
4. Ship an explicit **oneshot** lifecycle; reserve persistent sessions for a later proposal.
5. Keep interpret as reference for eval; native/`bn build` eval is **out of 0.5.1 MVP** unless trivially free.
6. Document how `bnr` must call `/usr/local/bin/bn eval` only — no source tree required.

---

## Non-goals (0.5.1)

- Rewriting or vendoring frontend/runtime into the REPL repo.
- A language keyword `EVAL` / `EXECUTE` inside Basic Next programs (different proposal if ever needed).
- Full Jupyter replacement (kernel may adopt `bn eval` later; not required to ship 0.5.1).
- Persistent process state across **separate** OS processes without an explicit session protocol.
- Compiling each fragment with LLVM (`bn build`) as the eval backend.
- Guaranteeing ARC teaching fixtures via eval (0.5.0 owns ARC; eval must not weaken ARC rules).
- Interactive line-editing / prompt UI inside `bn` (belongs to the REPL process).

---

## Proposed CLI surface

### Entry form (Carlos lock 2026-09-14 — reverts 2026-09-13 `-e`)

**Locked:** eval is the **`bn eval`** subcommand. Do **not** document or ship `-e` / `--expr` as the 0.5.1 entry.

```text
bn eval '<source>' [options]
bn eval --stdin [options]
```

| Form | Meaning |
| --- | --- |
| `bn eval` *source* | Evaluate the given source string (shell-quoted by the caller) — **primary MVP** |
| `bn eval --stdin` | Read entire stdin as the eval source (UTF-8) |

The `eval` subcommand selects the eval pipeline (snippet/program wrapping, result envelope). Other top-level commands (`run`, `check`, `build`, …) remain unchanged and still take a file path; in particular, `bn run` and `bn build` continue to exist.

### Options (MVP)

| Option | Default | Intent |
| --- | --- | --- |
| `--mode snippet\|program` | `snippet` | See fragment kinds |
| `--format text\|json` | `text` | Result envelope |
| `--session` | unavailable | Rejected in 0.5.1 by D-T1-01; reserved for a future proposal |
| Existing policy flags | as `bn run` | `--no-filesystem`, `--sandbox`, `--read-root`, `--write-root`, warning controls |

Reuse warning/`--color` flags from the global CLI where applicable.

---

## Fragment kinds

### A. `program` mode

Source must be a full program acceptable to `bn run` today (including `FUNCTION Start()`).
Behaviour: equivalent to writing the bytes to a temp file and running the interpret path of `bn run`, then deleting the temp file.
**Purpose:** compatibility with Jupyter-style “whole cell is a program” and migration from ad hoc temp files.

### B. `snippet` mode (default) — MVP rules

A **snippet** is one of:

1. **Declarations + statements** that are legal inside `FUNCTION Start() AS VOID` (and, if needed, a synthetic wrapper).
2. A single **expression** that yields a printable value of MVP type:
   `INTEGER` \| `FLOAT` \| `BOOLEAN` \| `STRING` (extend only with explicit acceptance).

**Wrapper (normative intent):**

- If the snippet parses as an **expression**, `bn eval` wraps as:

  ```basic
  FUNCTION Start() AS VOID
      PRINT <expression>
  END FUNCTION
  ```

  (Exact IR may bind then print; observable is the printed representation + exit 0 on success.)

- If the snippet is **statements** (possibly with `LET`/`CONST`), wrap as:

  ```basic
  FUNCTION Start() AS VOID
      <statements>
  END FUNCTION
  ```

- If the parser recognizes a top-level `FUNCTION Start`, auto-promote to
  **program** semantics and emit exactly one stable structured warning. Text in
  strings/comments does not count. Explicit `--mode program` emits no promotion
  warning. Normal warning policy applies; promotion to error prevents execution.

**Imports:** `IMPORT` lines allowed at the top of a snippet before statements; they hoist outside `Start` as in a normal program.

**Classes / multi-function:** allowed in snippet only if the fragment is a valid full program (same as program mode) — do not invent partial-class eval in 0.5.1.

---

## Result envelope

### Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Success (runtime completed without unrecovered Error / trap) |
| `1` | Language / toolchain failure (parse, semantic, runtime Error surfaced as process failure — match `bn run` policy) |
| `2` | Usage / invalid options / missing source form |

### `--format text` (default)

- Diagnostics on stderr (same style as `bn run`).
- Program `PRINT` / console output on stdout.
- No extra banners required.

### `--format json` (MVP)

One JSON object on stdout (UTF-8, single line preferred for oneshot):

```json
{
  "ok": true,
  "stdout": "...",
  "stderr": "...",
  "exit_code": 0
}
```

The older alternative of mirroring diagnostics to process stderr is superseded
by D-T1-03. Failure uses `"ok": false` and structured `diagnostics[]` in the
same v1 envelope; process stderr remains empty after JSON mode recognition.

**Locked by D-T1-03 (Carlos 2026-09-13):** emit one JSON v1 object plus newline
on process stdout. Process stderr remains empty after JSON mode recognition.
Program stdout/stderr are captured in their envelope fields. Diagnostics appear
only as structured `diagnostics[]`; no human prose, ANSI or logs may escape the
envelope. Usage/config failures use the envelope once JSON mode is recognized.

---

## Session mode (`bn eval --session`) — deferred from 0.5.1

**Purpose:** allow a REPL to keep **one** `bn` process and send many fragments without cold-start each time; optionally retain bindings.

### Protocol (JSON Lines on stdin)

Each request line:

```json
{"id":"1","source":"LET x AS INTEGER = 1","mode":"snippet"}
```

Each reply line:

```json
{"id":"1","ok":true,"stdout":"","stderr":"","exit_code":0}
```

### State policy (must lock one)

| Option | Behaviour |
| --- | --- |
| **S0 — Stateless session (future option)** | Each request is a fresh program environment. It may amortize process initialization; eval has no JIT to amortize. |
| **S1 — Accumulating session** | Top-level bindings from successful snippets remain visible to later snippets in the same process. |

**0.5.1 decision (Carlos 2026-09-13):** ship neither S0 nor S1. The CLI rejects
`--session`. Oneshot is the only lifecycle in this release. Measure its latency
before deciding whether a later stateless protocol is worthwhile. The protocol
below remains future design material, not shipped 0.5.1 behavior.

If S1 is pulled into 0.5.1 by Carlos, acceptance must include ARC fixtures for bindings that outlive a snippet (no use-after-release; weak/NULL rules unchanged).

---

## Security and HOST defaults

- Default filesystem policy for `bn eval` should match **`bn run`** unless flags say otherwise.
- REPL hosts that are untrusted-input facing should pass `--no-filesystem` (and sandbox roots) explicitly — document as host responsibility.
- No new ambient authority beyond `bn run`.

---

## Implementation sketch (non-prescriptive)

1. CLI parse for `bn eval` (+ optional `--stdin` / `--format`); reject `--session`.
2. Materialize source string → existing frontend (`check` path) → interpret `Start` (reuse `bn run` engine).
3. Snippet parse probe: try expression vs statements; wrap; on failure emit actionable diagnostic.
4. JSON envelope writer.
5. Record the oneshot latency baseline for future session design.
6. Man page + `docs/project/usage.md` + fixtures under `docs/superpowers/evidence/bn-eval-0.5.1/`.

---

## Acceptance (0.5.1 done when)

1. `bn eval 'PRINT 1+1'` exits 0 and prints `2` (or equivalent PRINT formatting).
2. `bn eval 'LET x AS INTEGER = 3\nPRINT x'` works in snippet mode.
3. `bn eval 'PRINT "hi"' --format json` yields parseable JSON with `ok: true`.
4. Invalid snippet → non-zero exit + diagnostic; JSON form has `ok: false`.
5. `bn eval` without a source string (and without `--stdin`) → usage, exit 2.
6. Evidence directory with NOTES + commands; Quorra gate.
7. Prepare the external `bnr` handoff text and command example using an explicit
   installed `bn eval` path. Updating that repository requires separate authorization.
8. Man page `bn.1` documents the `bn eval` subcommand.

---

## Out of 0.5.1 / later

| Item | Notes |
| --- | --- |
| S1 accumulating session | Needs ARC session story |
| Expression result without PRINT wrapper | Nice-to-have if IR can return a value to the host |
| `bn eval` → LLVM | Not needed for REPL |
| Language-level `EVAL` keyword | Separate DNA proposal |

---

## Open questions (Carlos)

1. ~~Session lifecycle~~ — **LOCKED (Carlos 2026-09-13):** option A, oneshot only; reject `--session`; S0/S1 deferred.
2. ~~Command shape~~ — **LOCKED (Carlos 2026-09-14):** **`bn eval`** subcommand. ~~Prior 2026-09-13 lock of global `-e` / `--expr` is stricken~~ — do not document `-e` as 0.5.1 entry.
3. ~~Start promotion~~ — **LOCKED (Carlos 2026-09-13):** option A; syntactic top-level `Start` auto-promotes with one structured policy-controlled warning.
4. ~~JSON diagnostic channel~~ — **LOCKED (Carlos 2026-09-13):** option A; one JSON v1 envelope on stdout, structured diagnostics, empty process stderr.
5. ~~Source forms~~ — **LOCKED (Carlos 2026-09-14):** option A; `bn eval SOURCE` or `bn eval --stdin`; no eval file mode/path inference; no `-e` / `--expr`.

---

## History

- **2026-09-12** — Drafted for 0.5.1 after REPL scaffold (`~/src/Basic Next REPL`) locked to `/usr/local/bin/bn` without rewriting Basic Next. Carlos: place under `todo/proposals/`.
- **2026-09-13** — ~~Carlos: eval entry is **`bn -e`**, not subcommand `bn eval`.~~ **STRICKEN** — see 2026-09-14 revert.
- **2026-09-14** — Carlos reverteu: subcomando **`bn eval`** (não `bn -e` / `--expr`). Aceites/man/bnr = `bn eval SOURCE` / `bn eval --stdin`. Proibido documentar `-e` como entrada 0.5.1.
- **2026-09-13** — Carlos: D-T1-01 option A — oneshot only; reject `--session`; S0/S1 deferred.
- **2026-09-13** — Carlos: D-T1-02 option A — syntactic top-level `Start` auto-promotes with one structured warning.
- **2026-09-13** — Carlos: D-T1-03 option A — one JSON v1 envelope on stdout; structured diagnostics; empty process stderr.
- **2026-09-13** — Carlos: D-T1-04 option A — source by argv or stdin; files remain `bn run` (entry spelling corrected 2026-09-14 to `bn eval`).
