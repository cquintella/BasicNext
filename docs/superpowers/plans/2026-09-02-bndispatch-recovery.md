# BNDispatch 0.4 Recovery Implementation Plan

> **For agentic workers:** Execute this plan task-by-task with focused tests and repository gates after the final task.

**Goal:** Restore BNDispatch conformance for worker lifecycle, ticket state, synchronization, configuration, and async isolation.

**Architecture:** Keep the provider dependency-free and safe, but make queue ownership explicit: the queue owns its sender and worker join handles, tickets expose monotonic transitions, and synchronization primitives maintain their own ownership/capacity invariants. Async tasks continue to receive a cloned module and a fresh executor; the host environment gets an independent task seed.

**Tech Stack:** Rust standard library synchronization and threads, existing BN runtime/module provider, TOML registry embedded by `build.rs`, Cargo integration tests.

**Spec:** `docs/superpowers/specs/2026-09-02-bndispatch-recovery-design.md` and `ongoing/0.4-concurrency-decision.md`.

## Global Constraints

- Do not add dependencies or `unsafe` code.
- Preserve explicit `ASYNC`/`AWAIT` grammar and synchronous BNWeb behavior.
- Use the versioned `config/0.4-bnweb-limits.toml` as the source of dispatch thresholds.
- Do not modify unrelated pending changes under `docs/book/en/`.
- Every slice adds a focused regression test before implementation and keeps diagnostics bounded.

### Task 1: Lock the queue lifecycle and ticket state invariants

**Files:**
- Modify: `src/dispatch.rs` tests and queue/ticket types
- Test: `src/dispatch.rs`

**Interfaces:**
- Produces legal transitions `PENDING -> RUNNING/CANCELLED -> COMPLETED/FAILED`.
- Produces a queue close path that can own sender disconnect and worker joins.

- [ ] Add tests for cancellation-before-start, panic-to-failure with later work, close idempotence, and close deadline while a running task is blocked.
- [ ] Run `cargo test dispatch::tests --lib` and observe failures from the current worker/lifecycle implementation.
- [ ] Implement ticket transition guards, worker panic conversion, explicit sender shutdown, worker handle retention, and deadline-aware joining.
- [ ] Run `cargo test dispatch::tests --lib` and confirm the focused suite passes.

### Task 2: Make pending capacity reflect live work

**Files:**
- Modify: `src/dispatch.rs`
- Test: `src/dispatch.rs`

**Interfaces:**
- Queue capacity tracks only non-terminal tickets.
- Terminal ticket status/error/output remains available through the ticket handle.

- [ ] Add a test that completes more than 1,024 tasks sequentially and a test that rejects simultaneous live pending work.
- [ ] Run the focused tests and observe the retained-history saturation failure.
- [ ] Replace the permanent ticket vector used for capacity with a live-ticket registry and remove each ticket exactly once at terminal transition.
- [ ] Preserve `Ticket.Close` failure diagnostics while releasing task/output payload according to the recovery design.
- [ ] Run the focused queue and runtime ticket tests.

### Task 3: Correct synchronization primitive semantics

**Files:**
- Modify: `src/dispatch/sync.rs`, `src/runtime/executor/part3.rs`
- Modify: `modules/bn/BNDispatch.bn`, `docs/language/0.4/0.4.md`, `docs/language/0.3/bndispatch.md`
- Test: `src/dispatch/sync.rs`, `tests/runtime.rs`

**Interfaces:**
- `Group.Leave`, `Semaphore.Release`, and `Mutex.Unlock` return `VOID OR Error`.
- Barrier timeout breaks its generation; foreign mutex unlock, excess release, and group underflow return typed errors.

- [ ] Add concurrent barrier timeout tests and misuse tests for group, semaphore, and mutex.
- [ ] Run the focused tests and observe current false-success/overflow/no-owner behavior.
- [ ] Implement broken barrier generations, capped permits, logical mutex ownership, and checked group count.
- [ ] Wire new errors through the runtime provider and update the 0.4 module contract and examples.
- [ ] Run synchronization Rust and runtime tests.

### Task 4: Move all dispatch thresholds to typed configuration

**Files:**
- Modify: `config/0.4-bnweb-limits.toml`, `src/config.rs`, `src/dispatch.rs`, `src/runtime/executor/part3.rs`
- Test: `src/config.rs`, `src/dispatch.rs`, `tests/runtime.rs`

**Interfaces:**
- `DispatchLimits` owns worker, pending-ticket, timeout, and output bounds.
- Queue and synchronization constructors consume validated limits without `web_limits()` coupling.

- [ ] Add registry/default/max parity tests for the dispatch section.
- [ ] Run configuration tests and confirm the current hardcoded/coupled values are not represented by a typed dispatch snapshot.
- [ ] Add dispatch fields to the registry, parse them into `DispatchLimits`, and pass the snapshot at provider construction.
- [ ] Remove dispatch resource literals and direct BNWeb-limit access from the provider.
- [ ] Run config, dispatch, and async runtime tests.

### Task 5: Prove executor and host isolation

**Files:**
- Modify: `src/runtime_impl.rs`, `src/runtime/executor/part3.rs`
- Test: `tests/runtime.rs`, `src/runtime/tests.rs`

**Interfaces:**
- Each task receives an independent module/runtime/heap/output and independent host-random seed.

- [ ] Add an async integration test with multiple tasks that mutate local values and emit distinct output.
- [ ] Add a host-random isolation test that distinguishes task streams from the caller stream.
- [ ] Run the new tests against the current clone behavior and record the seed-duplication failure if reproduced.
- [ ] Make task-host cloning derive independent random state without changing fixed-host determinism for ordinary execution.
- [ ] Run the focused runtime isolation suite.

### Task 6: Re-run release evidence and close only with proof

**Files:**
- Modify: `ongoing/bucket.md`, `ongoing/WBS-0.4.md`, `ongoing/0.4-conformance.md`, `done/0.4-release-news.md`, `README.md`, `examples/parallel-examples.md`
- Test: all Rust and plugin gates

- [ ] Run the three new BNDispatch examples and their exact-output runtime tests.
- [ ] Run `cargo fmt --check`, `cargo test --all-targets -- --test-threads=1`, `cargo clippy --all-targets -- -D warnings`, and `git diff --check` with unrelated whitespace changes isolated and reported.
- [ ] Run the VS Code and Jupyter checks required by the 0.4 release record.
- [ ] Update recovery evidence and mark Activity 5.1/G5/G7 done only if every acceptance criterion is evidenced.
