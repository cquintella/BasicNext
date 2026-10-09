// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Constant folding for native emission: unary, binary, and cast results of
// constant operands, computed at the precision of their BN type.
#![allow(
    clippy::wildcard_imports,
    clippy::match_same_arms,
    clippy::cast_possible_truncation
)]
use super::*;

/// A folded value takes the precision of the type it is bound to: a `FLOAT32`
/// constant must hold the f32 value, or folding would outrun the program
/// (`0.1 AS FLOAT32 AS FLOAT64` is `0.10000000149011612`, not `0.1`).
/// Rounding one f64 `+ - * /` result to f32 equals computing it in f32.
pub(crate) fn typed_constant(value: ConstantValue, ty: &Type) -> ConstantValue {
    match (value, ty) {
        (ConstantValue::Float(value), Type::Float(FloatType::Float32)) => {
            ConstantValue::Float(f64::from(value as f32))
        }
        (value, _) => value,
    }
}

/// Folds a unary operator at the precision of `ty`.
pub(crate) fn fold_unary(
    operator: &str,
    operand: Option<&ConstantValue>,
    ty: &Type,
) -> Option<ConstantValue> {
    fold_unary_raw(operator, operand, ty).map(|value| typed_constant(value, ty))
}

fn fold_unary_raw(
    operator: &str,
    operand: Option<&ConstantValue>,
    ty: &Type,
) -> Option<ConstantValue> {
    let operand = operand?;
    match (operator, operand) {
        ("Plus", ConstantValue::Integer(value, kind)) => {
            Some(ConstantValue::Integer(*value, *kind))
        }
        ("Minus", ConstantValue::Integer(value, _)) => Some(ConstantValue::Integer(
            checked_integer_value(value.checked_neg()?, ty)?,
            integer_kind(ty),
        )),
        ("Plus", ConstantValue::Float(value)) => Some(ConstantValue::Float(*value)),
        ("Minus", ConstantValue::Float(value)) => Some(ConstantValue::Float(-value)),
        ("NOT", ConstantValue::Boolean(value)) => Some(ConstantValue::Boolean(!value)),
        ("NOT", ConstantValue::Integer(value, _)) => Some(ConstantValue::Integer(
            checked_integer_value(!*value, ty)?,
            integer_kind(ty),
        )),
        _ => None,
    }
}

/// Folds a binary operator at the precision of `ty`.
pub(crate) fn fold_binary(
    operator: &str,
    left: Option<&ConstantValue>,
    right: Option<&ConstantValue>,
    ty: &Type,
) -> Option<ConstantValue> {
    fold_binary_raw(operator, left, right, ty).map(|value| typed_constant(value, ty))
}

#[allow(clippy::cast_sign_loss, clippy::float_cmp, clippy::too_many_lines)]
fn fold_binary_raw(
    operator: &str,
    left: Option<&ConstantValue>,
    right: Option<&ConstantValue>,
    ty: &Type,
) -> Option<ConstantValue> {
    match (left?, right?) {
        (ConstantValue::Integer(left, _), ConstantValue::Integer(right, _)) => match operator {
            "Plus" => Some(ConstantValue::Integer(
                checked_integer_value(left.checked_add(*right)?, ty)?,
                integer_kind(ty),
            )),
            "Minus" => Some(ConstantValue::Integer(
                checked_integer_value(left.checked_sub(*right)?, ty)?,
                integer_kind(ty),
            )),
            "Star" | "Multiply" => Some(ConstantValue::Integer(
                checked_integer_value(left.checked_mul(*right)?, ty)?,
                integer_kind(ty),
            )),
            "DIV" if *right != 0 => Some(ConstantValue::Integer(
                checked_integer_value(left.checked_div_euclid(*right)?, ty)?,
                integer_kind(ty),
            )),
            "Percent" if *right != 0 => Some(ConstantValue::Integer(
                checked_integer_value(left.checked_rem_euclid(*right)?, ty)?,
                integer_kind(ty),
            )),
            "Power" if *right >= 0 => Some(ConstantValue::Integer(
                checked_integer_value(left.checked_pow(u32::try_from(*right).ok()?)?, ty)?,
                integer_kind(ty),
            )),
            "SHL" if (0..i128::from(shift_width(ty))).contains(right) => {
                let count = u32::try_from(*right).ok()?;
                checked_integer_value(left.checked_shl(count)?, ty)
                    .map(|value| ConstantValue::Integer(value, integer_kind(ty)))
            }
            "SHR" if (0..i128::from(shift_width(ty))).contains(right) => {
                let count = u32::try_from(*right).ok()?;
                let mask = (1_u128 << shift_width(ty)) - 1;
                Some(ConstantValue::Integer(
                    checked_integer_value(
                        ((left.cast_unsigned() & mask) >> count).cast_signed(),
                        ty,
                    )?,
                    integer_kind(ty),
                ))
            }
            "AND" => Some(ConstantValue::Integer(
                checked_integer_value(*left & *right, ty)?,
                integer_kind(ty),
            )),
            "OR" => Some(ConstantValue::Integer(
                checked_integer_value(*left | *right, ty)?,
                integer_kind(ty),
            )),
            "XOR" => Some(ConstantValue::Integer(
                checked_integer_value(*left ^ *right, ty)?,
                integer_kind(ty),
            )),
            "Less" => Some(ConstantValue::Boolean(if is_unsigned(ty) {
                left.cast_unsigned() < right.cast_unsigned()
            } else {
                left < right
            })),
            "LessEqual" => Some(ConstantValue::Boolean(if is_unsigned(ty) {
                left.cast_unsigned() <= right.cast_unsigned()
            } else {
                left <= right
            })),
            "Greater" => Some(ConstantValue::Boolean(if is_unsigned(ty) {
                left.cast_unsigned() > right.cast_unsigned()
            } else {
                left > right
            })),
            "GreaterEqual" => Some(ConstantValue::Boolean(if is_unsigned(ty) {
                left.cast_unsigned() >= right.cast_unsigned()
            } else {
                left >= right
            })),
            "Equal" | "Assign" => Some(ConstantValue::Boolean(left == right)),
            "NotEqual" => Some(ConstantValue::Boolean(left != right)),
            _ => None,
        },
        (ConstantValue::Boolean(left), ConstantValue::Boolean(right)) => match operator {
            "AND" => Some(ConstantValue::Boolean(*left && *right)),
            "OR" => Some(ConstantValue::Boolean(*left || *right)),
            "XOR" => Some(ConstantValue::Boolean(*left ^ *right)),
            "Equal" | "Assign" => Some(ConstantValue::Boolean(left == right)),
            "NotEqual" => Some(ConstantValue::Boolean(left != right)),
            _ => None,
        },
        (ConstantValue::Float(left), ConstantValue::Float(right)) => match operator {
            "Plus" => Some(ConstantValue::Float(left + right)),
            "Minus" => Some(ConstantValue::Float(left - right)),
            "Star" | "Multiply" => Some(ConstantValue::Float(left * right)),
            "Slash" | "Divide" => Some(ConstantValue::Float(left / right)),
            "Power" => Some(ConstantValue::Float(left.powf(*right))),
            "Less" => Some(ConstantValue::Boolean(left < right)),
            "LessEqual" => Some(ConstantValue::Boolean(left <= right)),
            "Greater" => Some(ConstantValue::Boolean(left > right)),
            "GreaterEqual" => Some(ConstantValue::Boolean(left >= right)),
            "Equal" | "Assign" => Some(ConstantValue::Boolean(left == right)),
            "NotEqual" => Some(ConstantValue::Boolean(left != right)),
            _ => None,
        },
        _ => None,
    }
}

fn shift_width(ty: &Type) -> u8 {
    integer_kind(ty).width()
}

#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
pub(crate) fn fold_cast(value: Option<&ConstantValue>, ty: &Type) -> Option<ConstantValue> {
    match (value?, ty) {
        (ConstantValue::Integer(value, _), Type::Integer(_)) => Some(ConstantValue::Integer(
            checked_integer_value(*value, ty)?,
            integer_kind(ty),
        )),
        (ConstantValue::Integer(value, _), Type::Float(_)) => {
            Some(ConstantValue::Float(*value as f64))
        }
        (ConstantValue::Float(value), Type::Float(FloatType::Float32)) => {
            Some(ConstantValue::Float(f64::from(*value as f32)))
        }
        (ConstantValue::Float(value), Type::Float(FloatType::Float64)) => {
            Some(ConstantValue::Float(*value))
        }
        (ConstantValue::Float(value), Type::Integer(_)) if value.is_finite() => {
            Some(ConstantValue::Integer(
                checked_integer_value(value.trunc() as i128, ty)?,
                integer_kind(ty),
            ))
        }
        (ConstantValue::Boolean(value), Type::Boolean) => Some(ConstantValue::Boolean(*value)),
        (ConstantValue::Integer(value, _), Type::Boolean) => {
            Some(ConstantValue::Boolean(*value != 0))
        }
        (ConstantValue::Float(value), Type::Boolean) => Some(ConstantValue::Boolean(*value != 0.0)),
        (ConstantValue::String(value), Type::Boolean) => {
            Some(ConstantValue::Boolean(!value.is_empty()))
        }
        (ConstantValue::String(value), Type::String) => Some(ConstantValue::String(value.clone())),
        _ => None,
    }
}

pub(crate) fn checked_integer_value(value: i128, ty: &Type) -> Option<i128> {
    let kind = integer_kind(ty);
    let (minimum, maximum) = crate::helpers::integer_range(kind);
    (minimum..=maximum).contains(&value).then_some(value)
}
