// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::source::Span;

include!(concat!(env!("OUT_DIR"), "/reserved_words.rs"));

#[derive(Debug, Eq, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TokenKind {
    Identifier(String),
    Integer(String),
    Float(String),
    String(String),
    Keyword(String),
    Special(&'static str),
    Symbol(Symbol),
    Newline,
    Eof,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Symbol {
    Assign,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Plus,
    Minus,
    Star,
    Power,
    Slash,
    Percent,
    PlusAssign,
    MinusAssign,
    StarAssign,
    PowerAssign,
    SlashAssign,
    PercentAssign,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    Comma,
    Colon,
    Dot,
}

#[must_use]
pub fn is_reserved_word(text: &str) -> bool {
    RESERVED_WORDS.binary_search(&text).is_ok()
}

#[must_use]
pub fn special_float_literal(text: &str) -> Option<&'static str> {
    SPECIAL_FLOAT_LITERALS
        .binary_search(&text)
        .ok()
        .map(|index| SPECIAL_FLOAT_LITERALS[index])
}

#[must_use]
pub fn reserved_words() -> &'static [&'static str] {
    RESERVED_WORDS
}

#[must_use]
pub fn special_float_literals() -> &'static [&'static str] {
    SPECIAL_FLOAT_LITERALS
}

#[cfg(test)]
mod tests {
    use super::{RESERVED_WORDS, is_reserved_word, special_float_literal};

    #[test]
    fn generated_reserved_words_are_exact_and_case_sensitive() {
        for word in RESERVED_WORDS {
            assert!(is_reserved_word(word));
            assert!(!is_reserved_word(&word.to_ascii_lowercase()));
        }
        assert!(!is_reserved_word("INF"));
        assert!(!is_reserved_word("NAN"));
        assert!(!is_reserved_word("Error"));
        assert_eq!(special_float_literal("NAN"), Some("NAN"));
        assert_eq!(special_float_literal("INF"), Some("INF"));
        assert_eq!(special_float_literal("nan"), None);
    }
}
