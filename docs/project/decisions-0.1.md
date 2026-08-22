# Basic Next 0.1 Decision Record

This indexes maintainer decisions made while closing the 0.1 specification. It
is not normative: `docs/language/0.1.ebnf`, `docs/language/0.1.md`, and
`docs/language/keywords.md` take precedence.

## Reference implementation architecture

- The validated typed AST lowers to a typed BN control-flow IR before
  execution.
- The 0.1 reference interpreter executes BN IR. A later compiler lowers the
  same BN IR to LLVM IR so execution and compilation share one operational
  representation.

## Numeric values and expressions

- Assignments never convert declared numeric values implicitly. Numeric binary
  expressions widen `INT8 → INT16 → INT32 → INT64` and `BYTE → UINT16 → UINT32
  → UINT64`; `BYTE` also widens safely to `INT16`. `FLOAT32 → FLOAT64`.
  Integers never mix implicitly with floats, and `UINT16`/`UINT32`/`UINT64`
  never mix implicitly with signed integers.
- Integer literals are contextual and default to `INTEGER` (`INT32`) without a
  context. Floating literals default to `FLOAT` (`FLOAT64`).
- `TIMESTAMP` is an `INT64` alias for a UTC Unix-epoch instant in milliseconds;
  it is fully compatible with `INT64`.
- `DATE` and `TIME` are fixed-size immutable value types: an `INT32` day count
  and `UINT32` milliseconds since midnight. `TIMEZONE` is an immutable,
  variable-size IANA TZDB identifier value. Civil values use years `0001` to
  `9999`.
- RFC 3339 is the mandatory `TIMESTAMP` interchange profile: canonical output
  is `YYYY-MM-DDTHH:MM:SS.mmmZ`; parsing accepts `Z` or numeric ISO 8601
  offsets and normalizes to UTC. `DATE` and `TIME` use their ISO 8601 forms;
  `TIMEZONE` uses IANA names, not numeric offsets.
- Integral overflow raises `NUMERIC_OVERFLOW`; integers never wrap or saturate.
- `/` returns `FLOAT`; `DIV` and `%` use Euclidean integer division/modulo.
- `**` is the only exponentiation operator: it is checked integral power with a
  non-negative exponent for integral operands and real power for floating
  operands. `^` is not Basic Next 0.1 syntax; `DIV=` is absent.
- Explicit integral casts, including narrowing and signed/unsigned conversions,
  are range-checked and raise `INVALID_NUMERIC_CONVERSION` when invalid.
- Floating values use the IEEE 754 model with `roundTiesToEven`: finite values,
  signed zero, `NAN`, `INF`, and `-INF`. Floating overflow and division by zero
  produce IEEE values rather than BN runtime errors. IEEE flags, traps, NaN
  payloads, and dynamic rounding modes are outside 0.1.
- `NAN`/infinity conversion to integer raises `INVALID_NUMERIC_CONVERSION`.
  A finite float narrowing conversion may produce signed infinity.
- `NAN` is unordered. Use `value IS NAN` to test it; `value IS INF` and
  `value IS -INF` test the corresponding signed infinities.
- `Math` is the canonical import-free standard-library namespace for numeric
  functions, including elementary, trigonometric, rounding, `HYPOT`, and `FMA`.
- Pure `TIMESTAMP` calendar conversions use UTC; clock acquisition remains an
  explicit `HOST.clock` operation. `Math.ToDate` still needs an accepted return
  type. `Hash`, `Random`, and `Statistics` are distinct future standard-library
  surfaces.
- Invalid literal shift counts are static errors; computed invalid counts raise
  `INVALID_SHIFT_COUNT`. `SHR` is logical. Unary `-` does not widen `BYTE`,
  `UINT16`, `UINT32`, or `UINT64`; unary `+` is absent.

## Names, declarations, functions, and modules

- A function/method body is one local lexical scope. `IF` and loops create no
  nested scope. Locals may hide module names, but local names are unique.
- Instance members require `SELF.member`. `PRIVATE` is visible to all methods
  of its declaring class. Functions and constructors are not overloaded.
- `FUNCTION CONSTRUCTOR(...)` and `FUNCTION DESTRUCTOR()` close with `END
  FUNCTION`, have no `AS` return type, and may use only bare `RETURN`.
- Fields initialize before the constructor. A class without one has an implicit
  private parameterless constructor. Class names are not first-class values.
- Imports are acyclic. `A.B` resolves to `A/B.bn` below the executable module's
  directory, and aliases cannot collide with module declarations.
- `bn check` uses `0` (valid), `1` (BN error), and `2` (tool error). `bn run`
  propagates `Start`'s `0`–`255` result on normal completion.

## Values, alternatives, and control flow

- `STRUCT` and vectors have value semantics. Classes and pointers have
  identity/reference semantics. Class/pointer equality is identity; vector and
  struct equality is structural.
- `CONST` fixes a binding, not a referenced object, vector, or allocation.
- Alternative types are unordered sets with no duplicates. `IS Type` only tests
  a declared alternative; interface downcasts are outside 0.1.
- `=`, `<>`, and `IS` with declared `NULL`, `NA`, or `EOF` alternatives narrow
  values. Facts flow through `AND`, `OR`, and `NOT`, and update on assignment.
- Evaluation is left to right. `AND`/`OR` short-circuit. Unreachable code is a
  static error. 0.1 emits errors, not warnings.
- Diagnostics use stable `UPPER_SNAKE_CASE` identifiers plus file, line, column,
  source excerpt, marker, explanation, and smallest useful correction.
- `PRINT` concatenates multiple expression representations with no separator
  and terminates the output line; `PRINT` with no expression writes a blank line.
- `PRINT` uses canonical text for booleans and special values, unquoted strings,
  and portable readable decimal numbers.
- Standard-library `Error` has `Code AS INTEGER` and `Message AS STRING`.
- `Parse` raises `PARSE_ERROR`; `TryParse` returns its declared alternative
  containing `Error` for expected invalid input.

## Objects, memory, and host capabilities

- `NEW` uses manual management. Any base-pointer alias may `DELETE`; pointer
  arithmetic is absent. `NULL` access, bounds, stale handles, and a second
  delete have the respective `NULL_POINTER_ACCESS`, `INDEX_OUT_OF_BOUNDS`,
  `USE_AFTER_DELETE`, and `DOUBLE_DELETE` diagnostics.
- Zero-size allocations are valid; negative computed sizes are
  `ALLOCATION_SIZE_INVALID`. Arithmetic overflow is `ALLOCATION_SIZE_OVERFLOW`;
  host capacity failure is `ALLOCATION_TOO_LARGE`.
- Failed construction exposes no object and runs no destructor. Program-end
  recovery does not run destructors.
- 0.1 includes `HOST.main` and `HOST.clock`. `main.ArgumentCount()` includes the
  executable and `main.Argument(0)` returns its host-supplied name or path.
  `Clock.Timestamp()` returns Unix-epoch milliseconds; `Clock.Monotonic()`
  returns nanoseconds from an unspecified origin. Missing imported capabilities
  fail before `Start` with `HOST_CAPABILITY_UNAVAILABLE`.
- `HOST.memory` is deferred. BN-owned typed regions use `NEW`; shared, mapped,
  device, and FFI memory require a later capability contract.
- Static initialization is lazy and source-ordered; reentry is
  `STATIC_INITIALIZATION_CYCLE`.

## Later decisions

- `STOP` and `Start() AS INTEGER` reject literal exit codes outside `0..255`
  statically and computed invalid values with `INVALID_EXIT_CODE`.
- Counted `FOR` start, end, and `STEP` use the binding's exact integral type.
  `FOR EACH` reads the current value at each fixed-vector index.
- `STRING` has equality but no ordering. `NULL`, `NA`, and `EOF` are equal to
  themselves; `NAN` remains the only non-reflexive value.
- Bitwise binary operations use integral widening. Shift counts may have any
  integral type; the left operand determines the shift result type.
- `NEW` only instantiates classes or numeric regions. Pointer elements are
  numeric only. Fixed pointer lengths are checked and can widen to dynamic
  pointer types; dynamic-to-fixed assignment checks the length.
- `PRINT` concatenates with no separator and emits canonical, round-trippable
  float text. `INPUT()` strips its line ending and retains `EOF` after it occurs.
- Interface methods require exact callable signatures and public instance
  implementations. Static methods cannot implement an interface.

## Deferred questions

- `HOST.network`: portable sockets, address values, resolution, lifetime, and
  diagnostics.
- Function pointers: reconsider only for a concrete FFI need; ordinary BN
  callbacks use `FUNCTION(...) AS ...` values.
