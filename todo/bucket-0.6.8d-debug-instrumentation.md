# Bucket 0.6.8d — Debug Infrastructure: Instrumentation Layer, Tracer, Profiler (from 0.6.4 plan)

**Status:** Deferred schedule **0.6.4** (Carlos 2026-09-21 invert: was planned as 0.6.1; BNCrypto takes 0.6.1). Planning origin 2026-09-17 (debug / DAP / instrumentation on `bni`).

**Gate:** Carlos. **Dependencies:** Bucket **0.6.0** (`bni`/`bnc` split) completed first; scheduled **after** 0.6.1–0.6.3 feature buckets unless Carlos pulls it forward again.

**Schedule note (2026-09-21):** Carlos invert — this debug bucket is **0.6.4**. Order: 0.6.0 binaries → **0.6.1** BNCrypto → **0.6.2** Sqlite/FS/Auth → **0.6.3** ORM → **0.6.4** debug.

**Spec check (AGENTS.md rule):** **No language surface change.** `language/0.5/*` unchanged; `HOST` catalog unchanged; IR structure unchanged. What changes is **runtime observability** — the interpreter gains an event-emission layer; the `bn_ir::Debug*` types gain a stable definition for spans and provenance. The book section on debugging (`docs/book/en/chapter-10-debugging.md` — new chapter) documents the toolchain, not the language. Implementation adds `bn_runtime_instrumentation` and redefines `bn_dap` to use it; no breaking change to semantics.

**Testing policy (Carlos, 2026-09-17):** One test battery per sprint; tests run sequentially (`cargo test --workspace --one-at-a-time`; see bucket-0.5.1e). CLI acceptance per sprint. Negative event fixtures for instrumentation (GC-INSTR-NEG). Parity: `test_interpreter_event_chain.py` ensures interpret-side events match recorded traces. Cross-tool: debugger breakpoint hits must align with tracer event lines.

---

## SECTION 0 — Verified starting point (2026-09-17, post-0.6.0)

| Fact | Measure | Evidence |
| --- | --- | --- |
| **Interpreter exists** | `crates/bni/src/main.rs` calls `bn_interp::execute_validated()` | `cargo tree -p bni \| rg 'bn_interp'` |
| **DAP protocol layer** | `src/dap.rs` 575 lines; implements DAP JSON-RPC; drives `execute_validated_with_host_debug_control()` | `src/dap.rs` lines 1–50 (session mgmt) |
| **DebugFrame struct** | Stack capture: function name, depth, line, variables | `src/dap.rs` lines 16–21 |
| **Debug hook invoked** | `execute_validated_with_host_debug_control()` callback at each instruction; hook sees (function, depth, span, variables) | `src/dap.rs` lines 252–325 (execute_program) |
| **Runtime support** | `crates/bn_runtime/src/lib.rs` exports `DebugVariable`, `DebugDecision` | `cargo tree -p bn_runtime` |
| **Breakpoints** | Registered by DAP, checked at hook time (line match in `breakpoints` registry) | `src/dap.rs` lines 280–286 |
| **Variable capture** | Hook collects variables from interpreter frame; DAP serves them via `variables` command | `src/dap.rs` lines 381–396 (variables_response) |
| **Diagnostic registry** | `crates/bn_diag` 60+ error codes; no debug events yet | `crates/bn_diag/src/lib.rs` lines 84–150 |
| **IDEs** | VS Code extension and Jupyter kernel launch `bni dap`; operate via JSON-RPC stdin/stdout | `docs/architecture/driver-sequence.md` |

**Today's gap:** The debug hook is **triggered reactively** (at breakpoints) and captures **snapshots** (frame, variables at pause time). There is **no event stream** — no record of intermediate steps, variable writes, function entry/exit, or error context. Profiling and tracing cannot be done without the stream.

---

## SECTION 1 — Target shape

```
┌─────────────────────────────────────────────────────┐
│ VS Code (Debugger UI)                               │
└────────────┬────────────────────────────────────────┘
             │ JSON-RPC DAP
             ↓
┌────────────────────────────────────────────────────────┐
│ bni (interpreter binary, 0.6.0)                        │
├────────────────────────────────────────────────────────┤
│ bn_dap (library)                                       │
│   ↓ uses ↓                                             │
│ DebuggerConsumer (breakpoints, step, stack, inspect)  │
├────────────────────────────────────────────────────────┤
│ bn_runtime_instrumentation (NEW)                       │
│   ├─ Event enum: FunctionEntry, Statement, LineChange,│
│   │              VariableWrite, VariableRead,         │
│   │              FunctionReturn, Error, …             │
│   ├─ EventEmitter (callback interface)                │
│   └─ Span + provenance info (source location, IR node)│
├────────────────────────────────────────────────────────┤
│ bn_interp (interpreter core)                          │
│   ├─ execute_validated() calls EventEmitter           │
│   ├─ at every statement, variable assign, call, ret   │
│   └─ hook decides: record to tracer, break, profile   │
├────────────────────────────────────────────────────────┤
│ Consumers (pluggable)                                 │
│   ├─ DebuggerConsumer: breakpoints, step, stack       │
│   ├─ TracerConsumer: record events to file/memory     │
│   └─ ProfilerConsumer: measure function time, calls   │
└────────────────────────────────────────────────────────┘

Compiled path (bnc):
┌─────────────────────────────────────────────────────┐
│ GDB / LLDB                                           │
└────────────┬────────────────────────────────────────┘
             │ MI (machine interface) or DAP (native)
             ↓
┌────────────────────────────────────────────────────────┐
│ bn debug-metadata extractor (NEW)                      │
│   reads IR + DWARF + bn-specific metadata file         │
│   ↓ serves to GDB ↓                                    │
│ GDB bridge module (NEW): maps GDB requests to metadata │
└────────────────────────────────────────────────────────┘
```

**Key insight:** One **event source** (the instrumentation layer), three **consumers** (debugger, tracer, profiler). Each consumer is optional and pluggable. The interpreter emits *once*; each consumer decides what to do.

---

## SECTION 2 — Decision register

| ID | Decision | Recommendation | Blocks |
| --- | --- | --- | --- |
| D-061-01 | **Event abstraction.** Events are an `enum` in `bn_runtime_instrumentation` (not trait objects or plugins). Consumers register themselves before execute; the interpreter calls all registered consumers for each event. | **Yes.** Static dispatch, no runtime indirection, predictable performance; adding a consumer is a cargo feature or link-time flag, not a plugin loader. | 1.1 |
| D-061-02 | **Span & provenance in events.** Every event carries the source `Span` (file, line, col) and optional `IrNodeId` (lowered node reference). Debugger uses Span for UI line numbers; tracer records both for error context. | **Yes.** Span is already in `bn_source::Span` and lowering preserves it; IR nodes are stable references. | 1.1, 1.2 |
| D-061-03 | **Variable writes / reads as events.** When a variable is assigned or read, emit `VariableWrite` or `VariableRead` event (not just at breakpoints). Name, old value (if write), new value, type from IR. Debugger ignores most; tracer records all. | **Yes.** Needed for profiling "which var changes most?" and for tracer to reconstruct memory state. Cheap: value serialization is already done by `format_value()`. | 1.2, 3.1 |
| D-061-04 | **Function entry / return events.** Emit `FunctionEntry` on call, `FunctionReturn` on return (normal or error). Depth, function name, args, return value. Replaces today's hook-at-every-instruction with a structured call stack. | **Yes.** Profiler measures function time; tracer logs the call tree; debugger knows when a frame is created/destroyed. | 1.2 |
| D-061-05 | **Error events & rethrow.** When an error is thrown or caught, emit `ErrorThrown` and `ErrorCaught` events with the `DiagId`, message, and Span. Allows tracer to record the full fault tree; profiler to charge time to error handlers. | **Yes.** Currently errors are silent until they reach the user; events let the IDE show a live error trace. | 1.3 |
| D-061-06 | **GDB bridge scope.** Compiled code (bnc → LLVM → binary) supports GDB breakpoints and step via DWARF + a metadata file (not via DAP in GDB; that's future work). The metadata file lists function names, line ranges, variable offsets (for DWARF var lookup). Bridge is read-only; full GDB MI integration is not in scope. | **Yes.** Blocks full GDB integration but unblocks basic stepping and breakpoint-hit reporting. | 5.x |
| D-061-07 | **Tracer output format.** Events are recorded as newline-delimited JSON (one event per line) to a file; replay via `bn-tracer replay <file>` reconstructs a timeline. Metadata: commit hash, timestamp, interpreter version. | **Yes.** Simple, grep-able, streaming-friendly. Binary format (protobuf) deferred to optimization. | 2.2, 3.1 |
| D-061-08 | **Profiler aggregation.** Profiler emits a summary (not per-event: function call count, cumulative time, self time, callers) on program exit or on demand. UI: `bn profile --summarize <trace-file>`. | **Yes.** Matches the use case ("measure time of calls"). Details go into tracer; profiler is a post-processor. | 3.2 |

---

## SECTION 3 — SPRINT 1: Instrumentation foundation

**Objective:** Define the event enum, plug it into the interpreter, and verify all events are reachable (no gaps).

- [ ] **ACTIVITY 1.1 — `crates/bn_runtime_instrumentation`**

  Deliver:
  - New crate: `pub enum Event { FunctionEntry{…}, Statement{…}, LineChange{…}, VariableWrite{…}, VariableRead{…}, FunctionReturn{…}, ErrorThrown{…}, ErrorCaught{…}, … }`
  - Each variant carries: `span: Span`, `ir_node_id: Option<IrNodeId>`, `depth: usize`, variant-specific fields
  - `pub struct EventContext { function: String, module: String, … }`
  - `pub trait EventConsumer { fn on_event(&mut self, event: &Event, ctx: &EventContext); }` (trait for future extensibility; D-061-01 uses static dispatch for now, trait remains for future)
  - `pub struct EventEmitter { consumers: Vec<Box<dyn EventConsumer>> }`
  - Factory function: `pub fn create_emitter(enabled: &[&str]) -> EventEmitter` (names: "debugger", "tracer", "profiler")

  Documented:
  - Enum variant semantics in a `EVENTS.md` file (include in docs/architecture/)
  - Span guarantees (always set, always valid)
  - Consumer contract: one consumer's error should not kill the emitter

  Quality:
  - No `println!`; emitter does not log, it calls consumers
  - Consumers are responsible for buffering / filtering / side effects
  - `Value` serialization delegates to `bn_runtime::format_value()`

  Acceptance:
  - `cargo build -p bn_runtime_instrumentation` succeeds
  - `Event` enum has ≥8 variants, each carries `span` and `ir_node_id`
  - `EventEmitter::new()` is clonable for thread-safe sharing (Arc<Mutex<_>>)

- [ ] **ACTIVITY 1.2 — Integrate instrumentation into `bn_interp`**

  Deliver:
  - Modify `execute_validated()` signature: add `emitter: Option<&mut EventEmitter>` parameter (backwards compat: None = no instrumentation)
  - At each interpreter step (statement execution, variable assign, call, return), emit corresponding event:
    - **Before statement:** emit `Statement { stmt_kind, span }`
    - **Line change (Span start.line changed from prev):** emit `LineChange { new_line, new_span }`
    - **Variable assign:** emit `VariableWrite { name, old_value, new_value, var_type }`
    - **Variable read (if trackable, e.g., in hover context):** emit `VariableRead { name, value }`
    - **Function call entry:** emit `FunctionEntry { function_name, args, depth }` (replace the hook-at-every-instruction)
    - **Function return:** emit `FunctionReturn { function_name, return_value, depth }`
  - If emitter is None, interpreter behaves identically to today (zero instrumentation overhead)
  - Store current Span in interpreter context for line-change detection

  Documented:
  - Which interpreter instructions trigger which events
  - Span provenance (comes from lowering)

  Quality:
  - No logic change to execution (events are observational)
  - Event emission failures are logged but do not stop execution
  - GC-INSTR-EMIT gate: every event type is reachable by a test program

  Acceptance:
  - `cargo test -p bn_interp --lib` green with emitter changes
  - `crates/bn_interp/tests/instrumentation_gates.rs` (new) verifies all 8 event types are emitted at least once
  - Parity: `test_interpreter_parity.py` unchanged (events are off by default)

- [ ] **ACTIVITY 1.3 — Negative event fixtures (GC-INSTR-NEG)**

  Deliver:
  - `tests/fixtures/instrumentation/` directory with `.bn` programs covering:
    - Valid events (happy path)
    - Edge cases:
      - Variable write with no prior read (uninitialized → write)
      - Nested function calls (events are nested, depth increases)
      - Error in middle of statement (error event fired, function-return not fired)
      - Recursive function (depth increments/decrements correctly)
    - Malformed inputs that should NOT crash the emitter (consumer throws, etc)
  - Expected event sequences recorded in `.events.golden` JSON files
  - Runner: `test_event_fixtures.py` compares emitted events to golden

  Quality:
  - ≥10 fixtures
  - Covers both normal and error paths

  Acceptance:
  - `python3 tests/test_event_fixtures.py` green
  - GC-INSTR-NEG: zero unexpected event sequences

**Verification SPRINT 1:** battery (seq test, parity script, clippy). DoD: `EventEmitter` is integrated into `bn_interp`; all event types are reachable; gates pass. Commit message references D-061-01 through D-061-03.

---

## SECTION 4 — SPRINT 2: DebuggerConsumer (breakpoints, step, stack, vars)

**Objective:** The debugger (today's DAP logic) is refactored into a consumer. Breakpoints, step-in/over/out, stack trace, variable inspection all work via events.

- [ ] **ACTIVITY 2.1 — `DebuggerConsumer` implementation**

  Deliver:
  - `pub struct DebuggerConsumer { breakpoints: HashMap<String, BTreeSet<u64>>, paused: bool, step_mode: Option<StepMode>, … }`
  - Implements `EventConsumer` trait
  - `on_event()`:
    - On `FunctionEntry`: check if at breakpoint, set paused if yes; check step-in logic
    - On `LineChange`: check breakpoint match; may pause
    - On `FunctionReturn`: handle step-out logic
    - On other events: pass (may skip in release build if step-mode is None)
  - `pause_session()`, `resume_session()`, `set_breakpoints()`, `step()` methods
  - Thread-safe: `Arc<Mutex<DebuggerConsumer>>`

  Quality:
  - No I/O (events → memory state only)
  - Breakpoint logic matches today's (line in file matches, respects enable/disable)
  - Step logic (depth tracking for next/in/out) is unchanged

  Acceptance:
  - `cargo build -p bn_runtime_instrumentation --features debugger` succeeds
  - Unit tests for breakpoint matching, step logic
  - Test: set breakpoint at line 5, step-over, hit breakpoint → event queue has LineChange + pause

- [ ] **ACTIVITY 2.2 — DAP refactor: use DebuggerConsumer events**

  Deliver:
  - `src/dap.rs` refactored: instead of hook inside `execute_validated_with_host_debug_control()`, use `EventEmitter` + `DebuggerConsumer`
  - DAP requests (continue, pause, next, stepIn, setBreakpoints) manipulate the consumer (call `set_breakpoints()`, `resume()`, etc)
  - DAP events (stopped, continued, breakpoint) are derived from consumer state changes
  - Remove `SessionState` from dap.rs; use `DebuggerConsumer::state()` methods
  - `DebugFrame` is reconstructed from the last `FunctionEntry` + variable events, not manually built

  Documented:
  - How DAP requests map to DebuggerConsumer calls
  - Event flow: emitter → consumer state → DAP response

  Quality:
  - `src/dap.rs` shrinks (logic moved to consumer)
  - Behavior is byte-identical to today

  Acceptance:
  - `cargo test --test cli -- dap_*` (from tests/bni.rs) green
  - DAP JSON responses are identical to 0.6.0 baseline (JSON diff tool)
  - `tests/dap_event_chain.rs` (new): verifies that a DAP setBreakpoints → pause → continue produces the right event sequence

- [ ] **ACTIVITY 2.3 — Stack trace from events**

  Deliver:
  - DebuggerConsumer maintains a **call stack** (Vec<DebugFrame>) updated on FunctionEntry/Return events
  - `FunctionEntry` pushes; `FunctionReturn` pops
  - Stack is served to DAP `stackTrace` request (vs. today's single-frame DebugFrame)
  - `DebugFrame` is now immutable: created at entry, carries span, args, current line (updated by LineChange events on the top frame)

  Quality:
  - Deep recursion handled (stack grows/shrinks correctly)
  - No stale frames left on error (ErrorThrown cleans stack if needed)

  Acceptance:
  - `cargo test …` for stack trace reconstruction
  - Test: call `f()` calls `g()` calls `h()` → pause in h → stack has [h, g, f, main]

**Verification SPRINT 2:** battery. DoD: DAP behavior is identical to 0.6.0; DebuggerConsumer is a reusable library; stack trace is event-driven. Commit message references D-061-04.

---

## SECTION 5 — SPRINT 3: TracerConsumer (event logging, replay)

**Objective:** A tracer consumer that records all events to a file and can replay them.

- [ ] **ACTIVITY 3.1 — `TracerConsumer` implementation**

  Deliver:
  - `pub struct TracerConsumer { file: File, buffer: Vec<Event>, metadata: TraceMetadata }`
  - Implements `EventConsumer`
  - `on_event()` serializes to NDJSON (newline-delimited JSON)
  - Metadata: git commit hash, bni version, timestamp, source files checksums
  - Buffering: flush every 1000 events or on FunctionReturn (keep file in sync)

  Format (example):
  ```json
  {"version":"0.6.1","commit":"cbd3ae1","timestamp":"2026-09-17T21:00:00Z"}
  {"type":"FunctionEntry","function":"bn_main","depth":0,"span":{"file":"prog.bn","line":1},"args":[]}
  {"type":"LineChange","new_line":2,"span":{"file":"prog.bn","line":2,"col":0}}
  {"type":"VariableWrite","name":"x","old_value":null,"new_value":"10","var_type":"INT"}
  {"type":"Statement","stmt_kind":"Assignment","span":{"file":"prog.bn","line":2}}
  {"type":"FunctionReturn","function":"bn_main","return_value":null,"depth":0}
  ```

  Quality:
  - File I/O errors are logged but do not crash the program
  - Flush is synchronous (performance cost is acceptable for debug/profiling)
  - Variables are truncated if > 1KB (prevent huge logs from huge arrays)

  Acceptance:
  - `cargo build -p bn_runtime_instrumentation --features tracer`
  - Trace file is valid NDJSON (can parse with `jq`)
  - Test: run a simple program, check trace has ≥10 events

- [ ] **ACTIVITY 3.2 — Tracer CLI and replay**

  Deliver:
  - `bn trace <program.bn>` command (in bni)
    - Runs the program with TracerConsumer enabled
    - Outputs trace file (default: `<program>.trace`)
  - `bn-tracer` utility crate (or library in bn_runtime_instrumentation)
    - `bn-tracer timeline <trace.file>` — prints events in chronological order (already are)
    - `bn-tracer filter <trace.file> --event VariableWrite` — filter by event type
    - `bn-tracer grep <trace.file> --var x` — find all reads/writes to variable x
    - `bn-tracer error <trace.file>` — show error events

  Documented:
  - Usage examples in `docs/debugging/tracing.md` (new chapter)

  Quality:
  - Tracer utilities are Rust (standalone, no Python dependency for trace analysis)
  - Replay is read-only (not modifying state)

  Acceptance:
  - `bni trace examples/language-tour.bn` produces a trace
  - `bn-tracer timeline` outputs all events
  - `bn-tracer filter --event FunctionEntry` shows only entry events

- [ ] **ACTIVITY 3.3 — Integration with Debugger**

  Deliver:
  - Both DebuggerConsumer and TracerConsumer can be active simultaneously
  - DAP has an optional `--trace <file>` flag: record events while debugging
  - When user pauses at breakpoint, the trace file shows the path taken to that point

  Quality:
  - No conflict between consumers (each has its own state)
  - Events are emitted once; consumers are called in sequence

  Acceptance:
  - `bni dap --trace debug.trace` starts DAP + tracer
  - IDE pauses at breakpoint; trace file has all events up to that point
  - Test: `test_concurrent_consumers.rs`

**Verification SPRINT 3:** battery. DoD: Trace file is valid JSON; tracer utilities work; debugger + tracer run together. Commit message references D-061-07.

---

## SECTION 6 — SPRINT 4: ProfilerConsumer (timing, call counts)

**Objective:** A profiler consumer that measures function timing and call frequency.

- [ ] **ACTIVITY 4.1 — `ProfilerConsumer` and timing**

  Deliver:
  - `pub struct ProfilerConsumer { call_stack: Vec<(String, Instant)>, stats: HashMap<String, FunctionStats> }`
  - `FunctionStats { count: u32, self_time_ms: f64, cumulative_time_ms: f64, callers: HashMap<String, u32> }`
  - On `FunctionEntry`: push (name, now) to stack
  - On `FunctionReturn`: pop, record elapsed time, increment count
  - On error: if function is still in stack (error during execution), charge time to that function

  Quality:
  - High-precision timing (using `std::time::Instant`, not wall clock)
  - Recursive calls are tracked separately (foo → foo: two entries, two timings)
  - Stack is accurate even with multiple consumers

  Acceptance:
  - `cargo build -p bn_runtime_instrumentation --features profiler`
  - Unit tests: recursive function profiling, error handling
  - Test: function called 100 times; count = 100

- [ ] **ACTIVITY 4.2 — Profiler output and analysis**

  Deliver:
  - `pub struct ProfilerSummary { functions: Vec<FunctionProfile> }`
  - `pub fn generate_summary(&profiler: &ProfilerConsumer) -> ProfilerSummary`
  - Output formats:
    - JSON (for IDE dashboards)
    - Text table (for CLI): function name, calls, self time %, cumulative time %
  - Sort by: call count (default), self time, cumulative time

  Example output:
  ```
  Function             Calls  Self (ms)  Cumulative (ms)  % Self
  ─────────────────────────────────────────────────────────────
  matrix_multiply      1      45.2       45.2             100%
  vector_dot           10     4.3        4.3              100%
  loop_body            1000   0.03       0.03             100%
  ```

  Quality:
  - Table is readable for ≥50 functions
  - JSON is machine-parseable

  Acceptance:
  - `bn profile --output json <trace.trace> > stats.json`
  - `jq '.functions[] | select(.self_time_ms > 1.0)'` works

- [ ] **ACTIVITY 4.3 — Profiler in bni and IDE integration**

  Deliver:
  - `bni profile <program.bn>` command
    - Runs with ProfilerConsumer
    - On exit, prints summary table
  - `bni profile --output json <program.bn>` for IDE dashboards
  - Optional: `bn profile --gui` (future: opens a flame graph in VS Code)

  Quality:
  - Profile runs are deterministic (same output each run, no randomness)
  - Overhead: ≤5% slowdown vs. un-instrumented run

  Acceptance:
  - `bni profile examples/perf-test.bn` completes in < 1s
  - Table output is readable
  - JSON output is valid and parseable

**Verification SPRINT 4:** battery. DoD: Profiler timing is accurate (manual checks); overhead is acceptable. Commit message references D-061-08.

---

## SECTION 7 — SPRINT 5: Compiled debugging (GDB bridge, metadata)

**Objective:** Compiled code (bnc → LLVM → binary) gets basic GDB support via debug metadata.

- [ ] **ACTIVITY 5.1 — Debug metadata extraction**

  Deliver:
  - `crates/bn_debug_metadata` (new) — post-compilation step
  - Input: IR (after lowering), compiled LLVM IR, final binary
  - Output: `<binary>.bn_debug` JSON file with:
    - Function map: function name → (start address, end address, IR node)
    - Line map: (file, line) → binary address(es)
    - Variable map: (function, variable name) → stack offset or register (from DWARF)
    - Source files: hash of each `.bn` file in the project

  Format (example):
  ```json
  {
    "version": "0.6.1",
    "binary": "program",
    "commit": "cbd3ae1",
    "functions": [
      {"name": "main", "start": "0x4000", "end": "0x4100", "ir_node": "fn:main"}
    ],
    "line_map": [
      {"file": "prog.bn", "line": 10, "addresses": ["0x4020", "0x4030"]}
    ],
    "variables": [
      {"function": "main", "name": "x", "offset": "-8(%rbp)", "type": "INT"}
    ]
  }
  ```

  Quality:
  - Metadata is generated automatically by LLVM backend (no manual steps)
  - Can be embedded in the binary as a custom ELF section (future optimization)

  Acceptance:
  - `bnc build program.bn` generates `program.bn_debug`
  - Metadata file is valid JSON
  - Line map has entries for every source line with instructions

- [ ] **ACTIVITY 5.2 — GDB bridge (read-only)**

  Deliver:
  - `crates/bn_gdb_bridge` (new) — command-line utility
  - `bn-gdb-bridge <binary> <binary.bn_debug>` — serves metadata via GDB MI
    - Intercepts GDB `info symbol <addr>` → looks up in line map
    - Intercepts GDB `list <func>` → returns source from line map
    - Intercepts `print <var>` → suggests DWARF lookup or custom format
  - **Not yet:** full DAP integration; that's future work (0.7.x)

  Quality:
  - Bridge is stateless; reads metadata each time (simple, no caching)
  - Graceful fallback: if metadata missing, GDB still works (uses DWARF alone)

  Acceptance:
  - `gdb <binary>` works normally (no bridge required)
  - `(gdb) break prog.bn:10` sets breakpoint (DWARF handles this)
  - Bridge is ready for IDE integration in 0.7.x

- [ ] **ACTIVITY 5.3 — Negative fixtures (GC-COMPILED-DEBUG)**

  Deliver:
  - Compile a few `.bn` programs with `bnc`
  - Verify `<binary.bn_debug` is generated correctly
  - Fixtures:
    - Simple program (linear flow)
    - Recursive function
    - Error handling (exception path)
  - Check: metadata is consistent with DWARF

  Acceptance:
  - Metadata file exists and is well-formed JSON
  - Line map entries are correct (compare with objdump)

**Verification SPRINT 5:** battery. DoD: `bnc build` generates metadata; GDB can use it (manual verification). Commit message references D-061-06.

---

## SECTION 8 — SPRINT 6: Integration and book chapter

**Objective:** All consumers are integrated; IDE (VS Code) knows about tracer and profiler; documentation is complete.

- [ ] **ACTIVITY 6.1 — Feature flags and defaults**

  Deliver:
  - `crates/bni/Cargo.toml`: features `debug-events` (default), `debug-tracer`, `debug-profiler`
  - `bni --version --features` shows enabled features
  - Default: debugger + tracer (profiler is optional; adds 5% overhead)
  - `bni --no-default-features --features "debug-events,debug-profiler"` for custom builds

  Quality:
  - Debugger is always on (for DAP)
  - Tracer is on by default (users expect logs)
  - Profiler is optional (users opt in)

  Acceptance:
  - `cargo build --no-default-features` succeeds (language + HOST only)
  - Feature gates in code are correct (`#[cfg(feature = "…")]`)

- [ ] **ACTIVITY 6.2 — Documentation chapter: `docs/book/en/chapter-10-debugging.md`**

  Deliver:
  - Sections:
    1. Overview: three tools (debugger, tracer, profiler)
    2. Interactive debugging with `bni dap` in VS Code
    3. Tracing with `bni trace` and analysis tools
    4. Profiling: `bni profile`, reading the output
    5. Compiled debugging: `bnc build` + GDB (limited, 0.7.x coming)
    6. Examples: step-by-step debug session, reading a trace
    7. Troubleshooting: common issues

  Quality:
  - English, clear examples
  - References to `docs/architecture/`
  - No outdated `bn` commands (all are `bni` or `bnc`)

  Acceptance:
  - Chapter compiles to HTML without warnings
  - Examples are runnable (tested with the book build process)

- [ ] **ACTIVITY 6.3 — Update `AGENTS.md` and architecture docs**

  Deliver:
  - `AGENTS.md`: note that debug infrastructure is now available in 0.6.1
  - `docs/architecture/target-architecture.md`: add runtime instrumentation layer
  - `docs/architecture/ir-contract.md` § Provenance: document that IR nodes carry Span
  - `docs/architecture/driver-sequence.md`: update to show bni/bnc + debug paths

  Quality:
  - Diagrams are updated (if any)
  - Links are internal and valid

  Acceptance:
  - No broken links (`rg '\[.+\]\(.*\.md\)' docs/architecture/` and check each)

- [ ] **ACTIVITY 6.4 — IDE integration (VS Code extension repo)**

  Deliver:
  - Issue/PR in the VS Code extension repo:
    - `bni dap` now emits tracer and profiler events (optional in UI)
    - UI shows trace live (buffer last N events)
    - Profiler data in a sidebar or panel (call tree, timing)
  - Note links in this bucket for tracking

  Quality:
  - No changes to DAP protocol (backwards compat)
  - UI enhancements are opt-in (off by default)

  Acceptance:
  - Extension still works with 0.6.0 bni (graceful degradation)

**Verification SPRINT 6 = bucket close:** battery + book build + manual IDE test. DoD: Carlos accepts; tag `v0.6.1`. Commit message: "bucket-0.6.1: runtime instrumentation, debugger, tracer, profiler" with references to D-061-01…08.

---

## SECTION 9 — Dependencies and gates (GC-INSTR-*)

| Gate | Requirement | Verification | Blocks |
| --- | --- | --- | --- |
| **GC-INSTR-EMIT** | Every event type (8 variants) is emitted by at least one test program. No event type is dead code. | `crates/bn_interp/tests/instrumentation_gates.rs` counts event types; ≥8 variants emitted. | SPRINT 1 done |
| **GC-INSTR-NEG** | Negative event sequences (malformed inputs, errors during execution) do not crash the emitter. Error handling is correct. | `tests/test_event_fixtures.py` runs 10+ fixture programs; all complete; tracer files are valid. | SPRINT 1 done |
| **GC-DEBUGGER-COMPAT** | Debugger behavior is identical to 0.6.0. DAP JSON responses match byte-for-byte (excluding timestamps). | `tests/dap_parity_baseline.rs`: run same DAP commands against 0.6.0 and 0.6.1; JSON diff. | SPRINT 2 done |
| **GC-TRACER-VALID** | Trace file is always valid NDJSON. Parser never hangs or crashes. | `tests/test_tracer_format.py`: parse 1000+ events from generated traces; verify no corruption. | SPRINT 3 done |
| **GC-PROFILER-OVERHEAD** | Profiler adds ≤5% slowdown vs. un-instrumented run. Measured on 3+ benchmark programs. | `tests/profile_overhead.sh`: run benchmark with/without profiler; measure time ratio. Report in commit message. | SPRINT 4 done |
| **GC-COMPILED-DEBUG** | Compiled code has valid debug metadata. Metadata can be parsed by GDB tools. | `tests/test_metadata_format.py`: generate metadata for 5+ compiled programs; verify JSON well-formed; GDB can read symbols. | SPRINT 5 done |

---

## SECTION 10 — Timeline and dependencies

```
0.6.0 (open/closed)
  ↓ (dependencies)
0.6.1 SPRINT 1 (instr layer)
  ↓
0.6.1 SPRINT 2 (debugger consumer)
  ↓
0.6.1 SPRINT 3 (tracer consumer)
  ↓
0.6.1 SPRINT 4 (profiler consumer)
  ↓
0.6.1 SPRINT 5 (compiled debug metadata)
  ↓
0.6.1 SPRINT 6 (IDE integration + book)
  ↓
0.6.1 CLOSED: tag v0.6.1

Parallel work (after 0.6.1 opens):
  - IDE repos (VS Code, Jupyter): update to use bni/bnc, track tracer/profiler UI (ACTIVITY 6.4)
  - 0.7.x planning: full GDB DAP bridge, flame graph UI
```

---

## SECTION 11 — Acceptance and sign-off

- [ ] **Language spec (0.5.md, 0.5.ebnf):** No changes required (events are observational). Book is the delta.
- [ ] **IR contract:** Span + IR node provenance are documented and stable. No breaking IR changes.
- [ ] **Compliance:** All GC-INSTR-* gates pass.
- [ ] **Carlos approves:** Roadmap is clear, decisions are recorded, work is doable in 6 sprints.
- [ ] **Tag:** `v0.6.1` on main branch; release notes document the new debug infrastructure.


## History

- **2026-09-21** — Carlos invert: debug moved from ongoing 0.6.1 to todo 0.6.4.

## Triage (2026-10-09)

The 0.6.4 number went to `BNSqlite` (`done/bucket-0.6.4-bnsqlite.md`) and
its letter lanes; this plan never ran. State of the code:

- Sprints 1, 3, 4 (instrumentation crate, `TracerConsumer`,
  `ProfilerConsumer`): not implemented. `bni --trace` only reports the
  entry point.
- Sprint 2 (`DebuggerConsumer`, DAP over events): not implemented as
  planned; `bni dap` works through `bn_interp` debug hooks
  (`crates/bn_interp/src/executor/debug_view.rs`, `crates/bn_dap`).
- Sprint 5: the goal (source breakpoints in compiled code) shipped by a
  different design, `bnc -g` (DWARF; PDB on Windows), tested in
  `crates/bnc/tests/bnc.rs`. The `.bn_debug` JSON sidecar and
  `bn_gdb_bridge` do not exist. TODO: Carlos decides whether 5.1–5.3 are
  cut (superseded by `-g`) or still wanted.
- 6.2: the book has `docs/book/en/20_debugging.md` (Appendix M: `bni dap`,
  `bnc -g`, ARC), not an instrumentation chapter.

Remaining work rescheduled after 0.6.7 as 0.6.8d.
