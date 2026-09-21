# Basic Next 0.6 Keyword Registry

This registry is normative together with [`0.6.ebnf`](0.6.ebnf) and
[`0.6.md`](0.6.md). It inherits the 0.4 reserved-word set
except as amended below.

## 0.5.0 changes (ARC)

| Change | Detail |
|---|---|
| **Removed** | `DELETE` — purged from language DNA (not class-only deprecation). |
| **Added** | `WEAK` — weak class-type qualifier: `AS WEAK ClassName` (typically with `OR NULL`); dead weak reads as `NULL`. |
| **Added** | `RELEASE` — optional advanced: early end of a binding (primary, fixed vector, struct, or class object); class = drop one strong; use-after-release is an error; not element-remove on fixed vectors. |
| **Retained** | `ASYNC`, `AWAIT` — typed `AWAIT` yields `T OR Error` when the ticket comes from `FUNCTION … AS T OR Error` (semantics in 0.6.md). |
| **Retained** | `NEW`, `DESTRUCTOR` — allocation and destructor bodies remain; lifetime is ARC, not manual dispose. |

`ASYNC` and `AWAIT` remain exact-uppercase reserved words. `BNDispatch` remains an
explicitly imported external module. `PARALLEL` remains reserved lexical-only.
Weak class types use the reserved word `WEAK` in `AS WEAK ClassName` forms.

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
OVERRIDE
PARALLEL
POINTER
PRINT
PRIVATE
PROTECTED
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
WEAK
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

## 0.5.1 changes

0.5.1 adds **no reserved word**. `HOST.Exec` is a capability reached through
`IMPORT HOST.Exec AS <alias>`; `Exec`, `Run`, `Result`, `ReturnCode`, `Stdout`
and `Stderr` are identifiers on host types, not keywords.

## 0.5.2 changes

Surface shipped by bucket `ongoing/bucket-0.5.2.md`. Since
2026-09-19 (SPRINT 3) the lexer registry is generated from **this** file and
`0.6.ebnf` (`crates/bn_frontend/build.rs`), so `PROTECTED` and `OVERRIDE` are
reserved words in the shipped lexer from that build on; `PROTECTED` semantics
ship with O3, `OVERRIDE` semantics with O2.

| Change | Detail |
|---|---|
| **Added (O3)** | `PROTECTED` — third `visibility`: same class and `EXTENDS` subclasses only. |
| **Added (O2)** | `OVERRIDE` — required marker on a method that overrides an inherited instance method. |
| **Operators (S1)** | `++` / `--` — statement-only postfix tokens, sugar for `+= 1` / `-= 1`. Tokens, not reserved words. |
| **No keyword** | I1 qualified import, O4 factories. O1 downcast is deferred (its surface, `AS` vs `TRYCAST`, is decided with it). |

Toolchain note: until 2026-09-19 `build.rs` read the historical
`docs/0.5.0/` tree; it now reads the active 0.5 tree (bucket 0.5.2 SPRINT 3).
