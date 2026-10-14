# Proposal: BNData Module Expansion (Series, Dynamic Vectors, and Analytical Operations)

**Author:** Carlos Quintella
**Status:** Proposed
**Target Version:** 0.4.x / 0.5
**Module:** `BNData` (`IMPORT BNData AS Data`)

## Motivation

Basic Next is designed with an explicit analytical DNA, combining the readability of classic BASIC and Pascal with the vectorized strengths of R. Currently, the language supports fixed-size vectors (`T[N]`) and a tabular `Data.DataFrame`. However, general-purpose programming and data analysis frequently require:

- Resizable, dynamic collections without forcing beginners to manipulate raw pointers (`POINTER TO T`) with `NEW` and `DELETE`.
- First-class 1D columnar series (`Series`) that support element-wise operations, boolean filtering, and missing-value (`NA`) handling.
- Richer tabular operations on `DataFrame`, including column extraction as series, grouped aggregations, and sorting.
- Convenient file-based data ingestion.

This proposal expands `BNData` strictly within the object-oriented and module boundary of Basic Next. It introduces **no new keywords**, requires **no compiler grammar changes**, and maintains the **KISS** philosophy.

---

## 1. Dynamic 1D Collections: The `Series` Family

Rather than adding a complex, monomorphized generic system (`<T>`) to the compiler core, `BNData` provides dedicated, contiguous dynamic series for fundamental primitive types:

- `Data.IntSeries` (wraps `INT32` / `INTEGER`)
- `Data.FloatSeries` (wraps `FLOAT64` / `FLOAT`)
- `Data.StringSeries` (wraps `STRING`)
- `Data.BoolSeries` (wraps `BOOLEAN`)

### Dynamic Resizing and Indexing

Each `Series` class encapsulates a heap-allocated buffer, managing geometric growth (doubling capacity on saturation) and safe disposal via its destructor.

```basic
IMPORT BNData AS Data

// Initialize with default or explicit initial capacity
LET s AS Data.IntSeries = NEW Data.IntSeries(16)

// Dynamic appending
s.Append(10)
s.Append(25)
s.Append(42)

PRINT "Count:", s.Len()
PRINT "Element at 1:", s.Get(1)

// In-place update
s.Set(1, 99)

// Release resources
DELETE s
```

### Conversion and Fixed Vector Integration

Series seamlessly bridge fixed-size language vectors and dynamic storage:

```basic
LET raw AS INTEGER[3] = [1, 2, 3]
s.Extend(raw)
```

---

## 2. Missing Value (`NA`) Semantics

Consistent with Basic Next’s first-class absence primitives, `Series` tracks missing values without sentinel magic numbers:

- `s.AppendNA()`: Appends an `NA` marker to the series.
- `s.IsNA(index AS INTEGER) AS BOOLEAN`: Checks if an entry is absent.
- `s.DropNA() AS Data.IntSeries`: Returns a new series omitting missing entries.
- `s.FillNA(replacement AS INTEGER) AS Data.IntSeries`: Returns a new series replacing `NA` values.

---

## 3. Vectorized Operations and Masked Filtering

`Series` implements vectorized operations and descriptive statistics directly:

### Element-Wise Transformations

```basic
LET a AS Data.FloatSeries = NEW Data.FloatSeries()
// ... populate a ...

// Scale all elements by a scalar factor
LET scaled AS Data.FloatSeries = a.MultiplyScalar(2.5)

// Vector addition
LET c AS Data.FloatSeries OR Error = a.Add(scaled)
```

### Statistical Aggregations

```basic
LET meanVal AS FLOAT OR Error = a.Mean()
LET stdVal AS FLOAT OR Error = a.StdDev()
LET medianVal AS FLOAT OR Error = a.Median()
LET minVal AS FLOAT OR Error = a.Min()
LET maxVal AS FLOAT OR Error = a.Max()
```

### Boolean Mask Filtering

Boolean series serve as selection masks, providing vectorized filtering without nested loops:

```basic
// Compares elements of 's' against threshold, returning a BoolSeries
LET mask AS Data.BoolSeries = s.GreaterThan(50)

// Filters elements where mask is TRUE
LET filtered AS Data.IntSeries = s.Filter(mask)
```

---

## 4. DataFrame Integration

The tabular `Data.DataFrame` expands to interoperability with `Series`:

### Column Extraction as Series

Existing 0.2 `DataFrame` methods copy data into fixed vectors. The expansion allows extracting columns directly as dynamic series:

```basic
LET ages AS Data.IntSeries OR Error = df.GetIntegerSeries("Age")
LET prices AS Data.FloatSeries OR Error = df.GetFloatSeries("Price")
```

### Appending Series as Columns

```basic
df.AddSeries("Score", scoreSeries)
```

### Sorting and Grouping

```basic
// Returns a new sorted DataFrame
LET sortedDf AS Data.DataFrame OR Error = df.SortBy("Age", TRUE)

// Grouped aggregations
LET grouped AS Data.GroupedFrame OR Error = df.GroupBy("Department")
LET summaryDf AS Data.DataFrame OR Error = grouped.Mean("Salary")
```

---

## 5. Convenient Tabular Ingestion

Alongside existing stream-based `Data.ReadCSV(file, hasHeader, sep)`, `BNData` introduces path-based helper functions to reduce boilerplate for common use cases:

```basic
// Automatically manages file handle opening, reading, and closing
LET df AS Data.DataFrame OR Error = Data.ReadCSVPath("data/sales.csv", TRUE, ",")
IF df IS Error THEN
    PRINT "Failed to load dataset."
    RETURN 1
END IF

// Safe batch export
LET status AS VOID OR Error = Data.WriteCSVPath("data/output.csv", df, TRUE, ",")
```

---

## 6. Architecture and Implementation Boundaries

- **No Compiler Extensions**: All classes and methods live within the library boundary (`modules/bn/BNData.bn` and internal runtime helpers).
- **Decoupling and ABI (AQ-05)**: Implementation relies on standard `bn_rt` allocators and runtime primitives, ensuring identical behavior across both the interpreter and the AOT LLVM compiler.
- **Memory Contiguity**: Internal buffers remain contiguously allocated arrays, maintaining predictable CPU cache behavior and enabling future SIMD lowering in the LLVM backend.
- **Safety and Disposal**: All heap-allocated series require deterministic disposal via `DELETE`, preserving the manual, explicit resource model of Basic Next.
