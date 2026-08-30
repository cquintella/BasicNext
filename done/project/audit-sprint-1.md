# Sprint 1 Audit — Language OO

**Status:** Complete

## Normative sources reviewed

- `docs/language/0.2/0.2.ebnf`
- `docs/language/0.2/0.2.md` — String indexing, Class inheritance, and
  Interfaces in 0.2
- `docs/language/0.2/keywords.md`
- `ongoing/WBS-0.2.md`
- `ongoing/bucket.md`

## Requirement matrix

| Requirement | Evidence | Result |
| --- | --- | --- |
| `EXTENDS` and `SUPER` parse | Parser tests and valid inheritance fixture | pass |
| Base resolves to exactly one class in scope | Local and imported inheritance runtime fixtures | pass |
| Inheritance graph is acyclic | `inheritance-cycle.bn` | pass |
| Public members inherit; private members remain inaccessible | Runtime and negative fixtures | pass |
| Field redeclaration and incompatible override fail | Negative fixtures and inherited-static runtime fixture | pass |
| Exact public instance override dispatches virtually | Runtime fixture | pass |
| `SUPER(...)` arguments, position, implicit call, and destructor order | Runtime and negative fixtures | pass |
| `SUPER.Name(...)` resolves nearest public ancestor only | Runtime fixture | pass |
| Dispatch is pinned during construction and destruction | Runtime fixture | pass |
| Inherited interfaces and duplicate interface entries | Module/runtime and negative fixtures | pass |
| Qualified imported interface | Multi-module check and run fixture | pass |
| Read-only Unicode-scalar string index | Runtime and negative fixtures | pass |

## Direct CLI evidence

- `cargo run --quiet -- check tests/grammar/valid/inheritance.bn` — pass.
- `cargo run --quiet -- run tests/grammar/valid/inheritance.bn` — pass.
- `cargo run --quiet -- check tests/grammar/invalid/inheritance-cycle.bn` —
  rejected with `INHERITANCE_CYCLE`.
- `tests/modules/imported-inheritance/main.bn` — executes imported base,
  inherited field, `SUPER()`, parent method, and imported-base upcast.

## Quality gates

- `cargo fmt --check` — pass.
- `cargo test` — pass.
- `cargo clippy -- -D warnings` — pass.
- `git diff --check` — pass.

## Open requirements

None.

**Completion decision:** Complete.
