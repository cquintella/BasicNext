# Basic Next 0.5.0 language tree

This directory is the **0.5.0 language contract** for Quorra-gate review.
It is the Carlos path for 0.5.0 docs — **not** `docs/language/0.5/`.

| File | Role |
| --- | --- |
| [`0.5.0.ebnf`](0.5.0.ebnf) | Normative grammar (ISO 14977-style EBNF) |
| [`language-0.5.0.md`](language-0.5.0.md) | Normative semantics and amendments |
| [`keywords.md`](keywords.md) | Reserved-word registry |
| [`arc-conformance.md`](arc-conformance.md) | Normative ARC/lifetime fixture checklist |
| [`memory-migration.md`](memory-migration.md) | 0.4.x → 0.5.0 memory migration; book ch.7 superseded |


## Relationship to other trees

- **[`docs/language/0.4/`](../language/0.4/)** — accepted 0.4 baseline. 0.5.0
  incorporates 0.4 by reference except where this tree amends it (`DELETE`
  purge, `RELEASE`, ARC locks, typed `AWAIT`).
- **[`docs/language/0.5/`](../language/0.5/)** — empty placeholder under the
  older `docs/language/` layout. Do not put 0.5.0 normative text there;
  authority for this release train lives in **`docs/0.5.0/`**.

Plan locks that drove this draft:
[`todo/proposals/bucket-0.5.0-corrective.md`](../../todo/proposals/bucket-0.5.0-corrective.md).
