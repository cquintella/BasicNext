# Basic Next 0.4 Keyword Registry

This registry is normative together with [`0.4.ebnf`](0.4.ebnf) and
[`0.4.md`](0.4.md). It inherits every reserved word from the 0.3 registry;
the only new 0.4 reserved words are:

| Keyword | Use in 0.4 |
|---|---|
| `ASYNC` | Declares an asynchronous named function or submits a named function to an explicitly selected `Dispatch.Queue`. |
| `AWAIT` | Waits for a `Dispatch.Ticket` for a bounded timeout. |

`ASYNC` and `AWAIT` are exact-uppercase reserved words and cannot be used as
identifiers. `BNDispatch` remains an explicitly imported external module;
neither word creates a built-in `HOST` capability. `PARALLEL` remains a
reserved lexical-only word and is not reactivated by this amendment.

<!-- reserved-words:start -->
```text
AND
AS
ASYNC
AWAIT
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

<!-- special-float-literals:start -->
```text
INF
NAN
```
<!-- special-float-literals:end -->
