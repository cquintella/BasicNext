// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Alternatives narrow only by `IS`, as Swift optionals (0.6.md, "`IS`
//! narrowing"; BDFL decision, 2026-10-26). A `LET` initializer or an
//! assignment never narrows, and an assignment ends a narrowing, also when
//! it happens inside a nested block. Using a binding that is not narrowed as
//! the member type is a static `TYPE_MISMATCH`, never a run-time failure or
//! a wrong value.

use bn_frontend::lexer::lex;
use bn_frontend::parser::parse;
use bn_frontend::semantic::analyze;
use bn_source::SourceFile;

const TYPE_MISMATCH: &str = "TYPE_MISMATCH";

/// `a` is narrowed to `INTEGER`; `{block}` assigns `NULL` inside a nested
/// block; `a + 1` follows that block, still inside the narrowing `IF`.
const NARROWED_PROGRAM: &str = "FUNCTION Start() AS VOID\nLET a AS INTEGER OR NULL = 5\nLET w AS INTEGER = 0\nIF a IS INTEGER THEN\n{block}PRINT a + 1\nEND IF\nEND FUNCTION\n";

/// One equivalence class per block kind, so one class never hides another.
const NESTED_ASSIGNMENTS: [(&str, &str); 5] = [
    ("IF", "IF w = 0 THEN\na = NULL\nEND IF\n"),
    ("WHILE", "WHILE w < 1\na = NULL\nw += 1\nEND WHILE\n"),
    (
        "REPEAT",
        "REPEAT\na = NULL\nw += 1\nUNTIL w > 0\nEND REPEAT\n",
    ),
    (
        "counted FOR",
        "FOR i AS INTEGER = 1 TO 2\na = NULL\nEND FOR\n",
    ),
    (
        "FOR EACH",
        "FOR EACH c AS STRING IN \"ab\"\na = NULL\nEND FOR\n",
    ),
];

fn analysis_code(text: String) -> Result<(), String> {
    let source = SourceFile::new("narrowing.bn", text);
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    analyze(&program)
        .map(|_| ())
        .map_err(|diagnostic| diagnostic.code.to_string())
}

#[test]
fn assignment_in_a_nested_block_ends_the_outer_narrowing() {
    let unsound = NESTED_ASSIGNMENTS
        .iter()
        .filter_map(|(block_kind, block)| {
            let result = analysis_code(NARROWED_PROGRAM.replace("{block}", block));
            (result != Err(TYPE_MISMATCH.to_string())).then(|| format!("{block_kind}: {result:?}"))
        })
        .collect::<Vec<_>>();
    assert!(unsound.is_empty(), "accepted or misreported: {unsound:#?}");
}

fn in_start(body: &str) -> String {
    format!("FUNCTION Start() AS VOID\n{body}END FUNCTION\n")
}

/// Swift rejects each of these with "value of optional type must be
/// unwrapped"; one class per way a narrowing could wrongly appear.
const NOT_NARROWED: [(&str, &str); 6] = [
    (
        "LET initializer",
        "LET a AS INTEGER OR NULL = 5\nPRINT a + 1\n",
    ),
    (
        "assignment",
        "LET a AS INTEGER OR NULL = NULL\na = 1\nPRINT a + 1\n",
    ),
    (
        "assignment inside an IS branch",
        "LET a AS INTEGER OR NULL = NULL\nIF a IS INTEGER THEN\na = 7\nPRINT a + 1\nEND IF\n",
    ),
    (
        "assignment in a REPEAT body",
        "LET a AS INTEGER OR NULL = NULL\nREPEAT\na = 1\nUNTIL TRUE\nEND REPEAT\nPRINT a + 1\n",
    ),
    (
        "assignment in a FOR body",
        "LET a AS INTEGER OR NULL = NULL\nFOR i AS INTEGER = 1 TO 3\na = i\nEND FOR\nPRINT a + 1\n",
    ),
    (
        "assignment in a FOR body that runs zero times",
        "LET a AS INTEGER OR NULL = NULL\nFOR i AS INTEGER = 1 TO 0\na = 1\nEND FOR\nPRINT a + 1\n",
    ),
];

/// `IS` narrowing that no assignment ends must survive nested blocks.
const STILL_NARROWED: [(&str, &str); 4] = [
    (
        "IS branch, through a FOR that does not assign",
        "LET a AS INTEGER OR NULL = 5\nIF a IS INTEGER THEN\nFOR i AS INTEGER = 1 TO 2\nPRINT a + i\nEND FOR\nPRINT a + 1\nEND IF\n",
    ),
    (
        "returning branch (guard), then a WHILE that does not assign",
        "LET a AS INTEGER OR NULL = 5\nLET w AS INTEGER = 0\nIF a IS NULL THEN\nRETURN\nEND IF\nWHILE w < 2\nw += 1\nEND WHILE\nPRINT a + w\n",
    ),
    (
        "WHILE IS body, before the assignment that ends it",
        "LET a AS INTEGER OR NULL = 5\nWHILE a IS INTEGER\nPRINT a + 1\na = NULL\nEND WHILE\n",
    ),
    (
        "IS branch, through a REPEAT that does not assign",
        "LET a AS INTEGER OR NULL = 5\nLET n AS INTEGER = 0\nIF a IS INTEGER THEN\nREPEAT\nn += a\nUNTIL n > 0\nEND REPEAT\nPRINT a + n\nEND IF\n",
    ),
];

#[test]
fn neither_initializer_nor_assignment_narrows() {
    let accepted = NOT_NARROWED
        .iter()
        .filter_map(|(class, body)| {
            let result = analysis_code(in_start(body));
            (result != Err(TYPE_MISMATCH.to_string())).then(|| format!("{class}: {result:?}"))
        })
        .collect::<Vec<_>>();
    assert!(
        accepted.is_empty(),
        "accepted or misreported: {accepted:#?}"
    );
}

#[test]
fn is_narrowing_survives_nested_blocks_that_do_not_assign() {
    let rejected = STILL_NARROWED
        .iter()
        .filter_map(|(class, body)| {
            analysis_code(in_start(body))
                .err()
                .map(|code| format!("{class}: {code}"))
        })
        .collect::<Vec<_>>();
    assert!(rejected.is_empty(), "rejected: {rejected:#?}");
}

/// The value of an assignment reads the target before the write, so it
/// still sees the narrowing; the narrowing ends only after the write (0.6.md,
/// "`IS` narrowing": "from that point").
const VALUE_READS_NARROWED_TARGET: &str =
    "LET a AS INTEGER OR NULL = 5\nIF a IS INTEGER THEN\na = a + 1\nEND IF\nPRINT a\n";

const NARROWING_ENDS_AFTER_SELF_ASSIGNMENT: &str =
    "LET a AS INTEGER OR NULL = 5\nIF a IS INTEGER THEN\na = a + 1\nPRINT a + 1\nEND IF\n";

#[test]
fn assignment_value_reads_the_narrowed_target() {
    assert_eq!(analysis_code(in_start(VALUE_READS_NARROWED_TARGET)), Ok(()));
    assert_eq!(
        analysis_code(in_start(NARROWING_ENDS_AFTER_SELF_ASSIGNMENT)),
        Err(TYPE_MISMATCH.to_string())
    );
}
