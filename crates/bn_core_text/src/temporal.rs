// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Parsing functions for Basic Next temporal literals and types (R8):
//! DATE (`YYYY-MM-DD`), TIME (`HH:MM:SS.mmm`), and RFC 3339 TIMESTAMP.

use super::civil::{self, DAY_MS, days_from_civil};

/// Parses a date string `YYYY-MM-DD` into `(year, month, day)`.
#[must_use]
pub fn parse_ymd(text: &str) -> Option<(i32, u32, u32)> {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year = parse_digits(&text[0..4])?;
    let month = u32::try_from(parse_digits(&text[5..7])?).ok()?;
    let day = u32::try_from(parse_digits(&text[8..10])?).ok()?;
    Some((i32::try_from(year).ok()?, month, day))
}

/// Parses a date string `YYYY-MM-DD` returning days since 1970-01-01.
#[must_use]
pub fn parse_date(text: &str) -> Option<i32> {
    let (year, month, day) = parse_ymd(text)?;
    days_from_civil(year, month, day)
}

/// Parses a time string `HH:MM:SS` or `HH:MM:SS.mmm` into milliseconds since midnight.
/// If `require_millis` is true, `.mmm` must be present.
#[must_use]
pub fn parse_hms(text: &str, require_millis: bool) -> Option<u32> {
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

/// Parses an RFC 3339 timestamp string into epoch milliseconds.
#[must_use]
pub fn parse_rfc3339(text: &str) -> Option<i64> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_date_works() {
        assert_eq!(parse_date("1970-01-01"), Some(0));
        assert_eq!(parse_date("2024-02-29"), Some(19782));
        assert_eq!(parse_date("2023-02-29"), None);
        assert_eq!(parse_date("invalid"), None);
    }

    #[test]
    fn parse_hms_works() {
        assert_eq!(parse_hms("00:00:00.000", true), Some(0));
        assert_eq!(parse_hms("01:02:03.004", true), Some(3_723_004));
        assert_eq!(parse_hms("01:02:03", false), Some(3_723_000));
        assert_eq!(parse_hms("01:02:03", true), None);
    }

    #[test]
    fn parse_rfc3339_works() {
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(parse_rfc3339("1970-01-01T01:00:00.000+01:00"), Some(0));
    }
}
