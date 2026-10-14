# Proposal: property-based and mutation testing

**Status:** Proposed (Carlos, 2026-10-14). Split from bucket 0.6.7
(`todo/bucket-0.6.7-test-system.md`), which keeps the independent-oracle,
real-component, fuzzing and concurrency work (section V).
**Date:** 2026-10-14.
**Target release:** TODO (decision).
**Language surface:** none. Test-only: new dev-dependencies and tools, no
grammar, semantics, HOST or IR change.

## Motivation

The suite checks examples: a program and its expected result. Two kinds of
defect pass through it:

- a rule that holds for the cases written but not for the inputs between
  them (an overflow boundary, a conversion of an unusual value, a
  comparison of nested `STRUCT`s);
- a test that runs the code but would not notice if the code were wrong
  (an assertion too weak, a branch never compared).

Property-based testing addresses the first; mutation testing measures the
second.

## Part A — Property-based testing

A property states what holds for every input of a domain; the tool
generates inputs and shrinks a failing one to the smallest.

Applied only where the oracle is cheap and exact, not across the code base:

| Core | Property | Oracle |
| --- | --- | --- |
| Integer arithmetic, conversions, overflow (`bn_value`, `bn_core_math`, the LLVM emission of the same operations) | the BN result equals a reference model, and overflow yields the spec's `Error` | a model in `i128` / `f64` written from `language/0.6/0.6.md` |
| Comparison and `PRINT` of `STRUCT`, vector and object values (the K68 class) | equality is reflexive, symmetric and agrees with field-wise equality; `PRINT` follows the spec's format | the spec text, as a function |
| Temporal values (the K80 class) | round-trips and ordering the contract `temporal.md` states | `temporal.md` |
| IR validation | every IR the lowering produces from an accepted program passes `validate` (W1); each mutation of a valid IR from a known list is rejected with its code (W2) | the validator's contract, `ir-contract.md` |
| ARC | after generated sequences of assignment, return and scope exit, every reference count returns to its start | the ownership rules, `arc-conformance.md` |

Parser round-trip (`parse(print(ast)) == ast`) only if a source printer of
the AST exists. TODO: not verified.

Open points:

- Tool: `proptest` as a dev-dependency (shrinking, persisted failure
  seeds). TODO (decision): approval of the dependency; check its current
  version and licence before adding it.
- Every failure the tool finds becomes a fixed example case (bucket 0.6.7
  V.1 rules), so the regression does not depend on a seed.
- Case count and time per property fixed and recorded, so the run stays
  deterministic in CI (fixed seed in CI, random locally).

## Part B — Selective mutation testing

A mutation tool changes the code (an operator, a condition, a return
value) and runs the tests; a mutant that survives shows a place the tests
do not check.

Applied only to small modules where a silent change is a security or
correctness failure:

- IR `validate` (`bn_ir`);
- HOST policy and its re-check at the call boundary
  (`bn_rt/src/policy.rs`, `host-traits.md`);
- overflow and conversion checks (interpreter and LLVM emission);
- ownership / ARC emission and runtime.

Open points:

- Tool: `cargo-mutants`. TODO: verify it handles the workspace layout and
  the integration tests that need `bni`, `bnc` and `libbn_rt.a` built.
- Cost: every mutant rebuilds and runs the selected tests; on macOS each
  fresh test binary pays the first-exec scan (bucket 0.6.7 Sprint 2). Run
  it on Linux, per module, at bucket close or on demand; never a CI gate.
- A surviving mutant becomes a test activity (a stronger assertion or a
  new case), or is recorded as equivalent with the reason; the list of
  equivalent mutants only shrinks.

## Dependencies

- Bucket 0.6.7 Sprint 5 (test index, `covers:` tags) to select the tests
  that cover each target module.
- Bucket 0.6.7 V.1 (independent oracle) so properties and new cases follow
  the same rule.
- Bucket 0.6.7 Sprint 2 measurements, for the cost of Part B on macOS.

## Out of scope

- Coverage targets as a goal: coverage (`cargo-llvm-cov`, Sprint 8) shows
  what runs, not what is checked; Part B is the check.
- Generative differential testing of `bni` × `bnc` (well-typed program
  generator): excluded from bucket 0.6.7 V.3; TODO (decision) whether it
  joins this proposal or gets its own.
