// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::{
    ast::{DeclarationKind, Expression, ExpressionKind, Item, Literal, Program, TypeReference},
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
    if tokens.is_empty() {
        // Callers with a position use `Parser::expression_in`; this backstop only
        // guarantees the public entry point never panics on an empty slice.
        let unknown = crate::source::Position {
            source_id: crate::source::SourceId::UNKNOWN,
            revision: crate::source::Revision::UNKNOWN,
            offset: 0,
            line: 1,
            column: 1,
        };
        return Err(Diagnostic::parse_facts(
            "expected expression",
            "expression parser",
            Span {
                start: unknown,
                end: unknown,
            },
        )
        .unwrap_or_else(|_| Diagnostic {
            code: "E0100",
            message: "parser error".into(),
            span: Span {
                start: unknown,
                end: unknown,
            },
            structured: None,
        }));
    }
    let mut parser = ExpressionParser { tokens, index: 0 };
    let expression = parser.expression(0)?;
    if !parser.at_end() && !matches!(parser.peek_kind(), TokenKind::Newline | TokenKind::Eof) {
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

#[path = "parser/expressions.rs"]
mod expressions;
use expressions::ExpressionParser;

/// Value of an integer literal's text: decimal, `0x` hexadecimal, or `0b`
/// binary, with an optional leading minus.
fn integer_literal_value(text: &str) -> Option<i128> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(digits) => (true, digits),
        None => (false, text),
    };
    let magnitude = if let Some(digits) = digits.strip_prefix("0x") {
        i128::from_str_radix(digits, 16).ok()?
    } else if let Some(digits) = digits.strip_prefix("0b") {
        i128::from_str_radix(digits, 2).ok()?
    } else {
        digits.parse::<i128>().ok()?
    };
    Some(if negative { -magnitude } else { magnitude })
}

/// `constant-literal` in `0.6.ebnf` (0.6.2 C1): a minus sign applied to one
/// integer or floating-point literal is itself a literal, so `-1` folds into
/// `Literal::Integer("-1")`. Negative integers are written in decimal
/// (`-0xFF` → `"-255"`) because every downstream integer parser strips a
/// `0x`/`0b` prefix only at the start of the text. Any other shape is
/// returned unchanged.
fn negative_literal(initializer: Expression) -> Expression {
    let ExpressionKind::Unary { operator, operand } = &initializer.kind else {
        return initializer;
    };
    // Unary operators carry the token text; `text()` spells symbols by `Debug`.
    if *operator != format!("{:?}", Symbol::Minus) {
        return initializer;
    }
    let literal = match &operand.kind {
        ExpressionKind::Literal(Literal::Integer(text)) => {
            let Some(value) = integer_literal_value(text) else {
                return initializer;
            };
            Literal::Integer((-value).to_string())
        }
        ExpressionKind::Literal(Literal::Float(text)) => Literal::Float(format!("-{text}")),
        _ => return initializer,
    };
    Expression {
        kind: ExpressionKind::Literal(literal),
        span: initializer.span,
    }
}

/// 0.6.2 C2: an integer literal (optionally negative) initializing a binding
/// declared as exactly one floating-point type denotes that value as a float,
/// so `LET f AS FLOAT = 0xFF` holds `255.0`. A value not exactly representable
/// in the declared type is left as an integer literal, which semantic analysis
/// rejects as `TYPE_MISMATCH`. Any other type or initializer is unchanged.
fn float_literal_initializer(type_ref: &TypeReference, initializer: Expression) -> Expression {
    let [atom] = type_ref.alternatives.as_slice() else {
        return initializer;
    };
    if !atom.dimensions.is_empty() || !atom.parts.is_empty() {
        return initializer;
    }
    let single_precision = match atom.name.as_str() {
        "FLOAT32" => true,
        "FLOAT" | "FLOAT64" => false,
        _ => return initializer,
    };
    let initializer = negative_literal(initializer);
    let ExpressionKind::Literal(Literal::Integer(text)) = &initializer.kind else {
        return initializer;
    };
    let Some(value) = integer_literal_value(text) else {
        return initializer;
    };
    // Round-trip through the target width: equal only when no bits were lost.
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let exact = if single_precision {
        (value as f32) as i128 == value
    } else {
        (value as f64) as i128 == value
    };
    if !exact {
        return initializer;
    }
    Expression {
        kind: ExpressionKind::Literal(Literal::Float(format!("{value}.0"))),
        span: initializer.span,
    }
}
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

#[path = "parser/phase1.rs"]
mod phase1;
#[path = "parser/phase2.rs"]
mod phase2;
#[path = "parser/phase3.rs"]
mod phase3;
#[path = "parser/phase4.rs"]
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
