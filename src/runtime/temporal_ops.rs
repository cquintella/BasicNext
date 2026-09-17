// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::{diagnostic::Diagnostic, source::Span, temporal, types::IntegerType};

use super::{Value, integer, require_arity, runtime_error, type_mismatch};

pub(crate) fn is_temporal_builtin(name: &str) -> bool {
    matches!(
        name,
        "Date.Parse"
            | "Time.Parse"
            | "TimeZone.Parse"
            | "Timestamp.Parse"
            | "Timestamp.Format"
    )
}

#[allow(clippy::too_many_lines)]
pub(crate) fn temporal_call(
    name: &str,
    arguments: &[Value],
    span: Span,
) -> Result<Value, Diagnostic> {
    match name {
        "Date.Parse" => {
            require_arity(name, arguments, 1, span)?;
            let Value::String(text) = &arguments[0] else {
                return Err(type_mismatch("STRING", "non-STRING value", "Date.Parse", span));
            };
            Ok(Value::Date(temporal::parse_date(text, span)?))
        }
        "Time.Parse" => {
            require_arity(name, arguments, 1, span)?;
            let Value::String(text) = &arguments[0] else {
                return Err(type_mismatch("STRING", "non-STRING value", "Time.Parse", span));
            };
            Ok(Value::Time(temporal::parse_time(text, span)?))
        }
        "TimeZone.Parse" => {
            require_arity(name, arguments, 1, span)?;
            let Value::String(text) = &arguments[0] else {
                return Err(type_mismatch("STRING", "non-STRING value", "TimeZone.Parse", span));
            };
            Ok(Value::TimeZone(temporal::parse_timezone(text, span)?))
        }
        "Timestamp.Parse" => {
            require_arity(name, arguments, 1, span)?;
            let Value::String(text) = &arguments[0] else {
                return Err(type_mismatch("STRING", "non-STRING value", "Timestamp.Parse", span));
            };
            Ok(Value::Integer(
                i128::from(temporal::parse_rfc3339(text, span)?),
                IntegerType::Int64,
            ))
        }
        "Timestamp.Format" => {
            require_arity(name, arguments, 1, span)?;
            let (timestamp, _) = integer(&arguments[0], span)?;
            let timestamp = i64::try_from(timestamp).map_err(|_| {
                runtime_error(
                    crate::diagnostic::DiagId::FORMAT_OUT_OF_RANGE,
                    "TIMESTAMP is outside 0001-01-01..9999-12-31",
                    span,
                )
            })?;
            Ok(Value::String(temporal::format_rfc3339(timestamp, span)?))
        }
        _ => Err(super::name_not_found(name, "temporal function", span)),
    }
}
