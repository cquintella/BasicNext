#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

#[path = "executor/terminal.rs"]
mod terminal;
use terminal::terminal_dimensions;
#[path = "executor/helpers.rs"]
mod helpers;
#[allow(unused_imports)]
use self::helpers::{integer_from_count, integer_from_u64, lifecycle_dispatch, require_console};

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
#[path = "executor/part9.rs"]
mod part9;
#[path = "executor/part10.rs"]
mod part10;
#[path = "executor/part11.rs"]
mod part11;
#[path = "executor/part12.rs"]
mod part12;
#[path = "executor/part13.rs"]
mod part13;
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
        _ => Err(runtime_error(
            "TYPE_MISMATCH",
            "invalid unary operation",
            span,
        )),
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
            return Err(runtime_error(
                "INVALID_IR",
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
            _ => Err(runtime_error(
                "TYPE_MISMATCH",
                "invalid BOOLEAN operation",
                span,
            )),
        };
    }
    if let (Value::String(left), Value::String(right)) = (left, right) {
        return match operator {
            "Plus" => Ok(Value::String(format!("{left}{right}"))),
            _ => Err(runtime_error(
                "TYPE_MISMATCH",
                "invalid STRING operation",
                span,
            )),
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
            _ => Err(runtime_error(
                "TYPE_MISMATCH",
                "invalid floating operation",
                span,
            )),
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
        "DIV" | "Percent" => Err(runtime_error(
            "DIVISION_BY_ZERO",
            "integer divisor cannot be zero",
            span,
        )),
        "Power" if right >= 0 => checked_integer(
            left.checked_pow(u32::try_from(right).map_err(|_| {
                runtime_error("INVALID_EXPONENT", "integer exponent is too large", span)
            })?),
            ty,
            span,
        ),
        "Power" => Err(runtime_error(
            "INVALID_EXPONENT",
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
        _ => Err(runtime_error(
            "TYPE_MISMATCH",
            "invalid integer operation",
            span,
        )),
    }
}

fn shift(value: i128, count: i128, ty: &Type, left: bool, span: Span) -> Result<Value, Diagnostic> {
    let width = integer_width(integer_kind(ty).unwrap_or(IntegerType::Int32));
    if count < 0 || count >= i128::from(width) {
        return Err(runtime_error(
            "INVALID_SHIFT_COUNT",
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
            Value::Float(_, _) => Err(runtime_error(
                "INVALID_NUMERIC_CONVERSION",
                "NAN and infinity cannot convert to an integer",
                span,
            )),
            _ => Err(runtime_error(
                "TYPE_MISMATCH",
                "value cannot convert to an integer",
                span,
            )),
        },
        Type::Float(_) => Ok(float_value(number_as_float(&value, span)?, float_kind(ty))),
        Type::Named(_) | Type::ImportedNamed { .. } => match value {
            Value::Object { .. } | Value::Record { .. } | Value::Handle { .. } | Value::Null => {
                Ok(value)
            }
            _ => Err(runtime_error(
                "TYPE_MISMATCH",
                "unsupported conversion",
                span,
            )),
        },
        _ => Err(runtime_error(
            "TYPE_MISMATCH",
            "unsupported conversion",
            span,
        )),
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
            | Type::HostNet,
        ) => Ok(value),
        (Value::Date(_), Type::Named(name)) if name == "DATE" => Ok(value),
        (Value::Time(_), Type::Named(name)) if name == "TIME" => Ok(value),
        (Value::TimeZone(_), Type::Named(name)) if name == "TIMEZONE" => Ok(value),
        (Value::Null, Type::Named(name)) if name == "VOID" => Ok(value),
        (Value::Error { .. }, Type::Named(name)) if name == "Error" => Ok(value),
        _ => {
            eprintln!("DEBUG coerce value={value:?} ty={ty:?}");
            Err(runtime_error(
            "TYPE_MISMATCH",
            "runtime value does not match its IR destination type",
            span,
        ))
        }
    }
}

fn checked_integer(value: Option<i128>, ty: &Type, span: Span) -> Result<Value, Diagnostic> {
    let value = value
        .ok_or_else(|| runtime_error("NUMERIC_OVERFLOW", "integer operation overflowed", span))?;
    let kind = integer_kind(ty).unwrap_or(IntegerType::Int32);
    let (minimum, maximum) = integer_range(kind);
    if !(minimum..=maximum).contains(&value) {
        return Err(runtime_error(
            "NUMERIC_OVERFLOW",
            format!("{value} does not fit {kind:?}"),
            span,
        ));
    }
    Ok(Value::Integer(value, kind))
}

#[allow(clippy::too_many_lines)] // Standard numeric functions share argument decoding and errors.
fn builtin(
    name: &str,
    arguments: &[Value],
    span: Span,
    memory: &Heap<Value>,
) -> Result<Value, Diagnostic> {
    if name == "$for_condition" {
        let current = integer(&arguments[0], span)?.0;
        let end = integer(&arguments[1], span)?.0;
        let step = integer(&arguments[2], span)?.0;
        if step == 0 {
            return Err(runtime_error(
                "INVALID_FOR_STEP",
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
            return Err(runtime_error("TYPE_MISMATCH", "ASC expects STRING", span));
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
    let math_name = name
        .strip_prefix("BNMath.")
        .ok_or_else(|| runtime_error("NAME_NOT_FOUND", "unknown builtin", span))?;
    if matches!(math_name, "TOHOUR" | "TOWEEKDAY") {
        let milliseconds = integer(&arguments[0], span)?.0;
        let days = milliseconds.div_euclid(86_400_000);
        let result = if math_name == "TOHOUR" {
            milliseconds.div_euclid(3_600_000).rem_euclid(24)
        } else {
            // 1970-01-01 was Thursday (ISO weekday 4).
            (days + 3).rem_euclid(7) + 1
        };
        return Ok(Value::Integer(result, IntegerType::Int32));
    }
    if math_name == "VAL" {
        let Value::String(text) = &arguments[0] else {
            return Err(runtime_error(
                "TYPE_MISMATCH",
                "BNMath.VAL expects STRING",
                span,
            ));
        };
        return Ok(Value::Float(parse_val(text), FloatType::Float64));
    }
    if matches!(
        math_name,
        "MEAN" | "MEDIAN" | "QUARTILE1" | "QUARTILE3" | "MODE" | "STDEV" | "VARIANCE" | "RANGE"
    ) || (matches!(math_name, "MIN" | "MAX") && arguments.len() == 1)
    {
        return reduce_vector(math_name, &arguments[0], span, memory);
    }
    if matches!(math_name, "ABS" | "MIN" | "MAX" | "SIGN")
        && arguments
            .iter()
            .all(|argument| matches!(argument, Value::Integer(_, _)))
    {
        let integers = arguments
            .iter()
            .map(|argument| integer(argument, span).map(|(value, _)| value))
            .collect::<Result<Vec<_>, _>>()?;
        let kind = integer(&arguments[0], span)?.1;
        let result = match math_name {
            "ABS" => integers[0]
                .checked_abs()
                .ok_or_else(|| runtime_error("NUMERIC_OVERFLOW", "BNMath.ABS overflowed", span))?,
            "MIN" => integers[0].min(integers[1]),
            "MAX" => integers[0].max(integers[1]),
            "SIGN" => integers[0].signum(),
            _ => unreachable!(),
        };
        return Ok(Value::Integer(result, kind));
    }
    let numbers = arguments
        .iter()
        .map(|value| number_as_float(value, span))
        .collect::<Result<Vec<_>, _>>()?;
    let result = match math_name {
        "ABS" => numbers[0].abs(),
        "MIN" => {
            if numbers.iter().any(|value| value.is_nan()) {
                f64::NAN
            } else {
                numbers[0].min(numbers[1])
            }
        }
        "MAX" => {
            if numbers.iter().any(|value| value.is_nan()) {
                f64::NAN
            } else {
                numbers[0].max(numbers[1])
            }
        }
        "SIGN" => {
            if numbers[0] == 0.0 || numbers[0].is_nan() {
                numbers[0]
            } else {
                numbers[0].signum()
            }
        }
        "FLOOR" => numbers[0].floor(),
        "CEIL" => numbers[0].ceil(),
        "TRUNC" => numbers[0].trunc(),
        "ROUND" => {
            let scale = 10_f64.powf(numbers[1]);
            (numbers[0] * scale).round_ties_even() / scale
        }
        "EXP" => numbers[0].exp(),
        "LOG" => numbers[0].ln(),
        "LOG10" => numbers[0].log10(),
        "LOG2" => numbers[0].log2(),
        "POW" => numbers[0].powf(numbers[1]),
        "SIN" => numbers[0].sin(),
        "COS" => numbers[0].cos(),
        "TAN" => numbers[0].tan(),
        "ASIN" => numbers[0].asin(),
        "ACOS" => numbers[0].acos(),
        "ATAN" => numbers[0].atan(),
        "ATAN2" => numbers[0].atan2(numbers[1]),
        "SQRT" => numbers[0].sqrt(),
        "HYPOT" => numbers[0].hypot(numbers[1]),
        "FMA" => numbers[0].mul_add(numbers[1], numbers[2]),
        _ => {
            return Err(runtime_error(
                "NAME_NOT_FOUND",
                format!("unknown BNMath function '{math_name}'"),
                span,
            ));
        }
    };
    Ok(Value::Float(result, FloatType::Float64))
}
