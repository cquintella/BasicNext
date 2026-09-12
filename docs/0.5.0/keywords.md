# Basic Next 0.5.0 Keyword Registry

This registry is normative together with [`0.5.0.ebnf`](0.5.0.ebnf) and
[`language-0.5.0.md`](language-0.5.0.md). It inherits the 0.4 reserved-word set
except as amended below.

## 0.5.0 changes

| Change | Detail |
|---|---|
| **Removed** | `DELETE` — purged from language DNA (not class-only deprecation). |
| **Added** | `RELEASE` — optional advanced: early end of a binding (primary, fixed vector, struct, or class object); class = drop one strong; use-after-release is an error; not element-remove on fixed vectors. |
| **Retained** | `ASYNC`, `AWAIT` — typed `AWAIT` yields `T OR Error` when the ticket comes from `FUNCTION … AS T OR Error` (semantics in language-0.5.0.md). |
| **Retained** | `NEW`, `DESTRUCTOR` — allocation and destructor bodies remain; lifetime is ARC, not manual dispose. |

`ASYNC` and `AWAIT` remain exact-uppercase reserved words. `BNDispatch` remains an
explicitly imported external module. `PARALLEL` remains reserved lexical-only.
Weak-reference annotation spelling is **TBD** (Carlos open) and is not a keyword
in this registry until locked.

HOST resource teardown uses capability methods (`Close`, `*_close`), which are
**identifiers on host types**, not the removed `DELETE` keyword.

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
RELEASE
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
