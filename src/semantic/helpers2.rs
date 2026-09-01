#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn member_target(object: &Type, name: &str) -> Option<MemberTarget> {
    match object {
        Type::Module(module) => Some(MemberTarget {
            module: Some(*module),
            owner: None,
            name: name.into(),
        }),
        Type::Named(owner) | Type::TypeName(owner) => Some(MemberTarget {
            module: None,
            owner: Some(owner.clone()),
            name: name.into(),
        }),
        Type::ImportedNamed {
            module,
            name: owner,
        }
        | Type::ImportedTypeName {
            module,
            name: owner,
        } => Some(MemberTarget {
            module: Some(*module),
            owner: Some(owner.clone()),
            name: name.into(),
        }),
        Type::System => Some(MemberTarget {
            module: None,
            owner: Some("SYSTEM".into()),
            name: name.into(),
        }),
        Type::HostClock => Some(MemberTarget {
            module: None,
            owner: Some("HOST.Clock".into()),
            name: name.into(),
        }),
        Type::HostRandom => Some(MemberTarget {
            module: None,
            owner: Some("HOST.Random".into()),
            name: name.into(),
        }),
        Type::HostFileSystem => Some(MemberTarget {
            module: None,
            owner: Some("HOST.FileSystem".into()),
            name: name.into(),
        }),
        Type::HostNet => Some(MemberTarget {
            module: None,
            owner: Some("HOST.Net".into()),
            name: name.into(),
        }),
        Type::HostConsole => Some(MemberTarget {
            module: None,
            owner: Some("HOST.Console".into()),
            name: name.into(),
        }),
        Type::Alternative(types) => {
            let mut targets = types.iter().filter_map(|ty| member_target(ty, name));
            let target = targets.next()?;
            targets.all(|other| other == target).then_some(target)
        }
        _ => None,
    }
}
pub(crate) fn constructor_target(ty: &Type) -> Option<MemberTarget> {
    match ty {
        Type::Named(owner) => Some(MemberTarget {
            module: None,
            owner: Some(owner.clone()),
            name: "CONSTRUCTOR".into(),
        }),
        Type::ImportedNamed {
            module,
            name: owner,
        } => Some(MemberTarget {
            module: Some(*module),
            owner: Some(owner.clone()),
            name: "CONSTRUCTOR".into(),
        }),
        Type::Pointer { .. } => Some(MemberTarget {
            module: None,
            owner: None,
            name: "NEW".into(),
        }),
        _ => None,
    }
}
pub(crate) fn validate_type_reference(reference: &TypeReference) -> Result<(), Diagnostic> {
    let mut types = Vec::new();
    for alternative in &reference.alternatives {
        if alternative.name == "POINTER" && !valid_pointer_parts(&alternative.parts) {
            return Err(error(
                "INVALID_POINTER_TYPE",
                "POINTER must name a numeric or declared element type, optionally followed by [literal] or []",
                alternative.span,
            ));
        }
        let ty = type_from_atom(alternative);
        if matches!(ty, Type::System) {
            return Err(error(
                "NAME_NOT_FOUND",
                "SYSTEM was withdrawn in 0.2; use HOST.Args",
                alternative.span,
            ));
        }
        if types.contains(&ty) {
            return Err(error(
                "TYPE_MISMATCH",
                format!("duplicate alternative type '{}'", display(&ty)),
                alternative.span,
            ));
        }
        types.push(ty);
    }
    Ok(())
}

pub(crate) fn valid_pointer_parts(parts: &[String]) -> bool {
    if parts.len() < 2 || parts[0] != "TO" {
        return false;
    }
    let shape = parts
        .iter()
        .position(|part| part == "LeftBracket")
        .unwrap_or(parts.len());
    let element = pointer_element_type(&parts[..shape]);
    if !matches!(element, Type::Integer(_) | Type::Float(_) | Type::Named(_)) {
        return false;
    }
    match &parts[shape..] {
        [] => true,
        [open, close] => open == "LeftBracket" && close == "RightBracket",
        [open, length, close] => {
            open == "LeftBracket" && parse_integer(length).is_some() && close == "RightBracket"
        }
        _ => false,
    }
}
pub(crate) fn error(code: &'static str, message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic {
        code,
        message: message.into(),
        span,
    }
}
pub(crate) fn validate_returns(
    statements: &[Statement],
    return_type: &TypeReference,
    is_start: bool,
) -> Result<(), Diagnostic> {
    validate_return_statements(statements, return_type, is_start)?;
    let is_void = return_type
        .alternatives
        .first()
        .is_some_and(|atom| atom.name == "VOID");
    if !is_void && !guarantees_return(statements) {
        return Err(error(
            "MISSING_RETURN",
            "non-VOID FUNCTION can complete without RETURN expression",
            return_type.span,
        ));
    }
    Ok(())
}

pub(crate) fn validate_return_statements(
    statements: &[Statement],
    return_type: &TypeReference,
    is_start: bool,
) -> Result<(), Diagnostic> {
    let is_void = return_type
        .alternatives
        .first()
        .is_some_and(|atom| atom.name == "VOID");
    for statement in statements {
        match statement {
            Statement::Return {
                value: Some(value), ..
            } if is_start
                && integer_literal(value).is_some_and(|code| !(0..=255).contains(&code)) =>
            {
                return Err(error(
                    "INVALID_EXIT_CODE",
                    "Start return code must be in 0..255",
                    value.span,
                ));
            }
            Statement::Return { value, span } if is_void && value.is_some() => {
                return Err(error(
                    "TYPE_MISMATCH",
                    "VOID FUNCTION cannot return a value",
                    *span,
                ));
            }
            Statement::Return { value: None, span } if !is_void => {
                return Err(error(
                    "TYPE_MISMATCH",
                    "non-VOID FUNCTION must return a value",
                    *span,
                ));
            }
            Statement::If {
                branches,
                otherwise,
                ..
            } => {
                for branch in branches {
                    validate_return_statements(&branch.body.statements, return_type, is_start)?;
                }
                if let Some(otherwise) = otherwise {
                    validate_return_statements(&otherwise.statements, return_type, is_start)?;
                }
            }
            Statement::While { body, .. }
            | Statement::Repeat { body, .. }
            | Statement::For { body, .. } => {
                validate_return_statements(&body.statements, return_type, is_start)?;
            }
            _ => {}
        }
    }
    Ok(())
}
pub(crate) fn guarantees_return(statements: &[Statement]) -> bool {
    statements.iter().any(|statement| match statement {
        Statement::Return { .. } | Statement::Stop { .. } => true,
        Statement::If {
            branches,
            otherwise: Some(otherwise),
            ..
        } => {
            branches
                .iter()
                .all(|branch| guarantees_return(&branch.body.statements))
                && guarantees_return(&otherwise.statements)
        }
        _ => false,
    })
}
pub(crate) fn statement_span(statement: &Statement) -> Span {
    match statement {
        Statement::If { span, .. }
        | Statement::While { span, .. }
        | Statement::Repeat { span, .. }
        | Statement::For { span, .. }
        | Statement::Binding { span, .. }
        | Statement::Assignment { span, .. }
        | Statement::Return { span, .. }
        | Statement::Print { span, .. }
        | Statement::ClearScreen { span, .. }
        | Statement::Beep { span, .. }
        | Statement::Delete { span, .. }
        | Statement::Stop { span, .. }
        | Statement::Control { span, .. }
        | Statement::Call { span, .. }
        | Statement::MemberFunction { span, .. } => *span,
    }
}
pub(crate) fn validate_member_names(statements: &[Statement]) -> Result<(), Diagnostic> {
    let mut names = std::collections::HashSet::new();
    for statement in statements {
        let (name, span) = match statement {
            Statement::Binding { name, span, .. }
            | Statement::MemberFunction { name, span, .. } => (name, *span),
            _ => continue,
        };
        if !names.insert(name) {
            return Err(error(
                "DUPLICATE_NAME",
                format!("duplicate member '{name}'"),
                span,
            ));
        }
    }
    Ok(())
}
pub(crate) fn block_uses_self(statements: &[Statement]) -> bool {
    statements.iter().any(|statement| match statement {
        Statement::Assignment { target, value, .. } => {
            expression_uses_self(target) || expression_uses_self(value)
        }
        Statement::Return { value, .. } => value.as_ref().is_some_and(expression_uses_self),
        Statement::Print { values, .. } => values.iter().any(expression_uses_self),
        Statement::ClearScreen { console: value, .. }
        | Statement::Beep { console: value, .. }
        | Statement::Delete { value, .. }
        | Statement::Stop { code: value, .. }
        | Statement::Call {
            expression: value, ..
        } => expression_uses_self(value),
        Statement::Binding { initializer, .. } => {
            initializer.as_ref().is_some_and(expression_uses_self)
        }
        Statement::If {
            branches,
            otherwise,
            ..
        } => {
            branches.iter().any(|branch| {
                expression_uses_self(&branch.condition) || block_uses_self(&branch.body.statements)
            }) || otherwise
                .as_ref()
                .is_some_and(|block| block_uses_self(&block.statements))
        }
        Statement::While {
            condition, body, ..
        }
        | Statement::Repeat {
            condition, body, ..
        } => expression_uses_self(condition) || block_uses_self(&body.statements),
        Statement::For { body, .. }
        | Statement::MemberFunction {
            body: Some(body), ..
        } => block_uses_self(&body.statements),
        _ => false,
    })
}
pub(crate) fn statement_uses_super(statement: &Statement) -> bool {
    match statement {
        Statement::Assignment { target, value, .. } => {
            expression_uses_super(target) || expression_uses_super(value)
        }
        Statement::Return { value, .. } => value.as_ref().is_some_and(expression_uses_super),
        Statement::Print { values, .. } => values.iter().any(expression_uses_super),
        Statement::ClearScreen { console, .. }
        | Statement::Beep { console, .. }
        | Statement::Delete { value: console, .. }
        | Statement::Stop { code: console, .. }
        | Statement::Call {
            expression: console,
            ..
        } => expression_uses_super(console),
        Statement::Binding { initializer, .. } => {
            initializer.as_ref().is_some_and(expression_uses_super)
        }
        Statement::If {
            branches,
            otherwise,
            ..
        } => {
            branches.iter().any(|branch| {
                expression_uses_super(&branch.condition)
                    || statement_block_uses_super(&branch.body.statements)
            }) || otherwise
                .as_ref()
                .is_some_and(|body| statement_block_uses_super(&body.statements))
        }
        Statement::While {
            condition, body, ..
        }
        | Statement::Repeat {
            condition, body, ..
        } => expression_uses_super(condition) || statement_block_uses_super(&body.statements),
        Statement::For { body, .. }
        | Statement::MemberFunction {
            body: Some(body), ..
        } => statement_block_uses_super(&body.statements),
        _ => false,
    }
}
pub(crate) fn statement_has_invalid_super(statement: &Statement) -> bool {
    match statement {
        Statement::Assignment { target, value, .. } => {
            expression_has_invalid_super(target) || expression_has_invalid_super(value)
        }
        Statement::Return { value, .. } => value.as_ref().is_some_and(expression_has_invalid_super),
        Statement::Print { values, .. } => values.iter().any(expression_has_invalid_super),
        Statement::ClearScreen { console, .. }
        | Statement::Beep { console, .. }
        | Statement::Delete { value: console, .. }
        | Statement::Stop { code: console, .. }
        | Statement::Call {
            expression: console,
            ..
        } => expression_has_invalid_super(console),
        Statement::Binding { initializer, .. } => initializer
            .as_ref()
            .is_some_and(expression_has_invalid_super),
        Statement::If {
            branches,
            otherwise,
            ..
        } => {
            branches.iter().any(|branch| {
                expression_has_invalid_super(&branch.condition)
                    || branch
                        .body
                        .statements
                        .iter()
                        .any(statement_has_invalid_super)
            }) || otherwise
                .as_ref()
                .is_some_and(|body| body.statements.iter().any(statement_has_invalid_super))
        }
        Statement::While {
            condition, body, ..
        }
        | Statement::Repeat {
            condition, body, ..
        } => {
            expression_has_invalid_super(condition)
                || body.statements.iter().any(statement_has_invalid_super)
        }
        Statement::For { body, .. }
        | Statement::MemberFunction {
            body: Some(body), ..
        } => body.statements.iter().any(statement_has_invalid_super),
        _ => false,
    }
}
pub(crate) fn statement_block_uses_super(statements: &[Statement]) -> bool {
    statements.iter().any(statement_uses_super)
}
pub(crate) fn expression_uses_self(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::Name { name } => name == "SELF",
        ExpressionKind::Unary { operand, .. }
        | ExpressionKind::Cast { value: operand, .. }
        | ExpressionKind::Length { operand }
        | ExpressionKind::SizeOf { operand } => expression_uses_self(operand),
        ExpressionKind::Binary { left, right, .. } => {
            expression_uses_self(left) || expression_uses_self(right)
        }
        ExpressionKind::Call { callee, arguments } => {
            expression_uses_self(callee) || arguments.iter().any(expression_uses_self)
        }
        ExpressionKind::Member { object, .. } => expression_uses_self(object),
        ExpressionKind::Index { object, index } => {
            expression_uses_self(object) || expression_uses_self(index)
        }
        ExpressionKind::Vector { values }
        | ExpressionKind::New {
            arguments: values, ..
        } => values.iter().any(expression_uses_self),
        _ => false,
    }
}
