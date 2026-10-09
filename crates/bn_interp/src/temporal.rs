// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use bn_diag::Diagnostic;
use bn_source::Span;

use bn_core_text::civil::{self, DAY_MS, days_from_civil};
use bn_core_text::temporal::{parse_hms, parse_rfc3339 as parse_rfc3339_core, parse_ymd};

#[must_use]
pub fn default_date() -> i32 {
    0
}

#[must_use]
pub fn default_time() -> u32 {
    0
}

/// # Errors
///
/// Returns the language diagnostic for a malformed value.
pub fn parse_date(text: &str, span: Span) -> Result<i32, Diagnostic> {
    let (year, month, day) = parse_ymd(text).ok_or_else(|| {
        temporal_error(
            bn_diag::DiagId::INVALID_DATE,
            "DATE must be YYYY-MM-DD",
            span,
        )
    })?;
    days_from_civil(year, month, day).ok_or_else(|| {
        temporal_error(
            bn_diag::DiagId::INVALID_DATE,
            format!("{text} is not a valid DATE"),
            span,
        )
    })
}

/// # Errors
///
/// Returns the language diagnostic for a malformed value.
pub fn parse_time(text: &str, span: Span) -> Result<u32, Diagnostic> {
    parse_hms(text, true).ok_or_else(|| {
        temporal_error(
            bn_diag::DiagId::INVALID_TIME,
            "TIME must be HH:MM:SS.mmm",
            span,
        )
    })
}

/// # Errors
///
/// Returns the language diagnostic for a malformed value.
pub fn parse_timezone(text: &str, span: Span) -> Result<String, Diagnostic> {
    if bn_types::text::is_timezone_id(text) {
        Ok(text.to_string())
    } else {
        Err(temporal_error(
            bn_diag::DiagId::INVALID_TIMEZONE,
            format!("'{text}' is not a canonical IANA time-zone identifier"),
            span,
        ))
    }
}

/// # Errors
///
/// Returns the language diagnostic for a malformed value.
pub fn parse_rfc3339(text: &str, span: Span) -> Result<i64, Diagnostic> {
    parse_rfc3339_core(text).ok_or_else(|| {
        temporal_error(
            bn_diag::DiagId::PARSE_ERROR,
            format!("'{text}' is not an RFC 3339 TIMESTAMP"),
            span,
        )
    })
}

/// # Errors
///
/// Returns `FORMAT_OUT_OF_RANGE` outside 0001-01-01..9999-12-31.
pub fn format_rfc3339(timestamp: i64, span: Span) -> Result<String, Diagnostic> {
    civil::rfc3339(timestamp).ok_or_else(|| out_of_range(span))
}

#[must_use]
pub fn format_date(days: i32) -> String {
    civil::format_date(days)
}

#[must_use]
pub fn format_time(millis: u32) -> String {
    civil::format_time(millis)
}

/// # Errors
///
/// Returns `FORMAT_OUT_OF_RANGE` outside the representable range.
pub fn date_from_timestamp(timestamp: i64, span: Span) -> Result<i32, Diagnostic> {
    Ok(split_timestamp(timestamp, span)?.0)
}

/// # Errors
///
/// Returns `FORMAT_OUT_OF_RANGE` outside the representable range.
pub fn time_from_timestamp(timestamp: i64, span: Span) -> Result<u32, Diagnostic> {
    Ok(split_timestamp(timestamp, span)?.1)
}

/// # Errors
///
/// Returns `FORMAT_OUT_OF_RANGE` outside the representable range.
pub fn timestamp_from_date_time(days: i32, millis: u32, span: Span) -> Result<i64, Diagnostic> {
    require_civil_date(days, span)?;
    if millis > 86_399_999 {
        return Err(temporal_error(
            bn_diag::DiagId::INVALID_TIME,
            "TIME must be in 00:00:00.000..23:59:59.999",
            span,
        ));
    }
    (i128::from(days) * DAY_MS)
        .checked_add(i128::from(millis))
        .and_then(|value| i64::try_from(value).ok())
        .ok_or_else(|| {
            temporal_error(
                bn_diag::DiagId::FORMAT_OUT_OF_RANGE,
                "TIMESTAMP is outside 0001-01-01..9999-12-31",
                span,
            )
        })
}

fn split_timestamp(timestamp: i64, span: Span) -> Result<(i32, u32), Diagnostic> {
    civil::split(timestamp).ok_or_else(|| out_of_range(span))
}

fn require_civil_date(days: i32, span: Span) -> Result<(), Diagnostic> {
    if civil::in_range(days) {
        Ok(())
    } else {
        Err(out_of_range(span))
    }
}

fn out_of_range(span: Span) -> Diagnostic {
    temporal_error(
        bn_diag::DiagId::FORMAT_OUT_OF_RANGE,
        "civil time must be in years 0001 through 9999",
        span,
    )
}

fn temporal_error(id: bn_diag::DiagId, message: impl Into<String>, span: Span) -> Diagnostic {
    let message = message.into();
    let arguments = match id.argument_schema() {
        [only] => vec![(only.name.into(), bn_diag::DiagnosticValue::Text(message))],
        schema => unreachable!(
            "{} needs an explicit argument mapping ({} arguments)",
            id.code(),
            schema.len()
        ),
    };
    Diagnostic::structured(
        id,
        arguments,
        vec![bn_diag::Label {
            span,
            style: bn_diag::LabelStyle::Primary,
            text: None,
        }],
    )
    .expect("runtime compatibility diagnostic schema")
}
