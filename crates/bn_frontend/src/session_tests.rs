// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for interactive submission classification and session compilation.

use std::collections::BTreeMap;

use super::*;

#[test]
fn complete_simple_statements_and_expressions() {
    assert!(matches!(
        classify_submission("LET x AS INT = 10"),
        SubmissionCompleteness::Complete { .. }
    ));
    assert!(matches!(
        classify_submission("PRINT 1 + 2"),
        SubmissionCompleteness::Complete { .. }
    ));
    assert!(matches!(
        classify_submission("1 + 2"),
        SubmissionCompleteness::Complete { .. }
    ));
    assert!(matches!(
        classify_submission(""),
        SubmissionCompleteness::Complete { .. }
    ));
    assert!(matches!(
        classify_submission("   \n   \n"),
        SubmissionCompleteness::Complete { .. }
    ));
    assert!(matches!(
        classify_submission("// full line comment\n"),
        SubmissionCompleteness::Complete { .. }
    ));
}

#[test]
fn incomplete_unclosed_string_and_block_comment() {
    let unclosed_str = classify_submission("PRINT \"hello world");
    assert!(
        matches!(unclosed_str, SubmissionCompleteness::Incomplete { open_constructs, .. } if open_constructs.contains(&"string".to_string()))
    );

    let unclosed_comment = classify_submission("/* start of comment");
    assert!(
        matches!(unclosed_comment, SubmissionCompleteness::Incomplete { open_constructs, .. } if open_constructs.contains(&"comment".to_string()))
    );

    let closed_str = classify_submission("PRINT \"hello world\"");
    assert!(matches!(
        closed_str,
        SubmissionCompleteness::Complete { .. }
    ));

    let closed_comment = classify_submission("/* comment */ 42");
    assert!(matches!(
        closed_comment,
        SubmissionCompleteness::Complete { .. }
    ));
}

#[test]
fn incomplete_open_delimiters() {
    let open_paren = classify_submission("PRINT (1 + 2");
    assert!(
        matches!(open_paren, SubmissionCompleteness::Incomplete { open_constructs, .. } if open_constructs.iter().any(|r| r.contains('(')))
    );

    let open_bracket = classify_submission("LET a = [1, 2, 3");
    assert!(
        matches!(open_bracket, SubmissionCompleteness::Incomplete { open_constructs, .. } if open_constructs.iter().any(|r| r.contains('[')))
    );
}

#[test]
fn incomplete_open_compound_blocks() {
    let open_if = classify_submission("IF x > 0 THEN\nPRINT x");
    assert!(
        matches!(open_if, SubmissionCompleteness::Incomplete { open_constructs, .. } if open_constructs.contains(&"block: IF".to_string()))
    );

    let open_while = classify_submission("WHILE count > 0\ncount = count - 1");
    assert!(
        matches!(open_while, SubmissionCompleteness::Incomplete { open_constructs, .. } if open_constructs.contains(&"block: WHILE".to_string()))
    );

    let open_for = classify_submission("FOR i = 1 TO 10\nPRINT i");
    assert!(
        matches!(open_for, SubmissionCompleteness::Incomplete { open_constructs, .. } if open_constructs.contains(&"block: FOR".to_string()))
    );

    let open_fn = classify_submission("FUNCTION Square(x AS INT) AS INT\nRETURN x * x");
    assert!(
        matches!(open_fn, SubmissionCompleteness::Incomplete { open_constructs, .. } if open_constructs.contains(&"block: FUNCTION".to_string()))
    );

    let open_class = classify_submission("CLASS Point\nLET x AS INT\nLET y AS INT");
    assert!(
        matches!(open_class, SubmissionCompleteness::Incomplete { open_constructs, .. } if open_constructs.contains(&"block: CLASS".to_string()))
    );

    let open_struct = classify_submission("STRUCT Rect\nw AS INT\nh AS INT");
    assert!(
        matches!(open_struct, SubmissionCompleteness::Incomplete { open_constructs, .. } if open_constructs.contains(&"block: STRUCT".to_string()))
    );

    let open_interface = classify_submission("INTERFACE Greeter\nFUNCTION Greet() AS STRING");
    assert!(
        matches!(open_interface, SubmissionCompleteness::Incomplete { open_constructs, .. } if open_constructs.contains(&"block: INTERFACE".to_string()))
    );
}

#[test]
fn complete_closed_compound_blocks() {
    let closed_if = classify_submission("IF x > 0 THEN\nPRINT x\nEND IF");
    assert!(matches!(closed_if, SubmissionCompleteness::Complete { .. }));

    let single_line_if = classify_submission("IF x > 0 THEN PRINT x ELSE PRINT 0");
    assert!(matches!(
        single_line_if,
        SubmissionCompleteness::Complete { .. }
    ));

    let closed_while = classify_submission("WHILE count > 0\ncount = count - 1\nEND WHILE");
    assert!(matches!(
        closed_while,
        SubmissionCompleteness::Complete { .. }
    ));

    let closed_for_end = classify_submission("FOR i = 1 TO 10\nPRINT i\nEND FOR");
    assert!(matches!(
        closed_for_end,
        SubmissionCompleteness::Complete { .. }
    ));

    let closed_fn =
        classify_submission("FUNCTION Square(x AS INT32) AS INT32\nRETURN x * x\nEND FUNCTION");
    assert!(matches!(closed_fn, SubmissionCompleteness::Complete { .. }));
}

#[test]
fn nested_blocks_and_mismatched_terminators() {
    let nested = classify_submission("IF x > 0 THEN\nWHILE y < 10\ny = y + 1");
    assert!(
        matches!(nested, SubmissionCompleteness::Incomplete { open_constructs, .. } if open_constructs.len() == 2)
    );

    let nested_closed =
        classify_submission("IF x > 0 THEN\nWHILE y < 10\ny = y + 1\nEND WHILE\nEND IF");
    assert!(matches!(
        nested_closed,
        SubmissionCompleteness::Complete { .. }
    ));

    let mismatched = classify_submission("IF x > 0 THEN\nEND WHILE");
    assert!(matches!(mismatched, SubmissionCompleteness::Invalid { .. }));

    let unexpected_end = classify_submission("END IF");
    assert!(matches!(
        unexpected_end,
        SubmissionCompleteness::Invalid { .. }
    ));

    let unmatched_paren = classify_submission("PRINT 1 + 2)");
    assert!(matches!(
        unmatched_paren,
        SubmissionCompleteness::Invalid { .. }
    ));
}

#[test]
fn incomplete_trailing_continuation() {
    let trailing_plus = classify_submission("1 +");
    assert!(
        matches!(trailing_plus, SubmissionCompleteness::Incomplete { open_constructs, .. } if open_constructs.contains(&"continuation".to_string()))
    );

    let trailing_assign = classify_submission("LET x =");
    assert!(
        matches!(trailing_assign, SubmissionCompleteness::Incomplete { open_constructs, .. } if open_constructs.contains(&"continuation".to_string()))
    );

    let trailing_comma = classify_submission("PRINT 1,");
    assert!(
        matches!(trailing_comma, SubmissionCompleteness::Incomplete { open_constructs, .. } if open_constructs.contains(&"continuation".to_string()))
    );
}

#[test]
fn session_accumulator_incremental_workflow() {
    let mut session = SessionAccumulator::new();
    assert_eq!(session.turn_count(), 0);

    // Turn 1: Empty
    let empty_res = session.compile_submission("   ").unwrap();
    assert_eq!(empty_res.kind, SubmissionKind::Empty);

    // Turn 2: Standalone expression
    let expr_res = session.compile_submission("10 + 20").unwrap();
    assert_eq!(expr_res.kind, SubmissionKind::Expression);

    // Turn 3: Variable declaration
    let stmt_res = session.compile_submission("LET x AS INT32 = 42").unwrap();
    assert_eq!(stmt_res.kind, SubmissionKind::Statement);
    let mut new_vars = BTreeMap::new();
    new_vars.insert("x".to_string(), "INT32".to_string());
    session.commit_turn("LET x AS INT32 = 42", new_vars);
    assert_eq!(session.turn_count(), 1);

    // Turn 4: Rebinding / modifying x in next turn
    let rebind_res = session.compile_submission("x = x + 1").unwrap();
    assert_eq!(rebind_res.kind, SubmissionKind::Statement);
    session.commit_turn("x = x + 1", BTreeMap::new());
    assert_eq!(session.turn_count(), 2);

    // Turn 5: Top-level function declaration
    let func_code = "FUNCTION Double(n AS INT32) AS INT32\nRETURN n * 2\nEND FUNCTION";
    let func_res = session.compile_submission(func_code).unwrap();
    assert_eq!(func_res.kind, SubmissionKind::Declaration);
    session.commit_turn(func_code, BTreeMap::new());
    assert_eq!(session.turn_count(), 3);

    // Turn 6: Invoking function using previous variable
    let call_res = session.compile_submission("Double(x)").unwrap();
    assert_eq!(call_res.kind, SubmissionKind::Expression);

    // Reconstruct canonical program
    let exported = session.export_program();
    assert!(exported.contains("FUNCTION Double(n AS INT32) AS INT32"));
    assert!(exported.contains("LET x AS INT32"));
    assert!(exported.contains("FUNCTION Start() AS VOID"));

    // Reset session
    session.reset();
    assert_eq!(session.turn_count(), 0);
    assert!(session.variables().is_empty());
}
