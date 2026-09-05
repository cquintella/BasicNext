# Data dictionary — BasicNext target DFDs

> Canonical: `docs/architecture/dfd/data-dictionary.md`  
> Companion to [dfd-0-to-be.md](dfd-0-to-be.md), [dfd-1-to-be.md](dfd-1-to-be.md), and [dfd-2/](dfd-2/).

This file is the **data dictionary** for the to-be DFDs: named flows, what each carries, and how composite data is composed. Diagrams keep short labels; definitions live here.

**Notation reminder:** flows are named `Fnn` at DFD-1; DFD-2 uses `Cnn` (Control), `Ann` (Analyze Sources), `Lnn` (Lower and Validate IR), `Inn` (Interpret IR), `Gnn` (Compile IR), and `Rnn` (Record process log). Store ids (`D1`…`D5`, `D_ast`, `D_sym`, `D_tok`, `D_graph`, `D_host`, `D_ll`, `D_cfg`, `D_job`, `D_status`, …) are defined under **Data stores**.

---

## Level-0 border flows and data elements

Each arrow on the diagram carries named data. Below, every component is spelled out so the DFD-0 border is unambiguous.

### 1. Developer → 0 — `source .bn, options`

This is the developer (or their script) asking the toolchain to do something with a program.

- **`source .bn`**: the entry text file of the BasicNext program (and, by implication, the module set the toolchain will need to resolve from that entry). It is source code, not IR and not a binary.
- **`options`**: the `bnc` invocation choices that select *what to do* with that source and *how* to run the pipeline. In the target UX that includes at least:
  - default = **interpret**;
  - **`-c`** = compile for the current host platform;
  - **`-c --target …`** = compile for an explicit platform (for example `wasm32`);
  - **`--check`** = analyze only (frontend through validated IR, no run/link);
  - logging controls (**`--log-level`**, log path/dir, or `--no-log`);
  - location controls (**`--programs-dir`**, and **`--plugins-dir`** reserved);
  - build output path when compiling (**`-o` / `--output`**);
  - optional **`--config`** file that supplies the same keys with CLI override.

Together: “here is the program, and here is the job.”

### 2. 0 → Developer — `diagnostics, logs, run output`

This is the toolchain reporting what happened. It is not a single blob; three kinds of result travel on this arrow (any subset may be present depending on the profile).

- **`diagnostics`**: structured errors and warnings from analysis, IR validation, runtime, or compile/link — with stable codes, source locations (spans), and (in the 0.4.5 expressive-diagnostics target) title/message/causes/help. This is what the human reads to fix the program. The same diagnostic *facts* may also be mirrored into the process log; they must not contradict each other.
- **`logs`**: the **process / pipeline log** narrative (phases entered, config snapshot that is safe to record, tool argv such as clang, success/failure of stages), filtered by **`log-level`**. For compile jobs this is typically the companion file (for example next to the artifact); the developer may also see a short summary on the terminal. This log explains *what the toolchain did*, not a second private error language.
- **`run output`**: only when the profile **interprets**: the program’s own stdout/stderr and the process **exit code**. When the profile is check-only or compile-only, there is no program stdout from interpretation; the developer still gets diagnostics (and for compile, artifact path via the file-system flows / messages).

Also implied on this border when useful: a clear **exit code** for the `bnc` process itself (success vs toolchain failure vs program failure), so scripts can branch.

### 3. 0 ↔ File system — `read sources / write artifacts`

Bidirectional use of durable storage.

**0 reads from the file system:**

- **`.bn` module sources** needed for the entry (and imports), under the resolved programs directory / project layout.
- **Optional config file** bytes when `--config` or the default search finds one (merged under CLI precedence inside process 0 — detail in DFD-2 of Control).

**0 writes to the file system:**

- **Build artifact**: native executable or `.wasm` after a successful compile path.
- **Companion process log** file when logging to disk is enabled (path from options / convention such as alongside `-o`).
- **Temporary LLVM `.ll`** (or equivalent intermediates) during compile, when the build session keeps them on disk before/while invoking clang; they may be cleaned up afterward.

If a write or read fails (permissions, disk full, missing entry file), process 0 must surface that as a toolchain failure to the developer — never pretend a binary was produced.

### 4. IDE ↔ 0 — `LSP / DAP`

The editor does not send “source .bn, options” as a shell argv line; it speaks protocols. The *meaning* is still the same core.

- **LSP → 0**: document open/change, completion/hover/definition/references requests, and configuration the client attaches. Process 0 must analyze with the **same frontend → IR** understanding as `bnc --check`, not a weaker toy path.
- **0 → LSP**: diagnostics for squiggles, hover/symbol info, and other language-service replies derived from that shared analysis.
- **DAP → 0**: debug launch/attach, breakpoints, continue/step, evaluate requests — which drive **interpret IR** under debug control.
- **0 → DAP**: stopped events, stack/variables snapshots, output, and termination — still from the IR oracle, not from a second semantics.

Shipping `bn-lsp` / `bn-dap` as separate binaries later does not add new externals; they remain doors into the same circle 0.

### 5. 0 → LLVM toolchain — `emit .ll + invoke`

Process 0 (via its compile backend) sends work to the **external** LLVM tools:

- **LLVM IR text** (`.ll`) — or the agreed on-disk form — lowered from **BN IR**, not from a divergent AST-only path.
- **Invoke argv**: the clang / ld / opt command line (target triple, optimization level, input `.ll`, output path, link libraries such as `bn_rt` when required).

No BasicNext language semantics are delegated to this arrow; it is only “please codegen/link this IR.”

### 6. LLVM toolchain → 0 — `linked native / wasm`

The external tools return:

- **Linked artifact bytes / path** — native binary or `.wasm` (and related object products as the session requires).
- **Tool exit status** (and stderr/stdout of clang when captured) so process 0 can turn link failures into **diagnostics** and **log** events for the developer.

Process 0 then places the successful artifact on the file system (flow 3) and reports outcome to the developer (flow 2).

---

## Level-1 data flows (F01–F36)

#### F01 — Developer → 1.0 — job request (entry + options)

The human (or a script) asks Control to run a job. The payload is the **entry** `.bn` path plus **options**: profile intent (default interpret, `-c`, `-c --target`, `--check`), log controls, programs-dir / plugins-dir reserved, output path, and optional config path. This is the CLI front door into `bnc`; it does not yet carry IR or binaries.

#### F02 — Editor IDE → 1.0 — IDE job (LSP/DAP request)

The editor reaches the same Control surface through protocol traffic instead of argv: document open/change, language-service requests, or a debug launch. Control must map that into an engine job that still uses the **same** frontend → IR core as F01, so IDE and CLI never diverge in meaning.

#### F03 — 1.0 → 2.0 — pipeline schedule (check | interpret | compile)

Control tells Analyze sources to start the Frontend leg for the chosen profile. The schedule names which profile is active and which entry/config resolved paths to use. Every successful job that needs IR begins here; Backend is not started until Frontend (and IR validation) succeed, except where Control later issues F04/F05.

#### F04 — 1.0 → 4.0 — interpret command

After Frontend has produced validated IR (or when Control is ready to run the interpret profile), Control commands **Interpret IR**. The command carries job identity, entry, and run-related options (not a second copy of source text). Interpret reads IR via F20; F04 is the authorization to run.

#### F05 — 1.0 → 5.0 — compile command (`-c` / `--target`)

Control commands **Compile IR** when the profile is compile. Includes host vs explicit `--target` (for example `wasm32`) and output path expectations. Compile consumes IR via F21 and may talk to the external LLVM toolchain via F25/F26.

#### F06 — 1.0 → 6.0 — check-only stop (after Frontend)

When the profile is `--check`, Control records that the pipeline **stops after Frontend**: no interpret, no compile. This is an explicit control fact for the process log so a check job is never mistaken for a silent no-op or a failed run.

#### F07 — 1.0 → 6.0 — control events

Ongoing Control narrative for the process log: invocation received, config merged, profile selected, options accepted or rejected, dispatch started, and (when DFD-2 opens 1.7) completion outcome. These events are about the **controller**, not about language diagnostics (those travel D3 → 6.0 on F32).

#### F08 — File system → 2.0 — source bytes

Raw bytes of `.bn` modules (and related files Analyze needs) read from disk under the resolved programs directory / project layout. This is the durable source of truth for text; D1 holds the in-session loaded view after F09.

#### F09 — 2.0 → D1 — load sources

Analyze deposits the loaded module texts (paths + contents, or handles) into store **D1 Sources** so later Frontend steps and tooling share one loaded set for this job instead of re-reading disk ad hoc.

#### F10 — 2.0 → D_ast — AST

Parse (and related Frontend structure) writes the **AST** for the job into **D_ast**. Lowering (3.0) will read it on F14. The store is job-scoped Intermediate Representation *of syntax*, not yet BN IR.

#### F11 — 2.0 → D_sym — symbols / semantic

Name resolution, types, and other semantic facts land in **D_sym**. Lowering consumes them on F15. Keeping symbols in a store (not only ephemeral locals) makes the Frontend → IR handoff inspectable and shared with IDE analysis paths later.

#### F12 — 2.0 → D3 — analysis diagnostics

Parse/semantic errors and warnings from Analyze are written into **D3 Diagnostics**. They are the same diagnostic model the developer and IDE will eventually see (F35/F36), not a private Frontend side channel.

#### F13 — 2.0 → 6.0 — analysis events

Process-log events from Analyze: phases entered (lex/parse/resolve), module counts, success/failure of analysis — narrative of *what the Frontend did*, distinct from the diagnostic payloads in D3.

#### F14 — D_ast → 3.0 — AST for lowering

Lower and validate IR reads the AST produced by Analyze. Without this flow, 3.0 would have nothing syntactic to lower.

#### F15 — D_sym → 3.0 — semantic for lowering

Lowering also reads the semantic/symbol model so IR construction respects resolved names and types rather than re-deriving a second, divergent analysis.

#### F16 — 2.0 → 3.0 — handoff to lower

The control/edge signal that Analyze is ready for Lower (same job). Complements F14/F15: stores hold data; F16 is the “proceed” between sibling Frontend processes.

#### F17 — 3.0 → D2 — validated IR

The single **BN IR** for the job, after lowering and validation, stored in **D2**. This is the contract both Backend legs consume. Interpret and compile must not invent a second meaning from AST alone.

#### F18 — 3.0 → D3 — IR diagnostics

Errors and warnings from lowering or IR validation (ill-formed IR, unsupported constructs, validation failures) enter D3 alongside analysis diagnostics.

#### F19 — 3.0 → 6.0 — lower/validate events

Process-log events for lower/validate: started, finished, IR size/summary safe to log, outcome. Again narrative, not the diagnostic records themselves.

#### F20 — D2 → 4.0 — IR to interpret

Interpret reads validated BN IR from D2. This is the **semantic oracle** path: running the language means executing this IR, not calling LLVM `lli`.

#### F21 — D2 → 5.0 — IR to compile

Compile reads the **same** validated BN IR from D2. Compile lowers BN IR to LLVM IR and invokes external tools; it must not compile from a private AST fork.

#### F22 — 4.0 → Developer — run output / exit

When interpreting, the program’s stdout/stderr and the program **exit code** go to the Developer. This is `run output` from DFD-0, separate from toolchain diagnostics.

#### F23 — 4.0 → D3 — runtime diagnostics

Traps, panics surfaced as diagnostics, HOST failures that become user-facing diagnostics, and similar runtime issues are recorded in D3 so they share the same store as Frontend/compile diagnostics.

#### F24 — 4.0 → 6.0 — interpret events

Process-log events for the interpret leg: start, finish, exit code summary, timing if recorded. Complements F22/F23.

#### F25 — 5.0 → LLVM EXTERNAL — LLVM IR + argv

Compile emits LLVM IR (`.ll` or agreed form) lowered from BN IR and the **invoke argv** for clang / ld / opt (target, opts, inputs, output, `bn_rt` as needed). Semantics stay inside BasicNext; this arrow is only codegen/link work.

#### F26 — LLVM EXTERNAL → 5.0 — linked object / binary

External tools return linked native or wasm products (and tool status/stderr as captured). Compile turns failures into diagnostics (F28) and success into the build artifact store (F27).

#### F27 — 5.0 → D5 — build artifact

The successful compile product (path/metadata and/or bytes handle) is recorded in **D5 Build artifact** before durable write and user notification.

#### F28 — 5.0 → D3 — compile diagnostics

Lowering-to-LLVM problems, clang/link failures mapped into the diagnostic model, and related compile warnings enter D3.

#### F29 — 5.0 → 6.0 — compile events

Process-log events for compile: emit `.ll`, clang argv (safe to record), link result, artifact path summary.

#### F30 — D5 → Developer — artifact path

The Developer is told where the successful binary/wasm lives (and that compile succeeded). Path messaging may also appear in the process log; F30 is the user-facing artifact pointer.

#### F31 — D5 → File system — write artifact

Durable write of the build artifact onto disk (E3). If write fails, that failure must surface as toolchain failure, not a silent “success” path.

#### F32 — D3 → 6.0 — diagnostics for log

Record process log may **mirror** diagnostic facts (codes, counts, selected messages) into the narrative log so support can correlate “what failed” with “what the pipeline did.” The log must not invent a contradictory second error language; D3 remains the diagnostic store of record.

#### F33 — 6.0 → D4 — process log record

6.0 assembles the job’s process-log record (control + stage events + optional diagnostic mirror) into store **D4**.

#### F34 — D4 → File system — write process log

Durable write of the companion process log (path from options / convention, often beside `-o`). Verbosity follows `log-level`; `--no-log` skips this write.

#### F35 — D3 → Developer — diagnostics to user

Human-facing diagnostics (errors/warnings with locations, and expressive title/message/causes/help when that model ships) presented on the CLI / terminal to the Developer. Same facts as in D3; presentation only.

#### F36 — D3 → Editor IDE — diagnostics to IDE

**Description:** Diagnostics from store **D3** are published to the Editor IDE (typically as LSP `publishDiagnostics` / related protocol payloads): codes, severities, source spans, and messages (plus expressive fields when available). The IDE must show the **same diagnostic facts** the CLI would show for the same analysis of the same sources — not a weaker or alternate checker. DAP may surface related runtime diagnostics while debugging, but F36 is the primary “squiggles and Problems panel” path from D3 to E2.

---

## Data stores

| Id | Name | Contents (definition) | Owned / used by (DFD-2) |
| --- | --- | --- | --- |
| D1 | Sources | Job-scoped loaded `.bn` module texts (paths + contents) after Analyze loads from the file system. | **2.0** owns (A03); Lex reads (A05). |
| D_tok | Tokens | Lexical-analysis product for the job (token streams per module). May stay session-private to Analyze. | **2.0** owns (A06); Syntactic analysis reads (A08). |
| D_ast | AST | Syntax trees / program structure for the job (parse products). | **2.0** owns (A09); graph/analyze and **3.0** Lower read (A11, A15, A23 / L02). |
| D_graph | Module graph | Import / module structure assembled for analysis and later lowering. | **2.0** owns (A13); Analyze reads (A14). |
| D_sym | Symbols | Semantic model: resolved names, types, and related facts. | **2.0** owns (A16); **3.0** Lower reads (A24 / L03); IDE analysis paths later. |
| D2 | IR | Single **BN IR** for the job — draft then validated — the contract both Interpret and Compile consume. | **3.0** owns (L04 draft, L08 validated); **4.0** / **5.0** read (L11–L12 / I02 / G02). |
| D3 | Diagnostics | Store of record for errors/warnings (lex, parse, analysis, IR, runtime, HOST, compile) with codes, spans, messages. | Written by **2.0**, **3.0**, **4.0**, **5.0**; mirrored by **6.0** (R06); shown to Developer/IDE (F35/F36). |
| D4 | Process log | Assembled process/pipeline log record for the job (control + stage events + optional diagnostic mirror). | **6.0** owns (R09); written to disk (R10). |
| D5 | Build artifact | Successful compile product handle/path (native or wasm) before/after durable write. | **5.0** owns (G14); Developer/FS via G15–G16. |
| D_host | HostEnv + heap | Bound host capabilities, providers, and runtime heap/values for an interpret session. | **4.0** owns (I03); Execute reads (I04). |
| D_ll | temp `.ll` | Session LLVM IR text lowered from BN IR, kept for clang invoke / temp write. | **5.0** owns (G03); Write temp / Invoke read (G06). |
| D_cfg | Effective config | Merged + validated settings (CLI > file > defaults), resolved dirs, profile id. | **1.0** owns (C05, C07, C09, C12); subprocesses 1.3–1.6 read. |
| D_job | Engine job | Engine job descriptor: entry, profile, paths, log controls — passed to Dispatch. | **1.0** owns (C14); Dispatch reads (C15). |
| D_status | Job status | Live job status while stages run and final outcome for Control. | **1.0** owns (C24); Dispatch reads (C25). |

---

## Level-2 Control flows (C01–C28)

#### C01 — Developer → 1.1 — raw argv / flags

The human (or a script) sends the raw command line into Parse invocation. This is the CLI front door for Control: entry path hints and flags before config merge. It does not yet carry an engine job or IR.

#### C02 — Editor IDE → 1.1 — IDE launch options

The editor reaches the same Parse invocation surface with launch / LSP-DAP options instead of shell argv. Control must map them into the same flag model as C01 so IDE and CLI share one controller path.

#### C03 — 1.1 → 1.2 — parsed flags

Parse invocation hands structured flags (and positional entry) to Load and merge config. Raw argv is no longer the working form; validation and profile selection will consume this structure after merge.

#### C04 — File system → 1.2 — config file bytes

Optional config file contents read from disk when `--config` or the default search finds one. Missing config is not an error by itself; CLI flags still apply with defaults.

#### C05 — 1.2 → D_cfg — merged settings

Load and merge config writes the effective settings blob (CLI over file over defaults) into **D_cfg**. Later Control steps refine dirs, profile, and acceptance into the same store.

#### C06 — D_cfg → 1.3 — settings

Resolve directories reads the current effective config so it can fix programs-dir (and reserved plugins-dir) against the job context.

#### C07 — 1.3 → D_cfg — resolved dirs (programs-dir, plugins-dir reserved)

Resolved directory paths are written back into **D_cfg**. Plugins-dir remains reserved for a future extension surface; programs-dir anchors source loads in Analyze.

#### C08 — D_cfg → 1.4 — settings

Select pipeline profile reads effective settings (including any explicit check / `-c` / `--target` intent) to choose check \| interpret \| compile.

#### C09 — 1.4 → D_cfg — profile id

The chosen profile identifier is stored in **D_cfg** so validation, job build, and Dispatch all see one authoritative profile for the job.

#### C10 — D_cfg → 1.5 — settings + profile

Validate options reads settings together with the profile id (for example `--target` requires `-c`, unknown flags reject).

#### C11 — 1.5 → Developer — rejected options (error)

Option validation failures are reported immediately to the Developer as toolchain errors. No engine job is dispatched; the process log may still record the rejection via later control events if Control emits them.

#### C12 — 1.5 → D_cfg — accepted settings

On success, Validate options confirms the accepted settings (and profile constraints) back into **D_cfg**, ready for Build engine job.

#### C13 — D_cfg → 1.6 — accepted settings

Build engine job reads the fully accepted config to construct the job descriptor passed to the rest of the pipeline.

#### C14 — 1.6 → D_job — engine job (entry, profile, paths, log)

The engine job descriptor — entry path, profile, resolved dirs, log controls, output expectations — is stored in **D_job**. This is what Dispatch will schedule against.

#### C15 — D_job → 1.7 — job

Dispatch reads the engine job from **D_job** and begins staging Frontend / Backend / Log according to the profile.

#### C16 — 1.7 → 2.0 — schedule Frontend

Dispatch tells Analyze sources to run the Frontend leg for this job (maps to F03 / A01). Every profile that needs IR starts here.

#### C17 — 1.7 → 4.0 — interpret command

When the profile is interpret and Frontend/IR succeeded, Dispatch authorizes Interpret IR (maps to F04 / I01). The command carries job identity and run options, not a second copy of source text.

#### C18 — 1.7 → 5.0 — compile command

When the profile is compile, Dispatch authorizes Compile IR with host vs `--target` and output path expectations (maps to F05 / G01).

#### C19 — 1.7 → 6.0 — check-only / control events

Control narrative for the process log: check-only stop after Frontend, invocation/config/profile facts, and related controller events (maps to F06/F07 and R01 inputs). These are about the controller, not language diagnostics.

#### C20 — 1.7 → 6.0 — dispatch start events

Dispatch records that stages were started (which legs were scheduled) so the process log can narrate pipeline kickoff separately from completion.

#### C21 — 2.0 → 1.7 — Frontend done (ok\|fail + diags summary)

Analyze (via completion toward Control) reports Frontend outcome: success or failure plus a diagnostics summary. Dispatch uses this to stop, continue to Lower/Backend, or finish a check-only job (pairs with A26 / L13 as applicable).

#### C22 — 4.0 → 1.7 — Interpret done (ok\|fail + exit code)

Interpret reports completion with ok/fail and the program exit code so Control can form the overall pipeline outcome (pairs with I13).

#### C23 — 5.0 → 1.7 — Compile done (ok\|fail + artifact path)

Compile reports completion with ok/fail and artifact path (when successful) so Control can notify the user and finalize status (pairs with G17).

#### C24 — 1.7 → D_status — update job status

Dispatch writes live and final job status into **D_status** as stages complete (running, failed, succeeded, check-only done, and so on).

#### C25 — D_status → 1.7 — status

Dispatch may re-read **D_status** when deciding what to emit next (outcome to Developer/IDE, completion events to the log).

#### C26 — 1.7 → Developer — pipeline outcome (exit code / summary)

Control delivers the coherent overall outcome to the Developer: toolchain exit code and a short summary. This complements per-stage run output (F22) and diagnostics (F35).

#### C27 — 1.7 → Editor IDE — pipeline outcome (IDE result)

The same overall outcome is delivered to the Editor IDE in protocol-friendly form so LSP/DAP clients can end the job consistently with CLI.

#### C28 — 1.7 → 6.0 — completion events

Final controller events for the process log: pipeline finished, overall ok/fail, and safe summary fields (feeds R01 completion).

---

## Level-2 Analyze Sources flows (A01–A26)

#### A01 — 1.0 → 2.1 — schedule Frontend

Control schedules the Frontend leg: entry path, resolved programs-dir, and the note that Analyze must run for this job (from F03 / C16). Load entry and modules is the first subprocess to receive it.

#### A02 — File system → 2.1 — source bytes

Raw `.bn` module bytes for the entry (and later imports) read from disk under the resolved layout (parent F08). This is the durable text source of truth before D1 holds the loaded view.

#### A03 — 2.1 → D1 — load sources

Load deposits module texts (paths + contents) into **D1 Sources** for this job so Lex and later steps share one loaded set (parent F09).

#### A04 — 2.1 → 2.2 — entry ready

Load signals Lex that the current module text is ready to tokenize (after the entry load, and again when imports are pulled in via A12).

#### A05 — D1 → 2.2 — source text

Lex reads loaded source text from **D1** rather than re-opening the file system ad hoc for each pass.

#### A06 — 2.2 → D_tok — tokens

**2.2 Lexical analysis** writes the token stream for the module into **D_tok**. **2.3 Syntactic analysis** consumes tokens from this store, not from a private side channel.


#### A07 — 2.2 → D3 — lex diagnostics

Lexer errors and warnings enter **D3** under the shared diagnostic model (part of parent F12).

#### A08 — D_tok → 2.3 — tokens

**2.3 Syntactic analysis** reads tokens from **D_tok** to build the AST for the module.

#### A09 — 2.3 → D_ast — AST

**2.3 Syntactic analysis** writes syntax trees / program structure into **D_ast** (parent F10). Lowering and module-graph assembly will read this store.

#### A10 — 2.3 → D3 — parse diagnostics

**2.3 Syntactic analysis** errors and warnings enter **D3** (part of parent F12).

#### A11 — D_ast → 2.4 — AST

Assemble module graph reads the AST to discover imports and build the job’s module structure.

#### A12 — 2.4 → 2.1 — need import

When the graph finds an unresolved import, it loops back to Load to bring that module into **D1** for the same job. This is an internal Analyze cycle, not a new Control schedule.

#### A13 — 2.4 → D_graph — module graph

The assembled import / module graph is stored in **D_graph** for **2.5 Semantic analysis**.

#### A14 — D_graph → 2.5 — graph

**2.5 Semantic analysis** reads the module graph so semantic work spans the whole import set, not only the entry AST.

#### A15 — D_ast → 2.5 — AST

**2.5 Semantic analysis** also reads ASTs from **D_ast** together with the graph.

#### A16 — 2.5 → D_sym — symbols / semantic

Resolved names, types, and related semantic facts land in **D_sym** (parent F11). Lowering will consume them; IDE analysis must reuse the same model later.

#### A17 — 2.5 → D3 — analysis diagnostics

Semantic / resolve errors and warnings enter **D3** (part of parent F12).

#### A18 — 2.1 → 6.0 — load events

Process-log events from Load: modules loaded, paths resolved, load failures — narrative of *what Load did* (part of parent F13).

#### A19 — 2.2 → 6.0 — lex events

Process-log events from Lex: phase entered, module count tokenized, outcome (part of parent F13).

#### A20 — 2.3 → 6.0 — parse events

Process-log events from Parse: phase entered, parse outcome (part of parent F13).

#### A21 — 2.4 → 6.0 — graph events

Process-log events from module-graph assembly: imports walked, graph size/summary safe to log (part of parent F13).

#### A22 — 2.5 → 6.0 — analysis events

Process-log events from Analyze and resolve: phase entered, success/failure of semantic analysis (part of parent F13).

#### A23 — D_ast → 3.0 — AST for lowering

Lower and validate IR reads the AST produced by Analyze (parent F14 / L02). Without this flow, 3.0 would have nothing syntactic to lower.

#### A24 — D_sym → 3.0 — semantic for lowering

Lowering also reads the semantic/symbol model so IR construction respects resolved names and types (parent F15 / L03).

#### A25 — 2.5 → 3.0 — handoff to lower

The control/edge signal that Analyze is ready for Lower for this job (parent F16 / L01). Stores hold data; A25 is the “proceed” between sibling Frontend processes.

#### A26 — 2.5 → 1.0 — Frontend done

Analyze reports Frontend completion (ok\|fail + diagnostics summary) to Control so Dispatch can stop, continue, or finish check-only (feeds C21). Hard failure and check-only both still report here.

---

## Level-2 Lower and Validate IR flows (L01–L13)

#### L01 — 2.0 → 3.1 — handoff to lower

Analyze signals that AST + symbols are ready to lower for this job (parent F16 / A25). Lower AST+semantic → BN IR begins.

#### L02 — D_ast → 3.1 — AST for lowering

Lowering reads program structure from **D_ast** (parent F14 / A23).

#### L03 — D_sym → 3.1 — semantic for lowering

Lowering reads resolved names and types from **D_sym** (parent F15 / A24) so IR is not a second, divergent analysis.

#### L04 — 3.1 → D2 — draft IR

Lower writes draft **BN IR** into **D2**. Validation has not yet stamped it as the contract backends may trust.

#### L05 — 3.1 → D3 — lower diagnostics

Problems discovered while constructing IR (unsupported constructs, lower failures) enter **D3** (part of parent F18).

#### L06 — 3.1 → 6.0 — lower events

Process-log events for the lower phase: started, finished, IR size/summary safe to log (part of parent F19).

#### L07 — D2 → 3.2 — draft IR

Validate IR reads the draft from **D2** to check structural and light semantic constraints.

#### L08 — 3.2 → D2 — validated IR

On success, Validate writes the validated **BN IR** back to **D2** — the single Intermediate Representation both backends consume (parent F17).

#### L09 — 3.2 → D3 — IR diagnostics

Ill-formed IR, validation failures, and related warnings enter **D3** (parent F18).

#### L10 — 3.2 → 6.0 — validate events

Process-log events for validate: started, finished, outcome (parent F19).

#### L11 — D2 → 4.0 — IR to interpret

When Control commanded interpret, Interpret reads validated BN IR from **D2** (parent F20 / I02). This is the semantic-oracle input.

#### L12 — D2 → 5.0 — IR to compile

When Control commanded compile, Compile reads the **same** validated BN IR from **D2** (parent F21 / G02). Compile must not lower from a private AST fork.

#### L13 — 3.2 → 1.0 — Frontend/IR done summary

Lower/validate reports a summary to Control so Dispatch knows Frontend+IR finished ok or failed (complements A26 / C21 for the IR gate). Hard validation failure stops Backend legs; check-only may end after a successful D2.

---

## Level-2 Interpret IR flows (I01–I13)

#### I01 — 1.0 → 4.1 — interpret command

Control authorizes Interpret for this job after Frontend/IR are ready (parent F04 / C17). Bind HostEnv is the first subprocess to receive the command.

#### I02 — D2 → 4.2 — IR to interpret

Execute BN IR reads validated IR from **D2** (parent F20 / L11). Running the language means executing this IR, not calling LLVM `lli`.

#### I03 — 4.1 → D_host — bound env

Bind HostEnv installs capability flags and host providers into **D_host** for this run (filesystem, network policy, and related session state).

#### I04 — D_host → 4.2 — env / heap

Execute reads the bound environment and heap/values from **D_host** while stepping the IR.

#### I05 — 4.2 → 4.3 — HOST ops

The executor issues platform / HOST operations (net, http, web, dispatch, dataframe, …) to HOST services. Target architecture keeps this a DAG of traits / shared values — no runtime↔http cycles.

#### I06 — 4.3 → 4.2 — values / side effects

HOST services return values and side-effect results to the executor so IR evaluation can continue.

#### I07 — 4.2 → D3 — runtime diagnostics

Traps, panics surfaced as diagnostics, and similar executor issues enter **D3** (parent F23).

#### I08 — 4.3 → D3 — HOST diagnostics

HOST failures that become user-facing diagnostics enter **D3** alongside runtime diagnostics (part of parent F23).

#### I09 — 4.2 → 6.0 — interpret events

Process-log events for the interpret leg: start, finish, exit-code summary, timing if recorded (parent F24).

#### I10 — 4.2 → 4.4 — execution result

Execute hands the run result (exit code, captured output handles, success/fail) to Produce run output.

#### I11 — 4.4 → Developer — run output / exit

Program stdout/stderr and the program exit code go to the Developer (parent F22). Separate from toolchain diagnostics.

#### I12 — 4.4 → Editor IDE — debug / DAP output

When debugging, Produce run output surfaces DAP events, stack/variables snapshots, evaluate results, and related debug traffic to the Editor IDE.

#### I13 — 4.4 → 1.0 — Interpret done

Interpret reports completion (ok\|fail + exit code) to Control (feeds C22). Check-only and compile-only profiles never enter this path.

---

## Level-2 Compile IR flows (G01–G17)

#### G01 — 1.0 → 5.1 — compile command

Control authorizes Compile with `-c`, optional `--target`, and output path expectations (parent F05 / C18). Lower BN IR → LLVM IR begins.

#### G02 — D2 → 5.1 — IR to compile

Compile reads the same validated BN IR Interpret would run (parent F21 / L12). Language semantics stay on BN IR; this step only targets LLVM.

#### G03 — 5.1 → D_ll — LLVM IR text

Emit writes session LLVM IR text into **D_ll** for temp write and clang invoke.

#### G04 — 5.1 → D3 — emit diagnostics

Problems lowering BN IR to LLVM IR enter **D3** (part of parent F28).

#### G05 — 5.1 → 6.0 — emit events

Process-log events for emit: started, finished, safe IR/artifact summaries (part of parent F29).

#### G06 — D_ll → 5.2 — .ll

Write temp `.ll` reads LLVM IR text from **D_ll** when the session keeps IR on disk for clang.

#### G07 — 5.2 → File system — write temp .ll

Durable (or session-temp) write of the `.ll` file the external toolchain will consume. Write failures must surface as toolchain failure.

#### G08 — 5.2 → 5.3 — ready to link

Write temp signals Invoke that the `.ll` (and related inputs) are ready for clang / ld / opt.

#### G09 — 5.3 → LLVM EXTERNAL — LLVM IR + argv

Compile sends LLVM IR (path or agreed form) and the invoke argv (target triple, opts, inputs, output, `bn_rt` as needed) to the external LLVM tools (parent F25). No BasicNext language semantics are delegated on this arrow.

#### G10 — LLVM EXTERNAL → 5.3 — linked object / binary

External tools return linked native or wasm products plus tool status/stderr as captured (parent F26).

#### G11 — 5.3 → D3 — compile diagnostics

Clang/link failures and related compile warnings mapped into the diagnostic model enter **D3** (parent F28).

#### G12 — 5.3 → 6.0 — compile events

Process-log events for invoke/link: clang argv safe to record, link result, timing (parent F29).

#### G13 — 5.3 → 5.4 — linked product

Invoke hands the successful linked product (or failure context) to Produce build artifact.

#### G14 — 5.4 → D5 — build artifact

The successful compile product handle/path is recorded in **D5** (parent F27) before durable write and user notification.

#### G15 — D5 → File system — write artifact

Durable write of the build artifact onto disk (parent F31). If write fails, that failure must surface as toolchain failure, not a silent success.

#### G16 — D5 → Developer — artifact path

The Developer is told where the successful binary/wasm lives (parent F30). Path messaging may also appear in the process log.

#### G17 — 5.4 → 1.0 — Compile done

Compile reports completion (ok\|fail + artifact path) to Control (feeds C23).

---

## Level-2 Record process log flows (R01–R12)

#### R01 — 1.0 → 6.1 — control / check-only / completion events

Control feeds controller narrative into Ingest: check-only stop, dispatch start, completion, config/profile facts (from F06/F07 / C19/C20/C28). These explain *what Control did*, not language diagnostics.

#### R02 — 2.0 → 6.1 — analysis events

Analyze stage events (load/lex/parse/graph/analyze) enter Ingest (parent F13 / A18–A22).

#### R03 — 3.0 → 6.1 — lower/validate events

Lower and validate stage events enter Ingest (parent F19 / L06, L10).

#### R04 — 4.0 → 6.1 — interpret events

Interpret stage events enter Ingest (parent F24 / I09).

#### R05 — 5.0 → 6.1 — compile events

Compile stage events enter Ingest (parent F29 / G05, G12).

#### R06 — D3 → 6.2 — diagnostics for log

Mirror diagnostics optionally reads selected diagnostic facts from **D3** (codes, counts, selected messages) so support can correlate failures with stages (parent F32). The log must not invent a contradictory second error language; D3 remains the store of record.

#### R07 — 6.1 → 6.3 — event stream

Ingest forwards the ordered stage/control event stream to Assemble log record.

#### R08 — 6.2 → 6.3 — mirrored diag facts

Optional mirrored diagnostic facts join the assemble step alongside the event stream.

#### R09 — 6.3 → D4 — process log record

Assemble writes the job’s process-log record (config snapshot safe to record, phases, tool argv, outcomes, optional diag mirror) into **D4** (parent F33).

#### R10 — 6.4 → File system — write process log

**6.4 Write / emit** writes the companion process-log file to the file system (path from options / convention, often beside `-o`), unless `--no-log`. This is the durable form of **F34** at DFD-1. The record content comes from **D4** via **R12**.

#### R11 — 6.4 → Developer — log summary (optional)

Write/emit may echo a short terminal summary of the log to the Developer, filtered by `log-level`. Full narrative remains in the companion file when written.

#### R12 — D4 → 6.4 — record to write

Store **D4** supplies the assembled process-log record to **6.4**, which then performs the durable write (**R10**) and optional developer summary (**R11**).


## Index of flow ids

| Prefix | Level | Document |
| --- | --- | --- |
| (unnamed / labeled on DFD-0) | 0 | [dfd-0-to-be.md](dfd-0-to-be.md) + **prose in this file** |
| F01–F36 | 1 | [dfd-1-to-be.md](dfd-1-to-be.md) + **prose in this file** |
| C01–C28 | 2 (1.0) | [dfd-2/1.0 Control.md](dfd-2/1.0 Control.md) + **prose in this file** |
| A01–A26 | 2 (2.0) | [dfd-2/2.0 Analyze Sources.md](dfd-2/2.0 Analyze Sources.md) + **prose in this file** |
| L01–L13 | 2 (3.0) | [dfd-2/3.0 Lower and Validate IR.md](dfd-2/3.0 Lower and Validate IR.md) + **prose in this file** |
| I01–I13 | 2 (4.0) | [dfd-2/4.0 interpret IR.md](dfd-2/4.0 interpret IR.md) + **prose in this file** |
| G01–G17 | 2 (5.0) | [dfd-2/5.0 Compile IR.md](dfd-2/5.0 Compile IR.md) + **prose in this file** |
| R01–R12 | 2 (6.0) | [dfd-2/6.0 Record process log.md](dfd-2/6.0 Record process log.md) + **prose in this file** |
