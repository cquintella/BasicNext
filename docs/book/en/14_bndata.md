# Appendix H: BNData

`BNData` is the standard data-provider module for tabular datasets and CSV operations in Basic Next. It is not built into the core language; use an explicit import:

```basic
IMPORT BNData AS Data
IMPORT HOST.FileSystem AS FS
```

While `HOST.FileSystem` handles raw text and binary byte streams, `BNData` provides structured columnar data manipulation centered around the `Data.DataFrame` class.

## CSV Reading and Writing

`BNData` consumes `HOST.FileSystem` open file handles for CSV parsing and serialization:

```basic
LET file AS FS.File OR Error = FS.Open("sales.csv", FS.READ)
IF file IS Error THEN
    RETURN
END IF

// Read CSV with headers and semicolon separator
LET table AS Data.DataFrame OR Error = Data.ReadCSV(file, TRUE, ";")
file.Close()
DELETE file

IF table IS Error THEN
    PRINT "CSV Error: " + table.Message
    RETURN
END IF
```

Writing a DataFrame back to disk:

```basic
LET outFile AS FS.File OR Error = FS.Open("output.csv", FS.WRITE)
IF outFile IS Error THEN
    RETURN
END IF

LET status AS VOID OR Error = Data.WriteCSV(outFile, table, TRUE, ",")
outFile.Close()
DELETE outFile
```

## The `Data.DataFrame` Class

`Data.DataFrame` represents an in-memory tabular structure where columns share equal row counts.

### Creating and Populating Columns

DataFrames can be constructed programmatically using fixed-size vectors:

```basic
LET df AS Data.DataFrame = NEW Data.DataFrame()

LET names AS STRING[3] = ["Alice", "Bob", "Charlie"]
LET ages AS INTEGER[3] = [25, 30, 35]

df.AddStringColumn("Name", names)
df.AddIntegerColumn("Age", ages)

PRINT "Rows:", df.RowCount()
PRINT "Cols:", df.ColumnCount()

DELETE df
```

### Statistics and Conversions

Values read via `ReadCSV` are stored as `STRING`. Numeric statistics require explicit conversion:

```basic
LET conv AS VOID OR Error = table.ConvertToFloat("Price")
IF NOT (conv IS Error) THEN
    LET avgPrice AS FLOAT OR Error = table.Mean("Price")
    PRINT "Average Price:", avgPrice
END IF
```

Every `Data.DataFrame` instance must be released with `DELETE df` when no longer needed.
