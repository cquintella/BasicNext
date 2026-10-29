// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNMath.Sum`, `Product` and `Dot` (math.md, "Algebraic reductions").
//!
//! The walk is in index order and allocates nothing. `Sum` and `Dot` reuse
//! the Neumaier sum of the descriptive statistics.

use crate::association::LengthMismatch;
use crate::binary64;
use crate::reduce::CompensatedSum;

/// The compensated sum. An empty slice is `0.0`. Any `NAN` is `NAN`.
/// Infinities of both signs are `NAN`; otherwise an infinity wins. A finite
/// sum that overflows keeps that infinity (math.md).
#[must_use]
pub fn sum(values: &[f64]) -> f64 {
    reduce_with_infinities(values.iter().copied())
}

/// The product (math.md). An empty slice is `1.0`. Whether the product is
/// zero, infinite or finite does not depend on order: each finite factor is
/// split into a magnitude in `[1, 2)` and a power of two, the powers are
/// added, and the magnitudes are multiplied in index order.
#[must_use]
pub fn product(values: &[f64]) -> f64 {
    let mut negative = false;
    let mut zero = false;
    let mut infinite = false;
    // The magnitude stays in `[1, 2)` so the power of two carries the
    // overflow. Each factor moves the power by at most 1074, so an `i64`
    // cannot wrap for any vector that fits in memory; an `i32` wraps past
    // about two million large factors.
    let mut magnitude = 1.0;
    let mut power: i64 = 0;
    for value in values {
        if value.is_nan() {
            return f64::NAN;
        }
        if value.is_sign_negative() {
            negative = !negative;
        }
        let absolute = value.abs();
        if absolute == 0.0 {
            zero = true;
        } else if absolute.is_infinite() {
            infinite = true;
        } else {
            let (factor, factor_power) = binary64::decompose(absolute);
            magnitude *= factor;
            power += factor_power;
            if magnitude >= 2.0 {
                magnitude *= 0.5;
                power += 1;
            }
        }
    }
    if zero && infinite {
        return f64::NAN;
    }
    if zero {
        return signed(0.0, negative);
    }
    if infinite {
        return signed(f64::INFINITY, negative);
    }
    signed(binary64::compose(magnitude, power), negative)
}

/// The compensated sum of the products.
///
/// # Errors
///
/// `LengthMismatch` when the vectors have different lengths (math.md,
/// `VECTOR_LENGTH_MISMATCH`).
pub fn dot(left: &[f64], right: &[f64]) -> Result<f64, LengthMismatch> {
    if left.len() != right.len() {
        return Err(LengthMismatch);
    }
    // A `NAN` factor makes its product `NAN`, which ends the reduction.
    Ok(reduce_with_infinities(
        left.iter().zip(right).map(|(left, right)| left * right),
    ))
}

/// `NAN` from a product (`0 · INF`) ends the reduction. Infinities of both
/// signs are `NAN`; one sign of infinity is that infinity; otherwise the
/// compensated sum. An empty iterator is `0.0`.
fn reduce_with_infinities(values: impl Iterator<Item = f64>) -> f64 {
    let mut sum = CompensatedSum::new();
    let (mut positive_infinity, mut negative_infinity) = (false, false);
    let mut seen = false;
    for value in values {
        seen = true;
        if value.is_nan() {
            return f64::NAN;
        } else if value.is_infinite() && value.is_sign_positive() {
            positive_infinity = true;
        } else if value.is_infinite() {
            negative_infinity = true;
        } else {
            sum.add(value);
        }
    }
    if !seen {
        return 0.0;
    }
    match (positive_infinity, negative_infinity) {
        (true, true) => f64::NAN,
        (true, false) => f64::INFINITY,
        (false, true) => f64::NEG_INFINITY,
        (false, false) => sum.total(),
    }
}

fn signed(magnitude: f64, negative: bool) -> f64 {
    if negative { -magnitude } else { magnitude }
}
