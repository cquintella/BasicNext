// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0.

#[allow(clippy::wildcard_imports)]
use super::*;

/// One `FOR EACH`: the scalar binding, its body, and the block `EXIT FOR` uses.
struct Each<'a> {
    element_type: &'a Type,
    symbol: SymbolId,
    body: &'a [Statement],
    leave: BlockId,
    span: Span,
}

/// The index of one rank of a `FOR EACH` (a row, or the scalar elements).
struct IndexCursor {
    preheader: BlockId,
    condition_block: BlockId,
    next_block: BlockId,
    index: ValueId,
    loop_index: ValueId,
    next_index: ValueId,
}

impl Builder<'_> {
    pub(super) fn short_circuit(
        &mut self,
        destination: ValueId,
        operator: &str,
        left: &Expression,
        right: &Expression,
        span: Span,
    ) -> Result<ValueId, Diagnostic> {
        let left = self.expression(left)?;
        let right_block = self.block();
        let short_block = self.block();
        let join = self.block();
        let (then_block, else_block, short_value) = if operator == "AND" {
            (right_block, short_block, false)
        } else {
            (short_block, right_block, true)
        };
        self.terminate(Terminator::Branch {
            condition: left,
            then_block,
            else_block,
        });
        self.current = short_block;
        let short_result = self.value();
        self.emit(Instruction::Constant {
            destination: short_result,
            value: Constant::Boolean(short_value),
            ty: Type::Boolean,
            span,
        });
        self.terminate(Terminator::Jump { target: join });
        self.current = right_block;
        let right = self.expression(right)?;
        let right_result = self.value();
        self.emit(Instruction::Copy {
            destination: right_result,
            source: right,
            ty: Type::Boolean,
            span,
        });
        let right_end = self.current;
        self.terminate(Terminator::Jump { target: join });
        self.current = join;
        self.emit(Instruction::Phi {
            destination,
            incoming: vec![(short_block, short_result), (right_end, right_result)],
            ty: Type::Boolean,
            span,
        });
        Ok(destination)
    }

    pub(super) fn type_test(&mut self, expression: &Expression) -> Result<ValueId, Diagnostic> {
        let value = match &expression.kind {
            ExpressionKind::TypeTest { type_ref } => {
                let written = type_ref
                    .alternatives
                    .first()
                    .map(type_test_name)
                    .ok_or_else(|| ir_error("invalid IS type test", expression.span))?;
                // HOST types are tested under the canonical name semantic
                // analysis resolved (`HOST.Net.Address` for `N.Address`,
                // `FS.File` for `Disk.File`).
                match self.model.expression(expression.span) {
                    Some(resolved)
                        if resolved.type_name.starts_with("HOST.")
                            || resolved.type_name == "FS.File" =>
                    {
                        resolved.type_name.clone()
                    }
                    // An imported type is tested by identity, whatever the import
                    // alias: a library handle (`Log.Logger`, a provider module) by
                    // its own name (K35), a BN class (`Str.Tokenizer`) by
                    // `#<module>.<Name>`, the identity its objects and the class
                    // model carry (K41).
                    Some(resolved) if let Type::ImportedNamed { module, name } = &resolved.ty => {
                        if self.model.standard_modules.contains(module) {
                            name.clone()
                        } else {
                            format!("#{}.{name}", module.0)
                        }
                    }
                    // `POINTER TO Log.Logger` is a pointer whose element is
                    // the imported type. The test keeps `POINTER TO` and uses
                    // that element's emitted name, not the import alias.
                    Some(resolved)
                        if let Type::Pointer { element, .. } = &resolved.ty
                            && let Type::ImportedNamed { module, name } = element.as_ref() =>
                    {
                        let identity = if self.model.standard_modules.contains(module) {
                            name.clone()
                        } else {
                            format!("#{}.{name}", module.0)
                        };
                        pointer_test_with_identity(&written, &identity)
                    }
                    _ => written,
                }
            }
            ExpressionKind::Name { name } | ExpressionKind::Literal(Literal::TypeName(name)) => {
                name.clone()
            }
            ExpressionKind::Literal(Literal::Null) => "NULL".into(),
            ExpressionKind::Literal(Literal::NotAvailable) => "NA".into(),
            ExpressionKind::Literal(Literal::EndOfFile) => "EOF".into(),
            ExpressionKind::Literal(Literal::Special(value)) => value.clone(),
            ExpressionKind::Unary { operator, operand } if operator == "Minus" => {
                let ExpressionKind::Literal(Literal::Special(value)) = &operand.kind else {
                    return Err(ir_error("invalid IS test", expression.span));
                };
                format!("-{value}")
            }
            _ => return Err(ir_error("invalid IS test", expression.span)),
        };
        let destination = self.value();
        self.emit(Instruction::Constant {
            destination,
            value: Constant::Type(value.clone()),
            ty: Type::TypeName(value.clone()),
            span: expression.span,
        });
        Ok(destination)
    }

    pub(super) fn if_statement(
        &mut self,
        branches: &[crate::ast::IfBranch],
        otherwise: Option<&AstBlock>,
    ) -> Result<(), Diagnostic> {
        let join = self.block();
        for branch in branches {
            let body = self.block();
            let next = self.block();
            let condition = self.expression(&branch.condition)?;
            self.terminate(Terminator::Branch {
                condition,
                then_block: body,
                else_block: next,
            });
            self.current = body;
            self.scoped_statements(&branch.body.statements)?;
            self.jump_if_open(join);
            self.current = next;
        }
        if let Some(otherwise) = otherwise {
            self.scoped_statements(&otherwise.statements)?;
        }
        self.jump_if_open(join);
        self.current = join;
        Ok(())
    }

    pub(super) fn while_statement(
        &mut self,
        condition: &Expression,
        body: &AstBlock,
    ) -> Result<(), Diagnostic> {
        let condition_block = self.block();
        let body_block = self.block();
        let exit = self.block();
        self.terminate(Terminator::Jump {
            target: condition_block,
        });
        self.current = condition_block;
        let value = self.expression(condition)?;
        self.terminate(Terminator::Branch {
            condition: value,
            then_block: body_block,
            else_block: exit,
        });
        self.current = body_block;
        self.loops.push(LoopTargets {
            kind: "WHILE",
            exit,
            continue_at: condition_block,
            scope_depth: self.scopes.len(),
        });
        self.scoped_statements(&body.statements)?;
        self.loops.pop();
        self.jump_if_open(condition_block);
        self.current = exit;
        Ok(())
    }

    pub(super) fn repeat_statement(
        &mut self,
        body: &AstBlock,
        condition: &Expression,
    ) -> Result<(), Diagnostic> {
        let body_block = self.block();
        let condition_block = self.block();
        let exit = self.block();
        self.terminate(Terminator::Jump { target: body_block });
        self.current = body_block;
        self.loops.push(LoopTargets {
            kind: "REPEAT",
            exit,
            continue_at: condition_block,
            scope_depth: self.scopes.len(),
        });
        self.scoped_statements(&body.statements)?;
        self.loops.pop();
        self.jump_if_open(condition_block);
        self.current = condition_block;
        let value = self.expression(condition)?;
        self.terminate(Terminator::Branch {
            condition: value,
            then_block: exit,
            else_block: body_block,
        });
        self.current = exit;
        Ok(())
    }

    #[allow(clippy::too_many_lines)] // Counted and FOR EACH lowering share loop target handling.
    pub(super) fn for_statement(
        &mut self,
        header: &ForHeader,
        body: &AstBlock,
        span: Span,
    ) -> Result<(), Diagnostic> {
        match header {
            ForHeader::Counted {
                type_ref,
                variable_span,
                start,
                end,
                step,
                ..
            } => {
                let binding_span = type_ref
                    .as_ref()
                    .map_or(*variable_span, |type_ref| type_ref.span);
                let symbol = self.symbol(binding_span)?;
                let variable_type = type_at(self.model, binding_span)?;
                let start = self.expression(start)?;
                let end = self.expression(end)?;
                let step = if let Some(step) = step {
                    self.expression(step)?
                } else {
                    self.integer_one(variable_type.clone(), span)
                };
                self.emit(Instruction::Store {
                    previous: None,
                    symbol,
                    value: start,
                    ty: variable_type.clone(),
                    span,
                });
                let condition_block = self.block();
                let body_block = self.block();
                let next_block = self.block();
                let exit = self.block();
                self.terminate(Terminator::Jump {
                    target: condition_block,
                });
                self.current = condition_block;
                let current = self.load(symbol, variable_type.clone(), span);
                let condition = self.value();
                let for_condition = self.function_constant("$for_condition", span);
                self.emit(Instruction::Call {
                    destination: condition,
                    callee: for_condition,
                    arguments: vec![current, end, step],
                    ty: Type::Boolean,
                    span,
                });
                self.terminate(Terminator::Branch {
                    condition,
                    then_block: body_block,
                    else_block: exit,
                });
                self.current = body_block;
                self.loops.push(LoopTargets {
                    kind: "FOR",
                    exit,
                    continue_at: next_block,
                    scope_depth: self.scopes.len(),
                });
                self.scoped_statements(&body.statements)?;
                self.loops.pop();
                self.jump_if_open(next_block);
                self.current = next_block;
                let current = self.load(symbol, variable_type.clone(), span);
                let fits = self.value();
                let for_fits = self.function_constant("$for_fits", span);
                self.emit(Instruction::Call {
                    destination: fits,
                    callee: for_fits,
                    arguments: vec![current, step],
                    ty: Type::Boolean,
                    span,
                });
                let step_block = self.block();
                self.terminate(Terminator::Branch {
                    condition: fits,
                    then_block: step_block,
                    else_block: exit,
                });
                self.current = step_block;
                let current = self.load(symbol, variable_type.clone(), span);
                let next = self.value();
                self.emit(Instruction::Binary {
                    destination: next,
                    operator: "Plus".into(),
                    left: current,
                    right: step,
                    ty: variable_type.clone(),
                    span,
                });
                self.emit(Instruction::Store {
                    previous: None,
                    symbol,
                    value: next,
                    ty: variable_type,
                    span,
                });
                self.terminate(Terminator::Jump {
                    target: condition_block,
                });
                self.current = exit;
            }
            ForHeader::Each {
                type_ref,
                variable_span,
                iterable,
                ..
            } => {
                let binding_span = type_ref
                    .as_ref()
                    .map_or(*variable_span, |type_ref| type_ref.span);
                let symbol = self.symbol(binding_span)?;
                let vector = self.expression(iterable)?;
                let vector_ty = type_at(self.model, iterable.span)?;
                let element_type = type_at(self.model, binding_span)?;
                let exit = self.block();
                let each = Each {
                    element_type: &element_type,
                    symbol,
                    body: &body.statements,
                    leave: exit,
                    span,
                };
                self.for_each_over(vector, &vector_ty, exit, &each)?;
                self.current = exit;
            }
        }
        Ok(())
    }

    /// `FOR EACH` over `vector`. A vector of more than one dimension is stored
    /// as rows; each row is visited, then its elements, in row-major order.
    /// `finished` follows this rank (the next outer row, or `each.leave`).
    fn for_each_over(
        &mut self,
        vector: ValueId,
        vector_ty: &Type,
        finished: BlockId,
        each: &Each<'_>,
    ) -> Result<(), Diagnostic> {
        let row = match vector_ty {
            Type::Vector {
                element,
                dimensions,
            } if dimensions.len() > 1 => Some(Type::Vector {
                element: element.clone(),
                dimensions: dimensions[1..].to_vec(),
            }),
            _ => None,
        };
        let index_ty = Type::Integer(crate::types::IntegerType::Int32);
        let index = self.value();
        self.emit(Instruction::Constant {
            destination: index,
            value: Constant::Integer("0".into()),
            ty: index_ty.clone(),
            span: each.span,
        });
        let length = self.value();
        self.emit(Instruction::Length {
            destination: length,
            vector,
            span: each.span,
        });
        let condition_block = self.block();
        let body_block = self.block();
        let next_block = self.block();
        let cursor = IndexCursor {
            preheader: self.current,
            condition_block,
            next_block,
            index,
            loop_index: self.value(),
            next_index: self.value(),
        };
        self.terminate(Terminator::Jump {
            target: condition_block,
        });
        self.current = condition_block;
        let condition = self.value();
        self.emit(Instruction::Binary {
            destination: condition,
            operator: "Less".into(),
            left: cursor.loop_index,
            right: length,
            ty: Type::Boolean,
            span: each.span,
        });
        self.terminate(Terminator::Branch {
            condition,
            then_block: body_block,
            else_block: finished,
        });
        self.current = body_block;
        let element = self.value();
        let indexed = row.clone().unwrap_or_else(|| each.element_type.clone());
        self.emit(Instruction::Index {
            destination: element,
            object: vector,
            index: cursor.loop_index,
            ty: indexed,
            span: each.span,
        });
        // A row walks its own loop and branches here when that row ends, so
        // this rank always has a back edge. A scalar body may leave instead.
        let continues = if let Some(row_ty) = row {
            self.for_each_over(element, &row_ty, next_block, each)?;
            true
        } else {
            self.bind_each_element(element, each, next_block)?
        };
        self.finish_index_loop(&cursor, &index_ty, continues, each.span);
        self.current = finished;
        Ok(())
    }

    /// Binds one scalar element, runs the body, and reports whether any path
    /// reaches `next_block`. The element is its own scope when it is counted.
    fn bind_each_element(
        &mut self,
        mut element: ValueId,
        each: &Each<'_>,
        next_block: BlockId,
    ) -> Result<bool, Diagnostic> {
        let scope_depth = self.scopes.len();
        let owns_element = self.is_arc(each.element_type);
        if owns_element {
            element = self.owned_value(element, each.span);
            self.scopes.push(Vec::new());
            self.declare_owner(each.symbol, each.element_type.clone(), each.span);
        }
        self.emit(Instruction::Store {
            previous: None,
            symbol: each.symbol,
            value: element,
            ty: each.element_type.clone(),
            span: each.span,
        });
        self.loops.push(LoopTargets {
            kind: "FOR",
            exit: each.leave,
            continue_at: next_block,
            scope_depth,
        });
        self.scoped_statements(each.body)?;
        self.loops.pop();
        if owns_element {
            if !self.terminated() {
                self.release_scopes(self.scopes.len() - 1);
            }
            self.scopes.pop();
        }
        let continues = !self.terminated();
        self.jump_if_open(next_block);
        Ok(continues)
    }

    /// Increments the index and joins it with the value from the preheader.
    fn finish_index_loop(
        &mut self,
        cursor: &IndexCursor,
        index_ty: &Type,
        continues: bool,
        span: Span,
    ) {
        self.current = cursor.next_block;
        let one = self.integer_one(index_ty.clone(), span);
        self.emit(Instruction::Binary {
            destination: cursor.next_index,
            operator: "Plus".into(),
            left: cursor.loop_index,
            right: one,
            ty: index_ty.clone(),
            span,
        });
        self.terminate(Terminator::Jump {
            target: cursor.condition_block,
        });
        let mut incoming = vec![(cursor.preheader, cursor.index)];
        if continues {
            incoming.push((cursor.next_block, cursor.next_index));
        }
        self.blocks[cursor.condition_block.0 as usize]
            .instructions
            .insert(
                0,
                Instruction::Phi {
                    destination: cursor.loop_index,
                    incoming,
                    ty: index_ty.clone(),
                    span,
                },
            );
    }
}
