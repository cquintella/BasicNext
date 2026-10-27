// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `a op= b` is `a = a op b` (0.6.md, "Compound assignment"): it is typed
//! exactly as the binary `a op b` stored into `a`, whatever `b` is.

use bn_frontend::lexer::lex;
use bn_frontend::parser::parse;
use bn_frontend::semantic::analyze;
use bn_source::SourceFile;

const TYPE_MISMATCH: &str = "TYPE_MISMATCH";

fn analysis_code(body: &str) -> Result<(), String> {
    let source = SourceFile::new(
        "compound.bn",
        format!("FUNCTION Start() AS VOID\n{body}END FUNCTION\n"),
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    analyze(&program)
        .map(|_| ())
        .map_err(|diagnostic| diagnostic.code.to_string())
}

/// The left operand is an alternative that is not narrowed, so `a + b` is a
/// `TYPE_MISMATCH` whether `b` is a constant, a variable or `++`.
const UNNARROWED_TARGETS: [(&str, &str); 3] = [
    (
        "constant right side",
        "LET a AS INTEGER OR NULL = NULL\na += 1\n",
    ),
    (
        "variable right side",
        "LET a AS INTEGER OR NULL = NULL\nLET n AS INTEGER = 1\na += n\n",
    ),
    (
        "increment statement",
        "LET a AS INTEGER OR NULL = NULL\na++\n",
    ),
];

/// Forms the spec makes valid, as `a = a op b`. On `HEAD`, `f += 1` was
/// rejected although `f = f + 1` runs (the same root as `a += 1`).
const VALID_FORMS: [(&str, &str); 6] = [
    (
        "narrowed alternative",
        "LET a AS INTEGER OR NULL = 5\nIF a IS INTEGER THEN\na += 1\nEND IF\nPRINT a\n",
    ),
    ("INTEGER", "LET n AS INTEGER = 1\nn += 2\nn++\nPRINT n\n"),
    (
        "FLOAT with an integer literal",
        "LET f AS FLOAT = 1.5\nf += 1\nPRINT f\n",
    ),
    (
        "STRING append",
        "LET s AS STRING = \"a\"\ns += \"b\"\nPRINT s\n",
    ),
    (
        "vector element",
        "LET v AS INTEGER[2] = [1, 2]\nv[0] += 5\nPRINT v[0]\n",
    ),
    (
        "every operator",
        "LET n AS INTEGER = 7\nn -= 1\nn *= 2\nn %= 5\nn **= 2\nPRINT n\n",
    ),
];

#[test]
fn compound_assignment_to_an_unnarrowed_alternative_is_a_type_mismatch() {
    let accepted = UNNARROWED_TARGETS
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
fn compound_assignment_valid_forms_stay_valid() {
    let rejected = VALID_FORMS
        .iter()
        .filter_map(|(class, body)| {
            analysis_code(body)
                .err()
                .map(|code| format!("{class}: {code}"))
        })
        .collect::<Vec<_>>();
    assert!(rejected.is_empty(), "rejected: {rejected:#?}");
}

#[test]
fn compound_assignment_ends_the_narrowing() {
    assert_eq!(
        analysis_code(
            "LET a AS INTEGER OR NULL = 5\nIF a IS INTEGER THEN\na += 1\nPRINT a + 1\nEND IF\n"
        ),
        Err(TYPE_MISMATCH.to_string())
    );
}
