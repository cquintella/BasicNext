# Basic Next 0.1 Console contract

`PRINT` and `INPUT()` use the default portable console capability.

`PRINT` writes the text representations of zero or more expressions, without a
separator, followed by one line ending. `PRINT` with no expression writes a
blank line. `INPUT()` takes no arguments, returns one `STRING` without its line
ending, and returns `EOF` after standard input ends; later calls also return
`EOF`.

`PRINT` renders booleans and special values as `TRUE`, `FALSE`, `NAN`, `INF`,
`-INF`, `NULL`, `NA`, and `EOF`. Strings have no quotes. Finite floats use the
shortest portable decimal representation that round-trips to their BN type.
