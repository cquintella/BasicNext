# Memory ownership and reference-counting audit — 0.5

Status: internal audit; implementation recommendation, not a closed contract.

## Current implementation

| Category | Current representation | Lifetime mechanism | Assessment |
| --- | --- | --- | --- |
| Primitive numbers, booleans, dates, times | `Value` inline variants | Rust value move/clone | Appropriate; no language-level refcount is needed. |
| `STRING` | `Value::String(String)` | Rust `String` ownership; cloning copies the string | Memory-safe, but not shared/refcounted. |
| Vectors | `Value::Vector(Vec<Value>)` | Rust `Vec` ownership; cloning recursively clones values | Memory-safe, but not a shared vector model and potentially expensive. |
| Records | `Value::Record { fields: HashMap<...> }` | Rust ownership; fields are cloned | No retain/release of nested language handles. |
| Class objects | `Value::Object { handle }`, stored in `Heap<Instance>` | Explicit `DELETE`; generation/live flags detect stale handles | No reference counting; aliases can double-delete or become stale. |
| Pointer regions | `Value::Pointer { handle }`, stored in `Heap<Value>` | Explicit `DELETE`; generation/live/destroying flags | Checked handles prevent silent use-after-delete, but aliases are not owners. |
| Host resources | Numeric IDs or handles in executor maps | Explicit close/delete plus container drop | Per-resource policies vary; no uniform language ownership contract. |

There is no `Rc`, `Arc`, `ref_count`, `retain`, or `release` path for BN
values. `Heap<T>` tracks slot generation and destruction state, not references.
The current executor therefore has handle safety, not reference-counted
ownership.

## Verdict

### Gap against the founding design intent

The founding intent was to model the reference-counting style used by Swift
ARC and Objective-C object ownership. The current implementation does not yet
meet that intent. It has a checked handle registry and explicit destruction,
but no compiler/runtime retain-release protocol for language values. This must
be tracked as an architectural gap, not described as an intentional simplified
ownership model.

The mechanism is adequate for short-lived `bn run` executions where the
executor owns all registries and explicit `DELETE` is disciplined. It is not
adequate as the final model for a persistent `bnr` session or shared values:

- copying an object/pointer copies its handle without retaining it;
- deleting one alias invalidates every other alias;
- deleting aliases independently produces `DOUBLE_DELETE`;
- overwriting a symbol only drops the Rust `Value`, so pointed allocations are
  not released by reference count;
- nested pointers/objects in fields are not recursively released when an
  instance payload is cleared;
- long-lived sessions can accumulate unreachable heap regions and host-table
  entries until executor teardown.

Generation checks make stale accesses diagnosable, but they do not reclaim
unreachable allocations and do not establish ownership. Reference-counting
alone also cannot collect cyclic object graphs.

## Recommended target model

Keep primitives as value types. Represent every shareable language allocation
with a managed entry containing `{generation, ref_count, state, payload}`.
`Handle` copies must call `retain`; binding overwrite, scope exit, error paths,
and `DELETE` must call `release`. The allocation is destroyed exactly once
when the count reaches zero, then its generation is advanced.

For vectors, use an immutable/shared allocation with length and capacity;
mutating operations require uniqueness or copy-on-write. Elements that are
managed values retain their children. A `Split` result should transfer one
managed handle to the receiving binding, not expose a raw address.

For objects, retain/release must cover fields and destructor execution must be
exactly-once. Either prohibit owning cycles in the 0.5 language contract or
add weak references/cycle collection before allowing cyclic object graphs.

For host resources, keep explicit close as an idempotent logical operation,
but make the underlying registry entry reference-safe and ensure executor,
session reset, cancellation, and failed constructors release all handles.

The first implementation slice should instrument retain/release counters and
add tests for aliasing, reassignment, return temporaries, destructor failure,
use-after-delete, REPL reset, and allocation cleanup. The ABI contract must
then be completed in `docs/architecture/value-memory-abi.md` before claiming
the memory model is closed.

The requested normative target is now recorded in
`todo/arc-contract-0.5.0.md`: Swift-style ARC semantics for managed
references, with value semantics for primitives, strings, vectors, and
records.
