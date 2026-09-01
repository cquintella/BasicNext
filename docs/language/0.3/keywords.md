# Basic Next 0.3 Keyword Registry

This registry is the source of truth for keyword intent. `Reserved` means the
word cannot be an identifier: it may have 0.1, 0.2, or 0.3 syntax or be deliberately
held for a documented future feature. `Proposed` means documented but not part
of the accepted language; `Decision needed` identifies competing forms that
must not coexist without a deliberate decision. `Accepted syntax` is 0.3 unless
a row says otherwise.

Keywords are case-sensitive and reserved only in the uppercase forms shown
below. For example, `PRINT` is a keyword and `Print` is an identifier.

## Lexer reserved-word list

The Rust frontend generates its reserved-word table from the alphabetical list
between the markers below at build time. Keep one exact-uppercase word per
line. Special literals such as `NAN` and `INF` have their own marked list;
`-INF` is unary `Minus` followed by `INF`, not a third literal.

<!-- reserved-words:start -->
```text
AND
AS
BOOLEAN
BYTE
CLASS
CONST
CONSTRUCTOR
CONTINUE
DATE
DELETE
DESTRUCTOR
DIV
EACH
ELSE
END
EOF
EXIT
EXPORT
EXTENDS
FALSE
FLOAT
FLOAT32
FLOAT64
FOR
FUNCTION
HOST
IF
IMPLEMENTS
IMPORT
IN
INPUT
INT16
INT32
INT64
INT8
INTEGER
INTERFACE
IS
LEN
LET
NA
NEW
NOT
NULL
OR
PARALLEL
POINTER
PRINT
PRIVATE
PUBLIC
REPEAT
RETURN
SELF
SHL
SHR
SIZEOF
STATIC
STEP
STOP
STRING
STRUCT
SUPER
SYSTEM
THEN
TIME
TIMESTAMP
TIMEZONE
TO
TRUE
UINT16
UINT32
UINT64
UNTIL
VOID
WHILE
XOR
```
<!-- reserved-words:end -->

## Lexer special-float-literal list

The frontend generates `SPECIAL_FLOAT_LITERALS` from this list. These spellings
are `TokenKind::Special`, not keywords, and cannot be identifiers.

<!-- special-float-literals:start -->
```text
INF
NAN
```
<!-- special-float-literals:end -->

## Declarations and program structure

| Form | Meaning | Status |
| --- | --- | --- |
| `LET` | Declares one or more mutable bindings of the same type. `AS TYPE` is mandatory; an initializer, when present, has one expression per binding. In local bindings, vector dimensions may be declaration-time expressions; signature and field vector dimensions stay literal-only. | Reserved |
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
| `SUPER` | Names the direct base class for constructor chaining (`SUPER(...)`) and parent method calls (`SUPER.Name(...)`). It is not a value. | Accepted syntax |
| `INTERFACE` | Declares a public contract of method signatures. | Reserved |
| `IMPLEMENTS` | Declares the interfaces implemented by a class. Each name is a `named-type` (`Printable` or `Pets.Named`). 0.2. | Reserved |
| `EXTENDS` | Declares the single base class for inheritance. | Accepted syntax |
| `PARALLEL` | Reserved for structured data-parallel blocks; no 0.1 semantics yet. Candidate forms are `PARALLEL ... END PARALLEL`, `PARALLEL FOR ... END PARALLEL FOR`, and `PARALLEL FOR EACH ... END PARALLEL FOR`. | Reserved for future use |
| `STATIC` | Declares a class-level field or function; it is accessed through the class name. | Reserved |
| `EXPORT` | Makes a module-level declaration available to importing modules. | Reserved |
| `END` | Closes a compound declaration or statement, such as `END FUNCTION` or `END IF`. | Reserved |
| `IMPORT` | Imports a module or host capability under an explicit local alias. Imported names are used only as `alias.member`. Language source modules use the `BN` root. | Reserved |
| `HOST` | Names the environment that provides capabilities: `HOST.Args`, `HOST.Clock`, `HOST.Console`, `HOST.Random`, `HOST.FileSystem`, `HOST.Net`, and `HOST.NumProcs()`. `HOST.Main` and `HOST.Network` are not capabilities. | Reserved |
| `SYSTEM` | 0.1 type of `HOST.Main`. Withdrawn in 0.2; the spelling stays reserved so it is not an identifier. | Reserved |
| `HOST.Args[index]` | 0.2 executable-module argument list. No `IMPORT`. `[0]` is the absolute path of the executed file. Replaces `HOST.Main.Argument`. | Host form |

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
| `TIMESTAMP` | Alias for `INT64`, representing a UTC Unix-epoch instant in milliseconds. | Reserved |
| `DATE` | Immutable Gregorian calendar date with a four-digit year. | Reserved |
| `TIME` | Immutable civil time of day with millisecond precision. | Reserved |
| `TIMEZONE` | Immutable named IANA time-zone rule set. | Reserved |
| `STRING` | Text value type. | Reserved |
| `BOOLEAN` | Logical value type. | Reserved |
| `VOID` | Absence of a return value; also the opaque element in `POINTER TO VOID`. | Reserved |
| `TYPE[length][...]` | Fixed-size mutable vector with one or more non-negative literal dimensions; indices begin at `0`. This is the literal-only form used by signatures, fields, parameters, and return types. | Accepted syntax |
| `string[index]` | Read-only Unicode-scalar index of a `STRING`; result is a `STRING` of length `1`. Not an lvalue. 0.2. | Accepted syntax |
| `POINTER TO TYPE` | Typed pointer to an allocated value. `[length]` defines a fixed region; `[]` accepts a runtime-sized region. | Reserved syntax |
| `POINTER TO VOID` | C-style opaque pointer; implicitly converts to and from compatible-shape typed pointers, but cannot be indexed directly. | Reserved syntax |
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

## Language functions (identifiers)

These names are ordinary identifiers. They are not reserved words and do not
belong to a module.

| Form | Meaning | Status |
| --- | --- | --- |
| `ASC(text)` | Unicode scalar value of the first character of a `STRING`, as `INTEGER OR Error`. | Language function |
| `CHAR(code)` | `STRING` of length 1 from a Unicode scalar. `CHAR` is not a type. | Language function |

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
| `BNMath.ABS`, `BNMath.MIN`, `BNMath.MAX`, `BNMath.SIGN` | General numeric functions in the standard `BNMath` namespace. | Standard library |
| `BNMath.FLOOR`, `BNMath.CEIL`, `BNMath.TRUNC`, `BNMath.ROUND` | Floating rounding functions in `BNMath`. | Standard library |
| `BNMath.EXP`, `BNMath.LOG`, `BNMath.LOG10`, `BNMath.LOG2`, `BNMath.POW` | Exponential, logarithmic, and real-power functions in `BNMath`. | Standard library |
| `BNMath.SIN`, `BNMath.COS`, `BNMath.TAN`, `BNMath.ASIN`, `BNMath.ACOS`, `BNMath.ATAN`, `BNMath.ATAN2` | Radian trigonometric functions in `BNMath`. | Standard library |
| `BNMath.SQRT`, `BNMath.HYPOT`, `BNMath.FMA` | Stable floating numerical functions in `BNMath`. | Standard library |
| `BNMath.VAL` | Classic BASIC numeric conversion from `STRING` to `FLOAT`. 0.2. Not `Float.TryParse`. | Standard library |
| `BNMath.MEAN`, `BNMath.MEDIAN`, `BNMath.QUARTILE1`, `BNMath.QUARTILE3` | Descriptive statistics on a numeric vector or numeric pointer region. 0.2. | Standard library |
| `BNMath.MODE`, `BNMath.STDEV`, `BNMath.VARIANCE`, `BNMath.RANGE` | Mode (`FLOAT OR NA`), sample stdev/variance (`n−1`), and range. 0.2. | Standard library |
| `BNMath.MAX_INTEGER`, `BNMath.MIN_INTEGER`, `BNMath.MAX_FLOAT`, `BNMath.MIN_FLOAT` | Numeric range constants; width-specific `MAX_*` / `MIN_*` names are in `math.md`. 0.2. | Standard library |
| `BNWeb` | Explicitly imported external module for bounded HTTP clients and servers. It is not a `HOST` capability and introduces no keyword. | Standard library |
| `BNLog` | Explicitly imported external module for bounded structured logging with levels, formats, and transports. It is not a `HOST` capability and introduces no keyword. | Standard library |
| `BNJson` | Explicitly imported external module for bounded JSON encoding and decoding. It is not a `HOST` capability and introduces no keyword. | Standard library |
| `BNMath.TOHOUR`, `BNMath.TOWEEKDAY` | Pure UTC conversions from `TIMESTAMP`; weekday uses ISO 8601 numbering. | Standard library |
| `BNMath.TODATE`, `BNMath.TOTIME`, `BNMath.TOTIMESTAMP` | Explicit UTC conversion among temporal value types. | Standard library |

## Control flow

| Form | Meaning | Status |
| --- | --- | --- |
| `IF ... THEN ... ELSE ... END IF` | Conditional execution. | Reserved |
| `IF condition THEN statement [ELSE statement]` | Single-line conditional: the complete form stays on one physical line, contains one non-compound statement per branch, and has no `END IF`. | Reserved form |
| `ELSE IF` | Block conditional branch; it is the sequence `ELSE` followed by `IF`, not a new token. Zero or more may appear before the optional final `ELSE`. | Reserved form |
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
| `HOST.Console.Cls()` | 0.2 method: clear display and home cursor. Withdraws the 0.1 statement `CLS(HOST.Console)`. `CLS` is not reserved in 0.2. | Host method |
| `HOST.Console.Beep()` | 0.2 method: host bell. Withdraws the 0.1 statement `BEEP(HOST.Console)`. `BEEP` is not reserved in 0.2. | Host method |
| `HOST.Console.PrintAt(column, row, text)` | 0.2 method: positioned write, 1-based cells. `PRINTAT` is not a keyword. | Host method |
| `HOST.Console.NumCols()`, `HOST.Console.NumRows()` | 0.2 methods: current window size in terminal cells. TTY required. | Host method |
| `LEN()` | Returns a count: `1` for a numeric value, Unicode scalar count for a `STRING`, the product of dimensions for a vector, in 0.2 the element count of a region pointer (`POINTER TO T[n]` or `POINTER TO T[]`), and `LEN(HOST.Args)` for the argument list. Still a static error for a single `POINTER TO T`. | Reserved syntax |
| `SIZEOF()` | Returns the portable byte size of a value representation, with no padding. | Reserved syntax |
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
