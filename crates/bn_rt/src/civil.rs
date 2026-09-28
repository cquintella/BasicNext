// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Civil date/time formatting matching `src/temporal.rs`.

const DAY_MS: i128 = 86_400_000;
const MIN_YEAR: i32 = 1;
const MAX_YEAR: i32 = 9999;

struct CivilDate {
    year: i32,
    month: u32,
    day: u32,
}

pub fn format_date(days: i32) -> String {
    let CivilDate { year, month, day } = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

pub fn format_time(millis: i32) -> String {
    let millis = u32::try_from(millis).unwrap_or(0);
    let hour = millis / 3_600_000;
    let minute = millis % 3_600_000 / 60_000;
    let second = millis % 60_000 / 1_000;
    let millis = millis % 1_000;
    format!("{hour:02}:{minute:02}:{second:02}.{millis:03}")
}

/// Canonical RFC 3339 text of a `TIMESTAMP` (milliseconds since the Unix
/// epoch, UTC), as `Timestamp.Format` prints it: `2026-09-28T11:05:03.042Z`.
/// Traps `FORMAT_OUT_OF_RANGE` outside years 0001..9999.
#[must_use]
pub fn format_rfc3339(timestamp: i64) -> String {
    let (days, millis) = split(timestamp);
    format!(
        "{}T{}Z",
        format_date(days),
        format_time(i32::try_from(millis).unwrap_or(0))
    )
}

pub fn totimestamp(days: i32, millis: i32) -> i64 {
    let millis = u32::try_from(millis).unwrap_or(0);
    (i128::from(days) * DAY_MS + i128::from(millis))
        .try_into()
        .unwrap_or(0)
}

fn split(timestamp: i64) -> (i32, u32) {
    split_at(timestamp, std::ptr::null())
}

/// Days and milliseconds of `timestamp`; outside the civil range the program
/// ends with the calling site's diagnostic (`trap`, rendered by `bnc`).
pub(crate) fn split_at(timestamp: i64, trap: *const std::ffi::c_char) -> (i32, u32) {
    let timestamp = i128::from(timestamp);
    let days = i32::try_from(timestamp.div_euclid(DAY_MS)).unwrap_or(0);
    let millis = u32::try_from(timestamp.rem_euclid(DAY_MS)).unwrap_or(0);
    let year = civil_from_days(days).year;
    if !(MIN_YEAR..=MAX_YEAR).contains(&year) {
        super::math::fail_at(
            trap,
            "FORMAT_OUT_OF_RANGE",
            "civil time must be in years 0001 through 9999",
        );
    }
    (days, millis)
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

#[cfg(test)]
mod tests {
    use super::format_rfc3339;

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
}
