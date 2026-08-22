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

struct ExpressionParser<'a> {
    tokens: &'a [Token],
    index: usize,
}

impl<'a> ExpressionParser<'a> {
    fn expression(&mut self, minimum: u8) -> Result<Expression, Diagnostic> {
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
        if self.keyword("INPUT") {
            let start = self.take().span.start;
            self.expect_symbol(Symbol::LeftParen)?;
            let end = self.expect_symbol(Symbol::RightParen)?.end;
            return Ok(Expression {
                kind: ExpressionKind::Input,
                span: Span { start, end },
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
            if !matches!(&self.peek().kind, TokenKind::Keyword(word) if matches!(word.as_str(),
                "BYTE" | "INT8" | "INT16" | "INT32" | "INT64" | "UINT16" | "UINT32"
                | "UINT64" | "FLOAT32" | "FLOAT64" | "INTEGER" | "FLOAT" | "TIMESTAMP"))
            {
                return Err(self.error("POINTER type test requires a numeric element type"));
            }
            tokens.push(self.take());
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
    fn peek(&self) -> &'a Token {
        &self.tokens[self.index]
    }
    fn at_end(&self) -> bool {
        self.index == self.tokens.len()
    }
    fn take(&mut self) -> &'a Token {
        let token = self.peek();
        self.index += 1;
        token
    }
    fn error(&self, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            code: "E0100",
            message: message.into(),
            span: self.peek().span,
        }
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

impl<'a> Parser<'a> {
    fn program(&mut self) -> Result<Program, Diagnostic> {
        let mut items = Vec::new();
        let mut saw_declaration = false;
        self.newlines();
        while !self.eof() {
            if self.keyword("IMPORT") {
                if saw_declaration {
                    return Err(self.error("IMPORT declarations must precede all declarations"));
                }
                items.push(self.import()?);
            } else {
                items.push(self.declaration()?);
                saw_declaration = true;
            }
            self.newlines();
        }
        Ok(Program {
            source_name: self.source_name.clone(),
            items,
        })
    }

    fn import(&mut self) -> Result<Item, Diagnostic> {
        let start = self.take().span.start;
        let mut path = vec![self.path_part_name()?];
        self.expect_symbol(Symbol::Dot)?;
        path.push(self.path_part_name()?);
        while self.symbol(Symbol::Dot) {
            self.take();
            path.push(self.identifier_name()?);
        }
        self.expect_keyword("AS")?;
        let alias = self.identifier_name()?;
        let end = self.newline()?;
        Ok(Item::Import {
            path,
            alias,
            span: Span { start, end },
        })
    }

    fn path_part_name(&mut self) -> Result<String, Diagnostic> {
        if self.keyword("HOST") {
            self.take();
            Ok("HOST".into())
        } else {
            self.identifier_name()
        }
    }

    fn declaration(&mut self) -> Result<Item, Diagnostic> {
        let start = self.peek().span.start;
        let exported = self.keyword("EXPORT");
        if exported {
            self.take();
        }
        let kind = if self.keyword("FUNCTION") {
            DeclarationKind::Function
        } else if self.keyword("CLASS") {
            DeclarationKind::Class
        } else if self.keyword("STRUCT") {
            DeclarationKind::Struct
        } else if self.keyword("INTERFACE") {
            DeclarationKind::Interface
        } else {
            return Err(self.error("expected IMPORT or a top-level declaration"));
        };
        self.take();
        let name = self.identifier_name()?;
        let interfaces = if kind == DeclarationKind::Class {
            self.implemented_interfaces()?
        } else {
            Vec::new()
        };
        let signature = if kind == DeclarationKind::Function {
            Some(self.function_signature()?)
        } else {
            None
        };
        self.consume_to_newline()?;
        let (end, statements) = self.block(kind.end_word())?;
        Ok(Item::Declaration {
            exported,
            kind,
            name,
            interfaces,
            signature,
            statements,
            span: Span { start, end },
        })
    }

    fn implemented_interfaces(&self) -> Result<Vec<String>, Diagnostic> {
        let line = self.line_tokens();
        let Some(implements) = line.iter().position(
            |token| matches!(&token.kind, TokenKind::Keyword(word) if word == "IMPLEMENTS"),
        ) else {
            return Ok(Vec::new());
        };
        let mut interfaces = Vec::new();
        for token in &line[implements + 1..] {
            if matches!(token.kind, TokenKind::Symbol(Symbol::Comma)) {
                continue;
            }
            match &token.kind {
                TokenKind::Identifier(name) => interfaces.push(name.clone()),
                _ => return Err(self.error("expected interface name after IMPLEMENTS")),
            }
        }
        if interfaces.is_empty() {
            return Err(self.error("IMPLEMENTS requires an interface name"));
        }
        Ok(interfaces)
    }

    fn function_signature(&self) -> Result<crate::ast::FunctionSignature, Diagnostic> {
        self.function_signature_from(self.line_tokens())
    }

    fn function_signature_from(
        &self,
        line: &[Token],
    ) -> Result<crate::ast::FunctionSignature, Diagnostic> {
        let start = line
            .first()
            .map_or(self.peek().span.start, |token| token.span.start);
        let close = line
            .iter()
            .position(|token| matches!(token.kind, TokenKind::Symbol(Symbol::RightParen)))
            .ok_or_else(|| self.error("expected ')' in function declaration"))?;
        let as_index = line
            .iter()
            .enumerate()
            .skip(close + 1)
            .find_map(|(index, token)| {
                matches!(&token.kind, TokenKind::Keyword(word) if word == "AS").then_some(index)
            })
            .ok_or_else(|| self.error("expected AS return type in function declaration"))?;
        let mut parameters = Vec::new();
        let mut index = 1;
        while index < close {
            if matches!(line[index].kind, TokenKind::Symbol(Symbol::Comma)) {
                index += 1;
                continue;
            }
            let name = match &line[index].kind {
                TokenKind::Identifier(name) => name.clone(),
                _ => return Err(self.error("expected parameter name")),
            };
            let parameter_start = line[index].span.start;
            index += 1;
            if !matches!(&line.get(index).map(|token| &token.kind), Some(TokenKind::Keyword(word)) if word == "AS")
            {
                return Err(self.error("expected AS in parameter"));
            }
            index += 1;
            let type_end = line[index..close]
                .iter()
                .position(|token| matches!(token.kind, TokenKind::Symbol(Symbol::Comma)))
                .map_or(close, |offset| index + offset);
            let type_ref = self.type_reference(&line[index..type_end]);
            let end = type_ref.span.end;
            parameters.push(crate::ast::Parameter {
                name,
                type_ref,
                span: Span {
                    start: parameter_start,
                    end,
                },
            });
            index = type_end;
        }
        let return_type = self.type_reference(&line[as_index + 1..]);
        Ok(crate::ast::FunctionSignature {
            parameters,
            span: Span {
                start,
                end: return_type.span.end,
            },
            return_type,
        })
    }

    fn member_function_signature(
        &self,
    ) -> Result<Option<crate::ast::FunctionSignature>, Diagnostic> {
        let line = self.line_tokens();
        let function = line
            .iter()
            .position(|token| matches!(&token.kind, TokenKind::Keyword(word) if word == "FUNCTION"))
            .ok_or_else(|| self.error("expected FUNCTION"))?;
        if matches!(line.get(function + 1).map(|token| &token.kind), Some(TokenKind::Keyword(word)) if matches!(word.as_str(), "CONSTRUCTOR" | "DESTRUCTOR"))
        {
            Ok(None)
        } else {
            self.function_signature_from(&line[function + 2..])
                .map(Some)
        }
    }

    fn member_function_parameters(&self) -> Result<Vec<crate::ast::Parameter>, Diagnostic> {
        let line = self.line_tokens();
        let function = line
            .iter()
            .position(|token| matches!(&token.kind, TokenKind::Keyword(word) if word == "FUNCTION"))
            .ok_or_else(|| self.error("expected FUNCTION"))?;
        let header = &line[function + 2..];
        let close = header
            .iter()
            .position(|token| matches!(token.kind, TokenKind::Symbol(Symbol::RightParen)))
            .ok_or_else(|| self.error("expected ')' in function declaration"))?;
        let mut parameters = Vec::new();
        let mut index = 1;
        while index < close {
            if matches!(header[index].kind, TokenKind::Symbol(Symbol::Comma)) {
                index += 1;
                continue;
            }
            let name = match &header[index].kind {
                TokenKind::Identifier(name) => name.clone(),
                _ => return Err(self.error("expected parameter name")),
            };
            let start = header[index].span.start;
            index += 1;
            if !matches!(&header.get(index).map(|token| &token.kind), Some(TokenKind::Keyword(word)) if word == "AS")
            {
                return Err(self.error("expected AS in parameter"));
            }
            index += 1;
            let type_end = header[index..close]
                .iter()
                .position(|token| matches!(token.kind, TokenKind::Symbol(Symbol::Comma)))
                .map_or(close, |offset| index + offset);
            let type_ref = self.type_reference(&header[index..type_end]);
            parameters.push(crate::ast::Parameter {
                name,
                span: Span {
                    start,
                    end: type_ref.span.end,
                },
                type_ref,
            });
            index = type_end;
        }
        Ok(parameters)
    }

    fn block(
        &mut self,
        outer_end: &'static str,
    ) -> Result<(crate::source::Position, Vec<crate::ast::Statement>), Diagnostic> {
        match self.block_until(outer_end, false)? {
            (BlockTerm::End(end), statements) => Ok((end, statements)),
            (BlockTerm::Else | BlockTerm::Until(_), _) => unreachable!(),
        }
    }

    fn block_until(
        &mut self,
        outer_end: &'static str,
        stop_at_else: bool,
    ) -> Result<(BlockTerm, Vec<crate::ast::Statement>), Diagnostic> {
        let mut statements = Vec::new();
        loop {
            if self.eof() {
                return Err(self.error(format!("expected END {outer_end} before end of file")));
            }
            if self.newline_token() {
                self.take();
                continue;
            }
            if stop_at_else && self.keyword("ELSE") {
                return Ok((BlockTerm::Else, statements));
            }
            if outer_end == "REPEAT" && self.keyword("UNTIL") {
                let line = self.line_tokens();
                if line.len() == 1 {
                    return Err(self.error("UNTIL requires an expression"));
                }
                let condition = parse_expression(&line[1..])?;
                self.consume_to_newline()?;
                return Ok((BlockTerm::Until(condition), statements));
            }
            if self.keyword("END") {
                self.take();
                let end = self.end_word()?;
                let position = self.previous().span.end;
                if end != outer_end {
                    return Err(self.error(format!("expected END {outer_end}, found END {end}")));
                }
                self.newline()?;
                return Ok((BlockTerm::End(position), statements));
            }
            if self.keyword("WHILE") {
                statements.push(self.loop_statement("WHILE")?);
                continue;
            }
            if self.keyword("IF") {
                statements.push(self.if_statement()?);
                continue;
            }
            if self.keyword("REPEAT") {
                statements.push(self.loop_statement("REPEAT")?);
                continue;
            }
            if self.keyword("FOR") {
                statements.push(self.loop_statement("FOR")?);
                continue;
            }
            if outer_end == "CLASS" && self.function_member_start() {
                let start = self.peek().span.start;
                let name = self.member_function_name()?;
                let (visibility, is_static) = self.member_modifiers();
                let parameters = self.member_function_parameters()?;
                let signature = self.member_function_signature()?;
                self.consume_to_newline()?;
                let (end, body) = self.block("FUNCTION")?;
                let span = Span { start, end };
                statements.push(crate::ast::Statement::MemberFunction {
                    name,
                    visibility,
                    is_static,
                    parameters,
                    signature,
                    body: Some(crate::ast::Block {
                        statements: body,
                        span,
                    }),
                    span: Span { start, end },
                });
                continue;
            }
            if outer_end == "INTERFACE" && self.keyword("FUNCTION") {
                let name = self.member_function_name()?;
                let (visibility, is_static) = self.member_modifiers();
                let parameters = self.member_function_parameters()?;
                let signature = self.member_function_signature()?;
                let start = self.take().span.start;
                self.consume_to_newline()?;
                let span = Span {
                    start,
                    end: self.previous().span.end,
                };
                statements.push(crate::ast::Statement::MemberFunction {
                    name,
                    visibility,
                    is_static,
                    parameters,
                    signature,
                    body: None,
                    span,
                });
                continue;
            }
            statements.push(self.statement_node()?);
            self.consume_to_newline()?;
        }
    }

    fn loop_statement(&mut self, kind: &'static str) -> Result<crate::ast::Statement, Diagnostic> {
        let line = self.line_tokens();
        let condition = if kind == "WHILE" {
            Some(parse_expression(&line[1..])?)
        } else {
            None
        };
        let for_header = if kind == "FOR" {
            Some(self.for_header(line)?)
        } else {
            None
        };
        let start = self.take().span.start;
        self.consume_to_newline()?;
        let mut repeat_condition = None;
        let (end, body) = if kind == "REPEAT" {
            let (term, body) = self.block_until("REPEAT", false)?;
            let BlockTerm::Until(repeat) = term else {
                return Err(self.error("expected UNTIL before END REPEAT"));
            };
            repeat_condition = Some(repeat);
            self.expect_keyword("END")?;
            if self.end_word()? != "REPEAT" {
                return Err(self.error("expected END REPEAT"));
            }
            let end = self.previous().span.end;
            self.newline()?;
            (end, body)
        } else {
            self.block(kind)?
        };
        let span = Span { start, end };
        Ok(match kind {
            "WHILE" => crate::ast::Statement::While {
                condition: condition.expect("while condition"),
                body: crate::ast::Block {
                    statements: body,
                    span,
                },
                span,
            },
            "REPEAT" => crate::ast::Statement::Repeat {
                body: crate::ast::Block {
                    statements: body,
                    span,
                },
                condition: repeat_condition.expect("repeat condition"),
                span,
            },
            "FOR" => crate::ast::Statement::For {
                header: for_header.expect("for header"),
                body: crate::ast::Block {
                    statements: body,
                    span,
                },
                span,
            },
            _ => unreachable!(),
        })
    }

    fn for_header(&self, line: &[Token]) -> Result<crate::ast::ForHeader, Diagnostic> {
        let name = |token: &Token| match &token.kind {
            TokenKind::Identifier(name) => Ok(name.clone()),
            _ => Err(self.error("expected FOR variable name")),
        };
        if matches!(line.get(1).map(|token| &token.kind), Some(TokenKind::Keyword(word)) if word == "EACH")
        {
            let variable = name(
                line.get(2)
                    .ok_or_else(|| self.error("expected FOR EACH variable"))?,
            )?;
            let as_index = 3;
            if !matches!(line.get(as_index).map(|token| &token.kind), Some(TokenKind::Keyword(word)) if word == "AS")
            {
                return Err(self.error("expected AS in FOR EACH"));
            }
            let in_index = line
                .iter()
                .position(|token| matches!(&token.kind, TokenKind::Keyword(word) if word == "IN"))
                .ok_or_else(|| self.error("expected IN in FOR EACH"))?;
            return Ok(crate::ast::ForHeader::Each {
                variable,
                type_ref: self.type_reference(&line[as_index + 1..in_index]),
                iterable: parse_expression(&line[in_index + 1..])?,
            });
        }
        let variable = name(
            line.get(1)
                .ok_or_else(|| self.error("expected FOR variable"))?,
        )?;
        let as_index = 2;
        let equal = line
            .iter()
            .position(|token| matches!(token.kind, TokenKind::Symbol(Symbol::Assign)))
            .ok_or_else(|| self.error("expected = in FOR"))?;
        let to = line
            .iter()
            .position(|token| matches!(&token.kind, TokenKind::Keyword(word) if word == "TO"))
            .ok_or_else(|| self.error("expected TO in FOR"))?;
        let step = line
            .iter()
            .position(|token| matches!(&token.kind, TokenKind::Keyword(word) if word == "STEP"));
        if !matches!(line.get(as_index).map(|token| &token.kind), Some(TokenKind::Keyword(word)) if word == "AS")
        {
            return Err(self.error("expected AS in FOR"));
        }
        Ok(crate::ast::ForHeader::Counted {
            variable,
            type_ref: self.type_reference(&line[as_index + 1..equal]),
            start: parse_expression(&line[equal + 1..to])?,
            end: parse_expression(&line[to + 1..step.unwrap_or(line.len())])?,
            step: step
                .map(|index| parse_expression(&line[index + 1..]))
                .transpose()?,
        })
    }

    fn if_statement(&mut self) -> Result<crate::ast::Statement, Diagnostic> {
        let line = self.line_tokens();
        let then = line
            .iter()
            .position(|token| matches!(&token.kind, TokenKind::Keyword(word) if word == "THEN"))
            .ok_or_else(|| self.error("expected THEN after IF condition"))?;
        let condition = parse_expression(&line[1..then])?;
        let start = self.take().span.start;
        self.consume_to_newline()?;
        let (mut term, body) = self.block_until("IF", true)?;
        let mut branches = vec![crate::ast::IfBranch {
            condition,
            body: crate::ast::Block {
                statements: body,
                span: Span { start, end: start },
            },
            span: Span { start, end: start },
        }];
        let mut otherwise = None;
        while matches!(term, BlockTerm::Else) {
            self.take();
            if self.keyword("IF") {
                self.take();
                let line = self.line_tokens();
                let then = line
                    .iter()
                    .position(
                        |token| matches!(&token.kind, TokenKind::Keyword(word) if word == "THEN"),
                    )
                    .ok_or_else(|| self.error("expected THEN after ELSE IF condition"))?;
                let condition = parse_expression(&line[..then])?;
                self.consume_to_newline()?;
                let (next, body) = self.block_until("IF", true)?;
                branches.push(crate::ast::IfBranch {
                    condition,
                    body: crate::ast::Block {
                        statements: body,
                        span: Span { start, end: start },
                    },
                    span: Span { start, end: start },
                });
                term = next;
            } else {
                self.newline()?;
                let (next, body) = self.block_until("IF", false)?;
                otherwise = Some(crate::ast::Block {
                    statements: body,
                    span: Span { start, end: start },
                });
                term = next;
            }
        }
        let BlockTerm::End(end) = term else {
            unreachable!()
        };
        let span = Span { start, end };
        for branch in &mut branches {
            branch.span.end = end;
            branch.body.span.end = end;
        }
        if let Some(body) = &mut otherwise {
            body.span.end = end;
        }
        Ok(crate::ast::Statement::If {
            branches,
            otherwise,
            span,
        })
    }

    #[allow(clippy::too_many_lines)] // Grammar alternatives are easier to audit together.
    fn statement_node(&self) -> Result<crate::ast::Statement, Diagnostic> {
        use crate::ast::Statement;
        let line = self.line_tokens();
        let end = line
            .last()
            .ok_or_else(|| self.error("expected statement"))?
            .span
            .end;
        let span = Span {
            start: line
                .first()
                .ok_or_else(|| self.error("expected statement"))?
                .span
                .start,
            end,
        };
        let mut field_start = 0;
        while matches!(line.get(field_start).map(|token| &token.kind), Some(TokenKind::Keyword(word)) if matches!(word.as_str(), "PUBLIC" | "PRIVATE" | "STATIC"))
        {
            field_start += 1;
        }
        if matches!(
            line.get(field_start).map(|token| &token.kind),
            Some(TokenKind::Identifier(_))
        ) && matches!(line.get(field_start + 1).map(|token| &token.kind), Some(TokenKind::Keyword(word)) if word == "AS")
        {
            let name = match &line[field_start].kind {
                TokenKind::Identifier(name) => name.clone(),
                _ => unreachable!(),
            };
            let type_ref = self.type_reference(&line[field_start + 2..]);
            if line.windows(2).any(|pair| matches!((&pair[0].kind, &pair[1].kind), (TokenKind::Keyword(word), TokenKind::Symbol(Symbol::LeftBracket)) if word == "VOID")) { return Err(self.error("VOID cannot be used as a vector element type")); }
            let initializer = line
                .iter()
                .position(|token| matches!(token.kind, TokenKind::Symbol(Symbol::Assign)))
                .map(|index| parse_expression(&line[index + 1..]))
                .transpose()?;
            return Ok(Statement::Binding {
                constant: false,
                visibility: line[..field_start]
                    .iter()
                    .find_map(|token| match &token.kind {
                        TokenKind::Keyword(word) if word == "PUBLIC" => {
                            Some(crate::ast::Visibility::Public)
                        }
                        TokenKind::Keyword(word) if word == "PRIVATE" => {
                            Some(crate::ast::Visibility::Private)
                        }
                        _ => None,
                    }),
                is_static: line[..field_start].iter().any(
                    |token| matches!(&token.kind, TokenKind::Keyword(word) if word == "STATIC"),
                ),
                name,
                type_ref,
                initialized: initializer.is_some(),
                initializer,
                span,
            });
        }
        if self.keyword("LET") || self.keyword("CONST") {
            let as_index = line
                .iter()
                .position(|token| matches!(&token.kind, TokenKind::Keyword(word) if word == "AS"))
                .ok_or_else(|| self.error("a binding declaration requires AS TYPE"))?;
            let name = match &line
                .get(1)
                .ok_or_else(|| self.error("expected binding name"))?
                .kind
            {
                TokenKind::Identifier(name) => name.clone(),
                _ => return Err(self.error("expected binding name")),
            };
            let type_ref = self.type_reference(&line[as_index + 1..]);
            if line.windows(2).any(|pair| matches!((&pair[0].kind, &pair[1].kind), (TokenKind::Keyword(word), TokenKind::Symbol(Symbol::LeftBracket)) if word == "VOID")) { return Err(self.error("VOID cannot be used as a vector element type")); }
            let has_initializer = line
                .iter()
                .any(|token| matches!(token.kind, TokenKind::Symbol(Symbol::Assign)));
            let initializer = line
                .iter()
                .position(|token| matches!(token.kind, TokenKind::Symbol(Symbol::Assign)))
                .map(|index| parse_expression(&self.tokens[self.index + index + 1..]))
                .transpose()?;
            if self.keyword("CONST") && !has_initializer {
                return Err(self.error("CONST requires an initializer"));
            }
            Ok(Statement::Binding {
                constant: self.keyword("CONST"),
                visibility: None,
                is_static: false,
                name,
                type_ref,
                initialized: has_initializer,
                initializer,
                span,
            })
        } else if self.keyword("RETURN") {
            let value = if line.len() == 1 {
                None
            } else {
                Some(parse_expression(&line[1..])?)
            };
            Ok(Statement::Return { value, span })
        } else if self.keyword("PRINT") {
            Ok(Statement::Print {
                values: Self::expression_list(&line[1..])?,
                span,
            })
        } else if self.keyword("DELETE") {
            if line.len() == 1 {
                return Err(self.error("DELETE requires an expression"));
            }
            Ok(Statement::Delete {
                value: parse_expression(&line[1..])?,
                span,
            })
        } else if self.keyword("STOP") {
            if line.len() == 1 {
                return Err(self.error("STOP requires an exit-code expression"));
            }
            Ok(Statement::Stop {
                code: parse_expression(&line[1..])?,
                span,
            })
        } else if matches!(line.first().map(|token| &token.kind), Some(TokenKind::Keyword(word)) if matches!(word.as_str(), "IF" | "WHILE" | "REPEAT" | "FOR" | "EXIT" | "CONTINUE"))
        {
            if line.len() != 2
                || !matches!(&line[1].kind, TokenKind::Keyword(word) if matches!(word.as_str(), "FOR" | "WHILE" | "REPEAT"))
            {
                return Err(self.error("EXIT and CONTINUE must name FOR, WHILE, or REPEAT"));
            }
            Ok(Statement::Control {
                kind: text(&line[0]),
                target: text(&line[1]),
                span,
            })
        } else if line.iter().any(|token| {
            matches!(
                token.kind,
                TokenKind::Symbol(
                    Symbol::Assign
                        | Symbol::PlusAssign
                        | Symbol::MinusAssign
                        | Symbol::StarAssign
                        | Symbol::PowerAssign
                        | Symbol::SlashAssign
                        | Symbol::PercentAssign
                )
            )
        }) {
            let operator = line
                .iter()
                .position(|token| {
                    matches!(
                        token.kind,
                        TokenKind::Symbol(
                            Symbol::Assign
                                | Symbol::PlusAssign
                                | Symbol::MinusAssign
                                | Symbol::StarAssign
                                | Symbol::PowerAssign
                                | Symbol::SlashAssign
                                | Symbol::PercentAssign
                        )
                    )
                })
                .expect("assignment operator");
            if operator == 0 || operator + 1 == line.len() {
                return Err(self.error("assignment requires a target and expression"));
            }
            let target = parse_expression(&line[..operator])?;
            if !matches!(
                target.kind,
                ExpressionKind::Name { .. }
                    | ExpressionKind::Member { .. }
                    | ExpressionKind::Index { .. }
            ) {
                return Err(
                    self.error("an assignment target must be an identifier, member, or index")
                );
            }
            Ok(Statement::Assignment {
                target,
                operator: text(&line[operator]),
                value: parse_expression(&line[operator + 1..])?,
                span,
            })
        } else {
            let expression = parse_expression(line)?;
            if !matches!(expression.kind, ExpressionKind::Call { .. }) {
                return Err(self.error("expected a call statement"));
            }
            Ok(Statement::Call { expression, span })
        }
    }

    fn expression_list(tokens: &[Token]) -> Result<Vec<Expression>, Diagnostic> {
        if tokens.is_empty() {
            return Ok(Vec::new());
        }
        let mut values = Vec::new();
        let mut start = 0;
        for (index, token) in tokens.iter().enumerate() {
            if matches!(token.kind, TokenKind::Symbol(Symbol::Comma)) {
                values.push(parse_expression(&tokens[start..index])?);
                start = index + 1;
            }
        }
        values.push(parse_expression(&tokens[start..])?);
        Ok(values)
    }

    fn type_reference(&self, tokens: &[Token]) -> crate::ast::TypeReference {
        let mut alternatives = Vec::new();
        let mut start = 0;
        let end = tokens
            .iter()
            .position(|token| matches!(token.kind, TokenKind::Symbol(Symbol::Assign)))
            .unwrap_or(tokens.len());
        for index in (0..end).chain(std::iter::once(end)) {
            if index != end
                && !matches!(&tokens[index].kind, TokenKind::Keyword(word) if word == "OR")
            {
                continue;
            }
            let part = &tokens[start..index];
            if let Some(first) = part.first() {
                alternatives.push(crate::ast::TypeAtom {
                    name: text(first),
                    parts: part.iter().skip(1).map(text).collect(),
                    span: Span {
                        start: first.span.start,
                        end: part.last().expect("non-empty type part").span.end,
                    },
                });
            }
            start = index + 1;
        }
        let span = alternatives
            .first()
            .map_or(self.peek().span, |atom| crate::source::Span {
                start: atom.span.start,
                end: alternatives
                    .last()
                    .expect("non-empty alternatives")
                    .span
                    .end,
            });
        crate::ast::TypeReference { alternatives, span }
    }

    fn line_tokens(&self) -> &'a [Token] {
        let end = self.tokens[self.index..]
            .iter()
            .position(|token| matches!(token.kind, TokenKind::Newline | TokenKind::Eof))
            .unwrap_or(0);
        &self.tokens[self.index..self.index + end]
    }

    fn function_member_start(&self) -> bool {
        let mut offset = 0;
        while matches!(self.tokens.get(self.index + offset).map(|token| &token.kind),
            Some(TokenKind::Keyword(word)) if matches!(word.as_str(), "PUBLIC" | "PRIVATE" | "STATIC"))
        {
            offset += 1;
        }
        matches!(self.tokens.get(self.index + offset).map(|token| &token.kind), Some(TokenKind::Keyword(word)) if word == "FUNCTION")
    }

    fn member_modifiers(&self) -> (Option<crate::ast::Visibility>, bool) {
        let line = self.line_tokens();
        let visibility = line.iter().find_map(|token| match &token.kind {
            TokenKind::Keyword(word) if word == "PUBLIC" => Some(crate::ast::Visibility::Public),
            TokenKind::Keyword(word) if word == "PRIVATE" => Some(crate::ast::Visibility::Private),
            _ => None,
        });
        let is_static = line
            .iter()
            .any(|token| matches!(&token.kind, TokenKind::Keyword(word) if word == "STATIC"));
        (visibility, is_static)
    }

    fn member_function_name(&self) -> Result<String, Diagnostic> {
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

    fn end_word(&mut self) -> Result<&'static str, Diagnostic> {
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
    fn consume_to_newline(&mut self) -> Result<(), Diagnostic> {
        while !self.newline_token() && !self.eof() {
            self.take();
        }
        self.newline().map(|_| ())
    }
    fn newline(&mut self) -> Result<crate::source::Position, Diagnostic> {
        if self.newline_token() {
            Ok(self.take().span.end)
        } else {
            Err(self.error("expected end of line"))
        }
    }
    fn newlines(&mut self) {
        while self.newline_token() {
            self.take();
        }
    }
    fn identifier_name(&mut self) -> Result<String, Diagnostic> {
        match &self.peek().kind {
            TokenKind::Identifier(name) => {
                let name = name.clone();
                self.take();
                Ok(name)
            }
            _ => Err(self.error("expected identifier")),
        }
    }
    fn expect_keyword(&mut self, word: &str) -> Result<(), Diagnostic> {
        if self.keyword(word) {
            self.take();
            Ok(())
        } else {
            Err(self.error(format!("expected {word}")))
        }
    }
    fn expect_symbol(&mut self, symbol: Symbol) -> Result<(), Diagnostic> {
        if self.symbol(symbol) {
            self.take();
            Ok(())
        } else {
            Err(self.error("expected punctuation"))
        }
    }
    fn keyword(&self, word: &str) -> bool {
        matches!(&self.peek().kind, TokenKind::Keyword(value) if value == word)
    }
    fn symbol(&self, symbol: Symbol) -> bool {
        matches!(self.peek().kind, TokenKind::Symbol(value) if value == symbol)
    }
    fn newline_token(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Newline)
    }
    fn eof(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Eof)
    }
    fn peek(&self) -> &'a Token {
        &self.tokens[self.index]
    }
    fn take(&mut self) -> &'a Token {
        let token = self.peek();
        self.index += 1;
        token
    }
    fn previous(&self) -> &'a Token {
        &self.tokens[self.index - 1]
    }
    fn error(&self, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            code: "E0100",
            message: message.into(),
            span: self.peek().span,
        }
    }
}

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
