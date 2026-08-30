// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::{
    diagnostic::Diagnostic,
    source::{Position, SourceFile, Span},
    token::{Symbol, Token, TokenKind, is_reserved_word, special_float_literal},
};

/// Lexes a source file into tokens.
///
/// # Errors
///
/// Returns a diagnostic when the source violates a lexical rule.
pub fn lex(source: &SourceFile) -> Result<Vec<Token>, Diagnostic> {
    Lexer::new(source).run()
}

struct Lexer<'a> {
    source: &'a SourceFile,
    offset: usize,
    line: usize,
    column: usize,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a SourceFile) -> Self {
        Self {
            source,
            offset: 0,
            line: 1,
            column: 1,
            tokens: Vec::new(),
        }
    }

    fn run(mut self) -> Result<Vec<Token>, Diagnostic> {
        while let Some(character) = self.peek() {
            match character {
                ' ' | '\t' => {
                    self.advance();
                }
                '\n' => self.newline(),
                '\r' if self.peek_next() == Some('\n') => {
                    self.advance();
                    self.newline();
                }
                '\r' => return Err(self.error("bare carriage return; use LF or CRLF")),
                '/' if self.starts("//") => self.line_comment(),
                '/' if self.starts("/*") => self.block_comment()?,
                '"' => self.string()?,
                '0'..='9' => self.number()?,
                '.' if self.peek_next().is_some_and(|value| value.is_ascii_digit()) => {
                    return Err(self.error("a number cannot start with '.'"));
                }
                value if is_identifier_start(value) => self.identifier(),
                _ => self.symbol()?,
            }
        }
        if !matches!(
            self.tokens.last().map(|token| &token.kind),
            Some(TokenKind::Newline) | None
        ) {
            self.push(TokenKind::Newline, self.position());
        }
        self.push(TokenKind::Eof, self.position());
        Ok(self.tokens)
    }

    fn position(&self) -> Position {
        Position {
            offset: self.offset,
            line: self.line,
            column: self.column,
        }
    }
    fn span(&self, start: Position) -> Span {
        Span {
            start,
            end: self.position(),
        }
    }
    fn error(&self, message: impl Into<String>) -> Diagnostic {
        Diagnostic::lexical(message, self.span(self.position()))
    }
    fn peek(&self) -> Option<char> {
        self.source.text[self.offset..].chars().next()
    }
    fn peek_next(&self) -> Option<char> {
        self.source.text[self.offset..].chars().nth(1)
    }
    fn starts(&self, text: &str) -> bool {
        self.source.text[self.offset..].starts_with(text)
    }
    fn advance(&mut self) -> Option<char> {
        let value = self.peek()?;
        self.offset += value.len_utf8();
        self.column += 1;
        Some(value)
    }
    fn push(&mut self, kind: TokenKind, start: Position) {
        self.tokens.push(Token {
            kind,
            span: self.span(start),
        });
    }
    fn newline(&mut self) {
        let start = self.position();
        self.advance();
        self.push(TokenKind::Newline, start);
        self.line += 1;
        self.column = 1;
    }

    fn line_comment(&mut self) {
        while self
            .peek()
            .is_some_and(|value| value != '\n' && value != '\r')
        {
            self.advance();
        }
    }
    fn block_comment(&mut self) -> Result<(), Diagnostic> {
        let start = self.position();
        self.advance();
        self.advance();
        while !self.starts("*/") {
            match self.peek() {
                None => {
                    return Err(Diagnostic::lexical(
                        "unterminated block comment",
                        self.span(start),
                    ));
                }
                Some('/') if self.starts("/*") => {
                    return Err(self.error("nested block comments are not allowed"));
                }
                Some('\n') => self.newline(),
                Some('\r') if self.peek_next() == Some('\n') => {
                    self.advance();
                    self.newline();
                }
                Some('\r') => return Err(self.error("bare carriage return; use LF or CRLF")),
                Some(_) => {
                    self.advance();
                }
            }
        }
        self.advance();
        self.advance();
        Ok(())
    }
    fn string(&mut self) -> Result<(), Diagnostic> {
        let start = self.position();
        self.advance();
        let mut value = String::new();
        loop {
            match self.peek() {
                None => {
                    return Err(Diagnostic::lexical(
                        "unterminated string literal",
                        self.span(start),
                    ));
                }
                Some('\n' | '\r') => {
                    return Err(Diagnostic::lexical(
                        "string literals cannot contain a line break",
                        self.span(start),
                    ));
                }
                Some('"') => {
                    self.advance();
                    self.push(TokenKind::String(value), start);
                    return Ok(());
                }
                Some('\\') => {
                    self.advance();
                    match self.advance() {
                        Some('"') => value.push('"'),
                        Some('\\') => value.push('\\'),
                        _ => return Err(self.error(r#"unsupported string escape; use \" or \\"#)),
                    }
                }
                Some(character) if character.is_control() || character == '\u{7f}' => {
                    return Err(Diagnostic::lexical(
                        "string literals cannot contain control characters",
                        self.span(start),
                    ));
                }
                Some(character) => {
                    value.push(character);
                    self.advance();
                }
            }
        }
    }
    fn number(&mut self) -> Result<(), Diagnostic> {
        let start = self.position();
        while self
            .peek()
            .is_some_and(|value| value.is_ascii_alphanumeric() || value == '_' || value == '.')
        {
            self.advance();
        }
        let text = &self.source.text[start.offset..self.offset];
        let binary = text.strip_prefix("0b");
        let hexadecimal = text.strip_prefix("0x");
        let valid = binary.is_some_and(|digits| {
            !digits.is_empty() && digits.chars().all(|digit| matches!(digit, '0' | '1'))
        }) || hexadecimal.is_some_and(|digits| {
            !digits.is_empty() && digits.chars().all(|digit| digit.is_ascii_hexdigit())
        }) || (is_decimal(text) && !text.ends_with('.'));
        if !valid {
            return Err(Diagnostic::lexical(
                format!("malformed numeric literal '{text}'"),
                self.span(start),
            ));
        }
        self.push(
            if text.contains('.') {
                TokenKind::Float(text.into())
            } else {
                TokenKind::Integer(text.into())
            },
            start,
        );
        Ok(())
    }
    fn identifier(&mut self) {
        let start = self.position();
        while self.peek().is_some_and(is_identifier_continue) {
            self.advance();
        }
        let text = &self.source.text[start.offset..self.offset];
        let kind = if let Some(literal) = special_float_literal(text) {
            TokenKind::Special(literal)
        } else if is_reserved_word(text) {
            TokenKind::Keyword(text.into())
        } else {
            TokenKind::Identifier(text.into())
        };
        self.push(kind, start);
    }
    fn symbol(&mut self) -> Result<(), Diagnostic> {
        let start = self.position();
        let (symbol, width) = if self.starts("**=") {
            (Symbol::PowerAssign, 3)
        } else if self.starts("**") {
            (Symbol::Power, 2)
        } else if self.starts("+=") {
            (Symbol::PlusAssign, 2)
        } else if self.starts("-=") {
            (Symbol::MinusAssign, 2)
        } else if self.starts("*=") {
            (Symbol::StarAssign, 2)
        } else if self.starts("/=") {
            (Symbol::SlashAssign, 2)
        } else if self.starts("%=") {
            (Symbol::PercentAssign, 2)
        } else if self.starts("<=") {
            (Symbol::LessEqual, 2)
        } else if self.starts(">=") {
            (Symbol::GreaterEqual, 2)
        } else if self.starts("<>") {
            (Symbol::NotEqual, 2)
        } else {
            match self.peek() {
                Some('=') => (Symbol::Assign, 1),
                Some('+') => (Symbol::Plus, 1),
                Some('-') => (Symbol::Minus, 1),
                Some('*') => (Symbol::Star, 1),
                Some('/') => (Symbol::Slash, 1),
                Some('%') => (Symbol::Percent, 1),
                Some('<') => (Symbol::Less, 1),
                Some('>') => (Symbol::Greater, 1),
                Some('(') => (Symbol::LeftParen, 1),
                Some(')') => (Symbol::RightParen, 1),
                Some('[') => (Symbol::LeftBracket, 1),
                Some(']') => (Symbol::RightBracket, 1),
                Some(',') => (Symbol::Comma, 1),
                Some(':') => (Symbol::Colon, 1),
                Some('.') => (Symbol::Dot, 1),
                Some('^') => {
                    return Err(self.error("'^' is not an operator; use '**' for exponentiation"));
                }
                Some(value) => return Err(self.error(format!("unexpected character '{value}'"))),
                None => unreachable!(),
            }
        };
        for _ in 0..width {
            self.advance();
        }
        self.push(TokenKind::Symbol(symbol), start);
        Ok(())
    }
}

fn is_identifier_start(value: char) -> bool {
    value.is_ascii_alphabetic() || value == '_'
}
fn is_identifier_continue(value: char) -> bool {
    is_identifier_start(value) || value.is_ascii_digit()
}
fn is_decimal(text: &str) -> bool {
    let mut dots = 0;
    let mut digits = 0;
    for value in text.chars() {
        if value == '.' {
            dots += 1;
        } else if value.is_ascii_digit() {
            digits += 1;
        } else {
            return false;
        }
    }
    digits > 0 && dots <= 1
}
