# Bucket 0.6.11b — Jupyter Kernel (`basicnext-jupyter`) Brought to bni 0.6

**Status:** todo (created 2026-10-28; runs after 0.6.12, independent of the language sequence — Carlos, 2026-10-28). Parts 1 and 3a are done and pushed in `basicnext-jupyter` commit `5c327a9` (`main`). Parts 2, 3b and 4 are open.
**Decisions (Carlos, 2026-10-28):** (1) notebooks keep state between cells through `bni eval --session`; this supersedes "no state between cells" in `language/0.6/0.6.md` § "Jupyter host", which still has to be rewritten (JUP-SPEC-1). (2) Diagnostics are passed through exactly as bni emits them; the `cell.bn` path rewrite is dropped. (3) Commit and push the plugin.
**Source:** Review of `basicnext-jupyter` (single commit `6d38484`, 2026-09-16, package `bn-kernel` 0.4.4) against `bni` at `2c09339` (0.6.11 dev) and `language/0.6/0.6.md` § "Jupyter host".
**Version slot:** `b` suffix of 0.6.11, the bucket in progress. The kernel is a sibling repo and a host of the existing runtime; it changes no language surface, so it does not take a numbered slot in the language sequence of `AGENTS.md`. 0.6.13 stays unassigned and is not reused. Anything that changes the host contract (Part 3) is a BDFL gate and does not close here.

The kernel stays a separate Python package. The Rust workspace does not link ZeroMQ (`0.6.md` § "Jupyter host").

---

## Part 0: Review Findings (2026-10-28)

### Contract with bni (verified)
* `bni run --no-filesystem --jupyter-stdin <file>` exists in 0.6.10/0.6.11 (`crates/bn_interpret_driver/src/options.rs`). The INPUT marker is `\x1eBN_INPUT_REQUEST\n` on stderr (`run.rs:330`), the same marker the plugin parses.
* `bnc` rejects `--jupyter-stdin` (`tests/snapshots/bnc_jupyter_stdin.txt`). The kernel never calls `bnc`. WebAssembly is gone from the toolchain; the plugin had no wasm reference.
* `BN_EXEC_POLICY=deny` and `BN_ENV_POLICY=deny` are honoured by `bni` (`tests/programs/compiler_parity.rs`). Any other value is `CONFIG_INVALID`.
* `bni eval --session` serves an NDJSON session protocol (`session_server.rs`: `classify`, `submit`, `complete`, `doc`, `bindings`, `reset`, `export`, …). It keeps state between submissions. The spec says a cell is a complete program with no state between cells, so the kernel does not use it yet (Part 3).

### Defects found
1. **Broken default executable in the Jupyter path.** The in-progress change made `bn` default to `None`; `jupyter.py` passed `None` straight to `Popen`. Every notebook cell would fail. The JSON-lines path resolved it; the Jupyter path did not.
2. **Sandbox only on one path.** `BN_EXEC_POLICY`/`BN_ENV_POLICY=deny` were set in `kernel.execute_cell` but not in the Jupyter kernel, so notebook cells could run processes and read the environment.
3. **0.5 driver fallback.** `default_executable()` fell back to `bn`, a 0.5 driver that no longer exists. Test default pointed at `target/debug/bn`.
4. **ZeroMQ thread safety.** The stderr reader thread sent `input_request` on the stdin socket and polled it while the main thread used the other sockets; the heartbeat socket was closed by the main thread while the heartbeat thread polled it (`ZMQError: Socket operation on non-socket` in the test run).
5. **Signing.** With an empty key the kernel still sent an HMAC of the empty key; the spec requires an empty signature.
6. **Wire contract gaps.** No `execute_input`, no iopub `error`, stderr of a successful run was dropped, `payload` was `{}` instead of `[]`, `date` was empty, header `session` reused the client's session, `shutdown_reply.restart` ignored the request, `silent`/`store_history`/`allow_stdin` were ignored, and `comm_info_request`, `is_complete_request`, `complete_request`, `inspect_request`, `history_request` got no reply (JupyterLab waits on `comm_info`).
7. **Output buffering.** stdout was delivered only after the program exited.
8. **No timeout and no missing-interpreter error.** A loop ran until interrupted; a missing `bni` raised `FileNotFoundError` inside the kernel.
9. **Stale metadata.** `pyproject` 0.4.4, `KERNEL_INFO` hard-coded `0.4.4`, README described the old `plugins/jupyter` monorepo path and `bn run`, kernelspec had no `interrupt_mode`, no install command.

---

## Part 1: Bring the kernel to bni 0.6 (done in working tree)

* `bn_kernel/kernel.py`: single `bni` resolution (`--bni` → `BN_KERNEL_BNI` → `bni` on PATH), `interpreter_version()`, `sandbox_environment()`, `run_command()` (`run --no-filesystem --color never --no-log [--jupyter-stdin]`), `cell_file()`, timeout and missing-interpreter results (124 / 127). Diagnostics are not rewritten.
* `bn_kernel/jupyter.py`: rewritten. Main thread owns shell/control/stdin/iopub; heartbeat thread owns hb; reader threads feed a queue. Live stdout streaming, INPUT ↔ `input_request` routed with the shell identities, `allow_stdin` honoured (EOF otherwise), iopub `execute_input`/`error`/`stream`, busy/idle around every request, signed/unsigned per key, `signature_scheme` checked, all auxiliary requests answered, interrupt marks the cell `Interrupted`, per-cell timeout.
* `bn_kernel/launcher.py`: `--bni` (alias `--bn`), `--timeout`/`BN_KERNEL_TIMEOUT`, bad JSON-lines requests answered instead of crashing, `install` subcommand.
* `bn_kernel/install.py` (new): kernelspec install via `jupyter_client` (`--user`, `--sys-prefix`, `--prefix`, `--bni`), argv pinned to `sys.executable`.
* `pyproject.toml` 0.6.11, console script `bn-kernel`, `install` extra, `dev` group (pytest, jupyter_client). `kernelspec/kernel.json` adds `interrupt_mode: message`. README rewritten.
* Tests: `tests/support.py` (binary discovery), `test_kernel.py` (+ INPUT, timeout, missing interpreter, cell path, version), `test_jupyter.py` rewritten (signed wire, streaming, error, stdin round trip, interrupt, heartbeat, auxiliary replies). 17 pass against `target/debug/bni`; a real `jupyter_client` `KernelManager` session against `target/release/bni` ran OK/error cells and shut down cleanly.

---

## Part 2: Packaging and CI (open)

* Commit the working tree in `basicnext-jupyter` (the user's in-progress diff is folded into Part 1).
* CI on Linux/macOS/Windows: build `bni` from a pinned `basicnext` tag, run `uv run --group dev pytest`. Windows: `terminate()` is `TerminateProcess`; check interrupt and the stderr marker with CRLF.
* Version policy: kernel version follows the `bni` release it is tested with; `kernel_info` reports both.
* Syntax highlighting: `pygments_lexer`/`codemirror_mode` are `text`. Ship a Pygments lexer and a CodeMirror 6 mode, or reuse the TextMate grammar of `basicnext-vscode`.

## Part 3a: Stateful notebook over `bni eval --session` (done)

### How the session server behaves (`crates/bn_interpret_driver/src/session_server.rs`, `eval.rs`)
* Start: `bni eval --session --format json` (without `--format json` it exits 2 with `CONFIG_INVALID`). Common flags (`--no-filesystem`, `--sandbox`, roots, `--module-path`, `--color`, `--no-log`) are parsed; the virtual path is `./.bn-session.bn`. `BN_EXEC_POLICY` / `BN_ENV_POLICY` are honoured (`HOST.Exec` returns `POLICY_DENIED`).
* Lifecycle: one process, state in memory only (`SessionAccumulator` + `InteractiveSession`), nothing on disk. NDJSON `{"action": ...}` per line; `quit` ends it, EOF ends it.
* `submit` → `{ok, exit_code, stdout, stderr, result_value, result_type, structured_data, diagnostics[]}`. A failed submission (semantic or runtime) is not committed, so the session survives. Expressions return `result_value`.
* `reset` clears accumulator and runtime; `classify` → `complete|incomplete|invalid`; `complete` takes `prefix`.
* `--jupyter-stdin` is accepted and ignored; `INPUT` gets `io::empty()` (EOF).

### Kernel
* `bn_kernel/session.py` (new): `BniSession` (start, send, poll, request with timeout, reset, classify, complete, close/stop/restart), `SessionLost`, `render_diagnostics()`.
* Kernel mode `session` is the default; `--mode program` keeps Part 1 behaviour. One session per kernel; restart → empty session; shutdown → `quit`; interrupt / timeout kill bni and the next cell starts a fresh session (`Interrupted` / `Timeout` with "earlier state is gone"). Expression cells publish `execute_result`. `is_complete` and `complete` use the session. JSON-lines host: one session, `{"reset": true}`.
* Tests: `tests/test_session.py` (state, reset, failed submission keeps state, exec deny, INPUT EOF, timeout, JSON-lines), `JupyterSessionTests` (state across cells, execute_result, error keeps session, restart clears, timeout, interrupt, is_complete/complete, filesystem denied). 32 pass. `jupyter_client` smoke: function + variable across cells, expression result, restart clears state, program-mode INPUT round trip.

## Part 3b: Gaps in `bni eval --session` (open, basicnext Rust)

Found while wiring the kernel; not changed here.
* **INPUT:** session mode reads `io::empty()`. Needs an input channel (e.g. an `input_request` NDJSON event and an `input` action) for notebooks to use `INPUT`.
* **Streaming:** stdout is buffered into the reply; long cells print nothing until they finish. Needs `stream` events before the final reply.
* **Interrupt:** no cancel action; the kernel must kill the process and state is lost. Needs a cooperative interrupt that keeps the session.
* **Diagnostics:** session diagnostics are JSON only, and label lines/columns refer to the accumulated program (a one-line cell reports line 3), not the cell. The kernel renders the heading, cause and help without a location.
* **Replay bug:** after a cell declares `LET r AS Exec.Result OR Error = ...` (with `IMPORT HOST.Exec AS Exec` in an earlier cell), every later submission fails with `UNKNOWN_TYPE: type 'HOST.Exec.Result'...`. The accumulator replays the declaration without resolving the imported alias.
* **`Start`:** a submitted `FUNCTION Start()` is accepted and not called (see 0.7.0 item 7).
* **Rich display** (MIME bundles, e.g. DataFrame as HTML) needs a runtime channel that does not exist in 0.6; `structured_data` is a start.

## Part 4: Docs

* Book chapter or appendix section on the notebook host, `bn-kernel install`, sandbox (filesystem, exec, env denied) and timeout.
* `man bni`: document the `--jupyter-stdin` marker and `eval --session` as stable host protocols.
* `language/0.6/0.6.md` § "Jupyter host": rewrite for the stateful default (submissions, no `Start` requirement, session lifecycle) and program mode. Spec change: goes through `language-guardian`.

---

## Activity Checklist

### Part 1: bni 0.6 alignment
- [x] **JUP-1 (Executable):** One `bni` resolution for both paths; drop the 0.5 `bn` fallback and test default.
- [x] **JUP-2 (Sandbox):** `--no-filesystem`, `BN_EXEC_POLICY=deny`, `BN_ENV_POLICY=deny` on the Jupyter and JSON-lines paths.
- [x] **JUP-3 (Threads):** Only the main thread uses shell/control/stdin/iopub; hb owned by its thread; reader threads use a queue.
- [x] **JUP-4 (Wire 5.3):** Signing per key, `execute_input`, iopub `error`, stderr stream, `payload` list, ISO `date`, kernel session id, `restart` echo, `silent`/`store_history`/`allow_stdin`, auxiliary replies.
- [x] **JUP-5 (Streaming):** stdout streamed while the cell runs.
- [x] **JUP-6 (Limits):** Per-cell timeout, missing-interpreter error, interrupt reported as `Interrupted`.
- [x] **JUP-7 (Metadata):** 0.6.11 version, `kernel_info` from `bni --version`, kernelspec `interrupt_mode`, `install` command, README.
- [x] **JUP-8 (Tests):** Unit and wire tests against `bni`; manual `jupyter_client` smoke run.

### Part 2: Packaging and CI
- [x] **JUP-PKG-1:** Commit and push `basicnext-jupyter` (`5c327a9`).
- [ ] **JUP-TAG-1:** Tag the plugin with the tested `bni` version.
- [ ] **JUP-CI-1:** Three-OS CI against a pinned `bni`.
- [ ] **JUP-WIN-1:** Verify interrupt, timeout and the INPUT marker on Windows.
- [ ] **JUP-HL-1:** Pygments lexer and CodeMirror mode.

### Part 3a: Stateful notebook (done)
- [x] **JUP-SES-1 (Gate):** Approved 2026-10-28: state between cells via `bni eval --session`.
- [x] **JUP-SES-2:** `is_complete` / `complete` through `classify` / `complete`.
- [x] **JUP-SES-3:** One session per kernel; restart, shutdown, interrupt, timeout; JSON-lines host; tests.
- [x] **JUP-DIAG-1 (Gate):** Resolved: no path rewrite; diagnostics as bni emits them.

### Part 3b: bni session gaps (basicnext)
- [ ] **JUP-BNI-1:** INPUT channel in session mode.
- [ ] **JUP-BNI-2:** Streamed stdout events before the reply.
- [ ] **JUP-BNI-3:** Cooperative interrupt that keeps the session.
- [ ] **JUP-BNI-4:** Diagnostic locations relative to the submitted cell; optional rendered text field.
- [ ] **JUP-BNI-5:** Fix replay of declarations typed by an imported alias (`Exec.Result`).
- [ ] **JUP-MIME-1:** Rich display channel (out of 0.6 unless approved).

### Part 4: Docs
- [ ] **JUP-DOC-1:** Book section on the notebook host.
- [ ] **JUP-DOC-2:** `man bni` documents `--jupyter-stdin` and `eval --session`.
- [ ] **JUP-SPEC-1:** Rewrite `language/0.6/0.6.md` § "Jupyter host" for the stateful default (supersedes "no state between cells").
