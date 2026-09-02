#![allow(clippy::wildcard_imports)]
use super::*;

impl Analyzer {
    pub(crate) fn record_expression(&mut self, span: Span, ty: &Type, symbol_id: Option<SymbolId>) {
        let resolved = ResolvedExpression {
            span,
            type_name: display(ty),
            ty: ty.clone(),
            symbol_id,
            member_target: None,
        };
        if let Some(existing) = self
            .expressions
            .iter_mut()
            .find(|existing| existing.span == span)
        {
            *existing = resolved;
        } else {
            self.expressions.push(resolved);
        }
    }

    #[allow(clippy::too_many_lines)] // Member lookup enumerates every accepted owner type.
    #[allow(clippy::too_many_lines)] // Member lookup enumerates every accepted owner type.
    pub(crate) fn member_type(
        &self,
        object: &Type,
        name: &str,
        span: Span,
    ) -> Result<Type, Diagnostic> {
        if object == &Type::Unknown {
            return Err(error(
                "UNRESOLVED_TYPE",
                "member receiver has no resolved type",
                span,
            ));
        }
        if let Type::Module(module) = object {
            return self
                .module_exports
                .get(module)
                .and_then(|exports| exports.get(name))
                .cloned()
                .ok_or_else(|| {
                    error(
                        "NAME_NOT_FOUND",
                        format!("imported module does not export '{name}'"),
                        span,
                    )
                });
        }
        if let Type::ImportedNamed {
            module,
            name: imported_name,
        }
        | Type::ImportedTypeName {
            module,
            name: imported_name,
        } = object
        {
            let static_access = matches!(object, Type::ImportedTypeName { .. });
            let member = self
                .imported_types
                .get(&(*module, imported_name.clone()))
                .and_then(|info| info.members.get(name));
            let member = member.ok_or_else(|| {
                error(
                    "NAME_NOT_FOUND",
                    format!(
                        "imported type '{}' has no exported member '{name}'",
                        display(object)
                    ),
                    span,
                )
            })?;
            if member.is_static != static_access {
                return Err(error(
                    "TYPE_MISMATCH",
                    "imported member has the wrong instance/static access form",
                    span,
                ));
            }
            return Ok(member.ty.clone());
        }
        let (owner, static_access) = match object {
            Type::System => ("SYSTEM", false),
            Type::HostClock => ("HOST.Clock", false),
            Type::HostRandom => ("HOST.Random", false),
            Type::HostConsole => ("HOST.Console", false),
            Type::HostFileSystem => ("HOST.FileSystem", false),
            Type::HostNet => ("HOST.Net", false),
            Type::Named(owner) => (owner.as_str(), false),
            Type::TypeName(owner) => (owner.as_str(), true),
            Type::Alternative(alternatives) => {
                let mut found = None;
                for alternative in alternatives {
                    if matches!(
                        alternative,
                        Type::Null | Type::NotAvailable | Type::EndOfFile
                    ) {
                        continue;
                    }
                    let ty = self.member_type(alternative, name, span)?;
                    if found.as_ref().is_some_and(|found| found != &ty) {
                        return Err(error(
                            "TYPE_MISMATCH",
                            format!("member '{name}' has incompatible alternative types"),
                            span,
                        ));
                    }
                    found = Some(ty);
                }
                return found.ok_or_else(|| {
                    error(
                        "TYPE_MISMATCH",
                        format!("member '{name}' is unavailable on {}", display(object)),
                        span,
                    )
                });
            }
            _ => {
                return Err(error(
                    "TYPE_MISMATCH",
                    format!("{} has no member '{name}'", display(object)),
                    span,
                ));
            }
        };
        let member = self
            .members
            .get(owner)
            .and_then(|members| members.get(name))
            .ok_or_else(|| {
                error(
                    "NAME_NOT_FOUND",
                    format!("type '{owner}' has no member '{name}'"),
                    span,
                )
            })?;
        if member.is_static != static_access {
            return Err(error(
                "TYPE_MISMATCH",
                if static_access {
                    format!("instance member '{owner}.{name}' requires an object")
                } else {
                    format!("STATIC member '{owner}.{name}' requires the type name")
                },
                span,
            ));
        }
        if member.private && self.current_class.as_deref() != Some(owner) {
            return Err(error(
                "PRIVATE_ACCESS",
                format!("member '{owner}.{name}' is PRIVATE"),
                span,
            ));
        }
        Ok(member.ty.clone())
    }
    pub(crate) fn validate_is_test(
        &self,
        subject: &Type,
        test: &Expression,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let test_type = is_test_type(test).ok_or_else(|| {
            error(
                "INVALID_ALTERNATIVE_USE",
                "IS requires a type, NULL, NA, EOF, NAN, or INF test",
                test.span,
            )
        })?;
        let test_type = self.resolve_type(test_type);
        if is_float(&test_type)
            && matches!(&test.kind, ExpressionKind::Literal(Literal::Special(_)))
            && is_float(subject)
        {
            return Ok(());
        }
        if compatible(subject, &test_type) {
            Ok(())
        } else {
            Err(error(
                "INVALID_ALTERNATIVE_USE",
                format!(
                    "{} is not an alternative of {}",
                    display(&test_type),
                    display(subject)
                ),
                span,
            ))
        }
    }
    pub(crate) fn narrowed_locals(
        &self,
        condition: &Expression,
        locals: &HashMap<String, Symbol>,
        truth: bool,
    ) -> Result<HashMap<String, Symbol>, Diagnostic> {
        if let ExpressionKind::Unary { operator, operand } = &condition.kind
            && operator == "NOT"
        {
            return self.narrowed_locals(operand, locals, !truth);
        }
        let ExpressionKind::Binary {
            operator,
            left,
            right,
        } = &condition.kind
        else {
            return Ok(locals.clone());
        };
        if operator == "AND" {
            if !truth {
                return Ok(locals.clone());
            }
            let left_locals = self.narrowed_locals(left, locals, true)?;
            return self.narrowed_locals(right, &left_locals, true);
        }
        if operator == "OR" {
            if truth {
                return Ok(locals.clone());
            }
            let left_locals = self.narrowed_locals(left, locals, false)?;
            return self.narrowed_locals(right, &left_locals, false);
        }
        if !matches!(operator.as_str(), "IS" | "Assign" | "NotEqual") {
            return Ok(locals.clone());
        }
        let ExpressionKind::Name { name } = &left.kind else {
            return Ok(locals.clone());
        };
        let test_type = if operator == "IS" {
            self.resolve_type(is_test_type(right).ok_or_else(|| {
                error(
                    "INVALID_ALTERNATIVE_USE",
                    "IS requires a valid type test",
                    right.span,
                )
            })?)
        } else {
            let Some(test_type) = is_test_type(right) else {
                return Ok(locals.clone());
            };
            if !matches!(test_type, Type::Null | Type::NotAvailable | Type::EndOfFile) {
                return Ok(locals.clone());
            }
            test_type
        };
        let mut narrowed = locals.clone();
        let symbol = narrowed.get_mut(name).ok_or_else(|| {
            error(
                "NAME_NOT_FOUND",
                format!("name '{name}' is not declared"),
                left.span,
            )
        })?;
        let Type::Alternative(alternatives) = &symbol.ty else {
            return Ok(narrowed);
        };
        let truth = if operator == "NotEqual" {
            !truth
        } else {
            truth
        };
        let choices: Vec<Type> = if truth {
            alternatives
                .iter()
                .filter(|alternative| compatible(alternative, &test_type))
                .cloned()
                .collect()
        } else {
            alternatives
                .iter()
                .filter(|alternative| !compatible(alternative, &test_type))
                .cloned()
                .collect()
        };
        symbol.ty = match choices.as_slice() {
            [] => Type::Unknown,
            [ty] => ty.clone(),
            _ => Type::Alternative(choices),
        };
        Ok(narrowed)
    }

    pub(crate) fn replace_narrowing_fact(
        target: &Expression,
        assigned: &Type,
        locals: &mut HashMap<String, Symbol>,
    ) {
        let ExpressionKind::Name { name } = &target.kind else {
            return;
        };
        let Some(symbol) = locals.get_mut(name) else {
            return;
        };
        if matches!(symbol.declared_ty, Type::Alternative(_)) {
            symbol.ty = default_literal_type(assigned.clone());
        } else {
            symbol.ty = symbol.declared_ty.clone();
        }
    }

    pub(crate) fn direct_base(&self, span: Span) -> Result<String, Diagnostic> {
        let class = self.current_class.as_deref().ok_or_else(|| {
            error(
                "INVALID_SUPER",
                "SUPER is valid only in a derived CLASS",
                span,
            )
        })?;
        self.base_classes.get(class).cloned().ok_or_else(|| {
            error(
                "INVALID_SUPER",
                "SUPER is valid only in a derived CLASS",
                span,
            )
        })
    }

    pub(crate) fn super_constructor_call(
        &mut self,
        arguments: &[Expression],
        locals: &HashMap<String, Symbol>,
        span: Span,
    ) -> Result<Type, Diagnostic> {
        let base = self.direct_base(span)?;
        let parameters = self
            .constructors
            .get(&base)
            .map_or_else(Vec::new, |constructor| constructor.parameters.clone());
        if arguments.len() != parameters.len() {
            return Err(error(
                "TYPE_MISMATCH",
                format!(
                    "SUPER expects {} argument(s), found {}",
                    parameters.len(),
                    arguments.len()
                ),
                span,
            ));
        }
        for (argument, parameter) in arguments.iter().zip(parameters) {
            let actual = self.expression_as(argument, &parameter, locals)?;
            if !self.compatible(&parameter, &actual) {
                return Err(error(
                    "TYPE_MISMATCH",
                    format!(
                        "cannot pass {} to SUPER parameter {}",
                        display(&actual),
                        display(&parameter)
                    ),
                    argument.span,
                ));
            }
        }
        Ok(Type::Named("VOID".into()))
    }

    pub(crate) fn call(
        &mut self,
        callee: &Expression,
        arguments: &[Expression],
        locals: &HashMap<String, Symbol>,
        span: Span,
    ) -> Result<Type, Diagnostic> {
        let callee_type = self.expression(callee, locals)?;
        if let ExpressionKind::Member { object, name } = &callee.kind
            && let Some(Type::Module(module)) = self
                .expressions
                .iter()
                .find(|expression| expression.span == object.span)
                .map(|expression| &expression.ty)
            && self.bnmath_modules.contains(module)
        {
            return self.math_call(name, arguments, locals, span);
        }
        let Type::Function {
            parameters,
            return_type,
        } = callee_type
        else {
            for argument in arguments {
                self.expression(argument, locals)?;
            }
            return Err(error(
                "NOT_CALLABLE",
                format!("{} is not callable", display(&callee_type)),
                span,
            ));
        };
        if arguments.len() != parameters.len() {
            return Err(error(
                "TYPE_MISMATCH",
                format!(
                    "call expects {} argument(s), found {}",
                    parameters.len(),
                    arguments.len()
                ),
                span,
            ));
        }
        for (argument, parameter) in arguments.iter().zip(parameters) {
            let actual = self.expression_as(argument, &parameter, locals)?;
            if !self.compatible(&parameter, &actual) {
                return Err(error(
                    "TYPE_MISMATCH",
                    format!(
                        "cannot pass {} to parameter of type {}",
                        display(&actual),
                        display(&parameter)
                    ),
                    argument.span,
                ));
            }
        }
        if let ExpressionKind::Member { name, .. } = &callee.kind {
            if name == "Wait"
                && let Some(timeout) = arguments.first().and_then(constant_integer)
                && !(1..=60_000).contains(&timeout)
            {
                return Err(error(
                    "AWAIT_TIMEOUT",
                    "AWAIT timeout must be between 1 and 60000 milliseconds",
                    arguments[0].span,
                ));
            }
            if name == "Async"
                && !matches!(
                    arguments.first().map(|argument| &argument.kind),
                    Some(ExpressionKind::Name { .. })
                )
            {
                return Err(error(
                    "ASYNC_TARGET",
                    "ASYNC submission requires a named function target",
                    span,
                ));
            }
        }
        if let ExpressionKind::Member { name, .. } = &callee.kind
            && name == "Open"
            && let Some(mode) = arguments.get(1).and_then(constant_integer)
            && !matches!(mode, 0..=2)
        {
            return Err(error(
                "INVALID_FILE_MODE",
                "FS.Open mode must be FS.READ, FS.WRITE, or FS.APPEND",
                arguments[1].span,
            ));
        }
        Ok(*return_type)
    }
}
