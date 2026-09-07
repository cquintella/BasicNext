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

**Stdlib native modules** (BNData/DataFrame, …): direction locked in
[native-stdlib-binding.md](native-stdlib-binding.md) — move Executor special-cases
into `bn_rt` (or one-way modules exporting C ABI), catalog each symbol here
(AQ-16), and stop shipping empty `.bn` stubs for claimed APIs.

---

## Release-slice ABI rows (0.4.5)

The following rows are the concrete ABI slice currently exercised by compiled
programs. Fields marked “borrowed” are valid only for the duration stated by
the call; a callee must not retain them. A returned handle is opaque and is
closed by its owning `*_close` operation.

| Boundary | Representation | Ownership / lifetime | Failure class | Evidence |
| --- | --- | --- | --- | --- |
| BN integer `BYTE`/`INT8`/`INT16`/`INT32`/`INT64` and unsigned widths | LLVM `i8`/`i16`/`i32`/`i64`; signedness is an operation rule, not a different bit layout | Value copied; no pointer ownership | Checked language overflow/trap path, never LLVM poison | `src/llvm.rs`; `tests/codegen_tests.rs` numeric fixtures |
| BN `FLOAT32` / `FLOAT64` | LLVM `float` / `double` | Value copied | BN floating semantics; no `nsw`/`nuw` flags | `src/llvm.rs`; `tests/codegen_tests.rs` |
| BN `STRING` passed to `bn_rt` | NUL-terminated borrowed `ptr` for the call; compiler-owned literal storage or temporary buffer | `bn_rt` does not retain or free the pointer | Non-zero status is converted to a runtime diagnostic | `src/llvm/runtime.rs`; `crates/bn_rt/src/console.rs` |
| `BNValue` dispatch argument/result | `#[repr(C)] { kind: u32, flags: u32, payload: union }`; byte payload is `(const u8*, u32)` | Task copies the value; pointer payload is borrowed for task duration; result storage is caller-owned | `BNDispatchStatus`, distinct from language `Error` | `crates/bn_rt/src/dispatch_abi.rs` layout tests |
| `BNDispatchError` | `#[repr(C)] { code: u32, message: char*, message_length: u32 }` | Runtime owns allocated message until `bn_rt_dispatch_error_free`; null is accepted | Dispatch/tool failure, not a BN `Error` value | `crates/bn_rt/src/dispatch_abi.rs` |
| Opaque network/dispatch handles | LLVM/C `i64` handle | Owning close operation invalidates the handle; use-after-close returns status/diagnostic | Runtime handle failure, never undefined behaviour | `src/llvm/runtime.rs`; `tests/runtime.rs` handle fixtures |
| BNData structural frame handles | `BNDataFrameHandle = u64`; borrowed `BNDataFrameColumnView` inputs are copied into the runtime registry | `create` copies names/values; `append_*`/`select` return new owning handles; `bn_rt_dataframe_close` invalidates exactly one handle; input views are never retained | `BNDataFrameStatus`: invalid argument, invalid handle, or contract error; no undefined behavior for rejected bounds/layout | `crates/bn_rt/src/dataframe_abi.rs` ABI test; empty-frame lifecycle lowered in `crates/bn_llvm/src/llvm/vectors.rs` |
| BNMath scalar and reduction ops | Scalar: `i64`/`double` values passed and returned by value; Vector reduction: borrowed buffer `ptr` + element count `i32` | Callee borrows array slice for duration of reduction call; does not retain or free buffer | Return status/NaN on empty or domain errors | `crates/bn_rt/src/stats.rs`; `crates/bn_llvm/src/llvm/math.rs` |
| Temporal and string helpers (`bn_rt_print_*`, `bn_rt_str_*`) | Dates/times passed as scalar `i32`/`i64`; strings passed as borrowed C `ptr` | Callee borrows pointer for duration of query/print; does not free or mutate | Status or fallback representation | `crates/bn_llvm/src/llvm/functions.rs`; `crates/bn_rt/src/lib.rs` |

### Complete `bn_rt` Symbol Ownership for LLVM-Emitted Symbols

The table below catalogs ownership and lifetime for all symbols declared in `BN_RT_DECLS` and `BN_RT_MATH_DECLS`:

| Symbol Group | Symbols | Parameter Ownership | Return / Out-Parameter Ownership | Invalidation / Lifetime |
| --- | --- | --- | --- | --- |
| **Execution Policy** | `bn_rt_policy_init(mode, mask)` | Scalars copied | Return code `i32` (0 = OK) | Process-wide static policy; immutable after init |
| **DataFrame Lifecycle** | `bn_rt_dataframe_create`, `bn_rt_dataframe_row_count`, `bn_rt_dataframe_column_count`, `bn_rt_dataframe_close` | `ptr` views borrowed for duration of call; handle `i64`/`u64` copied | Out pointer receives owned `u64` handle or count scalar | `bn_rt_dataframe_close` destroys frame in registry; subsequent calls fail with invalid handle status |
| **Console & Clock** | `bn_rt_clock_now`, `bn_rt_clock_timer`, `bn_rt_console_cls`, `bn_rt_console_beep`, `bn_rt_console_print_at`, `bn_rt_console_num_cols`, `bn_rt_console_num_rows` | Scalars copied; string `ptr` in `print_at` borrowed | Timestamp `i64` or status `i32` | No retained state; ephemeral duration of call |
| **Network Endpoints & Handles** | `bn_rt_net_address_parse`, `bn_rt_net_ping`, `bn_rt_net_reverse`, `bn_rt_net_neighbor`, `bn_rt_net_resolve`, `bn_rt_net_addresses_*`, `bn_rt_net_handle_close` | String `ptr` borrowed; out pointers caller-allocated | Out pointer populated; handles returned by value | Address lists freed by `bn_rt_net_addresses_free`; socket handles closed by `bn_rt_net_handle_close` |
| **TCP / UDP Streams** | `bn_rt_net_tcp_*`, `bn_rt_net_udp_*` | Buffer `ptr` borrowed for call duration; handles copied | Out pointer receives bytes transferred or new stream handle | Streams/listeners invalidated by `bn_rt_net_handle_close`; UDP packets borrowed/copied |
| **Dispatch & Concurrency** | `bn_rt_dispatch_queue_*`, `bn_rt_dispatch_submit`, `bn_rt_dispatch_await`, `bn_rt_dispatch_cancel`, `bn_rt_dispatch_ticket_close`, `bn_rt_dispatch_group_*`, `bn_rt_dispatch_barrier_*`, `bn_rt_dispatch_semaphore_*`, `bn_rt_dispatch_mutex_*` | Function pointer and context `ptr` borrowed; queue/ticket handles copied | Ticket handle or completion status returned | Tickets closed by `bn_rt_dispatch_ticket_close`; synchronization primitives closed by matching `*_close` |
| **BNMath Scalars & Temporal** | `bn_rt_math_iabs`, `bn_rt_math_isign`, `bn_rt_math_imin`, `bn_rt_math_imax`, `bn_rt_math_fabs`, `bn_rt_math_fsign`, `bn_rt_math_floor`, `bn_rt_math_ceil`, `bn_rt_math_trunc`, `bn_rt_math_exp`, `bn_rt_math_log*`, `bn_rt_math_sin`, `bn_rt_math_cos`, `bn_rt_math_tan`, `bn_rt_math_asin`, `bn_rt_math_acos`, `bn_rt_math_atan*`, `bn_rt_math_sqrt`, `bn_rt_math_pow`, `bn_rt_math_hypot`, `bn_rt_math_fmin`, `bn_rt_math_fmax`, `bn_rt_math_round`, `bn_rt_math_fma`, `bn_rt_math_todate`, `bn_rt_math_totime`, `bn_rt_math_totimestamp` | Scalars copied by value | Return value computed and returned by value | Pure mathematical functions; no heap or persistent lifetime |
| **BNMath Vector Reductions** | `bn_rt_math_vmin_*`, `bn_rt_math_vmax_*`, `bn_rt_math_mean_*`, `bn_rt_math_median_*`, `bn_rt_math_quartile1_*`, `bn_rt_math_quartile3_*`, `bn_rt_math_range_*`, `bn_rt_math_stdev_*`, `bn_rt_math_variance_*`, `bn_rt_math_mode_*` | Array `ptr` borrowed for call; length `i32` copied | Scalar reduction value returned; mode writes into caller-owned buffer | Read-only slice access; callee neither mutates nor frees the buffer |
| **String Operations** | `bn_rt_str_len`, `bn_rt_str_index`, `bn_rt_str_eq`, `bn_rt_print_date`, `bn_rt_print_time`, `bn_rt_print_float` | `ptr` borrowed for call duration | Scalar result or borrowed substring `ptr` | Pure operations on immutable string buffers |

The layout assertions cover the release slice on every supported target by
checking field offsets and alignment rather than baking a host pointer width
into the language contract. Interpreter `Value` remains a private tagged
representation; it is not ABI-visible.

## Not a closed ABI manual

The rows above close only the listed 0.4.5 slice. The structural DataFrame
row is an ABI foundation and has tested ownership/handle semantics, but is not
yet a claimed compiled-language feature until LLVM lowering and parity fixtures
are present. Execution policy bit coverage for compiled targets is currently
enforced across active `bn_rt` entry points for `POLICY_CLOCK`, `POLICY_CONSOLE`,
`POLICY_NET`, and `POLICY_DISPATCH`. `POLICY_FILESYSTEM` and `POLICY_RANDOM` are
currently enforced interpret-only via HostEnv / Capabilities; native compilation
does not yet lower filesystem/random HOST ops to `bn_rt` native calls. Unlisted BNData/DataFrame
native symbols, full network ownership tables, and the final extracted-crate
ABI remain open; do not treat this document as a complete ABI manual.

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
