# Proposal: Checked Numeric Semantics

**Status:** 0.1 language rules accepted and present in the interpreter.
Audit 2026-09-03 against `src/semantic/type_ops.rs`, `src/runtime/executor.rs`,
`src/heap.rs`, `examples/type_test.bn`, and `tests/runtime.rs`. `[X]` is in
the tree. `[ ]` is not. The historical `docs/language/0.1/0.1.md` path is gone;
keywords and the runtime are the live contract.

It changes semantics only; the grammar is unchanged (`^` is not exponentiation).

Overflow is a fatal runtime diagnostic (`NUMERIC_OVERFLOW`, process exit 1),
not an `Error` object. That matches `examples/type_test.bn` and
`tests/runtime.rs`. It never wraps and must not panic.

## Types, literals, and widening

- [X] No implicit assignment conversion between declared numeric types
      (including no silent widening on `LET` / `=`)
- [X] Contextual integer/float literals take the target type; untyped default
      is `INTEGER` / `FLOAT`
- [X] Signed expression widening `INT8 → INT16 → INT32 → INT64`
- [X] Unsigned expression widening `BYTE → UINT16 → UINT32 → UINT64`
- [X] `BYTE` is the only unsigned that widens into signed (`INT16`)
- [X] `UINT16`/`UINT32`/`UINT64` do not combine with signed integers
      (`promote_integers` returns `None` → `TYPE_MISMATCH`)
- [X] `FLOAT32` and `FLOAT64` combine as `FLOAT64`
- [X] Integers do not combine implicitly with floats (except an integer
      literal next to a float)

## Operations

- [X] Integral `+ - * ** SHL`, unary `-`, `+= -= *= **=`, and `FOR` binding
      updates are range-checked; overflow is `NUMERIC_OVERFLOW`
- [X] `/` returns `FLOAT` (IEEE 754). `1 / 0` is `INF`, `0 / 0` is `NAN`
- [X] `DIV` and `%` are Euclidean integers. `(-5) DIV 2 = -3`, `(-5) % 2 = 1`
- [X] `%` / `DIV` by zero is `DIVISION_BY_ZERO` (runtime)
- [X] Integral `**` rejects a negative literal statically and a negative or
      oversized computed exponent with `INVALID_EXPONENT`
- [X] Floating `**` allows a negative exponent
- [X] Shift count in `0 .. width`; literal out of range is
      `INVALID_SHIFT_COUNT` (semantic); computed is the same at runtime
- [X] `SHR` is logical; `SHL` is checked, not masked
- [X] `/=` only when the left target is `FLOAT`; `%=` only for integrals
- [X] Compound assignment does not silently narrow
- [X] `^` is not exponentiation (`tests/grammar/invalid/caret-exponentiation.bn`)

## IEEE and conversions

- [X] Float ops are IEEE 754 binary with `roundTiesToEven` (host `f32`/`f64`)
- [X] `NAN`, signed zero, infinities are values; float `/ 0` is IEEE, not a
      BN error
- [X] No IEEE status flags, traps, NaN payloads, or rounding-mode API
- [X] `NAN` unordered: `=` is false (IEEE `==`), `<>` is true; ordered
      `< <= > >=` are false in Rust `f64` compares
- [X] Explicit conversion among integrals, with range check
- [X] `INTEGER` follows `INT32`
- [X] Float-to-integer truncates toward zero, then range-checks
      (`value.trunc()`)
- [X] `NAN` / `INF` / `-INF` to integer is `INVALID_NUMERIC_CONVERSION`
- [X] Conversion to `FLOAT32`/`FLOAT` uses IEEE rounding; overflow becomes
      signed infinity

## Allocation sizes

- [X] Vector/`NEW` dimensions: negative → `ALLOCATION_SIZE_INVALID`
- [X] Dimension product overflow → `ALLOCATION_SIZE_OVERFLOW`
- [X] Size over host limit → `ALLOCATION_TOO_LARGE`
- [X] No wrapped size reaches the allocator (`src/heap.rs`,
      `src/runtime/executor/part10.rs`)

## Conformance fixtures

Positive coverage exists (`examples/type_test.bn`, `tests/runtime.rs`
overflow, IEEE `/ 0`, Euclidean in the factorial program).

- [X] Integral overflow (`INT8` `+= 1`, constructor overflow, type_test probes)
- [X] IEEE `/ 0` → `INF` / `NAN`
- [X] Euclidean `DIV` / `%` on negatives
- [X] `SHL` / `SHR` happy path
- [X] `ALLOCATION_TOO_LARGE` / `ALLOCATION_SIZE_INVALID` tests
- [ ] Executable fixture for `DIVISION_BY_ZERO` (`DIV` / `%`)
- [ ] Executable fixture for `INVALID_SHIFT_COUNT` (computed count)
- [ ] Executable fixture for `INVALID_EXPONENT`
- [ ] Executable fixture for `INVALID_NUMERIC_CONVERSION` (`NAN`/`INF` as INT)
- [ ] `INT32_MIN % -1 = 0` (rule is in the proposal; not asserted in type_test)
- [ ] `NAN = NAN` / ordered compares as `CheckBoolean` in type_test
- [ ] Multidimensional `ALLOCATION_SIZE_OVERFLOW` fixture

## Compiled path (not this proposal's 0.1 text, noted)

LLVM analysis now lists `DIV`, `%`, `**`, `SHL`, `SHR` as supported and uses
overflow intrinsics for `+ - *`. Differential `bn build` vs `bn run` for those
ops is the 0.4.3 bucket, not a hole in this numeric contract.

## Alternatives considered (accepted)

- [X] No two's-complement wrap
- [X] No automatic assignment widening
- [X] No saturating arithmetic
