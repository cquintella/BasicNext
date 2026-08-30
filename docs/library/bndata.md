# BNData standard library 0.2

## Status

Accepted 0.2 standard-library contract. Nothing here is 0.1. The
implementation sprint follows `HOST.FileSystem`.

## Access

`BNData` is not a prelude. Every use requires an import:

```basic
IMPORT BNData AS Data
IMPORT HOST.FileSystem AS FS
```

Its logical name is `BNData`. The standard-library source location is
`modules/bn/BNData.bn`; programs continue to use `IMPORT BNData AS Data`,
not a filesystem path. The namespace is not a language keyword and adds no
grammar production.

Variable-size `TYPE[]` is not part of 0.2. `Data.DataFrame` is a class that
owns its columns. It does not return borrowed pointers, and it does not
spell `INTEGER[]`.

## CSV

UTF-8 text. `separator` is a `STRING` of `LEN` 1; it must not be `"` or a
line ending. Fields may be quoted with `"`; an embedded quote is `""`.
`ReadCSV` yields **string columns only**. Numeric conversion is
`ConvertToInteger` / `ConvertToFloat` on that frame.

```basic
LET file AS FS.File OR Error = FS.Open("vendas.csv", FS.READ)
IF file IS Error THEN
    RETURN
END IF
LET tabela AS Data.DataFrame OR Error = Data.ReadCSV(file, TRUE, ";")
LET closed AS VOID OR Error = file.Close()
DELETE file
```

| Function | Meaning |
| --- | --- |
| `Data.ReadCSV(file AS FS.File, hasHeader AS BOOLEAN, separator AS STRING) AS Data.DataFrame OR Error` | Read remaining text of `file` as rows. `hasHeader TRUE` takes the first row as column names. |
| `Data.WriteCSV(file AS FS.File, table AS Data.DataFrame, writeHeader AS BOOLEAN, separator AS STRING) AS VOID OR Error` | Write `table`. |

`ReadCSV` stores every field as `STRING`. Empty fields are the empty
string, not `NA`. A ragged row (wrong field count) or an unterminated
quoted field returns `Error`. `WriteCSV` writes the whole table in one host write and returns `Error` if
that write fails; it does not report success after a partial write.
Statistics methods require an integer or float column; they return `Error`
on a string column. After `ReadCSV`, convert:

```basic
LET conv AS VOID OR Error = tabela.ConvertToFloat("preco")
LET media AS FLOAT OR Error = tabela.Mean("preco")
```

## DataFrame

```basic
LET df AS Data.DataFrame = NEW Data.DataFrame()
LET nomes AS STRING[3] = ["Ana", "Carlos", "João"]
LET idades AS INTEGER[3] = [28, 35, 42]
df.AddStringColumn("Nome", nomes)
df.AddIntegerColumn("Idade", idades)
```

`NEW Data.DataFrame()` creates an empty frame. Every instance, including
those returned by `ReadCSV`, `Select`, and `Slice`, must be released with
`DELETE`. A user class must not `EXTENDS Data.DataFrame`.

`Add*Column` appends a fixed-size vector. The parameter accepts every
declared length of that element type (`INTEGER[1]`, `INTEGER[2]`, …). That
is the 0.2 library vector-parameter rule, not a user type `INTEGER[n]`.
Every column must have the same length; a mismatch returns `Error`. The
first column sets `RowCount`. Duplicate column names return `Error`,
including duplicate CSV headers and a `Select` that repeats a column
index.

| Method | Meaning |
| --- | --- |
| `AddIntegerColumn(name AS STRING, values)` | `values` is any fixed-size `INTEGER` vector. Append an integer column. |
| `AddFloatColumn(name AS STRING, values)` | Any fixed-size `FLOAT` vector. |
| `AddStringColumn(name AS STRING, values)` | Any fixed-size `STRING` vector. |
| `AddBooleanColumn(name AS STRING, values)` | Any fixed-size `BOOLEAN` vector. |
| `RowCount() AS INTEGER` | Number of rows. |
| `ColumnCount() AS INTEGER` | Number of columns. |
| `ColumnName(index AS INTEGER) AS STRING OR Error` | Name at a 0-based column index. |
| `SetLabel(oldLabel AS STRING, newLabel AS STRING) AS VOID OR Error` | Rename one column in place. |
| `Transpose() AS DataFrame OR Error` | Return a string-valued transposition. |
| `AppendRows(other AS DataFrame) AS DataFrame OR Error` | Return a new frame with the rows of both frames; labels, order, and non-empty column types must match. |
| `AppendColumns(other AS DataFrame) AS DataFrame OR Error` | Return a new frame with the columns of both frames; row counts must match and labels must remain unique. |
| `Join(other AS DataFrame, leftKey AS STRING, rightKey AS STRING) AS DataFrame OR Error` | Inner join. |
| `LeftJoin(other AS DataFrame, leftKey AS STRING, rightKey AS STRING) AS DataFrame OR Error` | Keep every row of the receiver. |
| `RightJoin(other AS DataFrame, leftKey AS STRING, rightKey AS STRING) AS DataFrame OR Error` | Keep every row of `other`. |
| `FullJoin(other AS DataFrame, leftKey AS STRING, rightKey AS STRING) AS DataFrame OR Error` | Keep all rows of both frames. |

Joins use exact column labels as keys and return a new frame. The receiver's
columns come first; the right key is omitted and its unmatched values are
placed in the receiver key column. Key cells equal to `NA` do not match. A
duplicated non-key label returns `Error`; use `SetLabel` before the join.
Unmatched non-key cells are `NA`.

Cell accessors use 0-based `row` and a column name. A missing name or row
out of range returns `Error`. A type mismatch (asking `GetInteger` of a
string column) returns `Error`. Empty / missing numeric cells are `NA`.

| Method | Meaning |
| --- | --- |
| `GetInteger(row AS INTEGER, name AS STRING) AS INTEGER OR NA OR Error` | Integer cell. |
| `GetFloat(row AS INTEGER, name AS STRING) AS FLOAT OR NA OR Error` | Float cell. |
| `GetString(row AS INTEGER, name AS STRING) AS STRING OR NA OR Error` | String cell. Empty string is a value, not `NA`. A missing cell (`NA` from a join) stays `NA`. |
| `GetBoolean(row AS INTEGER, name AS STRING) AS BOOLEAN OR NA OR Error` | Boolean cell. |

There is no `df[i, j]` syntax.

## Column conversion

These methods replace an existing **string** column in place. They do not
add a column and they do not change `RowCount`. After a successful
conversion, `GetInteger` / `GetFloat` and the statistics methods apply.

Conversion uses `BNMath.VAL` per cell, then `AS INTEGER` or assignment to
`FLOAT` as in 0.1 (range-checked). Leading and trailing spaces are handled
as `VAL` specifies (leading spaces skipped; trailing non-numeric text
ignored). A cell whose `LEN` is 0 after skipping leading spaces becomes
`NA`, not `0`.

| Method | Meaning |
| --- | --- |
| `ConvertToInteger(name AS STRING) AS VOID OR Error` | Replace string column `name` with integer cells (`INTEGER OR NA`). |
| `ConvertToFloat(name AS STRING) AS VOID OR Error` | Replace string column `name` with float cells (`FLOAT OR NA`). |

A missing name, or a column that is not `STRING`, returns `Error`.
`AS INTEGER` overflow on a cell returns `Error` and leaves the frame
unchanged (no partial conversion).

## Statistics on a column

These instance methods use the same algorithms and `NAN` / `NA` rules as
`BNMath` descriptive statistics. The named column must be integer or float;
otherwise `Error`.

| Method | Meaning |
| --- | --- |
| `Mean(name AS STRING) AS FLOAT OR Error` | Arithmetic mean. |
| `Median(name AS STRING) AS FLOAT OR Error` | Tukey / `BNMath.MEDIAN`. |
| `Quartile1(name AS STRING) AS FLOAT OR Error` | `BNMath.QUARTILE1`. |
| `Quartile3(name AS STRING) AS FLOAT OR Error` | `BNMath.QUARTILE3`. |
| `Mode(name AS STRING) AS FLOAT OR NA OR Error` | `BNMath.MODE`. |
| `Stdev(name AS STRING) AS FLOAT OR Error` | Sample `n−1`. |
| `Variance(name AS STRING) AS FLOAT OR Error` | Sample `n−1`. |
| `Range(name AS STRING) AS FLOAT OR Error` | `BNMath.RANGE`. |
| `Min(name AS STRING) AS FLOAT OR Error` | Minimum as `FLOAT`. |
| `Max(name AS STRING) AS FLOAT OR Error` | Maximum as `FLOAT`. |
| `ZScore(name AS STRING) AS Data.DataFrame OR Error` | New one-column `FLOAT` frame: `(x − mean) / Stdev` using sample `n−1`. `NA` cells stay `NA`. `NAN` in the column, `n<2`, or zero deviation yield `NAN` z-scores. Missing or non-numeric column → `Error`. |

## Copy-out interop with `BNMath`

A caller that already allocated a region may copy a column into it. The
caller owns the region and `DELETE`s it. `LEN` of `dest` must equal
`RowCount()`; otherwise `Error`.

| Method | Meaning |
| --- | --- |
| `CopyIntegerColumn(name AS STRING, dest AS POINTER TO INTEGER[]) AS VOID OR Error` | Copy; `NA` cells are not valid for this copy and return `Error`. |
| `CopyFloatColumn(name AS STRING, dest AS POINTER TO FLOAT[]) AS VOID OR Error` | Copy; `NA` is not valid and returns `Error`. |
| `CopyStringColumn(name AS STRING, dest AS POINTER TO VOID)` | Not in 0.2. |

After a successful integer or float copy, `BNMath.MEAN(dest)` and the other
vector functions apply.

## Subsets

Both methods return a **new** `DataFrame`. They do not mutate the receiver.
There is no operator overloading.

| Method | Meaning |
| --- | --- |
| `Select(rows, columns) AS Data.DataFrame OR Error` | Discrete 0-based row and column indices. `rows` and `columns` are any fixed-size `INTEGER` vectors. |
| `Slice(startRow AS INTEGER, rowCount AS INTEGER, startCol AS INTEGER, colCount AS INTEGER) AS Data.DataFrame OR Error` | Contiguous block. |

Out-of-range indices return `Error`.
