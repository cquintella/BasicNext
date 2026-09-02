#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) struct ExpressionParser<'a> {
    pub(crate) tokens: &'a [Token],
    pub(crate) index: usize,
}

impl<'a> ExpressionParser<'a> {
    pub(crate) fn expression(&mut self, minimum: u8) -> Result<Expression, Diagnostic> {
        let mut left = self.prefix()?;
        loop {
            if self.at_end() {
                break;
            }
            if self.symbol(Symbol::LeftParen) {
                left = self.call(left)?;
                continue;
            }
            if self.symbol(Symbol::Dot) {
                left = self.member(left)?;
                continue;
            }
            if !self.at_end() && self.symbol(Symbol::LeftBracket) {
                left = self.index(left)?;
                continue;
            }
            if self.keyword("AS") {
                left = self.cast(left)?;
                continue;
            }
            let Some((precedence, right_associative)) = self.binary() else {
                break;
            };
            if precedence < minimum {
                break;
            }
            let operator = text(self.take());
            let right = if operator == "IS" && self.type_test_start() {
                self.type_test()?
            } else {
                self.expression(precedence + u8::from(!right_associative))?
            };
            let span = Span {
                start: left.span.start,
                end: right.span.end,
            };
            left = Expression {
                kind: ExpressionKind::Binary {
                    operator,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }
        Ok(left)
    }

    #[allow(clippy::too_many_lines)] // Primary-expression alternatives mirror the grammar.
    fn prefix(&mut self) -> Result<Expression, Diagnostic> {
        if self.keyword("ASYNC") {
            return self.async_submit();
        }
        if self.keyword("AWAIT") {
            return self.await_expression();
        }
        if self.keyword("INPUT") {
            let start = self.take().span.start;
            self.expect_symbol(Symbol::LeftParen)?;
            let end = self.expect_symbol(Symbol::RightParen)?.end;
            return Ok(Expression {
                kind: ExpressionKind::Input,
                span: Span { start, end },
            });
        }
        if self.keyword("LEN") {
            return self.operand_form(|operand| ExpressionKind::Length { operand });
        }
        if self.keyword("SIZEOF") {
            return self.operand_form(|operand| ExpressionKind::SizeOf { operand });
        }
        if self.keyword("HOST") {
            let start = self.take().span.start;
            self.expect_symbol(Symbol::Dot)?;
            let name_token = self.take();
            let TokenKind::Identifier(name) = &name_token.kind else {
                return Err(self.error("expected host capability name"));
            };
            require_host_capability_name(name).map_err(|message| self.error(message))?;
            return Ok(Expression {
                kind: ExpressionKind::HostCapability { name: name.clone() },
                span: Span {
                    start,
                    end: name_token.span.end,
                },
            });
        }
        if self.keyword("NEW") {
            let start = self.take().span.start;
            let type_token = self.take();
            let mut type_name = text(type_token);
            if type_name.is_empty() {
                return Err(self.error("expected type after NEW"));
            }
            let numeric = matches!(
                type_name.as_str(),
                "BYTE"
                    | "INT8"
                    | "INT16"
                    | "INT32"
                    | "INT64"
                    | "UINT16"
                    | "UINT32"
                    | "UINT64"
                    | "FLOAT32"
                    | "FLOAT64"
                    | "INTEGER"
                    | "FLOAT"
                    | "TIMESTAMP"
            );
            if !numeric && !matches!(type_token.kind, TokenKind::Identifier(_)) {
                return Err(self.error("NEW requires a numeric type or CLASS name"));
            }
            while !numeric && self.symbol(Symbol::Dot) {
                self.take();
                type_name.push('.');
                type_name.push_str(&text(self.identifier_token()?));
            }
            let mut arguments = Vec::new();
            if self.symbol(Symbol::LeftBracket) {
                if !numeric {
                    return Err(self.error("CLASS construction requires parentheses"));
                }
                self.take();
                arguments.push(self.expression(0)?);
                let end = self.expect_symbol(Symbol::RightBracket)?.end;
                return Ok(Expression {
                    kind: ExpressionKind::New {
                        type_name,
                        arguments,
                    },
                    span: Span { start, end },
                });
            }
            if self.symbol(Symbol::LeftParen) {
                if numeric {
                    return Err(self.error("numeric NEW does not use parentheses"));
                }
                self.take();
                if !self.symbol(Symbol::RightParen) {
                    loop {
                        arguments.push(self.expression(0)?);
                        if !self.symbol(Symbol::Comma) {
                            break;
                        }
                        self.take();
                    }
                }
                let end = self.expect_symbol(Symbol::RightParen)?.end;
                return Ok(Expression {
                    kind: ExpressionKind::New {
                        type_name,
                        arguments,
                    },
                    span: Span { start, end },
                });
            }
            if !numeric {
                return Err(self.error("CLASS construction requires parentheses"));
            }
            return Ok(Expression {
                kind: ExpressionKind::New {
                    type_name,
                    arguments,
                },
                span: Span {
                    start,
                    end: type_token.span.end,
                },
            });
        }
        if self.symbol(Symbol::LeftBracket) {
            let start = self.take().span.start;
            let mut values = Vec::new();
            if !self.symbol(Symbol::RightBracket) {
                loop {
                    values.push(self.expression(0)?);
                    if !self.symbol(Symbol::Comma) {
                        break;
                    }
                    self.take();
                }
            }
            let end = self.expect_symbol(Symbol::RightBracket)?.end;
            return Ok(Expression {
                kind: ExpressionKind::Vector { values },
                span: Span { start, end },
            });
        }
        if self.keyword("NOT") || self.symbol(Symbol::Minus) {
            let token = self.take();
            let operand = self.expression(11)?;
            let end = operand.span.end;
            return Ok(Expression {
                kind: ExpressionKind::Unary {
                    operator: text(token),
                    operand: Box::new(operand),
                },
                span: Span {
                    start: token.span.start,
                    end,
                },
            });
        }
        if self.symbol(Symbol::LeftParen) {
            self.take();
            let expression = self.expression(0)?;
            self.expect_symbol(Symbol::RightParen)?;
            return Ok(expression);
        }
        if matches!(
            self.peek().kind,
            TokenKind::Identifier(_)
                | TokenKind::Integer(_)
                | TokenKind::Float(_)
                | TokenKind::String(_)
                | TokenKind::Special(_)
        ) || self.keyword("TRUE")
            || self.keyword("FALSE")
            || self.keyword("NULL")
            || self.keyword("NA")
            || self.keyword("EOF")
            || self.keyword("SELF")
            || self.keyword("SUPER")
            || matches!(&self.peek().kind, TokenKind::Keyword(word) if matches!(word.as_str(), "BOOLEAN" | "BYTE" | "INT8" | "INT16" | "INT32" | "INT64" | "UINT16" | "UINT32" | "UINT64" | "FLOAT32" | "FLOAT64" | "INTEGER" | "FLOAT" | "TIMESTAMP" | "STRING" | "SYSTEM"))
        {
            let token = self.take();
            return Ok(Expression {
                kind: expression_atom(token),
                span: token.span,
            });
        }
        Err(self.error("expected expression"))
    }

    fn async_submit(&mut self) -> Result<Expression, Diagnostic> {
        let start = self.take().span.start;
        // The queue operand is intentionally a single primary in the initial
        // contract; parsing it without consuming the target keeps the sugar
        // equivalent to `queue.Async(Function, ...)`.
        let queue = self.prefix()?;
        let target = self.identifier_token()?;
        let target_name = match &target.kind {
            TokenKind::Identifier(name) => name.clone(),
            _ => unreachable!(),
        };
        self.expect_symbol(Symbol::LeftParen)?;
        let mut arguments = vec![Expression {
            kind: ExpressionKind::Name { name: target_name },
            span: target.span,
        }];
        if !self.symbol(Symbol::RightParen) {
            loop {
                arguments.push(self.expression(0)?);
                if !self.symbol(Symbol::Comma) {
                    break;
                }
                self.take();
            }
        }
        let end = self.expect_symbol(Symbol::RightParen)?.end;
        let member = Expression {
            span: Span {
                start: queue.span.start,
                end: target.span.end,
            },
            kind: ExpressionKind::Member {
                object: Box::new(queue),
                name: "Async".into(),
            },
        };
        Ok(Expression {
            span: Span { start, end },
            kind: ExpressionKind::Call {
                callee: Box::new(member),
                arguments,
            },
        })
    }

    fn await_expression(&mut self) -> Result<Expression, Diagnostic> {
        let start = self.take().span.start;
        let ticket = self.prefix()?;
        self.expect_symbol(Symbol::LeftParen)?;
        let timeout = self.expression(0)?;
        let end = self.expect_symbol(Symbol::RightParen)?.end;
        let member = Expression {
            // Keep a distinct span from the receiver: the IR model indexes
            // resolved expression types by span.
            span: Span {
                start: ticket.span.start,
                end: timeout.span.start,
            },
            kind: ExpressionKind::Member {
                object: Box::new(ticket),
                name: "Wait".into(),
            },
        };
        Ok(Expression {
            span: Span { start, end },
            kind: ExpressionKind::Call {
                callee: Box::new(member),
                arguments: vec![timeout],
            },
        })
    }
    fn call(&mut self, callee: Expression) -> Result<Expression, Diagnostic> {
        self.take();
        let mut arguments = Vec::new();
        if !self.symbol(Symbol::RightParen) {
            loop {
                arguments.push(self.expression(0)?);
                if !self.symbol(Symbol::Comma) {
                    break;
                }
                self.take();
            }
        }
        let end = self.expect_symbol(Symbol::RightParen)?.end;
        Ok(Expression {
            span: Span {
                start: callee.span.start,
                end,
            },
            kind: ExpressionKind::Call {
                callee: Box::new(callee),
                arguments,
            },
        })
    }
    fn member(&mut self, object: Expression) -> Result<Expression, Diagnostic> {
        self.take();
        let token = self.identifier_token()?;
        let end = token.span.end;
        Ok(Expression {
            span: Span {
                start: object.span.start,
                end,
            },
            kind: ExpressionKind::Member {
                object: Box::new(object),
                name: match &token.kind {
                    TokenKind::Identifier(name) => name.clone(),
                    _ => unreachable!(),
                },
            },
        })
    }
    fn index(&mut self, object: Expression) -> Result<Expression, Diagnostic> {
        self.take();
        let index = self.expression(0)?;
        let end = self.expect_symbol(Symbol::RightBracket)?.end;
        Ok(Expression {
            span: Span {
                start: object.span.start,
                end,
            },
            kind: ExpressionKind::Index {
                object: Box::new(object),
                index: Box::new(index),
            },
        })
    }
    fn cast(&mut self, value: Expression) -> Result<Expression, Diagnostic> {
        self.take();
        if !matches!(&self.peek().kind, TokenKind::Keyword(word) if matches!(word.as_str(), "BYTE" | "INT8" | "INT16" | "INT32" | "INT64" | "UINT16" | "UINT32" | "UINT64" | "FLOAT32" | "FLOAT64" | "INTEGER" | "FLOAT" | "TIMESTAMP" | "BOOLEAN"))
        {
            return Err(self.error("expected numeric type or BOOLEAN after AS"));
        }
        let type_token = self.take();
        let end = type_token.span.end;
        Ok(Expression {
            span: Span {
                start: value.span.start,
                end,
            },
            kind: ExpressionKind::Cast {
                value: Box::new(value),
                type_ref: crate::ast::TypeReference {
                    alternatives: vec![crate::ast::TypeAtom {
                        name: text(type_token),
                        parts: Vec::new(),
                        dimensions: Vec::new(),
                        span: type_token.span,
                    }],
                    span: type_token.span,
                },
            },
        })
    }
    fn type_test_start(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Identifier(_))
            || matches!(&self.peek().kind, TokenKind::Keyword(word) if matches!(word.as_str(),
                "BOOLEAN" | "BYTE" | "INT8" | "INT16" | "INT32" | "INT64" | "UINT16"
                | "UINT32" | "UINT64" | "FLOAT32" | "FLOAT64" | "INTEGER" | "FLOAT"
                | "TIMESTAMP" | "STRING" | "DATE" | "TIME" | "TIMEZONE" | "SYSTEM" | "POINTER"))
    }
    fn type_test(&mut self) -> Result<Expression, Diagnostic> {
        let first = self.take();
        let mut tokens = vec![first];
        if matches!(&first.kind, TokenKind::Keyword(word) if word == "POINTER") {
            if !self.keyword("TO") {
                return Err(self.error("expected TO in POINTER type test"));
            }
            tokens.push(self.take());
            if matches!(&self.peek().kind, TokenKind::Keyword(word) if matches!(word.as_str(),
                "BYTE" | "INT8" | "INT16" | "INT32" | "INT64" | "UINT16" | "UINT32"
                | "UINT64" | "FLOAT32" | "FLOAT64" | "INTEGER" | "FLOAT" | "TIMESTAMP"
                | "VOID"))
            {
                tokens.push(self.take());
            } else if matches!(self.peek().kind, TokenKind::Identifier(_)) {
                tokens.push(self.take());
                while !self.at_end() && self.symbol(Symbol::Dot) {
                    tokens.push(self.take());
                    tokens.push(self.identifier_token()?);
                }
            } else {
                return Err(
                    self.error("POINTER type test requires a numeric, named, or VOID element type")
                );
            }
            if !self.at_end() && self.symbol(Symbol::LeftBracket) {
                tokens.push(self.take());
                if !self.symbol(Symbol::RightBracket) {
                    if !matches!(self.peek().kind, TokenKind::Integer(_)) {
                        return Err(self.error("pointer shape requires an integer literal"));
                    }
                    tokens.push(self.take());
                }
                if !self.symbol(Symbol::RightBracket) {
                    return Err(self.error("expected ] in pointer type test"));
                }
                tokens.push(self.take());
            }
        } else if matches!(first.kind, TokenKind::Identifier(_)) {
            while !self.at_end() && self.symbol(Symbol::Dot) {
                tokens.push(self.take());
                tokens.push(self.identifier_token()?);
            }
        }
        let span = Span {
            start: first.span.start,
            end: tokens.last().expect("type test token").span.end,
        };
        let atom = crate::ast::TypeAtom {
            name: text(first),
            parts: tokens.iter().skip(1).map(|token| text(token)).collect(),
            dimensions: Vec::new(),
            span,
        };
        Ok(Expression {
            kind: ExpressionKind::TypeTest {
                type_ref: crate::ast::TypeReference {
                    alternatives: vec![atom],
                    span,
                },
            },
            span,
        })
    }
    fn binary(&self) -> Option<(u8, bool)> {
        let value = match &self.peek().kind {
            TokenKind::Keyword(word) => match word.as_str() {
                "OR" => (1, false),
                "XOR" => (2, false),
                "AND" => (3, false),
                "IS" => (4, false),
                "SHL" | "SHR" => (6, false),
                "DIV" => (8, false),
                _ => return None,
            },
            TokenKind::Symbol(Symbol::Assign | Symbol::NotEqual) => (4, false),
            TokenKind::Symbol(
                Symbol::Less | Symbol::LessEqual | Symbol::Greater | Symbol::GreaterEqual,
            ) => (5, false),
            TokenKind::Symbol(Symbol::Plus | Symbol::Minus) => (7, false),
            TokenKind::Symbol(Symbol::Star | Symbol::Slash | Symbol::Percent) => (8, false),
            TokenKind::Symbol(Symbol::Power) => (10, true),
            _ => return None,
        };
        Some(value)
    }
    fn operand_form(
        &mut self,
        wrap: impl FnOnce(Box<Expression>) -> ExpressionKind,
    ) -> Result<Expression, Diagnostic> {
        let start = self.take().span.start;
        self.expect_symbol(Symbol::LeftParen)?;
        let operand = Box::new(self.expression(0)?);
        let end = self.expect_symbol(Symbol::RightParen)?.end;
        Ok(Expression {
            kind: wrap(operand),
            span: Span { start, end },
        })
    }

    fn keyword(&self, word: &str) -> bool {
        matches!(&self.peek().kind, TokenKind::Keyword(value) if value == word)
    }
    fn symbol(&self, symbol: Symbol) -> bool {
        matches!(self.peek().kind, TokenKind::Symbol(value) if value == symbol)
    }
    fn expect_symbol(&mut self, symbol: Symbol) -> Result<Span, Diagnostic> {
        if self.symbol(symbol) {
            Ok(self.take().span)
        } else {
            Err(self.error("expected punctuation"))
        }
    }
    fn identifier_token(&mut self) -> Result<&'a Token, Diagnostic> {
        if matches!(self.peek().kind, TokenKind::Identifier(_)) {
            Ok(self.take())
        } else {
            Err(self.error("expected identifier"))
        }
    }
    pub(crate) fn peek(&self) -> &'a Token {
        if self.index < self.tokens.len() {
            &self.tokens[self.index]
        } else {
            self.tokens.last().expect("token stream is not empty")
        }
    }
    pub(crate) fn at_end(&self) -> bool {
        self.index == self.tokens.len()
    }
    fn take(&mut self) -> &'a Token {
        let token = self.peek();
        self.index += 1;
        token
    }
    pub(crate) fn error(&self, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            code: "E0100",
            message: message.into(),
            span: self.peek().span,
        }
    }
}
