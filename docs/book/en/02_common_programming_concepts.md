# Common Programming Concepts

[← Previous: Introduction](01_introduction.md) · [Contents](toc.md)

Before a program can decide anything or repeat anything, it needs somewhere to keep values and a vocabulary for describing them. This chapter covers that layer: how to document intent in a comment, how to declare variables and constants, what the primitive types guarantee, how operators combine them, how to convert between them, and how to read from and write to the console.

The organizing idea throughout is that Basic Next requires the type of every binding to be written down and never changes a value's type without being told to. That single decision determines most of what follows, including the diagnostics, so the chapter states the rule in each context and shows what the compiler reports when it is not met.

## Comments

Basic Next has two comment forms: `//` runs to the end of the line, and `/* ... */` spans as many lines as needed.

```basic
// Reads the next reading from the sensor buffer.

/*
   The calibration constant below comes from the vendor datasheet,
   revision C. It changes with the firmware, not with the hardware.
*/
```

The distinction worth holding onto is that code already states what a program does, so a comment that repeats it adds a line that can fall out of date without being noticed. What code cannot state is why a particular decision was made: which constraint forced a value, which alternative was rejected and for what reason, which input the function is known not to handle. That is what belongs in a comment.

In practice this means documenting the rationale rather than the mechanics, describing the contract of anything a module exports, updating a comment in the same edit that changes the code it describes, and marking unfinished work with a recognizable token such as `TODO`, `FIXME`, or `NOTE` so that it can be found by searching. A comment explaining a line that would be clear under a better name should be replaced by the better name.

## Variables

A variable is declared with `LET`, followed by its name, `AS`, its type, and optionally an initial value. The type is fixed for the variable's entire lifetime; a binding declared `INTEGER` never holds a `FLOAT`.

```basic
LET counter AS INTEGER = 10
LET name AS STRING = "Alice"
```

Several names of the same type may be declared in one statement, each with its own initializer:

```basic
LET first, second AS STRING = "auto", "bus"
```

Omitting the initializer is allowed for the scalar types, and the binding takes the documented default for its type: `0` or `0.0` for numeric types, `FALSE` for `BOOLEAN`, and the empty string for `STRING`.

```basic
LET score AS INTEGER     // 0
LET active AS BOOLEAN    // FALSE
LET label AS STRING      // ""
```

Alternative types are the exception: a binding whose type includes `OR` must be initialized, because there is no basis for choosing which alternative the default belongs to.

```basic
LET answer AS STRING OR EOF = ""
```

Omitting the type is never allowed. There is no inference for `LET`:

```basic
LET message = "Hello"
```

```
error[E0100]: Syntax error: a binding declaration requires AS TYPE
```

The reason for the rule is that a reader of a declaration should not have to evaluate the initializer, or trace a chain of function calls, to know what the binding holds. Writing the type also makes the declaration the place where a wrong assumption is caught, rather than the first arithmetic operation that depends on it.

Names are worth spending thought on at this point, because Basic Next gives no way to recover meaning that a name fails to carry. Prefer a name that states what the value is in the problem being solved — `remainingAttempts`, `unitPrice`, `isEligible` — over one that describes its representation or its position in a loop. A name whose meaning requires a comment to explain is a name that has not been chosen yet.

## Constants

`CONST` declares a binding that cannot be reassigned. It always requires an initializer, and an attempt to assign to it is rejected:

```basic
CONST MAX_USERS AS INTEGER = 100
```

```
error[TYPE_MISMATCH]: Expected mutable binding, but found CONST 'MAX_USERS' in assignment target.
```

The type may be omitted when the initializer is a direct scalar literal, in which case it is inferred: a whole number gives `INTEGER` (that is, `INT32`), a decimal number gives `FLOAT` (`FLOAT64`), `TRUE` and `FALSE` give `BOOLEAN`, and quoted text gives `STRING`.

```basic
CONST LIMIT = 100          // INT32
CONST RATIO = 3.14159      // FLOAT64
CONST ACTIVE = TRUE        // BOOLEAN
CONST APP_NAME = "MyApp"   // STRING
```

Inference applies to literals only. A specific width or signedness, a negative value, an expression, a vector, or a special value such as `NULL` or `EOF` requires the type to be written:

```basic
CONST BUFFER_SIZE AS UINT32 = 4096
CONST SENTINEL AS INTEGER = -1
CONST AREA AS INTEGER = 4 * 5
```

Note that `CONST` fixes the binding, not what the binding refers to. A constant holding a class instance cannot be pointed at a different instance, and the instance's fields remain mutable.

Constants are the mechanism for removing unexplained literals from code. A `100` appearing in three conditions is three independent facts that happen to agree; `MAX_USERS` is one fact referenced three times, and changing it is one edit rather than a search. Declare the constant where the value's meaning belongs — inside the function when only that function is concerned, at module level with `EXPORT` when it is part of the module's contract — and name it for what it limits rather than for its value.

## Inspecting a Type

`TYPEOF(expression)` returns the canonical name of an expression's static type as a `STRING`. The aliases report their underlying representation, which makes it a useful way to confirm what an inferred constant actually is:

```basic
FUNCTION Start() AS VOID
    CONST count = 10
    CONST rate = 10.2
    LET flag AS BOOLEAN = TRUE
    LET message AS STRING = "Basic Next"

    PRINT TYPEOF(count)     // INT32
    PRINT TYPEOF(rate)      // FLOAT64
    PRINT TYPEOF(flag)      // BOOLEAN
    PRINT TYPEOF(message)   // STRING
END FUNCTION
```

`TYPEOF` reports the type known at compile time. It is a diagnostic aid for the programmer, not a mechanism for branching on types at runtime.

## Primitive Types

### Numeric Types

The two general-purpose numeric types are `INTEGER`, a signed 32-bit integer identical to `INT32`, and `FLOAT`, an IEEE 754 double-precision number identical to `FLOAT64`. Use them unless a specific width is required by the problem.

When it is — a binary file format, a hardware register, a protocol field — the fixed-width types state the layout exactly. The signed types are `INT8`, `INT16`, `INT32`, and `INT64`; the unsigned types are `BYTE` (unsigned 8-bit), `UINT16`, `UINT32`, and `UINT64`; the floating-point types are `FLOAT32` and `FLOAT64`. `TIMESTAMP` is an alias for `INT64` with a documented meaning, described below.

Integer arithmetic in Basic Next does not wrap. An operation whose result falls outside the range of its type raises `NUMERIC_OVERFLOW` at runtime, naming the value and the target type:

```basic
LET n AS INT8 = 127
n += 1
```

```
error[NUMERIC_OVERFLOW]: Numeric overflow while converting 128 to Int8.
```

This is a deliberate trade. Wraparound is faster and is what most languages inherit from the hardware, but a program that wraps continues running with a value that is arithmetically wrong, and the consequence surfaces somewhere else entirely. Halting at the operation that overflowed puts the failure where the cause is.

Floating-point values follow IEEE 754, including the special values `NAN`, `INF`, and `-INF`. `NAN` is not equal to itself, so testing for it uses `IS NAN` rather than `=`.

### `BOOLEAN`

`BOOLEAN` has exactly two values, `TRUE` and `FALSE`, and no other type is interchangeable with it. `PRINT` renders them as `TRUE` and `FALSE`. The consequences for conditions are the subject of the next chapter.

### `STRING`

A `STRING` is an immutable sequence of Unicode scalar values, written between double quotes. A literal cannot span a line break.

`LEN(s)` gives the number of scalar values, and `s[i]` gives the scalar at a zero-based index as a `STRING` of length one. An index outside `0` through `LEN(s) - 1` raises `INDEX_OUT_OF_BOUNDS`.

```basic
FUNCTION Start() AS VOID
    LET word AS STRING = "café"
    PRINT LEN(word)                  // 4
    PRINT word[0]                    // c
    PRINT word[LEN(word) - 1]        // é
    PRINT SIZEOF(word)               // 5
END FUNCTION
```

The last two lines illustrate a distinction that matters whenever text leaves the program. `LEN` counts characters in the sense a reader means; `SIZEOF` gives the size in bytes of the UTF-8 encoding, which is larger whenever the text contains a character outside ASCII. Use `LEN` for indexing and for anything presented to a person, and `SIZEOF` for buffers and storage.

Strings are immutable: `s[i]` is not an assignment target, there is no slice syntax, and there is no `CHAR` type. Two global functions convert between a character and its code point: `ASC(text)` returns the scalar value of the first character as `INTEGER OR Error`, and `CHAR(code)` returns a one-character `STRING OR Error`. Both return `Error` rather than failing — for an empty string and for a code that is not a valid scalar value, respectively — and neither requires an import.

### Temporal Types

Four types describe time. `TIMESTAMP` is an `INT64` count of milliseconds since the Unix epoch in UTC. `DATE`, `TIME`, and `TIMEZONE` are distinct value types for a calendar date, a clock time, and a zone identifier. Their defaults are what an uninitialized binding takes:

```basic
FUNCTION Start() AS VOID
    LET stamp AS TIMESTAMP     // 0
    LET day AS DATE            // 1970-01-01
    LET clock AS TIME          // 00:00:00.000
    LET zone AS TIMEZONE       // UTC

    PRINT day, clock, zone
END FUNCTION
```

Chapter eight covers reading the current time through `HOST.Clock` and converting between these types.

## Operators and Expressions

Expressions are strictly typed. Operands of a binary operator must have the same type, and mixing types without an explicit conversion is rejected at compile time.

### Arithmetic

The arithmetic operators are `+`, `-`, `*`, and `**` for exponentiation, which is right-associative. Division is three separate operations rather than one that changes meaning with its operands:

```basic
LET half AS FLOAT = 5 / 2            // 2.5
LET quotient AS INTEGER = 5 DIV 2    // 2
LET remainder AS INTEGER = 5 % 2     // 1
```

`/` always produces a `FLOAT`, even when both operands are integers. `DIV` performs integer division, truncating towards zero. `%` gives the integer remainder. Writing three operators instead of one removes the most common source of quiet arithmetic error in other languages, where `5 / 2` yields `2` or `2.5` depending on the static types of the operands.

`+` also concatenates strings. It does not concatenate a string with a number; convert the number first.

### Comparison

`=` tests equality and `<>` inequality; `<`, `<=`, `>`, and `>=` compare ordered values. Both operands must have the same static type. Note that `=` is equality in an expression and assignment in a statement, and the two positions never overlap: an assignment is a statement, so a comparison cannot be written where an assignment is meant, and the class of defect represented by C's `if (x = 0)` does not arise.

### Logical and Bitwise

`AND`, `OR`, `NOT`, and `XOR` serve both roles, determined by the type of their operands. With `BOOLEAN` operands they are short-circuit logical operators, evaluating the right-hand side only when the result is not already determined. With integer operands they are bitwise:

```basic
FUNCTION Start() AS VOID
    LET a AS INTEGER = 12       // 0b1100
    LET b AS INTEGER = 10       // 0b1010

    PRINT a AND b               // 8
    PRINT a OR b                // 14
    PRINT a XOR b               // 6
    PRINT a SHL 2               // 48
    PRINT a SHR 2               // 3
END FUNCTION
```

`SHL` and `SHR` shift left and right and apply to integer types only. Because the operands cannot be mixed, there is no ambiguity about which reading applies at a given site.

When a compound condition is long enough that its grouping is not obvious at a glance, parenthesize it, or give it a name by extracting a function that returns `BOOLEAN`. The second is usually better: it documents the rule and makes it testable in isolation.

## Explicit Conversion

Basic Next performs no implicit conversion, including between numeric types that would lose nothing. Assigning an `INTEGER` to a `FLOAT` binding is an error:

```
error[TYPE_MISMATCH]: Expected FLOAT(FLOAT64), but found INTEGER(INT32) in binding initializer.
```

The `AS` operator states the conversion:

```basic
FUNCTION Start() AS VOID
    LET count AS INTEGER = 3
    LET ratio AS FLOAT = count AS FLOAT
    LET truncated AS INTEGER = 3.9 AS INTEGER     // 3
    LET flag AS BOOLEAN = 0 AS BOOLEAN            // FALSE

    PRINT ratio, truncated, flag
END FUNCTION
```

Converting from floating-point to integer truncates the fractional part towards zero; it does not round. A value outside the target type's range raises `INVALID_NUMERIC_CONVERSION`. Converting to `BOOLEAN` treats numeric zero and the empty string as `FALSE` and anything else as `TRUE` — a conversion that must be asked for, which is what distinguishes it from the implicit truthiness the language does not have.

A conversion in the middle of an expression is a signal worth reading. It usually means a value is being carried in a type that does not match its meaning, and the better fix is often to change the declaration rather than to add the cast.

## Numeric Limits

The bounds of the numeric types are constants in the standard `BNMath` module, which must be imported:

```basic
IMPORT BNMath AS Math

FUNCTION Start() AS VOID
    PRINT Math.MIN_INT32, Math.MAX_INT32    // -2147483648 2147483647
    PRINT Math.MIN_INT64, Math.MAX_INT64
    PRINT Math.MIN_FLOAT, Math.MAX_FLOAT
END FUNCTION
```

Reading a bound from `BNMath` rather than writing the number keeps the meaning visible and survives a change of type in the declaration it guards.

## Console Input and Output

### `PRINT`

`PRINT` writes its arguments to standard output followed by a newline. Arguments separated by commas are printed with a single space between them; joining with `+` concatenates without adding anything. `PRINT` with no arguments writes an empty line.

```basic
FUNCTION Start() AS VOID
    LET name AS STRING = "Alice"

    PRINT "Processing user:", name       // Processing user: Alice
    PRINT "Processing user: " + name     // Processing user: Alice
    PRINT
    PRINT "done"
END FUNCTION
```

The two forms above produce the same output here only because the literal ends with a space. Prefer the comma form for messages assembled from several values, and reserve `+` for text that is genuinely one string.

### `INPUT`

Standard input can end at any time, so reading a line returns an alternative type, `STRING OR EOF`, rather than a `STRING`. The expression form reads one line:

```basic
FUNCTION Start() AS VOID
    LET line AS STRING OR EOF = INPUT()
    IF line IS EOF THEN
        PRINT "End of input stream."
        RETURN
    END IF
    PRINT "got:", line
END FUNCTION
```

The `EOF` case has to be handled before the value can be used as a `STRING`; the next chapter explains the narrowing rule that makes the code after the guard typecheck. This is the language's general approach to operations that can fail to produce a value: the possibility appears in the type, and the compiler requires it to be addressed.

The statement form writes a prompt and stores the result in an existing binding. The binding must already be declared and initialized, because its type includes an alternative:

```basic
FUNCTION Start() AS VOID
    LET x AS STRING OR EOF = ""

    INPUT "Enter value for X: ", x
    IF x IS EOF THEN
        RETURN
    END IF
    PRINT "x =", x
END FUNCTION
```

### Positioned Output

Clearing the screen, moving the cursor, and writing at a coordinate are capabilities of the host rather than statements of the language, and they require an import:

```basic
IMPORT HOST.Console AS Console

FUNCTION Start() AS VOID
    Console.Cls()
    Console.Beep()
    Console.PrintAt(1, 1, "Top left corner")
END FUNCTION
```

`PrintAt` coordinates are one-based terminal cells, in contrast to the zero-based indices used for strings and vectors. The two conventions are deliberate and are not interchangeable. Setting foreground and background colours is not available in this version.

## Choosing Types Deliberately

Three habits follow from the rules in this chapter and are worth adopting from the first program.

Declare the type that matches the meaning of the value, not the one that makes the current line compile. A count of retries is an `INTEGER`; a byte read from a file is a `BYTE`; a ratio is a `FLOAT`. When a declaration forces a cast at every use, the declaration is usually the thing to change.

Use `INTEGER` and `FLOAT` by default and reach for a fixed width when the layout is part of the problem. A narrower type does not make a program faster in any way a beginner will measure, and it introduces an overflow boundary that must be respected.

Replace a literal with a named constant as soon as it appears in a condition or is used more than once. The name is where the reasoning lives, and it is the only part of the program that can explain why the number is what it is.

## Diagnostics Introduced in This Chapter

| Diagnostic | Raised when |
| --- | --- |
| `E0100` | A `LET` declaration omits `AS TYPE`. |
| `TYPE_MISMATCH` | Types differ without an explicit conversion; a `CONST` is used as an assignment target; an alternative-typed binding has no initializer. |
| `E0001` | A `CONST` without a type is initialized by something other than a scalar literal. |
| `NUMERIC_OVERFLOW` | An integer operation or conversion leaves the range of its type. |
| `INVALID_NUMERIC_CONVERSION` | An `AS` conversion targets a type that cannot hold the value. |
| `INDEX_OUT_OF_BOUNDS` | A string index is outside `0` through `LEN(s) - 1`. |

## Summary

Every binding states its type, and the type does not change. Constants fix a name to a value and are the place where a number's meaning is recorded. Integers do not wrap, floating-point division is a different operator from integer division, and no value becomes another type without `AS`. Reading a line of input yields `STRING OR EOF`, because input can end, and the program must say what it does when it has.

Each of these makes a program longer to write and shorter to reason about. The next chapter applies them to the constructs that give a program its shape: conditionals, loops, and the rules that govern leaving them.

---

[Next: Control Flow →](03_control_flow.md)
