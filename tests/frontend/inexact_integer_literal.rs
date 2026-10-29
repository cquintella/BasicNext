// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An integer constant takes a floating-point operand's type only when the
//! value is exact (0.6.md, "Numeric operands"; BDFL decisions, 2026-10-26,
//! A.5d, and 2026-10-28). `2^53 + 1` is not a `FLOAT`. `AS FLOAT` still rounds.

use bn_frontend::lexer::lex;
use bn_frontend::parser::parse;
use bn_frontend::semantic::analyze;
use bn_source::SourceFile;

const TYPE_MISMATCH: &str = "TYPE_MISMATCH";

fn analysis_code(body: &str) -> Result<(), String> {
    let source = SourceFile::new(
        "inexact.bn",
        format!("FUNCTION Start() AS VOID\n{body}END FUNCTION\n"),
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    analyze(&program)
        .map(|_| ())
        .map_err(|diagnostic| diagnostic.code.to_string())
}

/// `2^53 + 1` and `2^24 + 1` are the first integers past an exact power of two.
const INEXACT: [(&str, &str); 9] = [
    (
        "FLOAT times 2^53 + 1",
        "LET f AS FLOAT = 1.0\nPRINT f * 9007199254740993\n",
    ),
    (
        "2^53 + 1 times FLOAT",
        "LET f AS FLOAT = 1.0\nPRINT 9007199254740993 * f\n",
    ),
    (
        "FLOAT32 times 2^24 + 1",
        "LET f AS FLOAT32 = 1.0\nPRINT f * 16777217\n",
    ),
    (
        "comparison",
        "LET f AS FLOAT = 1.0\nPRINT f = 9007199254740993\n",
    ),
    (
        "division",
        "LET f AS FLOAT = 1.0\nPRINT f / 9007199254740993\n",
    ),
    ("float literal", "PRINT 3.5 * 9007199254740993\n"),
    (
        "negated literal",
        "LET f AS FLOAT = 1.0\nPRINT f * -9007199254740993\n",
    ),
    (
        "hexadecimal past 2^53",
        "LET f AS FLOAT = 1.0\nPRINT f * 0x20000000000001\n",
    ),
    (
        "constant expression past 2^53",
        "LET f AS FLOAT = 1.0\nPRINT f * (9007199254740993 * 1)\n",
    ),
];

/// The same forms with the last exact integer, and the conversions this
/// decision does not change.
const EXACT_OR_EXPLICIT: [(&str, &str); 8] = [
    ("FLOAT times 2", "LET f AS FLOAT = 1.5\nPRINT f * 2\n"),
    (
        "FLOAT times 2^53",
        "LET f AS FLOAT = 1.0\nPRINT f * 9007199254740992\n",
    ),
    (
        "FLOAT32 times 2^24",
        "LET f AS FLOAT32 = 1.0\nPRINT f * 16777216\n",
    ),
    ("integer literal times float literal", "PRINT 2 * 3.5\n"),
    (
        "comparison with an exact literal",
        "LET f AS FLOAT = 1.0\nPRINT f < 2\n",
    ),
    (
        "explicit nearest conversion",
        "LET f AS FLOAT = 9007199254740993 AS FLOAT\nPRINT f\n",
    ),
    (
        "integer constant expression plus float literal",
        "PRINT 2 * 3 + 1.5\n",
    ),
    (
        "negative constant exponent stays the power rule",
        "LET n AS INT64 = 9007199254740993\nPRINT n ** -1\n",
    ),
];

#[test]
fn an_inexact_integer_literal_with_a_float_is_a_type_mismatch() {
    let accepted = INEXACT
        .iter()
        .filter_map(|(class, body)| {
            let result = analysis_code(body);
            (result != Err(TYPE_MISMATCH.to_string())).then(|| format!("{class}: {result:?}"))
        })
        .collect::<Vec<_>>();
    assert!(
        accepted.is_empty(),
        "accepted or misreported: {accepted:#?}"
    );
}

#[test]
fn an_exact_integer_literal_still_takes_the_float_type() {
    let rejected = EXACT_OR_EXPLICIT
        .iter()
        .filter_map(|(class, body)| {
            analysis_code(body)
                .err()
                .map(|code| format!("{class}: {code}"))
        })
        .collect::<Vec<_>>();
    assert!(rejected.is_empty(), "rejected: {rejected:#?}");
}
