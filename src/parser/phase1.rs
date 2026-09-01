#![allow(clippy::wildcard_imports)]
use super::*;

impl Parser<'_> {
    pub(crate) fn program(&mut self) -> Result<Program, Diagnostic> {
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

    pub(crate) fn import(&mut self) -> Result<Item, Diagnostic> {
        let start = self.take().span.start;
        let first = self.path_part_name()?;
        let host_import = first == "HOST";
        let mut path = vec![first];
        if host_import || self.symbol(Symbol::Dot) {
            self.expect_symbol(Symbol::Dot)?;
            let capability = self.path_part_name()?;
            if host_import {
                require_host_capability_name(&capability).map_err(|message| self.error(message))?;
            }
            path.push(capability);
            while self.symbol(Symbol::Dot) {
                self.take();
                path.push(self.identifier_name()?);
            }
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

    pub(crate) fn path_part_name(&mut self) -> Result<String, Diagnostic> {
        if self.keyword("HOST") {
            self.take();
            Ok("HOST".into())
        } else {
            self.identifier_name()
        }
    }

    pub(crate) fn declaration(&mut self) -> Result<Item, Diagnostic> {
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
        let base_class = if kind == DeclarationKind::Class {
            self.base_class()?
        } else {
            None
        };
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
        if kind != DeclarationKind::Function
            && (kind != DeclarationKind::Class && !self.line_tokens().is_empty()
                || kind == DeclarationKind::Class && !Self::valid_class_header(self.line_tokens()))
        {
            return Err(self.error("unexpected token after declaration name"));
        }
        self.consume_to_newline()?;
        let (end, statements) = self.block(kind.end_word())?;
        Ok(Item::Declaration {
            exported,
            kind,
            name,
            base_class,
            interfaces,
            signature,
            statements,
            span: Span { start, end },
        })
    }

    pub(crate) fn base_class(&self) -> Result<Option<crate::ast::TypeReference>, Diagnostic> {
        let line = self.line_tokens();
        let Some(extends) = line
            .iter()
            .position(|token| matches!(&token.kind, TokenKind::Keyword(word) if word == "EXTENDS"))
        else {
            return Ok(None);
        };
        let end = line[extends + 1..]
            .iter()
            .position(
                |token| matches!(&token.kind, TokenKind::Keyword(word) if word == "IMPLEMENTS"),
            )
            .map_or(line.len(), |offset| extends + 1 + offset);
        if extends + 1 == end {
            return Err(self.error("EXTENDS requires a class name"));
        }
        Ok(Some(self.type_reference(&line[extends + 1..end], false)?))
    }

    pub(crate) fn implemented_interfaces(&self) -> Result<Vec<String>, Diagnostic> {
        let line = self.line_tokens();
        let Some(implements) = line.iter().position(
            |token| matches!(&token.kind, TokenKind::Keyword(word) if word == "IMPLEMENTS"),
        ) else {
            return Ok(Vec::new());
        };
        let mut interfaces = Vec::new();
        let mut index = implements + 1;
        while index < line.len() {
            let TokenKind::Identifier(name) = &line[index].kind else {
                return Err(self.error("expected interface name after IMPLEMENTS"));
            };
            let mut interface = name.clone();
            index += 1;
            while matches!(
                line.get(index).map(|token| &token.kind),
                Some(TokenKind::Symbol(Symbol::Dot))
            ) {
                index += 1;
                let Some(Token {
                    kind: TokenKind::Identifier(name),
                    ..
                }) = line.get(index)
                else {
                    return Err(self.error("expected interface name after '.'"));
                };
                interface.push('.');
                interface.push_str(name);
                index += 1;
            }
            interfaces.push(interface);
            if index == line.len() {
                break;
            }
            if !matches!(line[index].kind, TokenKind::Symbol(Symbol::Comma)) {
                return Err(self.error("expected ',' between interface names"));
            }
            index += 1;
        }
        if interfaces.is_empty() {
            return Err(self.error("IMPLEMENTS requires an interface name"));
        }
        Ok(interfaces)
    }

    pub(crate) fn function_signature(&self) -> Result<crate::ast::FunctionSignature, Diagnostic> {
        self.function_signature_from(self.line_tokens())
    }

    pub(crate) fn function_signature_from(
        &self,
        line: &[Token],
    ) -> Result<crate::ast::FunctionSignature, Diagnostic> {
        let start = line
            .first()
            .map_or(self.peek().span.start, |token| token.span.start);
        let mut parentheses = 0_i32;
        let close = line
            .iter()
            .enumerate()
            .find_map(|(index, token)| {
                match token.kind {
                    TokenKind::Symbol(Symbol::LeftParen) => parentheses += 1,
                    TokenKind::Symbol(Symbol::RightParen) => {
                        parentheses -= 1;
                        if parentheses == 0 {
                            return Some(index);
                        }
                    }
                    _ => {}
                }
                None
            })
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
        if matches!(
            line.get(close.saturating_sub(1)).map(|token| &token.kind),
            Some(TokenKind::Symbol(Symbol::Comma))
        ) {
            return Err(self.error("trailing comma in FUNCTION parameters"));
        }
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
            let mut nested = 0_i32;
            let type_end = line[index..close]
                .iter()
                .enumerate()
                .find_map(|(offset, token)| {
                    match token.kind {
                        TokenKind::Symbol(Symbol::LeftParen | Symbol::LeftBracket) => nested += 1,
                        TokenKind::Symbol(Symbol::RightParen | Symbol::RightBracket) => nested -= 1,
                        TokenKind::Symbol(Symbol::Comma) if nested == 0 => {
                            return Some(index + offset);
                        }
                        _ => {}
                    }
                    None
                })
                .unwrap_or(close);
            let type_ref = self.type_reference(&line[index..type_end], false)?;
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
        let return_type = self.type_reference(&line[as_index + 1..], false)?;
        Ok(crate::ast::FunctionSignature {
            parameters,
            span: Span {
                start,
                end: return_type.span.end,
            },
            return_type,
        })
    }

    pub(crate) fn member_function_signature(
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

    pub(crate) fn member_function_parameters(
        &self,
    ) -> Result<Vec<crate::ast::Parameter>, Diagnostic> {
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
            let type_ref = self.type_reference(&header[index..type_end], false)?;
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

    pub(crate) fn block(
        &mut self,
        outer_end: &'static str,
    ) -> Result<(crate::source::Position, Vec<crate::ast::Statement>), Diagnostic> {
        match self.block_until(outer_end, false)? {
            (BlockTerm::End(end), statements) => Ok((end, statements)),
            (BlockTerm::Else | BlockTerm::Until(_), _) => unreachable!(),
        }
    }
}
