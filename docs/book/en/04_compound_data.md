# Compound Data and Error Handling

Basic Next provides mechanisms to group data together and explicitly manage missing or erroneous values. It heavily favors explicit structures over hidden state or implicit nullability.

## Fixed-Size Vectors

A fixed-size vector stores multiple values of the same type sequentially. Vectors are defined by an element type followed by one or more dimensions inside brackets.

```basic
LET dimensions AS INTEGER[3] = [10, 20, 30]
LET grid AS INTEGER[2][3] = [[1, 2, 3], [4, 5, 6]]
```

For function signatures, fields, and parameters, each dimension must be a non-negative integer literal. However, in version 0.3, local `LET` bindings can use declaration-time vector dimensions that are evaluated once at binding time:

```basic
LET n AS INTEGER = 4
LET values AS INTEGER[n] = [1, 2, 3, 4]
```

Indices are zero-based, running from `0` to one less than the dimension. Accessing an index outside this range raises an `INDEX_OUT_OF_BOUNDS` runtime error.

If you declare a vector without an initializer, each element is automatically assigned its type's default value:

```basic
LET values AS INTEGER[5]  // A vector of five 0s
values[2] = 42
```

In Basic Next, vectors have value semantics for assignments and parameter passing. Assigning a vector to a new variable copies all of its elements. The new vector does not alias the old one; modifying one will not affect the other.

## Value Types (`STRUCT`)

A `STRUCT` declares a small, composite value type with named fields. Structs cannot contain methods, static fields, constructors, or visibility modifiers (all fields are implicitly public).

```basic
STRUCT Point
    X AS FLOAT = 0.0
    Y AS FLOAT = 0.0
END STRUCT

LET origin AS Point
LET moved AS Point = origin
moved.X = 10.0
```

Like vectors, structs have value semantics. Assignment or parameter passing creates a complete copy of the field values. Changing `moved.X` in the example above does not affect `origin.X`. The `NEW` and `DELETE` keywords are not used with structs.

## String Indexing

Strings in Basic Next are UTF-8 encoded, and their contents can be read using zero-based indexing to access individual Unicode scalars (characters).

```basic
LET greeting AS STRING = "Hello"
LET firstChar AS STRING = greeting[0] // "H"
```

String indexing is read-only. Attempting to assign to a string index (e.g., `greeting[0] = "J"`) results in a static error. Accessing an index outside the string's length raises an `INDEX_OUT_OF_BOUNDS` runtime error.

## Alternative Types and Absence

Basic Next does not allow variables to be implicitly null or missing. If a variable might contain an absence marker or an error, you must explicitly declare it using an alternative type with the `OR` keyword.

Basic Next provides specific singleton values to represent the absence of data:
- `EOF`: End of input.
- `NA`: A missing observation or data point.
- `NULL`: The explicit absence of an object reference or pointer.

```basic
LET line AS STRING OR EOF = INPUT()
```

You cannot use a variable with an alternative type directly if the operation requires a concrete type. You must first test and narrow the type using the `IS` operator.

### Narrowing with `IS`

The `IS` operator tests if a value matches a specific alternative or singleton. When used inside an `IF` condition, the compiler automatically narrows the type for the corresponding branch.

```basic
IF line IS EOF THEN
    PRINT "Stream ended."
ELSE
    PRINT line
END IF
```

In the `ELSE` branch above, the compiler knows `line` cannot be `EOF`, so its type is narrowed to `STRING`, making it safe to use in `PRINT`. 

In version 0.3, if an `IF` statement has no `ELSE` branch and all of its branches unconditionally terminate (e.g., via `RETURN` or `STOP`), the type is automatically narrowed in the remainder of the block:

```basic
LET file AS FS.File OR Error = FS.Open("config.txt", FS.READ)
IF file IS Error THEN
    RETURN
END IF
// After the returning branch, 'file' is safely narrowed to FS.File
LET closed AS VOID OR Error = file.Close()
```

You can also test for `NULL` or `NA` using `IS NULL` or `IS NA`.

## Error Handling

Basic Next does not use exceptions for error handling. Fallible operations explicitly return their success value or a built-in `Error` object. 

The `Error` object provides standard properties: `Code AS INTEGER` and `Message AS STRING`. Functions that might fail indicate this by returning their standard type `OR Error`.

```basic
IMPORT HOST.FileSystem AS FS

LET file AS FS.File OR Error = FS.Open("config.txt", FS.READ)

IF file IS Error THEN
    PRINT "Failed to open file: " + file.Message
ELSE
    PRINT "File opened successfully."
    file.Close()
END IF
```

By returning `Error` as an alternative type, Basic Next forces the caller to explicitly check for and handle the error before accessing the underlying value, eliminating unhandled exceptions at runtime.
