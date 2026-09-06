---
name: rust-low-level-development
description: Develop low-level Rust components such as parsers, runtimes, protocol code, and systems modules with clear invariants, focused tests, and quality checks.
---
# Low-Level Rust Compiler Development

Use this skill for implementation and review of Rust compiler frontend code where correctness depends on representation, ownership, parsing, memory layout, or deterministic runtime behavior.

## Pipeline Architecture & Modularity

- **Strict Unidirectional Pipeline:** Phases flow strictly forward: `Source` -> `Tokens` -> `AST` -> `Semantic Analysis` -> `IR`. Lower phases must never depend on higher phases.
- **Anti-God-Module Rule:** No file or module may exceed 500 lines. `lib.rs` must act exclusively as a public re-export gateway containing zero business logic or monolithic context structs.
- **Isolate State:** Avoid monolithic `CompilerState` or `Context` bags. Pass only the narrowest data subset required by a specific pass, visitor, or transformation.
- **Single Responsibility per Phase:**
  - *Lexer:* Maps source code to spanned tokens only.
  - *Parser:* Builds raw AST from tokens; performs no type checking or symbol resolution.
  - *Analyzer:* Operates via decoupled visitor traits to attach types and build symbol tables.

## Implementation & Safety

- Keep unsafe code strictly isolated in the smallest module, document safety invariants, and test the safe API.
- Prefer small `enum` and `struct` types over untyped maps, stringly state, or hidden globals.
- Preserve source spans and actionable context in parsers, diagnostics, and errors.
- Treat integer conversion, indexing, allocation sizes, and external input as checked boundaries. State overflow and error behavior explicitly.

### Idiomatic Control Flow & Error Handling

- Propagate errors using the `?` operator via `thiserror` for domain contracts and standard-library types first.
- Use `if let`, `while let`, and exhaustive `match` appropriately; prefer iterator chains (`filter_map`, `fold`, `collect`) when they directly express transformations.
- Group imports from the same crate; use struct update syntax (`..base`) to preserve ownership and default-value invariants.

## Sprint Execution & Verification

- Complete the defined sprint scope fully before handing back control; stop early only for an unresolvable technical blocker.
- Add the smallest test that fails if the changed invariant breaks, covering boundary and error cases.
- Run `cargo fmt --check`, the affected `cargo test` target, and `cargo clippy -- -D warnings` before reporting completion.
