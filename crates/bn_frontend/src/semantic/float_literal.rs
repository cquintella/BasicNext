// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A floating-point literal must fit the type it takes (0.6.md,
//! "Floating-point literals out of range"; BDFL decision, 2026-10-27,
//! bucket 0.6.11 S5.3): a literal that rounds to infinity in `FLOAT64`,
//! or in a `FLOAT32` destination, is a `TYPE_MISMATCH`, the same category
//! and message form as an integer literal out of range (S5.1).

#![allow(clippy::wildcard_imports)]
use super::*;
use bn_types::literals::{NumericClass, numeric_alternative};

/// The value of a floating-point literal, or of a minus sign applied to one,
/// with its source text.
pub(crate) fn float_literal_leaf(expression: &Expression) -> Option<(f64, String)> {
    match &expression.kind {
        ExpressionKind::Literal(Literal::Float(text)) => Some((text.parse().ok()?, text.clone())),
        ExpressionKind::Unary { operator, operand } if operator == "Minus" => {
            let (value, text) = float_literal_leaf(operand)?;
            matches!(operand.kind, ExpressionKind::Literal(_)).then(|| (-value, format!("-{text}")))
        }
        _ => None,
    }
}

/// The `FLOAT32` that a literal stored in `expected` takes: the type itself,
/// or the floating-point alternative a literal takes (0.6.md, "Numeric
/// literals in alternative types").
pub(crate) fn float32_destination(expected: &Type) -> bool {
    match expected {
        Type::Float(FloatType::Float32) => true,
        Type::Alternative(alternatives) => {
            numeric_alternative(alternatives, NumericClass::Float)
                == Some(&Type::Float(FloatType::Float32))
        }
        _ => false,
    }
}

/// Whether `value` rounds to an infinity in `kind` (round to nearest, as
/// every conversion of the language does).
pub(crate) fn rounds_to_infinity(value: f64, kind: FloatType) -> bool {
    match kind {
        FloatType::Float64 => value.is_infinite(),
        #[allow(clippy::cast_possible_truncation)]
        FloatType::Float32 => (value as f32).is_infinite(),
    }
}

/// `TYPE_MISMATCH` for the floating-point literal `text` outside `kind`.
pub(crate) fn float_out_of_range(
    kind: FloatType,
    text: &str,
    context: impl Into<String>,
    span: Span,
) -> Diagnostic {
    let (name, largest) = match kind {
        FloatType::Float32 => ("FLOAT32", f64::from(f32::MAX)),
        FloatType::Float64 => ("FLOAT64", f64::MAX),
    };
    let range = bn_catalog::fill(
        "frontend",
        "msg.numeric_type_range",
        &[
            ("type", &name),
            ("minimum", &format!("{:e}", -largest)),
            ("maximum", &format!("{largest:e}")),
        ],
    );
    type_mismatch(range, text, context, span)
}

/// `TYPE_MISMATCH` when `expression` is a floating-point literal that rounds
/// to an infinity in `kind`.
pub(crate) fn check_float_literal(
    expression: &Expression,
    kind: FloatType,
    context: &str,
) -> Result<(), Diagnostic> {
    match float_literal_leaf(expression) {
        Some((value, text)) if rounds_to_infinity(value, kind) => {
            Err(float_out_of_range(kind, &text, context, expression.span))
        }
        _ => Ok(()),
    }
}
