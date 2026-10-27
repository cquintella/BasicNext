// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The shape of a distribution for `BNMath`: `Skewness`, `Kurtosis` and
//! their population forms (math.md, "Descriptive statistics"; BDFL
//! decision, 2026-10-27: type 2, G1 and the excess G2, as Excel `SKEW` and
//! `KURT`, and type 1, g1 and g2, for the population forms).

use crate::reduce::CompensatedSum;

/// Which estimator of the shape: the sample one (type 2, adjusted for the
/// sample size) or the population one (type 1).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Estimator {
    Sample,
    Population,
}

/// The least sample sizes the type 2 estimators need: `n - 2` and
/// `(n - 2)(n - 3)` are divisors.
const SAMPLE_SKEWNESS_MINIMUM: usize = 3;
const SAMPLE_KURTOSIS_MINIMUM: usize = 4;

/// The excess kurtosis subtracts the normal distribution's 3.
const NORMAL_KURTOSIS: f64 = 3.0;

/// The corrected sums of the deviations `d = x - mean` of the scaled
/// values: `S2 = Σd² - (Σd)² / n`, `S3 = Σd³`, `S4 = Σd⁴`.
struct CentralSums {
    count: f64,
    second: f64,
    third: f64,
    fourth: f64,
}

impl CentralSums {
    /// The sums, or `None` for an empty vector, an infinite value or a zero
    /// variance. Skewness and kurtosis do not change when every value is
    /// multiplied by the same positive number, so the values are scaled by
    /// the power of two that brings the largest magnitude near 1: exact,
    /// and `d⁴` cannot overflow.
    #[allow(clippy::cast_precision_loss)] // n beyond 2^53 values is not addressable
    fn of<T: Copy + Into<f64>>(values: &[T]) -> Option<Self> {
        let largest = values
            .iter()
            .map(|value| (*value).into().abs())
            .reduce(f64::max)?;
        if !largest.is_finite() || largest == 0.0 {
            return None;
        }
        let scale = 2f64.powi(-binary_exponent(largest));
        let count = values.len() as f64;
        let mut total = CompensatedSum::new();
        for value in values {
            total.add((*value).into() * scale);
        }
        let mean = total.total() / count;
        let mut sums = [(); 4].map(|()| CompensatedSum::new());
        for value in values {
            let d = (*value).into() * scale - mean;
            let square = d * d;
            for (sum, term) in sums
                .iter_mut()
                .zip([d, square, square * d, square * square])
            {
                sum.add(term);
            }
        }
        // In the order of the terms above: Σd, Σd², Σd³, Σd⁴.
        let totals = sums.map(|sum| sum.total());
        let second = totals[1] - totals[0] * totals[0] / count;
        (second > 0.0).then_some(Self {
            count,
            second,
            third: totals[2],
            fourth: totals[3],
        })
    }

    /// `g1 = √n · S3 / S2^(3/2)`.
    fn population_skewness(&self) -> f64 {
        self.count.sqrt() * self.third / (self.second * self.second.sqrt())
    }

    /// `g2 = (n · S4 - 3 · S2²) / S2²`, which is exact when its terms are,
    /// where `n · S4 / S2² - 3` would cancel the leading digits.
    fn population_kurtosis(&self) -> f64 {
        let square = self.second * self.second;
        (self.count * self.fourth - NORMAL_KURTOSIS * square) / square
    }
}

/// The exponent `e` with `2^e ≤ value < 2^(e+1)`, for a finite positive
/// value (a subnormal value gives the least normal exponent, which is still
/// a safe scale).
#[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)] // 11 bits
fn binary_exponent(value: f64) -> i32 {
    const EXPONENT_SHIFT: u32 = 52;
    const EXPONENT_MASK: u64 = 0x7ff;
    const EXPONENT_BIAS: i32 = 1023;
    ((value.to_bits() >> EXPONENT_SHIFT) & EXPONENT_MASK) as i32 - EXPONENT_BIAS
}

/// `Skewness` (G1, type 2: `g1 · √(n(n-1)) / (n - 2)`, `n ≥ 3`) or
/// `PSkewness` (g1, type 1); `NAN` below the size, with an infinite value,
/// or with a zero variance.
#[allow(clippy::cast_precision_loss)] // n beyond 2^53 values is not addressable
pub(crate) fn skewness<T: Copy + Into<f64>>(values: &[T], estimator: Estimator) -> f64 {
    if estimator == Estimator::Sample && values.len() < SAMPLE_SKEWNESS_MINIMUM {
        return f64::NAN;
    }
    let Some(sums) = CentralSums::of(values) else {
        return f64::NAN;
    };
    let population = sums.population_skewness();
    match estimator {
        Estimator::Population => population,
        Estimator::Sample => {
            let n = sums.count;
            population * (n * (n - 1.0)).sqrt() / (n - 2.0)
        }
    }
}

/// `Kurtosis` (the excess G2, type 2: `((n + 1) g2 + 6)(n - 1) /
/// ((n - 2)(n - 3))`, `n ≥ 4`) or `PKurtosis` (the excess g2, type 1); `NAN`
/// below the size, with an infinite value, or with a zero variance.
pub(crate) fn kurtosis<T: Copy + Into<f64>>(values: &[T], estimator: Estimator) -> f64 {
    /// The constant term of G2: `(n + 1) g2 + 6`.
    const SAMPLE_KURTOSIS_OFFSET: f64 = 6.0;
    if estimator == Estimator::Sample && values.len() < SAMPLE_KURTOSIS_MINIMUM {
        return f64::NAN;
    }
    let Some(sums) = CentralSums::of(values) else {
        return f64::NAN;
    };
    match estimator {
        Estimator::Population => sums.population_kurtosis(),
        Estimator::Sample => {
            // ((n + 1) g2 + 6)(n - 1) / ((n - 2)(n - 3)), written over the sums
            // with one division at the end, so exact sums give an exact
            // result.
            let n = sums.count;
            let square = sums.second * sums.second;
            let excess = n * sums.fourth - NORMAL_KURTOSIS * square;
            (n - 1.0) * ((n + 1.0) * excess + SAMPLE_KURTOSIS_OFFSET * square)
                / ((n - 2.0) * (n - 3.0) * square)
        }
    }
}
