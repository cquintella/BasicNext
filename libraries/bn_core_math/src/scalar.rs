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

/// The binary64 value of π / 180, the one factor `BNMath.DegreeToRad` multiplies by
/// (math.md, "Angle conversions"): a fixed ratio keeps the result identical
/// on every backend and platform.
pub const RADIANS_PER_DEGREE: f64 = std::f64::consts::PI / 180.0;

/// The binary64 value of 180 / π, the one factor `BNMath.RadToDegree` multiplies by.
pub const DEGREES_PER_RADIAN: f64 = 180.0 / std::f64::consts::PI;

/// `BNMath.DegreeToRad`: degrees to radians.
#[must_use]
pub fn degrees_to_radians(degrees: f64) -> f64 {
    degrees * RADIANS_PER_DEGREE
}

/// `BNMath.RadToDegree`: radians to degrees.
#[must_use]
pub fn radians_to_degrees(radians: f64) -> f64 {
    radians * DEGREES_PER_RADIAN
}

/// `BNMath.Gcd` as a magnitude: the greatest common divisor of `|a|` and
/// `|b|` (Euclid). It is at most `2^63`, `Gcd(MIN_INT64, 0)`, so it always
/// fits `u64`; the caller reports a result above `MAX_INT64` (math.md,
/// "Integer divisors").
#[must_use]
pub fn gcd_magnitude(a: i64, b: i64) -> u64 {
    let (mut larger, mut smaller) = (a.unsigned_abs(), b.unsigned_abs());
    while smaller != 0 {
        (larger, smaller) = (smaller, larger % smaller);
    }
    larger
}

/// `BNMath.Lcm`, exact: `|a| / Gcd(a, b) * |b|`, at most `2^126`, so it
/// always fits `i128`; `0` when an argument is `0`.
#[must_use]
pub fn lcm_exact(a: i64, b: i64) -> i128 {
    match gcd_magnitude(a, b) {
        0 => 0,
        divisor => i128::from(a.unsigned_abs() / divisor) * i128::from(b.unsigned_abs()),
    }
}
