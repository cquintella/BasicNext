#![allow(clippy::wildcard_imports)]
use super::*;

impl Parser<'_> {
    pub(crate) fn block_until(
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

    pub(crate) fn loop_statement(
        &mut self,
        kind: &'static str,
    ) -> Result<crate::ast::Statement, Diagnostic> {
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

    pub(crate) fn for_header(&self, line: &[Token]) -> Result<crate::ast::ForHeader, Diagnostic> {
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
                type_ref: self.type_reference(&line[as_index + 1..in_index], false)?,
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
            type_ref: self.type_reference(&line[as_index + 1..equal], false)?,
            start: parse_expression(&line[equal + 1..to])?,
            end: parse_expression(&line[to + 1..step.unwrap_or(line.len())])?,
            step: step
                .map(|index| parse_expression(&line[index + 1..]))
                .transpose()?,
        })
    }

    pub(crate) fn if_statement(&mut self) -> Result<crate::ast::Statement, Diagnostic> {
        let line = self.line_tokens();
        let then = line
            .iter()
            .position(|token| matches!(&token.kind, TokenKind::Keyword(word) if word == "THEN"))
            .ok_or_else(|| self.error("expected THEN after IF condition"))?;
        let condition = parse_expression(&line[1..then])?;
        let start = self.take().span.start;
        if then + 1 < line.len() {
            let else_index = line[then + 1..]
                .iter()
                .position(|token| matches!(&token.kind, TokenKind::Keyword(word) if word == "ELSE"))
                .map(|index| index + then + 1);
            let true_end = else_index.unwrap_or(line.len());
            let true_statement = self.inline_simple_statement(&line[then + 1..true_end])?;
            let otherwise = if let Some(index) = else_index {
                Some(crate::ast::Block {
                    statements: vec![self.inline_simple_statement(&line[index + 1..])?],
                    span: Span {
                        start,
                        end: line.last().expect("if line").span.end,
                    },
                })
            } else {
                None
            };
            let end = line.last().expect("if line").span.end;
            self.consume_to_newline()?;
            return Ok(crate::ast::Statement::If {
                branches: vec![crate::ast::IfBranch {
                    condition,
                    body: crate::ast::Block {
                        statements: vec![true_statement],
                        span: Span { start, end },
                    },
                    span: Span { start, end },
                }],
                otherwise,
                span: Span { start, end },
            });
        }
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

    pub(crate) fn inline_simple_statement(
        &self,
        tokens: &[Token],
    ) -> Result<crate::ast::Statement, Diagnostic> {
        if tokens.is_empty() {
            return Err(self.error("single-line IF requires a statement after THEN/ELSE"));
        }
        if tokens.iter().any(|token| {
            matches!(&token.kind, TokenKind::Keyword(word) if matches!(word.as_str(), "IF" | "WHILE" | "REPEAT" | "FOR"))
        }) {
            return Err(self.error("single-line IF branches cannot contain compound statements"));
        }
        let mut owned = tokens.to_vec();
        let end = owned.last().expect("non-empty inline statement").span.end;
        owned.push(Token {
            kind: TokenKind::Newline,
            span: Span { start: end, end },
        });
        let parser = Parser {
            tokens: &owned,
            index: 0,
            source_name: self.source_name.clone(),
        };
        parser.statement_node()
    }
}
