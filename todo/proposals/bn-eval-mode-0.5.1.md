# Proposal: `bn -e` eval mode (tooling surface for 0.5.1)

**Status:** Proposed — **CLI / toolchain contract** (not a new language keyword).  
**Date:** 2026-09-12  
**Target release:** **0.5.1** (after 0.5.0 ARC + typed `AWAIT` claim).  
**Owner (tracker):** Tron until Carlos names implementer.  
**Gate:** Quorra before Carlos.  
**Motivation:** The external **Basic Next REPL** (`bnr`; local companion under `~/src/Basic Next REPL`) (and similar hosts) must drive the **installed** `/usr/local/bin/bn` without rewriting Basic Next. Today `bn` is file-oriented (`bn run <file.bn>`). A documented **`-e` / eval** mode lets a REPL (or scripts) submit source fragments and receive structured results without vendoring the language.

**Related (do not conflate):**
- Jupyter `bn-kernel` — already writes a temp `.bn` and calls `bn run --no-filesystem --jupyter-stdin`; **stateless between cells** ([`docs/project/kernel.md`](../../docs/project/kernel.md)).
- DAP stepping — IR step, **not** expression eval ([`docs/project/usage.md`](../../docs/project/usage.md)).
- 0.5.0 bucket — ARC + typed dispatch; **eval is explicitly 0.5.1**, not in the 0.5.0 claim.
- REPL architecture lock — thin shell around `/usr/local/bin/bn`; no `path` dependency on `~/src/BasicNext`.

Nothing here is normative until accepted into `docs/project/usage.md`, `docs/man/bn.1`, and implemented behind **`bn -e`** (global flag / eval entry; not a separate `bn eval` subcommand).

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

1. Add a first-class **`bn -e`** eval entry (name locked below) that accepts Basic Next source from **argv string**, **stdin**, or **file**, without requiring the caller to invent an undocumented wrapper forever.
2. Define **fragment kinds** and how they map to existing frontend + interpret (reference) — prefer reuse of `check`/`run` pipelines over a second semantic engine.
3. Define a **stable result envelope** (stdout and/or exit codes) suitable for a REPL subprocess: success value presentation, diagnostics, exit status.
4. Specify an optional **session** mode (multi-eval process) vs **oneshot** (default), with clear HOST/policy defaults.
5. Keep interpret as reference for eval; native/`bn build` eval is **out of 0.5.1 MVP** unless trivially free.
6. Document how `bnr` must call `/usr/local/bin/bn -e` only — no source tree required.

---

## Non-goals (0.5.1)

- Rewriting or vendoring frontend/runtime into the REPL repo.
- A language keyword `EVAL` / `EXECUTE` inside Basic Next programs (different proposal if ever needed).
- Full Jupyter replacement (kernel may adopt `bn -e` later; not required to ship 0.5.1).
- Persistent process state across **separate** OS processes without an explicit session protocol.
- Compiling each fragment with LLVM (`bn build`) as the eval backend.
- Guaranteeing ARC teaching fixtures via eval (0.5.0 owns ARC; eval must not weaken ARC rules).
- Interactive line-editing / prompt UI inside `bn` (belongs to the REPL process).

---

## Proposed CLI surface

### Entry form (Carlos lock 2026-09-13)

**Locked:** eval is **`bn -e`**, not a `bn eval` subcommand.

```text
bn -e '<source>' [options]
bn --expr '<source>' [options]
bn -e --stdin [options]          # optional: source from stdin when -e is set without string
```

| Form | Meaning |
| --- | --- |
| `-e` / `--expr` *source* | Evaluate the given source string (shell-quoted by the caller) — **primary MVP** |
| `-e` with `--stdin` | Read entire stdin as the eval source (UTF-8) |
| `bn eval …` | **Not** part of this proposal — do not add a parallel subcommand in 0.5.1 |

Presence of `-e` / `--expr` selects the eval pipeline (snippet/program wrapping, result envelope). Other top-level commands (`run`, `check`, `build`, …) remain unchanged and still take a file path.

### Options (MVP)

| Option | Default | Intent |
| --- | --- | --- |
| `--mode snippet\|program` | `snippet` | See fragment kinds |
| `--format text\|json` | `text` | Result envelope |
| `--session` | off | With `-e`: keep process alive; read successive JSON Lines requests on stdin (see Session) |
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

- If the snippet parses as an **expression**, `bn -e` wraps as:

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

- If the snippet already contains top-level `FUNCTION Start`, treat as **`program`** (or reject with a clear diagnostic if `--mode snippet` forbids it — **recommend:** auto-promote to program semantics and warn once).

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

On failure: `"ok": false`, diagnostics mirrored in `stderr` and/or a `diagnostics` array (stable shape to be fixed at implementer time; minimum: `ok`, `exit_code`, `stderr`).

**Do not** print human prose on stdout when `--format json` is set (diagnostics may still go to stderr, or be embedded — pick one in implementation and document in `bn.1`; **recommend:** diagnostics only inside JSON + empty stderr for machine hosts).

---

## Session mode (`bn -e --session`) — optional but specified

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
| **S0 — Stateless session (MVP recommend)** | Each request is a fresh program environment (like Jupyter cells). Session only amortizes process/JIT startup. |
| **S1 — Accumulating session** | Top-level bindings from successful snippets remain visible to later snippets in the same process. |

**0.5.1 recommendation:** ship **S0** first (matches kernel today; smaller ARC/lifetime surface). Document **S1** as 0.5.2+ candidate with explicit retain/release rules under 0.5.0 ARC.

If S1 is pulled into 0.5.1 by Carlos, acceptance must include ARC fixtures for bindings that outlive a snippet (no use-after-release; weak/NULL rules unchanged).

---

## Security and HOST defaults

- Default filesystem policy for `bn -e` should match **`bn run`** unless flags say otherwise.
- REPL hosts that are untrusted-input facing should pass `--no-filesystem` (and sandbox roots) explicitly — document as host responsibility.
- No new ambient authority beyond `bn run`.

---

## Implementation sketch (non-prescriptive)

1. CLI parse for global `-e` / `--expr` (+ optional `--stdin` / `--session` / `--format`).  
2. Materialize source string → existing frontend (`check` path) → interpret `Start` (reuse `bn run` engine).  
3. Snippet parse probe: try expression vs statements; wrap; on failure emit actionable diagnostic.  
4. JSON envelope writer.  
5. Optional `--session` loop.  
6. Man page + `docs/project/usage.md` + fixtures under `docs/superpowers/evidence/bn-eval-0.5.1/`.

---

## Acceptance (0.5.1 done when)

1. `bn -e 'PRINT 1+1'` exits 0 and prints `2` (or equivalent PRINT formatting).  
2. `bn -e 'LET x AS INTEGER = 3\nPRINT x'` works in snippet mode.  
3. `bn -e 'PRINT "hi"' --format json` yields parseable JSON with `ok: true`.  
4. Invalid snippet → non-zero exit + diagnostic; JSON form has `ok: false`.  
5. `bn -e` without a source string (and without `--stdin`) → usage, exit 2.  
6. Evidence directory with NOTES + commands; Quorra gate.  
7. `bnr` README updated to call `/usr/local/bin/bn -e` (still no source dependency).  
8. Man page `bn.1` documents `-e` / `--expr` (and states there is no `bn eval` subcommand in 0.5.1).

---

## Out of 0.5.1 / later

| Item | Notes |
| --- | --- |
| S1 accumulating session | Needs ARC session story |
| Expression result without PRINT wrapper | Nice-to-have if IR can return a value to the host |
| `bn -e` → LLVM | Not needed for REPL |
| Language-level `EVAL` keyword | Separate DNA proposal |

---

## Open questions (Carlos)

1. Confirm **S0 vs S1** for 0.5.1 session (recommend S0).  
2. ~~Command shape~~ — **LOCKED (Carlos 2026-09-13):** **`bn -e`** (not `bn eval` subcommand).  
3. Confirm whether snippet auto-promotes when `FUNCTION Start` is present.  
4. JSON diagnostics: stderr empty vs duplicated — prefer single machine channel.

---

## History

- **2026-09-12** — Drafted for 0.5.1 after REPL scaffold (`~/src/Basic Next REPL`) locked to `/usr/local/bin/bn` without rewriting Basic Next. Carlos: place under `todo/proposals/`.
- **2026-09-13** — Carlos: eval entry is **`bn -e`**, not subcommand `bn eval`.
