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
            if !self.terminated() {
                self.release_temporaries(None);
            }
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
                let weak = type_ref
                    .alternatives
                    .first()
                    .is_some_and(|atom| atom.name == "WEAK");
                for (symbol, initializer, binding_span) in bindings {
                    let mut value = if let Some(initializer) = initializer {
                        let value = self.expression(initializer)?;
                        self.patch_await_type(value, ty.clone());
                        value
                    } else {
                        self.default_value(ty.clone(), type_ref, binding_span)?
                    };
                    if weak {
                        // A weak binding owns nothing.
                        self.weak_locals.insert(symbol);
                    } else if self.is_arc(&ty) {
                        value = self.owned_value(value, binding_span);
                        self.declare_owner(symbol, ty.clone(), binding_span);
                    }
                    self.emit(Instruction::Store {
                        previous: None,
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
            Statement::Return { value, span } => {
                let mut value = value
                    .as_ref()
                    .map(|value| self.expression(value))
                    .transpose()?;
                // The caller receives an owned reference; then every open
                // scope releases its locals.
                value = value.map(|value| self.owned_value(value, *span));
                self.release_scopes(0);
                self.terminate(Terminator::Return { value });
            }
            Statement::Stop { code, .. } => {
                let code = self.expression(code)?;
                // 0.6.md, "`STOP`": this function's locals are released;
                // each caller releases its own after the call (`stop_check`).
                self.release_scopes(0);
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
                self.release_scopes(targets.scope_depth);
                self.terminate(Terminator::Jump {
                    target: destination,
                });
            }
            Statement::Release { value, span } => self.release_statement(value, *span)?,
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
        let place = self.assignment_place(target)?;
        let target_type = type_at(self.model, target.span)?;
        let weak_target = matches!(&place, AssignPlace::Binding { symbol, .. } if self.weak_locals.contains(symbol))
            || self.is_weak_field(target);
        // Explicit ownership: the write takes an owned value and gives back
        // the previous content, which is released right after.
        let (value, previous) = if self.is_arc(&target_type) && !weak_target {
            (self.owned_value(value, span), Some(self.value()))
        } else {
            (value, None)
        };
        self.write_place(place, value, previous, &target_type, span);
        if let Some(previous) = previous {
            self.emit(Instruction::Release {
                value: previous,
                destructor: None,
                span,
            });
        }
        Ok(())
    }

    /// Emits the write instruction for `place`.
    #[allow(clippy::too_many_lines)] // One arm per assignable place.
    fn write_place(
        &mut self,
        place: AssignPlace,
        value: ValueId,
        previous: Option<ValueId>,
        ty: &Type,
        span: Span,
    ) {
        match place {
            AssignPlace::Binding { symbol, indices } if indices.is_empty() => {
                self.emit(Instruction::Store {
                    previous,
                    symbol,
                    value,
                    ty: ty.clone(),
                    span,
                });
            }
            AssignPlace::Binding { symbol, indices } => {
                self.emit(Instruction::SetIndex {
                    previous,
                    symbol,
                    indices,
                    value,
                    ty: ty.clone(),
                    span,
                });
            }
            AssignPlace::Member {
                object,
                name,
                owner,
            } => {
                self.emit(Instruction::SetMember {
                    previous,
                    object,
                    field: None,
                    name,
                    owner,
                    value,
                    ty: ty.clone(),
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
                    previous,
                    object,
                    field: None,
                    name,
                    owner,
                    indices,
                    value,
                    ty: ty.clone(),
                    span,
                });
            }
            AssignPlace::Field {
                symbol,
                root_owner,
                path,
            } => {
                self.emit(Instruction::SetField {
                    previous,
                    symbol,
                    root_owner,
                    path,
                    fields: None,
                    value,
                    ty: ty.clone(),
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
                    previous,
                    symbol,
                    root_owner,
                    path,
                    fields: None,
                    indices,
                    value,
                    ty: ty.clone(),
                    span,
                });
            }
            AssignPlace::Static { class, field } => {
                self.ensure_class(&class, span);
                self.emit(Instruction::StoreStatic {
                    previous,
                    class,
                    field,
                    value,
                    ty: ty.clone(),
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
                    previous,
                    class,
                    field,
                    indices,
                    value,
                    ty: ty.clone(),
                    span,
                });
            }
        }
    }

    /// `RELEASE x` (0.6.md, "`RELEASE`"): the
    /// binding ends (`EndBinding`); a local that owns a reference gives it up
    /// (`Take` + `Release`); a parameter or a weak local owns none; a local
    /// of another type hands its content to `Release`, which closes a native
    /// handle and does nothing to a primary.
    fn release_statement(&mut self, value: &Expression, span: Span) -> Result<(), Diagnostic> {
        let AssignPlace::Binding { symbol, indices } = self.assignment_place(value)? else {
            return Err(ir_error("RELEASE target is not a binding", span));
        };
        if !indices.is_empty() {
            return Err(ir_error("RELEASE target is not a binding", span));
        }
        let ty = type_at(self.model, value.span)?;
        // The diagnostics of a released binding point at its name.
        self.emit(Instruction::EndBinding {
            symbol,
            span: value.span,
        });
        let owner = self
            .scopes
            .iter()
            .flatten()
            .any(|(local, _, _)| *local == symbol);
        let borrowed = self.parameters.contains(&symbol)
            || self.weak_locals.contains(&symbol)
            || self.is_arc(&ty);
        if !owner && borrowed {
            return Ok(());
        }
        let destructor = destructor_name(self.model, value.span, &self.methods, &self.prefix);
        let taken = self.value();
        self.emit(Instruction::Take {
            destination: taken,
            symbol,
            ty,
            span,
        });
        self.emit(Instruction::Release {
            value: taken,
            destructor: if owner { None } else { destructor },
            span,
        });
        Ok(())
    }
}
