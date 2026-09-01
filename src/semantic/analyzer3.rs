#![allow(clippy::wildcard_imports)]
use super::*;

impl Analyzer {
    #[allow(clippy::too_many_lines)] // Statement alternatives mirror the grammar.
    pub(crate) fn block(
        &mut self,
        statements: &[Statement],
        locals: &mut HashMap<String, Symbol>,
        loops: &mut Vec<&'static str>,
        declaration_kind: DeclarationKind,
        declaration_name: &str,
        return_type: Option<&Type>,
    ) -> Result<(), Diagnostic> {
        let mut terminated = false;
        for statement in statements {
            if statement_has_invalid_super(statement) {
                return Err(error(
                    "INVALID_SUPER",
                    "SUPER is only valid as SUPER(...) or SUPER.Name(...)",
                    statement_span(statement),
                ));
            }
            if terminated {
                return Err(error(
                    "UNREACHABLE_CODE",
                    "statement is unreachable after control flow leaves this path",
                    statement_span(statement),
                ));
            }
            match statement {
                Statement::Binding {
                    constant,
                    name,
                    type_ref,
                    initialized,
                    initializer,
                    additional_names,
                    additional_name_spans,
                    additional_initializers,
                    span,
                    is_static,
                    ..
                } => {
                    self.validate_type_reference(type_ref)?;
                    let ty = self.resolve_reference(type_ref);
                    for atom in &type_ref.alternatives {
                        for dimension in &atom.dimensions {
                            let (value, span) = match dimension {
                                crate::ast::VectorDimension::Literal { value, span } => {
                                    (value.parse::<i128>().ok(), *span)
                                }
                                crate::ast::VectorDimension::Expression(expression) => {
                                    let dimension_type = self.expression(expression, locals)?;
                                    if !is_integer(&dimension_type) {
                                        return Err(error(
                                            "INVALID_VECTOR_DIMENSION",
                                            "vector dimension must be a non-negative integer",
                                            expression.span,
                                        ));
                                    }
                                    (constant_integer(expression), expression.span)
                                }
                            };
                            if let Some(value) = value {
                                if value < 0 {
                                    return Err(error(
                                        "INVALID_VECTOR_DIMENSION",
                                        "vector dimension must be a non-negative integer",
                                        span,
                                    ));
                                }
                                if value > i128::from(i32::MAX) {
                                    return Err(error(
                                        "NUMERIC_OVERFLOW",
                                        "vector dimension does not fit INTEGER",
                                        span,
                                    ));
                                }
                            }
                        }
                    }
                    if !initialized
                        && (requires_initializer(type_ref) || self.type_requires_initializer(&ty))
                    {
                        return Err(error(
                            "TYPE_MISMATCH",
                            format!(
                                "{} bindings require an initializer",
                                type_ref.alternatives[0].name
                            ),
                            *span,
                        ));
                    }
                    if declaration_kind == DeclarationKind::Class && *is_static && !initialized {
                        return Err(error(
                            "TYPE_MISMATCH",
                            "STATIC field requires an initializer",
                            *span,
                        ));
                    }
                    if let Some(initializer) = initializer {
                        let actual = self.expression_as(initializer, &ty, locals)?;
                        if !self.compatible(&ty, &actual) {
                            return Err(error(
                                if pointer_literal_length_mismatch(&ty, &actual) {
                                    "POINTER_LENGTH_MISMATCH"
                                } else {
                                    "TYPE_MISMATCH"
                                },
                                format!("cannot assign {} to {}", display(&actual), display(&ty)),
                                initializer.span,
                            ));
                        }
                    }
                    self.declare_local(locals, name, ty.clone(), *constant, *span)?;
                    for (index, additional_name) in additional_names.iter().enumerate() {
                        if let Some(additional_initializer) = additional_initializers.get(index) {
                            let actual = self.expression_as(additional_initializer, &ty, locals)?;
                            if !self.compatible(&ty, &actual) {
                                return Err(error(
                                    "TYPE_MISMATCH",
                                    format!(
                                        "cannot assign {} to {}",
                                        display(&actual),
                                        display(&ty)
                                    ),
                                    additional_initializer.span,
                                ));
                            }
                        } else if requires_initializer(type_ref)
                            || self.type_requires_initializer(&ty)
                        {
                            return Err(error(
                                "TYPE_MISMATCH",
                                format!(
                                    "{} bindings require an initializer",
                                    type_ref.alternatives[0].name
                                ),
                                *span,
                            ));
                        }
                        let name_span = additional_name_spans.get(index).copied().unwrap_or(*span);
                        self.declare_local(
                            locals,
                            additional_name,
                            ty.clone(),
                            *constant,
                            name_span,
                        )?;
                    }
                }
                Statement::Assignment {
                    target,
                    operator,
                    value,
                    span,
                } => {
                    let target_type = self.assignment_target(target, locals)?;
                    let value_type = self.expression_as(value, &target_type, locals)?;
                    let result_type = if operator == "Assign" {
                        value_type
                    } else {
                        binary_type(
                            compound_operator(operator).ok_or_else(|| {
                                error("TYPE_MISMATCH", "unknown assignment operator", *span)
                            })?,
                            &target_type,
                            &value_type,
                            value,
                        )?
                    };
                    if !self.compatible(&target_type, &result_type) {
                        return Err(error(
                            if pointer_literal_length_mismatch(&target_type, &result_type) {
                                "POINTER_LENGTH_MISMATCH"
                            } else {
                                "TYPE_MISMATCH"
                            },
                            format!(
                                "cannot assign {} to {}",
                                display(&result_type),
                                display(&target_type)
                            ),
                            *span,
                        ));
                    }
                    Self::replace_narrowing_fact(target, &result_type, locals);
                }
                Statement::If {
                    branches,
                    otherwise,
                    ..
                } => {
                    let mut remaining_locals = locals.clone();
                    let mut taken_exits = true;
                    for branch in branches {
                        self.require_boolean(&branch.condition, &remaining_locals)?;
                        let mut branch_locals =
                            self.narrowed_locals(&branch.condition, &remaining_locals, true)?;
                        self.block(
                            &branch.body.statements,
                            &mut branch_locals,
                            loops,
                            declaration_kind,
                            declaration_name,
                            return_type,
                        )?;
                        taken_exits &= guarantees_return(&branch.body.statements);
                        remaining_locals =
                            self.narrowed_locals(&branch.condition, &remaining_locals, false)?;
                    }
                    if let Some(Block { statements, .. }) = otherwise {
                        self.block(
                            statements,
                            &mut remaining_locals,
                            loops,
                            declaration_kind,
                            declaration_name,
                            return_type,
                        )?;
                    } else if taken_exits {
                        *locals = remaining_locals;
                    }
                }
                Statement::While {
                    condition, body, ..
                } => {
                    self.require_boolean(condition, locals)?;
                    loops.push("WHILE");
                    self.block(
                        &body.statements,
                        locals,
                        loops,
                        declaration_kind,
                        declaration_name,
                        return_type,
                    )?;
                    loops.pop();
                }
                Statement::Repeat {
                    condition, body, ..
                } => {
                    loops.push("REPEAT");
                    self.block(
                        &body.statements,
                        locals,
                        loops,
                        declaration_kind,
                        declaration_name,
                        return_type,
                    )?;
                    loops.pop();
                    self.require_boolean(condition, locals)?;
                }
                Statement::For { header, body, .. } => {
                    self.for_header(header, locals)?;
                    loops.push("FOR");
                    self.block(
                        &body.statements,
                        locals,
                        loops,
                        declaration_kind,
                        declaration_name,
                        return_type,
                    )?;
                    loops.pop();
                }
                Statement::Control { kind, target, span } => {
                    if !matches!(kind.as_str(), "EXIT" | "CONTINUE")
                        || !loops.iter().rev().any(|loop_kind| *loop_kind == target)
                    {
                        return Err(error(
                            "INVALID_LOOP_CONTROL",
                            format!("{kind} {target} requires an enclosing {target} loop"),
                            *span,
                        ));
                    }
                }
                Statement::Return { value, span } => {
                    if declaration_kind != DeclarationKind::Function && value.is_some() {
                        return Err(error(
                            "TYPE_MISMATCH",
                            "constructors and destructors cannot return a value",
                            *span,
                        ));
                    }
                    if let Some(value) = value {
                        let value_type = if let Some(return_type) = return_type {
                            self.expression_as(value, return_type, locals)?
                        } else {
                            self.expression(value, locals)?
                        };
                        if let Some(return_type) = return_type
                            && !self.compatible(return_type, &value_type)
                        {
                            return Err(error(
                                "TYPE_MISMATCH",
                                format!(
                                    "cannot return {} from FUNCTION AS {}",
                                    display(&value_type),
                                    display(return_type)
                                ),
                                value.span,
                            ));
                        }
                    }
                }
                Statement::Print { values, .. } => {
                    for value in values {
                        self.expression(value, locals)?;
                    }
                }
                Statement::ClearScreen { span, .. } | Statement::Beep { span, .. } => {
                    return Err(error(
                        "NAME_NOT_FOUND",
                        "CLS and BEEP statements were withdrawn in 0.2; use HOST.Console methods",
                        *span,
                    ));
                }
                Statement::Delete { value, .. } => {
                    let ty = self.expression(value, locals)?;
                    if !self.deletable(&ty) {
                        return Err(error(
                            "INVALID_DELETE_TARGET",
                            format!(
                                "DELETE requires a pointer or CLASS reference, found {}",
                                display(&ty)
                            ),
                            value.span,
                        ));
                    }
                }
                Statement::Stop { code, .. } => {
                    if !is_integer(&self.expression(code, locals)?) {
                        return Err(error(
                            "TYPE_MISMATCH",
                            "STOP requires an INTEGER exit code",
                            code.span,
                        ));
                    }
                    if let Some(value) = integer_literal(code)
                        && !(0..=255).contains(&value)
                    {
                        return Err(error(
                            "INVALID_EXIT_CODE",
                            "STOP exit code must be in 0..255",
                            code.span,
                        ));
                    }
                }
                Statement::Call { expression, .. } => {
                    self.expression(expression, locals)?;
                }
                Statement::MemberFunction {
                    name,
                    is_static,
                    parameters,
                    signature,
                    body: Some(body),
                    ..
                } => {
                    if *is_static && block_uses_self(&body.statements) {
                        return Err(error(
                            "TYPE_MISMATCH",
                            "STATIC FUNCTION cannot access SELF",
                            body.span,
                        ));
                    }
                    let member_kind = if matches!(name.as_str(), "CONSTRUCTOR" | "DESTRUCTOR") {
                        DeclarationKind::Class
                    } else {
                        DeclarationKind::Function
                    };
                    let mut member_locals = HashMap::new();
                    if !*is_static {
                        self.declare_local(
                            &mut member_locals,
                            "SELF",
                            Type::Named(declaration_name.into()),
                            true,
                            body.span,
                        )?;
                    }
                    let return_type = signature
                        .as_ref()
                        .map(|signature| self.resolve_reference(&signature.return_type));
                    if let Some(signature) = signature {
                        for parameter in &signature.parameters {
                            self.validate_type_reference(&parameter.type_ref)?;
                            self.declare_local(
                                &mut member_locals,
                                &parameter.name,
                                self.resolve_reference(&parameter.type_ref),
                                false,
                                parameter.span,
                            )?;
                        }
                    } else {
                        for parameter in parameters {
                            self.validate_type_reference(&parameter.type_ref)?;
                            self.declare_local(
                                &mut member_locals,
                                &parameter.name,
                                self.resolve_reference(&parameter.type_ref),
                                false,
                                parameter.span,
                            )?;
                        }
                    }
                    self.block(
                        &body.statements,
                        &mut member_locals,
                        loops,
                        member_kind,
                        declaration_name,
                        return_type.as_ref(),
                    )?;
                    if let Some(signature) = signature {
                        validate_returns(&body.statements, &signature.return_type, false)?;
                    }
                }
                Statement::MemberFunction { body: None, .. } => {}
            }
            terminated = matches!(
                statement,
                Statement::Return { .. } | Statement::Stop { .. } | Statement::Control { .. }
            );
        }
        Ok(())
    }
}
