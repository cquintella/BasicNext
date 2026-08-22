# Basic Next

Basic Next (BN) is an explicitly typed, object-oriented language. The project
is now implementing its Rust reference frontend and interpreter.

## Authority

- Carlos Quintella (`@cquintella`) is the BDFL; `GOVERNANCE.md` governs final
  decisions.
- The accepted language contract is, in order: `docs/language/0.1.ebnf`,
  `docs/language/0.1.md`, and `docs/language/keywords.md`.
- Do not change language behavior for implementation convenience. Report a
  conflict between normative sources instead of choosing a rule.

## Implementation

- Keep the pipeline explicit: lexer → parser/AST → semantic analysis →
  interpreter. Retain source spans throughout.
- Use simple Rust: enums, structs, standard library, and small modules. No
  speculative abstractions or dependencies.
- The lexer is handwritten; declarations/statements use recursive descent;
  expressions use precedence parsing.
- `unsafe` is forbidden unless Carlos explicitly approves a narrow use.
- LLVM is a post-0.1 backend. Do not add it before a validated typed AST exists.
- development process must be based on sprints, the execution effort is to finish Sprint by Sprint, stopping and asking if there is a doubt or info gap preventing futher development.

## Changes and checks

- A language change updates its accepted specification, grammar, keywords, and
  relevant positive/negative fixtures together.
- An implementation change adds the smallest useful test.
- Run `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`, and
  `git diff --check` for Rust changes.
- Never claim a feature works unless the relevant check ran.

## Repository hygiene

- Write tracked documentation in English; keep `.bn` examples indented.
- Preserve unrelated work. Do not commit, push, publish, or release unless
  explicitly asked.
- Never add secrets, local paths, or generated build output to Git.
