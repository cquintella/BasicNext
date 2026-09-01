// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::{
    ast::{Expression, ExpressionKind, Literal},
    diagnostic::Diagnostic,
};

use super::{FloatType, IntegerType, Type, display, error, is_float, is_integer};

pub(super) fn comparable(left: &Type, right: &Type) -> bool {
    super::compatible(left, right)
        || super::compatible(right, left)
        || numeric_result(left, right).is_some()
}

pub(super) fn binary_type(
    operator: &str,
    left: &Type,
    right: &Type,
    expression: &Expression,
) -> Result<Type, Diagnostic> {
    let invalid = || {
        error(
            "TYPE_MISMATCH",
            format!(
                "operator {operator} cannot combine {} and {}",
                display(left),
                display(right)
            ),
            expression.span,
        )
    };

    match operator {
        "Assign" | "NotEqual" => comparable(left, right)
            .then_some(Type::Boolean)
            .ok_or_else(invalid),
        "Less" | "LessEqual" | "Greater" | "GreaterEqual" => {
            numeric_result(left, right).map_or_else(|| Err(invalid()), |_| Ok(Type::Boolean))
        }
        "AND" | "OR" | "XOR" if left == &Type::Boolean && right == &Type::Boolean => {
            Ok(Type::Boolean)
        }
        "AND" | "OR" | "XOR" | "DIV" | "Percent" => integer_result(left, right).ok_or_else(invalid),
        "SHL" | "SHR" => {
            if !is_integer(left) || !is_integer(right) {
                return Err(invalid());
            }
            let result = default_literal_type(left.clone());
            if let Some(count) = constant_integer_from_type(right)
                && (count < 0 || count >= i128::from(integer_width(&result)))
            {
                return Err(error(
                    "INVALID_SHIFT_COUNT",
                    "shift count must be non-negative and smaller than the left operand width",
                    expression.span,
                ));
            }
            Ok(result)
        }
        "Plus" if left == &Type::String && right == &Type::String => Ok(Type::String),
        "Slash" => numeric_result(left, right)
            .map(|_| Type::Float(FloatType::Float64))
            .ok_or_else(invalid),
        "Plus" | "Minus" | "Star" | "Power" => {
            if let Some(value) = constant_integer(expression) {
                return Ok(Type::IntegerLiteral(value.to_string()));
            }
            numeric_result(left, right).ok_or_else(invalid)
        }
        _ => Err(invalid()),
    }
}

pub(super) fn conversion_allowed(source: &Type, target: &Type) -> bool {
    let numeric = |ty: &Type| {
        matches!(
            ty,
            Type::Integer(_) | Type::IntegerLiteral(_) | Type::Float(_) | Type::FloatLiteral
        )
    };
    numeric(source) && (numeric(target) || *target == Type::Boolean)
        || *source == Type::String && *target == Type::Boolean
        || matches!(source, Type::Null | Type::NotAvailable | Type::EndOfFile)
            && *target == Type::Boolean
}

pub(super) fn compound_operator(operator: &str) -> Option<&'static str> {
    match operator {
        "PlusAssign" => Some("Plus"),
        "MinusAssign" => Some("Minus"),
        "StarAssign" => Some("Star"),
        "SlashAssign" => Some("Slash"),
        "PercentAssign" => Some("Percent"),
        "PowerAssign" => Some("Power"),
        _ => None,
    }
}

pub(super) fn type_from_name(name: &str) -> Type {
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
pub(super) fn integer_type(name: &str) -> Option<IntegerType> {
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
pub(super) fn float_type(name: &str) -> Option<FloatType> {
    match name {
        "FLOAT32" => Some(FloatType::Float32),
        "FLOAT64" | "FLOAT" => Some(FloatType::Float64),
        _ => None,
    }
}

pub(super) fn integer_literal_fits(value: &str, target: IntegerType) -> bool {
    let parsed = parse_integer(value);
    let Some(value) = parsed else { return false };
    let (minimum, maximum) = integer_range(target);
    (minimum..=maximum).contains(&value)
}

pub(super) fn integer_range(target: IntegerType) -> (i128, i128) {
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

pub(super) fn parse_integer(value: &str) -> Option<i128> {
    if let Some(value) = value.strip_prefix("0b") {
        i128::from_str_radix(value, 2).ok()
    } else if let Some(value) = value.strip_prefix("0x") {
        i128::from_str_radix(value, 16).ok()
    } else {
        value.parse().ok()
    }
}

pub(super) fn constant_integer_from_type(ty: &Type) -> Option<i128> {
    let Type::IntegerLiteral(value) = ty else {
        return None;
    };
    parse_integer(value)
}

pub(super) fn constant_integer(expression: &Expression) -> Option<i128> {
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

pub(super) fn default_literal_type(ty: Type) -> Type {
    match ty {
        Type::IntegerLiteral(_) => Type::Integer(IntegerType::Int32),
        Type::FloatLiteral => Type::Float(FloatType::Float64),
        other => other,
    }
}

pub(super) fn numeric_result(left: &Type, right: &Type) -> Option<Type> {
    match (left, right) {
        (Type::IntegerLiteral(_), Type::Float(kind))
        | (Type::Float(kind), Type::IntegerLiteral(_)) => Some(Type::Float(*kind)),
        (Type::IntegerLiteral(_) | Type::FloatLiteral, Type::FloatLiteral)
        | (Type::FloatLiteral, Type::IntegerLiteral(_)) => Some(Type::Float(FloatType::Float64)),
        (Type::Float(left), Type::Float(right)) => Some(Type::Float(
            if left == &FloatType::Float64 || right == &FloatType::Float64 {
                FloatType::Float64
            } else {
                FloatType::Float32
            },
        )),
        (Type::Float(kind), Type::FloatLiteral) | (Type::FloatLiteral, Type::Float(kind)) => {
            Some(Type::Float(*kind))
        }
        _ => integer_result(left, right),
    }
}

pub(super) fn integer_result(left: &Type, right: &Type) -> Option<Type> {
    match (left, right) {
        (Type::IntegerLiteral(left), Type::IntegerLiteral(right)) => {
            let left = parse_integer(left)?;
            let right = parse_integer(right)?;
            integer_kind_for_range(left.min(right), left.max(right)).map(Type::Integer)
        }
        (Type::Integer(kind), Type::IntegerLiteral(value))
        | (Type::IntegerLiteral(value), Type::Integer(kind)) => {
            integer_literal_fits(value, *kind).then_some(Type::Integer(*kind))
        }
        (Type::Integer(left), Type::Integer(right)) => {
            promote_integers(*left, *right).map(Type::Integer)
        }
        _ => None,
    }
}

fn promote_integers(left: IntegerType, right: IntegerType) -> Option<IntegerType> {
    if left == right {
        return Some(left);
    }
    if left == IntegerType::Byte {
        return Some(if is_unsigned(right) {
            right
        } else {
            widest_signed(IntegerType::Int16, right)
        });
    }
    if right == IntegerType::Byte {
        return Some(if is_unsigned(left) {
            left
        } else {
            widest_signed(left, IntegerType::Int16)
        });
    }
    match (is_unsigned(left), is_unsigned(right)) {
        (true, true) => Some(
            if integer_width(&Type::Integer(left)) >= integer_width(&Type::Integer(right)) {
                left
            } else {
                right
            },
        ),
        (false, false) => Some(widest_signed(left, right)),
        _ => None,
    }
}

fn is_unsigned(kind: IntegerType) -> bool {
    matches!(
        kind,
        IntegerType::Byte | IntegerType::UInt16 | IntegerType::UInt32 | IntegerType::UInt64
    )
}

fn widest_signed(left: IntegerType, right: IntegerType) -> IntegerType {
    if integer_width(&Type::Integer(left)) >= integer_width(&Type::Integer(right)) {
        left
    } else {
        right
    }
}

fn integer_width(ty: &Type) -> u8 {
    match ty {
        Type::Integer(IntegerType::Byte | IntegerType::Int8) => 8,
        Type::Integer(IntegerType::Int16 | IntegerType::UInt16) => 16,
        Type::Integer(IntegerType::Int32 | IntegerType::UInt32) | Type::IntegerLiteral(_) => 32,
        Type::Integer(IntegerType::Int64 | IntegerType::UInt64) => 64,
        _ => 0,
    }
}

fn integer_kind_for_range(minimum: i128, maximum: i128) -> Option<IntegerType> {
    [IntegerType::Int32, IntegerType::Int64, IntegerType::UInt64]
        .into_iter()
        .find(|kind| {
            let (low, high) = integer_range(*kind);
            minimum >= low && maximum <= high
        })
}

pub(super) fn is_numeric(ty: &Type) -> bool {
    is_integer(ty) || is_float(ty)
}

#[must_use]
pub(super) fn static_len(ty: &Type) -> Option<u64> {
    if is_numeric(ty) {
        return Some(1);
    }
    if let Type::Vector { dimensions, .. } = ty {
        return dimension_product(dimensions);
    }
    None
}

#[must_use]
pub(super) fn static_size_of(ty: &Type) -> Option<u64> {
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
pub(super) fn integer_byte_size(kind: IntegerType) -> u64 {
    match kind {
        IntegerType::Byte | IntegerType::Int8 => 1,
        IntegerType::Int16 | IntegerType::UInt16 => 2,
        IntegerType::Int32 | IntegerType::UInt32 => 4,
        IntegerType::Int64 | IntegerType::UInt64 => 8,
    }
}

pub(super) fn dimension_product(dimensions: &[u64]) -> Option<u64> {
    if dimensions.contains(&u64::MAX) {
        return None;
    }
    dimensions
        .iter()
        .try_fold(1u64, |product, dimension| product.checked_mul(*dimension))
}
