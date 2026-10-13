// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Value conformance matrix (bucket 0.6.5, plan V.1): every value type
//! (`STRING`, a scalar `STRUCT`, a `STRUCT` holding a vector, a nested
//! `STRUCT`, vectors of one and two dimensions, a vector of `STRUCT`) held
//! in every place (local, parameter, return value, object field, `STRUCT`
//! field, vector element, `STATIC`) under every operation that can expose
//! sharing (copy then change the source, change the copy, store into a
//! place, outlive the frame that stored the value). The expected output
//! follows 0.6.md "Core language (0.6.5)", "Values and copies", never one
//! backend's output. `Clobber` overwrites the stack between a store and the
//! read, so a value left pointing into a dead frame reads wrong.

mod support;

#[cfg(unix)]
use support::matrix::sanitizer_failures;
use support::{
    TestDir,
    matrix::{Cell, KnownFailures, run_matrix, selected},
};

const HEADER: &str = "STRUCT Point
    x AS INTEGER = 0
END STRUCT

STRUCT Bag
    items AS INTEGER[2] = [0, 0]
END STRUCT

STRUCT Line
    a AS Point
END STRUCT

CLASS Box
    PUBLIC p AS Point
    PUBLIC v AS INTEGER[2] = [0, 0]
    PUBLIC s AS STRING = \"\"
    PUBLIC FUNCTION CONSTRUCTOR()
    END FUNCTION
END CLASS

CLASS Keep
    PUBLIC STATIC p AS Point
    PUBLIC STATIC v AS INTEGER[2] = [0, 0]
    PUBLIC STATIC s AS STRING = \"\"
END CLASS

FUNCTION Clobber(a AS INTEGER, b AS INTEGER, c AS INTEGER) AS INTEGER
    LET q AS Point
    q.x = a + b + c
    LET w AS INTEGER[2] = [a, b]
    LET t AS STRING = \"zz\" + \"zz\"
    RETURN q.x + w[0] + LEN(t)
END FUNCTION

FUNCTION MakePoint(x AS INTEGER) AS Point
    LET local AS Point
    local.x = x
    RETURN local
END FUNCTION

FUNCTION GrabPoint(p AS Point) AS INTEGER
    LET mine AS Point = p
    mine.x = 9
    RETURN p.x
END FUNCTION

FUNCTION FillBox(box AS Box) AS VOID
    LET local AS Point
    local.x = 5
    box.p = local
    LET values AS INTEGER[2] = [5, 6]
    box.v = values
    LET text AS STRING = \"ab\" + \"cd\"
    box.s = text
END FUNCTION

FUNCTION FillKeep() AS VOID
    LET local AS Point
    local.x = 5
    Keep.p = local
    LET values AS INTEGER[2] = [5, 6]
    Keep.v = values
    LET text AS STRING = \"ab\" + \"cd\"
    Keep.s = text
END FUNCTION

FUNCTION MakeBag() AS Bag
    LET bag AS Bag
    bag.items[1] = 6
    RETURN bag
END FUNCTION

FUNCTION GrabVector(v AS INTEGER[2]) AS INTEGER
    LET mine AS INTEGER[2] = v
    mine[0] = 9
    RETURN v[0]
END FUNCTION

FUNCTION MakeVector() AS INTEGER[2]
    LET local AS INTEGER[2] = [5, 6]
    RETURN local
END FUNCTION

FUNCTION MakeText() AS STRING
    LET text AS STRING = \"ab\" + \"cd\"
    RETURN text
END FUNCTION

";

/// One cell: `type / place / operation`, the body of `Case`, and the exact
/// output the rules give.
const CELLS: &[(&str, &str, &str)] = &[
    // A STRUCT of scalars.
    (
        "Point / local / copy then change the source",
        "LET a AS Point
    a.x = 1
    LET b AS Point = a
    a.x = 2
    PRINT b.x",
        "1\n",
    ),
    (
        "Point / local / change the copy",
        "LET a AS Point
    a.x = 1
    LET b AS Point = a
    b.x = 2
    PRINT a.x",
        "1\n",
    ),
    (
        "Point / parameter / the callee changes its own copy",
        "LET a AS Point
    a.x = 1
    PRINT GrabPoint(a)
    PRINT a.x",
        "1\n1\n",
    ),
    (
        "Point / return / outlives the frame",
        "LET made AS Point = MakePoint(4)
    LET junk AS INTEGER = Clobber(100, 200, 300)
    PRINT made.x",
        "4\n",
    ),
    (
        "Point / object field / copy then change the source",
        "LET box AS Box = NEW Box()
    LET a AS Point
    a.x = 1
    box.p = a
    a.x = 2
    PRINT box.p.x",
        "1\n",
    ),
    (
        "Point / object field / outlives the frame",
        "LET box AS Box = NEW Box()
    FillBox(box)
    LET junk AS INTEGER = Clobber(100, 200, 300)
    PRINT box.p.x",
        "5\n",
    ),
    (
        "Point / object field / store into the place",
        "LET box AS Box = NEW Box()
    box.p.x = 7
    PRINT box.p.x",
        "7\n",
    ),
    (
        "Point / struct field / copy then change the source",
        "LET line AS Line
    LET a AS Point
    a.x = 1
    line.a = a
    a.x = 2
    PRINT line.a.x",
        "1\n",
    ),
    (
        "Point / struct field / store into the place",
        "LET line AS Line
    line.a.x = 7
    PRINT line.a.x",
        "7\n",
    ),
    (
        "Point / struct field / change a copy of the owner",
        "LET line AS Line
    line.a.x = 1
    LET other AS Line = line
    other.a.x = 9
    PRINT line.a.x",
        "1\n",
    ),
    (
        "Point / element / copy then change the source",
        "LET items AS Point[2]
    LET a AS Point
    a.x = 1
    items[0] = a
    a.x = 2
    PRINT items[0].x",
        "1\n",
    ),
    (
        "Point / element / store into the place",
        "LET items AS Point[2]
    items[1].x = 7
    PRINT items[1].x",
        "7\n",
    ),
    (
        "Point / element / change a copy of the vector",
        "LET items AS Point[2]
    items[0].x = 1
    LET others AS Point[2] = items
    others[0].x = 9
    PRINT items[0].x",
        "1\n",
    ),
    (
        "Point / STATIC / copy then change the source",
        "LET a AS Point
    a.x = 1
    Keep.p = a
    a.x = 2
    PRINT Keep.p.x",
        "1\n",
    ),
    (
        "Point / STATIC / outlives the frame",
        "FillKeep()
    LET junk AS INTEGER = Clobber(100, 200, 300)
    PRINT Keep.p.x",
        "5\n",
    ),
    (
        "Point / STATIC / store into the place",
        "Keep.p.x = 7
    PRINT Keep.p.x",
        "7\n",
    ),
    (
        "Point / compare",
        "LET a AS Point
    LET b AS Point
    PRINT a = b
    b.x = 1
    PRINT a = b",
        "TRUE\nFALSE\n",
    ),
    (
        "Point / print",
        "LET a AS Point
    PRINT a",
        "Point\n",
    ),
    // A STRUCT holding a vector.
    (
        "Bag / local / default",
        "LET bag AS Bag
    PRINT bag.items[1]",
        "0\n",
    ),
    (
        "Bag / local / store into the place",
        "LET bag AS Bag
    bag.items[1] = 3
    PRINT bag.items[1]",
        "3\n",
    ),
    (
        "Bag / local / copy then change the source",
        "LET bag AS Bag
    bag.items[0] = 1
    LET other AS Bag = bag
    bag.items[0] = 2
    PRINT other.items[0]",
        "1\n",
    ),
    (
        "Bag / local / change the copy",
        "LET bag AS Bag
    bag.items[0] = 1
    LET other AS Bag = bag
    other.items[0] = 2
    PRINT bag.items[0]",
        "1\n",
    ),
    (
        "Bag / return / outlives the frame",
        "LET bag AS Bag = MakeBag()
    LET junk AS INTEGER = Clobber(100, 200, 300)
    PRINT bag.items[1]",
        "6\n",
    ),
    (
        "Bag / local / replaced in a loop",
        "LET bag AS Bag
    FOR i AS INTEGER = 1 TO 3
        bag = MakeBag()
        bag.items[0] = i
    END FOR
    PRINT bag.items[0]
    PRINT bag.items[1]",
        "3\n6\n",
    ),
    // A STRUCT holding a STRUCT.
    (
        "Line / local / change the copy",
        "LET line AS Line
    line.a.x = 1
    LET other AS Line = line
    other.a.x = 2
    PRINT line.a.x
    PRINT other.a.x",
        "1\n2\n",
    ),
    // A vector of one dimension.
    (
        "vector / local / copy then change the source",
        "LET a AS INTEGER[2] = [1, 2]
    LET b AS INTEGER[2] = a
    a[0] = 9
    PRINT b[0]",
        "1\n",
    ),
    (
        "vector / parameter / the callee changes its own copy",
        "LET a AS INTEGER[2] = [1, 2]
    PRINT GrabVector(a)
    PRINT a[0]",
        "1\n1\n",
    ),
    (
        "vector / return / outlives the frame",
        "LET made AS INTEGER[2] = MakeVector()
    LET junk AS INTEGER = Clobber(100, 200, 300)
    PRINT made[0]",
        "5\n",
    ),
    (
        "vector / object field / copy then change the source",
        "LET box AS Box = NEW Box()
    LET a AS INTEGER[2] = [1, 2]
    box.v = a
    a[0] = 9
    PRINT box.v[0]",
        "1\n",
    ),
    (
        "vector / object field / store into the place",
        "LET box AS Box = NEW Box()
    box.v[1] = 7
    PRINT box.v[1]",
        "7\n",
    ),
    (
        "vector / object field / outlives the frame",
        "LET box AS Box = NEW Box()
    FillBox(box)
    LET junk AS INTEGER = Clobber(100, 200, 300)
    PRINT box.v[1]",
        "6\n",
    ),
    (
        "vector / STATIC / copy then change the source",
        "LET a AS INTEGER[2] = [1, 2]
    Keep.v = a
    a[0] = 9
    PRINT Keep.v[0]",
        "1\n",
    ),
    (
        "vector / STATIC / store into the place",
        "Keep.v[1] = 7
    PRINT Keep.v[1]",
        "7\n",
    ),
    (
        "vector / STATIC / outlives the frame",
        "FillKeep()
    LET junk AS INTEGER = Clobber(100, 200, 300)
    PRINT Keep.v[1]",
        "6\n",
    ),
    (
        "vector / compare",
        "LET a AS INTEGER[2] = [1, 2]
    LET b AS INTEGER[2] = [1, 2]
    PRINT a = b
    b[1] = 3
    PRINT a = b",
        "TRUE\nFALSE\n",
    ),
    // A vector of two dimensions.
    (
        "grid / local / copy then change the source",
        "LET g AS INTEGER[2][2] = [[1, 2], [3, 4]]
    LET h AS INTEGER[2][2] = g
    g[0][0] = 9
    PRINT h[0][0]",
        "1\n",
    ),
    (
        "grid / row / copy then change the source",
        "LET g AS INTEGER[2][2] = [[1, 2], [3, 4]]
    LET row AS INTEGER[2] = g[1]
    g[1][0] = 9
    PRINT row[0]",
        "3\n",
    ),
    // STRING.
    (
        "STRING / return / outlives the frame",
        "LET made AS STRING = MakeText()
    LET junk AS INTEGER = Clobber(100, 200, 300)
    PRINT made",
        "abcd\n",
    ),
    (
        "STRING / object field / outlives the frame",
        "LET box AS Box = NEW Box()
    FillBox(box)
    LET junk AS INTEGER = Clobber(100, 200, 300)
    PRINT box.s",
        "abcd\n",
    ),
    (
        "STRING / STATIC / outlives the frame",
        "FillKeep()
    LET junk AS INTEGER = Clobber(100, 200, 300)
    PRINT Keep.s",
        "abcd\n",
    ),
    (
        "STRING / element / copy then change the source",
        "LET texts AS STRING[2]
    LET t AS STRING = \"a\"
    texts[0] = t
    t = \"b\"
    PRINT texts[0]",
        "a\n",
    ),
    (
        "STRING / holds U+0000",
        "LET nul AS STRING OR Error = CHAR(0)
    IF nul IS STRING THEN
        LET text AS STRING = \"a\" + nul + \"b\"
        PRINT LEN(text)
    END IF",
        "3\n",
    ),
    (
        "STRING / FOR EACH",
        "FOR EACH letter IN \"ab\"
        PRINT letter
    END FOR",
        "a\nb\n",
    ),
    (
        "STRING / ordering",
        "PRINT \"a\" < \"b\"
    PRINT \"Z\" < \"a\"
    PRINT \"ab\" < \"abc\"",
        "TRUE\nTRUE\nTRUE\n",
    ),
];

/// Every listed cell names the known issue or bucket activity that explains
/// it (`ongoing/known_issues.md`, `ongoing/bucket-0.6.5.md`).
const KNOWN_BNI_FAILURES: KnownFailures = &[
    // S.13 (C7, C11): FOR EACH over a STRING, without AS.
    ("STRING / FOR EACH", "E0100"),
    // S.13 (A5): STRING ordering.
    ("STRING / ordering", "TYPE_MISMATCH"),
];
const KNOWN_BNC_FAILURES: KnownFailures = &[
    // K67: a STRUCT stored into a field, an element or a STATIC is not
    // copied.
    (
        "Point / object field / copy then change the source",
        "WRONG",
    ),
    ("Point / object field / outlives the frame", "WRONG"),
    (
        "Point / struct field / copy then change the source",
        "WRONG",
    ),
    ("Point / element / copy then change the source", "WRONG"),
    ("Point / STATIC / copy then change the source", "WRONG"),
    ("Point / STATIC / outlives the frame", "WRONG"),
    // K67 / K65: stores into a field of a field are refused.
    (
        "Point / object field / store into the place",
        "TARGET_UNSUPPORTED_OP",
    ),
    (
        "Point / struct field / store into the place",
        "TARGET_UNSUPPORTED_OP",
    ),
    (
        "Point / struct field / change a copy of the owner",
        "TARGET_UNSUPPORTED_OP",
    ),
    // K67: a vector stored into an object field is not copied.
    (
        "vector / object field / copy then change the source",
        "WRONG",
    ),
    ("vector / object field / outlives the frame", "WRONG"),
    // K65: an indexed store into a STATIC vector is refused.
    (
        "vector / STATIC / store into the place",
        "TARGET_UNSUPPORTED_OP",
    ),
    // T.K10 (K67 family): a STRUCT holding a vector or a STRUCT is refused.
    ("Bag / local / default", "TARGET_UNSUPPORTED_OP"),
    (
        "Bag / local / store into the place",
        "TARGET_UNSUPPORTED_OP",
    ),
    (
        "Bag / local / copy then change the source",
        "TARGET_UNSUPPORTED_OP",
    ),
    ("Bag / local / change the copy", "TARGET_UNSUPPORTED_OP"),
    ("Bag / return / outlives the frame", "TARGET_UNSUPPORTED_OP"),
    ("Bag / local / replaced in a loop", "TARGET_UNSUPPORTED_OP"),
    ("Line / local / change the copy", "TARGET_UNSUPPORTED_OP"),
    // K68: comparing STRUCTs fails to build, comparing vectors panics in
    // bnc, PRINT writes no type name.
    ("Point / compare", "BUILD_EMISSION_FAILED"),
    ("vector / compare", "WRONG"),
    ("Point / print", "WRONG"),
    // K1 / G.S1 (A7): U+0000 in a native STRING.
    ("STRING / holds U+0000", "TARGET_UNSUPPORTED_TYPE"),
    // S.13 (C7, C11, A5), as on bni.
    ("STRING / FOR EACH", "E0100"),
    ("STRING / ordering", "TYPE_MISMATCH"),
];

fn source(body: &str) -> String {
    format!(
        "{HEADER}FUNCTION Case() AS VOID\n    {body}\nEND FUNCTION\n\nFUNCTION Start() AS VOID\n    Case()\nEND FUNCTION\n"
    )
}

/// Every cell, with its full source.
fn cells() -> Vec<Cell> {
    CELLS
        .iter()
        .map(|(name, body, expected)| Cell {
            name: (*name).to_owned(),
            source: source(body),
            expected: (*expected).to_owned(),
        })
        .collect()
}

#[test]
fn values_are_copied_where_the_rules_say_on_both_backends() {
    let directory = TestDir::new("value-matrix").expect("create test directory");
    let cells = cells();
    // `BN_VALUE_CELL=<text>` runs only the cells whose name contains it.
    let cells = selected(cells, "BN_VALUE_CELL");
    run_matrix(&cells, &directory, KNOWN_BNI_FAILURES, KNOWN_BNC_FAILURES);
}

/// Native memory safety of value copies: every cell `bnc` runs as the rules
/// define, built with `AddressSanitizer`, prints its expected output with no
/// sanitizer report (no read of a dead frame, no double free).
#[cfg(unix)]
#[test]
fn native_values_run_clean_under_address_sanitizer() {
    let cells = cells()
        .into_iter()
        .filter(|cell| {
            !KNOWN_BNC_FAILURES
                .iter()
                .any(|(known, _)| *known == cell.name)
        })
        .collect::<Vec<_>>();
    let failures = sanitizer_failures(&cells, "value-asan");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
