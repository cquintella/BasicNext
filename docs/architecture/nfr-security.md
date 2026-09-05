# NFR / security — architecture view

> Canonical: `docs/architecture/nfr-security.md`  
> Status: **index + architecture implications** — **2026-09-05**  
> Does **not** invent a new full threat model.

Security and non-functional hardening for BasicNext (especially BNWeb / HOST
network surfaces) already have canonical working documents. Architecture
docs should **point to** those sources and state only the toolchain-structure
implications: where controls live in the DAG, which stores and flows must not
leak secrets, and which trust boundaries the DFDs already name.

---

## Canonical sources (do not duplicate)

| Document | Role |
| --- | --- |
| [`../../ongoing/0.4-threat-model.md`](../../ongoing/0.4-threat-model.md) | Working **threat model** and resource policy for 0.4 BNWeb hardening |
| [`../../ongoing/0.4-security-register.md`](../../ongoing/0.4-security-register.md) | Active **security register** (findings, mitigations, residual risk) |

If a control’s evidence or severity changes, update the register / threat
model — not this page. This page only maps those controls onto the target
architecture (Control, Frontend, Interpret, Compile, process log).

---

## Architecture-relevant controls

The following are the implications architects and crate-extraction work must
respect. Each maps to processes or stores on the to-be DFDs.

### 1. Sandbox via Capabilities / HostEnv

Interpret (**4.0**) binds **Capabilities** into **HostEnv** before executing
BN IR. Capability denial and “provider unavailable” results must be
deterministic and shared with IR validation / wasm gates — not a separate
ad-hoc check inside one HOST crate. See [host-traits.md](host-traits.md) and
DFD-2 **4.1**.

Threat-model themes: capability confusion / downgrade (e.g. T-10), explicit
imports, no silent TLS-to-cleartext fallback.

### 2. Wasm and feature gates

Compile-to-wasm and optional feature sets must read the **same** capability
story as the oracle. A binary that omits host-web or llvm still must not
claim HOST surfaces it cannot enforce. Success criteria in
[target-architecture.md](target-architecture.md) call this out explicitly.

### 3. No secrets in the process log

Process **6.0** / companion `.bnbuild.log` is a forensic record of pipeline
phases, safe config snapshots, diagnostic summaries, and external tool argv.
It must **not** record tokens, cookie values, TLS private material, raw
request bodies, or other secrets. Prefer redaction and bounded records
aligned with BNLog / threat-model log rules (e.g. T-06). Control events
(**F07**, **C20**, **C28**) and compile argv (**F25**) are in scope for this
discipline.

### 4. External LLVM trust boundary

The **LLVM toolchain** (clang, ld, opt) is an **external entity** on DFD-0/1
(**F25** / **F26**). `bn_llvm` emits IR and invokes tools; it does not absorb
the full LLVM libraries as the product boundary. Architecture treats tool
exit status and stderr as inputs to diagnostics and the process log — not as
a second semantic oracle. Supply-chain and “what binary did we link?”
questions belong to build forensics + documented argv, not to language
meaning.

### 5. Plugins directory reserved

`--plugins-dir` is **reserved in MVP**: recorded in config/log, **no dynamic
load** until a plugin ABI and allowlist exist. Loading code early recreates
supply-chain risk called out in the pipeline-observability critique. Control
**1.3** resolves the path; it does not execute plugins.

### 6. HOST / BNWeb policy stays outside language core

SSRF classification, admission limits, session entropy, and stop/drain rules
live in host/web implementation and the 0.4 register — not as new IR opcodes
or Frontend forks. Runtime must call one validated policy layer; Frontend
only needs `bn_host_spec` catalogs, not http internals.

---

## Mapping to DFD stores and externals (short)

| Architecture element | Security-relevant note |
| --- | --- |
| **D3 Diagnostics** | User-facing; may mirror into log — still no secret payloads |
| **D4 Process log** | Forensics; redact; not a reproducibility certificate |
| **D_host HostEnv** | Capability + provider binding surface |
| **E4 LLVM EXTERNAL** | Trust boundary for codegen/link only |
| **Editor IDE (LSP/DAP)** | Same Frontend/Interpret controls as CLI; no weaker sandbox story |

---

## Related architecture docs

- [host-traits.md](host-traits.md) — HostEnv / Capabilities sketch
- [sequences.md](sequences.md) — where interpret/compile/log run in profile order
- [open-questions.md](open-questions.md) — unresolved product locks that may affect controls
- [target-architecture.md](target-architecture.md) — DAG and success criteria
