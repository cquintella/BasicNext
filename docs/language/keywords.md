# Basic Next Keyword Registry

This registry is the source of truth for keyword intent. A keyword is reserved
only after its syntax and semantics are accepted into a language specification.
`Reserved` means present in the 0.1 draft; `Proposed` means documented but not
part of 0.1; `Decision needed` identifies competing forms that must not coexist
without a deliberate decision.

Keywords are case-sensitive and reserved only in the uppercase forms shown
below. For example, `PRINT` is a keyword and `Print` is an identifier.

## Declarations and program structure

| Form | Meaning | Status |
| --- | --- | --- |
| `LET` | Declares a mutable binding. `AS TYPE` is mandatory. | Reserved |
| `CONST` | Declares an immutable binding. `AS TYPE` and an initializer are mandatory. | Reserved |
| `AS` | Introduces an explicit type or import alias. | Reserved |
| `FUNCTION` | Declares a callable routine. `AS VOID` declares an action; another type declares a value-producing function. | Reserved |
| `RETURN` | Returns a value from a non-`VOID` `FUNCTION`; it is not permitted in `FUNCTION ... AS VOID`. | Reserved |
| `CLASS` | Declares an object-oriented type. | Reserved |
| `PUBLIC`, `PRIVATE` | Set the visibility of a class member. | Reserved |
| `CONSTRUCTOR` | Initializes a new class instance. | Reserved |
| `DESTRUCTOR` | Runs once when `DELETE` releases a class instance. | Reserved |
| `SELF` | Names the current class instance. | Reserved |
| `INTERFACE` | Declares a public contract of method signatures. | Reserved |
| `IMPLEMENTS` | Declares the interfaces implemented by a class. | Reserved |
| `EXPORT` | Makes a module-level declaration available to importing modules. | Reserved |
| `END` | Closes a compound declaration or statement, such as `END FUNCTION` or `END IF`. | Reserved |
| `IMPORT` | Imports a module or host capability under an explicit local alias. | Reserved |
| `HOST` | Names the environment that provides capabilities. | Reserved |
| `SYSTEM` | Names the host-provided system object type. | Reserved |

## Types and values

| Form | Meaning | Status |
| --- | --- | --- |
| `BYTE` | Unsigned 8-bit integer. | Reserved |
| `INT16` | Signed 16-bit integer. | Reserved |
| `INT32` | Signed 32-bit integer. | Reserved |
| `UINT32` | Unsigned 32-bit integer. | Reserved |
| `FLOAT32` | IEEE 754 binary32 floating-point value. | Reserved |
| `FLOAT64` | IEEE 754 binary64 floating-point value. | Reserved |
| `INTEGER` | Alias for `INT32`; the default integer spelling. | Reserved |
| `FLOAT` | Alias for `FLOAT64`; the default floating-point spelling. | Reserved |
| `STRING` | Text value type. | Reserved |
| `BOOLEAN` | Logical value type. | Reserved |
| `VOID` | Absence of a return value. | Reserved |
| `TYPE[length]` | Fixed-size mutable vector. `length` is a non-negative integer literal; indices begin at `0`. | Accepted syntax |
| `POINTER TO TYPE` | Typed pointer to an allocated value. `[length]` defines a fixed region; `[]` accepts a runtime-sized region. | Reserved syntax |
| `NEW TYPE` | Allocates a value, region, or class instance. | Reserved syntax |
| `DELETE value` | Releases an allocation created by `NEW`. | Reserved syntax |
| `TRUE`, `FALSE` | Boolean literals. | Reserved |
| `EOF` | Singleton end-of-input value returned by `INPUT()`. | Reserved |
| `IS` | Tests an allowed alternative and narrows the binding in the matching branch. | Reserved |
| `NA` | Candidate literal for a missing value. | Proposed |

## Expressions

| Form | Meaning | Status |
| --- | --- | --- |
| `AND`, `OR`, `NOT` | Logical operators for `BOOLEAN`; bitwise operators for integral values. After `AS`, `OR` separates explicitly allowed types. | Reserved |
| `XOR` | Exclusive OR for `BOOLEAN` and bitwise exclusive OR for integral values. | Reserved |
| `SHL`, `SHR` | Left and logical-right shifts for integral values. | Reserved |
| `0b...`, `0x...` | Binary and hexadecimal integer literals. | Accepted syntax |
| `+`, `-`, `*`, `/` | Basic numeric arithmetic. `/` always performs floating-point division. | Accepted syntax |
| `**` | Exponentiation. It is right-associative and binds more tightly than unary `-`. | Accepted syntax |
| `%` | Integer modulo; its result is non-negative and smaller than the absolute divisor. | Accepted syntax |
| `+=`, `-=`, `*=`, `/=`, `%=`, `**=` | Assignment forms corresponding to the arithmetic operators. | Accepted syntax |

## Control flow

| Form | Meaning | Status |
| --- | --- | --- |
| `IF ... THEN ... ELSE ... END IF` | Conditional execution. | Reserved |
| `ELSE IF` | Compound conditional branch; it is the sequence `ELSE` followed by `IF`, not a new token. | Reserved form |
| `FOR ... TO ... END FOR` | Inclusive counted loop. | Reserved |
| `FOR EACH` | Candidate loop over elements of a collection. | Proposed |
| `WHILE ... END WHILE` | Loop that tests its condition before each iteration. | Reserved |
| `REPEAT ... UNTIL ... END REPEAT` | Loop that tests its condition after each iteration and closes explicitly. | Reserved |
| `STOP` | Candidate explicit termination of the current program. | Proposed |

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
