// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Statistics of two vectors for `BNMath`: `Covariance` and `Correlation`
//! (math.md, "Descriptive statistics"; BDFL decisions G-5 and the overflow
//! rules, 2026-10-27), on the corrected two-pass kernel of `VARIANCE`.

use crate::reduce::CompensatedSum;

/// The two vectors of a covariance or correlation differ in length; the
/// backends report `VECTOR_LENGTH_MISMATCH`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LengthMismatch;

/// The power of two that brings overflowing sums back into range: values
/// up to `2^1024 · 2^-540 = 2^484` give products up to `2^970`, which
/// leaves room for `2^54` values. Scaling by a power of two is exact.
const ASSOCIATION_SCALE_EXPONENT: i32 = 540;

/// The compensated sums of the corrected two-pass algorithm for `x · scale`
/// and `y · scale`, with `d = x - mean(x)` and `e = y - mean(y)`.
struct CoMoments {
    count: f64,
    deviation_x_sum: f64,
    deviation_y_sum: f64,
    cross_sum: f64,
    square_x_sum: f64,
    square_y_sum: f64,
}

impl CoMoments {
    /// The sums, or `None` when one of them overflows.
    #[allow(clippy::cast_precision_loss)] // n beyond 2^53 values is not addressable
    fn of(x: &[f64], y: &[f64], scale: f64) -> Option<Self> {
        let count = x.len() as f64;
        let mean = |values: &[f64]| {
            let mut sum = CompensatedSum::new();
            for value in values {
                sum.add(value * scale);
            }
            sum.total() / count
        };
        let (mean_x, mean_y) = (mean(x), mean(y));
        if !mean_x.is_finite() || !mean_y.is_finite() {
            return None;
        }
        let mut sums = [(); 5].map(|()| CompensatedSum::new());
        for (x, y) in x.iter().zip(y) {
            let d = x * scale - mean_x;
            let e = y * scale - mean_y;
            for (sum, term) in sums.iter_mut().zip([d, e, d * e, d * d, e * e]) {
                sum.add(term);
            }
        }
        // In the order of the terms above: Σd, Σe, Σde, Σd², Σe².
        let totals = sums.map(|sum| sum.total());
        totals.iter().all(|sum| sum.is_finite()).then_some(Self {
            count,
            deviation_x_sum: totals[0],
            deviation_y_sum: totals[1],
            cross_sum: totals[2],
            square_x_sum: totals[3],
            square_y_sum: totals[4],
        })
    }

    /// `Σde - Σd · Σe / n`, the co-moment with the error of the means removed.
    fn co_moment(&self) -> f64 {
        self.cross_sum - self.deviation_x_sum * self.deviation_y_sum / self.count
    }

    fn spread_x(&self) -> f64 {
        self.square_x_sum - self.deviation_x_sum * self.deviation_x_sum / self.count
    }

    fn spread_y(&self) -> f64 {
        self.square_y_sum - self.deviation_y_sum * self.deviation_y_sum / self.count
    }
}

/// `NAN` unless both vectors hold at least two values, all finite.
fn defined(x: &[f64], y: &[f64]) -> bool {
    x.len() >= 2 && x.iter().chain(y).all(|value| value.is_finite())
}

/// `BNMath.Covariance`, the sample covariance (`n - 1`). An overflow is
/// taken again on scaled values, so the result is the true value, or
/// `±INF` with its sign beyond `MAX`.
///
/// # Errors
///
/// `LengthMismatch` when `x` and `y` differ in length.
///
/// # Panics
///
/// Never: values scaled by `2^-540` cannot overflow the sums.
pub fn covariance(x: &[f64], y: &[f64]) -> Result<f64, LengthMismatch> {
    if x.len() != y.len() {
        return Err(LengthMismatch);
    }
    if !defined(x, y) {
        return Ok(f64::NAN);
    }
    let divisor = |moments: &CoMoments| moments.count - 1.0;
    if let Some(moments) = CoMoments::of(x, y, 1.0)
        && moments.co_moment().is_finite()
    {
        return Ok(moments.co_moment() / divisor(&moments));
    }
    let unscale = 2f64.powi(ASSOCIATION_SCALE_EXPONENT);
    let moments = CoMoments::of(x, y, 2f64.powi(-ASSOCIATION_SCALE_EXPONENT))
        .expect("scaled values cannot overflow the sums");
    // Each scaled value carries 2^-540, so the co-moment carries 2^-1080.
    Ok(moments.co_moment() / divisor(&moments) * unscale * unscale)
}

/// `BNMath.Correlation`, Pearson's, held in `[-1, 1]` as R's `cor`. A
/// constant vector gives `NAN`. An overflow is taken again on scaled
/// values, which do not change a correlation.
///
/// # Errors
///
/// `LengthMismatch` when `x` and `y` differ in length.
///
/// # Panics
///
/// Never: values scaled by `2^-540` cannot overflow the sums.
pub fn correlation(x: &[f64], y: &[f64]) -> Result<f64, LengthMismatch> {
    if x.len() != y.len() {
        return Err(LengthMismatch);
    }
    if !defined(x, y) || constant(x) || constant(y) {
        return Ok(f64::NAN);
    }
    let moments = CoMoments::of(x, y, 1.0)
        .filter(|moments| moments.co_moment().is_finite())
        .or_else(|| CoMoments::of(x, y, 2f64.powi(-ASSOCIATION_SCALE_EXPONENT)))
        .expect("scaled values cannot overflow the sums");
    let (spread_x, spread_y) = (moments.spread_x(), moments.spread_y());
    if spread_x <= 0.0 || spread_y <= 0.0 {
        return Ok(f64::NAN);
    }
    // One square root of the product rounds once less than two; the product
    // is used only while it is a normal number.
    let product = spread_x * spread_y;
    let scale = if product.is_finite() && product >= f64::MIN_POSITIVE {
        product.sqrt()
    } else {
        spread_x.sqrt() * spread_y.sqrt()
    };
    let correlation = moments.co_moment() / scale;
    Ok(correlation.clamp(-1.0, 1.0))
}

/// Whether every value equals the first: the variance is zero.
#[allow(clippy::float_cmp)] // equal values are exactly equal
fn constant(values: &[f64]) -> bool {
    values.iter().all(|value| *value == values[0])
}
