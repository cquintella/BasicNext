# Proposal S1 — `++` / `--` statements (0.5.2)

**Status:** proposed 2026-09-17 (Carlos: "coloca no bucket 0.5.2"). Syntax lock pending (§2.6 of `ongoing/bucket-0.5.2.md`).

## Today

`language/0.5/0.5.ebnf:132` — `assignment-operator = "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "**=" ;`
No `++`/`--` anywhere; `bn check` rejects `i++` with `E0100` (toolchain agrees with the spec).
Idiom: `i += 1` / `i -= 1`.

## Proposal (default candidate — statement only)

```
increment-statement = assignment-target ( "++" | "--" ) ;
```

- A **statement**, never an expression: `i++` has no value, so there is no
  pre/post-increment debate and no `x = i++` (minimalist posture; AGENTS.md
  § Approved direction 4).
- Postfix only (`i++`); no prefix form.
- Exact sugar for `target += 1` / `target -= 1`: same target grammar
  (`identifier { member-suffix | index-suffix }`, `SELF.…`), same typing
  (integer widths; `FLOAT` allowed iff `+= 1` is), same overflow rule
  (checked, `NUMERIC_OVERFLOW`), same ARC/ownership behaviour (none: scalars).
- Lowering: desugars in the frontend to the existing compound assignment —
  **no IR change**, hence native parity for free (both backends see `+= 1`).
- Lexer: new tokens `++` and `--`. BN comments are `//` and `/* */`
  (`0.5.ebnf:238`), so `--` does not collide with a comment; the only lexing
  question is `a - -b` (two `-`) vs `a--b`: tokens are produced when the two
  characters are adjacent, and the parser accepts `--` only right after an
  assignment target at statement start, so `a--b` is a syntax error rather
  than `a - -b`. Lock this in §2.6.
- Diagnostics: `i++` on a non-numeric target reuses the `+=` diagnostic;
  `i++` used as an expression (`PRINT i++`) → syntax error naming the rule.

## Not proposed

Prefix `++i`; `++` as expression; `+=`-style operators for other ops (`**++`).

## Fixtures (S*)

- S1 `i++` / `i--` on INTEGER local, field (`SELF.count++`), indexed (`a[2]++`).
- S2 overflow at `INTEGER` max → `NUMERIC_OVERFLOW`, same as `+= 1`.
- S3 negative: `PRINT i++` and `x = i++` → syntax error.
- S4 lexing: `a - -b`, `a--b` (error, not `a -- b`), comment vs `--` if applicable.
- S5 native parity: S1/S2 identical under `bn build`.
