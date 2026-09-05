# Interpret × LLVM support matrix (stub)

> Canonical: `docs/architecture/support-matrix.md`  
> **Status:** **stub** — **2026-09-05**  
> Rows marked **EXAMPLE** are illustrative placeholders only. They are **not**
> claimed coverage from the codebase. Do not treat EXAMPLE rows as normative
> until filled from `bn_ir` / runtime / llvm evidence and reviewed.

---

## Purpose

The oracle rule says **interpret** defines language+HOST meaning; **LLVM /
native / wasm** compile paths implement a **documented subset**. This matrix
is the public place to record, per BN IR operation or language feature:

- whether **interpret** supports it;
- whether **llvm/native** compile supports it;
- whether **wasm** compile supports it;
- notes (diagnostics codes on reject, HOST dependency, tracking issues).

CI and `validate_for(Backend::Llvm)` (target) should eventually fail closed
outside the matrix with stable **Diagnostic** codes (bucket **0.4.5** / SM5
G2 direction). Until the matrix is filled from code, unsupported-for-llvm
behavior remains an implementation fact — document it here as you discover it.

Related:

- [ir-contract.md](ir-contract.md) — checklist item “LLVM subset matrix”
- [`../../ongoing/bucket-0.4.5.md`](../../ongoing/bucket-0.4.5.md) — §2 IR contract minimum / support-matrix activities
- [milestones-map.md](milestones-map.md) — XM0 (document matrix), XM10 (`validate_for`)
- [target-architecture.md](target-architecture.md) — principle 1 (oracle)

---

## Status

| Field | Value |
| --- | --- |
| Completeness | **Stub** — not filled from code |
| Normative? | **No** (except the purpose and column definitions) |
| Next step | Inventory ops from IR model + llvm emission; replace EXAMPLE rows |

---

## Columns

| Column | Meaning |
| --- | --- |
| **BN IR op / language feature** | Instruction kind, HOST op, or language feature under test |
| **interpret** | Supported by the oracle runtime? (`yes` / `no` / `partial`) |
| **llvm/native** | Supported by compile-to-native path? |
| **wasm** | Supported by compile-to-wasm path? |
| **notes** | Codes, limitations, links |

---

## Example placeholder rows (NOT real coverage)

> **WARNING:** The following rows are **EXAMPLE** fiction for table shape only.
> Delete or replace them when real inventory lands. They must not be cited as
> product guarantees.

| BN IR op / language feature | interpret | llvm/native | wasm | notes |
| --- | --- | --- | --- | --- |
| EXAMPLE: integer add / arithmetic basics | yes | yes | yes | EXAMPLE placeholder — replace from code |
| EXAMPLE: HOST.Console write | yes | partial | partial | EXAMPLE — native/wasm bridging TBD |
| EXAMPLE: HOST.Net connect | yes | no | no | EXAMPLE — often interpret-only until matrix says otherwise |
| EXAMPLE: dataframe column op | yes | no | no | EXAMPLE — track with `bn_value` / dataframe extract |
| EXAMPLE: debug breakpoint nop | yes | n/a | n/a | EXAMPLE — DAP/interpret only |

---

## Filling rules (when leaving stub status)

1. Every row must cite evidence (test name, source module, or issue id) in
   **notes** or a linked appendix.
2. Prefer IR-level rows over vague language marketing names; map language
   features → IR ops in the IR contract.
3. Mark **partial** only with an explicit subset description.
4. Removing EXAMPLE rows is required before calling this document anything
   other than a stub.

## See also

- [open-questions.md](open-questions.md) — AQ-08
- [glossary.md](glossary.md) — Oracle, BN IR, LLVM toolchain external
