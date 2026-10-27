// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A loop owns its variable and its body is a block (0.6.md, "Scopes and
//! bindings"): neither is visible after the loop, and a name is a duplicate
//! only of a name in the same block or an enclosing one, never of a
//! sibling block's name (BDFL decision, 2026-10-26).

use bn_frontend::lexer::lex;
use bn_frontend::parser::parse;
use bn_frontend::semantic::analyze;
use bn_source::SourceFile;
use std::fs;

const DUPLICATE_NAME: &str = "DUPLICATE_NAME";
const NAME_NOT_FOUND: &str = "NAME_NOT_FOUND";

const BODY_BINDING_AFTER_LOOP_FIXTURES: [&str; 3] = [
    "tests/grammar/invalid/for-body-binding-after-end-for.bn",
    "tests/grammar/invalid/for-each-body-binding-after-end-for.bn",
    "tests/grammar/invalid/repeat-body-binding-after-end-repeat.bn",
];

/// The `UNTIL` condition belongs to the `REPEAT` block and sees the body's
/// bindings (BDFL decision, 2026-10-26).
const UNTIL_READS_BODY_BINDING: &str = "LET count AS INTEGER = 0\nREPEAT\nLET done AS BOOLEAN = count > 0\ncount += 1\nUNTIL done\nEND REPEAT\nPRINT count\n";

const SIBLING_LOOPS_FIXTURE: &str = "tests/grammar/valid/for-variable-sibling-loops.bn";

const ENCLOSING_REDECLARATION_FIXTURES: [&str; 3] = [
    "tests/grammar/invalid/for-variable-shadows-enclosing.bn",
    "tests/grammar/invalid/for-variable-redeclares-local.bn",
    "tests/grammar/invalid/for-each-variable-shadows-parameter.bn",
];

/// One equivalence class each, so a failing class never hides another.
const SIBLING_CLASSES: [(&str, &str); 8] = [
    (
        "LET in sibling counted FOR bodies",
        "FOR j AS INTEGER = 1 TO 2\nLET y AS INTEGER = j\nPRINT y\nEND FOR\nFOR k AS INTEGER = 1 TO 2\nLET y AS INTEGER = k\nPRINT y\nEND FOR\n",
    ),
    (
        "LET in sibling FOR EACH bodies",
        "FOR EACH c AS STRING IN \"a\"\nLET y AS STRING = c\nPRINT y\nEND FOR\nFOR EACH d AS STRING IN \"b\"\nLET y AS STRING = d\nPRINT y\nEND FOR\n",
    ),
    (
        "LET in sibling REPEAT bodies",
        "LET n AS INTEGER = 0\nREPEAT\nLET y AS INTEGER = 1\nn += y\nUNTIL n > 0\nEND REPEAT\nREPEAT\nLET y AS INTEGER = 2\nn += y\nUNTIL n > 2\nEND REPEAT\nPRINT n\n",
    ),
    (
        "counted FOR with AS",
        "FOR j AS INTEGER = 1 TO 2\nPRINT j\nEND FOR\nFOR j AS INTEGER = 1 TO 2\nPRINT j\nEND FOR\n",
    ),
    (
        "counted FOR without AS",
        "FOR j = 1 TO 2\nPRINT j\nEND FOR\nFOR j = 1 TO 2\nPRINT j\nEND FOR\n",
    ),
    (
        "FOR EACH",
        "FOR EACH c AS STRING IN \"ab\"\nPRINT c\nEND FOR\nFOR EACH c AS STRING IN \"cd\"\nPRINT c\nEND FOR\n",
    ),
    (
        "FOR inside IF, then FOR after END IF",
        "IF TRUE THEN\nFOR j AS INTEGER = 1 TO 2\nPRINT j\nEND FOR\nEND IF\nFOR j AS INTEGER = 1 TO 2\nPRINT j\nEND FOR\n",
    ),
    (
        "LET after END FOR",
        "FOR j AS INTEGER = 1 TO 2\nPRINT j\nEND FOR\nLET j AS INTEGER = 3\nPRINT j\n",
    ),
];

fn analyze_source(name: &str, text: String) -> Result<(), String> {
    let source = SourceFile::new(name, text);
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    analyze(&program)
        .map(|_| ())
        .map_err(|diagnostic| diagnostic.code.to_string())
}

fn in_start(body: &str) -> String {
    format!("FUNCTION Start() AS VOID\n{body}END FUNCTION\n")
}

#[test]
fn sibling_loops_may_reuse_their_variable_name() {
    let rejected = SIBLING_CLASSES
        .iter()
        .filter_map(|(class, body)| {
            analyze_source("sibling-loops.bn", in_start(body))
                .err()
                .map(|code| format!("{class}: {code}"))
        })
        .collect::<Vec<_>>();
    assert!(rejected.is_empty(), "rejected classes: {rejected:#?}");
}

#[test]
fn sibling_loops_fixture_passes_semantic_analysis() {
    let text = fs::read_to_string(SIBLING_LOOPS_FIXTURE).expect("read fixture");
    assert_eq!(analyze_source(SIBLING_LOOPS_FIXTURE, text), Ok(()));
}

#[test]
fn loop_body_bindings_are_not_visible_after_the_loop() {
    for path in BODY_BINDING_AFTER_LOOP_FIXTURES {
        let text = fs::read_to_string(path).expect("read fixture");
        assert_eq!(
            analyze_source(path, text),
            Err(NAME_NOT_FOUND.to_string()),
            "{path}"
        );
    }
}

#[test]
fn until_condition_reads_a_binding_of_the_repeat_body() {
    assert_eq!(
        analyze_source("until.bn", in_start(UNTIL_READS_BODY_BINDING)),
        Ok(())
    );
}

#[test]
fn loop_variable_redeclaring_an_enclosing_name_is_a_duplicate() {
    for path in ENCLOSING_REDECLARATION_FIXTURES {
        let text = fs::read_to_string(path).expect("read fixture");
        assert_eq!(
            analyze_source(path, text),
            Err(DUPLICATE_NAME.to_string()),
            "{path}"
        );
    }
}
