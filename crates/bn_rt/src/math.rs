// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::process;

pub(crate) fn fail(code: &str, message: &str) -> ! {
    eprintln!("error[{code}]: {message}");
    process::exit(1);
}

pub fn parse_val(text: &str) -> f64 {
    let text = text.trim_start();
    let bytes = text.as_bytes();
    let mut end = usize::from(bytes.first().is_some_and(|b| matches!(b, b'+' | b'-')));
    let digits = end;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
    }
    if end == digits || (end == digits + 1 && bytes.get(digits) == Some(&b'.')) {
        return 0.0;
    }
    text[..end].parse().unwrap_or(0.0)
}

pub fn iabs(value: i64) -> i64 {
    value
        .checked_abs()
        .unwrap_or_else(|| fail("NUMERIC_OVERFLOW", "BNMath.ABS overflowed"))
}

pub fn isign(value: i64) -> i64 {
    value.signum()
}

pub fn tohour(milliseconds: i64) -> i32 {
    i32::try_from(milliseconds.div_euclid(3_600_000).rem_euclid(24)).unwrap_or(0)
}

pub fn toweekday(milliseconds: i64) -> i32 {
    let days = milliseconds.div_euclid(86_400_000);
    i32::try_from((days + 3).rem_euclid(7) + 1).unwrap_or(4)
}

pub fn fsign(value: f64) -> f64 {
    if value == 0.0 || value.is_nan() {
        value
    } else {
        value.signum()
    }
}

pub fn fmin(left: f64, right: f64) -> f64 {
    if left.is_nan() || right.is_nan() {
        f64::NAN
    } else {
        left.min(right)
    }
}

pub fn fmax(left: f64, right: f64) -> f64 {
    if left.is_nan() || right.is_nan() {
        f64::NAN
    } else {
        left.max(right)
    }
}

pub fn round_ties_even(value: f64, digits: f64) -> f64 {
    let scale = 10_f64.powf(digits);
    (value * scale).round_ties_even() / scale
}
