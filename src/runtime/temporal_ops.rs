// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::{diagnostic::Diagnostic, source::Span, temporal, types::IntegerType};

use super::{Value, integer, require_arity, runtime_error, type_mismatch};

pub(super) fn is_temporal_builtin(name: &str) -> bool {
    matches!(
        name,
        "Date.Parse"
            | "Time.Parse"
            | "TimeZone.Parse"
            | "Timestamp.Parse"
            | "Timestamp.Format"
            | "BNMath.TODATE"
            | "BNMath.TOTIME"
            | "BNMath.TOTIMESTAMP"
    )
}

#[allow(clippy::too_many_lines)]
pub(super) fn temporal_call(
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
        "Timestamp.Format" | "BNMath.TODATE" | "BNMath.TOTIME" => {
            require_arity(name, arguments, 1, span)?;
            let (timestamp, _) = integer(&arguments[0], span)?;
            let timestamp = i64::try_from(timestamp).map_err(|_| {
                runtime_error(
                    "FORMAT_OUT_OF_RANGE",
                    "TIMESTAMP is outside 0001-01-01..9999-12-31",
                    span,
                )
            })?;
            match name {
                "Timestamp.Format" => Ok(Value::String(temporal::format_rfc3339(timestamp, span)?)),
                "BNMath.TODATE" => Ok(Value::Date(temporal::date_from_timestamp(timestamp, span)?)),
                _ => Ok(Value::Time(temporal::time_from_timestamp(timestamp, span)?)),
            }
        }
        "BNMath.TOTIMESTAMP" => {
            require_arity(name, arguments, 2, span)?;
            let Value::Date(days) = arguments[0] else {
                return Err(type_mismatch("DATE", "non-DATE value", "BNMath.TOTIMESTAMP date", span));
            };
            let Value::Time(millis) = arguments[1] else {
                return Err(type_mismatch("TIME", "non-TIME value", "BNMath.TOTIMESTAMP time", span));
            };
            Ok(Value::Integer(
                i128::from(temporal::timestamp_from_date_time(days, millis, span)?),
                IntegerType::Int64,
            ))
        }
        _ => Err(super::name_not_found(name, "temporal function", span)),
    }
}
