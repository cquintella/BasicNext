# Semantic analysis contract (Frontend 2.5) — to-be

> Canonical: `docs/architecture/semantic-analysis.md`  
> Status: **architecture requirements locked 2026-09-05** for *what* static analysis must cover. Detailed rules remain in [`../language/0.4/0.4.md`](../language/0.4/0.4.md); this file is the toolchain contract checklist for process **2.5 Semantic analysis**.

## Purpose

After **lexical** (2.2) and **syntactic** (2.3) analysis (and module-graph assembly in 2.4), **semantic analysis** must decide whether a program is statically well-formed **before** lowering to BN IR (3.0). Interpret and compile must not invent a second meaning from an unchecked AST.

The language document (`0.4.md`) is normative for individual rules. This contract states the **mandatory categories** the Frontend must implement and report via **D3 Diagnostics**. A job that fails these checks must not present a “validated IR” that pretends the program was fine.

## Layering

| Layer | Process | Responsibility |
| --- | --- | --- |
| Lexical | 2.2 | Tokens |
| Syntactic | 2.3 | AST (matches EBNF) |
| Module graph | 2.4 | Imports / load set via [module-path.md](module-path.md) |
| **Semantic (this contract)** | **2.5** | Definitions, types/operands, conditions, calls/returns, references → **D_sym** |
| Lower / validate IR | 3.0 | BN IR shape + **CFG definite assignment** of values; does **not** replace 2.5 |
| Interpret | 4.0 | **Executable reference** for dynamic behaviour (subordinate to the spec), not static checks |

IDE / `bnc --check` must use the **same** 2.5 path as the CLI.

---

## Required analyses (normative checklist)

The contract **requires** that semantic analysis cover at least the following. Each category must produce diagnostics with stable codes and source spans when violated (expressive Fluent rendering is a separate 0.4.5 track; the *facts* still land in **D3**).

### 1. Definitions available by path

> **Not control-flow definite assignment.** This section is **name binding** (modules, scope, imports). Ensuring every **IR value** is defined on **all executable paths** that reach a use is an **`ir::validate` / CFG** obligation — see [ir-contract.md](ir-contract.md) § Definite assignment. Frontend name resolution does **not** detect a defective IR produced by lowering.

The analyzer must establish, for every name use that refers to a definition:

- Which **module** and **declaration** it binds to, after `IMPORT` resolution along the ordered **module-path** (and HOST capability imports where applicable).
- That the definition is **in scope** and **visible** under language rules (exports, aliases, same-module vs imported names).
- That resolution is **unambiguous** (no illegal overload/clash unless the language defines one).
- That missing modules / missing members are errors — not silent fallbacks.

**Out of scope for “path” here:** filesystem layout policy (that is [module-path.md](module-path.md)); this item is the *binding* of names to definitions once modules are loaded.

### 2. Operand compatibility

For every operator and typed operation in the program (arithmetic, comparison, indexing, member access, conversions, pointer ops, etc.):

- Operand types must be **compatible** with the operator under `0.4.md` rules.
- Illegal combinations are static errors (not deferred to runtime when the language marks them static).
- Implicit conversions, if any, must follow the documented rules only — no ad hoc widening in the Frontend.

### 3. Boolean conditions

Wherever the language requires a condition (`IF`, `WHILE`, and any other conditional forms in the grammar):

- The condition expression’s static type must be **boolean** (or the single documented coercion, if `0.4.md` defines one — none may be invented here).
- Non-boolean conditions are static errors.

### 4. Call signatures and returns

For every call (functions, methods, HOST members, bound forms as the language allows):

- The callee must resolve to a callable definition (**§1**).
- **Arity** and **argument types** must match the signature.
- **Return type** use at the call site must be consistent (including `VOID`, error-union / `OR Error` forms as specified).
- Return statements inside a callable must match that callable’s declared result type.

### 5. Valid references

Beyond bare name binding, the program must not contain **invalid references**, including at least:

- Use of undeclared names.
- Illegal use of a bound name (wrong kind: type used as value, value used as type, etc., per language rules).
- References that violate definite language constraints already specified (e.g. static initialization cycles, illegal `HOST.Args` use in non-executable modules, interface/impl obligations) — as listed in `0.4.md`.
- No “ghost” symbols that do not exist in **D_sym** after a successful 2.5.

---

## Outputs of a successful 2.5

On success (no blocking errors), **D_sym** holds a consistent semantic model sufficient for **3.0 Lower**: every reference needed for lowering is resolved; types needed for lowering are known. Warnings may remain in **D3** without blocking, per diagnostic policy.

On failure, Control / check must surface diagnostics; Backend must not run as if the program were valid.

## Relationship to the IR contract

[ir-contract.md](ir-contract.md) assumes Frontend semantic obligations are met before validated IR is advertised. Process **3.2** may re-check structural IR invariants; it is **not** a second full language typechecker. Duplicating all of §1–§5 only in IR validation would be an architecture smell.

## Traceability

| Item | DFD | Stores | Language |
| --- | --- | --- | --- |
| §1–§5 | [dfd-2/2.0 Analyze Sources.md](dfd/dfd-2/2.0 Analyze Sources.md) process **2.5** | **D_sym**, **D3**; reads **D_ast**, **D_graph** | [`0.4.md`](../language/0.4/0.4.md) static rules |
| Module load paths | 2.1 / 2.4 | **D1**, **D_graph** | + [module-path.md](module-path.md) |

## See also

- [dfd/dfd-2/2.0 Analyze Sources.md](dfd/dfd-2/2.0 Analyze Sources.md)
- [ir-contract.md](ir-contract.md)
- [module-path.md](module-path.md)
- [`../language/0.4/0.4.ebnf`](../language/0.4/0.4.ebnf) (syntax only)
