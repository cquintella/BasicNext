# Modules

Place project-owned Basic Next modules under this directory. Import paths are
logical names, not filesystem paths.

`bn/` contains all Basic Next standard-library source modules (`BNMath`,
`BNData`, and future `BN*` modules). Every such module requires `IMPORT`.
Project code must not be placed there.

## BNString

`bn/BNString.bn` is an official extras module: `CLASS String` + `CLASS Tokenizer`
over the primary `STRING` (Carlos 2026-09-12). Import as `IMPORT BNString AS S`.
See `docs/library/bnstring.md`.

## Precompiled module objects

There is **no** `.bno` / compile-cache artifact format in the toolchain today.
**Intent for 0.5.x:** ship precompiled objects for `modules/bn/*` when the
pipeline exists. Until then, standard-library modules are **source `.bn` only**.
Do not invent an ad-hoc binary format in-tree.

