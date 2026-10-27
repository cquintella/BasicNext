// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `Skewness`, `Kurtosis`, `PSkewness`, `PKurtosis` (math.md; BDFL decision,
//! 2026-10-27: type 2 G1 and G2, type 1 g1 and g2). The data are exact, so
//! `bc` gives the exact values; R's e1071 1.7 agrees within a few units,
//! except that it cancels digits in the kurtosis (`-0.21200000000000019`
//! where the exact value is `-0.212`).

#![allow(clippy::float_cmp, clippy::excessive_precision)] // exact bc digits

use bn_core_math::{Reduction, Statistic, reduce, reduce_f64};

fn value(statistic: Statistic, values: &[f64]) -> f64 {
    match reduce_f64(statistic, values) {
        Reduction::Float(value) => value,
        Reduction::Na => panic!("{statistic:?} is NA"),
    }
}

/// Within `units` units in the last place of `exact`.
fn near(computed: f64, exact: f64, units: u64) -> bool {
    computed.to_bits().abs_diff(exact.to_bits()) <= units
}

const SPREAD: [f64; 8] = [2.0, 8.0, 0.0, 4.0, 1.0, 9.0, 9.0, 0.0];
const SKEWED: [f64; 5] = [1.0, 2.0, 3.0, 4.0, 10.0];

#[test]
fn shape_statistics_match_the_exact_values() {
    let cases = [
        (
            Statistic::Skewness,
            &SPREAD[..],
            0.330_582_180_407_974_662_184_960_597_344_640,
        ),
        (
            Statistic::Kurtosis,
            &SPREAD[..],
            -2.098_602_258_096_086_653_134_528_686_310_721,
        ),
        (
            Statistic::PopulationSkewness,
            &SPREAD[..],
            0.265_055_412_269_857_311_175_598_018_787_745,
        ),
        (
            Statistic::PopulationKurtosis,
            &SPREAD[..],
            -1.666_001_075_283_850_787_206_918_422_052_724,
        ),
        (
            Statistic::Skewness,
            &SKEWED[..],
            1.697_056_274_847_714_058_562_026_469_051_637,
        ),
        (
            Statistic::PopulationSkewness,
            &SKEWED[..],
            1.138_419_957_660_616_559_519_601_675_995_778,
        ),
    ];
    for (statistic, values, exact) in cases {
        let computed = value(statistic, values);
        assert!(
            near(computed, exact, 2),
            "{statistic:?}: {computed} against {exact}"
        );
    }
    // Exact results stay exact: (n S4 - 3 S2²) / S2² does not cancel.
    assert_eq!(value(Statistic::PopulationKurtosis, &SKEWED), -0.212);
    assert_eq!(value(Statistic::Kurtosis, &SKEWED), 3.152);
}

/// `NAN` below the size of each estimator, for a zero variance and for an
/// infinite value; an `INTEGER` vector is read as `FLOAT`; and values near
/// `MAX` give the shape of the same data at a small scale, since the shape
/// does not change with a positive scale (decided with `Correlation`).
#[test]
fn shape_statistics_cover_their_boundaries() {
    assert!(value(Statistic::Skewness, &[1.0, 2.0]).is_nan());
    assert!(value(Statistic::Kurtosis, &[1.0, 2.0, 3.0]).is_nan());
    assert!(value(Statistic::PopulationSkewness, &[]).is_nan());
    assert!(value(Statistic::PopulationKurtosis, &[5.0]).is_nan());
    assert!(value(Statistic::Skewness, &[4.0, 4.0, 4.0]).is_nan());
    assert!(value(Statistic::Kurtosis, &[1.0, 2.0, 3.0, f64::INFINITY]).is_nan());
    assert_eq!(value(Statistic::PopulationSkewness, &[1.0, 2.0, 3.0]), 0.0);
    let integers = match reduce(Statistic::PopulationKurtosis, &[1, 2, 3, 4, 10]) {
        Reduction::Float(value) => value,
        Reduction::Na => f64::NAN,
    };
    assert_eq!(integers, -0.212);
    let scale = 2f64.powi(1000);
    let huge = SKEWED.map(|value| value * scale);
    for statistic in [
        Statistic::Skewness,
        Statistic::Kurtosis,
        Statistic::PopulationSkewness,
        Statistic::PopulationKurtosis,
    ] {
        assert_eq!(
            value(statistic, &huge),
            value(statistic, &SKEWED),
            "{statistic:?}"
        );
    }
}
