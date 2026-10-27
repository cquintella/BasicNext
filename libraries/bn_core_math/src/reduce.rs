// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Statistical and vector reductions for `BNMath` (R5).

use crate::shape::{self, Estimator};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reduction {
    Float(f64),
    Na,
}

/// A descriptive statistic of `BNMath` (math.md, "Descriptive statistics"),
/// named by its `BNMath` member. The native ABI passes it as a code from
/// `bn_types::numeric_abi`, which the runtime turns back into a name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Statistic {
    Mean,
    Median,
    Quartile1,
    Quartile3,
    Range,
    Stdev,
    Variance,
    Mode,
    PopulationVariance,
    PopulationStdev,
    Skewness,
    Kurtosis,
    PopulationSkewness,
    PopulationKurtosis,
}

impl Statistic {
    const ALL: [Self; 14] = [
        Self::Mean,
        Self::Median,
        Self::Quartile1,
        Self::Quartile3,
        Self::Range,
        Self::Stdev,
        Self::Variance,
        Self::Mode,
        Self::PopulationVariance,
        Self::PopulationStdev,
        Self::Skewness,
        Self::Kurtosis,
        Self::PopulationSkewness,
        Self::PopulationKurtosis,
    ];

    /// The `BNMath` member name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Mean => "MEAN",
            Self::Median => "MEDIAN",
            Self::Quartile1 => "QUARTILE1",
            Self::Quartile3 => "QUARTILE3",
            Self::Range => "RANGE",
            Self::Stdev => "STDEV",
            Self::Variance => "VARIANCE",
            Self::Mode => "MODE",
            Self::PopulationVariance => "PVariance",
            Self::PopulationStdev => "PStdev",
            Self::Skewness => "Skewness",
            Self::Kurtosis => "Kurtosis",
            Self::PopulationSkewness => "PSkewness",
            Self::PopulationKurtosis => "PKurtosis",
        }
    }

    /// The statistic a `BNMath` member name denotes.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|statistic| statistic.name() == name)
    }
}

#[must_use]
pub fn vmin_i32(values: &[i32]) -> Option<i32> {
    values.iter().copied().min()
}

#[must_use]
pub fn vmax_i32(values: &[i32]) -> Option<i32> {
    values.iter().copied().max()
}

#[must_use]
pub fn vmin_f64(values: &[f64]) -> Option<f64> {
    values.iter().copied().reduce(|left, right| {
        if left.is_nan() || right.is_nan() {
            f64::NAN
        } else {
            left.min(right)
        }
    })
}

#[must_use]
pub fn vmax_f64(values: &[f64]) -> Option<f64> {
    values.iter().copied().reduce(|left, right| {
        if left.is_nan() || right.is_nan() {
            f64::NAN
        } else {
            left.max(right)
        }
    })
}

/// The additive identity of IEEE 754: `-0.0 + x` is `x` for every `x`,
/// including `-0.0`, which a `+0.0` start would turn into `+0.0`.
const ADDITIVE_IDENTITY: f64 = -0.0;

#[must_use]
pub fn reduce(statistic: Statistic, values: &[i32]) -> Reduction {
    reduce_numbers(statistic, values)
}

#[must_use]
pub fn reduce_f64(statistic: Statistic, values: &[f64]) -> Reduction {
    if values.iter().any(|value| value.is_nan()) {
        return Reduction::Float(f64::NAN);
    }
    reduce_numbers(statistic, values)
}

/// One statistic of `values`, which hold no `NAN`. `MEAN`, `VARIANCE`,
/// `STDEV` and `RANGE` read the values in index order without a copy; the
/// order statistics sort a copy (math.md, "Order, summation and overflow").
fn reduce_numbers<T: Copy + Into<f64>>(statistic: Statistic, values: &[T]) -> Reduction {
    match statistic {
        Statistic::Mean => Reduction::Float(mean(values)),
        Statistic::Variance => Reduction::Float(variance(values, SAMPLE_CORRECTION)),
        Statistic::Stdev => Reduction::Float(variance(values, SAMPLE_CORRECTION).sqrt()),
        Statistic::PopulationVariance => Reduction::Float(variance(values, POPULATION_CORRECTION)),
        Statistic::PopulationStdev => {
            Reduction::Float(variance(values, POPULATION_CORRECTION).sqrt())
        }
        Statistic::Skewness => Reduction::Float(shape::skewness(values, Estimator::Sample)),
        Statistic::Kurtosis => Reduction::Float(shape::kurtosis(values, Estimator::Sample)),
        Statistic::PopulationSkewness => {
            Reduction::Float(shape::skewness(values, Estimator::Population))
        }
        Statistic::PopulationKurtosis => {
            Reduction::Float(shape::kurtosis(values, Estimator::Population))
        }
        Statistic::Range => Reduction::Float(range(values)),
        Statistic::Median => Reduction::Float(median(&sorted_copy(values))),
        Statistic::Quartile1 => Reduction::Float(lower_hinge(&sorted_copy(values))),
        Statistic::Quartile3 => Reduction::Float(upper_hinge(&sorted_copy(values))),
        Statistic::Mode => mode_of_sorted(&sorted_copy(values)),
    }
}

/// Neumaier's compensated sum of finite values. Once the sum overflows it
/// keeps the infinity it reached: the compensation term would be
/// `INF - INF`, which turns the total into `NAN`.
pub(crate) struct CompensatedSum {
    sum: f64,
    compensation: f64,
    overflow: Option<f64>,
}

impl CompensatedSum {
    pub(crate) const fn new() -> Self {
        Self {
            sum: ADDITIVE_IDENTITY,
            compensation: ADDITIVE_IDENTITY,
            overflow: None,
        }
    }

    pub(crate) fn add(&mut self, value: f64) {
        if self.overflow.is_some() {
            return;
        }
        let next = self.sum + value;
        if !next.is_finite() {
            self.overflow = Some(next);
            return;
        }
        self.compensation += if self.sum.abs() >= value.abs() {
            (self.sum - next) + value
        } else {
            (value - next) + self.sum
        };
        self.sum = next;
    }

    /// A zero compensation leaves the sum, and its sign of zero, unchanged.
    #[allow(clippy::float_cmp)]
    pub(crate) fn total(&self) -> f64 {
        self.overflow.unwrap_or(if self.compensation == 0.0 {
            self.sum
        } else {
            self.sum + self.compensation
        })
    }
}

/// G-1, in one pass: infinities decide first (`INF` with `-INF` is `NAN`);
/// otherwise the compensated sum of the finite values over `n`.
#[allow(clippy::cast_precision_loss)] // n beyond 2^53 values is not addressable
#[allow(clippy::float_cmp)] // an infinity is one exact value
fn mean<T: Copy + Into<f64>>(values: &[T]) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    let mut sum = CompensatedSum::new();
    let (mut positive_infinity, mut negative_infinity) = (false, false);
    for value in values {
        let value: f64 = (*value).into();
        if value == f64::INFINITY {
            positive_infinity = true;
        } else if value == f64::NEG_INFINITY {
            negative_infinity = true;
        } else {
            sum.add(value);
        }
    }
    match (positive_infinity, negative_infinity) {
        (true, true) => f64::NAN,
        (true, false) => f64::INFINITY,
        (false, true) => f64::NEG_INFINITY,
        (false, false) => sum.total() / values.len() as f64,
    }
}

/// Bessel's correction of the sample variance and standard deviation: the
/// divisor is `n - 1`.
const SAMPLE_CORRECTION: usize = 1;

/// The population variance and standard deviation divide by `n`.
const POPULATION_CORRECTION: usize = 0;

/// The variance with divisor `n - correction`: `NAN` unless `n` exceeds
/// `correction`, or with an infinite value; otherwise the corrected
/// two-pass algorithm, and `INF` when any of its sums overflows, as `MEAN`
/// (math.md, G-1 and G-11).
fn variance<T: Copy + Into<f64>>(values: &[T], correction: usize) -> f64 {
    if values.len() <= correction || values.iter().any(|value| (*value).into().is_infinite()) {
        return f64::NAN;
    }
    corrected_two_pass(values, correction).unwrap_or(f64::INFINITY)
}

/// The corrected two-pass variance: with `d = x - mean`,
/// `(Σd² - (Σd)² / n) / (n - correction)`, every sum compensated. The
/// `(Σd)²` term removes the error of the mean. `None` when an intermediate
/// value overflows.
#[allow(clippy::cast_precision_loss)] // n beyond 2^53 values is not addressable
fn corrected_two_pass<T: Copy + Into<f64>>(values: &[T], correction: usize) -> Option<f64> {
    // Chan, Golub and LeVeque, "Algorithms for computing the sample
    // variance", The American Statistician 37 (1983).
    let count = values.len() as f64;
    let mut sum = CompensatedSum::new();
    for value in values {
        sum.add((*value).into());
    }
    let mean = sum.total() / count;
    if !mean.is_finite() {
        return None;
    }
    let mut squares = CompensatedSum::new();
    let mut deviations = CompensatedSum::new();
    for value in values {
        let deviation = (*value).into() - mean;
        squares.add(deviation * deviation);
        deviations.add(deviation);
    }
    let (squares, deviations) = (squares.total(), deviations.total());
    let deviation_correction = deviations * deviations / count;
    let divisor = (values.len() - correction) as f64;
    (squares.is_finite() && deviation_correction.is_finite())
        .then(|| (squares - deviation_correction) / divisor)
}

/// `MAX - MIN` in one pass, ordered as the sort is (`-0.0` below `0.0`);
/// `NAN` when empty.
fn range<T: Copy + Into<f64>>(values: &[T]) -> f64 {
    let mut numbers = values.iter().map(|value| (*value).into());
    let Some(first) = numbers.next() else {
        return f64::NAN;
    };
    let (minimum, maximum) = numbers.fold((first, first), |(minimum, maximum), value: f64| {
        (
            if value.total_cmp(&minimum).is_lt() {
                value
            } else {
                minimum
            },
            if value.total_cmp(&maximum).is_gt() {
                value
            } else {
                maximum
            },
        )
    });
    maximum - minimum
}

fn sorted_copy<T: Copy + Into<f64>>(values: &[T]) -> Vec<f64> {
    let mut sorted = values
        .iter()
        .map(|value| (*value).into())
        .collect::<Vec<f64>>();
    sorted.sort_by(f64::total_cmp);
    sorted
}

/// The middle element, or the midpoint of the two middle ones; `NAN` when
/// empty.
fn median(sorted: &[f64]) -> f64 {
    let middle = sorted.len() / 2;
    match sorted.len() {
        0 => f64::NAN,
        length if length % 2 == 1 => sorted[middle],
        _ => sorted[middle - 1].midpoint(sorted[middle]),
    }
}

/// `QUARTILE1`, Tukey's lower hinge: the median of the lower half, which
/// leaves out the overall median when `n` is odd; `NAN` below two values.
fn lower_hinge(sorted: &[f64]) -> f64 {
    if sorted.len() < 2 {
        return f64::NAN;
    }
    median(&sorted[..sorted.len() / 2])
}

/// `QUARTILE3`, Tukey's upper hinge; `NAN` below two values.
fn upper_hinge(sorted: &[f64]) -> f64 {
    if sorted.len() < 2 {
        return f64::NAN;
    }
    median(&sorted[sorted.len().div_ceil(2)..])
}

/// The unique most frequent value of `sorted` (ascending, no `NAN`), or
/// `NA` on a tie, in one pass over its runs of equal values. Equality is
/// FLOAT `==`, so `-0.0` and `0.0` are one run (math.md: "counts exact FLOAT
/// equality"); a run is represented by its first value.
#[allow(clippy::float_cmp)] // exact FLOAT equality is the MODE contract
fn mode_of_sorted(sorted: &[f64]) -> Reduction {
    let mut best: Option<(f64, usize)> = None;
    let mut tie = false;
    let mut index = 0;
    while index < sorted.len() {
        let value = sorted[index];
        let run = sorted[index..]
            .iter()
            .take_while(|candidate| **candidate == value)
            .count();
        match best {
            Some((_, count)) if run == count => tie = true,
            Some((_, count)) if run < count => {}
            _ => {
                best = Some((value, run));
                tie = false;
            }
        }
        index += run;
    }
    match best {
        Some((value, _)) if !tie => Reduction::Float(value),
        _ => Reduction::Na,
    }
}

/// `BNMath.Quantile` of `INTEGER` values (math.md, R type 7).
#[must_use]
pub fn quantile(values: &[i32], p: f64) -> f64 {
    quantile_of(values, p)
}

/// `BNMath.Quantile` of `FLOAT` values: `NAN` when a value is `NAN`.
#[must_use]
pub fn quantile_f64(values: &[f64], p: f64) -> f64 {
    if values.iter().any(|value| value.is_nan()) {
        return f64::NAN;
    }
    quantile_of(values, p)
}

/// R's type 7 (`quantile.default`, R 4.5.1, whose fuzz is 0 for type 7):
/// `index = 1 + (n - 1) p`, `lo = floor(index)`, `hi = ceil(index)`, and
/// `(1 - h) x[lo] + h x[hi]` when the two differ; `NAN` for an empty vector
/// or a `p` outside `[0, 1]`.
#[allow(clippy::cast_precision_loss)] // n beyond 2^53 values is not addressable
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // whole numbers in 1..=n
#[allow(clippy::float_cmp)] // R compares the two order statistics exactly
fn quantile_of<T: Copy + Into<f64>>(values: &[T], p: f64) -> f64 {
    if values.is_empty() || !(0.0..=1.0).contains(&p) {
        return f64::NAN;
    }
    let sorted = sorted_copy(values);
    let index = 1.0 + (sorted.len() - 1) as f64 * p;
    let lower = index.floor() as usize;
    let upper = index.ceil() as usize;
    let (low, high) = (sorted[lower - 1], sorted[upper - 1]);
    if index > lower as f64 && high != low {
        let weight = index - lower as f64;
        (1.0 - weight) * low + weight * high
    } else {
        low
    }
}
