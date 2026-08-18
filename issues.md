# Basic Next Issues

## Numeric overflow is not fully specified

**Status:** Open  
**Priority:** Critical  
**Scope:** Basic Next 0.1 semantic specification

The language currently does not define a complete policy for numeric overflow,
promotion, narrowing, exponentiation, or conversion of `NaN` and infinity to
integer types.

### Required decisions

- Define checked behavior for every integer operation and compound assignment.
- Define overflow behavior for `BYTE`, `INT16`, `INT32`, `UINT32`, and aliases.
- Define numeric promotion and the result type of mixed numeric expressions.
- Define division, modulo, shift, and exponentiation overflow behavior.
- Define allocation-size and multidimensional-vector size overflow checks.
- Define conversion behavior when a floating-point value is outside an integer
  range, including `NaN` and infinity.
- Ensure overflow produces a deterministic BN runtime `Error`, never wrapping
  silently and never causing a Rust panic.
- Add positive and negative conformance fixtures and diagnostics for each rule.

### Acceptance criteria

The specification must state one deterministic result for every valid numeric
operation and one defined diagnostic for every overflow or invalid conversion.
The reference interpreter must enforce the same rules with checked arithmetic.
