// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Scalar mathematical functions for `BNMath`.

#[must_use]
pub fn iabs(value: i64) -> Option<i64> {
    value.checked_abs()
}

#[must_use]
pub fn isign(value: i64) -> i64 {
    value.signum()
}

#[must_use]
pub fn imin(left: i64, right: i64) -> i64 {
    left.min(right)
}

#[must_use]
pub fn imax(left: i64, right: i64) -> i64 {
    left.max(right)
}

#[must_use]
pub fn tohour(milliseconds: i64) -> i32 {
    i32::try_from(milliseconds.div_euclid(3_600_000).rem_euclid(24)).unwrap_or(0)
}

#[must_use]
pub fn toweekday(milliseconds: i64) -> i32 {
    let days = milliseconds.div_euclid(86_400_000);
    i32::try_from((days + 3).rem_euclid(7) + 1).unwrap_or(4)
}

#[must_use]
pub fn fsign(value: f64) -> f64 {
    if value == 0.0 || value.is_nan() {
        value
    } else {
        value.signum()
    }
}

#[must_use]
pub fn fmin(left: f64, right: f64) -> f64 {
    if left.is_nan() || right.is_nan() {
        f64::NAN
    } else {
        left.min(right)
    }
}

#[must_use]
pub fn fmax(left: f64, right: f64) -> f64 {
    if left.is_nan() || right.is_nan() {
        f64::NAN
    } else {
        left.max(right)
    }
}

#[must_use]
pub fn round_ties_even(value: f64, digits: f64) -> f64 {
    let scale = 10_f64.powf(digits);
    (value * scale).round_ties_even() / scale
}
