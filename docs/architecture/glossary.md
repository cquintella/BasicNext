# Glossary — BasicNext architecture terms

> Canonical: `docs/architecture/glossary.md`  
> Status: companion glossary — **2026-09-05**  
> American spelling (**Analyze**). Expansions favor the to-be toolchain, not
> every historical as-is name.

Short definitions for reading DFDs, the IR contract, and target-architecture
prose. Normative behavior lives in the language docs and DFD/data-dictionary
files; this page is a map of vocabulary.

---

## Core pipeline

| Term | Meaning |
| --- | --- |
| **IR** | Intermediate Representation in general. |
| **BN IR** | BasicNext’s **single** IR between Frontend and the interpret/compile backends; stored in **D2**. |
| **Oracle** | The **interpret** path (`bn_runtime` executing validated BN IR under HostEnv) as the source of truth for language+HOST semantics. LLVM implements a documented subset; it does not redefine meaning. |
| **Frontend** | Produce IR: **2.0 Analyze Sources** + **3.0 Lower and Validate IR** (lexical / syntactic / semantic analysis → validated BN IR). Shared by CLI and IDE. |
| **Backend** | Consume IR: **4.0 Interpret IR** and/or **5.0 Compile IR**. |
| **Analyze** | American spelling for process **2.0** (and check-only analysis). Prefer **Analyze** in architecture docs. |
| **Lower and Validate** | Process **3.0** — AST/symbols → draft then validated BN IR; IR diagnostics to **D3**. |
| **Interpret** | Process **4.0** — execute BN IR (semantic oracle). |
| **Compile** | Process **5.0** — BN IR → LLVM IR → external toolchain → artifact. |
| **Control** | Process **1.0** (`bnc` / pipeline manager) — parse invocation, profile, dispatch, collect completion. |
| **Process log** | Companion pipeline/forensic log (**6.0** / **D4**), e.g. `.bnbuild.log`; phase events + safe config + tool argv; not a second diagnostic language. |

---

## Binaries and packages

| Term | Meaning |
| --- | --- |
| **`bnc`** | BN Controller — proposed thin pipeline manager + process log; calls `bn` as engine; flag UX (`--check`, `-c`, `--target`). Optional ship in 0.4.5. |
| **`bn`** | Engine / CLI binary (today’s monolith driver; tomorrow thin orchestration over crates). |
| **`bn-lsp` / `bn-dap`** | IDE host binaries (or features) for Language Server Protocol and Debug Adapter Protocol; doors into the same Frontend/Interpret core. |
| **`bn_host_spec`** | Leaf/spec crate for HOST ABI tables consumed by semantic and docs — not http/net implementations. |
| **`bn_diag`** | Shared diagnostics taxonomy / render path (Fluent catalogs targeted for 0.4.5). |
| **`bn_ir` / `bn_frontend` / `bn_runtime` / `bn_llvm`** | Target crates owning IR, Frontend, oracle runtime, and LLVM emission respectively. |

---

## HOST and sandbox

| Term | Meaning |
| --- | --- |
| **HOST** | Language-visible platform services (`HOST.Net`, filesystem, console, web, …) exposed as capabilities/objects — not vendor keywords. |
| **HostEnv** | Runtime-bound environment for a job: providers, heap/handles, capability flags, debug hooks (**D_host**). |
| **Capabilities** | Structured allow/deny set for HOST surfaces; must align across IR validation, interpret, and wasm gates. |
| **Provider traits** | Clock, Console, FileSystem, Net, Http/Web Handler, … — interfaces bound into HostEnv; see [host-traits.md](host-traits.md). |

---

## IDE protocols

| Term | Meaning |
| --- | --- |
| **LSP** | Language Server Protocol — editor language services; diagnostics from **D3** via **F36**. |
| **DAP** | Debug Adapter Protocol — debug launch/step/variables via Interpret under debug control. |

---

## Modeling and milestones

| Term | Meaning |
| --- | --- |
| **DFD** | Data Flow Diagram — to-be models in `docs/architecture/dfd/` (DFD-0 context, DFD-1 named flows, DFD-2 per process). |
| **F-flows** | DFD-1 named flows (**F01–F36**). |
| **C-flows** | DFD-2 Control flows (**C01–C28**), including completions **C21–C23** back to Control. |
| **SM*** | Soft→hard Frontend/Backend **split** milestones (**SM0–SM7**). |
| **XM*** | Crate-**extraction** / DAG migration roadmap (**XM0–XM11**). |
| **Bucket** | Release checklist band (e.g. **0.4.4**, **0.4.5**). |
| **Fluent** | `.ftl` message catalogs for expressive diagnostic prose (proposal chosen for 0.4.5). |
| **LLVM toolchain external** | clang / ld / opt as **external entity**; `bn_llvm` emits and invokes, does not embed the full toolchain as the product boundary. |

---

## Diagnostics and stores (brief)

| Term | Meaning |
| --- | --- |
| **Diagnostic** | Structured error/warning with stable code, spans, and (target) Fluent-rendered prose — not free-form `String` on contract boundaries. |
| **D1** | Sources — loaded `.bn` module texts for the job. |
| **D2** | IR — single BN IR (draft then validated) for interpret and compile. |
| **D3** | Diagnostics — store of record for toolchain and runtime diagnostics. |
| **D4** | Process log — assembled pipeline log record. |
| **D5** | Build artifact — successful compile product handle/path. |
| **D_ast / D_sym** | Frontend stores for AST and symbols/semantic model (DFD-1). |
| **D_host** | HostEnv + heap for interpret. |

---

## See also

- [README.md](README.md) — document index
- [dfd/data-dictionary.md](dfd/data-dictionary.md) — full flow and store definitions
- [open-questions.md](open-questions.md) — unresolved decisions
