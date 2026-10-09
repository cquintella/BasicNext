// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Civil date and time arithmetic of `DATE`, `TIME`, and `TIMESTAMP`, the
//! one implementation both backends use: `bn_interp::temporal` maps its
//! failures to diagnostics, and the C ABI ends the program with the call
//! site's diagnostic (bucket 0.6.2b R6).

pub use bn_core_text::civil::{
    CivilDate, DAY_MS, MAX_YEAR, MIN_YEAR, civil_from_days, civil_time, days_from_civil,
    format_date, format_time, in_range, rfc3339, split, totimestamp,
};

/// [`rfc3339`] for a timestamp known to be in range (the wall clock); ends
/// the program outside it.
#[must_use]
pub fn format_rfc3339(timestamp: i64) -> String {
    split_at(timestamp, std::ptr::null());
    rfc3339(timestamp).unwrap_or_default()
}

/// [`split`], ending the program outside the civil range with the calling
/// site's diagnostic (`trap`, rendered by `bnc`).
pub(crate) fn split_at(timestamp: i64, trap: *const std::ffi::c_char) -> (i32, u32) {
    split(timestamp).unwrap_or_else(|| {
        super::math::fail_at(
            trap,
            "FORMAT_OUT_OF_RANGE",
            bn_core_text::civil::RANGE_ERROR,
        )
    })
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
