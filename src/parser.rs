// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::{
    ast::{DeclarationKind, Expression, ExpressionKind, Item, Literal, Program},
    diagnostic::Diagnostic,
    source::Span,
    token::{Symbol, Token, TokenKind},
};

/// Parses the top-level structure and compound block terminators of a BN source file.
///
/// # Errors
///
/// Returns a diagnostic when declarations or block terminators are malformed.
pub fn parse(tokens: &[Token]) -> Result<Program, Diagnostic> {
    Parser {
        tokens,
        index: 0,
        source_name: None,
    }
    .program()
}

/// Parses tokens while preserving the originating source name in the syntax AST.
///
/// # Errors
///
/// Returns a diagnostic when the token sequence does not follow BN grammar.
pub fn parse_named(
    tokens: &[Token],
    source_name: impl Into<String>,
) -> Result<Program, Diagnostic> {
    Parser {
        tokens,
        index: 0,
        source_name: Some(source_name.into()),
    }
    .program()
}

/// Parses one expression token sequence, terminated by `NEWLINE` or `EOF`.
///
/// # Errors
///
/// Returns a diagnostic when the expression does not follow BN precedence rules.
pub fn parse_expression(tokens: &[Token]) -> Result<Expression, Diagnostic> {
    let mut parser = ExpressionParser { tokens, index: 0 };
    let expression = parser.expression(0)?;
    if !parser.at_end() && !matches!(parser.peek().kind, TokenKind::Newline | TokenKind::Eof) {
        return Err(parser.error("unexpected token after expression"));
    }
    Ok(expression)
}

fn require_host_capability_name(name: &str) -> Result<(), &'static str> {
    if name
        .chars()
        .next()
        .is_some_and(|letter| letter.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err("host capability names after HOST. must start with a capital letter")
    }
}

mod expressions;
use expressions::ExpressionParser;
fn text(token: &Token) -> String {
    match &token.kind {
        TokenKind::Keyword(value) | TokenKind::Identifier(value) => value.clone(),
        TokenKind::Integer(value) | TokenKind::Float(value) | TokenKind::String(value) => {
            value.clone()
        }
        TokenKind::Special(value) => (*value).into(),
        TokenKind::Symbol(symbol) => format!("{symbol:?}"),
        _ => String::new(),
    }
}

fn expression_atom(token: &Token) -> ExpressionKind {
    match &token.kind {
        TokenKind::Identifier(name) => ExpressionKind::Name { name: name.clone() },
        TokenKind::Integer(value) => ExpressionKind::Literal(Literal::Integer(value.clone())),
        TokenKind::Float(value) => ExpressionKind::Literal(Literal::Float(value.clone())),
        TokenKind::String(value) => ExpressionKind::Literal(Literal::String(value.clone())),
        TokenKind::Special(value) => ExpressionKind::Literal(Literal::Special((*value).into())),
        TokenKind::Keyword(word) if word == "TRUE" => {
            ExpressionKind::Literal(Literal::Boolean(true))
        }
        TokenKind::Keyword(word) if word == "FALSE" => {
            ExpressionKind::Literal(Literal::Boolean(false))
        }
        TokenKind::Keyword(word) if word == "NULL" => ExpressionKind::Literal(Literal::Null),
        TokenKind::Keyword(word) if word == "NA" => ExpressionKind::Literal(Literal::NotAvailable),
        TokenKind::Keyword(word) if word == "EOF" => ExpressionKind::Literal(Literal::EndOfFile),
        TokenKind::Keyword(word) if word == "SELF" => ExpressionKind::Name { name: word.clone() },
        TokenKind::Keyword(word) if word == "SUPER" => ExpressionKind::Super,
        TokenKind::Keyword(word) => ExpressionKind::Literal(Literal::TypeName(word.clone())),
        _ => ExpressionKind::Literal(Literal::String(text(token))),
    }
}

struct Parser<'a> {
    tokens: &'a [Token],
    index: usize,
    source_name: Option<String>,
}

enum BlockTerm {
    End(crate::source::Position),
    Else,
    Until(Expression),
}

mod phase1;
mod phase2;
mod phase3;
mod phase4;

impl DeclarationKind {
    fn end_word(self) -> &'static str {
        match self {
            Self::Function => "FUNCTION",
            Self::Class => "CLASS",
            Self::Struct => "STRUCT",
            Self::Interface => "INTERFACE",
        }
    }
}
