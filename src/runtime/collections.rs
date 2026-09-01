// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::{
    dataframe::DataFrameColumn, diagnostic::Diagnostic, heap::Heap, semantic::FloatType,
    source::Span,
};

use super::{Value, integer, runtime_error};

pub(super) fn dataframe_index_error() -> Value {
    Value::Error {
        code: 1,
        message: "DataFrame index out of bounds".into(),
    }
}

pub(super) fn unsigned_indices(values: Vec<i128>) -> Option<Vec<usize>> {
    values
        .into_iter()
        .map(|value| usize::try_from(value).ok())
        .collect()
}

#[allow(clippy::cast_precision_loss)]
pub(super) fn dataframe_numeric_values(
    column: &DataFrameColumn,
) -> Result<Vec<Value>, &'static str> {
    let mut values = Vec::new();
    for value in &column.values {
        match value {
            Value::Integer(number, _) => {
                values.push(Value::Float(*number as f64, FloatType::Float64));
            }
            Value::Float(number, _) => values.push(Value::Float(*number, FloatType::Float64)),
            Value::NotAvailable => {}
            _ => return Err("column is not numeric"),
        }
    }
    Ok(values)
}

pub(super) fn collect_indices(
    value: &Value,
    memory: &Heap<Value>,
    span: Span,
) -> Result<Vec<i128>, Diagnostic> {
    let values = match value {
        Value::Vector(values) => values.clone(),
        Value::Pointer { handle } => (0..memory.len(*handle, span)?)
            .map(|index| memory.get(*handle, index, span).cloned())
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            return Err(runtime_error(
                "TYPE_MISMATCH",
                "indices must be an INTEGER vector",
                span,
            ));
        }
    };
    values
        .into_iter()
        .map(|value| integer(&value, span).map(|(value, _)| value))
        .collect()
}
