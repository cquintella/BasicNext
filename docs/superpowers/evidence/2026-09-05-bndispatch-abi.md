# BNDispatch ABI evidence

The compiled `examples/dispatch_reliability_simulation.bn` fixture builds and
runs through the native LLVM path. Its four line records and completion marker
match `bn run`; task order is intentionally nondeterministic because the queue
is concurrent.

Focused runtime evidence:

- `cargo test -p bn_rt dispatch_abi`: ABI tests pass, including timeout,
  cancellation before start, idempotent ticket close, semaphore, and mutex.
- `cargo test --test cli build_lowers_dispatch_examples_with_native_parity`:
  reliability, game-tournament, and cellular-automaton native fixtures pass
  after normalizing the intentionally nondeterministic task order.
- `cargo clippy --all-targets -- -D warnings`: passes.
- `cargo fmt --check`: passes.

`parallel_work.bn` is rejected by the language type checker because its
integer-returning task does not satisfy the declared `FUNCTION() AS VOID OR
Error` queue contract; it is outside the supported dispatch signature. The
full workspace test command is currently affected by the sandbox's permission
denial for loopback socket tests in `bn_rt`; the focused dispatch and compiler
checks pass independently.
