#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

#[path = "executor/helpers.rs"]
mod helpers;
#[allow(unused_imports)]
use self::helpers::{
    integer_from_count, integer_from_u64, lifecycle_dispatch, require_console,
};
pub(crate) use self::helpers::numeric_overflow;
pub(crate) use self::helpers::integer_from_count as integer_from_count_pub;

#[path = "executor/part1.rs"]
mod part1;
#[path = "executor/part2.rs"]
mod part2;
#[path = "executor/part3.rs"]
mod part3;
#[path = "executor/part4.rs"]
mod part4;
#[path = "executor/part5.rs"]
mod part5;
#[path = "executor/part6.rs"]
mod part6;
#[path = "executor/part7.rs"]
mod part7;
#[path = "executor/part8.rs"]
mod part8;
#[path = "executor/part10.rs"]
mod part10;
#[path = "executor/part11.rs"]
mod part11;
#[path = "executor/part14.rs"]
mod part14;
#[path = "executor/part15.rs"]
mod part15;
#[path = "executor/part16.rs"]
mod part16;
#[path = "executor/part17.rs"]
mod part17;
#[path = "executor/part18.rs"]
mod part18;
#[path = "executor/part19.rs"]
mod part19;

fn unary(operator: &str, operand: &Value, ty: &Type, span: Span) -> Result<Value, Diagnostic> {
    match (operator, operand) {
        ("Minus", Value::Integer(value, _)) => checked_integer(value.checked_neg(), ty, span),
        ("Minus", Value::Float(value, _)) => Ok(float_value(-value, float_kind(ty))),
        ("NOT", Value::Boolean(value)) => Ok(Value::Boolean(!value)),
        ("NOT", Value::Integer(value, _)) => checked_integer(Some(!value), ty, span),
        _ => Err(super::type_mismatch("numeric or BOOLEAN operand", "incompatible value", format!("unary {operator}"), span)),
    }
}

#[allow(clippy::too_many_lines)] // Operator behavior is intentionally explicit and centralized.
fn binary(
    operator: &str,
    left: &Value,
    right: &Value,
    ty: &Type,
    span: Span,
) -> Result<Value, Diagnostic> {
    if operator == "IS" {
        let Value::Type(test) = right else {
            return Err(runtime_error(crate::diagnostic::DiagId::INVALID_IR,
                "IS requires a type operand",
                span,
            ));
        };
        return Ok(Value::Boolean(is_value(left, test)));
    }
    if matches!(operator, "Assign" | "NotEqual") {
        let equal = equals(left, right);
        return Ok(Value::Boolean(if operator == "Assign" {
            equal
        } else {
            !equal
        }));
    }
    if let (Value::Boolean(left), Value::Boolean(right)) = (left, right) {
        return match operator {
            "AND" => Ok(Value::Boolean(*left && *right)),
            "OR" => Ok(Value::Boolean(*left || *right)),
            "XOR" => Ok(Value::Boolean(*left ^ *right)),
            _ => Err(super::type_mismatch("AND, OR or XOR", operator, "BOOLEAN operation", span)),
        };
    }
    if let (Value::String(left), Value::String(right)) = (left, right) {
        return match operator {
            "Plus" => Ok(Value::String(format!("{left}{right}"))),
            _ => Err(super::type_mismatch("Plus", operator, "STRING operation", span)),
        };
    }
    if let (Value::Date(left), Value::Date(right)) = (left, right) {
        return ordered(operator, left, right, span);
    }
    if let (Value::Time(left), Value::Time(right)) = (left, right) {
        return ordered(operator, left, right, span);
    }
    if is_float_value(left) || is_float_value(right) || operator == "Slash" {
        let left = number_as_float(left, span)?;
        let right = number_as_float(right, span)?;
        return match operator {
            "Plus" => Ok(float_value(left + right, float_kind(ty))),
            "Minus" => Ok(float_value(left - right, float_kind(ty))),
            "Star" => Ok(float_value(left * right, float_kind(ty))),
            "Slash" => Ok(float_value(left / right, float_kind(ty))),
            "Power" => Ok(float_value(left.powf(right), float_kind(ty))),
            "Less" => Ok(Value::Boolean(left < right)),
            "LessEqual" => Ok(Value::Boolean(left <= right)),
            "Greater" => Ok(Value::Boolean(left > right)),
            "GreaterEqual" => Ok(Value::Boolean(left >= right)),
            _ => Err(super::type_mismatch("numeric operator", operator, "floating operation", span)),
        };
    }
    let (left, _) = integer(left, span)?;
    let (right, _) = integer(right, span)?;
    match operator {
        "Plus" => checked_integer(left.checked_add(right), ty, span),
        "Minus" => checked_integer(left.checked_sub(right), ty, span),
        "Star" => checked_integer(left.checked_mul(right), ty, span),
        "DIV" if right != 0 => checked_integer(left.checked_div_euclid(right), ty, span),
        "Percent" if right != 0 => checked_integer(left.checked_rem_euclid(right), ty, span),
        "DIV" | "Percent" => Err(division_by_zero(operator, span)),
        "Power" if right >= 0 => checked_integer(
            left.checked_pow(u32::try_from(right).map_err(|_| {
                runtime_error(crate::diagnostic::DiagId::INVALID_EXPONENT, "integer exponent is too large", span)
            })?),
            ty,
            span,
        ),
        "Power" => Err(runtime_error(crate::diagnostic::DiagId::INVALID_EXPONENT,
            "integer exponent cannot be negative",
            span,
        )),
        "AND" => checked_integer(Some(left & right), ty, span),
        "OR" => checked_integer(Some(left | right), ty, span),
        "XOR" => checked_integer(Some(left ^ right), ty, span),
        "SHL" => shift(left, right, ty, true, span),
        "SHR" => shift(left, right, ty, false, span),
        "Less" => Ok(Value::Boolean(left < right)),
        "LessEqual" => Ok(Value::Boolean(left <= right)),
        "Greater" => Ok(Value::Boolean(left > right)),
        "GreaterEqual" => Ok(Value::Boolean(left >= right)),
        _ => Err(super::type_mismatch("integer operator", operator, "integer operation", span)),
    }
}

fn division_by_zero(operator: &str, span: Span) -> Diagnostic {
    Diagnostic::structured(
        crate::diagnostic::DiagId::DIVISION_BY_ZERO,
        vec![("operation".into(), operator.into())],
        vec![crate::diagnostic::Label {
            span,
            style: crate::diagnostic::LabelStyle::Primary,
            text: None,
        }],
    )
    .expect("division-by-zero diagnostic schema")
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)] // shared executor helpers follow this inline module.
mod diagnostic_tests {
    use super::{default_span, division_by_zero};

    #[test]
    fn division_by_zero_preserves_operator_fact() {
        let diagnostic = division_by_zero("Percent", default_span());
        let spec = diagnostic
            .structured
            .expect("division-by-zero must use the structured catalog");
        assert_eq!(spec.id.code(), "DIVISION_BY_ZERO");
        assert_eq!(spec.args[0].0, "operation");
        assert_eq!(spec.args[0].1.to_string(), "Percent");
    }

    #[test]
    fn index_error_preserves_bound_and_context_facts() {
        let diagnostic = super::super::index_out_of_bounds(4, 3, "vector", default_span());
        let spec = diagnostic
            .structured
            .expect("index error must use the structured catalog");
        assert_eq!(spec.id.code(), "INDEX_OUT_OF_BOUNDS");
        assert_eq!(spec.args[0].1.to_string(), "4");
        assert_eq!(spec.args[1].1.to_string(), "3");
        assert_eq!(spec.args[2].1.to_string(), "vector");
    }
}

fn shift(value: i128, count: i128, ty: &Type, left: bool, span: Span) -> Result<Value, Diagnostic> {
    let width = integer_width(integer_kind(ty).unwrap_or(IntegerType::Int32));
    if count < 0 || count >= i128::from(width) {
        return Err(runtime_error(crate::diagnostic::DiagId::INVALID_SHIFT_COUNT,
            format!("shift count must be in 0..{width}"),
            span,
        ));
    }
    let count = u32::try_from(count).expect("validated shift count");
    if left {
        checked_integer(value.checked_shl(count), ty, span)
    } else {
        let mask = (1_u128 << width) - 1;
        checked_integer(
            Some(((value.cast_unsigned() & mask) >> count).cast_signed()),
            ty,
            span,
        )
    }
}

fn cast(value: Value, ty: &Type, span: Span) -> Result<Value, Diagnostic> {
    match ty {
        Type::Boolean => Ok(Value::Boolean(match value {
            Value::Boolean(value) => value,
            Value::Integer(value, _) => value != 0,
            Value::Float(value, _) => value != 0.0,
            Value::String(value) => !value.is_empty(),
            Value::Null | Value::NotAvailable | Value::EndOfFile => false,
            _ => true,
        })),
        Type::Integer(_) => match value {
            Value::Integer(value, _) => checked_integer(Some(value), ty, span),
            #[allow(clippy::cast_possible_truncation)]
            // BN specifies truncation followed by range checking.
            Value::Float(value, _) if value.is_finite() => {
                checked_integer(Some(value.trunc() as i128), ty, span)
            }
            Value::Float(_, _) => Err(runtime_error(crate::diagnostic::DiagId::INVALID_NUMERIC_CONVERSION,
                "NAN and infinity cannot convert to an integer",
                span,
            )),
            _ => Err(super::type_mismatch("INTEGER", "non-integer-compatible value", "integer conversion", span)),
        },
        Type::Float(_) => Ok(float_value(number_as_float(&value, span)?, float_kind(ty))),
        Type::Named(_) | Type::ImportedNamed { .. } => match value {
            Value::Object { .. } | Value::Record { .. } | Value::Handle { .. } | Value::Null => {
                Ok(value)
            }
            _ => Err(super::type_mismatch("named value", "incompatible value", "named conversion", span)),
        },
        _ => Err(super::type_mismatch("supported conversion target", "incompatible value or type", "value conversion", span)),
    }
}

pub(super) fn coerce(value: Value, ty: &Type, span: Span) -> Result<Value, Diagnostic> {
    match (&value, ty) {
        (Value::Integer(number, _), Type::Integer(_)) => checked_integer(Some(*number), ty, span),
        (Value::Float(number, _), Type::Float(_)) => Ok(float_value(*number, float_kind(ty))),
        (_, Type::Alternative(types)) if types.iter().any(|ty| value_matches_type(&value, ty)) => {
            Ok(value)
        }
        (Value::Boolean(_), Type::Boolean)
        | (Value::String(_), Type::String)
        | (Value::Vector(_), Type::Vector { .. })
        | (Value::Function(_), Type::Function { .. })
        | (Value::Null, Type::Null)
        | (Value::NotAvailable, Type::NotAvailable)
        | (Value::EndOfFile, Type::EndOfFile)
        | (Value::HostConsole, Type::HostConsole)
        | (Value::Handle { .. }, Type::Named(_) | Type::Pointer { .. })
        | (Value::Pointer { .. }, Type::Pointer { .. })
        | (
            Value::Record { .. }
            | Value::Object { .. }
            | Value::TcpStream(_)
            | Value::TcpListener(_)
            | Value::UdpSocket(_)
            | Value::LogFields(_)
            | Value::LogEntry(_)
            | Value::LogLogger(_)
            | Value::Json(_),
            Type::Named(_) | Type::TypeName(_) | Type::ImportedNamed { .. } | Type::ImportedTypeName { .. },
        )
        | (
            Value::DispatchQueue(_)
            | Value::DispatchTicket(_)
            | Value::DispatchGroup(_)
            | Value::DispatchBarrier(_)
            | Value::DispatchSemaphore(_)
            | Value::DispatchMutex(_),
            Type::Named(_) | Type::ImportedNamed { .. },
        )
        | (
            Value::File(_),
            Type::Named(_)
            | Type::ImportedNamed { .. }
            | Type::TypeName(_)
            | Type::ImportedTypeName { .. },
        )
        | (
            Value::DataFrame(_),
            Type::Named(_)
            | Type::ImportedNamed { .. }
            | Type::TypeName(_)
            | Type::ImportedTypeName { .. },
        )
        | (
            Value::Type(_),
            Type::System
            | Type::HostClock
            | Type::HostRandom
            | Type::HostFileSystem
            | Type::HostNet
            | Type::HostExec,
        ) => Ok(value),
        (Value::Date(_), Type::Named(name)) if name == "DATE" => Ok(value),
        (Value::Time(_), Type::Named(name)) if name == "TIME" => Ok(value),
        (Value::TimeZone(_), Type::Named(name)) if name == "TIMEZONE" => Ok(value),
        (Value::Null, Type::Named(name)) if name == "VOID" => Ok(value),
        (Value::Error { .. }, Type::Named(name)) if name == "Error" => Ok(value),
        _ => Err(super::type_mismatch("IR destination type", "runtime value", "IR coercion", span)),
    }
}

fn checked_integer(value: Option<i128>, ty: &Type, span: Span) -> Result<Value, Diagnostic> {
    let value = value
        .ok_or_else(|| numeric_overflow("performing an integer operation", span))?;
    let kind = integer_kind(ty).unwrap_or(IntegerType::Int32);
    let (minimum, maximum) = integer_range(kind);
    if !(minimum..=maximum).contains(&value) {
        return Err(numeric_overflow(format!("converting {value} to {kind:?}"), span));
    }
    Ok(Value::Integer(value, kind))
}

/// Language globals (`ASC`, `CHAR`, `TOLOWER`, `TOUPPER`) and the
/// `$for_condition` intrinsic. Library functions live behind the provider
/// seam (`crate::libraries`), never here.
fn builtin(name: &str, arguments: &[Value], span: Span) -> Result<Value, Diagnostic> {
    if name == "$for_condition" {
        let current = integer(&arguments[0], span)?.0;
        let end = integer(&arguments[1], span)?.0;
        let step = integer(&arguments[2], span)?.0;
        if step == 0 {
            return Err(runtime_error(crate::diagnostic::DiagId::INVALID_FOR_STEP,
                "FOR STEP cannot be zero",
                span,
            ));
        }
        return Ok(Value::Boolean(if step > 0 {
            current <= end
        } else {
            current >= end
        }));
    }
    if name == "ASC" {
        let Value::String(text) = &arguments[0] else {
            return Err(super::type_mismatch("STRING", "non-STRING value", "ASC", span));
        };
        return Ok(text.chars().next().map_or_else(
            || Value::Error {
                code: 1,
                message: "ASC requires a non-empty STRING".into(),
            },
            |c| Value::Integer(i128::from(u32::from(c)), IntegerType::Int32),
        ));
    }
    if name == "CHAR" {
        let (code, _) = integer(&arguments[0], span)?;
        return Ok(u32::try_from(code)
            .ok()
            .and_then(char::from_u32)
            .map_or_else(
                || Value::Error {
                    code: 1,
                    message: "CHAR code is not a Unicode scalar".into(),
                },
                |c| Value::String(c.into()),
            ));
    }
    if name == "TOLOWER" {
        let Value::String(text) = &arguments[0] else {
            return Err(super::type_mismatch("STRING", "non-STRING value", "TOLOWER", span));
        };
        // Unicode case mapping (same as bn_rt_str_to_lower / Rust to_lowercase).
        return Ok(Value::String(text.to_lowercase()));
    }
    if name == "TOUPPER" {
        let Value::String(text) = &arguments[0] else {
            return Err(super::type_mismatch("STRING", "non-STRING value", "TOUPPER", span));
        };
        // Unicode case mapping (same as bn_rt_str_to_upper / Rust to_uppercase).
        return Ok(Value::String(text.to_uppercase()));
    }
    Err(super::name_not_found(name, "builtin dispatch", span))
}
