# Memory model migration: 0.4.x → 0.5.0

**Authority for 0.5.0 lifetime:** [`language-0.5.0.md`](language-0.5.0.md)  
(ARC, `RELEASE`, weak, no `DELETE`). Grammar: [`0.5.0.ebnf`](0.5.0.ebnf).

## Book chapter 7 (0.4 / 0.3 text) is superseded

[`docs/book/en/07_memory_management.md`](../book/en/07_memory_management.md)
still documents **manual** `NEW` / `DELETE` (0.3-era teaching). For Basic Next
**0.5.0**, that chapter is **superseded** by the ARC contract in
`language-0.5.0.md`. Do not treat book ch.7 as normative for 0.5.0 lifetime.

A short overlay note also sits at the top of that book file pointing here.

## What changed

| 0.4.x | 0.5.0 |
| --- | --- |
| Manual `DELETE` as destruction point | **`DELETE` removed** from language DNA |
| Aliases without language-visible retain | **Strong by default**; toolchain retain/release |
| No weak surface | **`AS WEAK ClassName`**; dead weak → **`NULL`** |
| — | **`RELEASE`** optional advanced (early end of binding) |
| HOST sometimes taught with `DELETE` | HOST / tickets: **`Close` / `*_close` only** |

## Migration recipes

1. **`DELETE x` on a class** → delete the statement; let scope / reassignment
   release the last strong. Use `RELEASE x` only for intentional early drop of
   that binding.
2. **HOST / ticket teardown** → `obj.Close()` (or `*_close`); never revive
   `DELETE`.
3. **Ticket vectors** → `FOR` + `tickets[i].Close()`, then optional
   `RELEASE tickets` (aggregate only) or just leave scope.
4. **Cycles** → make at least one edge `AS WEAK T OR NULL`; after collection,
   check `IS NULL`.
5. **Diagnostics** named `USE_AFTER_DELETE` / `DOUBLE_DELETE` → rename in FE
   work to use-after-release / invalid binding (implementation wave).

## Related

- Conformance: [`arc-conformance.md`](arc-conformance.md)
- Plan locks: [`todo/proposals/bucket-0.5.0-corrective.md`](../../todo/proposals/bucket-0.5.0-corrective.md)
