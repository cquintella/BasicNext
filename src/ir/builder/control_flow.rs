// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0.

#[allow(clippy::wildcard_imports)]
use super::*;

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
        self.emit(Instruction::Constant {
            destination,
            value: Constant::Boolean(short_value),
            ty: Type::Boolean,
            span,
        });
        self.terminate(Terminator::Jump { target: join });
        self.current = right_block;
        let right = self.expression(right)?;
        self.emit(Instruction::Copy {
            destination,
            source: right,
            ty: Type::Boolean,
            span,
        });
        self.terminate(Terminator::Jump { target: join });
        self.current = join;
        Ok(destination)
    }

    pub(super) fn type_test(&mut self, expression: &Expression) -> Result<ValueId, Diagnostic> {
        let value = match &expression.kind {
            ExpressionKind::TypeTest { type_ref } => type_ref
                .alternatives
                .first()
                .map(type_test_name)
                .ok_or_else(|| ir_error("invalid IS type test", expression.span))?,
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
            self.statements(&branch.body.statements)?;
            self.jump_if_open(join);
            self.current = next;
        }
        if let Some(otherwise) = otherwise {
            self.statements(&otherwise.statements)?;
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
        });
        self.statements(&body.statements)?;
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
        });
        self.statements(&body.statements)?;
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
                start,
                end,
                step,
                ..
            } => {
                let symbol = self.symbol(type_ref.span)?;
                let start = self.expression(start)?;
                let end = self.expression(end)?;
                let step = if let Some(step) = step {
                    self.expression(step)?
                } else {
                    self.integer_one(type_at(self.model, type_ref.span)?, span)
                };
                self.emit(Instruction::Store {
                    symbol,
                    value: start,
                    ty: type_at(self.model, type_ref.span)?,
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
                let current = self.load(symbol, type_at(self.model, type_ref.span)?, span);
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
                });
                self.statements(&body.statements)?;
                self.loops.pop();
                self.jump_if_open(next_block);
                self.current = next_block;
                let current = self.load(symbol, type_at(self.model, type_ref.span)?, span);
                let next = self.value();
                self.emit(Instruction::Binary {
                    destination: next,
                    operator: "Plus".into(),
                    left: current,
                    right: step,
                    ty: type_at(self.model, type_ref.span)?,
                    span,
                });
                self.emit(Instruction::Store {
                    symbol,
                    value: next,
                    ty: type_at(self.model, type_ref.span)?,
                    span,
                });
                self.terminate(Terminator::Jump {
                    target: condition_block,
                });
                self.current = exit;
            }
            ForHeader::Each {
                type_ref, iterable, ..
            } => {
                let symbol = self.symbol(type_ref.span)?;
                let vector = self.expression(iterable)?;
                let index = self.value();
                self.emit(Instruction::Constant {
                    destination: index,
                    value: Constant::Integer("0".into()),
                    ty: Type::Integer(crate::semantic::IntegerType::Int32),
                    span,
                });
                let length = self.value();
                self.emit(Instruction::Length {
                    destination: length,
                    vector,
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
                let condition = self.value();
                self.emit(Instruction::Binary {
                    destination: condition,
                    operator: "Less".into(),
                    left: index,
                    right: length,
                    ty: Type::Boolean,
                    span,
                });
                self.terminate(Terminator::Branch {
                    condition,
                    then_block: body_block,
                    else_block: exit,
                });
                self.current = body_block;
                let element = self.value();
                self.emit(Instruction::Index {
                    destination: element,
                    object: vector,
                    index,
                    ty: type_at(self.model, type_ref.span)?,
                    span,
                });
                self.emit(Instruction::Store {
                    symbol,
                    value: element,
                    ty: type_at(self.model, type_ref.span)?,
                    span,
                });
                self.loops.push(LoopTargets {
                    kind: "FOR",
                    exit,
                    continue_at: next_block,
                });
                self.statements(&body.statements)?;
                self.loops.pop();
                self.jump_if_open(next_block);
                self.current = next_block;
                let one =
                    self.integer_one(Type::Integer(crate::semantic::IntegerType::Int32), span);
                self.emit(Instruction::Binary {
                    destination: index,
                    operator: "Plus".into(),
                    left: index,
                    right: one,
                    ty: Type::Integer(crate::semantic::IntegerType::Int32),
                    span,
                });
                self.terminate(Terminator::Jump {
                    target: condition_block,
                });
                self.current = exit;
            }
        }
        Ok(())
    }
}
