# BNDispatch Compiled ABI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the approved functional compiled BNDispatch ABI and native parity.

**Architecture:** `bn_rt` owns bounded opaque handles, tagged values, task contexts, queue execution, and synchronization. LLVM emits trampolines and lowers IR dispatch instructions to the C ABI; interpreter behavior remains the semantic reference.

**Tech Stack:** Rust, LLVM textual IR emission, Basic Next IR, existing `src/dispatch.rs`, `crates/bn_rt`, and CLI differential fixtures.

**Spec:** `docs/superpowers/specs/2026-09-05-bndispatch-abi-design.md`

## Global Constraints

- Preserve explicit `IMPORT BNDispatch` and existing interpreter semantics.
- Use bounded handles and configured dispatch limits.
- Do not expose Rust layouts or OS thread handles through the ABI.
- Keep `unsafe` limited to the existing FFI boundary in `bn_rt`.
- Every task ends with focused tests plus `cargo fmt --check` and `git diff --check`.

### Task 1: ABI value, status, and error contract

**Files:**
- Create: `crates/bn_rt/src/dispatch_abi.rs`
- Modify: `crates/bn_rt/src/lib.rs`
- Test: `crates/bn_rt/src/dispatch_abi.rs`

- [X] Define `BNDispatchStatus`, `BNValueKind`, `BNValue`, `BNDispatchError`, and bounded conversion helpers.
- [X] Export `bn_rt_dispatch_error_free` and test all status/tag values and bounded message copying.
- [X] Run `cargo test -p bn_rt dispatch_abi`.

### Task 2: Queue, ticket, task context, and lifecycle ABI

**Files:**
- Modify: `crates/bn_rt/src/dispatch_abi.rs`, `crates/bn_rt/src/lib.rs`
- Reuse: `src/dispatch.rs`, `src/config.rs`
- Test: `crates/bn_rt/src/dispatch_abi.rs`

- [X] Implement opaque handle storage for queues and tickets using bounded limits.
- [X] Implement submit, await, cancel, join, close, and idempotent ticket close.
- [X] Copy ABI argument descriptors into task-owned submission context and enforce output, timeout, and handle bounds.
- [X] Test success, timeout, cancellation-before-start, synchronization lifecycle, and stale-handle rejection.

### Task 3: LLVM task trampolines and dispatch lowering

**Files:**
- Modify: `src/llvm/runtime.rs`, `src/llvm/helpers.rs`, `src/llvm/lowering.rs`
- Test: `tests/codegen_tests.rs`, `tests/grammar/valid/build-dispatch-basic.bn`

- [X] Emit a trampoline for each supported no-argument `ASYNC FUNCTION` signature.
- [X] Lower `DispatchSubmit` and `DispatchAwait` to the ABI with tagged handle/status results.
- [ ] Preserve distinct `Error`, `NA`, and `EOF` values and output capture.
- [ ] Add codegen assertions for ABI declarations and generated trampolines.

### Task 4: Synchronization primitive ABI

**Files:**
- Modify: `crates/bn_rt/src/dispatch_abi.rs`, `src/llvm/runtime.rs`
- Test: `crates/bn_rt/src/dispatch_abi.rs`, `tests/codegen_tests.rs`

- [X] Implement Group, Barrier, Semaphore, and Mutex handles and methods.
- [X] Add LLVM declarations and lowering for their constructor and wait/lock/release calls.
- [X] Test bounded waits, invalid handles, close behavior, and permit/lock ownership.

### Task 5: Differential examples and gate evidence

**Files:**
- Create: `tests/grammar/valid/build-dispatch-reliability.bn`
- Modify: `tests/cli.rs`, `ongoing/bucket-0.4.4.md`, `ongoing/bucket-0.4.3.md`

- [X] Compile and run dispatch reliability, cellular automaton, and game tournament examples; `parallel_work.bn` is outside the declared `Queue.Async(FUNCTION() AS VOID OR Error)` signature and is rejected semantically.
- [X] Compare native output and exit status for all three dispatch fixtures; concurrent task order is normalized, and cancellation/timeout are covered by the ABI unit tests.
- [ ] Run `cargo test --all-targets`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, and `git diff --check`.
- [X] Close BN-044-006 and Gate G6: all `examples/dispatch_*.bn` fixtures pass without `BUILD_LOWERING_UNAVAILABLE`.
