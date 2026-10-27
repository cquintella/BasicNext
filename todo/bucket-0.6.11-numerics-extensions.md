# Bucket 0.6.11 — numeric extensions deferred from 0.6.10

**Status:** todo, created 2026-10-27 (Carlos: "coloca isso num bucket 0.6.11"). Starts after
`ongoing/bucket-0.6.10-numerics-na-for-scope-bni-perf.md` closes.
**Source:** `todo/proposals/bnmath-bndata-statistics-0.6.10.md` (sections 3–6) and
`todo/proposals/bndata-expansion.md`; decision G-5 of bucket 0.6.10 moved these rows here.

Every row is an extension: spec text, red tests on `bni` and `bnc`, then one implementation in
`bn_core_math` / `bn_stdlib` adapted by both backends. Each waits for the decision named with it.
New members are PascalCase (object style, BDFL 2026-10-26).

## Deferred by Carlos (the list he moved, 2026-10-27)

- [ ] **Factorial, Binomial.** Returning `FLOAT` for an integer result is a design choice that
  needs its own discussion (the proposal follows R: exact to `22!`, then rounded; `n > 170` is
  `INF`). Decide the result type first.
- [ ] **Sinh, Cosh, Tanh, Log1p, Expm1.** They come from each platform's libm, so `bni` and `bnc`
  can differ between macOS, Linux and Windows (question Q6). Decide: accept the caveat, as for
  `SIN`, or ship correctly rounded versions in `bn_core_math`.
- [ ] **Spearman, WeightedMean.** Competing definitions (tie handling) need a choice; `WeightedMean`
  with a negative weight is question Q4. (`Skewness`, `Kurtosis` and their population forms shipped
  in 0.6.10, Carlos 2026-10-27.)
- [ ] **HOST.Random.Normal.** New `HOST` surface; depends on Q5 (the error code for `sigma < 0`)
  and Q6 (`LOG` from libm).
- [ ] **CumSum, NormalVector.** Depend on Q3: the language cannot yet return a vector whose
  length is computed from the argument. Options recorded in bucket 0.6.10: methods on the E.5
  `Series`, a same-length vector return (a language extension), or an output vector.
- [ ] **E.5 (`bndata-expansion.md`).** A separate proposal, not yet approved. It must be updated
  first: it uses `DELETE`, which the language does not have (ARC and `RELEASE`), and its
  `ReadCSVPath`/`WriteCSVPath` must go through `HOST.FileSystem` and its policy.

## Inference (Carlos, 2026-10-27: "t-student depois, já p-valor para 0.6.11")

- [ ] **p-value: the Student t distribution.** `StudentT(t, df)` (the CDF, R `pt`) and its two-sided
  p-value, through the regularized incomplete beta `I(df / (df + t²); df/2, 1/2)` with `lgamma`. Open:
  libm `exp`/`log` (the Q6 caveat: the last digit may differ between platforms) or an own
  implementation in `bn_core_math` from `+ − × ÷ √` (the same bits everywhere; recommended), and the
  quantile `StudentTQuantile` (R `qt`). Validate against R 4.5.1 within a few units.
- [ ] **t-test (later than the p-value).** One sample, Welch, pooled and paired; the statistic and
  degrees of freedom from the existing means and variances; the result form (an Excel-style `TTest`
  returning the p-value, or a `STRUCT` with statistic, degrees of freedom and p-value) is open.

## Also left out of 0.6.10 by G-5

- [ ] **Sum, Product, Dot.** Open: the overflow rule (recommendation `±INF`, as `MEAN`) and the
  result type for integer vectors (`FLOAT`, or exact integer with `NUMERIC_OVERFLOW`).
- [ ] **Fmod.** Open: the sign of the result (sign of `a`, as C `fmod`, or Euclidean, as `%`).
- [ ] **Q-NAMES.** The legacy upper-case `BNMath` names (`SIN`, `SQRT`, `MEAN`, …): PascalCase
  aliases, with or without a deprecation warning, or a rename.
- [ ] **Q-IS.** `x IS INTEGER = TRUE` is accepted though the grammar has no `=` after `IS type`;
  fix the parser to the grammar or keep it.

## Closure

- [ ] Each shipped row: spec (`language/0.6/math.md`, `bndata.md`, `host.md`), `modules/bn/*.bn`,
  tests on both backends, book (`docs/book/en/`), release notes `docs/releases/0.6.11.md`.
- [ ] Full battery, `--gates`, the Windows batch; `git mv` to `done/`.
