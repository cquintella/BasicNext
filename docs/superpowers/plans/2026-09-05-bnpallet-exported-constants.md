# BNPallet Exported Constants Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Support exported module-level constants so `IMPORT BNPallet AS Color; PRINT Color.BLUE` resolves and executes, with the CSS named colors exposed as `UINT32` RGB values.

**Architecture:** Add a top-level `Item::Constant` AST node carrying its exported name, type, and literal initializer. Semantic module exports will include constant types; IR lowering will resolve module constant members to typed `Constant` instructions using the imported module's source declaration, while existing class/member behavior remains unchanged.

**Tech Stack:** Rust frontend, handwritten lexer/parser, semantic analyzer, typed IR, interpreter, LLVM backend, `.bn` standard-library modules.

**Spec:** User-approved API: `IMPORT BNPallet AS Color` followed by `Color.BLUE`; values are CSS/W3C named sRGB colors encoded as `UINT32` `0xRRGGBB`.

## Global Constraints

- Do not add a Rust runtime provider for the palette values.
- Preserve the explicit import requirement for `BNPallet`.
- Keep `+` and comma `PRINT` semantics unchanged from the completed PRINT change.
- Use `UINT32` because 24-bit RGB values do not fit in `INT16`.
- Preserve source spans and reject non-literal or non-constant top-level initializers.

### Task 1: Parser and AST

**Files:** `src/ast.rs`, `src/parser/phase1.rs`; test in `tests/parser.rs`.

- [ ] Add `Item::Constant { exported, name, type_ref, initializer, span }`.
- [ ] Parse `EXPORT CONST NAME AS TYPE = literal` before ordinary declarations.
- [ ] Reject unexported top-level constants and non-literal initializers with diagnostics.
- [ ] Add a parser test for `EXPORT CONST BLUE AS UINT32 = 0x0000FF`.

### Task 2: Semantic exports

**Files:** `src/semantic/analyzer1.rs`, `src/semantic/module_analysis.rs`, `src/semantic.rs`; test in `tests/module_graph.rs` or `tests/semantic.rs`.

- [ ] Include exported constants in `module_exports` with their resolved type.
- [ ] Validate the initializer against the declared type and record the constant value for lowering.
- [ ] Resolve module member lookup (`Color.BLUE`) as the exported constant type.
- [ ] Reject duplicate module exports.

### Task 3: IR and execution

**Files:** `src/semantic.rs`, `src/ir/model.rs` only if required, `src/ir/builder/expressions.rs`, `src/ir/helpers.rs`.

- [ ] Preserve constant literal values in semantic resolution or a module constant table.
- [ ] Lower imported module constants to `Instruction::Constant` with `Constant::Integer`.
- [ ] Ensure no runtime provider or class storage is emitted.
- [ ] Add an IR/runtime regression executing `Color.BLUE` and `Color.REBECCAPURPLE`.

### Task 4: BNPallet module and documentation

**Files:** rename `modules/bn/BNColors.bn` to `modules/bn/BNPallet.bn`; update `tests/grammar/valid/bncolors.bn` to `bnpallet.bn`; update language/library docs as needed.

- [ ] Replace the class-based module with 148 `EXPORT CONST ... AS UINT32` declarations.
- [ ] Keep CSS aliases and exclude non-RGB `transparent` and `currentColor`.
- [ ] Document the import and RGB encoding.

### Task 5: Verification

- [ ] Run the focused parser, semantic, runtime, and CLI tests.
- [ ] Run `cargo fmt --check`, `cargo test --all-targets -- --test-threads=1`, and `git diff --check`, reporting unrelated baseline failures separately.
