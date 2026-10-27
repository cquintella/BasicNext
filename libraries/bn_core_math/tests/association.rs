// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNMath.Covariance` / `Correlation` (math.md): R 4.5.1 values, the `NAN`
//! cases, the length check, and the overflow rules decided on 2026-10-27.

#![allow(clippy::float_cmp)] // exact values are the contract

use bn_core_math::{LengthMismatch, correlation, covariance};

#[allow(clippy::excessive_precision)] // the bc digits, rounded to binary64 by the parser
const ONE_OVER_TWO_ROOT_TWO: f64 = 0.353_553_390_593_273_762_200_422_181_052_424_5;

/// Within one unit in the last place of `exact`.
fn near(computed: Result<f64, LengthMismatch>, exact: f64) -> bool {
    computed.is_ok_and(|value| value.to_bits().abs_diff(exact.to_bits()) <= 1)
}

/// Exact data, so `bc` gives the exact results: the covariances are the
/// correctly rounded values, and the correlations within one unit of them
/// (the mean 2.8 is not a binary64 value; R 4.5.1 is one unit off on the
/// first and exact on the second, the other way round).
#[test]
#[allow(clippy::excessive_precision)] // the bc digits, rounded to binary64 by the parser
fn covariance_and_correlation_match_the_exact_values() {
    let (x, y) = ([1.0, 2.0, 3.0, 4.0], [2.0, 4.0, 6.0, 9.0]);
    assert_eq!(covariance(&x, &y), Ok(3.833_333_333_333_333_5));
    // 11.5 / sqrt(5 * 26.75)
    assert!(near(
        correlation(&x, &y),
        0.994_376_712_684_368_912_567_846_631_859_4
    ));
    let (tens, digits) = ([10.0, 20.0, 30.0, 40.0, 50.0], [3.0, 1.0, 4.0, 1.0, 5.0]);
    assert_eq!(covariance(&tens, &digits), Ok(10.0));
    // 40 / sqrt(1000 * 12.8) = 1 / (2 sqrt 2)
    assert!(near(correlation(&tens, &digits), ONE_OVER_TWO_ROOT_TWO));
    assert_eq!(correlation(&[1.0, 2.0, 3.0], &[6.0, 4.0, 2.0]), Ok(-1.0));
}

#[test]
fn undefined_cases_give_nan_and_lengths_must_agree() {
    assert!(covariance(&[1.0], &[1.0]).is_ok_and(f64::is_nan));
    assert!(correlation(&[1.0, f64::NAN], &[1.0, 2.0]).is_ok_and(f64::is_nan));
    assert!(covariance(&[1.0, f64::INFINITY], &[1.0, 2.0]).is_ok_and(f64::is_nan));
    assert!(correlation(&[5.0, 5.0, 5.0], &[1.0, 2.0, 3.0]).is_ok_and(f64::is_nan));
    assert_eq!(
        covariance(&[1.0, 2.0], &[1.0, 2.0, 3.0]),
        Err(LengthMismatch)
    );
    assert_eq!(correlation(&[1.0, 2.0], &[1.0]), Err(LengthMismatch));
}

/// Overflow: the covariance keeps its true sign beyond `MAX` and its true
/// value below it; the correlation is always its true value.
#[test]
fn overflow_keeps_the_true_value_or_its_signed_infinity() {
    let max = f64::MAX;
    assert_eq!(covariance(&[max, -max], &[max, -max]), Ok(f64::INFINITY));
    assert_eq!(
        covariance(&[max, -max], &[-max, max]),
        Ok(f64::NEG_INFINITY)
    );
    assert_eq!(correlation(&[max, -max], &[max, -max]), Ok(1.0));
    assert_eq!(correlation(&[max, -max], &[-max, max]), Ok(-1.0));
    // The mean of [MAX, MAX] overflows, but the true covariance is 0.
    assert_eq!(covariance(&[max, max], &[1.0, 2.0]), Ok(0.0));
    // a = 1e154: the products sum past MAX, the covariance 4a²/3 does not.
    let large = 1e154;
    let values = [large, -large, large, -large];
    let expected = large * (4.0 * large / 3.0);
    let computed = covariance(&values, &values).expect("equal lengths");
    assert!(
        ((computed - expected) / expected).abs() < 1e-15,
        "{computed}"
    );
}
