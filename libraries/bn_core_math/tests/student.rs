// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Student's t (math.md, "Student's t").

#![allow(clippy::excessive_precision)] // 28-digit series, rounded to binary64 by the parser
#![allow(clippy::float_cmp)] // infinities and exact zeros are the contract
//!
//! The cumulative value and the p-value are compared with a 28-digit
//! evaluation of the same incomplete-beta series (`bc -l`, scale 28).
//! R 4.5.1 `pt` / `qt` is recorded in comments: it is not correctly
//! rounded, and `qt` is not antisymmetric.

use bn_core_math::{student_t_cdf, student_t_pvalue, student_t_quantile};

/// Conformance points stay inside this many units of the 28-digit series.
const SERIES_ULPS: u64 = 9;

fn ulps(left: f64, right: f64) -> u64 {
    if left.is_nan() && right.is_nan() {
        return 0;
    }
    if left.is_infinite() || right.is_infinite() {
        return u64::from(left.to_bits() != right.to_bits());
    }
    left.to_bits().abs_diff(right.to_bits())
}

fn near_series(actual: f64, reference: f64) {
    let distance = ulps(actual, reference);
    assert!(
        distance <= SERIES_ULPS,
        "{actual:.17} is {distance} ulps from the series {reference:.17}"
    );
}

#[test]
fn cumulative_and_pvalue_match_the_series() {
    // (t, df, cdf, two-tailed p). The decimals are the 28-digit series.
    // R 4.5.1 `pt` for (-1.5, 5) is 0.09695184012123657, 10 ulps from the
    // series; the other R values sit on the series or within a few units.
    let points = [
        (0.0, 10.0, 0.5, 1.0),
        (1.0, 1.0, 0.75, 0.5),
        (
            -1.5,
            5.0,
            0.096_951_840_121_236_716_066,
            0.193_903_680_242_473_432_133,
        ),
        (
            1.96,
            30.0,
            0.970_328_843_551_974_764_014,
            0.059_342_312_896_050_471_972,
        ),
        (
            2.228_138_851_986_273,
            10.0,
            0.974_999_999_999_999_925_894,
            0.050_000_000_000_000_148_211,
        ),
        (
            -0.5,
            0.5,
            0.378_659_036_464_718_315_670,
            0.757_318_072_929_436_631_341,
        ),
        (
            3.0,
            2.5,
            0.963_711_952_225_484_078_053,
            0.072_576_095_549_031_843_893,
        ),
    ];
    for (t, df, cdf, pvalue) in points {
        near_series(student_t_cdf(t, df), cdf);
        near_series(student_t_pvalue(t, df), pvalue);
    }
    near_series(student_t_cdf(f64::INFINITY, 10.0), 1.0);
    near_series(student_t_cdf(f64::NEG_INFINITY, 10.0), 0.0);
    near_series(student_t_pvalue(f64::INFINITY, 10.0), 0.0);
    assert!(student_t_cdf(1.0, 0.0).is_nan());
    assert!(student_t_cdf(1.0, -2.0).is_nan());
    assert!(student_t_cdf(f64::NAN, 5.0).is_nan());
    near_series(student_t_cdf(1.959_963_984_540_054, f64::INFINITY), 0.975);
    near_series(student_t_cdf(0.0, f64::INFINITY), 0.5);
}

#[test]
fn quantiles_invert_the_cumulative_value_and_negate() {
    // (p, df). For p > 0.5 the quantile inverts StudentT. The lower tail
    // is the exact negation, which is what math.md requires. R 4.5.1
    // `qt(0.99, 1)` differs by 51 units; `qt(0.025, 1)` is not the
    // negation of `qt(0.975, 1)`.
    let upper = [
        (0.9, 1.0),
        (0.95, 1.0),
        (0.975, 1.0),
        (0.99, 1.0),
        (0.975, 2.0),
        (0.975, 5.0),
        (0.95, 5.0),
        (0.975, 10.0),
        (0.9, 10.0),
        (0.99, 10.0),
        (0.975, 30.0),
        (0.975, 100.0),
        (0.975, f64::INFINITY),
        (0.95, f64::INFINITY),
        (0.9, f64::INFINITY),
        (0.99, f64::INFINITY),
    ];
    for (probability, df) in upper {
        let magnitude = student_t_quantile(probability, df);
        let lower = student_t_quantile(1.0 - probability, df);
        assert_eq!(
            lower.to_bits(),
            (-magnitude).to_bits(),
            "p={probability} df={df}"
        );
        let back = student_t_cdf(magnitude, df);
        let distance = ulps(back, probability);
        assert!(
            distance <= 1,
            "cdf({magnitude}) is {distance} ulps from {probability} at df={df}"
        );
    }
    assert_eq!(student_t_quantile(0.5, 1.0).to_bits(), 0.0_f64.to_bits());
    assert_eq!(student_t_quantile(0.0, 10.0), f64::NEG_INFINITY);
    assert_eq!(student_t_quantile(1.0, 10.0), f64::INFINITY);
    assert!(student_t_quantile(1.5, 10.0).is_nan());
    assert!(student_t_quantile(0.9, 0.0).is_nan());
    assert!(student_t_quantile(f64::NAN, 5.0).is_nan());
}

/// `df = 1` is the Cauchy distribution: the two-tailed p-value is
/// `(2 / π) · arctan(1 / t)`, which is `2 / (π t)` to double precision for
/// these `t`. Past `|t| ≈ 1.34e154` the square `t²` overflows; below
/// `x = df / (df + t²)` in the normal range the argument loses its bits.
/// Neither may turn the p-value into `0`.
#[test]
fn far_tail_pvalue_keeps_its_magnitude() {
    for t in [1e150, 1e154, 1.4e154, 1e155, 1e200, 1e300, f64::MAX] {
        near_series(student_t_pvalue(t, 1.0), std::f64::consts::FRAC_2_PI / t);
        near_series(student_t_cdf(-t, 1.0), std::f64::consts::FRAC_1_PI / t);
    }
    // A small `df` has a heavy tail: the p-value is far from `0`.
    let heavy = student_t_pvalue(1e300, 0.01);
    assert!(heavy > 1e-4 && heavy < 1e-2, "{heavy}");
    assert!(student_t_pvalue(1e300, 0.01) > student_t_pvalue(1e301, 0.01));
}

/// With a small `df` the quantile lies past any fixed number of doublings
/// of the bracket; the result still inverts the cumulative value.
#[test]
fn small_df_quantiles_invert_the_cumulative_value() {
    for (probability, df) in [(0.99, 0.01), (0.999_999, 0.1), (0.9, 0.05)] {
        let quantile = student_t_quantile(probability, df);
        assert!(quantile.is_finite(), "{probability} {df}: {quantile}");
        let back = student_t_cdf(quantile, df);
        assert!(
            ulps(back, probability) <= 1,
            "{probability} {df}: StudentT({quantile}) = {back}"
        );
    }
}
