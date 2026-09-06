# Native stdlib binding (architecture)

> Canonical: `docs/architecture/native-stdlib-binding.md`  
> Status: **Direction locked 2026-09-06.** Carlos subsequently approved **AQ-22
> alternative B: EXTERN with explicit stdlib and foreign-C profiles**. Concrete
> grammar and implementation remain delivery work. Empty `.bn` stubs with hardcoded Executor logic are
> **not** an acceptable production shape (production bar).

## Problem (as-is)

Some standard modules (notably **BNData** / DataFrame) ship `.bn` files with
empty or fake bodies (`RETURN 0`, etc.) while real behaviour lives hardcoded in
the interpreter (`runtime/executor/part8.rs` and related). That was a fast PoC;
it breaks the architecture story:

- Semantic analysis and the `.bn` module **lie** about behaviour.
- The **compile** path cannot share meaning with interpret without a second
  fork of the same logic.
- Executor knowledge of CSV / tables / provider name strings couples
  `bn_runtime` to module specifics.

## Locked direction

### 1. Shared native ABI (`bn_rt`) — parity first

Move stdlib native logic out of Executor special-cases into **`bn_rt`** (or a
one-way module/crate that **exports a C ABI** consumed through `bn_rt`), as
**Rust-owned** code independent of AST/IR.

- Export a **stable C ABI** (symbol names + ownership rules), e.g. illustrative:
  `bn_rt_dataframe_read_csv(...)`, `bn_rt_dataframe_mean(...)`.
- **`bn run` (interpret)** and **`bn build` (LLVM + link)** must call the
  **same** static library / symbol set for the claimed support subset.
- Each symbol enters the **value/memory/ABI catalog** ([value-memory-abi.md](value-memory-abi.md),
  AQ-16): layout, borrow/transfer/free, Error taxonomy.

Sharing helpers expands the existing pattern in value-memory-abi (“prefer
shared `bn_rt` when behaviour is a HOST/native boundary”). Interpreter
`Value` layouts may still differ; **observables** must not.

**Dataframe crate split** remains **AQ-05**: `bn_value` first; a dedicated
`bn_dataframe` crate only when deploy weight justifies it. Until then, ops may
live under runtime/`bn_rt` **without** recreating the Value cycle (XM2).

### 2. Honest module surface (no fake bodies)

Production modules must not simulate returns. Prefer one of:

| Option | Role |
| --- | --- |
| **A. Native binding declarations** | Approved EXTERN mechanism with an explicit stdlib profile; signatures bind to cataloged `bn_rt_*` symbols. Concrete grammar still requires specification and fixtures. |
| **B. Real BN bodies** | Pure stdlib written in BN when no native code is needed |

Semantic validates types at compile time; lowering emits **external calls**
(normal linker symbols). Interpret resolves the same symbols via HostEnv /
`bn_rt` wrappers.

**Do not conflate** this with user-facing **C FFI** (`HOST.c` + `EXTERN` in
[`../../todo/proposals/c-ffi.md`](../../todo/proposals/c-ffi.md)):

| Concern | Stdlib native binding (this doc) | Foreign C FFI (`c-ffi` proposal) |
| --- | --- | --- |
| Who owns the impl | BN/`bn_rt` | Third-party / OS / adapter `.a`/`.so` |
| Audience | Standard modules (BNData, …) | Program imports `HOST.c` |
| Types | May include BN handles/`Error` per ABI catalog | Profile 1: fixed-width C types only |
| Load | **Static link** of `bn_rt` first | Logical library name; dynamic load later |

**AQ-22 approved by Carlos:** use one EXTERN mechanism with explicit stdlib and
foreign-C profiles, rather than a second stdlib declaration form. Stdlib symbols
resolve through the owned bn_rt catalog; foreign C does not implicitly inherit
stdlib privileges or BN-handle access. The historical HOST.c sketch is not
automatically accepted grammar.

Before declaration implementation, finish functions, classes, constructors,
methods, type mappings, ownership, symbols, errors and profile restrictions.
Update active specification, EBNF, keyword registry and positive/negative
fixtures together. Architecture approval does not make EXTERN currently valid
BN syntax. Complete real BN bodies remain valid where native code is unnecessary.

### 3. HostEnv provider traits (interpret wiring)

Interpret (**4.1 Bind HostEnv**) injects **provider traits** so the Executor
does not hardcode CSV/table strings ([host-traits.md](host-traits.md)).

Sketch (architectural, not API freeze):

```text
trait DataProvider { … read_csv / mean / … }
HostEnv binds DataProvider (+ policy); Executor calls the trait only.
```

Compiled images do **not** need Rust traits: they call **`bn_rt_*`** with the
same ownership/Error rules and **policy re-check** at the call boundary
(AQ-17 approved: embedded permission ceiling plus execution-time restrictions).

Provider traits are **Rust DAG wiring**, not new language DNA.

### 4. Dynamic plugins — deferred

| Surface | Status |
| --- | --- |
| **Static `bn_rt` link** for parity | **Locked** — first maturity step |
| **`--plugins-dir`** | **Reserved** for **toolchain** plugins; MVP **no load** ([module-path.md](module-path.md), target-architecture) |
| **User/package `.so` / `.dylib` / `.dll`** | **Deferred** — after static parity + ABI catalog; no platform paths in `.bn` source; follow c-ffi logical-library direction |

Claiming “100% behavioural parity” requires the shared static ABI first.
Dynamic loading is an adoption path, not the 0.4.5 correctness gate.

## Non-goals

- Changing FE → IR → interpret(IR) | compile(IR).
- Mixing toolchain diagnostics (Fluent / `bn check`) with program `Error`.
- Freezing AQ-22 syntax in this file.
- Shipping dynamic plugin load in MVP.

## Production bar

Empty `.bn` stubs + Executor special-cases for announced stdlib APIs are
**provisional** and **unacceptable** once a module is claimed supported on
interpret and/or llvm. Carlos's approved completion rule requires full delivery
of existing functionality even if effort increases. Matrix reductions must not
hide incomplete APIs or remove relied-on behavior. Each API needs declaration,
analysis, IR, runtime, advertised backends, diagnostics and tests connected.
Code extraction or interface creation alone does not close the activity.

## Bucket / milestones

Implementation work lands primarily under **0.4.5** §2b (ABI / `bn_rt`
catalog + shared helpers) and follow-on activities for stdlib binding —
see [`../../ongoing/bucket-0.4.5.md`](../../ongoing/bucket-0.4.5.md).
Crate packaging still follows XM2 / AQ-05.

## See also

- [host-traits.md](host-traits.md) — HostEnv, providers, policy
- [value-memory-abi.md](value-memory-abi.md) — symbol ownership / Error classes
- [support-matrix.md](support-matrix.md) — claimed support evidence
- [module-path.md](module-path.md) — modules vs `--plugins-dir`
- [open-questions.md](open-questions.md) — AQ-05, AQ-16, AQ-17, AQ-22
- [`../../todo/proposals/c-ffi.md`](../../todo/proposals/c-ffi.md) — foreign C FFI
