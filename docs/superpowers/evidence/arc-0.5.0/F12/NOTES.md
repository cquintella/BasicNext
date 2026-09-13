# F12 — No DELETE in fixture programs + scope lifetime

**Status:** `RED_EXPECTED` — fixture committed before M2/D1 runtime (Carlos/Quorra fixtures-first).  
**Backend:** interpret (`bn run`)

Split checks (do **not** combine with a broken `rg && bn run` pipeline).

### (a) No `DELETE` keyword in fixture `program.bn` files **and** 0.5.0 grammar

`! rg` is **forbidden** as a gate: inverting exit codes also succeeds when the
file is missing or `rg` errors. Accept **only** `rg` exit code **1** (no
matches). Exit **0** (matches found) and exit **≥2** (error / missing path)
are **failures**.

```bash
# (a1) every arc-0.5.0 fixture program — expect exit 1 (no matches)
rg -n '\\bDELETE\\b' docs/superpowers/evidence/arc-0.5.0/*/program.bn
test $? -eq 1

# (a2) 0.5.0 EBNF reserved/productions — expect exit 1 (keyword absent;
# comments that spell DELETE in prose are outside reserved-word / productions;
# prefer scanning reserved-word and statement productions, or require zero
# DELETE as a quoted terminal)
rg -n '"DELETE"' docs/0.5.0/0.5.0.ebnf
test $? -eq 1
```

Scope of (a1): **all** `docs/superpowers/evidence/arc-0.5.0/*/program.bn`, not
only F12. Checking a single file does **not** prove the suite or grammar.

### (b) Scope / lifetime run (this fixture)

```bash
bn run docs/superpowers/evidence/arc-0.5.0/F12/program.bn
```

**Expected exit (when M2 green):** `0`  
**Expected observation:** program prints `LIVE` then `DEINIT` (runtime lifetime).

### (c) NOTES / quarantine Markdown mentions OK

`NOTES.md`, README, and quarantine docs **may** mention the word DELETE when
documenting the forbidden surface. Those mentions are **not** a fixture
failure. Do not treat Markdown prose as program tokens.

**Today (pre-M2):** may fail parse/typecheck/runtime on 0.4 toolchain — intentional red until waves land.  
**Forbidden in `program.bn`:** `DELETE`, force-dispose, silent use-after-release.
