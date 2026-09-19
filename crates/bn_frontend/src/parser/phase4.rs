#![allow(clippy::wildcard_imports)]
use super::*;

impl<'a> Parser<'a> {
    /// `OVERRIDE` is accepted only immediately before `FUNCTION`
    /// (`PUBLIC OVERRIDE FUNCTION Name…`, lock §2.2 of bucket 0.5.2).
    pub(crate) fn override_modifier(&self) -> Result<bool, Diagnostic> {
        let line = self.line_tokens();
        let Some(position) = line.iter().position(
            |token| matches!(&token.kind, TokenKind::Keyword(word) if word == "OVERRIDE"),
        ) else {
            return Ok(false);
        };
        let before_function = matches!(
            line.get(position + 1).map(|token| &token.kind),
            Some(TokenKind::Keyword(word)) if word == "FUNCTION"
        );
        if before_function {
            Ok(true)
        } else {
            Err(self.error("OVERRIDE immediately before FUNCTION (PUBLIC OVERRIDE FUNCTION Name)"))
        }
    }

    pub(crate) fn member_modifiers(&self) -> (Option<crate::ast::Visibility>, bool) {
        let line = self.line_tokens();
        let visibility = line.iter().find_map(|token| match &token.kind {
            TokenKind::Keyword(word) if word == "PUBLIC" => Some(crate::ast::Visibility::Public),
            TokenKind::Keyword(word) if word == "PRIVATE" => Some(crate::ast::Visibility::Private),
            TokenKind::Keyword(word) if word == "PROTECTED" => {
                Some(crate::ast::Visibility::Protected)
            }
            _ => None,
        });
        let is_static = line
            .iter()
            .any(|token| matches!(&token.kind, TokenKind::Keyword(word) if word == "STATIC"));
        (visibility, is_static)
    }

    pub(crate) fn member_function_name(&self) -> Result<String, Diagnostic> {
        let line = self.line_tokens();
        let function = line
            .iter()
            .position(|token| matches!(&token.kind, TokenKind::Keyword(word) if word == "FUNCTION"))
            .ok_or_else(|| self.error("expected FUNCTION"))?;
        match &line
            .get(function + 1)
            .ok_or_else(|| self.error("expected function name"))?
            .kind
        {
            TokenKind::Identifier(name) => Ok(name.clone()),
            TokenKind::Keyword(name) if matches!(name.as_str(), "CONSTRUCTOR" | "DESTRUCTOR") => {
                Ok(name.clone())
            }
            _ => Err(self.error("expected function name")),
        }
    }

    pub(crate) fn end_word(&mut self) -> Result<&'static str, Diagnostic> {
        for word in [
            "FUNCTION",
            "CLASS",
            "STRUCT",
            "INTERFACE",
            "IF",
            "WHILE",
            "REPEAT",
            "FOR",
        ] {
            if self.keyword(word) {
                self.take();
                return Ok(word);
            }
        }
        Err(self.error("expected a compound form after END"))
    }
    pub(crate) fn consume_to_newline(&mut self) -> Result<(), Diagnostic> {
        while !self.newline_token() && !self.eof() {
            self.take();
        }
        self.newline().map(|_| ())
    }
    pub(crate) fn newline(&mut self) -> Result<crate::source::Position, Diagnostic> {
        if self.newline_token() {
            Ok(self.take().span.end)
        } else {
            Err(self.error("expected end of line"))
        }
    }
    pub(crate) fn newlines(&mut self) {
        while self.newline_token() {
            self.take();
        }
    }
    pub(crate) fn identifier_name(&mut self) -> Result<String, Diagnostic> {
        match &self.peek().kind {
            TokenKind::Identifier(name) => {
                let name = name.clone();
                self.take();
                Ok(name)
            }
            _ => Err(self.error("expected identifier")),
        }
    }
    pub(crate) fn expect_keyword(&mut self, word: &str) -> Result<(), Diagnostic> {
        if self.keyword(word) {
            self.take();
            Ok(())
        } else {
            Err(Diagnostic::parse_facts(
                word,
                "the current declaration or statement",
                self.peek().span,
            )
            .expect("parser diagnostic schema"))
        }
    }
    pub(crate) fn expect_symbol(&mut self, symbol: Symbol) -> Result<(), Diagnostic> {
        if self.symbol(symbol) {
            self.take();
            Ok(())
        } else {
            Err(self.error("expected punctuation"))
        }
    }
    pub(crate) fn keyword(&self, word: &str) -> bool {
        matches!(&self.peek().kind, TokenKind::Keyword(value) if value == word)
    }
    pub(crate) fn symbol(&self, symbol: Symbol) -> bool {
        matches!(self.peek().kind, TokenKind::Symbol(value) if value == symbol)
    }
    pub(crate) fn newline_token(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Newline)
    }
    pub(crate) fn eof(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Eof)
    }
    pub(crate) fn peek(&self) -> &'a Token {
        if self.index < self.tokens.len() {
            &self.tokens[self.index]
        } else {
            self.tokens.last().expect("token stream is not empty")
        }
    }
    pub(crate) fn take(&mut self) -> &'a Token {
        let token = self.peek();
        self.index += 1;
        token
    }
    pub(crate) fn previous(&self) -> &'a Token {
        &self.tokens[self.index - 1]
    }
    /// Parses an expression slice taken from the current line; an empty slice
    /// (e.g. `LET x AS INTEGER =`) is a syntax error at the cursor, never a panic.
    pub(crate) fn expression_in(&self, tokens: &[Token]) -> Result<Expression, Diagnostic> {
        if tokens.is_empty() {
            return Err(self.error("expected expression"));
        }
        parse_expression(tokens)
    }

    pub(crate) fn error(&self, message: impl Into<String>) -> Diagnostic {
        let message = message.into();
        Diagnostic::parse_facts(message, "source parser", self.peek().span).unwrap_or_else(|_| {
            Diagnostic {
                code: "E0100",
                message: "parser error".into(),
                span: self.peek().span,
                structured: None,
            }
        })
    }
}
