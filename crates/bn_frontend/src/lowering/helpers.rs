#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn module_id_from_prefix(prefix: &str) -> FrontendModuleId {
    if prefix.starts_with('#') && prefix.ends_with('.') {
        prefix[1..prefix.len() - 1]
            .parse::<u32>()
            .map_or(FrontendModuleId(0), FrontendModuleId)
    } else {
        FrontendModuleId(0)
    }
}

pub(crate) fn type_at(model: &SemanticModel, span: Span) -> Result<Type, Diagnostic> {
    model
        .expression(span)
        .map(|expression| expression.ty.clone())
        .or_else(|| model.symbol_at(span).map(|symbol| symbol.ty.clone()))
        .ok_or_else(|| ir_error("type information is missing for IR lowering", span))
}

pub(crate) fn host_capability_constant(name: &str, span: Span) -> Result<Constant, Diagnostic> {
    match name {
        "Args" => Ok(Constant::HostArgs),
        "Console" => Ok(Constant::HostConsole),
        "Clock" => Ok(Constant::Type("HOST.Clock".into())),
        "NumProcs" => Ok(Constant::Function("HOST.NumProcs".into())),
        _ => Err(ir_error(format!("HOST.{name} cannot be lowered"), span)),
    }
}

pub(crate) fn constant(literal: &Literal) -> Constant {
    match literal {
        Literal::Integer(value) => Constant::Integer(value.clone()),
        Literal::Float(value) | Literal::Special(value) => Constant::Float(value.clone()),
        Literal::String(value) => Constant::String(value.clone()),
        Literal::TypeName(value) => Constant::Type(value.clone()),
        Literal::Boolean(value) => Constant::Boolean(*value),
        Literal::Null => Constant::Null,
        Literal::NotAvailable => Constant::NotAvailable,
        Literal::EndOfFile => Constant::EndOfFile,
    }
}

pub(crate) fn module_constant(value: &crate::semantic::ConstantValue) -> Constant {
    match value {
        crate::semantic::ConstantValue::Integer(value) => Constant::Integer(value.clone()),
        crate::semantic::ConstantValue::Float(value) => Constant::Float(value.clone()),
        crate::semantic::ConstantValue::String(value) => Constant::String(value.clone()),
        crate::semantic::ConstantValue::Boolean(value) => Constant::Boolean(*value),
        crate::semantic::ConstantValue::Null => Constant::Null,
        crate::semantic::ConstantValue::NotAvailable => Constant::NotAvailable,
        crate::semantic::ConstantValue::EndOfFile => Constant::EndOfFile,
    }
}

pub(crate) fn assignment_operator(operator: &str) -> Result<&'static str, Diagnostic> {
    match operator {
        "PlusAssign" => Ok("Plus"),
        "MinusAssign" => Ok("Minus"),
        "StarAssign" => Ok("Star"),
        "SlashAssign" => Ok("Slash"),
        "PercentAssign" => Ok("Percent"),
        "PowerAssign" => Ok("Power"),
        _ => Err(ir_error("unknown assignment operator", default_span())),
    }
}

pub(crate) fn ir_error(message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic::structured(
        bn_diag::DiagId::IR_LOWERING,
        vec![("detail".into(), message.into().into())],
        vec![bn_diag::Label {
            span,
            style: bn_diag::LabelStyle::Primary,
            text: None,
        }],
    )
    .expect("IR lowering diagnostic schema is registered")
}

pub(crate) fn type_test_name(atom: &TypeAtom) -> String {
    // A vector test is named `ELEMENT[n]...`, the spelling `bn_interp`'s
    // `is_value` reads back.
    if !atom.dimensions.is_empty() {
        return atom
            .dimensions
            .iter()
            .fold(atom.name.clone(), |name, dimension| match dimension {
                crate::ast::VectorDimension::Literal { value, .. } => format!("{name}[{value}]"),
                crate::ast::VectorDimension::Expression(_) => format!("{name}[]"),
            });
    }
    let dotted = atom.parts.iter().any(|part| part == "." || part == "Dot");
    let names: Vec<&str> = std::iter::once(atom.name.as_str())
        .chain(
            atom.parts
                .iter()
                .filter(|part| *part != "." && *part != "Dot")
                .map(String::as_str),
        )
        .collect();
    if dotted {
        names.join(".")
    } else {
        names.join(" ")
    }
}

pub(crate) fn named_or_void(reference: &TypeReference) -> Type {
    if reference.alternatives.is_empty() {
        Type::Named("VOID".into())
    } else {
        crate::semantic::type_from_reference(reference)
    }
}

pub(crate) fn is_namespace_type(ty: &Type) -> bool {
    matches!(
        ty,
        Type::TypeName(_)
            | Type::ImportedTypeName { .. }
            | Type::HostClock
            | Type::HostRandom
            | Type::HostConsole
            | Type::HostFileSystem
            | Type::HostNet
            | Type::HostExec
            | Type::HostEnv
            | Type::Module(_)
    )
}

pub(crate) fn math_constant(name: &str) -> Option<Constant> {
    let value = match name {
        "MAX_INT8" => Constant::Integer("127".into()),
        "MIN_INT8" => Constant::Integer("-128".into()),
        "MAX_INT16" => Constant::Integer("32767".into()),
        "MIN_INT16" => Constant::Integer("-32768".into()),
        "MAX_INTEGER" | "MAX_INT32" => Constant::Integer("2147483647".into()),
        "MIN_INTEGER" | "MIN_INT32" => Constant::Integer("-2147483648".into()),
        "MAX_INT64" | "MAX_TIMESTAMP" => Constant::Integer("9223372036854775807".into()),
        "MIN_INT64" | "MIN_TIMESTAMP" => Constant::Integer("-9223372036854775808".into()),
        "MAX_BYTE" | "MAX_UINT16" => {
            Constant::Integer(if name == "MAX_BYTE" { "255" } else { "65535" }.into())
        }
        "MIN_BYTE" | "MIN_UINT16" | "MIN_UINT32" | "MIN_UINT64" => Constant::Integer("0".into()),
        "MAX_UINT32" => Constant::Integer("4294967295".into()),
        "MAX_UINT64" => Constant::Integer("18446744073709551615".into()),
        "MAX_FLOAT32" => Constant::Float("3.4028234663852886e38".into()),
        "MIN_FLOAT32" => Constant::Float("-3.4028234663852886e38".into()),
        "MIN_POSITIVE_FLOAT32" => Constant::Float("1.1754943508222875e-38".into()),
        "MAX_FLOAT" | "MAX_FLOAT64" => Constant::Float("1.7976931348623157e308".into()),
        "MIN_FLOAT" | "MIN_FLOAT64" => Constant::Float("-1.7976931348623157e308".into()),
        "MIN_POSITIVE_FLOAT" | "MIN_POSITIVE_FLOAT64" => {
            Constant::Float("2.2250738585072014e-308".into())
        }
        _ => return None,
    };
    Some(value)
}

pub(crate) fn filesystem_import_span(program: &Program) -> Option<Span> {
    program.items.iter().find_map(|item| match item {
        Item::Import { path, span, .. }
            if path.len() == 2 && path[0] == "HOST" && path[1] == "FileSystem" =>
        {
            Some(*span)
        }
        _ => None,
    })
}

pub(crate) fn console_import_span(program: &Program) -> Option<Span> {
    program.items.iter().find_map(|item| match item {
        Item::Import { path, span, .. }
            if path.len() == 2 && path[0] == "HOST" && path[1] == "Console" =>
        {
            Some(*span)
        }
        _ => None,
    })
}

pub(crate) fn clock_import_span(program: &Program) -> Option<Span> {
    program.items.iter().find_map(|item| match item {
        Item::Import { path, span, .. }
            if path.len() == 2 && path[0] == "HOST" && path[1] == "Clock" =>
        {
            Some(*span)
        }
        _ => None,
    })
}

pub(crate) fn random_import_span(program: &Program) -> Option<Span> {
    program.items.iter().find_map(|item| match item {
        Item::Import { path, span, .. }
            if path.len() == 2 && path[0] == "HOST" && path[1] == "Random" =>
        {
            Some(*span)
        }
        _ => None,
    })
}

pub(crate) fn network_import_span(program: &Program) -> Option<Span> {
    program.items.iter().find_map(|item| match item {
        Item::Import { path, span, .. }
            if path.as_slice() == ["HOST".to_string(), "Net".to_string()] =>
        {
            Some(*span)
        }
        _ => None,
    })
}

pub(crate) fn exec_import_span(program: &Program) -> Option<Span> {
    program.items.iter().find_map(|item| match item {
        Item::Import { path, span, .. }
            if path.len() == 2 && path[0] == "HOST" && path[1] == "Exec" =>
        {
            Some(*span)
        }
        _ => None,
    })
}

pub(crate) fn env_import_span(program: &Program) -> Option<Span> {
    program.items.iter().find_map(|item| match item {
        Item::Import { path, span, .. }
            if path.len() == 2 && path[0] == "HOST" && path[1] == "Env" =>
        {
            Some(*span)
        }
        _ => None,
    })
}

pub(crate) fn standard_import_span(program: &Program, name: &str) -> Option<Span> {
    program.items.iter().find_map(|item| match item {
        Item::Import { path, span, .. } if path.len() == 1 && path[0] == name => Some(*span),
        _ => None,
    })
}

/// The value of an `INTEGER` member of a HOST capability: an open mode or a
/// portable error code from one table in `bn_types::error_codes`.
pub(crate) fn capability_constant(object_type: &Type, name: &str) -> Option<Constant> {
    let value = match object_type {
        Type::HostFileSystem => match name {
            "READ" => 0,
            "WRITE" => 1,
            "APPEND" => 2,
            _ => {
                bn_types::error_codes::fs::ALL
                    .iter()
                    .find(|(code, _)| *code == name)?
                    .1
            }
        },
        Type::HostNet => {
            bn_types::error_codes::net::ALL
                .iter()
                .find(|(code, _)| *code == name)?
                .1
        }
        Type::HostExec => {
            bn_types::error_codes::exec::ALL
                .iter()
                .find(|(code, _)| *code == name)?
                .1
        }
        Type::HostEnv => {
            bn_types::error_codes::env::ALL
                .iter()
                .find(|(code, _)| *code == name)?
                .1
        }
        _ => return None,
    };
    Some(Constant::Integer(value.to_string()))
}

pub(crate) fn namespace_function(object_type: &Type, name: &str, prefix: &str) -> Option<String> {
    match object_type {
        Type::TypeName(owner) if user_class_name(object_type).is_some() => {
            Some(format!("{prefix}{owner}.{name}"))
        }
        Type::TypeName(owner) => Some(format!("{owner}.{name}")),
        Type::ImportedTypeName {
            module,
            name: owner,
        } => Some(format!("#{}.{owner}.{name}", module.0)),
        Type::Module(module) => Some(format!("#{}.{name}", module.0)),
        Type::HostClock => Some(format!("HOST.Clock.{name}")),
        Type::HostRandom => Some(format!("HOST.Random.{name}")),
        Type::HostConsole => Some(format!("HOST.Console.{name}")),
        Type::HostFileSystem => Some(format!("HOST.FileSystem.{name}")),
        Type::HostNet => Some(format!("HOST.Net.{name}")),
        Type::HostExec => Some(format!("HOST.Exec.{name}")),
        Type::HostEnv => Some(format!("HOST.Env.{name}")),
        _ => None,
    }
}

pub(crate) fn is_host_or_builtin_owner(name: &str) -> bool {
    name.starts_with("HOST.")
        || name == "FS.File"
        || name.ends_with(".File")
        || matches!(
            name,
            "Error" | "SYSTEM" | "Float" | "Date" | "Time" | "TimeZone" | "Timestamp"
        )
}

pub(crate) fn is_user_class_name(name: &str) -> bool {
    !is_host_or_builtin_owner(name)
}

pub(crate) fn user_class_name(ty: &Type) -> Option<String> {
    match ty {
        Type::TypeName(name)
            if !matches!(
                name.as_str(),
                "Float" | "Date" | "Time" | "TimeZone" | "Timestamp" | "Error" | "SYSTEM"
            ) =>
        {
            Some(name.clone())
        }
        Type::ImportedTypeName { name, .. } => Some(name.clone()),
        _ => None,
    }
}

pub(crate) fn static_class_name(ty: &Type, prefix: &str) -> String {
    match ty {
        Type::TypeName(name) if is_user_class_name(name) => format!("{prefix}{name}"),
        Type::TypeName(name) => name.clone(),
        Type::ImportedTypeName { module, name } => format!("#{}.{name}", module.0),
        other => display_type(other),
    }
}

pub(crate) fn class_ir_name(ty: &Type, type_name: &str, prefix: &str) -> String {
    match ty {
        Type::Named(name) | Type::TypeName(name) => {
            if is_user_class_name(name) {
                format!("{prefix}{name}")
            } else {
                name.clone()
            }
        }
        Type::ImportedNamed { module, name } | Type::ImportedTypeName { module, name } => {
            format!("#{}.{name}", module.0)
        }
        _ => {
            if is_numeric_type_name(type_name) || !is_user_class_name(type_name) {
                type_name.into()
            } else {
                format!("{prefix}{type_name}")
            }
        }
    }
}

pub(crate) fn is_numeric_type_name(name: &str) -> bool {
    matches!(
        name,
        "BYTE"
            | "INT8"
            | "INT16"
            | "INT32"
            | "INT64"
            | "INTEGER"
            | "UINT16"
            | "UINT32"
            | "UINT64"
            | "FLOAT32"
            | "FLOAT64"
            | "FLOAT"
            | "TIMESTAMP"
    )
}

pub(crate) fn destructor_name(
    model: &SemanticModel,
    span: Span,
    methods: &HashSet<String>,
    prefix: &str,
) -> Option<String> {
    let ty = type_at(model, span).ok()?;
    let name = match ty {
        Type::Named(name) => format!("{prefix}{name}.DESTRUCTOR"),
        Type::ImportedNamed { module, name } => format!("#{}.{name}.DESTRUCTOR", module.0),
        Type::Alternative(types) => types.iter().find_map(|ty| match ty {
            Type::Named(name) => Some(format!("{prefix}{name}.DESTRUCTOR")),
            Type::ImportedNamed { module, name } => {
                Some(format!("#{}.{name}.DESTRUCTOR", module.0))
            }
            _ => None,
        })?,
        _ => return None,
    };
    methods.contains(&name).then_some(name)
}

pub(crate) fn display_type(ty: &Type) -> String {
    match ty {
        Type::Named(name) | Type::TypeName(name) | Type::ImportedNamed { name, .. } => name.clone(),
        _ => format!("{ty:?}"),
    }
}
