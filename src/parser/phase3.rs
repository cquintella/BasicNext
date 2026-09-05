#![allow(clippy::wildcard_imports)]
use super::*;

impl<'a> Parser<'a> {
    #[allow(clippy::too_many_lines)] // Grammar alternatives are easier to audit together.
    pub(crate) fn statement_node(&self) -> Result<crate::ast::Statement, Diagnostic> {
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
            let type_ref = self.type_reference(&line[field_start + 2..], false)?;
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
                additional_names: Vec::new(),
                additional_name_spans: Vec::new(),
                type_ref,
                initialized: initializer.is_some(),
                initializer,
                additional_initializers: Vec::new(),
                span,
            });
        }
        if self.keyword("LET") || self.keyword("CONST") {
            let as_index = line
                .iter()
                .position(|token| matches!(&token.kind, TokenKind::Keyword(word) if word == "AS"))
                .ok_or_else(|| self.error("a binding declaration requires AS TYPE"))?;
            let name_parts = line[1..as_index]
                .split(|token| matches!(token.kind, TokenKind::Symbol(Symbol::Comma)))
                .map(|part| match part {
                    [
                        Token {
                            kind: TokenKind::Identifier(name),
                            span,
                        },
                    ] => Ok((name.clone(), *span)),
                    _ => Err(self.error("expected binding name")),
                })
                .collect::<Result<Vec<_>, _>>()?;
            let (name, _) = name_parts
                .first()
                .cloned()
                .ok_or_else(|| self.error("expected binding name"))?;
            let additional_names = name_parts
                .iter()
                .skip(1)
                .map(|(name, _)| name.clone())
                .collect::<Vec<_>>();
            let additional_name_spans = name_parts
                .iter()
                .skip(1)
                .map(|(_, span)| *span)
                .collect::<Vec<_>>();
            if self.keyword("CONST") && !additional_names.is_empty() {
                return Err(self.error("CONST accepts one binding name"));
            }
            let type_ref = self.type_reference(&line[as_index + 1..], true)?;
            if line.windows(2).any(|pair| matches!((&pair[0].kind, &pair[1].kind), (TokenKind::Keyword(word), TokenKind::Symbol(Symbol::LeftBracket)) if word == "VOID")) { return Err(self.error("VOID cannot be used as a vector element type")); }
            let has_initializer = line
                .iter()
                .any(|token| matches!(token.kind, TokenKind::Symbol(Symbol::Assign)));
            let initializer_tokens = line
                .iter()
                .position(|token| matches!(token.kind, TokenKind::Symbol(Symbol::Assign)))
                .map(|index| &line[index + 1..]);
            let initializers = initializer_tokens
                .map(Self::expression_list)
                .transpose()?
                .unwrap_or_default();
            if !initializers.is_empty() && initializers.len() != additional_names.len() + 1 {
                return Err(self.error("initializer count must match binding count"));
            }
            let mut initializers = initializers.into_iter();
            let initializer = initializers.next();
            let additional_initializers = initializers.collect();
            if self.keyword("CONST") && !has_initializer {
                return Err(self.error("CONST requires an initializer"));
            }
            Ok(Statement::Binding {
                constant: self.keyword("CONST"),
                visibility: None,
                is_static: false,
                name,
                additional_names,
                additional_name_spans,
                type_ref,
                initialized: has_initializer,
                initializer,
                additional_initializers,
                span,
            })
        } else if self.keyword("RETURN") {
            let value = if line.len() == 1 {
                None
            } else {
                Some(parse_expression(&line[1..])?)
            };
            Ok(Statement::Return { value, span })
        } else if self.keyword("INPUT") {
            let comma = line
                .iter()
                .position(|token| matches!(token.kind, TokenKind::Symbol(Symbol::Comma)));
            let (prompt, target_tokens) = if let Some(comma) = comma {
                if comma <= 1 || comma + 1 >= line.len() {
                    return Err(self.error("INPUT prompt requires a target"));
                }
                (
                    Some(Box::new(parse_expression(&line[1..comma])?)),
                    &line[comma + 1..],
                )
            } else {
                if line.len() != 2 {
                    return Err(self.error("INPUT requires a target or prompt and target"));
                }
                (None, &line[1..])
            };
            let target = parse_expression(target_tokens)?;
            if !matches!(
                target.kind,
                ExpressionKind::Name { .. }
                    | ExpressionKind::Member { .. }
                    | ExpressionKind::Index { .. }
            ) {
                return Err(self.error("INPUT target must be assignable"));
            }
            Ok(Statement::Assignment {
                target,
                operator: "Assign".into(),
                value: Expression {
                    kind: ExpressionKind::Input { prompt },
                    span,
                },
                span,
            })
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
        } else if Self::assignment_operator(line).is_some() {
            let operator = Self::assignment_operator(line).expect("assignment operator");
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

    pub(crate) fn assignment_operator(tokens: &[Token]) -> Option<usize> {
        let mut depth = 0;
        tokens.iter().position(|token| match token.kind {
            TokenKind::Symbol(Symbol::LeftParen | Symbol::LeftBracket) => {
                depth += 1;
                false
            }
            TokenKind::Symbol(Symbol::RightParen | Symbol::RightBracket) => {
                depth -= 1;
                false
            }
            TokenKind::Symbol(
                Symbol::Assign
                | Symbol::PlusAssign
                | Symbol::MinusAssign
                | Symbol::StarAssign
                | Symbol::PowerAssign
                | Symbol::SlashAssign
                | Symbol::PercentAssign,
            ) => depth == 0,
            _ => false,
        })
    }

    pub(crate) fn valid_class_header(tokens: &[Token]) -> bool {
        let mut index = 0;
        let named_type = |index: &mut usize| {
            if !matches!(
                tokens.get(*index).map(|token| &token.kind),
                Some(TokenKind::Identifier(_))
            ) {
                return false;
            }
            *index += 1;
            while matches!(
                tokens.get(*index).map(|token| &token.kind),
                Some(TokenKind::Symbol(Symbol::Dot))
            ) {
                *index += 1;
                if !matches!(
                    tokens.get(*index).map(|token| &token.kind),
                    Some(TokenKind::Identifier(_))
                ) {
                    return false;
                }
                *index += 1;
            }
            true
        };
        if matches!(tokens.get(index).map(|token| &token.kind), Some(TokenKind::Keyword(word)) if word == "EXTENDS")
        {
            index += 1;
            if !named_type(&mut index) {
                return false;
            }
        }
        if matches!(tokens.get(index).map(|token| &token.kind), Some(TokenKind::Keyword(word)) if word == "IMPLEMENTS")
        {
            index += 1;
            if !named_type(&mut index) {
                return false;
            }
            while matches!(
                tokens.get(index).map(|token| &token.kind),
                Some(TokenKind::Symbol(Symbol::Comma))
            ) {
                index += 1;
                if !named_type(&mut index) {
                    return false;
                }
            }
        }
        index == tokens.len()
    }

    pub(crate) fn expression_list(tokens: &[Token]) -> Result<Vec<Expression>, Diagnostic> {
        if tokens.is_empty() {
            return Ok(Vec::new());
        }
        let mut values = Vec::new();
        let mut start = 0;
        let mut depth = 0i32;
        for (index, token) in tokens.iter().enumerate() {
            match token.kind {
                TokenKind::Symbol(Symbol::LeftParen | Symbol::LeftBracket) => depth += 1,
                TokenKind::Symbol(Symbol::RightParen | Symbol::RightBracket) => depth -= 1,
                TokenKind::Symbol(Symbol::Comma) if depth == 0 => {
                    values.push(parse_expression(&tokens[start..index])?);
                    start = index + 1;
                }
                _ => {}
            }
        }
        values.push(parse_expression(&tokens[start..])?);
        Ok(values)
    }

    pub(crate) fn type_reference(
        &self,
        tokens: &[Token],
        allow_expression_dimensions: bool,
    ) -> Result<crate::ast::TypeReference, Diagnostic> {
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
                let name = text(first);
                let dimensions = if matches!(name.as_str(), "POINTER" | "FUNCTION") {
                    Vec::new()
                } else {
                    self.vector_dimensions(part, allow_expression_dimensions)?
                };
                alternatives.push(crate::ast::TypeAtom {
                    name,
                    parts: part.iter().skip(1).map(text).collect(),
                    dimensions,
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
        Ok(crate::ast::TypeReference { alternatives, span })
    }

    pub(crate) fn vector_dimensions(
        &self,
        tokens: &[Token],
        allow_expressions: bool,
    ) -> Result<Vec<crate::ast::VectorDimension>, Diagnostic> {
        let mut dimensions = Vec::new();
        let mut index = 0;
        while index < tokens.len() {
            if !matches!(tokens[index].kind, TokenKind::Symbol(Symbol::LeftBracket)) {
                index += 1;
                continue;
            }
            let open = index;
            let mut depth = 1_u32;
            index += 1;
            let content_start = index;
            while index < tokens.len() && depth != 0 {
                match tokens[index].kind {
                    TokenKind::Symbol(Symbol::LeftBracket) => depth += 1,
                    TokenKind::Symbol(Symbol::RightBracket) => depth -= 1,
                    _ => {}
                }
                if depth != 0 {
                    index += 1;
                }
            }
            if depth != 0 {
                return Err(self.error("expected ']' in vector type"));
            }
            let content = &tokens[content_start..index];
            let span = Span {
                start: tokens[open].span.start,
                end: tokens[index].span.end,
            };
            match content {
                [] => {}
                [
                    Token {
                        kind: TokenKind::Integer(value),
                        ..
                    },
                ] => dimensions.push(crate::ast::VectorDimension::Literal {
                    value: value.clone(),
                    span,
                }),
                _ if allow_expressions => dimensions.push(crate::ast::VectorDimension::Expression(
                    parse_expression(content)?,
                )),
                _ => {
                    return Err(self.error(
                        "vector dimensions in signatures and fields require an integer literal",
                    ));
                }
            }
            index += 1;
        }
        Ok(dimensions)
    }

    pub(crate) fn line_tokens(&self) -> &'a [Token] {
        let end = self.tokens[self.index..]
            .iter()
            .position(|token| matches!(token.kind, TokenKind::Newline | TokenKind::Eof))
            .unwrap_or(0);
        &self.tokens[self.index..self.index + end]
    }

    pub(crate) fn function_member_start(&self) -> bool {
        let mut offset = 0;
        while matches!(self.tokens.get(self.index + offset).map(|token| &token.kind),
            Some(TokenKind::Keyword(word)) if matches!(word.as_str(), "PUBLIC" | "PRIVATE" | "STATIC"))
        {
            offset += 1;
        }
        matches!(self.tokens.get(self.index + offset).map(|token| &token.kind), Some(TokenKind::Keyword(word)) if word == "FUNCTION")
    }
}
