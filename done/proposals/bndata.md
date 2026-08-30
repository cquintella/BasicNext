# Proposal: BNData Module (CSV and DataFrame)

**Status:** Accepted into 0.2 with amendments. Historical proposal only.
Normative text is [`docs/library/bndata.md`](../../docs/library/bndata.md).
0.2 does not add `TYPE[]`; CSV columns are `STRING`; statistics live on the
`DataFrame` object. Column extraction is `GetInteger` / copy-out, not
`GetIntegerColumn`.

## Motivation
To support data manipulation and descriptive statistics (planned for 0.2) without compromising Basic Next's explicit typing and KISS philosophy. This proposal introduces the `BNData` module, focusing on a columnar `DataFrame` structure and safe CSV parsing.

## 1. Module and Capability
All structured data manipulation resides in the standard `BNData` module. It consumes `HOST.FileSystem` handles for input.

```basic
IMPORT BNData AS Data
IMPORT HOST.FileSystem AS FS
```

## 2. CSV Parsing
Reading tabular text data uses explicit factory functions. To prevent runtime ambiguity, parameters like header presence and separator must be declared explicitly.

```basic
LET file AS FS.File OR Error = FS.Open("vendas.csv", FS.READ)

// Data.ReadCSV(file handle, hasHeader, separator)
LET tabela AS Data.DataFrame OR Error = Data.ReadCSV(file, TRUE, ";")
```

## 3. The DataFrame Structure
Basic Next does not use dynamic object properties or generics (in 0.2). Therefore, `Data.DataFrame` acts as a columnar container. Under the hood, it is functionally similar to R (a collection of equal-length typed vectors), but accessed via strict, type-safe method contracts.

### Column Extraction
The draft below used `TYPE[]` getters. 0.2 withdrew that shape; see
`docs/library/bndata.md`.

```basic
// Extraction by column name
LET idades AS INTEGER[] OR Error = tabela.GetIntegerColumn("idade")

// Extraction by zero-based positional index
LET pesos AS FLOAT[] OR Error = tabela.GetFloatColumnByIndex(2)
```

By returning native vectors, DataFrames integrate seamlessly with the 0.2 `BNMath` descriptive statistics:
```basic
LET media AS FLOAT = BNMath.MEAN(idades)
```

### Manual Population
When creating DataFrames programmatically, the developer instantiates the object and appends native vectors.

```basic
LET df AS NEW Data.DataFrame()

LET nomes AS STRING[3] = ["Ana", "Carlos", "João"]
LET idades AS INTEGER[3] = [28, 35, 42]

df.AddStringColumn("Nome", nomes)
df.AddIntegerColumn("Idade", idades)
```

## 4. Subsets and Slicing
Because Basic Next rejects operator overloading (to maintain low cognitive load), selecting sub-sections of a DataFrame uses explicit instance methods. Both methods return a **new** isolated `DataFrame` instance.

### Select (Discrete Indices)
Extracts non-contiguous rows and columns by explicitly providing integer vectors.

```basic
LET linhas AS INTEGER[3] = [0, 5, 12]
LET colunas AS INTEGER[2] = [0, 2]

LET amostra AS Data.DataFrame OR Error = tabela.Select(linhas, colunas)
```

### Slice (Contiguous Ranges)
Extracts a continuous block of data, similar to pagination.

```basic
LET startRow AS INTEGER = 100
LET rowCount AS INTEGER = 50
LET startCol AS INTEGER = 0
LET colCount AS INTEGER = 3

// Generates a DataFrame with 50 rows and 3 columns
LET fatia AS Data.DataFrame OR Error = tabela.Slice(startRow, rowCount, startCol, colCount)
```

## 5. Alignment with Philosophy
- **Object-oriented by default**: DataFrames are classes that manage their own tabular state and bounds checking.
- **Low Cognitive Load**: No "magic" syntax (like `df[1, 2]`). Method names explicitly declare their behavior (`GetIntegerColumn`, `Slice`).
- **Explicit Contracts**: All operations that can fail (column not found, index out of bounds, type mismatch) use the `OR Error` return signature.
