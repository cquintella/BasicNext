// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNMath` descriptive statistics (math.md, "Descriptive statistics"):
//! the one implementation both backends call.

#![allow(clippy::float_cmp)] // Exact values are the contract.

use bn_core_math::{
    Reduction, Statistic, quantile, quantile_f64, reduce_f64, vmax_f64, vmax_i32, vmin_f64,
    vmin_i32,
};

const EVERY_STATISTIC: [Statistic; 14] = [
    Statistic::Mean,
    Statistic::Median,
    Statistic::Quartile1,
    Statistic::Quartile3,
    Statistic::Range,
    Statistic::Stdev,
    Statistic::Variance,
    Statistic::Mode,
    Statistic::PopulationVariance,
    Statistic::PopulationStdev,
    Statistic::Skewness,
    Statistic::Kurtosis,
    Statistic::PopulationSkewness,
    Statistic::PopulationKurtosis,
];

#[test]
fn float_reductions_match_the_bnmath_contract() {
    let values = [1.0, 2.0, 2.0, 5.0];
    assert!(
        matches!(reduce_f64(Statistic::Mean, &values), Reduction::Float(value) if value == 2.5)
    );
    assert!(
        matches!(reduce_f64(Statistic::Median, &values), Reduction::Float(value) if value == 2.0)
    );
    assert!(
        matches!(reduce_f64(Statistic::Quartile1, &values), Reduction::Float(value) if value == 1.5)
    );
    assert!(
        matches!(reduce_f64(Statistic::Quartile3, &values), Reduction::Float(value) if value == 3.5)
    );
    assert!(
        matches!(reduce_f64(Statistic::Mode, &values), Reduction::Float(value) if value == 2.0)
    );
}

#[test]
fn float_reductions_preserve_nan_and_mode_na_rules() {
    assert!(
        matches!(reduce_f64(Statistic::Mean, &[f64::NAN, 1.0]), Reduction::Float(value) if value.is_nan())
    );
    assert!(matches!(
        reduce_f64(Statistic::Mode, &[1.0, 2.0]),
        Reduction::Na
    ));
    assert!(matches!(reduce_f64(Statistic::Mode, &[]), Reduction::Na));
}

#[test]
fn min_max_reductions() {
    let ints = [5, -2, 10, 0];
    assert_eq!(vmin_i32(&ints), Some(-2));
    assert_eq!(vmax_i32(&ints), Some(10));
    assert_eq!(vmin_i32(&[]), None);
    assert_eq!(vmax_i32(&[]), None);

    let floats = [5.0, -2.5, 10.0, 0.0];
    assert_eq!(vmin_f64(&floats), Some(-2.5));
    assert_eq!(vmax_f64(&floats), Some(10.0));
    assert_eq!(vmin_f64(&[]), None);
    assert_eq!(vmax_f64(&[]), None);
}

#[test]
fn statistic_names_round_trip() {
    for statistic in EVERY_STATISTIC {
        assert_eq!(Statistic::from_name(statistic.name()), Some(statistic));
    }
    assert_eq!(Statistic::from_name("AVERAGE"), None);
}

/// MODE straight from math.md: count each value by exact FLOAT equality;
/// a NAN anywhere is NAN; empty or a tie for the highest count is NA.
fn mode_by_definition(values: &[f64]) -> Reduction {
    if values.iter().any(|value| value.is_nan()) {
        return Reduction::Float(f64::NAN);
    }
    let counts = values
        .iter()
        .map(|value| values.iter().filter(|other| *other == value).count())
        .collect::<Vec<_>>();
    let Some(&highest) = counts.iter().max() else {
        return Reduction::Na;
    };
    let mut winners = values
        .iter()
        .zip(&counts)
        .filter(|(_, count)| **count == highest)
        .map(|(value, _)| *value)
        .collect::<Vec<f64>>();
    winners.sort_by(f64::total_cmp);
    winners.dedup_by(|left, right| left == right);
    match winners.as_slice() {
        [unique] => Reduction::Float(*unique),
        _ => Reduction::Na,
    }
}

fn same(left: Reduction, right: Reduction) -> bool {
    match (left, right) {
        (Reduction::Na, Reduction::Na) => true,
        (Reduction::Float(left), Reduction::Float(right)) => {
            (left.is_nan() && right.is_nan()) || left == right
        }
        _ => false,
    }
}

/// Deterministic vectors with many repeats, ties and both zeros.
fn mode_samples() -> Vec<Vec<f64>> {
    const SAMPLE_COUNT: u64 = 400;
    const MAX_LENGTH: u64 = 12;
    const VALUE_SPAN: u64 = 5;
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = || {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    };
    let mut samples = vec![
        Vec::new(),
        vec![-0.0, 0.0, 1.0],
        vec![0.0, -0.0, 1.0, 1.0],
        vec![f64::NAN, 1.0, 1.0],
        vec![f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY],
    ];
    for _ in 0..SAMPLE_COUNT {
        let length = next() % MAX_LENGTH;
        #[allow(clippy::cast_precision_loss)] // values below VALUE_SPAN
        let sample = (0..length)
            .map(|_| (next() % VALUE_SPAN) as f64 - 2.0)
            .collect();
        samples.push(sample);
    }
    samples
}

#[test]
fn mode_matches_its_definition() {
    let wrong = mode_samples()
        .into_iter()
        .filter(|sample| {
            !same(
                reduce_f64(Statistic::Mode, sample),
                mode_by_definition(sample),
            )
        })
        .collect::<Vec<_>>();
    assert!(wrong.is_empty(), "MODE differs on {wrong:?}");
}

#[test]
fn every_reduction_of_an_empty_or_nan_vector_follows_math_md() {
    for statistic in EVERY_STATISTIC {
        let empty = reduce_f64(statistic, &[]);
        let expected_empty = if statistic == Statistic::Mode {
            Reduction::Na
        } else {
            Reduction::Float(f64::NAN)
        };
        assert!(same(empty, expected_empty), "{statistic:?} of empty");
        assert!(
            same(
                reduce_f64(statistic, &[1.0, f64::NAN]),
                Reduction::Float(f64::NAN)
            ),
            "{statistic:?} with NAN"
        );
    }
}

fn float_of(statistic: Statistic, values: &[f64]) -> f64 {
    match reduce_f64(statistic, values) {
        Reduction::Float(value) => value,
        Reduction::Na => panic!("{statistic:?} of {values:?} is NA"),
    }
}

/// math.md "Order, summation and overflow" (D-2, G-1): `MEAN` is a
/// Neumaier sum in index order over `n`; non-finite values are resolved
/// first, and an overflowing sum is `±INF`, never `NaN`.
#[test]
fn mean_is_a_compensated_sum_in_index_order() {
    const LARGE: f64 = 1e16;
    let max = f64::MAX;
    assert_eq!(float_of(Statistic::Mean, &[LARGE, 1.0, -LARGE, 1.0]), 0.5);
    assert_eq!(
        float_of(Statistic::Mean, &[f64::INFINITY, 1.0]),
        f64::INFINITY
    );
    assert_eq!(
        float_of(Statistic::Mean, &[f64::NEG_INFINITY, 1.0]),
        f64::NEG_INFINITY
    );
    assert!(float_of(Statistic::Mean, &[f64::INFINITY, 1.0, f64::NEG_INFINITY]).is_nan());
    assert_eq!(float_of(Statistic::Mean, &[max, max]), f64::INFINITY);
    assert_eq!(float_of(Statistic::Mean, &[-max, -max]), f64::NEG_INFINITY);
    // The compensation term of a plain Neumaier sum turns this into NaN.
    assert_eq!(float_of(Statistic::Mean, &[max, max, 1.0]), f64::INFINITY);
    assert_eq!(float_of(Statistic::Mean, &[max, max, -max]), f64::INFINITY);
    assert_eq!(float_of(Statistic::Mean, &[-max, max, max]), max / 3.0);
}

/// The corrected two-pass variance (G-11): an infinite value gives `NAN`, an
/// overflow gives `INF`, and the classic shifted data is exact.
#[test]
fn variance_is_a_corrected_two_pass_in_index_order() {
    let max = f64::MAX;
    assert!(float_of(Statistic::Variance, &[f64::INFINITY, 1.0]).is_nan());
    assert!(float_of(Statistic::Stdev, &[1.0, f64::NEG_INFINITY]).is_nan());
    assert_eq!(float_of(Statistic::Variance, &[max, -max]), f64::INFINITY);
    assert_eq!(float_of(Statistic::Variance, &[-max, max]), f64::INFINITY);
    assert_eq!(float_of(Statistic::Stdev, &[max, -max]), f64::INFINITY);
    // The mean of [MAX, MAX] overflows, so the variance is INF (G-1, the
    // derived rule decided (b) on 2026-10-26), though its true value is 0.
    assert_eq!(float_of(Statistic::Variance, &[max, max]), f64::INFINITY);
    let shifted = [1e9 + 4.0, 1e9 + 7.0, 1e9 + 13.0, 1e9 + 16.0];
    assert_eq!(float_of(Statistic::Variance, &shifted), 30.0);
    assert_eq!(float_of(Statistic::Range, &[3.0, -1.0, 10.0]), 11.0);
}

/// Any intermediate overflow makes the variance `INF`, as an overflowing
/// sum makes `MEAN` `INF` (Carlos, 2026-10-26, option (b)): the squares of
/// `[a, -a, a, -a]` with `a = 1e154` sum to `4e308`, so the result is `INF`
/// although the true variance `4a²/3 ≈ 1.33e308` is below `MAX`.
#[test]
fn an_overflowing_sum_of_squares_makes_the_variance_inf() {
    const LARGE: f64 = 1e154;
    let values = [LARGE, -LARGE, LARGE, -LARGE];
    assert_eq!(float_of(Statistic::Variance, &values), f64::INFINITY);
    assert_eq!(float_of(Statistic::Stdev, &values), f64::INFINITY);
}

/// `-0.0` is the additive identity, so a mean of zeros keeps the sign the
/// exact sum has, and `RANGE` orders zeros as the sort does.
#[test]
fn zeros_keep_their_sign_through_mean_and_range() {
    assert!(float_of(Statistic::Mean, &[-0.0]).is_sign_negative());
    assert!(float_of(Statistic::Mean, &[-0.0, -0.0]).is_sign_negative());
    assert!(float_of(Statistic::Mean, &[-0.0, 0.0]).is_sign_positive());
    assert!(float_of(Statistic::Range, &[0.0, -0.0]).is_sign_positive());
    assert!(float_of(Statistic::Range, &[-0.0, 0.0]).is_sign_positive());
}

/// NIST `StRD` univariate `NumAcc1`–`NumAcc4`
/// (`https://www.itl.nist.gov/div898/strd/univ/data/NumAcc{1..4}.dat`):
/// three values, then a centre followed by 500 (low, high) pairs. The
/// expected standard deviation is the exact one of the binary64 data,
/// computed with `bc` at 60 digits (G-11), so it tests the algorithm and
/// not the decimal-to-binary error of the data itself.
fn numacc(centre: f64, low: f64, high: f64) -> Vec<f64> {
    const PAIRS: usize = 500;
    let mut values = vec![centre];
    for _ in 0..PAIRS {
        values.push(low);
        values.push(high);
    }
    values
}

#[test]
#[allow(clippy::excessive_precision)] // the bc digits, rounded to binary64 by the parser
fn stdev_of_nist_numacc_is_the_exact_value_of_the_data() {
    let cases = [
        (
            "NumAcc1",
            vec![10_000_001.0, 10_000_003.0, 10_000_002.0],
            1.0,
        ),
        (
            "NumAcc2",
            numacc(1.2, 1.1, 1.3),
            0.099_999_999_999_999_977_795_539_507_496_869_2,
        ),
        (
            "NumAcc3",
            numacc(1_000_000.2, 1_000_000.1, 1_000_000.3),
            0.100_000_000_034_924_596_548_097_368_071_141_4,
        ),
        (
            "NumAcc4",
            numacc(10_000_000.2, 10_000_000.1, 10_000_000.3),
            0.100_000_000_558_793_544_773_619_585_564_518_6,
        ),
    ];
    for (name, values, exact) in cases {
        assert_eq!(float_of(Statistic::Stdev, &values), exact, "{name}");
    }
}

/// `PStdev` on NIST `NumAcc1`–`NumAcc4` is within one unit in the last place of the
/// exact population value of the binary64 data (`bc`, 60 digits): the same
/// kernel as `STDEV` with divisor `n` (math.md, G-5); the variance and its
/// `SQRT` round once each, so `NumAcc4` lands one unit above. `NumAcc1` is
/// `sqrt(2/3)`.
#[test]
#[allow(clippy::excessive_precision)] // the bc digits, rounded to binary64 by the parser
fn population_stdev_of_nist_numacc_is_within_one_ulp_of_the_data() {
    let cases = [
        (
            "NumAcc1",
            vec![10_000_001.0, 10_000_003.0, 10_000_002.0],
            0.816_496_580_927_726_032_732_428_024_901_963_797_3,
        ),
        (
            "NumAcc2",
            numacc(1.2, 1.1, 1.3),
            0.099_950_037_468_777_296_969_796_084_107_171_029,
        ),
        (
            "NumAcc3",
            numacc(1_000_000.2, 1_000_000.1, 1_000_000.3),
            0.099_950_037_503_684_466_498_805_295_713_760_44,
        ),
        (
            "NumAcc4",
            numacc(10_000_000.2, 10_000_000.1, 10_000_000.3),
            0.099_950_038_027_291_676_537_504_406_961_471_118,
        ),
    ];
    for (name, values, exact) in cases {
        let computed = float_of(Statistic::PopulationStdev, &values);
        assert!(
            computed.to_bits().abs_diff(f64::to_bits(exact)) <= 1,
            "{name}: {computed} against {exact}"
        );
    }
    assert_eq!(float_of(Statistic::PopulationVariance, &[7.0]), 0.0);
    assert!(float_of(Statistic::PopulationVariance, &[]).is_nan());
}

/// `Quantile` is R's type 7 to the bit (R 4.5.1): `(n - 1) p` may fall a
/// rounding below a whole number, `100 * 0.29 = 28.999999999999996`, and R
/// (no fuzz for type 7) then interpolates between `x[29] = 56` and
/// `x[30] = 58`, giving `57.999999999999993`, not `58`.
#[test]
fn quantile_is_r_type_7_to_the_bit() {
    let mut values = (1..=100)
        .map(|step| f64::from(step) * 2.0)
        .collect::<Vec<_>>();
    values.push(5.0);
    assert_eq!(quantile_f64(&values, 0.29), 57.999_999_999_999_99);
    assert_eq!(quantile_f64(&[7.0, 1.0, 6.0, 2.0, 5.0, 3.0, 4.0], 0.1), 1.6);
    assert_eq!(
        quantile_f64(&[1.0, 2.0, 3.0, 4.0], 0.7),
        3.099_999_999_999_999_6
    );
    assert!(quantile_f64(&[f64::NEG_INFINITY, f64::INFINITY], 0.5).is_nan());
    assert!(quantile_f64(&[1.0, 2.0], 1.5).is_nan());
    assert!(quantile_f64(&[], 0.5).is_nan());
    assert_eq!(quantile(&[10, 20, 30, 40, 50], 0.7), 38.0);
}
