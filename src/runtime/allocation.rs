// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::{
    diagnostic::Diagnostic,
    semantic::{FloatType, Type, integer_byte_size},
    source::Span,
};

use super::{Value, integer_overflow, runtime_error};

pub(super) fn pointer_element_default(element: &Type, span: Span) -> Result<Value, Diagnostic> {
    match element {
        Type::Integer(kind) => Ok(Value::Integer(0, *kind)),
        Type::Float(kind) => Ok(Value::Float(0.0, *kind)),
        _ => Err(runtime_error(
            "TYPE_MISMATCH",
            "pointer element is not a numeric type",
            span,
        )),
    }
}

pub(super) fn pointer_element_size(element: &Type) -> Option<u64> {
    match element {
        Type::Integer(kind) => Some(integer_byte_size(*kind)),
        Type::Float(FloatType::Float32) => Some(4),
        Type::Float(FloatType::Float64) => Some(8),
        _ => None,
    }
}

pub(super) fn display_element(element: &Type) -> String {
    match element {
        Type::Integer(kind) => format!("{kind:?}"),
        Type::Float(kind) => format!("{kind:?}"),
        _ => "POINTER".into(),
    }
}

pub(super) fn add_sizes(total: u64, size: &Value, span: Span) -> Result<u64, Diagnostic> {
    let Value::Integer(size, _) = size else {
        return Err(runtime_error(
            "TYPE_MISMATCH",
            "value has no byte size",
            span,
        ));
    };
    let size = u64::try_from(*size).map_err(|_| integer_overflow(span))?;
    total
        .checked_add(size)
        .ok_or_else(|| integer_overflow(span))
}
