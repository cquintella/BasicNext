// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::{diagnostic::Diagnostic, source::Span};

const DAY_MS: i128 = 86_400_000;
const MIN_YEAR: i32 = 1;
const MAX_YEAR: i32 = 9999;

pub(crate) struct CivilDate {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

pub(crate) fn default_date() -> i32 {
    0
}

pub(crate) fn default_time() -> u32 {
    0
}

pub(crate) fn parse_date(text: &str, span: Span) -> Result<i32, Diagnostic> {
    let (year, month, day) = parse_ymd(text)
        .ok_or_else(|| temporal_error("INVALID_DATE", "DATE must be YYYY-MM-DD", span))?;
    days_from_civil(year, month, day)
        .ok_or_else(|| temporal_error("INVALID_DATE", format!("{text} is not a valid DATE"), span))
}

pub(crate) fn parse_time(text: &str, span: Span) -> Result<u32, Diagnostic> {
    parse_hms(text, true)
        .ok_or_else(|| temporal_error("INVALID_TIME", "TIME must be HH:MM:SS.mmm", span))
}

pub(crate) fn parse_timezone(text: &str, span: Span) -> Result<String, Diagnostic> {
    if is_iana_identifier(text) {
        Ok(text.to_string())
    } else {
        Err(temporal_error(
            "INVALID_TIMEZONE",
            format!("'{text}' is not a canonical IANA time-zone identifier"),
            span,
        ))
    }
}

pub(crate) fn parse_rfc3339(text: &str, span: Span) -> Result<i64, Diagnostic> {
    parse_rfc3339_text(text).ok_or_else(|| {
        temporal_error(
            "PARSE_ERROR",
            format!("'{text}' is not an RFC 3339 TIMESTAMP"),
            span,
        )
    })
}

pub(crate) fn format_rfc3339(timestamp: i64, span: Span) -> Result<String, Diagnostic> {
    let (date, time) = split_timestamp(timestamp, span)?;
    let CivilDate { year, month, day } = civil_from_days(date);
    let (hour, minute, second, millis) = civil_time(time);
    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z"
    ))
}

pub(crate) fn format_date(days: i32) -> String {
    let CivilDate { year, month, day } = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

pub(crate) fn format_time(millis: u32) -> String {
    let (hour, minute, second, millis) = civil_time(millis);
    format!("{hour:02}:{minute:02}:{second:02}.{millis:03}")
}

pub(crate) fn date_from_timestamp(timestamp: i64, span: Span) -> Result<i32, Diagnostic> {
    Ok(split_timestamp(timestamp, span)?.0)
}

pub(crate) fn time_from_timestamp(timestamp: i64, span: Span) -> Result<u32, Diagnostic> {
    Ok(split_timestamp(timestamp, span)?.1)
}

pub(crate) fn timestamp_from_date_time(
    days: i32,
    millis: u32,
    span: Span,
) -> Result<i64, Diagnostic> {
    require_civil_date(days, span)?;
    if millis > 86_399_999 {
        return Err(temporal_error(
            "INVALID_TIME",
            "TIME must be in 00:00:00.000..23:59:59.999",
            span,
        ));
    }
    (i128::from(days) * DAY_MS)
        .checked_add(i128::from(millis))
        .and_then(|value| i64::try_from(value).ok())
        .ok_or_else(|| {
            temporal_error(
                "FORMAT_OUT_OF_RANGE",
                "TIMESTAMP is outside 0001-01-01..9999-12-31",
                span,
            )
        })
}

fn split_timestamp(timestamp: i64, span: Span) -> Result<(i32, u32), Diagnostic> {
    let timestamp = i128::from(timestamp);
    let days = timestamp.div_euclid(DAY_MS);
    let millis = timestamp.rem_euclid(DAY_MS);
    let days = i32::try_from(days).map_err(|_| out_of_range(span))?;
    require_civil_date(days, span)?;
    Ok((
        days,
        u32::try_from(millis).expect("millisecond remainder fits u32"),
    ))
}

fn require_civil_date(days: i32, span: Span) -> Result<(), Diagnostic> {
    let CivilDate { year, .. } = civil_from_days(days);
    if (MIN_YEAR..=MAX_YEAR).contains(&year) {
        Ok(())
    } else {
        Err(out_of_range(span))
    }
}

fn out_of_range(span: Span) -> Diagnostic {
    temporal_error(
        "FORMAT_OUT_OF_RANGE",
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
    if (MIN_YEAR..=MAX_YEAR).contains(&civil_from_days(utc_days).year) {
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

fn is_iana_identifier(text: &str) -> bool {
    if text == "UTC" {
        return true;
    }
    let mut parts = 0;
    for part in text.split('/') {
        parts += 1;
        let mut characters = part.chars();
        let Some(first) = characters.next() else {
            return false;
        };
        if !first.is_ascii_alphabetic() {
            return false;
        }
        if !characters.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '+')
        }) {
            return false;
        }
    }
    parts >= 2
}

fn days_from_civil(year: i32, month: u32, day: u32) -> Option<i32> {
    if !(MIN_YEAR..=MAX_YEAR).contains(&year) || !(1..=12).contains(&month) || day == 0 {
        return None;
    }
    let last = days_in_month(year, month)?;
    if day > last {
        return None;
    }
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 }.div_euclid(400);
    let yoe = u32::try_from(y - era * 400).ok()?;
    let month_index = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * month_index + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + i32::try_from(doe).ok()? - 719_468)
}

fn civil_from_days(days: i32) -> CivilDate {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = u32::try_from(z - era * 146_097).unwrap_or(0);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = i32::try_from(yoe).unwrap_or(0) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let month_prime = (5 * doy + 2) / 153;
    let day = doy - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    CivilDate {
        year: year + i32::from(month <= 2),
        month,
        day,
    }
}

fn days_in_month(year: i32, month: u32) -> Option<u32> {
    Some(match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => return None,
    })
}

fn is_leap(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn civil_time(millis: u32) -> (u32, u32, u32, u32) {
    let hour = millis / 3_600_000;
    let minute = millis % 3_600_000 / 60_000;
    let second = millis % 60_000 / 1_000;
    let millis = millis % 1_000;
    (hour, minute, second, millis)
}

fn temporal_error(code: &'static str, message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic {
        code,
        message: message.into(),
        span,
    }
}
