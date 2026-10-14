# Proposal: destructuring `LET`

**Status:** Accepted. Syntax decided by the BDFL on 2026-10-11; rules decided
2026-10-12 (DS1–DS10) and written into `language/0.6/0.6.md`, "Destructuring
`LET` and `STRUCT` literals (0.6.5)". Implementation: bucket 0.6.5 S.11.

Decisions 2026-10-12: DS1 a binding takes the type of its value (`AS`
optional); DS2 a `STRUCT` is destructured by field name; DS3 a vector needs
the exact count, a `STRUCT` any subset of fields; DS4 a run-time length that
differs stops with `INDEX_OUT_OF_BOUNDS`; DS5 objects are not destructured;
DS6 a reference taken out is retained; DS7 a `STRUCT` literal needs one
element per field; DS8 literals never make objects; DS9 a `STRUCT` literal
takes any declared destination; DS10 in 0.6 (0.6.5). The text below is the
history of the discussion; the spec governs.
**Date:** 2026-10-11.
**Target release:** 0.6.5.
**Language surface:** grammar (`let-declaration`), semantics (`0.6.md`),
fixtures. Lowering only: the backends see ordinary `LET`s.

## Motivation

Reading several values out of a vector, a `STRUCT` or an object takes one
`LET` per value today:

```basic
LET x AS INTEGER = values[0]
LET y AS INTEGER = values[1]
```

A destructuring `LET` binds them in one declaration.

## Syntax (Carlos, 2026-10-11)

Square brackets, one binding per position, and every binding with its own
type:

```basic
LET [x AS INTEGER, y AS INTEGER] = values
LET [z AS FLOAT, t AS STRING, p AS INTEGER] = myObj
```

Not this form (one type after the list, as the multi-binding `LET`):

```basic
LET [first, second] AS INTEGER = values   // refused
```

Each binding states its type, as every `LET` does (`let-declaration`
requires `AS`): no type is inferred.

Grammar sketch (to be checked against `0.6.ebnf`):

```text
let-declaration = "LET" identifier { "," identifier } "AS" local-variable-type
                  [ "=" expression { "," expression } ]
                | "LET" "[" typed-binding { "," typed-binding } "]" "=" expression ;
typed-binding   = identifier "AS" local-variable-type ;
```

`LET [` is not valid today, so no existing program changes meaning.

## Semantics (proposed)

The bindings take, left to right, the values at positions 0, 1, … of the
source, each checked as an assignment to its declared type (the rules of
assignment and `AS` conversion are unchanged). All bindings are out of scope
in the initializer, and duplicate names are a declaration error, as in the
multi-binding `LET`.

* **Vector:** position `i` is element `i`. A fixed vector whose length is
  not the number of bindings is a `TYPE_MISMATCH` at compile time.
* **`STRUCT`:** position `i` is the field declared `i`-th.
* **Object:** position `i` is the `i`-th field of the class (see open
  point 3).

## Open points (Carlos decides)

1. **Fewer bindings than values.** Is `LET [x AS INTEGER] = values` (two
   elements) valid, taking a prefix, or must the count match exactly?
2. **Dynamic vectors (`POINTER TO T[]`, `NEW T[n]`).** Their length is known
   only at run time: a count mismatch stops the program with
   `INDEX_OUT_OF_BOUNDS`, or such sources are refused statically?
3. **Objects.** Which fields count: only `PUBLIC` ones (a `PRIVATE` or
   `PROTECTED` field is not readable from outside the class), in
   declaration order, base class fields first? Or are objects excluded and
   only vectors and `STRUCT`s destructured?
4. **Field order of a `STRUCT`.** Positions follow the declaration order, so
   reordering two fields of the same type changes which value each binding
   gets with no diagnostic. Accepted as the cost of positional syntax, or
   should the compiler require the binding names to match the field names
   (`LET [name AS STRING, size AS INTEGER] = row` checks `name` and `size`
   are the fields at positions 0 and 1)?
5. **Ownership.** An object or region taken out of a source is a new strong
   reference (retained), as a copy is; confirm.

## Verification (when accepted)

Spec first (`0.6.ebnf`, `0.6.md`, `keywords.md` unchanged), then valid and
invalid fixtures (each refusal with its diagnostic), parity on both backends
(`bni` × `bnc`), and the book chapter on bindings.

## The inverse: filling a `STRUCT` in one declaration (Carlos, 2026-10-11)

Instead of one assignment per field

```basic
LET site AS Site
site.port = 10
site.name = "web"
```

a bracket literal fills the fields by position when the destination type
is declared:

```basic
LET site AS Site = [10, "web"]
```

It extends the existing rule "a vector literal takes a declared
destination" (`0.6.md`, 0.6.5): when the declared type is a `STRUCT`,
position `i` of the literal is the field declared `i`-th, each element
checked as an assignment to that field's type. Today this is a
`TYPE_MISMATCH`, so no program changes meaning.

Open points (Carlos decides):

6. **Fewer elements than fields.** The missing fields keep their declared
   default (`= expression` of the field, or the type's zero value), or the
   count must match?
7. **Objects.** An object is made by `NEW Class(arguments)` and its
   `CONSTRUCTOR`; a bracket literal would bypass the constructor. Proposed:
   `STRUCT` only, objects keep `NEW`.
8. **Field order**, as point 4: reordering two fields of the same type
   silently swaps the values.
9. **Other destinations** (an argument, a `RETURN` value, an assignment):
   the same rule as vector literals, which take any declared destination.
