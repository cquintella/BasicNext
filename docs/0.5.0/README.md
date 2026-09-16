# Basic Next 0.5.0 language tree (historical)

**Superseded 2026-09-16.** The authority for the 0.5.x line is now
[`docs/language/0.5/`](../language/0.5/README.md), which incorporates the
0.4 baseline and these 0.5.0 amendments inline and adds 0.5.1. This
directory is kept for the ARC conformance checklist and the memory-migration
bridge, which `docs/language/0.5/` links to; `0.5.0.ebnf`, `language-0.5.0.md`
and `keywords.md` here are historical drafts — do not edit them.

Original 2026-09-12 note follows.

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
- **[`docs/language/0.5/`](../language/0.5/)** — **current authority** for
  0.5.x (supersedes the earlier note that kept it empty).

Plan locks that drove this draft:
[`todo/proposals/bucket-0.5.0-corrective.md`](../../todo/proposals/bucket-0.5.0-corrective.md).
