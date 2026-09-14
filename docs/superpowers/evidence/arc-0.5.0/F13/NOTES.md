# F13 — Typed AWAIT + args

**Status:** `GREEN` D1 interpret + `GREEN` D2 native (Tron 2026-09-13).
**Backend:** interpret (`bn run`) and LLVM/native (`bn build`)

**Commands:**
```bash
bn run docs/superpowers/evidence/arc-0.5.0/F13/program.bn
bn build docs/superpowers/evidence/arc-0.5.0/F13/program.bn -o /tmp/F13 && /tmp/F13
```

**Expected exit:** `0` (both)
**Observed (interpret):** exit `0`; `PASS F13 sum= 10` (1+2+3+4); AWAIT yields INTEGER OR Error
**Observed (native):** exit `0`; `PASS F13 sum= 10`

Runtime tests also cover FLOAT, STRING, BOOLEAN, VOID tasks, replay until Close,
argument checking, and result type mismatch / worker-returned Error.

**Forbidden:** `DELETE`, force-dispose, silent use-after-release.
