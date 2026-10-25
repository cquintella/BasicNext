// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use bn_diag::Diagnostic;
use bn_source::Span;
use bn_types::IntegerType;

use crate::temporal;

use super::{Value, integer, require_arity, runtime_error, shared_string, type_mismatch};

pub(crate) fn is_temporal_builtin(name: &str) -> bool {
    matches!(
        name,
        "Date.Parse"
            | "Time.Parse"
            | "TimeZone.Parse"
            | "TimeZone.Offset"
            | "TimeZone.IsDST"
            | "Timestamp.Parse"
            | "Timestamp.Format"
            | "Timestamp.AtZone"
            | "Timestamp.FromLocal"
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
                return Err(type_mismatch(
                    "STRING",
                    "non-STRING value",
                    "Date.Parse",
                    span,
                ));
            };
            Ok(Value::Date(temporal::parse_date(text, span)?))
        }
        "Time.Parse" => {
            require_arity(name, arguments, 1, span)?;
            let Value::String(text) = &arguments[0] else {
                return Err(type_mismatch(
                    "STRING",
                    "non-STRING value",
                    "Time.Parse",
                    span,
                ));
            };
            Ok(Value::Time(temporal::parse_time(text, span)?))
        }
        "TimeZone.Parse" => {
            require_arity(name, arguments, 1, span)?;
            let Value::String(text) = &arguments[0] else {
                return Err(type_mismatch(
                    "STRING",
                    "non-STRING value",
                    "TimeZone.Parse",
                    span,
                ));
            };
            Ok(Value::TimeZone(shared_string(temporal::parse_timezone(
                text, span,
            )?)))
        }
        "Timestamp.Parse" => {
            require_arity(name, arguments, 1, span)?;
            let Value::String(text) = &arguments[0] else {
                return Err(type_mismatch(
                    "STRING",
                    "non-STRING value",
                    "Timestamp.Parse",
                    span,
                ));
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
                    bn_diag::DiagId::FORMAT_OUT_OF_RANGE,
                    "TIMESTAMP is outside 0001-01-01..9999-12-31",
                    span,
                )
            })?;
            Ok(Value::String(shared_string(temporal::format_rfc3339(
                timestamp, span,
            )?)))
        }
        "TimeZone.Offset" => {
            require_arity(name, arguments, 2, span)?;
            let Value::TimeZone(tz) = &arguments[0] else {
                return Err(type_mismatch(
                    "TIMEZONE",
                    bn_catalog::text("temporal", "msg.non_timezone_value"),
                    "TimeZone.Offset",
                    span,
                ));
            };
            let (ts, _) = integer(&arguments[1], span)?;
            let ts = i64::try_from(ts).map_err(|_| {
                runtime_error(
                    bn_diag::DiagId::FORMAT_OUT_OF_RANGE,
                    bn_catalog::text("temporal", "msg.timestamp_out_of_range"),
                    span,
                )
            })?;
            let offset = temporal::timezone_offset(tz, ts, span)?;
            Ok(Value::Integer(i128::from(offset), IntegerType::Int32))
        }
        "TimeZone.IsDST" => {
            require_arity(name, arguments, 2, span)?;
            let Value::TimeZone(tz) = &arguments[0] else {
                return Err(type_mismatch(
                    "TIMEZONE",
                    bn_catalog::text("temporal", "msg.non_timezone_value"),
                    "TimeZone.IsDST",
                    span,
                ));
            };
            let (ts, _) = integer(&arguments[1], span)?;
            let ts = i64::try_from(ts).map_err(|_| {
                runtime_error(
                    bn_diag::DiagId::FORMAT_OUT_OF_RANGE,
                    bn_catalog::text("temporal", "msg.timestamp_out_of_range"),
                    span,
                )
            })?;
            let is_dst = temporal::timezone_is_dst(tz, ts, span)?;
            Ok(Value::Boolean(is_dst))
        }
        "Timestamp.AtZone" => {
            require_arity(name, arguments, 2, span)?;
            let (ts, _) = integer(&arguments[0], span)?;
            let ts = i64::try_from(ts).map_err(|_| {
                runtime_error(
                    bn_diag::DiagId::FORMAT_OUT_OF_RANGE,
                    bn_catalog::text("temporal", "msg.timestamp_out_of_range"),
                    span,
                )
            })?;
            let Value::TimeZone(tz) = &arguments[1] else {
                return Err(type_mismatch(
                    "TIMEZONE",
                    bn_catalog::text("temporal", "msg.non_timezone_value"),
                    "Timestamp.AtZone",
                    span,
                ));
            };
            let formatted = temporal::format_at_zone(ts, tz, span)?;
            Ok(Value::String(shared_string(formatted)))
        }
        "Timestamp.FromLocal" => {
            require_arity(name, arguments, 3, span)?;
            let Value::Date(days) = &arguments[0] else {
                return Err(type_mismatch(
                    "DATE",
                    bn_catalog::text("temporal", "msg.non_date_value"),
                    "Timestamp.FromLocal",
                    span,
                ));
            };
            let Value::Time(millis) = &arguments[1] else {
                return Err(type_mismatch(
                    "TIME",
                    bn_catalog::text("temporal", "msg.non_time_value"),
                    "Timestamp.FromLocal",
                    span,
                ));
            };
            let Value::TimeZone(tz) = &arguments[2] else {
                return Err(type_mismatch(
                    "TIMEZONE",
                    bn_catalog::text("temporal", "msg.non_timezone_value"),
                    "Timestamp.FromLocal",
                    span,
                ));
            };
            let millis = i32::try_from(*millis).unwrap_or(0);
            let ts = temporal::timestamp_from_local(*days, millis, tz, span)?;
            Ok(Value::Integer(i128::from(ts), IntegerType::Int64))
        }
        _ => Err(super::name_not_found(name, "temporal function", span)),
    }
}
