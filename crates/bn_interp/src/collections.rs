// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use bn_diag::Diagnostic;
use bn_runtime::Heap;
use bn_source::Span;

use super::{Value, integer, type_mismatch};

#[must_use]
pub fn dataframe_index_error() -> Value {
    Value::Error {
        code: 1,
        message: "DataFrame index out of bounds".into(),
    }
}

#[must_use]
pub fn unsigned_indices(values: Vec<i128>) -> Option<Vec<usize>> {
    values
        .into_iter()
        .map(|value| usize::try_from(value).ok())
        .collect()
}

/// # Errors
///
/// Returns the runtime diagnostic for a value that is not an index or index vector.
pub fn collect_indices(
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
            return Err(type_mismatch(
                "INTEGER vector",
                "non-vector value",
                "index collection",
                span,
            ));
        }
    };
    values
        .into_iter()
        .map(|value| integer(&value, span).map(|(value, _)| value))
        .collect()
}
