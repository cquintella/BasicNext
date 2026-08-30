# Modules

Place project-owned Basic Next modules under this directory. Import paths are
logical names, not filesystem paths.

`bn/` contains all Basic Next standard-library source modules (`BNMath`,
`BNData`, and future `BN*` modules). Every such module requires `IMPORT`.
Project code must not be placed there.
