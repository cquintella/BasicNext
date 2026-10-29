// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::borrow::Cow;

use bn_diag::Diagnostic;
use bn_runtime::Heap;
use bn_source::Span;
use bn_types::FloatType;

use bn_value::Value;

use bn_interp::{
    index_out_of_bounds_pub as index_out_of_bounds, integer_pub as integer,
    number_as_float_pub as number_as_float, runtime_error_pub as runtime_error, type_mismatch,
};
#[allow(
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::manual_midpoint,
    clippy::too_many_lines
)]
pub(super) fn reduce_vector(
    name: &str,
    value: &Value,
    span: Span,
    memory: &Heap<Value>,
) -> Result<Value, Diagnostic> {
    reduce_elements(name, &vector_elements(value, span, memory)?, span)
}

/// `BNMath.Quantile` of a numeric vector or region (math.md, R type 7).
pub(super) fn quantile(
    value: &Value,
    probability: &Value,
    span: Span,
    memory: &Heap<Value>,
) -> Result<Value, Diagnostic> {
    let numbers = vector_elements(value, span, memory)?
        .iter()
        .map(|element| number_as_float(element, span))
        .collect::<Result<Vec<_>, _>>()?;
    let probability = number_as_float(probability, span)?;
    Ok(Value::Float(
        bn_core_math::quantile_f64(&numbers, probability),
        FloatType::Float64,
    ))
}

/// `BNMath.Covariance` or `BNMath.Correlation` of two numeric vectors or
/// regions (math.md); different lengths are `VECTOR_LENGTH_MISMATCH`.
pub(super) fn association(
    name: &str,
    x: &Value,
    y: &Value,
    span: Span,
    memory: &Heap<Value>,
) -> Result<Value, Diagnostic> {
    let numbers = |value: &Value| {
        vector_elements(value, span, memory)?
            .iter()
            .map(|element| number_as_float(element, span))
            .collect::<Result<Vec<_>, _>>()
    };
    let (x, y) = (numbers(x)?, numbers(y)?);
    let result = match name {
        "Covariance" => bn_core_math::covariance(&x, &y),
        "Dot" => bn_core_math::dot(&x, &y),
        _ => bn_core_math::correlation(&x, &y),
    };
    result
        .map(|value| Value::Float(value, FloatType::Float64))
        .map_err(|bn_core_math::LengthMismatch| {
            runtime_error(
                bn_diag::DiagId::VECTOR_LENGTH_MISMATCH,
                bn_diag::trap_texts::STATISTIC_VECTORS_DIFFER,
                span,
            )
        })
}
/// `BNMath.Sum` or `BNMath.Product` (math.md, "Algebraic reductions").
pub(super) fn algebraic(
    name: &str,
    value: &Value,
    span: Span,
    memory: &Heap<Value>,
) -> Result<Value, Diagnostic> {
    let numbers = vector_elements(value, span, memory)?
        .iter()
        .map(|element| number_as_float(element, span))
        .collect::<Result<Vec<_>, _>>()?;
    let result = if name == "Sum" {
        bn_core_math::sum(&numbers)
    } else {
        bn_core_math::product(&numbers)
    };
    Ok(Value::Float(result, FloatType::Float64))
}

/// The elements of a vector, or of the region a pointer names.
fn vector_elements<'a>(
    value: &'a Value,
    span: Span,
    memory: &Heap<Value>,
) -> Result<Cow<'a, [Value]>, Diagnostic> {
    match value {
        Value::Vector(values) => Ok(Cow::Borrowed(values.as_slice())),
        Value::Pointer { handle, .. } => {
            let len = memory.len(*handle, span)?;
            (0..len)
                .map(|index| memory.get(*handle, index, span).cloned())
                .collect::<Result<Vec<_>, _>>()
                .map(Cow::Owned)
        }
        _ => Err(type_mismatch(
            "vector",
            "non-vector value",
            "BNMath reduction",
            span,
        )),
    }
}

#[allow(
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::manual_midpoint,
    clippy::too_many_lines
)]
fn reduce_elements(name: &str, values: &[Value], span: Span) -> Result<Value, Diagnostic> {
    let numbers = values
        .iter()
        .map(|v| number_as_float(v, span))
        .collect::<Result<Vec<_>, _>>()?;
    if matches!(name, "MIN" | "MAX") {
        if numbers.is_empty() {
            return Err(index_out_of_bounds(
                0,
                0,
                format!("the input of BNMath.{name}"),
                span,
            ));
        }
        let first = values.first().ok_or_else(|| {
            index_out_of_bounds(0, 0, format!("the input of BNMath.{name}"), span)
        })?;
        if let Value::Integer(_, kind) = first {
            let integers = values
                .iter()
                .map(|value| integer(value, span).map(|(value, _)| value))
                .collect::<Result<Vec<_>, _>>()?;
            let result = if name == "MIN" {
                integers.iter().copied().reduce(i128::min)
            } else {
                integers.iter().copied().reduce(i128::max)
            }
            .ok_or_else(|| {
                index_out_of_bounds(0, 0, format!("the input of BNMath.{name}"), span)
            })?;
            return Ok(Value::Integer(result, *kind));
        }
        let Value::Float(_, kind) = first else {
            return Err(type_mismatch(
                "numeric value",
                "non-numeric value",
                "BNMath reduction",
                span,
            ));
        };
        let result = if name == "MIN" {
            bn_core_math::vmin_f64(&numbers)
        } else {
            bn_core_math::vmax_f64(&numbers)
        }
        .ok_or_else(|| index_out_of_bounds(0, 0, format!("the input of BNMath.{name}"), span))?;
        return Ok(Value::Float(result, *kind));
    }
    let kind = values
        .first()
        .and_then(|value| match value {
            Value::Float(_, kind) => Some(*kind),
            _ => None,
        })
        .unwrap_or(FloatType::Float64);
    let statistic =
        bn_core_math::Statistic::from_name(name).expect("the caller passes a statistic name");
    match bn_core_math::reduce_f64(statistic, &numbers) {
        bn_core_math::Reduction::Float(result) => Ok(Value::Float(result, kind)),
        bn_core_math::Reduction::Na => Ok(Value::NotAvailable),
    }
}
