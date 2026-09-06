# Common Programming Concepts

This chapter covers the fundamental building blocks of Basic Next: how to document code, store data in variables and constants, inspect types, and interact with the console.

## Comments

Comments help document the intent and operation of your code. Basic Next supports two comment styles:

- Single-line comments starting with `//`
- Multi-line comments enclosed within `/*` and `*/`

```basic
// This is a single-line comment

/*
This is a multi-line comment.
It spans several lines.
*/
```

Code tells the machine what to do, while comments tell other developers why it was done. Even when writing software for yourself, notes on decisions and edge cases save significant time later.

Helpful guidelines for writing comments:
- Explain the rationale (*why*), not just the syntax (*what*).
- Document public functions, classes, and exported module APIs.
- Keep comments updated whenever you modify the corresponding code.
- Avoid obvious comments that only repeat what the code already states.
- Use recognizable markers for work in progress: `TODO`, `FIXME`, or `NOTE`.

## Variables

Basic Next is an explicitly typed language. Every variable must have a declared type, and that type remains fixed throughout the variable's lifetime. An `INTEGER` variable, for example, can never hold a `FLOAT` value.

Variables are declared using the `LET` keyword, followed by the name, `AS`, the type, and an optional initial value:

```basic
LET counter AS INTEGER = 10
LET name AS STRING = "Alice"
LET first, second AS STRING = "auto", "bus"
```

If you do not provide an initial value, Basic Next initializes the variable to a safe default for its type:
- Numeric types default to `0` or `0.0`
- `BOOLEAN` defaults to `FALSE`
- `STRING` defaults to an empty string `""`

```basic
LET score AS INTEGER   // Initialized to 0
LET active AS BOOLEAN  // Initialized to FALSE
```

Omitting the type annotation in a `LET` binding causes a compile-time error:

```basic
LET message = "Hello"
// error[E0100]: a binding declaration requires AS TYPE
```

## Constants

Constants store fixed values that cannot be reassigned once defined. They are declared with the `CONST` keyword and always require an initial value:

```basic
CONST MAX_USERS AS INTEGER = 100
```

Basic Next can infer the type of a constant when the assigned value is a direct scalar literal:
- Whole numbers infer `INTEGER` (`INT32`)
- Decimal numbers infer `FLOAT` (`FLOAT64`)
- `TRUE` or `FALSE` infer `BOOLEAN`
- Quoted text infers `STRING`

```basic
CONST LIMIT = 100        // Inferred as INT32
CONST RATIO = 3.14159    // Inferred as FLOAT64
CONST ACTIVE = TRUE      // Inferred as BOOLEAN
CONST APP_NAME = "MyApp" // Inferred as STRING
```

When you need a specific integer width or signedness (such as `UINT32` or `INT64`), provide the type explicitly:

```basic
CONST BUFFER_SIZE AS UINT32 = 4096
```

Explicit type annotations are also required when initializing constants from expressions, vectors, or special values like `NULL` or `EOF`.

`CONST` prevents reassigning the variable name. It does not make referenced heap objects or pointers deeply immutable.

## Type Inspection with `TYPEOF`

Basic Next provides a built-in `TYPEOF(expression)` function to inspect the static type of any value or variable. It returns the canonical type name as a `STRING`.

Standard aliases are reported using their underlying representation:
- `INTEGER` is reported as `"INT32"`
- `FLOAT` is reported as `"FLOAT64"`
- Explicit types return their canonical name (`"BOOLEAN"`, `"STRING"`, `"UINT32"`, etc.)

```basic
CONST count = 10
CONST rate = 10.2
LET flag AS BOOLEAN = TRUE
LET message AS STRING = "Basic Next"

PRINT TYPEOF(count)    // Outputs: INT32
PRINT TYPEOF(rate)     // Outputs: FLOAT64
PRINT TYPEOF(flag)     // Outputs: BOOLEAN
PRINT TYPEOF(message)  // Outputs: STRING
```

## Primitive Types

Basic Next provides clear primitive types with guaranteed, cross-platform behavior.

### Numeric Types

The standard numeric types are `INTEGER` (a signed 32-bit integer, `INT32`) and `FLOAT` (an IEEE 754 64-bit floating-point number, `FLOAT64`).

Fixed-width types are available when exact memory layout is needed:
- **Signed integers:** `INT8`, `INT16`, `INT32`, `INT64`
- **Unsigned integers:** `BYTE` (unsigned 8-bit), `UINT16`, `UINT32`, `UINT64`
- **Floating-point:** `FLOAT32`, `FLOAT64`

Integer operations in Basic Next never silently wrap around or overflow. If a calculation exceeds the bounds of its type, the program halts with a `NUMERIC_OVERFLOW` error at runtime.

Floating-point numbers support standard IEEE 754 values, including `NAN` (Not a Number), `INF` (positive infinity), and `-INF` (negative infinity).

### Boolean and String Types

- `BOOLEAN` represents logical states using `TRUE` and `FALSE`.
- `STRING` represents UTF-8 encoded text enclosed in double quotes. Line breaks inside string literals are not permitted.

### Temporal Types

Basic Next includes built-in temporal primitives:
- `TIMESTAMP`: An alias for `INT64` representing milliseconds since the Unix epoch (UTC).
- `DATE`, `TIME`, and `TIMEZONE`: Dedicated value types for calendar dates and clock times. Their defaults are `1970-01-01`, `00:00:00.000`, and `UTC`.

## Operators and Expressions

Expressions in Basic Next are strictly typed. Mixing incompatible types without an explicit conversion is rejected by the compiler.

### Arithmetic Operators

The basic arithmetic operators are `+`, `-`, `*`, and `**` (exponentiation).

Division is strictly distinguished:
- `/` always performs floating-point division and returns a `FLOAT`, even with integer operands.
- `DIV` performs integer division (truncating towards zero).
- `%` calculates the integer modulo (remainder).

```basic
LET half AS FLOAT = 5 / 2            // 2.5
LET quotient AS INTEGER = 5 DIV 2    // 2
LET remainder AS INTEGER = 5 % 2     // 1
```

The `+` operator is also used to concatenate `STRING` values.

### Equality and Comparison

Comparisons using `=` (equal) and `<>` (not equal), as well as `<`, `<=`, `>`, and `>=`, require operands to share the same static type:

```basic
IF counter = 10 THEN
    PRINT "Limit reached"
END IF
```

### Logical and Bitwise Operators

The operators `AND`, `OR`, `NOT`, and `XOR` adjust their behavior depending on operand types:
- With `BOOLEAN` operands, they perform short-circuit logical operations.
- With integer operands, they perform bitwise operations.

Basic Next also provides `SHL` (shift left) and `SHR` (shift right) for integer types.

## Explicit Type Conversion

Because Basic Next does not perform implicit type coercion, use the `AS` operator when converting values between compatible types:

```basic
LET count AS INTEGER = 3
LET ratio AS FLOAT = count AS FLOAT
```

Converting a floating-point number to an integer truncates any fractional portion. If a value falls outside the target type's range, an `INVALID_NUMERIC_CONVERSION` error is raised.

When converting to `BOOLEAN`:
- Numeric `0` becomes `FALSE`, while any non-zero value becomes `TRUE`.
- An empty string `""` becomes `FALSE`, while non-empty strings become `TRUE`.

## Type Limits

To inspect the minimum and maximum boundaries of numeric types, import the standard `BNMath` module:

```basic
IMPORT BNMath AS Math

PRINT Math.MIN_INT32, Math.MAX_INT32
PRINT Math.MIN_FLOAT, Math.MAX_FLOAT
PRINT Math.MIN_INT64, Math.MAX_INT64
```

## Basic Console I/O

Console output and input are handled through built-in statements:

`PRINT` outputs text to the screen followed by a new line. Separating values with a comma (`,`) inserts a single space between them. Using `+` concatenates strings directly without spaces:

```basic
LET name AS STRING = "Alice"
PRINT "Processing user:", name       // Prints: Processing user: Alice
PRINT "Processing user: " + name     // Prints: Processing user: Alice
```

Calling `PRINT` without arguments prints an empty line.

`INPUT()` reads a line of text from standard input. Because the stream may terminate, it returns an alternative type: `STRING OR EOF`.

```basic
LET line AS STRING OR EOF = INPUT()
IF line IS EOF THEN
    PRINT "End of input stream."
END IF
```

Basic Next also supports prompt-style input statements:

```basic
LET x AS STRING OR EOF
LET y AS STRING OR EOF

INPUT "Enter value for X: ", x
INPUT "Enter value for Y: ", y
```

For screen positioning and cursor manipulation, import `HOST.Console`:

```basic
IMPORT HOST.Console AS Console

Console.Cls()
Console.Beep()
Console.PrintAt(1, 1, "Top left corner")
```

## Changing Console Colors

Setting foreground and background colors in the terminal is an upcoming feature planned for `HOST.Console`. Dedicated color functions will be added in a future release.
