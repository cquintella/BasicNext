# Value, memory, and ABI contract (to-be)

> Canonical: `docs/architecture/value-memory-abi.md`  
> Status: **requirements checklist locked; contract content PARTIAL / NOT CLOSED (2026-09-05).** This file states *what* must be contracted. It does **not** yet specify concrete layouts or per-symbol ownership for `bn_rt`. Fill the **next release subset** with tables + tests before claiming closed — [review-status.md](review-status.md), AQ-16.

## Why this exists

Extracting **`bn_value`** fixes *Rust* coupling (e.g. dataframe ↔ runtime), but it does **not** by itself establish **equivalence** between objects in the interpreter and objects in a native/wasm image linked with **`bn_rt`**.

Today the architecture largely treats `bn_rt` as “Keep” with an interface that looks like a bag of extern calls ([target-architecture.md](target-architecture.md) contracts table). That is necessary but insufficient: without an explicit **value / memory / ABI** contract, interpret and compile can drift on identity, lifetime, dispatch, layout, and error taxonomy while still “sharing IR.”

## Hierarchy (same as conformance)

1. **Language specification** defines observable behaviour (including `Error`, traps, `DELETE`, static init, numeric rules).
2. **Executable reference** (`bn_runtime` + interpreter `Value`/`bn_value`) implements that behaviour for tests.
3. **Compiled path** (`bn_llvm` + **`bn_rt`** + layout) must match the specification on the [support-matrix.md](support-matrix.md) subset — see [conformance.md](conformance.md).

Internal representations **may differ** between interpret and native (tagged heap vs structs/pointers). **Observable** identity, aliasing, lifetime, dispatch, and ABI results must not.

## Sharing `bn_rt` with the interpreter

There is already useful sharing of **`bn_rt`** helpers from the interpreter (e.g. clock and console). That pattern should be **expanded when it clarifies a single HOST/ABI truth**, without forcing identical in-memory layouts:

| Prefer shared `bn_rt` (or thin wrappers) when… | Prefer distinct internals when… |
| --- | --- |
| Behaviour is a HOST/native boundary (time, console I/O, math that must match linked binaries) | Representation is an interpreter optimization (tagged `Value`, GC/arena details) |
| Conformance tests would otherwise fork two copies of the same syscall story | Layout is only meaningful after LLVM emission |

Sharing helpers ≠ claiming that interpreter `Value` bits equal native object bits.

---

## Not a closed ABI manual

Until each area below has **concrete** layout/ownership rows (or an explicit carve-out) and tests for the claimed support subset, treat this document as a **requirements index**, not a finished ABI specification.

## Required contract areas (normative checklist)

The toolchain **must** document and test the following. Gaps here are architecture defects, not “implementation detail.”

### 1. Identity, copy, and aliasing

For objects, vectors, strings, and other language values:

- When two names / handles refer to the **same** object (aliasing) vs a **copy**.
- What assignment, parameter passing, and return do (share vs copy) per `0.4.md`.
- How vectors/strings behave under index update, concatenation, and slice-like operations the language defines.
- Equality vs identity where the language distinguishes them.

Interpret and compile must agree on these observables for the support subset.

### 2. Construction, destruction, `DELETE`, and handle validity

- How values are **constructed** (defaults, constructors, static fields).
- How **`DELETE`** (and any related disposal) affects handle validity.
- When using a handle after delete / move is a **language trap** vs undefined/internal failure.
- Interaction with HOST resources (files, sockets) if a handle wraps them — deny/use-after-close rules.

### 3. Method dispatch, interfaces, and static initialization

- How method / interface dispatch selects the implementation (vtable, dictionary, IR-level call targets).
- Obligations of `IMPLEMENTS` / interface conformance at runtime for both backends.
- **Static initialization** order, cycles (`STATIC_INITIALIZATION_CYCLE` and related), and when init runs relative to `Start` / module load — same story for interpret and linked binaries.

### 4. Layout, alignment, and representation at native boundaries

For the compile path (and any FFI/`bn_rt` surface):

- Documented **layout and alignment** of BN values that cross into native code (structs, vectors headers, string representation, fat pointers, etc.).
- Which IR types map to which C/LLVM types in `bn_rt`.
- What is **ABI-visible** vs private to the interpreter heap.

Interpret need not use the same layout *internally*, but any value that is defined to be ABI-visible must round-trip / observe consistently when both paths exercise the same HOST/ABI helper.

### 5. Ownership of ABI arguments and results

For every `bn_rt` / extern entry used by lowering:

- Who **owns** pointer arguments (borrow vs transfer).
- Who frees results; aliasing with callee-stored pointers.
- Thread-/reentrancy constraints where relevant.
- No silent double-free or leak that the language model would forbid.

The “known extern call set” in the LLVM ↔ `bn_rt` row is the **index**; each entry needs these ownership rules, not only a symbol name.

### 6. `Error` vs language trap vs internal runtime failure

Three distinct classes (names may map to diagnostic codes / exit paths):

| Class | Meaning | Typical handling |
| --- | --- | --- |
| **`Error` (language)** | First-class / documented error value or `OR Error` result | Program-visible; may be returned/propagated per language rules |
| **Language trap** | Violation of a language dynamic rule (e.g. invalid handle use, banned operation) | Abort or documented trap semantics — **not** silently turned into `poison` or UB |
| **Internal runtime / toolchain failure** | Bug or invariant break inside interpreter, `bn_rt`, or linker glue | Toolchain diagnostic / abort; must not be confused with a normal `Error` value |

Compile must not map language traps to LLVM undefined behaviour without an explicit, tested lowering that preserves the language meaning.

---

## Numeric lowering obligations (interpret ↔ LLVM)

Numeric behaviour is part of the language contract ([numeric-semantics.md](../../todo/proposals/numeric-semantics.md), `0.4.md`). Lowering to LLVM must state, for each op:

- Whether overflow/underflow is a **language error/trap**, wrapping, saturating, or unspecified — **as the BN spec says**.
- How that maps to LLVM instructions and flags.

### Example (non-negotiable warning): `add nsw` ≠ BN overflow error

LLVM’s `add` with the **`nsw`** (no signed wrap) flag means: if signed overflow occurs, the result is **poison** — not a structured BN `Error`, and not a defined trap by itself. See the official LangRef:

- [LLVM Language Reference — `add` instruction](https://llvm.org/docs/LangRef.html#add-instruction)

Therefore:

- Emitting `add nsw` (or `nuw`) **does not** automatically implement “BN integer overflow → language error.”
- If BN requires a checked overflow, lowering must emit an **explicit** check / intrinsic / `bn_rt` helper whose failure path matches the language (`Error` or trap), and conformance tests must cover it on **both** backends.
- If BN defines wrapping, lowering must use ops that match wrapping — not `nsw` “and hope.”

Poison, `undef`, and similar LLVM concepts are **toolchain hazards**; they are not synonyms for BN `Error`.

---

## Relation to crates

| Crate | Role under this contract |
| --- | --- |
| **`bn_value`** | Interpreter-facing value/handle payloads; extract breaks Rust cycles — **not** a substitute for ABI equivalence docs |
| **`bn_runtime`** | Executable reference heap/dispatch; may call shared `bn_rt` helpers |
| **`bn_rt`** | Native helpers / ABI surface for linked images; documented ownership + layout |
| **`bn_llvm`** | Must lower IR respecting this contract and the numeric obligations above |
| **`bn_ir`** | IR ops that imply value semantics must be interpretable under this contract |

## Conformance expectation

Per [conformance.md](conformance.md): fixtures for identity/aliasing, `DELETE`/handles, dispatch/static init, ABI round-trips, and numeric overflow must run on **interpret** and on **compile** (support-matrix filtered), plus cross-backend comparison where both apply.

Policy denials (unauthorized HOST op) are **not** language `Error` values unless the language defines them that way; they are execution-policy failures — see [host-traits.md](host-traits.md).

## See also

- [conformance.md](conformance.md)
- [support-matrix.md](support-matrix.md)
- [ir-contract.md](ir-contract.md)
- [host-traits.md](host-traits.md)
- [target-architecture.md](target-architecture.md) (contracts table)
- [LLVM LangRef — `add`](https://llvm.org/docs/LangRef.html#add-instruction)
