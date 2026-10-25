// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use bn_diag::Diagnostic;
use bn_runtime::Heap;
use bn_source::Span;
use bn_types::FloatType;

use bn_value::Value;

use bn_interp::{
    index_out_of_bounds_pub as index_out_of_bounds, integer_pub as integer,
    number_as_float_pub as number_as_float, type_mismatch,
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
    let owned;
    let values = match value {
        Value::Vector(values) => values,
        Value::Pointer { handle, .. } => {
            let len = memory.len(*handle, span)?;
            owned = (0..len)
                .map(|index| memory.get(*handle, index, span).cloned())
                .collect::<Result<Vec<_>, _>>()?;
            &owned
        }
        _ => {
            return Err(type_mismatch(
                "vector",
                "non-vector value",
                "BNMath reduction",
                span,
            ));
        }
    };
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
    match bn_core_math::reduce_f64(name, &numbers) {
        bn_core_math::Reduction::Float(result) => Ok(Value::Float(result, kind)),
        bn_core_math::Reduction::Na => Ok(Value::NotAvailable),
    }
}
