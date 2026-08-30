# Basic Next Console contract

`PRINT` and `INPUT()` use the default portable console capability. They do not
require an import.

`HOST.Console` is the named console capability. New host capability names after
`HOST.` start with a capital letter. An optional `IMPORT HOST.Console AS CON`
alias has the same type as `HOST.Console`.

## 0.1 statements withdrawn in 0.2

`CLS(HOST.Console)` and `BEEP(HOST.Console)` are not 0.2 syntax. `CLS`,
`BEEP`, and `PRINTAT` are not reserved words in 0.2. The replacement is
methods on the console object.

## Stream macros (unchanged)

`PRINT` writes the text representations of zero or more expressions, without a
separator, followed by one line ending. `PRINT` with no expression writes a
blank line. `INPUT()` takes no arguments, returns one `STRING` without its line
ending, and returns `EOF` after standard input ends; later calls also return
`EOF`.

`PRINT` renders booleans and special values as `TRUE`, `FALSE`, `NAN`, `INF`,
`-INF`, `NULL`, `NA`, and `EOF`. Strings have no quotes. Finite floats use the
shortest portable decimal representation that round-trips to their BN type.

## Methods (0.2)

```basic
IMPORT HOST.Console AS CON

CON.Cls()
CON.Beep()
CON.PrintAt(column, row, text)
LET cols AS INTEGER = CON.NumCols()
LET rows AS INTEGER = CON.NumRows()
```

| Method | Meaning |
| --- | --- |
| `Cls() AS VOID` | Clear the display and home the cursor. ANSI erase + home. Available when piped. |
| `Beep() AS VOID` | ASCII BEL. Available when piped. |
| `PrintAt(column AS INTEGER, row AS INTEGER, text AS STRING) AS VOID` | Write `text` at 1-based `(column, row)`. No newline. Cursor ends after the last character. ANSI CUP. |
| `NumCols() AS INTEGER` | Current window width in terminal cells. Not cached at `Start`. TTY required. |
| `NumRows() AS INTEGER` | Current window height in terminal cells. Not cached at `Start`. TTY required. |

`PrintAt` does not concatenate like `PRINT`. A coordinate outside the current
window, or a string that would extend past `NumCols` on that row, raises
`INDEX_OUT_OF_BOUNDS`. No wrap, no clip. Extent is `LEN(text)` terminal
cells: one Unicode scalar per cell. Combining marks and East Asian wide
glyphs are outside 0.2 (they still occupy one cell in this check).

String and vector indices are 0-based. `PrintAt` cells are 1-based. `(1, 1)`
is the top-left cell.

If `PrintAt`, `NumCols`, or `NumRows` **runs** and stdout is not a TTY, that
call raises `HOST_CAPABILITY_UNAVAILABLE`. Piped `PRINT`, `INPUT()`, `Cls`,
and `Beep` still run.

The Jupyter 0.2 kernel is not a TTY: `PrintAt` / `NumCols` / `NumRows` are
unavailable there. Stream I/O and `Cls` / `Beep` ANSI sequences go to the
cell output; `INPUT()` uses Jupyter stdin.

Reference implementation: CSI CUP for `PrintAt`, ANSI erase+home for `Cls`,
`ioctl` or Win32 console buffer size for `NumCols`/`NumRows`. No ncurses
crate and no extra dependency.
