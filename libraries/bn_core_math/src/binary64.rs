// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The IEEE 754 binary64 layout of `FLOAT`: a value split into a magnitude
//! in `[1, 2)` and a power of two, and put back together with one rounding.
//! `Product` and Student's t scale by powers of two through these two rules.

/// Fraction bits of an `f64`.
const FRACTION_BITS: u32 = 52;
/// The stored exponent of a normal `f64` is the power of two plus this bias.
const EXPONENT_BIAS: i64 = 1023;
/// A stored exponent with every bit set is an infinity or `NAN`.
const INFINITE_STORED_EXPONENT: i64 = 0x7ff;
const FRACTION_MASK: u64 = 0x000f_ffff_ffff_ffff;
/// `2^52`: scales a subnormal into the normal range without rounding.
const SUBNORMAL_SCALE: f64 = 4_503_599_627_370_496.0;

/// `(magnitude, power)` with `magnitude` in `[1, 2)` and
/// `value = magnitude · 2^power`. `value` is positive and finite.
pub(crate) fn decompose(value: f64) -> (f64, i64) {
    debug_assert!(value > 0.0 && value.is_finite(), "decompose {value}");
    if value < f64::MIN_POSITIVE {
        let (magnitude, power) = decompose(value * SUBNORMAL_SCALE);
        return (magnitude, power - i64::from(FRACTION_BITS));
    }
    let bits = value.to_bits();
    let stored = i64::try_from(bits >> FRACTION_BITS).expect("a positive f64 has no sign bit");
    let magnitude =
        f64::from_bits((bits & FRACTION_MASK) | (EXPONENT_BIAS.unsigned_abs() << FRACTION_BITS));
    (magnitude, stored - EXPONENT_BIAS)
}

/// `magnitude · 2^power` for `magnitude` in `[1, 2)`: `INF` past the largest
/// finite value, and below the smallest normal value rounded once.
pub(crate) fn compose(magnitude: f64, power: i64) -> f64 {
    let stored = power + EXPONENT_BIAS;
    if stored >= INFINITE_STORED_EXPONENT {
        return f64::INFINITY;
    }
    if stored > 0 {
        return f64::from_bits(
            (magnitude.to_bits() & FRACTION_MASK) | (stored.unsigned_abs() << FRACTION_BITS),
        );
    }
    // `magnitude · 2^(stored − 1) · MIN_POSITIVE`. The halvings stay in the
    // normal range and are exact; the final product is the one rounding.
    // Past `FRACTION_BITS + 1` halvings the value is below half the smallest
    // subnormal and rounds to zero.
    let halvings = 1 - stored;
    if halvings > i64::from(FRACTION_BITS) + 1 {
        return 0.0;
    }
    let mut scaled = magnitude;
    for _ in 0..halvings {
        scaled *= 0.5;
    }
    scaled * f64::MIN_POSITIVE
}
