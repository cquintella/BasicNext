// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0.

#[allow(clippy::wildcard_imports)]
use super::*;

impl Builder<'_> {
    pub(crate) fn statements(&mut self, statements: &[Statement]) -> Result<(), Diagnostic> {
        for statement in statements {
            if self.terminated() {
                break;
            }
            self.statement(statement)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)] // IR cases mirror the statement AST.
    pub(super) fn statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        match statement {
            Statement::Binding {
                initializer,
                additional_names,
                additional_name_spans,
                additional_initializers,
                type_ref,
                span,
                ..
            } => {
                let ty = type_at(self.model, *span)?;
                let mut bindings = vec![(self.symbol(*span)?, initializer.as_ref(), *span)];
                bindings.extend(additional_names.iter().enumerate().map(|(index, _)| {
                    let binding_span = additional_name_spans[index];
                    (
                        self.symbol(binding_span).expect("validated binding symbol"),
                        additional_initializers.get(index),
                        binding_span,
                    )
                }));
                for (symbol, initializer, binding_span) in bindings {
                    let value = if let Some(initializer) = initializer {
                        let value = self.expression(initializer)?;
                        self.patch_await_type(value, ty.clone());
                        value
                    } else {
                        self.default_value(ty.clone(), type_ref, binding_span)?
                    };
                    self.emit(Instruction::Store {
                        symbol,
                        value,
                        ty: ty.clone(),
                        span: binding_span,
                    });
                }
            }
            Statement::Assignment {
                target,
                operator,
                value,
                span,
            } => {
                let mut result = self.expression(value)?;
                self.patch_await_type(result, type_at(self.model, target.span)?);
                if operator != "Assign" {
                    let left = self.expression(target)?;
                    let destination = self.value();
                    self.emit(Instruction::Binary {
                        destination,
                        operator: assignment_operator(operator)?.into(),
                        left,
                        right: result,
                        ty: type_at(self.model, target.span)?,
                        span: *span,
                    });
                    result = destination;
                }
                self.store_to_target(target, result, *span)?;
            }
            Statement::Print { values, span } => {
                let values = values
                    .iter()
                    .map(|value| self.expression(value))
                    .collect::<Result<Vec<_>, _>>()?;
                self.emit(Instruction::Print {
                    values,
                    span: *span,
                });
            }
            Statement::ClearScreen { console, span } => {
                let console = self.expression(console)?;
                self.emit(Instruction::ClearScreen {
                    console,
                    span: *span,
                });
            }
            Statement::Beep { console, span } => {
                let console = self.expression(console)?;
                self.emit(Instruction::Beep {
                    console,
                    span: *span,
                });
            }
            Statement::Call { expression, .. } => {
                self.expression(expression)?;
            }
            Statement::Return { value, .. } => {
                let value = value
                    .as_ref()
                    .map(|value| self.expression(value))
                    .transpose()?;
                self.terminate(Terminator::Return { value });
            }
            Statement::Stop { code, .. } => {
                let code = self.expression(code)?;
                self.terminate(Terminator::Stop { code });
            }
            Statement::If {
                branches,
                otherwise,
                ..
            } => self.if_statement(branches, otherwise.as_ref())?,
            Statement::While {
                condition, body, ..
            } => self.while_statement(condition, body)?,
            Statement::Repeat {
                body, condition, ..
            } => self.repeat_statement(body, condition)?,
            Statement::For { header, body, span } => self.for_statement(header, body, *span)?,
            Statement::Control { kind, target, span } => {
                let targets = self
                    .loops
                    .iter()
                    .rev()
                    .find(|targets| targets.kind == target)
                    .ok_or_else(|| ir_error("loop target is missing", *span))?;
                let destination = if kind == "EXIT" {
                    targets.exit
                } else {
                    targets.continue_at
                };
                self.terminate(Terminator::Jump {
                    target: destination,
                });
            }
            Statement::Release { value, span } => {
                let deleted = self.expression(value)?;
                let destructor =
                    destructor_name(self.model, value.span, &self.methods, &self.prefix);
                self.emit(Instruction::Release {
                    value: deleted,
                    destructor,
                    span: *span,
                });
            }
            Statement::MemberFunction { .. } => {}
        }
        Ok(())
    }

    /// Stores `value` into the assignment target `target` (binding, element,
    /// member, field or static). Shared by compound assignment statements
    /// and increment-expressions (0.6.1 S1').
    #[allow(clippy::too_many_lines)] // One arm per assignable place; moved verbatim from the statement.
    pub(crate) fn store_to_target(
        &mut self,
        target: &Expression,
        value: ValueId,
        span: Span,
    ) -> Result<(), Diagnostic> {
        match self.assignment_place(target)? {
            AssignPlace::Binding { symbol, indices } if indices.is_empty() => {
                self.emit(Instruction::Store {
                    symbol,
                    value,
                    ty: type_at(self.model, target.span)?,
                    span,
                });
            }
            AssignPlace::Binding { symbol, indices } => {
                self.emit(Instruction::SetIndex {
                    symbol,
                    indices,
                    value,
                    ty: type_at(self.model, target.span)?,
                    span,
                });
            }
            AssignPlace::Member {
                object,
                name,
                owner,
            } => {
                self.emit(Instruction::SetMember {
                    object,
                    field: None,
                    name,
                    owner,
                    value,
                    ty: type_at(self.model, target.span)?,
                    span,
                });
            }
            AssignPlace::MemberIndex {
                object,
                name,
                owner,
                indices,
            } => {
                self.emit(Instruction::SetMemberIndex {
                    object,
                    field: None,
                    name,
                    owner,
                    indices,
                    value,
                    ty: type_at(self.model, target.span)?,
                    span,
                });
            }
            AssignPlace::Field {
                symbol,
                root_owner,
                path,
            } => {
                self.emit(Instruction::SetField {
                    symbol,
                    root_owner,
                    path,
                    fields: None,
                    value,
                    ty: type_at(self.model, target.span)?,
                    span,
                });
            }
            AssignPlace::FieldIndex {
                symbol,
                root_owner,
                path,
                indices,
            } => {
                self.emit(Instruction::SetFieldIndex {
                    symbol,
                    root_owner,
                    path,
                    fields: None,
                    indices,
                    value,
                    ty: type_at(self.model, target.span)?,
                    span,
                });
            }
            AssignPlace::Static { class, field } => {
                self.ensure_class(&class, span);
                self.emit(Instruction::StoreStatic {
                    class,
                    field,
                    value,
                    ty: type_at(self.model, target.span)?,
                    span,
                });
            }
            AssignPlace::StaticIndex {
                class,
                field,
                indices,
            } => {
                self.ensure_class(&class, span);
                self.emit(Instruction::SetStaticIndex {
                    class,
                    field,
                    indices,
                    value,
                    ty: type_at(self.model, target.span)?,
                    span,
                });
            }
        }
        Ok(())
    }
}
