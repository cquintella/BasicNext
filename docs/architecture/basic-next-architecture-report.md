# Basic Next Architecture Report

## 1. Overview

Basic Next is a language toolchain with one semantic pipeline:

```text
.bn source -> lexer -> parser/AST -> semantic analysis -> BN IR validation
                                                        |                |
                                                        v                v
                                                bn_runtime          bn_llvm -> LLVM/clang
```

The architectural contract is that interpretation and compilation consume the
same language-level BN IR. LLVM is a backend and is not the language
specification. The normative architecture is documented in
[`README.md`](README.md) and [`target-architecture.md`](target-architecture.md).

The repository is in a migration from the original root package to a crate
graph. The target design is modular and acyclic, while some interpreter and
tooling code remains in `src/`.

## 2. Component classification

| Component | Classification | Main responsibility |
| --- | --- | --- |
| `bn_frontend::lexer` | Syntactic frontend | Text to tokens |
| `bn_frontend::parser` | Syntactic frontend | Tokens to AST |
| `bn_frontend::semantic` | Semantic frontend | Names, types, calls and rules |
| `bn_frontend::module_graph` | Resolution layer | Imports and module providers |
| `bn_frontend::lowering` | Frontend/IR boundary | Typed AST to BN IR |
| `bn_ir` | Intermediate representation | Language-owned program model |
| `bn_ir::validate` | Contract verification | Reject malformed IR |
| `src/runtime` / `src/runtime_impl.rs` | Interpretive runtime | Execute validated IR |
| `bn_rt` | Native runtime | C ABI for compiled programs |
| `bn_llvm` | Compiler backend | BN IR to textual LLVM IR |
| HOST providers | Platform boundary | Network, files, console, time and dispatch |
| `bn_diag` | Cross-cutting infrastructure | Stable diagnostics and rendering |
| `bn_source` | Cross-cutting infrastructure | Source IDs, revisions and spans |
| `bn_types` | Shared type identity | Types used by frontend and IR |
| `bn_value` | Runtime value model | Values and resource handles |
| `bn` / `bnc` | CLI/orchestration | Check, run, build and policy options |
| LSP/DAP code | Tooling integration | IDE diagnostics and debugging |
| LLVM/clang | External build interface | Assembly, linking and final artifact |

## 3. Lexer

The lexer in `crates/bn_frontend/src/lexer.rs` is handwritten. It recognizes
keywords, identifiers, numbers, strings, symbols, comments, newlines and
special literals. Tokens retain source spans.

This is a good fit for Basic Next's small, deliberately controlled language:
it avoids generator dependencies, gives direct control over diagnostics and
makes source tracking straightforward. The cost is that every lexical change
requires manual code and boundary tests. The main long-term risk is divergence
between the lexer and the normative EBNF.

## 4. Parser and AST

The parser in `crates/bn_frontend/src/parser/` is handwritten and split by
grammar phase. It uses recursive descent for declarations, blocks, statements
and types. Expressions use precedence climbing, handling prefix operators,
binary operators, calls, member access, indexing, casts, `IS`, `NEW`, vectors,
`ASYNC` and `AWAIT`.

Advantages are simple control flow, precise spans and easy diagnostics. The
tradeoff is maintenance: grammar rules are distributed among parser modules,
and there is no parser generator to mechanically enforce correspondence with
the EBNF. Parser fixtures are therefore part of the language contract.

The AST in `ast.rs` is a syntax model. It should remain a frontend concern;
backends must consume the lowered IR rather than AST nodes or semantic
frontend types.

## 5. Semantic analysis

`crates/bn_frontend/src/semantic/` resolves names and checks the language's
static rules. It covers scopes, declarations, types, operand compatibility,
boolean conditions, function signatures, returns, members, references,
imports, HOST calls and definite assignment.

This is the main semantic protection layer. It prevents each backend from
inventing its own type rules and allows interpretation and compilation to start
from the same meaning. Its cost is complexity around alternative values such as
`INTEGER OR NA OR Error`, object members, managed values and provider methods.

## 6. Module graph and FrontendSession

`module_graph.rs` loads imported `.bn` modules, assigns module IDs and records
standard providers such as BNData, BNLog, BNMath and HOST. Ordered
`--module-path` resolution is separate from the programs directory.

`FrontendSession` is the intended shared lifecycle for CLI, LSP and DAP. Its
purpose is to manage snapshots, unsaved documents, revisions, invalidation,
cancellation and revision-scoped diagnostics. This prevents IDE analysis from
becoming a weaker parallel frontend.

The advantage is consistent language behavior across tools. The risk is state
management: module loading, cached analysis and unsaved revisions must never
produce diagnostics from the wrong source revision.

## 7. Lowering and BN IR

The lowering layer converts typed AST into the language-owned IR. BN IR models
modules, functions, blocks, values, symbols, constants, calls, control flow,
vectors, allocation, members, HOST operations and resource operations.

`bn_ir` is intentionally independent of `bn_frontend`. This separation is
valuable because it gives the project one stable handoff format and allows
multiple backends. It also makes validation explicit.

The cost is that every language feature crosses several contracts: semantic
analysis, lowering, IR validation, interpreter behavior, LLVM support and
native ABI. An incomplete IR surface cannot be repaired safely with a backend
special case.

## 8. IR validation

`bn_ir::validate` verifies definitions, uses, operands, blocks, terminators,
references and definite assignment. It must reject malformed language IR with
stable language diagnostics before either backend executes it.

The architecture distinguishes:

```text
invalid IR
valid IR unsupported by a target
valid IR supported by a target
```

That distinction is one of Basic Next's strongest design choices. The main
risk is incomplete use enumeration or confusing target limitations with
language-invalid programs.

## 9. Interpretive runtime

The interpreter in `src/runtime/` and `src/runtime_impl.rs` executes validated
BN IR directly. It implements control flow, values, objects, vectors, memory,
HOST providers, standard modules, errors, resources and dispatch.

It is fast to iterate and useful as a reference execution path, but it is not
the specification. A bug in the interpreter must not become mandatory compiler
behavior. The principal risk is semantic drift between this runtime and
`bn_rt`.

## 10. Native runtime and ABI

`crates/bn_rt` is the runtime linked by LLVM-generated programs. It exposes
functions for console, clock, random, math, statistics, network, sockets,
dispatch, policy, strings, DataFrame operations and logging.

This keeps LLVM emission small and centralizes operating-system interaction.
The price is ABI sensitivity: pointer layout, aggregate layout, error unions,
ownership and handle lifetime must match exactly between `bn_llvm` and `bn_rt`.
The C ABI is also an unsafe boundary and requires hostile-argument validation,
focused tests and explicit ownership rules.

## 11. LLVM backend

`crates/bn_llvm` consumes validated BN IR, checks target support, computes data
layouts and emits textual LLVM IR. It declares calls into `bn_rt`; clang and
the linker remain external tools.

Textual emission is lightweight and portable across LLVM installations. It
also makes the backend responsible for exact aggregate types, calling
conventions, integer widths, overflow behavior, pointer lifetimes and platform
symbols. LLVM poison or `nsw` cannot be used as a substitute for BN overflow
semantics.

The support matrix is therefore a real product boundary. An unsupported valid
IR program must fail with `TARGET_UNSUPPORTED_*`, while a supported example
must compile, link and satisfy parity/effect checks.

## 12. HOST and execution policy

HOST is the language's built-in platform interface. Current families include
console, clock, random, filesystem, network and dispatch. BNData, BNLog and
BNMath are external standard modules that use explicit imports.

The architecture separates three questions:

```text
Does the program require the capability?
Does the target implement it?
Does the execution policy permit it now?
```

Capability declarations make dependencies visible and policy checks provide a
runtime security boundary. The disadvantage is implementation cost: each
provider needs specification, interpreter behavior, native ABI and target
matrix evidence. A provider implemented only in the interpreter is not native
support.

## 13. Diagnostics, source identity and shared values

`bn_diag` centralizes codes, severities, messages and spans. `bn_source` gives
all layers source identity and revision tracking. `bn_types` supplies shared
type identities, and `bn_value` contains runtime values and resource handles.

These leaves reduce duplication and allow CLI, LSP and DAP to describe the same
failure. They also create an important boundary: semantic implementation
details must not leak into the public IR model, and resource values must have a
clear owner and lifetime.

## 14. CLI, LSP, DAP and LLVM tools

The `bn` command orchestrates check, run and build. `bnc` is a compiler-facing
wrapper. They should remain thin: parsing, semantic analysis, HOST catalogs and
backend rules belong to library crates.

LSP and DAP provide editor and debugger integration. Their main advantage is
reuse of the same frontend and IR. Their main risk is accidental parallel
analysis or execution paths that disagree with `bn check` and `bn run`.

LLVM tools are an external interface. This avoids embedding the full LLVM
library but makes installation, platform symbols and toolchain versions part of
the build environment.

## 15. Dependency architecture

The target dependency direction is an acyclic graph:

```text
bn_source / bn_diag / bn_types / bn_value
                         ↓
                       bn_ir
                         ↓
                    bn_frontend
                    ↙          ↘
             bn_runtime      bn_llvm
                                  ↓
                                bn_rt
```

The DAG improves compilation boundaries, API clarity and test isolation. The
current disadvantage is migration debt: parts of the interpreter and tooling
still live in the root package, so physical crate separation is not yet the
same as complete behavioral separation.

## 16. Overall assessment

The architecture is appropriate for Basic Next. Its strongest decisions are:

- one frontend-to-IR pipeline;
- a language-owned validated IR;
- interpreter and compiler as separate consumers;
- explicit HOST capabilities and execution policy;
- stable diagnostics with source identity;
- a modular dependency direction;
- a documented, verifiable LLVM support subset.

The most important risks are ABI drift between `bn_llvm` and `bn_rt`, semantic
drift between interpreter and native providers, incomplete provider surfaces,
complex alternative-value layouts, textual LLVM fragility and the remaining
monolith-to-DAG migration.

The project does not need a different high-level architecture. It needs to
finish the contracts between the existing layers, keep unsupported target
behavior explicit, and verify every claimed native feature with fixtures that
cover values, errors, effects and ownership rather than stdout alone.
