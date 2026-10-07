// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use bn_diag::Diagnostic;
use bn_source::Span;

use bn_rt::civil::{self, DAY_MS, days_from_civil};

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
    parse_rfc3339_text(text).ok_or_else(|| {
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

fn parse_ymd(text: &str) -> Option<(i32, u32, u32)> {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year = parse_digits(&text[0..4])?;
    let month = u32::try_from(parse_digits(&text[5..7])?).ok()?;
    let day = u32::try_from(parse_digits(&text[8..10])?).ok()?;
    Some((i32::try_from(year).ok()?, month, day))
}

fn parse_hms(text: &str, require_millis: bool) -> Option<u32> {
    let bytes = text.as_bytes();
    if bytes.len() < 8 || bytes[2] != b':' || bytes[5] != b':' {
        return None;
    }
    let hour = u32::try_from(parse_digits(&text[0..2])?).ok()?;
    let minute = u32::try_from(parse_digits(&text[3..5])?).ok()?;
    let second = u32::try_from(parse_digits(&text[6..8])?).ok()?;
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let millis = if bytes.len() == 8 && !require_millis {
        0
    } else if bytes.get(8) == Some(&b'.') {
        let fraction = bytes.get(9..)?;
        if fraction.is_empty()
            || fraction.iter().any(|byte| !byte.is_ascii_digit())
            || (require_millis && fraction.len() != 3)
            || fraction
                .get(3..)
                .is_some_and(|rest| rest.iter().any(|byte| *byte != b'0'))
        {
            return None;
        }
        fraction
            .iter()
            .take(3)
            .chain(std::iter::repeat_n(
                &b'0',
                3_usize.saturating_sub(fraction.len()),
            ))
            .try_fold(0_u32, |value, byte| {
                value.checked_mul(10)?.checked_add(u32::from(*byte - b'0'))
            })?
    } else {
        return None;
    };
    Some(hour * 3_600_000 + minute * 60_000 + second * 1_000 + millis)
}

fn parse_rfc3339_text(text: &str) -> Option<i64> {
    let separator = text.find('T')?;
    let (date, rest) = text.split_at(separator);
    let rest = rest.get(1..)?;
    let (time, offset) = split_offset(rest)?;
    let days = {
        let (year, month, day) = parse_ymd(date)?;
        days_from_civil(year, month, day)?
    };
    let millis = parse_hms(time, false)?;
    let utc = i128::from(days) * DAY_MS + i128::from(millis) - i128::from(offset);
    let timestamp = i64::try_from(utc).ok()?;
    let utc_days = i32::try_from(i128::from(timestamp).div_euclid(DAY_MS)).ok()?;
    if civil::in_range(utc_days) {
        Some(timestamp)
    } else {
        None
    }
}

fn split_offset(text: &str) -> Option<(&str, i32)> {
    if let Some(time) = text.strip_suffix('Z') {
        return Some((time, 0));
    }
    let sign_index = text.rfind(['+', '-'])?;
    if sign_index < 8 {
        return None;
    }
    let (time, offset) = text.split_at(sign_index);
    let sign = if offset.starts_with('+') { 1 } else { -1 };
    let offset = &offset[1..];
    if offset.len() != 5 || offset.as_bytes()[2] != b':' {
        return None;
    }
    let hours = i32::try_from(parse_digits(&offset[0..2])?).ok()?;
    let minutes = i32::try_from(parse_digits(&offset[3..5])?).ok()?;
    if hours > 23 || minutes > 59 {
        return None;
    }
    Some((time, sign * (hours * 3_600_000 + minutes * 60_000)))
}

fn parse_digits(text: &str) -> Option<i64> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
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
