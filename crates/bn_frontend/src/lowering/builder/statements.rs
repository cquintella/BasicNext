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
                        self.expression(initializer)?
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
                match self.assignment_place(target)? {
                    AssignPlace::Binding { symbol, indices } if indices.is_empty() => {
                        self.emit(Instruction::Store {
                            symbol,
                            value: result,
                            ty: type_at(self.model, target.span)?,
                            span: *span,
                        });
                    }
                    AssignPlace::Binding { symbol, indices } => {
                        self.emit(Instruction::SetIndex {
                            symbol,
                            indices,
                            value: result,
                            ty: type_at(self.model, target.span)?,
                            span: *span,
                        });
                    }
                    AssignPlace::Member {
                        object,
                        name,
                        owner,
                    } => {
                        self.emit(Instruction::SetMember {
                            object,
                            name,
                            owner,
                            value: result,
                            ty: type_at(self.model, target.span)?,
                            span: *span,
                        });
                    }
                    AssignPlace::Field { symbol, path } => {
                        self.emit(Instruction::SetField {
                            symbol,
                            path,
                            value: result,
                            ty: type_at(self.model, target.span)?,
                            span: *span,
                        });
                    }
                    AssignPlace::Static { class, field } => {
                        self.ensure_class(&class, *span);
                        self.emit(Instruction::StoreStatic {
                            class,
                            field,
                            value: result,
                            ty: type_at(self.model, target.span)?,
                            span: *span,
                        });
                    }
                }
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
            Statement::Delete { value, span } => {
                let deleted = self.expression(value)?;
                let destructor =
                    destructor_name(self.model, value.span, &self.methods, &self.prefix);
                self.emit(Instruction::Delete {
                    value: deleted,
                    destructor,
                    span: *span,
                });
            }
            Statement::MemberFunction { .. } => {}
        }
        Ok(())
    }
}
