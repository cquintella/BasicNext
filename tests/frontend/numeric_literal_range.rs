// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Numeric literals out of range (0.6.md, C1, "Integer constants without a
//! destination", "Floating-point literals out of range"; bucket 0.6.11 S5.1
//! to S5.3): a `TYPE_MISMATCH` that names the value and the range.

use bn_frontend::lexer::lex;
use bn_frontend::parser::parse;
use bn_frontend::semantic::analyze;
use bn_source::SourceFile;
use std::fs;

const TYPE_MISMATCH: &str = "TYPE_MISMATCH";
const INT32_RANGE: &str = "INT32 from -2147483648 to 2147483647";
const FLOAT32_RANGE: &str = "FLOAT32 from -3.4028234663852886e38 to 3.4028234663852886e38";
const FLOAT64_RANGE: &str = "FLOAT64 from -1.7976931348623157e308 to 1.7976931348623157e308";

fn mismatch(source: &SourceFile) -> String {
    let tokens = lex(source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let diagnostic = analyze(&program).expect_err("out of range");
    assert_eq!(diagnostic.code, TYPE_MISMATCH, "{}", diagnostic.message);
    diagnostic.message.to_string()
}

fn body_mismatch(body: &str) -> String {
    mismatch(&SourceFile::new(
        "range.bn",
        format!("FUNCTION Start() AS VOID\n{body}END FUNCTION\n"),
    ))
}

fn analyzes(body: &str) -> bool {
    let source = SourceFile::new(
        "range.bn",
        format!("FUNCTION Start() AS VOID\n{body}END FUNCTION\n"),
    );
    let tokens = lex(&source).expect("lex source");
    analyze(&parse(&tokens).expect("parse source")).is_ok()
}

#[test]
fn out_of_range_literals_name_the_value_and_the_range() {
    let cases = [
        (
            "    LET x AS INT32 = 3000000000\n",
            INT32_RANGE,
            "3000000000",
        ),
        ("    LET b AS BYTE = 300\n", "BYTE from 0 to 255", "300"),
        (
            "    LET h AS UINT16 = -0x10\n",
            "UINT16 from 0 to 65535",
            "-16",
        ),
        (
            "    LET y AS INT32 = 2147483647 + 1\n",
            INT32_RANGE,
            "2147483648",
        ),
        ("    PRINT -3000000000\n", INT32_RANGE, "-3000000000"),
        ("    PRINT NOT 3000000000\n", INT32_RANGE, "3000000000"),
        ("    PRINT 3000000000 SHL 1\n", INT32_RANGE, "3000000000"),
        ("    PRINT TYPEOF(3000000000)\n", INT32_RANGE, "3000000000"),
        ("    PRINT [3000000000, 1]\n", INT32_RANGE, "3000000000"),
        ("    PRINT 3000000000 DIV 2\n", INT32_RANGE, "3000000000"),
        ("    PRINT 3000000000 AND 1\n", INT32_RANGE, "3000000000"),
        ("    PRINT 3000000000 DIV 0\n", INT32_RANGE, "3000000000"),
        (
            "    FOR i AS BYTE = 0 TO 300\n        PRINT i\n    END FOR\n",
            "BYTE from 0 to 255",
            "300",
        ),
        ("    PRINT 3000000000 / 2\n", INT32_RANGE, "3000000000"),
        (
            "    PRINT 9007199254740993 ** -1\n",
            INT32_RANGE,
            "9007199254740993",
        ),
        (
            "    LET z AS INT32 = 3000000000 DIV 1\n",
            INT32_RANGE,
            "3000000000",
        ),
    ];
    for (body, range, value) in cases {
        let message = body_mismatch(body);
        assert!(
            message.starts_with(&format!("{range}, {value}, ")),
            "{body}: {message}"
        );
    }
}

#[test]
fn literal_fixtures_are_type_mismatches() {
    let cases = [
        ("integer-literal-without-destination.bn", INT32_RANGE),
        ("integer-literal-comparison-out-of-range.bn", INT32_RANGE),
        ("float-literal-above-float64.bn", FLOAT64_RANGE),
        ("float32-literal-out-of-range.bn", FLOAT32_RANGE),
    ];
    for (name, range) in cases {
        let path = format!("tests/grammar/invalid/{name}");
        let source = SourceFile::new(path.clone(), fs::read_to_string(&path).expect("read"));
        let message = mismatch(&source);
        assert!(
            message.starts_with(&format!("{range}, ")),
            "{name}: {message}"
        );
    }
}

#[test]
fn constants_that_fit_or_have_a_destination_are_accepted() {
    for body in [
        "    PRINT 2147483647, -2147483648\n",
        "    PRINT 2147483647 + 1\n",
        "    PRINT 10 ** 30\n",
        "    LET x AS INT64 = 2 ** 40\n    PRINT x\n",
        "    PRINT 3000000000 AS INT64\n",
        "    LET q AS INT64 = 2 ** 70 DIV 2 ** 10\n    PRINT q\n",
        "    LET h AS INT32 = 3000000000 DIV 2\n    PRINT h\n",
        "    PRINT 2147483647 * 2 / 2\n",
        "    PRINT 3000000000.0 / 2, 2 * 3 + 1.5\n",
        "    LET v AS INT64[2] = [3000000000, 1]\n    PRINT v[0]\n",
        "    LET n AS INT64 = 5\n    PRINT n + 3000000000\n",
        "    LET f AS FLOAT32 = 340282340000000000000000000000000000000.0\n    PRINT f\n",
        "    LET g AS FLOAT64 = 1.0\n    PRINT g\n",
    ] {
        assert!(analyzes(body), "{body}");
    }
}
