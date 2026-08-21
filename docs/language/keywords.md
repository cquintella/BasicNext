# Basic Next Keyword Registry

This registry is the source of truth for keyword intent. `Reserved` means the
word cannot be an identifier: it may have 0.1 syntax or be deliberately held
for a documented future feature. `Proposed` means documented but not part of
0.1; `Decision needed` identifies competing forms that must not coexist without
a deliberate decision.

Keywords are case-sensitive and reserved only in the uppercase forms shown
below. For example, `PRINT` is a keyword and `Print` is an identifier.

## Declarations and program structure

| Form | Meaning | Status |
| --- | --- | --- |
| `LET` | Declares a mutable binding. `AS TYPE` is mandatory. | Reserved |
| `CONST` | Declares an immutable binding. `AS TYPE` and an initializer are mandatory. | Reserved |
| `AS` | Introduces an explicit type, postfix conversion, or import alias. | Reserved |
| `FUNCTION` | Declares a callable routine and spells a function value type: `FUNCTION(types) AS return-type`. | Reserved |
| `RETURN` | `RETURN expression` returns from a non-`VOID` `FUNCTION`; bare `RETURN` exits a `VOID` function early. | Reserved |
| `EXIT` | Leaves the named nearest loop early. | Reserved |
| `CLASS` | Declares an object-oriented type. | Reserved |
| `STRUCT` | Declares a copied value type with named public fields. | Reserved |
| `PUBLIC`, `PRIVATE` | Set the visibility of a class member. | Reserved |
| `CONSTRUCTOR` | Marks `FUNCTION CONSTRUCTOR(...)`, which initializes a new class instance. | Reserved |
| `DESTRUCTOR` | Marks `FUNCTION DESTRUCTOR()`, which runs once when `DELETE` releases a class instance. | Reserved |
| `SELF` | Names the current class instance. | Reserved |
| `INTERFACE` | Declares a public contract of method signatures. | Reserved |
| `IMPLEMENTS` | Declares the interfaces implemented by a class. | Reserved |
| `EXTENDS` | Reserved for a future inheritance declaration; no 0.1 semantics yet. | Reserved for future use |
| `PARALLEL` | Reserved for structured data-parallel blocks; no 0.1 semantics yet. Candidate forms are `PARALLEL ... END PARALLEL`, `PARALLEL FOR ... END PARALLEL FOR`, and `PARALLEL FOR EACH ... END PARALLEL FOR`. | Reserved for future use |
| `STATIC` | Declares a class-level field or function; it is accessed through the class name. | Reserved |
| `EXPORT` | Makes a module-level declaration available to importing modules. | Reserved |
| `END` | Closes a compound declaration or statement, such as `END FUNCTION` or `END IF`. | Reserved |
| `IMPORT` | Imports a module or host capability under an explicit local alias. | Reserved |
| `HOST` | Names the environment that provides capabilities. | Reserved |
| `SYSTEM` | Names the host-provided system object type. | Reserved |

## Types and values

| Form | Meaning | Status |
| --- | --- | --- |
| `BYTE` | Unsigned 8-bit integer. | Reserved |
| `INT8` | Signed 8-bit integer. | Reserved |
| `INT16` | Signed 16-bit integer. | Reserved |
| `INT32` | Signed 32-bit integer. | Reserved |
| `INT64` | Signed 64-bit integer. | Reserved |
| `UINT16` | Unsigned 16-bit integer. | Reserved |
| `UINT32` | Unsigned 32-bit integer. | Reserved |
| `UINT64` | Unsigned 64-bit integer. | Reserved |
| `FLOAT32` | IEEE 754 binary32 floating-point value. | Reserved |
| `FLOAT64` | IEEE 754 binary64 floating-point value. | Reserved |
| `INTEGER` | Alias for `INT32`; the default integer spelling. | Reserved |
| `FLOAT` | Alias for `FLOAT64`; the default floating-point spelling. | Reserved |
| `TIMESTAMP` | Alias for `INT64`, representing a UTC Unix-epoch instant in nanoseconds. | Reserved |
| `STRING` | Text value type. | Reserved |
| `BOOLEAN` | Logical value type. | Reserved |
| `VOID` | Absence of a return value. | Reserved |
| `TYPE[length][...]` | Fixed-size mutable vector with one or more non-negative literal dimensions; indices begin at `0`. | Accepted syntax |
| `POINTER TO TYPE` | Typed pointer to an allocated value. `[length]` defines a fixed region; `[]` accepts a runtime-sized region. | Reserved syntax |
| `NEW TYPE` | Allocates a value, region, or class instance. | Reserved syntax |
| `DELETE value` | Releases an allocation created by `NEW`. | Reserved syntax |
| `TRUE`, `FALSE` | Boolean literals. | Reserved |
| `EOF` | Singleton end-of-input value returned by `INPUT()`. | Reserved |
| `NULL` | Absence of an object, reference, or pointer; requires an explicit alternative type. | Reserved |
| `NA` | Missing observation or datum; requires an explicit alternative type. | Reserved |
| `NAN` | IEEE 754 not-a-number `FLOAT` value; it is distinct from `NA`. Its exact spelling is a special literal and cannot be an identifier. | Special literal |
| `INF`, `-INF` | Positive and negative IEEE 754 `FLOAT` infinity. `-INF` is unary negation of the `INF` special literal. | Special literal |
| `IS` | Tests an allowed alternative and narrows the binding in the matching branch. | Reserved |
| `Error` | Standard-library error object with `Code` and `Message`; it is not a keyword. | Standard-library type |

## Expressions

| Form | Meaning | Status |
| --- | --- | --- |
| `AND`, `OR`, `NOT` | Logical operators for `BOOLEAN`; bitwise operators for integral values. Operand types select the meaning; mixed operands are invalid. After `AS`, `OR` separates explicitly allowed types. | Reserved |
| `XOR` | Exclusive OR for `BOOLEAN` and bitwise exclusive OR for integral values. | Reserved |
| `SHL`, `SHR` | Left and logical-right shifts for integral values. | Reserved |
| `0b...`, `0x...` | Binary and hexadecimal integer literals. | Accepted syntax |
| `+`, `-`, `*`, `/`, `DIV` | Basic numeric arithmetic. `/` always performs floating-point division; `DIV` performs Euclidean integer division. | Accepted syntax |
| `**` | The only exponentiation operator. It performs checked integral power for integral operands and real power for floating operands; it is right-associative and binds more tightly than unary `-`. | Accepted syntax |
| `%` | Integer modulo; its result is non-negative and smaller than the absolute divisor. | Accepted syntax |
| `+=`, `-=`, `*=`, `/=`, `%=`, `**=` | Assignment forms corresponding to the arithmetic operators. | Accepted syntax |
| `Math.ABS`, `Math.MIN`, `Math.MAX`, `Math.SIGN` | General numeric functions in the standard `Math` namespace. | Standard library |
| `Math.FLOOR`, `Math.CEIL`, `Math.TRUNC`, `Math.ROUND` | Floating rounding functions in `Math`. | Standard library |
| `Math.EXP`, `Math.LOG`, `Math.LOG10`, `Math.LOG2`, `Math.POW` | Exponential, logarithmic, and real-power functions in `Math`. | Standard library |
| `Math.SIN`, `Math.COS`, `Math.TAN`, `Math.ASIN`, `Math.ACOS`, `Math.ATAN`, `Math.ATAN2` | Radian trigonometric functions in `Math`. | Standard library |
| `Math.SQRT`, `Math.HYPOT`, `Math.FMA` | Stable floating numerical functions in `Math`. | Standard library |

## Control flow

| Form | Meaning | Status |
| --- | --- | --- |
| `IF ... THEN ... ELSE ... END IF` | Conditional execution. | Reserved |
| `ELSE IF` | Compound conditional branch; it is the sequence `ELSE` followed by `IF`, not a new token. | Reserved form |
| `FOR ... TO ... STEP ... END FOR` | Inclusive counted loop; `STEP` is optional and defaults to `1`. | Reserved |
| `FOR EACH item AS TYPE IN values ... END FOR` | Iterates in index order over a fixed-size vector; the explicitly typed item binding is read-only. | Reserved |
| `EACH`, `IN` | Form the `FOR EACH` loop syntax. | Reserved |
| `EXIT FOR`, `EXIT WHILE`, `EXIT REPEAT` | Leave the nearest matching loop. `EXIT FOR` also exits `FOR EACH`. | Reserved form |
| `CONTINUE FOR`, `CONTINUE WHILE`, `CONTINUE REPEAT` | Skip to the next iteration of the nearest matching loop. `CONTINUE FOR` also applies to `FOR EACH`. | Reserved form |
| `STEP` | Introduces the signed increment of a counted `FOR`. | Reserved |
| `WHILE ... END WHILE` | Loop that tests its condition before each iteration. | Reserved |
| `REPEAT ... UNTIL ... END REPEAT` | Loop that tests its condition after each iteration and closes explicitly. | Reserved |
| `STOP value` | Immediately terminates the BN program and reports an `INTEGER` exit code from `0` through `255` to the operating system. | Reserved |

## I/O and comments

| Form | Meaning | Status |
| --- | --- | --- |
| `PRINT` | Macro for `Console.WriteLine(...)`; writes to standard output. | Reserved |
| `INPUT()` | Macro for `Console.ReadLine()`; reads one line as a `STRING`, or returns `EOF` at end of input. | Reserved syntax |
| `//` | Starts a line comment that continues to the line ending. It is punctuation, not a keyword. | Accepted syntax |
| `/* ... */` | Starts and ends a non-nesting block comment. It is punctuation, not a keyword. | Accepted syntax |

## Future pattern and error handling forms

| Form | Meaning | Status |
| --- | --- | --- |
| `MATCH` | Candidate pattern matching over values. | Proposed |
| `OK`, `ERR` | Candidate constructors for error values. | Proposed |

## Decision rule

Basic Next follows KISS: where forms compete, the project selects one canonical
form instead of keeping aliases indefinitely. New language features must not
introduce synonyms. `//` is the line-comment form and `/* ... */` is the
accepted non-nesting block form.
