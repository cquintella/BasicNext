// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::{
    diagnostic::Diagnostic,
    semantic::{FloatType, IntegerType, Type},
    source::Span,
};

use super::{Value, runtime_error};

pub(super) fn integer(value: &Value, span: Span) -> Result<(i128, IntegerType), Diagnostic> {
    let Value::Integer(value, kind) = value else {
        return Err(runtime_error(
            "TYPE_MISMATCH",
            "expected integral value",
            span,
        ));
    };
    Ok((*value, *kind))
}

pub(super) fn boolean(value: &Value, span: Span) -> Result<bool, Diagnostic> {
    let Value::Boolean(value) = value else {
        return Err(runtime_error(
            "TYPE_MISMATCH",
            "expected BOOLEAN value",
            span,
        ));
    };
    Ok(*value)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
pub(super) fn number_as_float(value: &Value, span: Span) -> Result<f64, Diagnostic> {
    match value {
        Value::Integer(value, _) => Ok(*value as f64),
        Value::Float(value, kind) => Ok(match kind {
            FloatType::Float32 => f64::from(*value as f32),
            FloatType::Float64 => *value,
        }),
        _ => Err(runtime_error(
            "TYPE_MISMATCH",
            "expected numeric value",
            span,
        )),
    }
}

pub(super) fn parse_val(text: &str) -> f64 {
    let text = text.trim_start();
    let bytes = text.as_bytes();
    let mut end = usize::from(bytes.first().is_some_and(|b| matches!(b, b'+' | b'-')));
    let digits = end;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
    }
    if end == digits || (end == digits + 1 && bytes.get(digits) == Some(&b'.')) {
        return 0.0;
    }
    text[..end].parse().unwrap_or(0.0)
}

pub(super) fn is_float_value(value: &Value) -> bool {
    matches!(value, Value::Float(_, _))
}

pub(super) fn integer_kind(ty: &Type) -> Option<IntegerType> {
    match ty {
        Type::Integer(kind) => Some(*kind),
        _ => None,
    }
}

pub(super) fn float_kind(ty: &Type) -> FloatType {
    match ty {
        Type::Float(kind) => *kind,
        _ => FloatType::Float64,
    }
}

#[allow(clippy::cast_possible_truncation)]
pub(super) fn float_value(value: f64, kind: FloatType) -> Value {
    Value::Float(
        match kind {
            FloatType::Float32 => f64::from(value as f32),
            FloatType::Float64 => value,
        },
        kind,
    )
}

pub(super) fn integer_width(kind: IntegerType) -> u8 {
    match kind {
        IntegerType::Byte | IntegerType::Int8 => 8,
        IntegerType::Int16 | IntegerType::UInt16 => 16,
        IntegerType::Int32 | IntegerType::UInt32 => 32,
        IntegerType::Int64 | IntegerType::UInt64 => 64,
    }
}

pub(super) fn integer_range(kind: IntegerType) -> (i128, i128) {
    match kind {
        IntegerType::Byte => (0, u8::MAX.into()),
        IntegerType::Int8 => (i8::MIN.into(), i8::MAX.into()),
        IntegerType::Int16 => (i16::MIN.into(), i16::MAX.into()),
        IntegerType::Int32 => (i32::MIN.into(), i32::MAX.into()),
        IntegerType::Int64 => (i64::MIN.into(), i64::MAX.into()),
        IntegerType::UInt16 => (0, u16::MAX.into()),
        IntegerType::UInt32 => (0, u32::MAX.into()),
        IntegerType::UInt64 => (0, u64::MAX.into()),
    }
}

pub(super) fn parse_integer(value: &str) -> Option<i128> {
    if let Some(value) = value.strip_prefix("0b") {
        i128::from_str_radix(value, 2).ok()
    } else if let Some(value) = value.strip_prefix("0x") {
        i128::from_str_radix(value, 16).ok()
    } else {
        value.parse().ok()
    }
}

pub(super) fn parse_float(value: &str) -> f64 {
    match value {
        "NAN" => f64::NAN,
        "INF" => f64::INFINITY,
        "-INF" => f64::NEG_INFINITY,
        _ => value.parse().expect("validated FLOAT literal"),
    }
}

pub(super) fn exit_code(code: i128, span: Span) -> Result<u8, Diagnostic> {
    u8::try_from(code)
        .map_err(|_| runtime_error("INVALID_EXIT_CODE", "exit code must be in 0..255", span))
}

pub(super) fn ordered<T: Ord>(
    operator: &str,
    left: &T,
    right: &T,
    span: Span,
) -> Result<Value, Diagnostic> {
    match operator {
        "Less" => Ok(Value::Boolean(left < right)),
        "LessEqual" => Ok(Value::Boolean(left <= right)),
        "Greater" => Ok(Value::Boolean(left > right)),
        "GreaterEqual" => Ok(Value::Boolean(left >= right)),
        _ => Err(runtime_error(
            "TYPE_MISMATCH",
            "invalid ordered comparison",
            span,
        )),
    }
}
