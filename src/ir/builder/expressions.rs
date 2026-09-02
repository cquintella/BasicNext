// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0.

#[allow(clippy::wildcard_imports)]
use super::*;

impl Builder<'_> {
    #[allow(clippy::too_many_lines)] // Expression alternatives mirror the syntax AST.
    pub(crate) fn expression(&mut self, expression: &Expression) -> Result<ValueId, Diagnostic> {
        let ty = type_at(self.model, expression.span)?;
        if let ExpressionKind::New {
            type_name,
            arguments,
        } = &expression.kind
        {
            return self.allocate(type_name, arguments, ty, expression.span);
        }
        let destination = self.value();
        let instruction = match &expression.kind {
            ExpressionKind::Super => unreachable!("semantic analysis rejects bare SUPER"),
            ExpressionKind::Literal(literal) => Instruction::Constant {
                destination,
                value: constant(literal),
                ty,
                span: expression.span,
            },
            ExpressionKind::Name { name } => {
                if name == "SELF" {
                    if let Some((symbol, receiver_type)) = self.receiver.clone() {
                        Instruction::Load {
                            destination,
                            symbol,
                            ty: receiver_type,
                            span: expression.span,
                        }
                    } else {
                        Instruction::Load {
                            destination,
                            symbol: self.expression_symbol(expression)?,
                            ty,
                            span: expression.span,
                        }
                    }
                } else if matches!(ty, Type::Function { .. }) {
                    let qualified = format!("{}{name}", self.prefix);
                    if self.methods.contains(&qualified) {
                        Instruction::Constant {
                            destination,
                            value: Constant::Function(qualified),
                            ty,
                            span: expression.span,
                        }
                    } else {
                        Instruction::Load {
                            destination,
                            symbol: self.expression_symbol(expression)?,
                            ty,
                            span: expression.span,
                        }
                    }
                } else if ty == Type::HostConsole {
                    Instruction::Constant {
                        destination,
                        value: Constant::HostConsole,
                        ty,
                        span: expression.span,
                    }
                } else if ty == Type::HostArgs {
                    Instruction::Constant {
                        destination,
                        value: Constant::HostArgs,
                        ty,
                        span: expression.span,
                    }
                } else if ty == Type::HostClock {
                    Instruction::Constant {
                        destination,
                        value: Constant::Type("HOST.Clock".into()),
                        ty,
                        span: expression.span,
                    }
                } else if ty == Type::HostRandom {
                    Instruction::Constant {
                        destination,
                        value: Constant::Type("HOST.Random".into()),
                        ty,
                        span: expression.span,
                    }
                } else if ty == Type::HostFileSystem {
                    Instruction::Constant {
                        destination,
                        value: Constant::Type("HOST.FileSystem".into()),
                        ty,
                        span: expression.span,
                    }
                } else if ty == Type::HostNet {
                    Instruction::Constant {
                        destination,
                        value: Constant::Type("HOST.Net".into()),
                        ty,
                        span: expression.span,
                    }
                } else if matches!(ty, Type::TypeName(_) | Type::Module(_)) {
                    Instruction::Constant {
                        destination,
                        value: Constant::Type(name.clone()),
                        ty,
                        span: expression.span,
                    }
                } else {
                    Instruction::Load {
                        destination,
                        symbol: self.expression_symbol(expression)?,
                        ty,
                        span: expression.span,
                    }
                }
            }
            ExpressionKind::Input => Instruction::Input {
                destination,
                ty,
                span: expression.span,
            },
            ExpressionKind::HostCapability { name } => Instruction::Constant {
                destination,
                value: host_capability_constant(name, expression.span)?,
                ty,
                span: expression.span,
            },
            ExpressionKind::Length { operand } => {
                let operand_type = if matches!(operand.kind, ExpressionKind::HostCapability { ref name } if name == "Args")
                {
                    Type::HostArgs
                } else {
                    type_at(self.model, operand.span)?
                };
                let value = if operand_type == Type::HostArgs {
                    let value = self.value();
                    self.emit(Instruction::Constant {
                        destination: value,
                        value: Constant::HostArgs,
                        ty: Type::HostArgs,
                        span: operand.span,
                    });
                    value
                } else {
                    self.expression(operand)?
                };
                if let Some(length) = static_len(&operand_type) {
                    Instruction::Constant {
                        destination,
                        value: Constant::Integer(length.to_string()),
                        ty: Type::Integer(crate::semantic::IntegerType::Int32),
                        span: expression.span,
                    }
                } else {
                    Instruction::Length {
                        destination,
                        vector: value,
                        span: expression.span,
                    }
                }
            }
            ExpressionKind::SizeOf { operand } => {
                let operand_type = type_at(self.model, operand.span)?;
                let value = self.expression(operand)?;
                if let Some(size) = self.model.size_of(&operand_type) {
                    Instruction::Constant {
                        destination,
                        value: Constant::Integer(size.to_string()),
                        ty: Type::Integer(crate::semantic::IntegerType::Int32),
                        span: expression.span,
                    }
                } else {
                    Instruction::SizeOf {
                        destination,
                        value,
                        span: expression.span,
                    }
                }
            }
            ExpressionKind::TypeTest { .. } => {
                return Err(ir_error(
                    "type tests are lowered only as the right operand of IS",
                    expression.span,
                ));
            }
            ExpressionKind::Unary { operator, operand } => Instruction::Unary {
                destination,
                operator: operator.clone(),
                operand: self.expression(operand)?,
                ty,
                span: expression.span,
            },
            ExpressionKind::Binary {
                operator,
                left,
                right,
            } => {
                if matches!(operator.as_str(), "AND" | "OR") && ty == Type::Boolean {
                    return self.short_circuit(destination, operator, left, right, expression.span);
                }
                let left = self.expression(left)?;
                let right = if operator == "IS" {
                    self.type_test(right)?
                } else {
                    self.expression(right)?
                };
                Instruction::Binary {
                    destination,
                    operator: operator.clone(),
                    left,
                    right,
                    ty,
                    span: expression.span,
                }
            }
            ExpressionKind::Call { callee, arguments } => {
                if matches!(callee.kind, ExpressionKind::Super) {
                    let owner = self
                        .model
                        .expression(expression.span)
                        .and_then(|resolved| resolved.member_target.as_ref())
                        .and_then(|target| target.owner.clone())
                        .ok_or_else(|| {
                            ir_error("SUPER call has no resolved base class", expression.span)
                        })?;
                    let (receiver, receiver_type) = self
                        .receiver
                        .clone()
                        .ok_or_else(|| ir_error("SUPER call has no receiver", expression.span))?;
                    let receiver = self.load(receiver, receiver_type, expression.span);
                    let mut values = Vec::new();
                    for argument in arguments {
                        values.push(self.expression(argument)?);
                    }
                    self.emit_super_construction(&owner, receiver, values, expression.span);
                    self.emit_derived_fields(receiver, expression.span);
                    Instruction::Constant {
                        destination,
                        value: Constant::Null,
                        ty,
                        span: expression.span,
                    }
                } else {
                    let is_async = matches!(callee.kind, ExpressionKind::Member { ref name, .. } if name == "Async");
                    let is_await = matches!(callee.kind, ExpressionKind::Member { ref name, .. } if name == "Wait");
                    let (callee, arguments) = self.call_operands(callee, arguments)?;
                    if is_async {
                        let [queue, task, rest @ ..] = arguments.as_slice() else {
                            return Err(ir_error(
                                "ASYNC submission has invalid operands",
                                expression.span,
                            ));
                        };
                        Instruction::DispatchSubmit {
                            destination,
                            callee,
                            queue: *queue,
                            task: *task,
                            arguments: rest.to_vec(),
                            ty,
                            span: expression.span,
                        }
                    } else if is_await {
                        let [ticket, timeout] = arguments.as_slice() else {
                            return Err(ir_error("AWAIT has invalid operands", expression.span));
                        };
                        Instruction::DispatchAwait {
                            destination,
                            callee,
                            ticket: *ticket,
                            timeout: *timeout,
                            ty,
                            span: expression.span,
                        }
                    } else {
                        Instruction::Call {
                            destination,
                            callee,
                            arguments,
                            ty,
                            span: expression.span,
                        }
                    }
                }
            }
            ExpressionKind::Cast { value, .. } => Instruction::Cast {
                destination,
                value: self.expression(value)?,
                ty,
                span: expression.span,
            },
            ExpressionKind::Vector { values } => Instruction::Vector {
                destination,
                values: values
                    .iter()
                    .map(|value| self.expression(value))
                    .collect::<Result<Vec<_>, _>>()?,
                ty,
                span: expression.span,
            },
            ExpressionKind::Index { object, index } => {
                let object = if matches!(object.kind, ExpressionKind::HostCapability { ref name } if name == "Args")
                {
                    let value = self.value();
                    self.emit(Instruction::Constant {
                        destination: value,
                        value: Constant::HostArgs,
                        ty: Type::HostArgs,
                        span: object.span,
                    });
                    value
                } else {
                    self.expression(object)?
                };
                Instruction::Index {
                    destination,
                    object,
                    index: self.expression(index)?,
                    ty,
                    span: expression.span,
                }
            }
            ExpressionKind::Member { object, name } => {
                let object_type = type_at(self.model, object.span)?;
                if matches!(object_type, Type::Module(module) if self.model.bnmath_modules.contains(&module))
                    && let Some(value) = math_constant(name)
                {
                    let _ = self.expression(object)?;
                    Instruction::Constant {
                        destination,
                        value,
                        ty,
                        span: expression.span,
                    }
                } else if matches!(object_type, Type::HostFileSystem)
                    && let Some(value) = filesystem_constant(name)
                {
                    let _ = self.expression(object)?;
                    Instruction::Constant {
                        destination,
                        value,
                        ty,
                        span: expression.span,
                    }
                } else {
                    let owner = self
                        .model
                        .expression(expression.span)
                        .and_then(|resolved| resolved.member_target.as_ref())
                        .and_then(|target| target.owner.clone())
                        .unwrap_or_default();
                    if matches!(ty, Type::Function { .. })
                        && let Some(function) = namespace_function(&object_type, name, &self.prefix)
                    {
                        let function =
                            if matches!(object_type, Type::TypeName(_)) && !owner.is_empty() {
                                format!("{}{}.{}", self.prefix, owner, name)
                            } else {
                                function
                            };
                        let _ = self.expression(object)?;
                        if user_class_name(&object_type).is_some() {
                            let class =
                                if owner.is_empty() || !matches!(object_type, Type::TypeName(_)) {
                                    static_class_name(&object_type, &self.prefix)
                                } else {
                                    format!("{}{}", self.prefix, owner)
                                };
                            self.ensure_class(&class, expression.span);
                        }
                        Instruction::Constant {
                            destination,
                            value: Constant::Function(function),
                            ty,
                            span: expression.span,
                        }
                    } else if matches!(ty, Type::TypeName(_) | Type::ImportedTypeName { .. }) {
                        let _ = self.expression(object)?;
                        Instruction::Constant {
                            destination,
                            value: Constant::Type(name.clone()),
                            ty,
                            span: expression.span,
                        }
                    } else if is_namespace_type(&object_type) {
                        let class = if owner.is_empty() || !matches!(object_type, Type::TypeName(_))
                        {
                            static_class_name(&object_type, &self.prefix)
                        } else {
                            format!("{}{}", self.prefix, owner)
                        };
                        self.ensure_class(&class, expression.span);
                        let _ = self.expression(object)?;
                        Instruction::LoadStatic {
                            destination,
                            class,
                            field: name.clone(),
                            ty,
                            span: expression.span,
                        }
                    } else {
                        Instruction::Member {
                            destination,
                            object: self.expression(object)?,
                            name: name.clone(),
                            owner,
                            ty,
                            span: expression.span,
                        }
                    }
                }
            }
            ExpressionKind::New { .. } => {
                return Err(ir_error(
                    "NEW is lowered as an allocation sequence",
                    expression.span,
                ));
            }
        };
        self.emit(instruction);
        Ok(destination)
    }
}
