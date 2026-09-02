# Common Programming Concepts

This chapter covers the fundamental building blocks of a Basic Next program: how to store data, manipulate values, and interact with the console.

## Variables and Constants

Basic Next enforces strict typing. Every variable must explicitly state its type. The language does not use type inference for bindings.

Variables are declared using the `LET` keyword, followed by the name, `AS`, the type, and an optional initializer. In version 0.3, you can also declare multiple variables of the same type in a single `LET` binding:

```basic
LET counter AS INTEGER = 10
LET name AS STRING = "Alice"
LET c, v AS STRING = "carro", "moto"
```

If you omit the initializer, the variable is initialized to its type's default value. In Basic Next, there is no uninitialized storage. The default for numeric types is `0` or `0.0`, `BOOLEAN` defaults to `FALSE`, and `STRING` defaults to an empty string `""`.

```basic
LET score AS INTEGER  // Initialized to 0
LET active AS BOOLEAN // Initialized to FALSE
```

Constants are declared using the `CONST` keyword. They must always include an initializer and cannot be reassigned:

```basic
CONST MAX_USERS AS INTEGER = 100
```

*Note: `CONST` fixes the binding itself. It does not make a referenced class or allocated pointer deeply immutable.*

## Primitive Types

Basic Next features a rich set of primitive types with guaranteed, cross-platform representations.

### Numeric Types

The default numeric types are `INTEGER` (an alias for a signed 32-bit integer) and `FLOAT` (an alias for an IEEE 754 64-bit floating-point number). 

When exact memory layout is important, Basic Next provides fixed-width types:
- **Signed:** `INT8`, `INT16`, `INT32`, `INT64`
- **Unsigned:** `BYTE` (unsigned 8-bit), `UINT16`, `UINT32`, `UINT64`
- **Floating-point:** `FLOAT32`, `FLOAT64`

Integer arithmetic in Basic Next never wraps or saturates implicitly. An operation that produces a result outside the destination type's range raises a `NUMERIC_OVERFLOW` error at runtime. 

Floating-point numbers follow IEEE 754 semantics and include special values: `NAN` (Not a Number), `INF` (positive infinity), and `-INF` (negative infinity).

### Boolean and String

- `BOOLEAN` represents logical states with the keywords `TRUE` and `FALSE`.
- `STRING` represents text. Strings are UTF-8 encoded and enclosed in double quotes. Line breaks inside strings are not permitted in version 0.3.

### Temporal Types

Basic Next treats time as primitive data:
- `TIMESTAMP`: An alias for `INT64` representing UTC Unix-epoch milliseconds.
- `DATE`, `TIME`, `TIMEZONE`: Value types representing specific calendar and clock concepts. Their default values are `1970-01-01`, `00:00:00.000`, and `UTC` respectively. 

## Operators and Expressions

Basic Next expressions are strictly typed. The compiler prevents mixing incompatible types, such as adding a string to a number, without explicit conversion.

### Arithmetic

The basic arithmetic operators are `+`, `-`, `*`, and `**` (exponentiation). 
Basic Next distinguishes strictly between integer and floating-point division:
- `/` always performs floating-point division and returns a `FLOAT`, even if both operands are integers.
- `DIV` performs Euclidean integer division.
- `%` performs Euclidean modulo (the remainder is always non-negative).

```basic
LET half AS FLOAT = 5 / 2       // 2.5
LET quotient AS INTEGER = 5 DIV 2 // 2
LET remainder AS INTEGER = 5 % 2  // 1
```

The `+` operator also concatenates `STRING` values.

### Equality and Comparison

Equality (`=`) and inequality (`<>`) require operands to have the exact same static type, outside of allowed numeric widening. You cannot implicitly compare a `STRING` to `FALSE` or an `INTEGER` to a `FLOAT` without explicit conversion.

```basic
IF counter = 10 THEN
    PRINT "Limit reached"
END IF
```

### Logical and Bitwise Operators

The operators `AND`, `OR`, `NOT`, and `XOR` change their behavior based on the static type of their operands:
- When used with `BOOLEAN`, they perform logical operations. `AND` and `OR` use short-circuit evaluation.
- When used with integer types, they perform bitwise operations.

Basic Next also provides `SHL` (shift left) and `SHR` (shift right) for integers.

## Explicit Type Conversion

Because assignments and comparisons do not implicitly widen or coerce values, you must use the `AS` keyword to convert values explicitly:

```basic
LET count AS INTEGER = 3
LET ratio AS FLOAT = count AS FLOAT
```

Converting a floating-point number to an integer truncates toward zero. Conversions are checked at runtime: attempting to convert a value that falls outside the target type's range raises an `INVALID_NUMERIC_CONVERSION` error.

`AS BOOLEAN` is a special case: for numeric types, `0` becomes `FALSE` and any non-zero value (including `NAN`) becomes `TRUE`. For strings, `""` is `FALSE` and any non-empty string is `TRUE`.

## Basic Console I/O

Interacting with the console uses straightforward built-in macros.

`PRINT` writes text to standard output and then a line ending. Several
expressions are concatenated with no separator. With no expression it writes
a blank line.

```basic
PRINT "Processing user: ", name
PRINT "Processing user: " + name
```

`INPUT()` reads a line from standard input. Because the input might end, it returns a compound alternative type: `STRING OR EOF`. 

```basic
LET line AS STRING OR EOF = INPUT()
IF line IS EOF THEN
    PRINT "End of input stream."
END IF
```

To manage the terminal window, pass the `HOST.Console` capability to explicitly access terminal control methods. `HOST.Console` is a primary expression.

```basic
IMPORT HOST.Console AS Console
Console.Cls()
Console.Beep()
Console.PrintAt(1, 1, "Top left corner")
```
