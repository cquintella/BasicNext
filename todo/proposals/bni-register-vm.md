# Proposal: a register execution form for `bni` (item 10)

**Author:** agent, for Carlos Quintella (BDFL)
**Status:** Proposed. Opened by bucket 0.6.10 F.9, as that item requires: after
F.2–F.8, a counted `FOR` still runs at more than 20× Lua.
**Scope:** `crates/bn_interp` only. No language change, no `bn_ir` change, no
change to what any program prints, returns or diagnoses.

## Measurement that opens it

Release build, median of 5, same session, this Mac (bucket 0.6.10 F.9 table):

| Bench | 0.6.9 `HEAD` | after F.2–F.8 | Lua 5.5.0 |
| --- | --- | --- | --- |
| `FOR` 2M iterations | 2168 ms | ~597 ms | 11.5 ms |
| `WHILE` 2M iterations | 1183 ms | 465 ms | — |
| 400k user calls | 740 ms | 263 ms | — |

`FOR` is about 52× Lua. The profile after F.7 has no dominant hot spot left:
the instruction `match`, the frame loop, `set`, the intrinsic call, checked
integer arithmetic and `coerce_to` each take 4–25 %. The cost is spread over
the tree-walking design itself: every instruction re-matches an enum of
owned operands, looks its operands up by `ValueId`, re-checks `debug_control`,
the emitter and the cancel flag, and builds and drops `Value`s.

## Proposal

Translate each validated `bn_ir::Function`, once, when it is first called,
into a private execution form inside `bn_interp`, and run that form:

- **Registers.** Every `ValueId` and every local `SymbolId` gets a dense
  register index, computed per function. This is the dense remap that F.2
  left out (Carlos, 2026-10-26: it moves here). Frames become one
  `Vec<Value>` sized once.
- **Pre-resolved operations.** Constants are materialized once
  (F.7 already memoizes them per frame); callees are resolved to a function
  index, a host member or an intrinsic before execution, so no call matches
  a name at run time; the two `FOR` intrinsics become operations.
- **Typed fast paths.** An integer `+`, `-` or comparison whose operand types
  the validated IR already fixes gets an operation with the same
  `checked_integer` diagnostics. F.7 measured this path at 2.3 % in the
  tree walker; in a register form it removes the operand lookups too, so it
  is measured again, not assumed.
- **Hooks out of the loop.** The debugger, trace emitter and cancel flag
  are checked by a separate instrumented loop, selected once per run, so an
  uninstrumented run pays nothing for them.

## Contracts it must keep

- **W1, W3.** The form is built from the validated IR that `bnc` also
  compiles, after `validate`, never from the AST. It is not a second IR:
  `bn_ir` does not change and nothing outside `bn_interp` sees it (W5).
- **Behaviour.** Every F.1 lock (`tests/conformance/interp/value-semantics.bn`,
  `tests/programs/runtime_locks.rs`, the DAP lock), the conformance cells,
  parity and the whole suite stay byte-identical. Diagnostic codes, spans,
  `STOP`, ownership (`RELEASE`, `AS WEAK`), the stack budget, the REPL
  session capture and the DAP view keep their behaviour.
- **One executor.** The tree walker is removed when the form covers every
  instruction, not kept as a second path (no dead code). Until then the
  form is not wired.

## Acceptance (proposed)

- `FOR` 2M at or below 20× Lua on this machine (≤ 230 ms), `WHILE` at most
  1.25× `FOR`, the vector and call gates linear.
- The full battery, Windows, and the DAP and LSP smoke tests green.

## Open questions

- **Q1.** Target version: 0.7.0 (with the binary BN IR module work in
  `bn-ir-binary-module.md`, whose loader could build the same form) or a
  0.6.x bucket of its own. Recommendation: its own bucket after 0.6.10,
  since it touches no language surface.
- **Q2.** Is 20× Lua the right acceptance line, or is the target a ratio
  against 0.6.9 (for example 10×)?
