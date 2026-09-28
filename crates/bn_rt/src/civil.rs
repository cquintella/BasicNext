// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Civil date and time arithmetic of `DATE`, `TIME`, and `TIMESTAMP`, the
//! one implementation both backends use: `bn_interp::temporal` maps its
//! failures to diagnostics, and the C ABI ends the program with the call
//! site's diagnostic (bucket 0.6.2b R6).

/// Milliseconds in a day.
pub const DAY_MS: i128 = 86_400_000;
/// The civil range of every temporal value: years 0001 through 9999.
pub const MIN_YEAR: i32 = 1;
pub const MAX_YEAR: i32 = 9999;

/// A proleptic Gregorian date.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CivilDate {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

/// `YYYY-MM-DD` of a day count since 1970-01-01.
#[must_use]
pub fn format_date(days: i32) -> String {
    let CivilDate { year, month, day } = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// `HH:MM:SS.mmm` of milliseconds since midnight.
#[must_use]
pub fn format_time(millis: u32) -> String {
    let (hour, minute, second, millis) = civil_time(millis);
    format!("{hour:02}:{minute:02}:{second:02}.{millis:03}")
}

/// Canonical RFC 3339 text of a `TIMESTAMP` (milliseconds since the Unix
/// epoch, UTC), as `Timestamp.Format` prints it: `2026-09-28T11:05:03.042Z`;
/// `None` outside the civil range.
#[must_use]
pub fn rfc3339(timestamp: i64) -> Option<String> {
    let (days, millis) = split(timestamp)?;
    Some(format!("{}T{}Z", format_date(days), format_time(millis)))
}

/// [`rfc3339`] for a timestamp known to be in range (the wall clock); ends
/// the program outside it.
#[must_use]
pub fn format_rfc3339(timestamp: i64) -> String {
    split_at(timestamp, std::ptr::null());
    rfc3339(timestamp).unwrap_or_default()
}

/// Milliseconds since the Unix epoch of a day count and a time of day.
#[must_use]
pub fn totimestamp(days: i32, millis: i32) -> i64 {
    let millis = u32::try_from(millis).unwrap_or(0);
    (i128::from(days) * DAY_MS + i128::from(millis))
        .try_into()
        .unwrap_or(0)
}

/// Days since 1970-01-01 and milliseconds since midnight of `timestamp`;
/// `None` outside years 0001..9999.
#[must_use]
pub fn split(timestamp: i64) -> Option<(i32, u32)> {
    let timestamp = i128::from(timestamp);
    let days = i32::try_from(timestamp.div_euclid(DAY_MS)).ok()?;
    let millis = u32::try_from(timestamp.rem_euclid(DAY_MS)).ok()?;
    in_range(days).then_some((days, millis))
}

/// Whether a day count falls in years 0001..9999.
#[must_use]
pub fn in_range(days: i32) -> bool {
    (MIN_YEAR..=MAX_YEAR).contains(&civil_from_days(days).year)
}

/// [`split`], ending the program outside the civil range with the calling
/// site's diagnostic (`trap`, rendered by `bnc`).
pub(crate) fn split_at(timestamp: i64, trap: *const std::ffi::c_char) -> (i32, u32) {
    split(timestamp).unwrap_or_else(|| {
        super::math::fail_at(
            trap,
            "FORMAT_OUT_OF_RANGE",
            "civil time must be in years 0001 through 9999",
        )
    })
}

/// The date of a day count since 1970-01-01.
#[must_use]
pub fn civil_from_days(days: i32) -> CivilDate {
    let z = i64::from(days) + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = u32::try_from(z - era * 146_097).unwrap_or(0);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = i64::from(yoe) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let month_prime = (5 * doy + 2) / 153;
    let day = doy - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    CivilDate {
        year: i32::try_from(year + i64::from(month <= 2)).unwrap_or(i32::MAX),
        month,
        day,
    }
}

/// The day count since 1970-01-01 of a date in years 0001..9999; `None` for
/// a date that does not exist.
#[must_use]
pub fn days_from_civil(year: i32, month: u32, day: u32) -> Option<i32> {
    if !(MIN_YEAR..=MAX_YEAR).contains(&year) || !(1..=12).contains(&month) || day == 0 {
        return None;
    }
    if day > days_in_month(year, month)? {
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

fn days_in_month(year: i32, month: u32) -> Option<u32> {
    Some(match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => return None,
    })
}

/// Hour, minute, second, and millisecond of milliseconds since midnight.
#[must_use]
pub const fn civil_time(millis: u32) -> (u32, u32, u32, u32) {
    (
        millis / 3_600_000,
        millis % 3_600_000 / 60_000,
        millis % 60_000 / 1_000,
        millis % 1_000,
    )
}

#[cfg(test)]
mod tests {
    use super::{CivilDate, civil_from_days, days_from_civil, format_rfc3339, rfc3339, split};

    #[test]
    fn rfc3339_is_canonical_utc_with_milliseconds() {
        assert_eq!(format_rfc3339(0), "1970-01-01T00:00:00.000Z");
        // 2026-09-28T11:05:03.042Z
        assert_eq!(
            format_rfc3339(1_790_593_503_042),
            "2026-09-28T11:05:03.042Z"
        );
        assert_eq!(format_rfc3339(-1), "1969-12-31T23:59:59.999Z");
    }

    #[test]
    fn dates_round_trip_and_the_range_is_0001_to_9999() {
        for (year, month, day) in [(1, 1, 1), (1970, 1, 1), (2024, 2, 29), (9999, 12, 31)] {
            let days = days_from_civil(year, month, day).expect("a real date");
            assert_eq!(civil_from_days(days), CivilDate { year, month, day });
        }
        assert_eq!(days_from_civil(2023, 2, 29), None);
        assert_eq!(days_from_civil(10_000, 1, 1), None);
        assert_eq!(split(400_000_000_000_000), None);
        assert_eq!(rfc3339(400_000_000_000_000), None);
    }
}
