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
                    DiagId::INVALID_SUPER,
                    "SUPER is only valid as SUPER(...) or SUPER.Name(...)",
                    statement_span(statement),
                ));
            }
            if terminated {
                let diagnostic = unreachable_code(
                    "statement is unreachable after control flow leaves this path",
                    statement_span(statement),
                );
                if self.collect_warnings {
                    self.warnings.push(diagnostic);
                    continue;
                }
                return Err(diagnostic);
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
                    ..
                } => {
                    self.validate_type_reference(type_ref, false)?;
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
                                            DiagId::INVALID_VECTOR_DIMENSION,
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
                                        DiagId::INVALID_VECTOR_DIMENSION,
                                        "vector dimension must be a non-negative integer",
                                        span,
                                    ));
                                }
                                if value > i128::from(i32::MAX) {
                                    return Err(error(
                                        DiagId::NUMERIC_OVERFLOW,
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
                        return Err(type_mismatch(
                            "initializer",
                            "missing initializer",
                            format!("{} binding", display(&ty)),
                            *span,
                        ));
                    }
                    if let Some(initializer) = initializer {
                        let actual = self.expression_as(initializer, &ty, locals)?;
                        if !self.compatible(&ty, &actual) {
                            return Err(if pointer_literal_length_mismatch(&ty, &actual) {
                                error(
                                    DiagId::POINTER_LENGTH_MISMATCH,
                                    format!(
                                        "cannot assign {} to {}",
                                        display(&actual),
                                        display(&ty)
                                    ),
                                    initializer.span,
                                )
                            } else {
                                value_mismatch(
                                    &ty,
                                    &actual,
                                    "binding initializer",
                                    initializer.span,
                                )
                            });
                        }
                    }
                    self.declare_local(locals, name, ty.clone(), *constant, *span)?;
                    for (index, additional_name) in additional_names.iter().enumerate() {
                        if let Some(additional_initializer) = additional_initializers.get(index) {
                            let actual = self.expression_as(additional_initializer, &ty, locals)?;
                            if !self.compatible(&ty, &actual) {
                                return Err(value_mismatch(
                                    &ty,
                                    &actual,
                                    "additional binding initializer",
                                    additional_initializer.span,
                                ));
                            }
                        } else if requires_initializer(type_ref)
                            || self.type_requires_initializer(&ty)
                        {
                            return Err(type_mismatch(
                                "initializer",
                                "missing initializer",
                                format!("{} binding", display(&ty)),
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
                Statement::Destructuring {
                    bindings,
                    source,
                    span,
                } => {
                    self.destructure(bindings, source, locals, declaration_kind, *span)?;
                }
                Statement::Assignment {
                    target,
                    value,
                    span,
                    ..
                } => {
                    // A compound assignment arrives as `target = target op value`
                    // (`ast::Statement::assignment`): one rule for both forms.
                    let target_type = self.assignment_target(target, locals)?;
                    let result_type = self.expression_as(value, &target_type, locals)?;
                    if !self.compatible(&target_type, &result_type) {
                        return Err(
                            if pointer_literal_length_mismatch(&target_type, &result_type) {
                                error(
                                    DiagId::POINTER_LENGTH_MISMATCH,
                                    format!(
                                        "cannot assign {} to {}",
                                        display(&result_type),
                                        display(&target_type)
                                    ),
                                    *span,
                                )
                            } else {
                                value_mismatch(&target_type, &result_type, "assignment", *span)
                            },
                        );
                    }
                    Self::end_narrowing_of_target(target, locals);
                }
                Statement::If {
                    branches,
                    otherwise,
                    ..
                } => {
                    let mut written = otherwise
                        .as_ref()
                        .map(|block| written_bindings(&block.statements))
                        .unwrap_or_default();
                    for branch in branches {
                        written.extend(written_bindings(&branch.body.statements));
                    }
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
                    // Every branch left and there is no ELSE: only the path
                    // that took no branch continues, and it wrote nothing.
                    let only_untaken_path_continues = otherwise.is_none() && taken_exits;
                    if let Some(Block { statements, .. }) = otherwise {
                        self.block(
                            statements,
                            &mut remaining_locals,
                            loops,
                            declaration_kind,
                            declaration_name,
                            return_type,
                        )?;
                    }
                    if only_untaken_path_continues {
                        *locals = remaining_locals;
                    } else {
                        Self::end_narrowings(&written, locals);
                    }
                }
                Statement::While {
                    condition, body, ..
                } => {
                    let written = written_bindings(&body.statements);
                    let mut entry_locals = locals.clone();
                    Self::end_narrowings(&written, &mut entry_locals);
                    self.require_boolean(condition, &entry_locals)?;
                    let mut body_locals = self.narrowed_locals(condition, &entry_locals, true)?;
                    Self::end_narrowings(&written, locals);
                    loops.push("WHILE");
                    self.block(
                        &body.statements,
                        &mut body_locals,
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
                    // A later iteration starts after the body's writes.
                    let written = written_bindings(&body.statements);
                    Self::end_narrowings(&written, locals);
                    let mut body_locals = locals.clone();
                    loops.push("REPEAT");
                    self.block(
                        &body.statements,
                        &mut body_locals,
                        loops,
                        declaration_kind,
                        declaration_name,
                        return_type,
                    )?;
                    loops.pop();
                    // `UNTIL` belongs to the block and sees the body's
                    // bindings, which end with the statement (0.6.md,
                    // "Scopes and bindings").
                    self.require_boolean(condition, &body_locals)?;
                }
                Statement::For { header, body, .. } => {
                    // The loop variable and the body are one block: both end
                    // at END FOR (0.6.md, "Scopes and bindings"). The header
                    // runs once, before any write of the body.
                    let written = written_bindings(&body.statements);
                    let mut loop_locals = locals.clone();
                    self.for_header(header, &mut loop_locals)?;
                    Self::end_narrowings(&written, &mut loop_locals);
                    Self::end_narrowings(&written, locals);
                    loops.push("FOR");
                    self.block(
                        &body.statements,
                        &mut loop_locals,
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
                            DiagId::INVALID_LOOP_CONTROL,
                            format!("{kind} {target} requires an enclosing {target} loop"),
                            *span,
                        ));
                    }
                }
                Statement::Return { value, span } => {
                    if declaration_kind != DeclarationKind::Function && value.is_some() {
                        return Err(type_mismatch(
                            "VOID return",
                            "value",
                            "constructor/destructor return",
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
                            return Err(value_mismatch(
                                return_type,
                                &value_type,
                                "FUNCTION return",
                                value.span,
                            ));
                        }
                    }
                }
                Statement::Print { values, .. } => {
                    let context = bn_catalog::text("frontend", "msg.print_value");
                    for value in values {
                        self.expression(value, locals)?;
                        self.settle_integer_constant(value, context)?;
                    }
                }
                Statement::Input {
                    prompt,
                    target,
                    span,
                } => {
                    if let Some(prompt) = prompt {
                        let prompt_type = self.expression(prompt, locals)?;
                        if prompt_type != Type::String {
                            return Err(error(
                                DiagId::INPUT_PROMPT_TYPE,
                                bn_catalog::text("frontend", "msg.input_prompt_must_be_string"),
                                prompt.span,
                            ));
                        }
                    }
                    let target_type = self.assignment_target(target, locals)?;
                    let is_valid = target_type == Type::String
                        || matches!(&target_type, Type::Alternative(alts) if alts.contains(&Type::String) && alts.contains(&Type::EndOfFile));
                    if !is_valid {
                        return Err(type_mismatch(
                            bn_catalog::text("frontend", "msg.string_or_string_or_eof"),
                            display(&target_type),
                            bn_catalog::text("frontend", "msg.input_target"),
                            *span,
                        ));
                    }
                    Self::end_narrowing_of_target(target, locals);
                }
                Statement::ClearScreen { span, .. } | Statement::Beep { span, .. } => {
                    return Err(name_not_found(
                        "the CLS or BEEP statement",
                        "Basic Next 0.6 (withdrawn in 0.2; use HOST.Console.Cls or Beep)",
                        *span,
                    ));
                }
                Statement::Release { value, .. } => {
                    // A local binding or a parameter, of any type, as Swift's
                    // `consume` (0.6.md, "`RELEASE`"). A field (even unqualified
                    // in a method), a STATIC, or an element is not one: assign
                    // NULL to let a field or a STATIC go.
                    let local = matches!(&value.kind, ExpressionKind::Name { name } if locals.contains_key(name));
                    if !local {
                        return Err(error(
                            DiagId::INVALID_RELEASE_TARGET,
                            "RELEASE takes a local binding or a parameter; assign NULL to a field or a STATIC instead",
                            value.span,
                        ));
                    }
                    self.expression(value, locals)?;
                }
                Statement::Stop { code, .. } => {
                    if !is_integer(&self.expression(code, locals)?) {
                        return Err(type_mismatch(
                            "INTEGER",
                            "non-INTEGER value",
                            "STOP exit code",
                            code.span,
                        ));
                    }
                    if let Some(value) = integer_literal(code)
                        && !(0..=255).contains(&value)
                    {
                        return Err(error(
                            DiagId::INVALID_EXIT_CODE,
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
                        return Err(type_mismatch(
                            "no SELF access",
                            "SELF reference",
                            "STATIC FUNCTION body",
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
                    self.record_return_type(signature.as_ref(), return_type.as_ref());
                    if let Some(signature) = signature {
                        for parameter in &signature.parameters {
                            self.validate_type_reference(&parameter.type_ref, true)?;
                            self.declare_parameter(
                                &mut member_locals,
                                &parameter.name,
                                self.resolve_reference(&parameter.type_ref),
                                parameter.span,
                            )?;
                        }
                    } else {
                        for parameter in parameters {
                            self.validate_type_reference(&parameter.type_ref, true)?;
                            self.declare_parameter(
                                &mut member_locals,
                                &parameter.name,
                                self.resolve_reference(&parameter.type_ref),
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
                    if let Some(signature) = signature
                        && (!self.is_standard_module || !body.statements.is_empty())
                    {
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
