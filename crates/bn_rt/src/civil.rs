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

use std::ffi::c_char;

use crate::{c_str, c_string};

/// [`split`], ending the program outside the civil range with the calling
/// site's diagnostic (`trap`, rendered by `bnc`).
pub fn split_at(timestamp: i64, trap: *const std::ffi::c_char) -> (i32, u32) {
    split(timestamp).unwrap_or_else(|| {
        super::trap_abi::fail_at(
            trap,
            bn_catalog::text("temporal", "msg.format_out_of_range"),
            bn_core_text::civil::RANGE_ERROR,
        )
    })
}

/// `Date.Parse(str)` (K80): returns days since 1970-01-01, or ends with the site's trap.
#[allow(unsafe_code)] // SAFETY: C ABI export for LLVM-emitted code reading NUL-terminated strings.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_temporal_date_parse(text: *const c_char, trap: *const c_char) -> i32 {
    let s = c_str(text).unwrap_or("");
    let Some(days) = bn_core_text::temporal::parse_date(s) else {
        super::trap_abi::fail_at(
            trap,
            bn_catalog::text("temporal", "msg.invalid_date"),
            bn_catalog::text("temporal", "msg.invalid_date_2"),
        );
    };
    days
}

/// `Time.Parse(str)` (K80): returns milliseconds since midnight, or ends with the site's trap.
#[allow(unsafe_code)] // SAFETY: C ABI export for LLVM-emitted code reading NUL-terminated strings.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_temporal_time_parse(text: *const c_char, trap: *const c_char) -> i32 {
    let s = c_str(text).unwrap_or("");
    let Some(millis) = bn_core_text::temporal::parse_hms(s, true) else {
        super::trap_abi::fail_at(
            trap,
            bn_catalog::text("temporal", "msg.invalid_time"),
            bn_catalog::text("temporal", "msg.invalid_time_2"),
        );
    };
    i32::try_from(millis).unwrap_or(0)
}

/// `Timestamp.Parse(str)` (K80): returns epoch milliseconds, or ends with the site's trap.
#[allow(unsafe_code)] // SAFETY: C ABI export for LLVM-emitted code reading NUL-terminated strings.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_temporal_timestamp_parse(text: *const c_char, trap: *const c_char) -> i64 {
    let s = c_str(text).unwrap_or("");
    let Some(ts) = bn_core_text::temporal::parse_rfc3339(s) else {
        super::trap_abi::fail_at(
            trap,
            bn_catalog::text("temporal", "msg.parse_error"),
            bn_catalog::text("temporal", "msg.invalid_timestamp"),
        );
    };
    ts
}

/// `Timestamp.Format(ts)` (K80): returns owned RFC 3339 string, or ends with the site's trap.
#[allow(unsafe_code)] // SAFETY: C ABI export for LLVM-emitted code allocating NUL-terminated strings.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_temporal_timestamp_format(
    timestamp: i64,
    trap: *const c_char,
) -> *mut c_char {
    let s = bn_core_text::civil::rfc3339(timestamp).unwrap_or_else(|| {
        super::trap_abi::fail_at(
            trap,
            bn_catalog::text("temporal", "msg.format_out_of_range"),
            bn_core_text::civil::RANGE_ERROR,
        )
    });
    c_string(&s)
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
