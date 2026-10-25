// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Interactive session infrastructure for the Basic Next frontend:
//! submission classification (`classify_submission`), token streaming for syntax
//! highlighting, and incremental symbol accumulation (`SessionAccumulator`)
//! compiling top-level statements and expressions into validated BN IR (W1–W5).

use bn_diag::Diagnostic;
use bn_source::{SourceFile, Span};

use crate::{
    lexer::lex,
    parser::{parse, parse_expression},
    token::{Symbol, Token, TokenKind, is_reserved_word},
};

/// Category of token produced during session classification for syntax highlighting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClassifiedTokenKind {
    Keyword,
    Identifier,
    Number,
    String,
    Comment,
    Symbol,
    Operator,
    Special,
    Newline,
    Whitespace,
    Other,
}

/// Token with byte offset, line, column, and classified category for editors/REPLs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassifiedToken {
    pub kind: ClassifiedTokenKind,
    pub text: String,
    pub offset: usize,
    pub length: usize,
    pub line: usize,
    pub column: usize,
}

/// Result of evaluating the completeness of a submission buffer in the REPL.
#[derive(Debug)]
pub enum SubmissionCompleteness {
    /// The submission is syntactically complete and ready for execution.
    Complete { tokens: Vec<ClassifiedToken> },
    /// The submission has open blocks or delimiters and expects further input.
    Incomplete {
        tokens: Vec<ClassifiedToken>,
        open_constructs: Vec<String>,
    },
    /// The submission contains an unrecoverable syntax or lexical error.
    Invalid {
        tokens: Vec<ClassifiedToken>,
        diagnostic: Diagnostic,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OpenBlockKind {
    Function,
    Class,
    Struct,
    Interface,
    If,
    While,
    For,
    Repeat,
}

impl OpenBlockKind {
    const fn name(self) -> &'static str {
        match self {
            Self::Function => "FUNCTION",
            Self::Class => "CLASS",
            Self::Struct => "STRUCT",
            Self::Interface => "INTERFACE",
            Self::If => "IF",
            Self::While => "WHILE",
            Self::For => "FOR",
            Self::Repeat => "REPEAT",
        }
    }
}

fn make_parse_error(message: impl Into<String>, span: Span) -> Diagnostic {
    let message = message.into();
    Diagnostic::parse_message(&message, span).unwrap_or_else(|_| Diagnostic::lexical(message, span))
}

/// Classifies a submission string to determine if it is complete, incomplete, or invalid.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn classify_submission(source: &str) -> SubmissionCompleteness {
    let (classified, lexical_incomplete) = scan_classified_tokens(source);

    if let Some(reason) = lexical_incomplete {
        return SubmissionCompleteness::Incomplete {
            tokens: classified,
            open_constructs: vec![reason],
        };
    }

    let source_file = SourceFile::new("repl.bn", source);
    let tokens = match lex(&source_file) {
        Ok(tokens) => tokens,
        Err(diagnostic) => {
            let message = diagnostic.message.as_ref();
            if message.contains(bn_catalog::text(
                "frontend",
                "msg.session_unterminated_string_literal",
            )) {
                return SubmissionCompleteness::Incomplete {
                    tokens: classified,
                    open_constructs: vec!["string".to_string()],
                };
            }
            if message.contains(bn_catalog::text(
                "frontend",
                "msg.session_unterminated_block_comment",
            )) {
                return SubmissionCompleteness::Incomplete {
                    tokens: classified,
                    open_constructs: vec!["comment".to_string()],
                };
            }
            return SubmissionCompleteness::Invalid {
                tokens: classified,
                diagnostic,
            };
        }
    };

    let meaningful: Vec<&Token> = tokens
        .iter()
        .filter(|token| !matches!(token.kind, TokenKind::Newline | TokenKind::Eof))
        .collect();

    if meaningful.is_empty() {
        return SubmissionCompleteness::Complete { tokens: classified };
    }

    // 1. Delimiter balance check: `(` and `[`
    let mut delimiter_stack = Vec::new();
    for token in &tokens {
        match &token.kind {
            TokenKind::Symbol(Symbol::LeftParen) => {
                delimiter_stack.push(("(".to_string(), token.span));
            }
            TokenKind::Symbol(Symbol::LeftBracket) => {
                delimiter_stack.push(("[".to_string(), token.span));
            }
            TokenKind::Symbol(Symbol::RightParen) => {
                if let Some((open, _)) = delimiter_stack.pop() {
                    if open != "(" {
                        return SubmissionCompleteness::Invalid {
                            tokens: classified,
                            diagnostic: make_parse_error(
                                bn_catalog::text("frontend", "msg.session_unmatched_closing_paren"),
                                token.span,
                            ),
                        };
                    }
                } else {
                    return SubmissionCompleteness::Invalid {
                        tokens: classified,
                        diagnostic: make_parse_error(
                            bn_catalog::text("frontend", "msg.session_unmatched_closing_paren"),
                            token.span,
                        ),
                    };
                }
            }
            TokenKind::Symbol(Symbol::RightBracket) => {
                if let Some((open, _)) = delimiter_stack.pop() {
                    if open != "[" {
                        return SubmissionCompleteness::Invalid {
                            tokens: classified,
                            diagnostic: make_parse_error(
                                bn_catalog::text(
                                    "frontend",
                                    "msg.session_unmatched_closing_bracket",
                                ),
                                token.span,
                            ),
                        };
                    }
                } else {
                    return SubmissionCompleteness::Invalid {
                        tokens: classified,
                        diagnostic: make_parse_error(
                            bn_catalog::text("frontend", "msg.session_unmatched_closing_bracket"),
                            token.span,
                        ),
                    };
                }
            }
            _ => {}
        }
    }

    // 2. Block structure check: FUNCTION, CLASS, STRUCT, INTERFACE, IF, WHILE, FOR, REPEAT
    let mut block_stack: Vec<(OpenBlockKind, Span)> = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if let TokenKind::Keyword(word) = &token.kind {
            match word.as_str() {
                "FUNCTION" => {
                    let is_preceded_by_end_or_exit = index > 0
                        && matches!(
                            &tokens[index - 1].kind,
                            TokenKind::Keyword(prev) if prev == "END" || prev == "EXIT"
                        );
                    if !is_preceded_by_end_or_exit {
                        block_stack.push((OpenBlockKind::Function, token.span));
                    }
                }
                "CLASS" => {
                    let is_end = index > 0
                        && matches!(
                            &tokens[index - 1].kind,
                            TokenKind::Keyword(prev) if prev == "END"
                        );
                    if !is_end {
                        block_stack.push((OpenBlockKind::Class, token.span));
                    }
                }
                "STRUCT" => {
                    let is_end = index > 0
                        && matches!(
                            &tokens[index - 1].kind,
                            TokenKind::Keyword(prev) if prev == "END"
                        );
                    if !is_end {
                        block_stack.push((OpenBlockKind::Struct, token.span));
                    }
                }
                "INTERFACE" => {
                    let is_end = index > 0
                        && matches!(
                            &tokens[index - 1].kind,
                            TokenKind::Keyword(prev) if prev == "END"
                        );
                    if !is_end {
                        block_stack.push((OpenBlockKind::Interface, token.span));
                    }
                }
                "WHILE" => {
                    let is_end_or_exit = index > 0
                        && matches!(
                            &tokens[index - 1].kind,
                            TokenKind::Keyword(prev) if prev == "END" || prev == "EXIT" || prev == "CONTINUE"
                        );
                    if !is_end_or_exit {
                        block_stack.push((OpenBlockKind::While, token.span));
                    }
                }
                "FOR" => {
                    let is_end_or_exit = index > 0
                        && matches!(
                            &tokens[index - 1].kind,
                            TokenKind::Keyword(prev) if prev == "END" || prev == "EXIT" || prev == "CONTINUE"
                        );
                    if !is_end_or_exit {
                        block_stack.push((OpenBlockKind::For, token.span));
                    }
                }
                "REPEAT" => {
                    let is_end_or_exit = index > 0
                        && matches!(
                            &tokens[index - 1].kind,
                            TokenKind::Keyword(prev) if prev == "END" || prev == "EXIT" || prev == "CONTINUE"
                        );
                    if !is_end_or_exit {
                        block_stack.push((OpenBlockKind::Repeat, token.span));
                    }
                }
                "IF" => {
                    let is_end = index > 0
                        && matches!(
                            &tokens[index - 1].kind,
                            TokenKind::Keyword(prev) if prev == "END" || prev == "ELSE"
                        );
                    if !is_end {
                        // Check if block IF vs single-line IF
                        let mut cursor = index + 1;
                        let mut is_block_if = false;
                        while cursor < tokens.len() {
                            match &tokens[cursor].kind {
                                TokenKind::Keyword(w) if w == "THEN" => {
                                    let next_after_then = cursor + 1;
                                    if next_after_then >= tokens.len()
                                        || matches!(
                                            &tokens[next_after_then].kind,
                                            TokenKind::Newline | TokenKind::Eof
                                        )
                                    {
                                        is_block_if = true;
                                    }
                                    break;
                                }
                                TokenKind::Newline | TokenKind::Eof => break,
                                _ => cursor += 1,
                            }
                        }
                        if is_block_if {
                            block_stack.push((OpenBlockKind::If, token.span));
                        }
                    }
                }
                "END" => {
                    if index + 1 < tokens.len()
                        && let TokenKind::Keyword(next_word) = &tokens[index + 1].kind
                    {
                        let matched = match next_word.as_str() {
                            "FUNCTION" => Some(OpenBlockKind::Function),
                            "CLASS" => Some(OpenBlockKind::Class),
                            "STRUCT" => Some(OpenBlockKind::Struct),
                            "INTERFACE" => Some(OpenBlockKind::Interface),
                            "IF" => Some(OpenBlockKind::If),
                            "WHILE" => Some(OpenBlockKind::While),
                            "FOR" => Some(OpenBlockKind::For),
                            "REPEAT" => Some(OpenBlockKind::Repeat),
                            _ => None,
                        };
                        if let Some(expected_kind) = matched {
                            if let Some((top_kind, _)) = block_stack.pop() {
                                if top_kind != expected_kind {
                                    return SubmissionCompleteness::Invalid {
                                        tokens: classified,
                                        diagnostic: make_parse_error(
                                            bn_catalog::fill(
                                                "frontend",
                                                "msg.session_mismatched_block_terminator",
                                                &[
                                                    ("expected", &top_kind.name()),
                                                    ("found", &next_word),
                                                ],
                                            ),
                                            token.span,
                                        ),
                                    };
                                }
                                index += 2;
                                continue;
                            }
                            return SubmissionCompleteness::Invalid {
                                tokens: classified,
                                diagnostic: make_parse_error(
                                    bn_catalog::fill(
                                        "frontend",
                                        "msg.session_unexpected_end_block",
                                        &[("name", &next_word)],
                                    ),
                                    token.span,
                                ),
                            };
                        }
                    }
                }
                "UNTIL" => {
                    // In Basic Next, REPEAT statement is terminated by END REPEAT:
                    // `REPEAT \n statements \n UNTIL expr \n END REPEAT`
                    // So OpenBlockKind::Repeat remains on the stack until END REPEAT.
                }
                _ => {}
            }
        }
        index += 1;
    }

    let mut open_reasons = Vec::new();
    for (open_delim, _) in delimiter_stack {
        open_reasons.push(format!("delimiter: {open_delim}"));
    }
    for (open_block, _) in block_stack {
        open_reasons.push(format!("block: {}", open_block.name()));
    }

    // 3. Trailing operator or connector check
    if let Some(last_token) = meaningful.last() {
        let is_trailing_continuation = match &last_token.kind {
            TokenKind::Symbol(symbol) => matches!(
                symbol,
                Symbol::Plus
                    | Symbol::Minus
                    | Symbol::Star
                    | Symbol::Slash
                    | Symbol::Percent
                    | Symbol::Power
                    | Symbol::Assign
                    | Symbol::PlusAssign
                    | Symbol::MinusAssign
                    | Symbol::StarAssign
                    | Symbol::SlashAssign
                    | Symbol::PercentAssign
                    | Symbol::PowerAssign
                    | Symbol::Equal
                    | Symbol::NotEqual
                    | Symbol::Less
                    | Symbol::LessEqual
                    | Symbol::Greater
                    | Symbol::GreaterEqual
                    | Symbol::Comma
            ),
            TokenKind::Keyword(word) => matches!(
                word.as_str(),
                "AND" | "OR" | "XOR" | "AS" | "TO" | "STEP" | "THEN" | "ELSE"
            ),
            _ => false,
        };
        if is_trailing_continuation {
            open_reasons.push("continuation".to_string());
        }
    }

    if !open_reasons.is_empty() {
        return SubmissionCompleteness::Incomplete {
            tokens: classified,
            open_constructs: open_reasons,
        };
    }

    // 4. Validate syntax
    if let Err(diagnostic) = validate_submission_syntax(&tokens) {
        return SubmissionCompleteness::Invalid {
            tokens: classified,
            diagnostic,
        };
    }

    SubmissionCompleteness::Complete { tokens: classified }
}

fn validate_submission_syntax(tokens: &[Token]) -> Result<(), Diagnostic> {
    if tokens.is_empty() {
        return Ok(());
    }
    if parse(tokens).is_ok() {
        return Ok(());
    }
    let trimmed = trim_newlines(tokens);
    if parse_expression(trimmed).is_ok() {
        return Ok(());
    }
    let sp = tokens[0].span;
    let end_sp = tokens.last().map_or(sp, |t| t.span);
    let mut wrapped = vec![
        Token {
            kind: TokenKind::Keyword("FUNCTION".to_string()),
            span: sp,
        },
        Token {
            kind: TokenKind::Identifier("Start".to_string()),
            span: sp,
        },
        Token {
            kind: TokenKind::Symbol(Symbol::LeftParen),
            span: sp,
        },
        Token {
            kind: TokenKind::Symbol(Symbol::RightParen),
            span: sp,
        },
        Token {
            kind: TokenKind::Keyword("AS".to_string()),
            span: sp,
        },
        Token {
            kind: TokenKind::Identifier("VOID".to_string()),
            span: sp,
        },
        Token {
            kind: TokenKind::Newline,
            span: sp,
        },
    ];
    wrapped.extend_from_slice(trim_newlines(tokens));
    wrapped.push(Token {
        kind: TokenKind::Newline,
        span: end_sp,
    });
    wrapped.push(Token {
        kind: TokenKind::Keyword("END".to_string()),
        span: end_sp,
    });
    wrapped.push(Token {
        kind: TokenKind::Keyword("FUNCTION".to_string()),
        span: end_sp,
    });
    wrapped.push(Token {
        kind: TokenKind::Newline,
        span: end_sp,
    });
    wrapped.push(Token {
        kind: TokenKind::Eof,
        span: end_sp,
    });

    parse(&wrapped).map(|_| ())
}

pub(crate) fn trim_newlines(tokens: &[Token]) -> &[Token] {
    let mut start = 0;
    while start < tokens.len() && matches!(tokens[start].kind, TokenKind::Newline) {
        start += 1;
    }
    let mut end = tokens.len();
    while end > start && matches!(tokens[end - 1].kind, TokenKind::Newline | TokenKind::Eof) {
        end -= 1;
    }
    &tokens[start..end]
}

/// Tokenizes text into `ClassifiedTokens`, capturing comments and unclosed literals.
#[allow(clippy::too_many_lines)]
fn scan_classified_tokens(source: &str) -> (Vec<ClassifiedToken>, Option<String>) {
    let mut tokens = Vec::new();
    let mut incomplete_reason = None;
    let bytes = source.as_bytes();
    let mut offset = 0;
    let mut line = 1;
    let mut column = 1;

    while offset < bytes.len() {
        let character = bytes[offset] as char;
        let token_offset = offset;
        let token_line = line;
        let token_column = column;

        if character == '\n' {
            tokens.push(ClassifiedToken {
                kind: ClassifiedTokenKind::Newline,
                text: "\n".to_string(),
                offset: token_offset,
                length: 1,
                line: token_line,
                column: token_column,
            });
            offset += 1;
            line += 1;
            column = 1;
            continue;
        }

        if character.is_whitespace() {
            let start = offset;
            while offset < bytes.len()
                && (bytes[offset] as char).is_whitespace()
                && bytes[offset] != b'\n'
            {
                offset += 1;
                column += 1;
            }
            tokens.push(ClassifiedToken {
                kind: ClassifiedTokenKind::Whitespace,
                text: source[start..offset].to_string(),
                offset: start,
                length: offset - start,
                line: token_line,
                column: token_column,
            });
            continue;
        }

        // Line comment `//`
        if offset + 1 < bytes.len() && bytes[offset] == b'/' && bytes[offset + 1] == b'/' {
            let start = offset;
            while offset < bytes.len() && bytes[offset] != b'\n' && bytes[offset] != b'\r' {
                offset += 1;
                column += 1;
            }
            tokens.push(ClassifiedToken {
                kind: ClassifiedTokenKind::Comment,
                text: source[start..offset].to_string(),
                offset: start,
                length: offset - start,
                line: token_line,
                column: token_column,
            });
            continue;
        }

        // Block comment `/* ... */`
        if offset + 1 < bytes.len() && bytes[offset] == b'/' && bytes[offset + 1] == b'*' {
            let start = offset;
            offset += 2;
            column += 2;
            let mut closed = false;
            while offset + 1 < bytes.len() {
                if bytes[offset] == b'*' && bytes[offset + 1] == b'/' {
                    offset += 2;
                    column += 2;
                    closed = true;
                    break;
                }
                if bytes[offset] == b'\n' {
                    line += 1;
                    column = 1;
                } else {
                    column += 1;
                }
                offset += 1;
            }
            if !closed {
                incomplete_reason = Some("comment".to_string());
                tokens.push(ClassifiedToken {
                    kind: ClassifiedTokenKind::Comment,
                    text: source[start..].to_string(),
                    offset: start,
                    length: source.len() - start,
                    line: token_line,
                    column: token_column,
                });
                break;
            }
            tokens.push(ClassifiedToken {
                kind: ClassifiedTokenKind::Comment,
                text: source[start..offset].to_string(),
                offset: start,
                length: offset - start,
                line: token_line,
                column: token_column,
            });
            continue;
        }

        // String literal `"`
        if character == '"' {
            let start = offset;
            offset += 1;
            column += 1;
            let mut closed = false;
            while offset < bytes.len() {
                let current = bytes[offset];
                if current == b'"' {
                    offset += 1;
                    column += 1;
                    closed = true;
                    break;
                }
                if current == b'\\' && offset + 1 < bytes.len() {
                    offset += 2;
                    column += 2;
                    continue;
                }
                if current == b'\n' {
                    line += 1;
                    column = 1;
                } else {
                    column += 1;
                }
                offset += 1;
            }
            if !closed {
                incomplete_reason = Some("string".to_string());
                tokens.push(ClassifiedToken {
                    kind: ClassifiedTokenKind::String,
                    text: source[start..].to_string(),
                    offset: start,
                    length: source.len() - start,
                    line: token_line,
                    column: token_column,
                });
                break;
            }
            tokens.push(ClassifiedToken {
                kind: ClassifiedTokenKind::String,
                text: source[start..offset].to_string(),
                offset: start,
                length: offset - start,
                line: token_line,
                column: token_column,
            });
            continue;
        }

        // Number
        if character.is_ascii_digit() {
            let start = offset;
            while offset < bytes.len()
                && (bytes[offset].is_ascii_alphanumeric() || bytes[offset] == b'.')
            {
                offset += 1;
                column += 1;
            }
            tokens.push(ClassifiedToken {
                kind: ClassifiedTokenKind::Number,
                text: source[start..offset].to_string(),
                offset: start,
                length: offset - start,
                line: token_line,
                column: token_column,
            });
            continue;
        }

        // Identifier or Keyword
        if character.is_ascii_alphabetic() || character == '_' {
            let start = offset;
            while offset < bytes.len()
                && (bytes[offset].is_ascii_alphanumeric() || bytes[offset] == b'_')
            {
                offset += 1;
                column += 1;
            }
            let text = &source[start..offset];
            let kind = if is_reserved_word(text) {
                ClassifiedTokenKind::Keyword
            } else if matches!(text, "NULL" | "NA" | "EOF" | "TRUE" | "FALSE") {
                ClassifiedTokenKind::Special
            } else {
                ClassifiedTokenKind::Identifier
            };
            tokens.push(ClassifiedToken {
                kind,
                text: text.to_string(),
                offset: start,
                length: offset - start,
                line: token_line,
                column: token_column,
            });
            continue;
        }

        // Symbols and Operators
        let start = offset;
        offset += 1;
        column += 1;
        let symbol_text = &source[start..offset];
        let kind = match character {
            '+' | '-' | '*' | '/' | '%' | '^' | '=' | '<' | '>' => ClassifiedTokenKind::Operator,
            '(' | ')' | '[' | ']' | ',' | '.' | ':' | ';' => ClassifiedTokenKind::Symbol,
            _ => ClassifiedTokenKind::Other,
        };
        tokens.push(ClassifiedToken {
            kind,
            text: symbol_text.to_string(),
            offset: start,
            length: 1,
            line: token_line,
            column: token_column,
        });
    }

    (tokens, incomplete_reason)
}

#[path = "session_accumulator.rs"]
pub mod session_accumulator;
pub use session_accumulator::*;

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
