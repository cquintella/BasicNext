# Basic Next Repository Instructions

## Scope

These instructions apply to the entire repository. Add a nested `AGENTS.md`
only when a directory develops genuinely different workflows; do not duplicate
the root rules.

Basic Next (BN) is an open-source, explicitly typed, object-oriented language
designed to reduce cognitive load and turn clear ideas into cross-platform
software. The project is specification-first and currently pre-implementation.

## Governance

- Carlos Quintella (`@cquintella`) is the creator, maintainer, and BDFL. He has
  final authority over the language, official specification, repository,
  releases, and acceptance of contributions. Follow `GOVERNANCE.md`.
- Discussion and alternatives are welcome. Do not present a proposal as an
  accepted decision until the maintainer explicitly accepts it.
- Do not commit, push, publish, create a release, or change external project
  state unless the user explicitly requests that action.

## Language and communication

- Write repository artifacts in English to support international collaboration.
- Use established technical terms when they improve precision, but prefer plain,
  direct language over jargon.
- Keep examples concrete and readable. Indentation in `.bn` files is optional
  to the language but mandatory for official examples and documentation.
- Separate current facts from future direction. Never imply that BN already has
  an interpreter, production maturity, performance results, or capabilities that
  have not been implemented and verified.
- Mark unresolved points as `TODO:` or as an explicit proposal; do not silently
  fill gaps with assumptions.

## Sources of truth

When artifacts disagree, use this authority order for the subject involved:

1. `GOVERNANCE.md` for project authority and decision rights.
2. `PHILOSOPHY.md` for enduring design principles.
3. `docs/language/0.1.ebnf` for accepted 0.1 syntax.
4. `docs/language/0.1.md` for accepted 0.1 semantics and behavior.
5. `docs/language/keywords.md` for keyword status and intent.
6. `docs/proposals/` for non-normative future designs.
7. `tests/grammar/` and `examples/` for conformance evidence; they illustrate
   the specification and must not redefine it.
8. `bucket.md`, `ROADMAP.md`, and `docs/project/` for planned work and delivery.

If two normative sources conflict, stop and report the conflict. Do not choose a
new language rule on behalf of the maintainer.

## Language-design invariants

- Prefer one canonical form over synonyms.
- Keep the core small; use libraries and `HOST` capabilities for non-fundamental
  behavior.
- Every binding has an explicit type. Do not introduce inferred declarations as
  a convenience shortcut.
- Preserve case-sensitive identifiers and exact-uppercase reserved words.
- Every compound form closes with its documented `END <KEYWORD>` form.
- A reserved future keyword is not executable syntax. In particular, `EXTENDS`
  and `PARALLEL` remain lexical-only until incorporated into a versioned grammar.
- `.bn` is the Basic Next source extension; `bn` is the command name.
- Optimize for readability, predictable local reasoning, helpful diagnostics,
  and a short path from source to feedback.

## Required workflow for a language change

Before implementing or documenting a new language feature:

1. Record the motivation, examples, alternatives, and grammar or semantic impact
   in `docs/proposals/` unless the maintainer has already made and recorded the
   decision.
2. State whether the change is accepted for 0.1, reserved, proposed, or deferred.
3. Update `docs/language/keywords.md` when a word or reserved form changes.
4. Update `docs/language/0.1.ebnf` for every accepted syntax change.
5. Update `docs/language/0.1.md` for every accepted semantic or behavioral
   change.
6. Add or update at least one valid and one invalid grammar fixture for accepted
   syntax.
7. Update official `.bn` examples when they exercise the affected construct.
8. Update `bucket.md`, `ROADMAP.md`, or the WBS when scope or delivery changes.
9. Review all affected artifacts together before declaring the change complete.

A semantic-only change may leave the EBNF untouched. A proposal must not modify
the normative grammar until it is explicitly accepted for that language version.

## Specification and grammar quality

- Keep the EBNF machine-oriented and the Markdown specification explanatory;
  do not duplicate large grammar blocks in prose.
- Grammar productions must not use catch-all rules that accept arbitrary source
  text.
- Define lexical edge cases, precedence, associativity, assignment targets,
  block terminators, blank lines, comments, and EOF behavior explicitly.
- Distinguish syntax errors, static semantic errors, and runtime errors.
- Diagnostics should teach: identify the location, explain the violated rule,
  and suggest the smallest useful correction when possible.
- Keep accepted examples conformant with the current grammar. Remove obsolete
  forms rather than maintaining undocumented aliases.

## Implementation guidance

- The reference implementation is planned in Rust. The specification decides
  behavior; implementation convenience must not create language semantics.
- Prefer a handwritten lexer, recursive-descent declaration/statement parser,
  and a precedence-based expression parser as recorded in `bucket.md`.
- Keep source spans on tokens and AST nodes from the beginning.
- Keep lexical analysis, parsing, semantic analysis, and interpretation as
  distinct stages with explicit data structures between them.
- Favor simple Rust enums, structs, standard-library facilities, and small
  modules. Do not add speculative frameworks or abstractions.
- Do not add a dependency without documenting why the standard library or an
  existing dependency is insufficient.
- Do not use Rust `unsafe` for the checked BN object/pointer runtime unless the
  maintainer explicitly approves a narrowly documented need.
- Never simulate a successful result. Run the relevant code or state clearly
  that the implementation or check does not exist yet.

## Tests and verification

For every change, run the smallest relevant checks and report exactly what ran.

- Always run `git diff --check`.
- For language changes, review the EBNF, specification, keyword registry,
  fixtures, and affected examples as one consistency set.
- Preserve both positive and negative fixtures. A negative fixture must state
  which lexical, syntax, or semantic rule it violates.
- Once the Rust workspace exists, run its configured formatter and tests for
  affected code. Do not claim parser or interpreter conformance before those
  executable checks exist.
- Do not conceal failing checks. Explain whether the failure was introduced by
  the change, pre-existing, or caused by a missing tool.

## Documentation, research, and positioning artifacts

- Keep private research in `/research/` or `/analise.md`; both are intentionally
  ignored by Git. Promote only reviewed conclusions into public documentation.
- Follow `docs/positioning/design-guide.md` for marketing and PDF artifacts.
  Positioning should be confident and technically adult, without unsupported
  hype or hostility toward other languages.
- Treat Markdown sources as authoritative for positioning papers. When a tracked
  PDF changes, update its source and visually inspect the rendered result.
- Preserve links, filenames, and `.bn` code indentation when editing documents.

## Repository safety

- Preserve unrelated user changes in a dirty worktree.
- Avoid destructive Git and filesystem operations unless explicitly requested.
- Never add credentials, access tokens, banking details, personal identifiers,
  local machine paths, or other secrets to tracked files.
- Keep generated build output and local research out of version control according
  to `.gitignore`.
