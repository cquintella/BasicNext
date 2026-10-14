# Proposal: LLVM Backend Modularization and BNSqlite Crate Segregation

- **Author:** Carlos Quintella / Basic Next Agent
- **Date:** 2026-10-14
- **Status:** Proposed (Priority Architectural Debt)
- **Target:** Post-0.6.5 Structural Refactoring Bucket

---

## 1. Context and Motivation

During the 0.6.5 audit and stabilization sprint, two significant architectural boundary violations were identified:

1. **Monolithic Backend (`crates/bn_llvm`)**:
   `crates/bn_llvm` contains over 25,500 lines of code across 72 source files. It violates the Clean Architecture and Single Responsibility principles by mixing:
   - Low-level LLVM IR syntax and text emission.
   - High-level memory layout calculations, struct offsets, and target ABIs.
   - Automatic Reference Counting (ARC) runtime glue, destructor sequences, and copy/destroy witness generation.
   - High-level library and domain-specific code generation (e.g., vectors, SQLite operators, dispatch trampolines).

2. **Incomplete Isolation of BNSqlite**:
   In previous architectural migrations, standard libraries were properly decoupled into domain crates (`bn_core_web`, `bn_core_net`, `bn_core_math`, `bn_core_log`), with `bn_lib_*` serving `bni` and `bn_rt` serving `bnc`.
   However, `BNSqlite` was left scattered:
   - `crates/bn_lib_sqlite` implements the interpreter side using `rusqlite` directly.
   - `crates/bn_rt/src/sqlite_abi.rs` duplicates SQLite handle binding and C ABI exports.
   - `crates/bn_llvm/src/llvm/sqlite_ops.rs` hardcodes SQLite method emission inside the compiler backend.
   SQLite is an optional external library, not part of the language core, and must be isolated into autonomous crates.

---

## 2. Target Architecture

### A. BNSqlite Segregation

* **`crates/bn_core_sqlite`**:
  - Independent domain crate holding portable SQLite operations, value conversions (INT, FLOAT, STRING, BLOB, NULL), parameter binding, and query metadata.
  - Zero dependencies on `bn_frontend`, `bn_ir`, or `bn_llvm`.
* **`crates/bn_lib_sqlite`**:
  - Minimal adapter for `bni` (interpreter) implementing `HOST.Provider` or external module interfaces backed by `bn_core_sqlite`.
* **`crates/bn_rt_sqlite`** (or feature-gated `bn_rt` module):
  - Native runtime C ABI exports (`bn_rt_sqlite_*`) delegating to `bn_core_sqlite`.

### B. LLVM Compiler Backend Modularization

The monolithic `crates/bn_llvm` will be partitioned into four cohesion-focused crates with strict unidirectional dependencies:

```
bnc (CLI driver) / bn_compile_driver
         │
         ▼
     bn_llvm (Facade / Coordinator / Lowering)
     ┌───┴───────────────┬─────────────────┐
     ▼                   ▼                 ▼
bn_llvm_ops         bn_llvm_arc       bn_llvm_layout
     │                   │                 │
     └───────────┬───────┴─────────────────┘
                 ▼
            bn_llvm_ir (IR AST, Types, Text Emission)
```

1. **`crates/bn_llvm_ir`**:
   - Pure typed representation of LLVM IR (instructions, basic blocks, functions, globals, metadata, linkage, calling conventions).
   - LLVM text serialization and formatter (`write_declarations`, `format_instruction`).
   - Zero dependencies on Basic Next AST, semantic analyzer, or runtime ABI.

2. **`crates/bn_llvm_layout`**:
   - Memory layout and ABI calculation derived from validated BN IR.
   - Canonical constants (`POINTER_BYTES`, `FAT_POINTER_BYTES`, alignments).
   - Struct offsets, field slots, padding, and type mappings (`llvm_type`).

3. **`crates/bn_llvm_arc`**:
   - Native ARC runtime glue emission (`bn_arc_retain`, `bn_arc_release`, `bn_arc_destroy`).
   - Witness functions (`__bn_copy_*`, `__bn_destroy_*`) for deep copying and dropping managed values and aggregates.
   - Dependent only on `bn_llvm_ir` and `bn_llvm_layout`.

4. **`crates/bn_llvm_ops`**:
   - Lowering routines for specialized operations:
     - Vector memory lifecycle (`malloc`, `memcpy`, `free`, bounds checking).
     - String slicing, concatenation, and formatting.
     - Dispatch trampolines and task state machines.
     - Foreign library call marshalling.

5. **`crates/bn_llvm` (Facade)**:
   - Retains the public compiler driver interface (`compile_module`).
   - Traverses validated `bn_ir::Module`, orchestrating the sub-crates without accumulating inline operational logic.

---

## 3. Benefits

* **Strict Enforceability of Clean Code & DRY**: each sub-crate has a single reason to change; magic literals and duplicate helper functions are structurally eliminated.
* **Compilation Speed**: granular crates allow parallel compilation by `cargo` and reduce incremental rebuild times.
* **Independent Testability**: unit testing layout calculations or IR emission without instantiating the entire compiler pipeline.
* **Reusability**: `bn_llvm_ir` and `bn_llvm_layout` can be reused by future tooling (e.g., JIT emitters, ahead-of-time analyzers, debug symbol generators).

---

## 4. Implementation Plan

1. **Phase 1: BNSqlite Separation**:
   - Extract shared SQLite conversion and error mapping into `crates/bn_core_sqlite`.
   - Refactor `bn_lib_sqlite` and `bn_rt/src/sqlite_abi.rs` to consume `bn_core_sqlite`.
   - Generalize native method call emission in `bn_llvm` so SQLite does not require ad-hoc compiler support.

2. **Phase 2: LLVM IR & Layout Extraction**:
   - Move `crates/bn_llvm/src/ir/*` into `crates/bn_llvm_ir`.
   - Move `crates/bn_llvm/src/llvm/layout.rs` and related ABI logic into `crates/bn_llvm_layout`.

3. **Phase 3: ARC & Ops Extraction**:
   - Move `arc_runtime.rs` and witness emitters into `crates/bn_llvm_arc`.
   - Move `vectors.rs`, `sqlite_ops.rs`, `dispatch.rs` into `crates/bn_llvm_ops`.

4. **Phase 4: Source Size Ratchet Adjustment**:
   - Update `tests/source_size.rs` to apply individual ceilings to each new crate, retiring the aggregate 25,600 line ceiling.
