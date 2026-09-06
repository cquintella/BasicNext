#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn allocation_length(arguments: &[Expression]) -> PointerLength {
    arguments.first().map_or(PointerLength::One, |length| {
        let ExpressionKind::Literal(Literal::Integer(length)) = &length.kind else {
            return PointerLength::Dynamic;
        };
        parse_integer(length)
            .and_then(|length| u64::try_from(length).ok())
            .map_or(PointerLength::Dynamic, PointerLength::Fixed)
    })
}
pub(crate) fn literal_type(literal: &Literal) -> Type {
    match literal {
        Literal::Integer(value) => Type::IntegerLiteral(value.clone()),
        Literal::Float(_) | Literal::Special(_) => Type::FloatLiteral,
        Literal::String(_) => Type::String,
        Literal::TypeName(_) => Type::Unknown,
        Literal::Boolean(_) => Type::Boolean,
        Literal::Null => Type::Null,
        Literal::NotAvailable => Type::NotAvailable,
        Literal::EndOfFile => Type::EndOfFile,
    }
}
pub(crate) fn is_test_type(expression: &Expression) -> Option<Type> {
    match &expression.kind {
        ExpressionKind::TypeTest { type_ref } => Some(type_from_reference(type_ref)),
        ExpressionKind::Literal(Literal::Null) => Some(Type::Null),
        ExpressionKind::Literal(Literal::NotAvailable) => Some(Type::NotAvailable),
        ExpressionKind::Literal(Literal::EndOfFile) => Some(Type::EndOfFile),
        ExpressionKind::Literal(Literal::Special(_)) => Some(Type::FloatLiteral),
        ExpressionKind::Unary { operator, operand } if operator == "Minus" => is_test_type(operand),
        ExpressionKind::Literal(Literal::TypeName(name)) | ExpressionKind::Name { name } => {
            Some(type_from_name(name))
        }
        _ => None,
    }
}
pub(crate) fn type_from_name(name: &str) -> Type {
    match name {
        "BOOLEAN" => Type::Boolean,
        "BYTE" | "INT8" | "INT16" | "INT32" | "INT64" | "UINT16" | "UINT32" | "UINT64"
        | "INTEGER" | "TIMESTAMP" => Type::Integer(integer_type(name).expect("numeric type")),
        "FLOAT32" | "FLOAT64" | "FLOAT" => Type::Float(float_type(name).expect("float type")),
        "STRING" => Type::String,
        "SYSTEM" => Type::System,
        name => Type::Named(name.into()),
    }
}
pub(crate) fn integer_type(name: &str) -> Option<IntegerType> {
    match name {
        "BYTE" => Some(IntegerType::Byte),
        "INT8" => Some(IntegerType::Int8),
        "INT16" => Some(IntegerType::Int16),
        "INT32" | "INTEGER" => Some(IntegerType::Int32),
        "INT64" | "TIMESTAMP" => Some(IntegerType::Int64),
        "UINT16" => Some(IntegerType::UInt16),
        "UINT32" => Some(IntegerType::UInt32),
        "UINT64" => Some(IntegerType::UInt64),
        _ => None,
    }
}
pub(crate) fn float_type(name: &str) -> Option<FloatType> {
    match name {
        "FLOAT32" => Some(FloatType::Float32),
        "FLOAT64" | "FLOAT" => Some(FloatType::Float64),
        _ => None,
    }
}

pub(crate) fn integer_literal_fits(value: &str, target: IntegerType) -> bool {
    let parsed = parse_integer(value);
    let Some(value) = parsed else { return false };
    let (minimum, maximum) = integer_range(target);
    (minimum..=maximum).contains(&value)
}

pub(crate) fn integer_range(target: IntegerType) -> (i128, i128) {
    match target {
        IntegerType::Byte => (0, 255),
        IntegerType::Int8 => (-128, 127),
        IntegerType::Int16 => (-32_768, 32_767),
        IntegerType::Int32 => (-2_147_483_648, 2_147_483_647),
        IntegerType::Int64 => (i128::from(i64::MIN), i128::from(i64::MAX)),
        IntegerType::UInt16 => (0, 65_535),
        IntegerType::UInt32 => (0, 4_294_967_295),
        IntegerType::UInt64 => (0, i128::from(u64::MAX)),
    }
}

pub(crate) fn parse_integer(value: &str) -> Option<i128> {
    if let Some(value) = value.strip_prefix("0b") {
        i128::from_str_radix(value, 2).ok()
    } else if let Some(value) = value.strip_prefix("0x") {
        i128::from_str_radix(value, 16).ok()
    } else {
        value.parse().ok()
    }
}

pub(crate) fn constant_integer_from_type(ty: &Type) -> Option<i128> {
    let Type::IntegerLiteral(value) = ty else {
        return None;
    };
    parse_integer(value)
}

pub(crate) fn constant_integer(expression: &Expression) -> Option<i128> {
    match &expression.kind {
        ExpressionKind::Literal(Literal::Integer(value)) => parse_integer(value),
        ExpressionKind::Unary { operator, operand } if operator == "Minus" => {
            constant_integer(operand)?.checked_neg()
        }
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } => {
            let left = constant_integer(left)?;
            let right = constant_integer(right)?;
            match operator.as_str() {
                "Plus" => left.checked_add(right),
                "Minus" => left.checked_sub(right),
                "Star" => left.checked_mul(right),
                "DIV" if right != 0 => left.checked_div_euclid(right),
                "Percent" if right != 0 => left.checked_rem_euclid(right),
                "Power" if right >= 0 => left.checked_pow(u32::try_from(right).ok()?),
                "SHL" if (0..128).contains(&right) => left.checked_shl(u32::try_from(right).ok()?),
                "SHR" if (0..128).contains(&right) => left.checked_shr(u32::try_from(right).ok()?),
                _ => None,
            }
        }
        _ => None,
    }
}

pub(crate) fn default_literal_type(ty: Type) -> Type {
    match ty {
        Type::IntegerLiteral(_) => Type::Integer(IntegerType::Int32),
        Type::FloatLiteral => Type::Float(FloatType::Float64),
        other => other,
    }
}

pub(crate) fn is_numeric(ty: &Type) -> bool {
    is_integer(ty) || is_float(ty)
}

#[must_use]
pub(crate) fn static_len(ty: &Type) -> Option<u64> {
    if is_numeric(ty) {
        return Some(1);
    }
    if let Type::Vector { dimensions, .. } = ty {
        return dimension_product(dimensions);
    }
    None
}

#[must_use]
pub(crate) fn static_size_of(ty: &Type) -> Option<u64> {
    match ty {
        Type::Boolean => Some(1),
        Type::Integer(kind) => Some(integer_byte_size(*kind)),
        Type::IntegerLiteral(_) | Type::Float(FloatType::Float32) => Some(4),
        Type::Float(FloatType::Float64) | Type::FloatLiteral => Some(8),
        Type::Named(name) if name == "DATE" || name == "TIME" => Some(4),
        Type::Vector {
            element,
            dimensions,
        } => static_size_of(element)
            .and_then(|element| dimension_product(dimensions)?.checked_mul(element)),
        _ => None,
    }
}

#[must_use]
pub(crate) fn integer_byte_size(kind: IntegerType) -> u64 {
    match kind {
        IntegerType::Byte | IntegerType::Int8 => 1,
        IntegerType::Int16 | IntegerType::UInt16 => 2,
        IntegerType::Int32 | IntegerType::UInt32 => 4,
        IntegerType::Int64 | IntegerType::UInt64 => 8,
    }
}

pub(crate) fn dimension_product(dimensions: &[u64]) -> Option<u64> {
    if dimensions.contains(&u64::MAX) {
        return None;
    }
    dimensions
        .iter()
        .try_fold(1u64, |product, dimension| product.checked_mul(*dimension))
}

pub(crate) fn host_capability_type(name: &str, span: Span) -> Result<Type, Diagnostic> {
    match name {
        "Args" => Ok(Type::HostArgs),
        "Console" => Ok(Type::HostConsole),
        "Main" => Err(error(
            "NAME_NOT_FOUND",
            "HOST.Main was withdrawn in 0.2; use HOST.Args",
            span,
        )),
        "Clock" => Ok(Type::HostClock),
        "Random" => Ok(Type::HostRandom),
        "FileSystem" => Ok(Type::HostFileSystem),
        "Net" => Ok(Type::HostNet),
        "NumProcs" => Ok(Type::Function {
            parameters: Vec::new(),
            return_type: Box::new(Type::Alternative(vec![
                Type::Integer(IntegerType::Int32),
                Type::Named("Error".into()),
            ])),
        }),
        _ => Err(error(
            "NAME_NOT_FOUND",
            format!("HOST.{name} is not a Basic Next 0.2 capability"),
            span,
        )),
    }
}

pub(crate) fn length_type(ty: &Type, span: Span) -> Result<Type, Diagnostic> {
    if is_numeric(ty) {
        return Ok(Type::Integer(IntegerType::Int32));
    }
    match ty {
        Type::HostArgs
        | Type::String
        | Type::Pointer {
            length: PointerLength::Dynamic,
            ..
        } => Ok(Type::Integer(IntegerType::Int32)),
        Type::Vector { dimensions, .. } => {
            if !dimensions.contains(&u64::MAX) {
                require_integer_fit(dimension_product(dimensions), span)?;
            }
            Ok(Type::Integer(IntegerType::Int32))
        }
        Type::Pointer {
            length: PointerLength::Fixed(length),
            ..
        } => {
            require_integer_fit(Some(*length), span)?;
            Ok(Type::Integer(IntegerType::Int32))
        }
        _ => Err(error(
            "TYPE_MISMATCH",
            "LEN requires a numeric value, STRING, vector, or pointer region",
            span,
        )),
    }
}

pub(crate) fn require_integer_fit(value: Option<u64>, span: Span) -> Result<(), Diagnostic> {
    match value {
        Some(value) if value <= 2_147_483_647 => Ok(()),
        None | Some(_) => Err(error(
            "NUMERIC_OVERFLOW",
            "result does not fit INTEGER",
            span,
        )),
    }
}
pub(crate) fn requires_initializer(reference: &TypeReference) -> bool {
    reference.alternatives.len() > 1
        || reference
            .alternatives
            .iter()
            .any(|atom| matches!(atom.name.as_str(), "POINTER" | "FUNCTION"))
}
pub(crate) fn compatible(expected: &Type, actual: &Type) -> bool {
    match (expected, actual) {
        (
            Type::Function {
                parameters: expected_parameters,
                return_type: expected_return,
            },
            Type::Function {
                parameters: actual_parameters,
                return_type: actual_return,
            },
        ) => {
            expected_parameters.len() == actual_parameters.len()
                && expected_parameters
                    .iter()
                    .zip(actual_parameters)
                    .all(|(expected, actual)| compatible(expected, actual))
                && compatible(expected_return, actual_return)
        }
        (
            Type::Named(expected),
            Type::ImportedNamed { name, .. } | Type::ImportedTypeName { name, .. },
        )
        | (
            Type::ImportedNamed { name, .. } | Type::ImportedTypeName { name, .. },
            Type::Named(expected),
        ) => expected.rsplit('.').next() == Some(name.as_str()),
        (Type::Named(expected), Type::Named(actual)) => {
            expected == actual
                || (expected.starts_with("Net.") && actual == &format!("HOST.{expected}"))
                || (actual.starts_with("Net.") && expected == &format!("HOST.{actual}"))
        }
        (Type::Alternative(expected), Type::Alternative(actual)) => actual
            .iter()
            .all(|actual| expected.iter().any(|expected| compatible(expected, actual))),
        (Type::Alternative(expected), actual) => {
            expected.iter().any(|expected| compatible(expected, actual))
        }
        (expected, Type::Alternative(actual)) => {
            actual.iter().all(|actual| compatible(expected, actual))
        }
        (Type::Integer(expected), Type::IntegerLiteral(value)) => {
            integer_literal_fits(value, *expected)
        }
        (Type::Float(_), Type::FloatLiteral) => true,
        (
            Type::Vector {
                element: expected,
                dimensions: expected_dimensions,
            },
            Type::Vector {
                element: actual,
                dimensions: actual_dimensions,
            },
        ) => {
            (expected_dimensions == actual_dimensions
                || (expected_dimensions.len() == 1 && expected_dimensions[0] == u64::MAX))
                && compatible(expected, actual)
        }
        (
            Type::Pointer {
                element: expected,
                length: expected_length,
            },
            Type::Pointer {
                element: actual,
                length: actual_length,
            },
        ) => {
            (is_void(expected) || is_void(actual) || compatible(expected, actual))
                && pointer_lengths_compatible(*expected_length, *actual_length)
        }
        (expected, actual) => expected == actual,
    }
}
pub(crate) fn pointer_lengths_compatible(expected: PointerLength, actual: PointerLength) -> bool {
    match (expected, actual) {
        (PointerLength::One, PointerLength::One)
        | (PointerLength::Dynamic, PointerLength::Fixed(_) | PointerLength::Dynamic)
        | (PointerLength::Fixed(_), PointerLength::Dynamic) => true,
        (PointerLength::Fixed(expected), PointerLength::Fixed(actual)) => expected == actual,
        _ => false,
    }
}
pub(crate) fn is_void(ty: &Type) -> bool {
    matches!(ty, Type::Named(name) if name == "VOID")
}
pub(crate) fn pointer_literal_length_mismatch(expected: &Type, actual: &Type) -> bool {
    matches!(
        (expected, actual),
        (
            Type::Pointer {
                length: PointerLength::Fixed(expected),
                ..
            },
            Type::Pointer {
                length: PointerLength::Fixed(actual),
                ..
            }
        ) if expected != actual
    )
}
pub(crate) fn is_integer(ty: &Type) -> bool {
    matches!(ty, Type::Integer(_) | Type::IntegerLiteral(_))
        || matches!(ty, Type::Alternative(alternatives) if alternatives.iter().all(is_integer))
}
pub(crate) fn is_float(ty: &Type) -> bool {
    matches!(ty, Type::Float(_) | Type::FloatLiteral)
        || matches!(ty, Type::Alternative(alternatives) if alternatives.iter().all(is_float))
}
pub(crate) fn integer_literal(expression: &Expression) -> Option<i64> {
    let ExpressionKind::Literal(Literal::Integer(value)) = &expression.kind else {
        return None;
    };
    if let Some(value) = value.strip_prefix("0b") {
        i64::from_str_radix(value, 2).ok()
    } else if let Some(value) = value.strip_prefix("0x") {
        i64::from_str_radix(value, 16).ok()
    } else {
        value.parse().ok()
    }
}
pub(crate) fn vector_shape(expression: &Expression) -> Option<Vec<usize>> {
    let ExpressionKind::Vector { values } = &expression.kind else {
        return Some(Vec::new());
    };
    let mut element_shape = None;
    for value in values {
        let shape = vector_shape(value)?;
        if let Some(expected) = &element_shape {
            if expected != &shape {
                return None;
            }
        } else {
            element_shape = Some(shape);
        }
    }
    let mut shape = vec![values.len()];
    if let Some(element_shape) = element_shape {
        shape.extend(element_shape);
    }
    Some(shape)
}
pub(crate) fn display(ty: &Type) -> String {
    match ty {
        Type::Named(name) | Type::TypeName(name) => name.clone(),
        Type::ImportedNamed { module, name } | Type::ImportedTypeName { module, name } => {
            format!("MODULE#{}.{}", module.0, name)
        }
        Type::System => "SYSTEM".into(),
        Type::HostClock => "HOST.Clock".into(),
        Type::HostRandom => "HOST.Random".into(),
        Type::HostConsole => "HOST.Console".into(),
        Type::HostFileSystem => "HOST.FileSystem".into(),
        Type::HostNet => "HOST.Net".into(),
        Type::Module(id) => format!("MODULE {}", id.0),
        Type::Alternative(alternatives) => alternatives
            .iter()
            .map(display)
            .collect::<Vec<_>>()
            .join(" OR "),
        Type::Function {
            parameters,
            return_type,
        } => format!(
            "FUNCTION({}) AS {}",
            parameters
                .iter()
                .map(display)
                .collect::<Vec<_>>()
                .join(", "),
            display(return_type)
        ),
        Type::Vector {
            element,
            dimensions,
        } => format!(
            "{}{}",
            display(element),
            dimensions
                .iter()
                .fold(String::new(), |mut name, dimension| {
                    use std::fmt::Write;
                    if *dimension == u64::MAX {
                        name.push_str("[]");
                    } else {
                        let _ = write!(name, "[{dimension}]");
                    }
                    name
                })
        ),
        Type::Pointer { element, length } => match length {
            PointerLength::One => format!("POINTER TO {}", display(element)),
            PointerLength::Fixed(length) => {
                format!("POINTER TO {}[{length}]", display(element))
            }
            PointerLength::Dynamic => format!("POINTER TO {}[]", display(element)),
        },
        other => format!("{other:?}").to_uppercase(),
    }
}

pub(crate) fn typeof_name(ty: &Type) -> String {
    match ty {
        Type::Boolean => "BOOLEAN".into(),
        Type::Integer(IntegerType::Byte) => "BYTE".into(),
        Type::Integer(IntegerType::Int8) => "INT8".into(),
        Type::Integer(IntegerType::Int16) => "INT16".into(),
        Type::Integer(IntegerType::Int32) | Type::IntegerLiteral(_) => "INT32".into(),
        Type::Integer(IntegerType::Int64) => "INT64".into(),
        Type::Integer(IntegerType::UInt16) => "UINT16".into(),
        Type::Integer(IntegerType::UInt32) => "UINT32".into(),
        Type::Integer(IntegerType::UInt64) => "UINT64".into(),
        Type::Float(FloatType::Float32) => "FLOAT32".into(),
        Type::Float(FloatType::Float64) | Type::FloatLiteral => "FLOAT64".into(),
        Type::String => "STRING".into(),
        Type::Null => "NULL".into(),
        Type::NotAvailable => "NA".into(),
        Type::EndOfFile => "EOF".into(),
        Type::Alternative(types) => types
            .iter()
            .map(typeof_name)
            .collect::<Vec<_>>()
            .join(" OR "),
        Type::Vector {
            element,
            dimensions,
        } => format!(
            "{}{}",
            typeof_name(element),
            dimensions
                .iter()
                .fold(String::new(), |mut name, dimension| {
                    use std::fmt::Write;
                    let _ = write!(name, "[{dimension}]");
                    name
                })
        ),
        other => display(other),
    }
}
