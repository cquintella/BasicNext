# Basic Next Keyword Registry

This registry is the source of truth for keyword intent. A keyword is reserved
only after its syntax and semantics are accepted into a language specification.
`Reserved` means present in the 0.1 draft; `Proposed` means documented but not
part of 0.1; `Decision needed` identifies competing forms that must not coexist
without a deliberate decision.

## Declarations and program structure

| Form | Meaning | Status |
| --- | --- | --- |
| `LET` | Declares a mutable binding. Types are explicit with `AS`. | Reserved |
| `CONST` | Declares an immutable binding. | Reserved |
| `AS` | Introduces an explicit type or import alias. | Reserved |
| `SUB` | Declares a procedure without a return value. | Reserved |
| `FUNCTION` | Declares a procedure with a return value. | Reserved |
| `RETURN` | Returns from the current procedure. | Reserved |
| `CLASS` | Declares an object-oriented type. | Reserved |
| `END` | Closes a compound declaration or statement, such as `END SUB` or `END IF`. | Reserved |
| `IMPORT` | Imports a module or host capability. Current spelling used by the draft. | Decision needed |
| `USE` | Candidate replacement for `IMPORT`. It is not reserved. | Decision needed |
| `HOST` | Names the environment that provides capabilities. | Reserved |
| `SYSTEM` | Names the host-provided system object type. | Reserved |

## Types and values

| Form | Meaning | Status |
| --- | --- | --- |
| `INTEGER` | Integer numeric type. | Reserved |
| `FLOAT` | Floating-point numeric type. | Reserved |
| `STRING` | Text value type. | Reserved |
| `BOOLEAN` | Logical value type. | Reserved |
| `VOID` | Absence of a return value. | Reserved |
| `TRUE`, `FALSE` | Boolean literals. | Reserved |
| `NA` | Candidate literal for a missing value. | Proposed |
| `IS` | Candidate type or absence test, for example `value IS NA`. | Proposed |

## Expressions

| Form | Meaning | Status |
| --- | --- | --- |
| `AND`, `OR`, `NOT` | Logical operators. `OR` is also proposed as a union separator in a type context. | Reserved / proposed extension |
| `MOD` | Integer remainder operator. | Reserved |

## Control flow

| Form | Meaning | Status |
| --- | --- | --- |
| `IF ... THEN ... ELSE ... END IF` | Conditional execution. | Reserved |
| `ELSE IF` | Compound conditional branch; it is the sequence `ELSE` followed by `IF`, not a new token. | Reserved form |
| `FOR ... TO ... NEXT` | Inclusive counted loop. | Reserved |
| `FOR EACH` | Candidate loop over elements of a collection. | Proposed |
| `WHILE ... END WHILE` | Loop that tests its condition before each iteration. | Reserved |
| `REPEAT ... UNTIL` | Loop that tests its condition after each iteration. | Reserved |
| `STOP` | Candidate explicit termination of the current program. | Proposed |

## I/O and comments

| Form | Meaning | Status |
| --- | --- | --- |
| `PRINT` | Writes a value to the current output provided by the host. | Reserved |
| `'` | Current line-comment marker in the 0.1 draft. | Reserved draft |
| `REM` | Candidate readable line-comment form. | Decision needed |
| `//` | Candidate familiar line-comment marker. It is punctuation, not a keyword. | Decision needed |

## Future pattern and error handling forms

| Form | Meaning | Status |
| --- | --- | --- |
| `MATCH` | Candidate pattern matching over unions and values. | Proposed |
| `OK`, `ERR` | Candidate constructors for error values. | Proposed |
| `UNION` | Candidate declaration of a named discriminated union. | Proposed |

## Decision rule

Basic Next follows KISS: where forms compete, the project selects one canonical
form instead of keeping aliases indefinitely. In particular, the project must
choose between `IMPORT` and `USE`, and between `'`, `REM`, and `//` for line
comments.
