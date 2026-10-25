// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Canonical IANA timezone resolution, daylight saving time calculations,
//! and civil time conversion across timezones.

use crate::civil::{
    CivilDate, civil_from_days, days_from_civil, format_date, format_time, split, totimestamp,
};

const SECONDS_PER_MINUTE: i32 = 60;
const MINUTES_PER_HOUR: i32 = 60;
const SECONDS_PER_HOUR: i32 = SECONDS_PER_MINUTE * MINUTES_PER_HOUR;
const MILLIS_PER_SECOND: i64 = 1000;
const MILLIS_PER_HOUR: i64 = (SECONDS_PER_HOUR as i64) * MILLIS_PER_SECOND;
const MILLIS_PER_DAY: i64 = 24 * MILLIS_PER_HOUR;
const ONE_HOUR_DST_SECS: i32 = SECONDS_PER_HOUR;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DstRule {
    None,
    UsCanada,
    EuUk,
    Brazil,
    AuEast,
    Nz,
}

#[derive(Clone, Copy, Debug)]
pub struct TimeZoneInfo {
    pub base_offset: i32,
    pub rule: DstRule,
}

const TIMEZONES: &[(&str, TimeZoneInfo)] = &[
    (
        "UTC",
        TimeZoneInfo {
            base_offset: 0,
            rule: DstRule::None,
        },
    ),
    (
        "Etc/UTC",
        TimeZoneInfo {
            base_offset: 0,
            rule: DstRule::None,
        },
    ),
    (
        "Etc/GMT",
        TimeZoneInfo {
            base_offset: 0,
            rule: DstRule::None,
        },
    ),
    (
        "GMT",
        TimeZoneInfo {
            base_offset: 0,
            rule: DstRule::None,
        },
    ),
    // North America
    (
        "America/New_York",
        TimeZoneInfo {
            base_offset: -5 * SECONDS_PER_HOUR,
            rule: DstRule::UsCanada,
        },
    ),
    (
        "America/Detroit",
        TimeZoneInfo {
            base_offset: -5 * SECONDS_PER_HOUR,
            rule: DstRule::UsCanada,
        },
    ),
    (
        "America/Chicago",
        TimeZoneInfo {
            base_offset: -6 * SECONDS_PER_HOUR,
            rule: DstRule::UsCanada,
        },
    ),
    (
        "America/Denver",
        TimeZoneInfo {
            base_offset: -7 * SECONDS_PER_HOUR,
            rule: DstRule::UsCanada,
        },
    ),
    (
        "America/Phoenix",
        TimeZoneInfo {
            base_offset: -7 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    (
        "America/Los_Angeles",
        TimeZoneInfo {
            base_offset: -8 * SECONDS_PER_HOUR,
            rule: DstRule::UsCanada,
        },
    ),
    (
        "America/Anchorage",
        TimeZoneInfo {
            base_offset: -9 * SECONDS_PER_HOUR,
            rule: DstRule::UsCanada,
        },
    ),
    (
        "America/Honolulu",
        TimeZoneInfo {
            base_offset: -10 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    (
        "America/Toronto",
        TimeZoneInfo {
            base_offset: -5 * SECONDS_PER_HOUR,
            rule: DstRule::UsCanada,
        },
    ),
    (
        "America/Vancouver",
        TimeZoneInfo {
            base_offset: -8 * SECONDS_PER_HOUR,
            rule: DstRule::UsCanada,
        },
    ),
    // Europe
    (
        "Europe/London",
        TimeZoneInfo {
            base_offset: 0,
            rule: DstRule::EuUk,
        },
    ),
    (
        "Europe/Dublin",
        TimeZoneInfo {
            base_offset: 0,
            rule: DstRule::EuUk,
        },
    ),
    (
        "Europe/Lisbon",
        TimeZoneInfo {
            base_offset: 0,
            rule: DstRule::EuUk,
        },
    ),
    (
        "Europe/Paris",
        TimeZoneInfo {
            base_offset: SECONDS_PER_HOUR,
            rule: DstRule::EuUk,
        },
    ),
    (
        "Europe/Berlin",
        TimeZoneInfo {
            base_offset: SECONDS_PER_HOUR,
            rule: DstRule::EuUk,
        },
    ),
    (
        "Europe/Rome",
        TimeZoneInfo {
            base_offset: SECONDS_PER_HOUR,
            rule: DstRule::EuUk,
        },
    ),
    (
        "Europe/Madrid",
        TimeZoneInfo {
            base_offset: SECONDS_PER_HOUR,
            rule: DstRule::EuUk,
        },
    ),
    (
        "Europe/Amsterdam",
        TimeZoneInfo {
            base_offset: SECONDS_PER_HOUR,
            rule: DstRule::EuUk,
        },
    ),
    (
        "Europe/Brussels",
        TimeZoneInfo {
            base_offset: SECONDS_PER_HOUR,
            rule: DstRule::EuUk,
        },
    ),
    (
        "Europe/Vienna",
        TimeZoneInfo {
            base_offset: SECONDS_PER_HOUR,
            rule: DstRule::EuUk,
        },
    ),
    (
        "Europe/Stockholm",
        TimeZoneInfo {
            base_offset: SECONDS_PER_HOUR,
            rule: DstRule::EuUk,
        },
    ),
    (
        "Europe/Oslo",
        TimeZoneInfo {
            base_offset: SECONDS_PER_HOUR,
            rule: DstRule::EuUk,
        },
    ),
    (
        "Europe/Copenhagen",
        TimeZoneInfo {
            base_offset: SECONDS_PER_HOUR,
            rule: DstRule::EuUk,
        },
    ),
    (
        "Europe/Warsaw",
        TimeZoneInfo {
            base_offset: SECONDS_PER_HOUR,
            rule: DstRule::EuUk,
        },
    ),
    (
        "Europe/Prague",
        TimeZoneInfo {
            base_offset: SECONDS_PER_HOUR,
            rule: DstRule::EuUk,
        },
    ),
    (
        "Europe/Budapest",
        TimeZoneInfo {
            base_offset: SECONDS_PER_HOUR,
            rule: DstRule::EuUk,
        },
    ),
    (
        "Europe/Athens",
        TimeZoneInfo {
            base_offset: 2 * SECONDS_PER_HOUR,
            rule: DstRule::EuUk,
        },
    ),
    (
        "Europe/Helsinki",
        TimeZoneInfo {
            base_offset: 2 * SECONDS_PER_HOUR,
            rule: DstRule::EuUk,
        },
    ),
    (
        "Europe/Bucharest",
        TimeZoneInfo {
            base_offset: 2 * SECONDS_PER_HOUR,
            rule: DstRule::EuUk,
        },
    ),
    (
        "Europe/Kyiv",
        TimeZoneInfo {
            base_offset: 2 * SECONDS_PER_HOUR,
            rule: DstRule::EuUk,
        },
    ),
    // South America
    (
        "America/Sao_Paulo",
        TimeZoneInfo {
            base_offset: -3 * SECONDS_PER_HOUR,
            rule: DstRule::Brazil,
        },
    ),
    (
        "America/Buenos_Aires",
        TimeZoneInfo {
            base_offset: -3 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    (
        "America/Argentina/Buenos_Aires",
        TimeZoneInfo {
            base_offset: -3 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    (
        "America/Santiago",
        TimeZoneInfo {
            base_offset: -4 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    (
        "America/Bogota",
        TimeZoneInfo {
            base_offset: -5 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    (
        "America/Lima",
        TimeZoneInfo {
            base_offset: -5 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    (
        "America/Montevideo",
        TimeZoneInfo {
            base_offset: -3 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    // Asia
    (
        "Asia/Tokyo",
        TimeZoneInfo {
            base_offset: 9 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    (
        "Asia/Seoul",
        TimeZoneInfo {
            base_offset: 9 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    (
        "Asia/Shanghai",
        TimeZoneInfo {
            base_offset: 8 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    (
        "Asia/Hong_Kong",
        TimeZoneInfo {
            base_offset: 8 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    (
        "Asia/Singapore",
        TimeZoneInfo {
            base_offset: 8 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    (
        "Asia/Taipei",
        TimeZoneInfo {
            base_offset: 8 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    (
        "Asia/Manila",
        TimeZoneInfo {
            base_offset: 8 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    (
        "Asia/Kolkata",
        TimeZoneInfo {
            base_offset: 5 * SECONDS_PER_HOUR + 30 * SECONDS_PER_MINUTE,
            rule: DstRule::None,
        },
    ),
    (
        "Asia/Bangkok",
        TimeZoneInfo {
            base_offset: 7 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    (
        "Asia/Jakarta",
        TimeZoneInfo {
            base_offset: 7 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    (
        "Asia/Dubai",
        TimeZoneInfo {
            base_offset: 4 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    // Oceania
    (
        "Australia/Sydney",
        TimeZoneInfo {
            base_offset: 10 * SECONDS_PER_HOUR,
            rule: DstRule::AuEast,
        },
    ),
    (
        "Australia/Melbourne",
        TimeZoneInfo {
            base_offset: 10 * SECONDS_PER_HOUR,
            rule: DstRule::AuEast,
        },
    ),
    (
        "Australia/Brisbane",
        TimeZoneInfo {
            base_offset: 10 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    (
        "Australia/Perth",
        TimeZoneInfo {
            base_offset: 8 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    (
        "Australia/Adelaide",
        TimeZoneInfo {
            base_offset: 9 * SECONDS_PER_HOUR + 30 * SECONDS_PER_MINUTE,
            rule: DstRule::AuEast,
        },
    ),
    (
        "Pacific/Auckland",
        TimeZoneInfo {
            base_offset: 12 * SECONDS_PER_HOUR,
            rule: DstRule::Nz,
        },
    ),
    (
        "Pacific/Honolulu",
        TimeZoneInfo {
            base_offset: -10 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    (
        "Pacific/Fiji",
        TimeZoneInfo {
            base_offset: 12 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    // Africa
    (
        "Africa/Cairo",
        TimeZoneInfo {
            base_offset: 2 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    (
        "Africa/Johannesburg",
        TimeZoneInfo {
            base_offset: 2 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    (
        "Africa/Lagos",
        TimeZoneInfo {
            base_offset: SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
    (
        "Africa/Nairobi",
        TimeZoneInfo {
            base_offset: 3 * SECONDS_PER_HOUR,
            rule: DstRule::None,
        },
    ),
];

/// Looks up timezone metadata by canonical IANA identifier or `Etc/GMT[+-]N`.
#[must_use]
pub fn lookup_timezone(name: &str) -> Option<TimeZoneInfo> {
    if let Some((_, info)) = TIMEZONES.iter().find(|(zone_name, _)| *zone_name == name) {
        return Some(*info);
    }
    // Handle Etc/GMT+X or Etc/GMT-X (POSIX sign convention: GMT+X is west of UTC, so negative offset)
    if let Some(suffix) = name.strip_prefix("Etc/GMT") {
        if suffix.is_empty() {
            return Some(TimeZoneInfo {
                base_offset: 0,
                rule: DstRule::None,
            });
        }
        let (sign, digits) = suffix.split_at(1);
        let hours: i32 = digits.parse().ok()?;
        if (1..=14).contains(&hours) {
            let offset = match sign {
                "+" => -hours * SECONDS_PER_HOUR,
                "-" => hours * SECONDS_PER_HOUR,
                _ => return None,
            };
            return Some(TimeZoneInfo {
                base_offset: offset,
                rule: DstRule::None,
            });
        }
    }
    None
}

/// Validates whether a timezone identifier is a known, supported canonical timezone.
#[must_use]
pub fn is_known_timezone(name: &str) -> bool {
    lookup_timezone(name).is_some()
}

/// Returns the weekday of a day count since 1970-01-01 (0 = Sunday, 1 = Monday, ..., 6 = Saturday).
fn weekday_sunday_zero(days: i32) -> u32 {
    let shifted = (days + 4).rem_euclid(7);
    u32::try_from(shifted).unwrap_or(0)
}

/// Day of the month of the N-th Sunday of a given month and year (1-based, n = 1, 2, ...).
fn nth_sunday_of_month(year: i32, month: u32, n: u32) -> Option<u32> {
    let first_day = days_from_civil(year, month, 1)?;
    let w = weekday_sunday_zero(first_day);
    let first_sunday = 1 + (7 - w) % 7;
    Some(first_sunday + (n - 1) * 7)
}

/// Day of the month of the last Sunday of a given month and year.
fn last_sunday_of_month(year: i32, month: u32) -> Option<u32> {
    let last_day_num = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => return None,
    };
    let last_day = days_from_civil(year, month, last_day_num)?;
    let w = weekday_sunday_zero(last_day);
    Some(last_day_num - w)
}

/// Evaluates whether DST is active and returns the total offset in seconds for a given timestamp.
#[must_use]
pub fn timezone_offset(name: &str, timestamp: i64) -> Option<i32> {
    let info = lookup_timezone(name)?;
    let is_dst = is_dst_active(info, timestamp);
    let offset = if is_dst {
        info.base_offset + ONE_HOUR_DST_SECS
    } else {
        info.base_offset
    };
    Some(offset)
}

/// Returns true if daylight saving time is active in the given timezone at timestamp `ts`.
#[must_use]
pub fn timezone_is_dst(name: &str, timestamp: i64) -> Option<bool> {
    let info = lookup_timezone(name)?;
    Some(is_dst_active(info, timestamp))
}

fn us_canada_dst(base_offset: i32, year: i32, timestamp: i64) -> bool {
    let Some(start_day) = nth_sunday_of_month(year, 3, 2) else {
        return false;
    };
    let Some(start_days) = days_from_civil(year, 3, start_day) else {
        return false;
    };
    let start_utc_ms = i64::from(start_days) * MILLIS_PER_DAY + 2 * MILLIS_PER_HOUR
        - i64::from(base_offset) * MILLIS_PER_SECOND;

    let Some(end_day) = nth_sunday_of_month(year, 11, 1) else {
        return false;
    };
    let Some(end_days) = days_from_civil(year, 11, end_day) else {
        return false;
    };
    let end_utc_ms = i64::from(end_days) * MILLIS_PER_DAY + 2 * MILLIS_PER_HOUR
        - i64::from(base_offset + ONE_HOUR_DST_SECS) * MILLIS_PER_SECOND;

    timestamp >= start_utc_ms && timestamp < end_utc_ms
}

fn eu_uk_dst(year: i32, timestamp: i64) -> bool {
    let Some(start_day) = last_sunday_of_month(year, 3) else {
        return false;
    };
    let Some(start_days) = days_from_civil(year, 3, start_day) else {
        return false;
    };
    let start_utc_ms = i64::from(start_days) * MILLIS_PER_DAY + MILLIS_PER_HOUR;

    let Some(end_day) = last_sunday_of_month(year, 10) else {
        return false;
    };
    let Some(end_days) = days_from_civil(year, 10, end_day) else {
        return false;
    };
    let end_utc_ms = i64::from(end_days) * MILLIS_PER_DAY + MILLIS_PER_HOUR;

    timestamp >= start_utc_ms && timestamp < end_utc_ms
}

fn brazil_dst(year: i32, timestamp: i64) -> bool {
    if year >= 2019 {
        return false;
    }
    let Some(start_day) = nth_sunday_of_month(year, 10, 3) else {
        return false;
    };
    let Some(start_days) = days_from_civil(year, 10, start_day) else {
        return false;
    };
    let start_utc_ms = i64::from(start_days) * MILLIS_PER_DAY + 3 * MILLIS_PER_HOUR;

    let Some(end_day) = nth_sunday_of_month(year + 1, 2, 3) else {
        return false;
    };
    let Some(end_days) = days_from_civil(year + 1, 2, end_day) else {
        return false;
    };
    let end_utc_ms = i64::from(end_days) * MILLIS_PER_DAY + 2 * MILLIS_PER_HOUR;

    if timestamp >= start_utc_ms && timestamp < end_utc_ms {
        return true;
    }

    let Some(prev_end_day) = nth_sunday_of_month(year, 2, 3) else {
        return false;
    };
    let Some(prev_end_days) = days_from_civil(year, 2, prev_end_day) else {
        return false;
    };
    let prev_end_utc_ms = i64::from(prev_end_days) * MILLIS_PER_DAY + 2 * MILLIS_PER_HOUR;
    timestamp < prev_end_utc_ms
}

fn au_east_dst(base_offset: i32, year: i32, timestamp: i64) -> bool {
    let Some(start_day) = nth_sunday_of_month(year, 10, 1) else {
        return false;
    };
    let Some(start_days) = days_from_civil(year, 10, start_day) else {
        return false;
    };
    let start_utc_ms = i64::from(start_days) * MILLIS_PER_DAY + 2 * MILLIS_PER_HOUR
        - i64::from(base_offset) * MILLIS_PER_SECOND;

    let Some(end_day) = nth_sunday_of_month(year + 1, 4, 1) else {
        return false;
    };
    let Some(end_days) = days_from_civil(year + 1, 4, end_day) else {
        return false;
    };
    let end_utc_ms = i64::from(end_days) * MILLIS_PER_DAY + 3 * MILLIS_PER_HOUR
        - i64::from(base_offset + ONE_HOUR_DST_SECS) * MILLIS_PER_SECOND;

    if timestamp >= start_utc_ms && timestamp < end_utc_ms {
        return true;
    }

    let Some(prev_end_day) = nth_sunday_of_month(year, 4, 1) else {
        return false;
    };
    let Some(prev_end_days) = days_from_civil(year, 4, prev_end_day) else {
        return false;
    };
    let prev_end_utc_ms = i64::from(prev_end_days) * MILLIS_PER_DAY + 3 * MILLIS_PER_HOUR
        - i64::from(base_offset + ONE_HOUR_DST_SECS) * MILLIS_PER_SECOND;
    timestamp < prev_end_utc_ms
}

fn nz_dst(base_offset: i32, year: i32, timestamp: i64) -> bool {
    let Some(start_day) = last_sunday_of_month(year, 9) else {
        return false;
    };
    let Some(start_days) = days_from_civil(year, 9, start_day) else {
        return false;
    };
    let start_utc_ms = i64::from(start_days) * MILLIS_PER_DAY + 2 * MILLIS_PER_HOUR
        - i64::from(base_offset) * MILLIS_PER_SECOND;

    let Some(end_day) = nth_sunday_of_month(year + 1, 4, 1) else {
        return false;
    };
    let Some(end_days) = days_from_civil(year + 1, 4, end_day) else {
        return false;
    };
    let end_utc_ms = i64::from(end_days) * MILLIS_PER_DAY + 3 * MILLIS_PER_HOUR
        - i64::from(base_offset + ONE_HOUR_DST_SECS) * MILLIS_PER_SECOND;

    if timestamp >= start_utc_ms && timestamp < end_utc_ms {
        return true;
    }

    let Some(prev_end_day) = nth_sunday_of_month(year, 4, 1) else {
        return false;
    };
    let Some(prev_end_days) = days_from_civil(year, 4, prev_end_day) else {
        return false;
    };
    let prev_end_utc_ms = i64::from(prev_end_days) * MILLIS_PER_DAY + 3 * MILLIS_PER_HOUR
        - i64::from(base_offset + ONE_HOUR_DST_SECS) * MILLIS_PER_SECOND;
    timestamp < prev_end_utc_ms
}

fn is_dst_active(info: TimeZoneInfo, timestamp: i64) -> bool {
    let Some((days, _)) = split(timestamp) else {
        return false;
    };
    let CivilDate { year, .. } = civil_from_days(days);
    match info.rule {
        DstRule::None => false,
        DstRule::UsCanada => us_canada_dst(info.base_offset, year, timestamp),
        DstRule::EuUk => eu_uk_dst(year, timestamp),
        DstRule::Brazil => brazil_dst(year, timestamp),
        DstRule::AuEast => au_east_dst(info.base_offset, year, timestamp),
        DstRule::Nz => nz_dst(info.base_offset, year, timestamp),
    }
}

/// Formats a UTC timestamp into canonical RFC 3339 text in the given timezone.
#[must_use]
pub fn format_at_zone(timestamp: i64, timezone_name: &str) -> Option<String> {
    let offset_secs = timezone_offset(timezone_name, timestamp)?;
    let local_timestamp = timestamp.checked_add(i64::from(offset_secs) * MILLIS_PER_SECOND)?;
    let (days, millis) = split(local_timestamp)?;
    let date_str = format_date(days);
    let time_str = format_time(millis);

    if offset_secs == 0 {
        return Some(format!("{date_str}T{time_str}Z"));
    }

    let sign = if offset_secs < 0 { '-' } else { '+' };
    let abs_secs = offset_secs.abs();
    let hours = abs_secs / SECONDS_PER_HOUR;
    let minutes = (abs_secs % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE;
    Some(format!(
        "{date_str}T{time_str}{sign}{hours:02}:{minutes:02}"
    ))
}

/// Converts local civil date and time in the given timezone into a UTC timestamp.
#[must_use]
pub fn timestamp_from_local(days: i32, millis: i32, timezone_name: &str) -> Option<i64> {
    let info = lookup_timezone(timezone_name)?;
    let local_ms = totimestamp(days, millis);

    // Initial estimate using standard base offset
    let approx_utc = local_ms - i64::from(info.base_offset) * MILLIS_PER_SECOND;
    let offset1 = timezone_offset(timezone_name, approx_utc)?;
    let utc1 = local_ms - i64::from(offset1) * MILLIS_PER_SECOND;

    // Check if the offset matches at the calculated UTC time
    let offset2 = timezone_offset(timezone_name, utc1)?;
    let final_utc = if offset2 == offset1 {
        utc1
    } else {
        local_ms - i64::from(offset2) * MILLIS_PER_SECOND
    };

    let (utc_days, _) = split(final_utc)?;
    if crate::civil::in_range(utc_days) {
        Some(final_utc)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_offset_and_formatting() {
        assert_eq!(timezone_offset("UTC", 0), Some(0));
        assert_eq!(timezone_is_dst("UTC", 0), Some(false));
        assert_eq!(
            format_at_zone(0, "UTC"),
            Some("1970-01-01T00:00:00.000Z".into())
        );
    }

    #[test]
    fn america_sao_paulo_without_dst_in_2026() {
        // 2026-08-22 10:20:30.000 UTC
        let (days, millis) = (20687, 37_230_000);
        let ts = totimestamp(days, millis);
        assert_eq!(timezone_offset("America/Sao_Paulo", ts), Some(-3 * 3600));
        assert_eq!(timezone_is_dst("America/Sao_Paulo", ts), Some(false));
        // Local time: 10:20:30 - 3h = 07:20:30.000-03:00
        assert_eq!(
            format_at_zone(ts, "America/Sao_Paulo"),
            Some("2026-08-22T07:20:30.000-03:00".into())
        );

        // Round trip from local
        let local_days = days_from_civil(2026, 8, 22).unwrap();
        let local_millis = (7 * 3600 + 20 * 60 + 30) * 1000;
        let converted = timestamp_from_local(local_days, local_millis, "America/Sao_Paulo");
        assert_eq!(converted, Some(ts));
    }

    #[test]
    fn europe_london_summer_and_winter() {
        // Winter: 2026-01-15 12:00:00 UTC (GMT, offset 0, DST false)
        let winter_days = days_from_civil(2026, 1, 15).unwrap();
        let winter_ts = totimestamp(winter_days, 12 * 3600 * 1000);
        assert_eq!(timezone_offset("Europe/London", winter_ts), Some(0));
        assert_eq!(timezone_is_dst("Europe/London", winter_ts), Some(false));
        assert_eq!(
            format_at_zone(winter_ts, "Europe/London"),
            Some("2026-01-15T12:00:00.000Z".into())
        );

        // Summer: 2026-07-15 12:00:00 UTC (BST, offset +3600, DST true)
        let summer_days = days_from_civil(2026, 7, 15).unwrap();
        let summer_ts = totimestamp(summer_days, 12 * 3600 * 1000);
        assert_eq!(timezone_offset("Europe/London", summer_ts), Some(3600));
        assert_eq!(timezone_is_dst("Europe/London", summer_ts), Some(true));
        assert_eq!(
            format_at_zone(summer_ts, "Europe/London"),
            Some("2026-07-15T13:00:00.000+01:00".into())
        );
    }

    #[test]
    fn america_new_york_summer_and_winter() {
        // Winter: 2026-01-15 12:00:00 UTC (EST, offset -5h = -18000, DST false)
        let winter_days = days_from_civil(2026, 1, 15).unwrap();
        let winter_ts = totimestamp(winter_days, 12 * 3600 * 1000);
        assert_eq!(timezone_offset("America/New_York", winter_ts), Some(-18000));
        assert_eq!(timezone_is_dst("America/New_York", winter_ts), Some(false));
        assert_eq!(
            format_at_zone(winter_ts, "America/New_York"),
            Some("2026-01-15T07:00:00.000-05:00".into())
        );

        // Summer: 2026-07-15 12:00:00 UTC (EDT, offset -4h = -14400, DST true)
        let summer_days = days_from_civil(2026, 7, 15).unwrap();
        let summer_ts = totimestamp(summer_days, 12 * 3600 * 1000);
        assert_eq!(timezone_offset("America/New_York", summer_ts), Some(-14400));
        assert_eq!(timezone_is_dst("America/New_York", summer_ts), Some(true));
        assert_eq!(
            format_at_zone(summer_ts, "America/New_York"),
            Some("2026-07-15T08:00:00.000-04:00".into())
        );
    }

    #[test]
    fn asia_tokyo_constant_offset() {
        let days = days_from_civil(2026, 5, 1).unwrap();
        let ts = totimestamp(days, 12 * 3600 * 1000);
        assert_eq!(timezone_offset("Asia/Tokyo", ts), Some(9 * 3600));
        assert_eq!(timezone_is_dst("Asia/Tokyo", ts), Some(false));
        assert_eq!(
            format_at_zone(ts, "Asia/Tokyo"),
            Some("2026-05-01T21:00:00.000+09:00".into())
        );
    }

    #[test]
    fn etc_gmt_offset_parsing() {
        assert_eq!(timezone_offset("Etc/GMT+3", 0), Some(-3 * 3600));
        assert_eq!(timezone_offset("Etc/GMT-5", 0), Some(5 * 3600));
    }
}
