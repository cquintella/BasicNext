#![allow(clippy::wildcard_imports)]
use super::*;

impl Analyzer {
    #[allow(clippy::too_many_lines)] // Expression alternatives mirror the syntax AST.
    pub(crate) fn expression(
        &mut self,
        expression: &Expression,
        locals: &HashMap<String, Symbol>,
    ) -> Result<Type, Diagnostic> {
        let result = match &expression.kind {
            ExpressionKind::Super => Err(error(
                "INVALID_SUPER",
                "SUPER is only valid as SUPER(...) or SUPER.Name(...)",
                expression.span,
            )),
            ExpressionKind::Literal(Literal::TypeName(name)) => Err(error(
                "TYPE_NAME_AS_VALUE",
                format!("type name '{name}' is not a first-class value"),
                expression.span,
            )),
            ExpressionKind::TypeTest { .. } => Err(error(
                "TYPE_NAME_AS_VALUE",
                "a type test may appear only to the right of IS",
                expression.span,
            )),
            ExpressionKind::Literal(literal) => Ok(literal_type(literal)),
            ExpressionKind::Input => Ok(Type::Alternative(vec![Type::String, Type::EndOfFile])),
            ExpressionKind::HostCapability { name } if name == "Args" => {
                if self.executable_module {
                    Err(error(
                        "INVALID_HOST_ARGS_USE",
                        "HOST.Args is valid only in LEN(HOST.Args) or HOST.Args[index]",
                        expression.span,
                    ))
                } else {
                    Err(error(
                        "HOST_ARGS_SCOPE",
                        "HOST.Args is valid only in the executable module",
                        expression.span,
                    ))
                }
            }
            ExpressionKind::HostCapability { name } => host_capability_type(name, expression.span),
            ExpressionKind::Length { operand } => {
                if matches!(operand.kind, ExpressionKind::HostCapability { ref name } if name == "Args")
                {
                    if self.executable_module {
                        Ok(Type::Integer(IntegerType::Int32))
                    } else {
                        Err(error(
                            "HOST_ARGS_SCOPE",
                            "HOST.Args is valid only in the executable module",
                            operand.span,
                        ))
                    }
                } else {
                    let ty = self.expression(operand, locals)?;
                    length_type(&ty, operand.span)
                }
            }
            ExpressionKind::SizeOf { operand } => {
                let ty = self.expression(operand, locals)?;
                self.sizeof_type(&ty, operand.span)
            }
            ExpressionKind::Name { name } => self
                .lookup(name, locals)
                .map(|symbol| symbol.ty.clone())
                .ok_or_else(|| {
                    error(
                        "NAME_NOT_FOUND",
                        format!("name '{name}' is not declared"),
                        expression.span,
                    )
                }),
            ExpressionKind::Vector { values } => {
                if vector_shape(expression).is_none() {
                    return Err(error(
                        "TYPE_MISMATCH",
                        "vector literal has inconsistent nested dimensions",
                        expression.span,
                    ));
                }
                let mut element_type = Type::Unknown;
                for value in values {
                    let actual = self.expression(value, locals)?;
                    if element_type == Type::Unknown {
                        element_type = actual;
                    } else if !comparable(&element_type, &actual) {
                        return Err(error(
                            "TYPE_MISMATCH",
                            "vector literal elements must have one compatible type",
                            value.span,
                        ));
                    }
                }
                if element_type == Type::Unknown {
                    return Err(error(
                        "TYPE_MISMATCH",
                        "empty vector literal has no inferable element type",
                        expression.span,
                    ));
                }
                let element_type = match element_type {
                    Type::Vector { element, .. } => *element,
                    element => element,
                };
                Ok(Type::Vector {
                    element: Box::new(default_literal_type(element_type)),
                    dimensions: vector_shape(expression)
                        .expect("validated vector shape")
                        .into_iter()
                        .map(|dimension| u64::try_from(dimension).expect("usize fits u64"))
                        .collect(),
                })
            }
            ExpressionKind::New {
                type_name,
                arguments,
            } => {
                let allocation_type = self.resolve_type(type_from_name(type_name));
                let mut argument_types = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    let argument_type = self.expression(argument, locals)?;
                    if matches!(allocation_type, Type::Integer(_) | Type::Float(_))
                        && !is_integer(&argument_type)
                    {
                        return Err(error(
                            "ALLOCATION_SIZE_INVALID",
                            "numeric NEW length must be integral",
                            argument.span,
                        ));
                    }
                    if matches!(allocation_type, Type::Integer(_) | Type::Float(_))
                        && constant_integer(argument).is_some_and(|length| length < 0)
                    {
                        return Err(error(
                            "ALLOCATION_SIZE_INVALID",
                            "numeric NEW length cannot be negative",
                            argument.span,
                        ));
                    }
                    argument_types.push(argument_type);
                }
                Ok(match allocation_type {
                    Type::Integer(kind) => Type::Pointer {
                        element: Box::new(Type::Integer(kind)),
                        length: allocation_length(arguments),
                    },
                    Type::Float(kind) => Type::Pointer {
                        element: Box::new(Type::Float(kind)),
                        length: allocation_length(arguments),
                    },
                    Type::ImportedNamed { module, name } => {
                        let info = self
                            .imported_types
                            .get(&(module, name.clone()))
                            .ok_or_else(|| {
                                error(
                                    "UNKNOWN_TYPE",
                                    format!("imported type '{type_name}' is unavailable"),
                                    expression.span,
                                )
                            })?;
                        if info.kind != DeclarationKind::Class {
                            return Err(error(
                                "INVALID_CONSTRUCTOR",
                                format!("NEW requires a CLASS, found '{type_name}'"),
                                expression.span,
                            ));
                        }
                        let constructor = info.constructor.as_ref().ok_or_else(|| {
                            error(
                                "PRIVATE_ACCESS",
                                format!(
                                    "CLASS '{type_name}' has only an implicit PRIVATE constructor"
                                ),
                                expression.span,
                            )
                        })?;
                        if !constructor.public {
                            return Err(error(
                                "PRIVATE_ACCESS",
                                format!("constructor for CLASS '{type_name}' is PRIVATE"),
                                expression.span,
                            ));
                        }
                        if constructor.parameters.len() != argument_types.len()
                            || constructor
                                .parameters
                                .iter()
                                .zip(&argument_types)
                                .any(|(expected, actual)| !self.compatible(expected, actual))
                        {
                            return Err(error(
                                "INVALID_CONSTRUCTOR",
                                format!("arguments do not match constructor for '{type_name}'"),
                                expression.span,
                            ));
                        }
                        Type::ImportedNamed { module, name }
                    }
                    Type::Named(name) if matches!(name.as_str(), "FS.File" | "DataFrame") => {
                        Type::Named(name)
                    }
                    _ => {
                        if self.declaration_kinds.get(type_name) != Some(&DeclarationKind::Class) {
                            return Err(error(
                                "INVALID_CONSTRUCTOR",
                                format!("NEW requires a declared CLASS, found '{type_name}'"),
                                expression.span,
                            ));
                        }
                        let constructor = self.constructors.get(type_name).cloned().or_else(|| {
                            (self.current_class.as_deref() == Some(type_name.as_str())
                                && arguments.is_empty())
                            .then_some(Constructor {
                                parameters: Vec::new(),
                                public: false,
                            })
                        });
                        let Some(constructor) = constructor else {
                            return Err(error(
                                "PRIVATE_ACCESS",
                                format!(
                                    "CLASS '{type_name}' has only an implicit PRIVATE constructor"
                                ),
                                expression.span,
                            ));
                        };
                        if !constructor.public
                            && self.current_class.as_deref() != Some(type_name.as_str())
                        {
                            return Err(error(
                                "PRIVATE_ACCESS",
                                format!("constructor for CLASS '{type_name}' is PRIVATE"),
                                expression.span,
                            ));
                        }
                        if constructor.parameters.len() != argument_types.len() {
                            return Err(error(
                                "INVALID_CONSTRUCTOR",
                                format!(
                                    "constructor for CLASS '{type_name}' expects {} argument(s), found {}",
                                    constructor.parameters.len(),
                                    argument_types.len()
                                ),
                                expression.span,
                            ));
                        }
                        for ((expected, actual), argument) in constructor
                            .parameters
                            .iter()
                            .zip(&argument_types)
                            .zip(arguments)
                        {
                            if !self.compatible(expected, actual) {
                                return Err(error(
                                    "INVALID_CONSTRUCTOR",
                                    format!(
                                        "constructor argument has type {}, expected {}",
                                        display(actual),
                                        display(expected)
                                    ),
                                    argument.span,
                                ));
                            }
                        }
                        Type::Named(type_name.clone())
                    }
                })
            }
            ExpressionKind::Unary { operator, operand } => {
                let operand_type = self.expression(operand, locals)?;
                match operator.as_str() {
                    "Minus" if is_numeric(&operand_type) => {
                        if let Some(value) = constant_integer(expression) {
                            Ok(Type::IntegerLiteral(value.to_string()))
                        } else {
                            Ok(default_literal_type(operand_type))
                        }
                    }
                    "NOT" if operand_type == Type::Boolean => Ok(Type::Boolean),
                    "NOT" if is_integer(&operand_type) => Ok(default_literal_type(operand_type)),
                    _ => Err(error(
                        "TYPE_MISMATCH",
                        format!(
                            "operator {operator} cannot be applied to {}",
                            display(&operand_type)
                        ),
                        expression.span,
                    )),
                }
            }
            ExpressionKind::Binary {
                operator,
                left,
                right,
            } => {
                let left_type = self.expression(left, locals)?;
                if operator == "IS" {
                    self.validate_is_test(&left_type, right, expression.span)?;
                    Ok(Type::Boolean)
                } else {
                    let right_type = self.expression(right, locals)?;
                    binary_type(operator, &left_type, &right_type, expression)
                }
            }
            ExpressionKind::Call { callee, arguments } => {
                if matches!(callee.kind, ExpressionKind::Super) {
                    self.super_constructor_call(arguments, locals, expression.span)
                } else {
                    self.call(callee, arguments, locals, expression.span)
                }
            }
            ExpressionKind::Member { object, name }
                if matches!(object.kind, ExpressionKind::Super) =>
            {
                let base = self.direct_base(expression.span)?;
                self.member_type(&Type::Named(base), name, expression.span)
            }
            ExpressionKind::Member { object, name } => {
                let object_type = self.expression(object, locals)?;
                self.member_type(&object_type, name, expression.span)
            }
            ExpressionKind::Index { object, index } => {
                let object_type = if matches!(object.kind, ExpressionKind::HostCapability { ref name } if name == "Args")
                {
                    if self.executable_module {
                        Type::HostArgs
                    } else {
                        return Err(error(
                            "HOST_ARGS_SCOPE",
                            "HOST.Args is valid only in the executable module",
                            object.span,
                        ));
                    }
                } else {
                    self.expression(object, locals)?
                };
                let index_type = self.expression(index, locals)?;
                if !is_integer(&index_type) {
                    return Err(error(
                        "TYPE_MISMATCH",
                        "index must have an integral type",
                        index.span,
                    ));
                }
                match object_type {
                    Type::Vector {
                        element,
                        mut dimensions,
                    } => {
                        if dimensions.len() <= 1 {
                            Ok(*element)
                        } else {
                            dimensions.remove(0);
                            Ok(Type::Vector {
                                element,
                                dimensions,
                            })
                        }
                    }
                    Type::Pointer { element, .. } if !is_void(&element) => Ok(*element),
                    Type::Pointer { .. } => Err(error(
                        "TYPE_MISMATCH",
                        "POINTER TO VOID must be converted to a typed pointer before indexing",
                        object.span,
                    )),
                    Type::String | Type::HostArgs => Ok(Type::String),
                    Type::Alternative(types) => types
                        .iter()
                        .find_map(|ty| match ty {
                            Type::Pointer { element, .. } if !is_void(element) => {
                                Some(*element.clone())
                            }
                            _ => None,
                        })
                        .ok_or_else(|| {
                            error(
                                "TYPE_MISMATCH",
                                format!("cannot index {}", display(&Type::Alternative(types))),
                                object.span,
                            )
                        }),
                    Type::Unknown => Err(error(
                        "UNRESOLVED_TYPE",
                        "indexed expression has no resolved type",
                        object.span,
                    )),
                    other => Err(error(
                        "TYPE_MISMATCH",
                        format!("cannot index {}", display(&other)),
                        object.span,
                    )),
                }
            }
            ExpressionKind::Cast { type_ref, value } => {
                let source = self.expression(value, locals)?;
                let target = self.resolve_reference(type_ref);
                if !conversion_allowed(&source, &target) {
                    return Err(error(
                        "TYPE_MISMATCH",
                        format!(
                            "cannot convert {} AS {}",
                            display(&source),
                            display(&target)
                        ),
                        expression.span,
                    ));
                }
                Ok(target)
            }
        };
        let ty = result?;
        let symbol_id = match &expression.kind {
            ExpressionKind::Name { name } => self.lookup(name, locals).map(|symbol| symbol.id),
            _ => None,
        };
        self.record_expression(expression.span, &ty, symbol_id);
        if let ExpressionKind::Member { object, name } = &expression.kind {
            if matches!(object.kind, ExpressionKind::Super)
                && let Ok(base) = self.direct_base(expression.span)
                && let Some(resolved) = self
                    .expressions
                    .iter_mut()
                    .find(|resolved| resolved.span == expression.span)
            {
                resolved.member_target = Some(MemberTarget {
                    module: None,
                    owner: Some(base),
                    name: name.clone(),
                });
            }
            let object_type = self
                .expressions
                .iter()
                .find(|resolved| resolved.span == object.span)
                .map(|resolved| resolved.ty.clone());
            if let Some(mut target) = object_type.and_then(|ty| member_target(&ty, name)) {
                let resolved_owner = target
                    .owner
                    .as_deref()
                    .and_then(|owner| self.member_owner(owner, name));
                if let Some(owner) = resolved_owner {
                    target.owner = Some(owner);
                }
                if let Some(resolved) = self
                    .expressions
                    .iter_mut()
                    .find(|resolved| resolved.span == expression.span)
                {
                    resolved.member_target = Some(target);
                }
            }
        }
        if matches!(expression.kind, ExpressionKind::New { .. })
            && let Some(target) = constructor_target(&ty)
            && let Some(resolved) = self
                .expressions
                .iter_mut()
                .find(|resolved| resolved.span == expression.span)
        {
            resolved.member_target = Some(target);
        }
        if let ExpressionKind::Call { callee, .. } = &expression.kind
            && matches!(callee.kind, ExpressionKind::Super)
            && let Ok(base) = self.direct_base(expression.span)
            && let Some(resolved) = self
                .expressions
                .iter_mut()
                .find(|resolved| resolved.span == expression.span)
        {
            resolved.member_target = Some(MemberTarget {
                module: None,
                owner: Some(base),
                name: "CONSTRUCTOR".into(),
            });
        }
        Ok(ty)
    }
}
