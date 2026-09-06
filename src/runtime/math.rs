// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::{diagnostic::Diagnostic, heap::Heap, source::Span, types::FloatType};

use super::{Value, integer, number_as_float, runtime_error};
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
        Value::Pointer { handle } => {
            let len = memory.len(*handle, span)?;
            owned = (0..len)
                .map(|index| memory.get(*handle, index, span).cloned())
                .collect::<Result<Vec<_>, _>>()?;
            &owned
        }
        _ => {
            return Err(runtime_error(
                "TYPE_MISMATCH",
                "BNMath reduction expects a vector",
                span,
            ));
        }
    };
    let mut numbers = values
        .iter()
        .map(|v| number_as_float(v, span))
        .collect::<Result<Vec<_>, _>>()?;
    if matches!(name, "MIN" | "MAX") {
        if numbers.is_empty() {
            return Err(runtime_error(
                "INDEX_OUT_OF_BOUNDS",
                "BNMath reduction received an empty vector",
                span,
            ));
        }
        let first = values.first().ok_or_else(|| {
            runtime_error(
                "INDEX_OUT_OF_BOUNDS",
                "BNMath reduction received an empty vector",
                span,
            )
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
                runtime_error(
                    "INDEX_OUT_OF_BOUNDS",
                    "BNMath reduction received an empty vector",
                    span,
                )
            })?;
            return Ok(Value::Integer(result, *kind));
        }
        let Value::Float(_, kind) = first else {
            return Err(runtime_error(
                "TYPE_MISMATCH",
                "BNMath reduction expects numeric values",
                span,
            ));
        };
        let length = i32::try_from(numbers.len()).map_err(|_| {
            runtime_error(
                "RESOURCE_LIMIT",
                "BNMath reduction vector is too large",
                span,
            )
        })?;
        let result = if name == "MIN" {
            bn_rt::bn_rt_math_vmin_f64(numbers.as_ptr(), length)
        } else {
            bn_rt::bn_rt_math_vmax_f64(numbers.as_ptr(), length)
        };
        return Ok(Value::Float(result, *kind));
    }
    if values
        .first()
        .is_some_and(|value| matches!(value, Value::Float(_, _)))
    {
        let kind = values
            .first()
            .and_then(|value| match value {
                Value::Float(_, kind) => Some(*kind),
                _ => None,
            })
            .expect("FLOAT reduction has a FLOAT first value");
        let length = i32::try_from(numbers.len()).map_err(|_| {
            runtime_error(
                "RESOURCE_LIMIT",
                "BNMath reduction vector is too large",
                span,
            )
        })?;
        let pointer = numbers.as_ptr();
        let result = match name {
            "MEAN" => Some((bn_rt::bn_rt_math_mean_f64(pointer, length), false)),
            "MEDIAN" => Some((bn_rt::bn_rt_math_median_f64(pointer, length), false)),
            "QUARTILE1" => Some((bn_rt::bn_rt_math_quartile1_f64(pointer, length), false)),
            "QUARTILE3" => Some((bn_rt::bn_rt_math_quartile3_f64(pointer, length), false)),
            "STDEV" => Some((bn_rt::bn_rt_math_stdev_f64(pointer, length), false)),
            "VARIANCE" => Some((bn_rt::bn_rt_math_variance_f64(pointer, length), false)),
            "RANGE" => Some((bn_rt::bn_rt_math_range_f64(pointer, length), false)),
            "MODE" => {
                let mut value = f64::NAN;
                let status = bn_rt::bn_rt_math_mode_f64(pointer, length, &raw mut value);
                Some((value, status != 0))
            }
            _ => None,
        };
        if let Some((result, not_available)) = result {
            return Ok(if not_available {
                Value::NotAvailable
            } else {
                Value::Float(result, kind)
            });
        }
    }
    if numbers.iter().any(|v| v.is_nan()) {
        return Ok(Value::Float(f64::NAN, FloatType::Float64));
    }
    if name == "MODE" && numbers.is_empty() {
        return Ok(Value::NotAvailable);
    }
    if numbers.is_empty() || (matches!(name, "STDEV" | "VARIANCE") && numbers.len() < 2) {
        return Ok(Value::Float(f64::NAN, FloatType::Float64));
    }
    if matches!(name, "QUARTILE1" | "QUARTILE3") && numbers.len() < 2 {
        return Ok(Value::Float(f64::NAN, FloatType::Float64));
    }
    numbers.sort_by(f64::total_cmp);
    let median = |xs: &[f64]| {
        if xs.len() % 2 == 1 {
            xs[xs.len() / 2]
        } else {
            (xs[xs.len() / 2 - 1] + xs[xs.len() / 2]) / 2.0
        }
    };
    let result = match name {
        "MEAN" => numbers.iter().sum::<f64>() / numbers.len() as f64,
        "MEDIAN" => median(&numbers),
        "QUARTILE1" => median(&numbers[..numbers.len() / 2]),
        "QUARTILE3" => median(&numbers[numbers.len().div_ceil(2)..]),
        "RANGE" => numbers[numbers.len() - 1] - numbers[0],
        "VARIANCE" | "STDEV" => {
            let mean = numbers.iter().sum::<f64>() / numbers.len() as f64;
            let variance = numbers.iter().map(|v| (v - mean).powi(2)).sum::<f64>()
                / (numbers.len() - 1) as f64;
            if name == "STDEV" {
                variance.sqrt()
            } else {
                variance
            }
        }
        "MODE" => {
            let mut best = None;
            let mut count = 0;
            let mut tie = false;
            for &v in &numbers {
                let c = numbers.iter().filter(|x| **x == v).count();
                if c > count {
                    best = Some(v);
                    count = c;
                    tie = false;
                } else if c == count && best != Some(v) {
                    tie = true;
                }
            }
            if tie {
                return Ok(Value::NotAvailable);
            }
            best.unwrap_or(f64::NAN)
        }
        _ => unreachable!(),
    };
    Ok(Value::Float(result, FloatType::Float64))
}
