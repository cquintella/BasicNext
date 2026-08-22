# Basic Next 0.1 Frontend Audit

## Status

Audited against the normative grammar and specification on 2026-08-22.

The lexical grammar and surface syntax are complete for the declared 0.1
scope. The specification also defines the core runtime and the accepted
object, pointer, module, and base `HOST` behavior. Features listed under
`Still undecided`, future proposals, and post-0.1 work are intentionally not
part of the 0.1 contract.

The implementation is conformant for the core pipeline and runtime:

```text
source -> tokens -> syntax AST -> semantic model -> typed BN IR -> execution
```

The Sprint 7 frontend loads an acyclic project-root module graph, resolves
imports and exports, and performs extended semantic checks for objects,
interfaces, pointers, temporal values, and the accepted `HOST` surface. The
extended 0.1 runtime is not complete: module and static execution, object and
interface dispatch, checked allocation lifecycles, and temporal/`HOST`
execution are delivered by Sprints 9 through 12.

## Defects found and corrected

- Binary AST nodes discarded the exact operator and retained only a broad
  precedence category. The AST now preserves `Plus`, `Slash`, `NotEqual`, and
  every other exact operator.
- Function signatures with parameters confused a parameter `AS` with the
  return-type `AS`. Parameter boundaries and return types are now distinct.
- Declaration AST nodes discarded `EXPORT`; type AST nodes discarded vector
  dimensions. Both are now preserved.
- Return-path analysis incorrectly required an intermediate nested block to
  return even when a later function-level `RETURN` covered every path.
- Alternative narrowing was not propagated into `ELSE` branches.
- Member types and exact interface signatures were not checked.
- Numeric operator typing, compound assignment result types, multidimensional
  vectors, and contextual integer expressions were incomplete.
- Imports were parsed in isolation. The frontend now builds an acyclic
  `ModuleId` graph and checks export visibility across source files.
- Constructor, member, pointer-shape, deletion, temporal, and base `HOST`
  expressions lacked extended semantic validation. Sprint 7 adds those checks
  before IR lowering.
- The official RPN example used implicit instance-member access contrary to
  the normative `SELF.member` rule. It now uses the canonical form.

Each correction has an executable regression test.

## Executable evidence

- Every valid grammar fixture passes `bn check`.
- Every invalid grammar fixture is rejected.
- Every official example passes lexical, syntax, and semantic checking.
- `examples/factorial.bn` and `examples/hello.bn` execute through typed BN IR.
- `examples/language-tour.bn` exercises the complete accepted frontend surface,
  including constructs whose extended runtime arrives in Sprints 9 through 12.

## Remaining risks

- `examples/shortest_path.bn` is a one-line design placeholder, not an
  executable example.
- `bn check` loads and validates imported modules; multi-module execution and
  cross-module static initialization remain Sprint 9 runtime work.
- Object/interface dispatch and checked heap lifecycle execution remain Sprints
  10 and 11 work after Sprint 7 semantic validation.
- The current IR is a typed mutable-register CFG, not SSA. This is intentional
  for the reference interpreter; LLVM lowering may construct SSA later.
- Standard-library `Hash`, `Random`, and `Statistics` remain proposals.
  `HOST.clock`, the accepted temporal namespaces, and the `Math` UTC conversion
  functions have frontend types; their runtime integration remains Sprint 12
  work.
