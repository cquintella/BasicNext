// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unary `-` and `NOT` are looser than `**` and tighter than `*`
//! (`0.6.ebnf` `unary-expression`, 0.6.md "Operators"; BDFL decision,
//! 2026-10-26: arithmetic follows mathematics, `-2 ** 2` is `-4`).

use bn_frontend::ast::{Expression, ExpressionKind, Literal};
use bn_frontend::lexer::lex;
use bn_frontend::parser::parse_expression;
use bn_source::SourceFile;

/// The tree as a fully parenthesized string, operators by their AST name.
fn shape(expression: &Expression) -> String {
    match &expression.kind {
        ExpressionKind::Unary { operator, operand } => format!("({operator} {})", shape(operand)),
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } => format!("({} {operator} {})", shape(left), shape(right)),
        ExpressionKind::Literal(Literal::Integer(value)) => value.clone(),
        ExpressionKind::Name { name } => name.clone(),
        other => format!("{other:?}"),
    }
}

fn parsed(text: &str) -> String {
    let source = SourceFile::new("precedence.bn", format!("{text}\n"));
    let tokens = lex(&source).expect("lex expression");
    shape(&parse_expression(&tokens).expect("parse expression"))
}

/// Source, and its tree under the grammar.
const CASES: [(&str, &str); 6] = [
    ("-2 ** 2", "(Minus (2 Power 2))"),
    ("-x ** 2", "(Minus (x Power 2))"),
    ("NOT a ** b", "(NOT (a Power b))"),
    ("2 ** -1", "(2 Power (Minus 1))"),
    ("-2 ** 3 ** 2", "(Minus (2 Power (3 Power 2)))"),
    ("-2 * 3", "((Minus 2) Star 3)"),
];

#[test]
fn unary_operators_are_looser_than_power_and_tighter_than_star() {
    let wrong = CASES
        .iter()
        .filter_map(|(source, expected)| {
            let actual = parsed(source);
            (actual != *expected).then(|| format!("{source}: {actual} != {expected}"))
        })
        .collect::<Vec<_>>();
    assert!(wrong.is_empty(), "{wrong:#?}");
}
