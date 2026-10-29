// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `Sum`, `Product` and `Dot` (math.md, "Algebraic reductions").

#![allow(clippy::float_cmp)] // exact totals are the contract

use bn_core_math::{LengthMismatch, dot, product, sum};

#[test]
fn sum_is_the_compensated_total() {
    assert_eq!(sum(&[]), 0.0);
    assert_eq!(sum(&[1.0, 2.0, 3.0]), 6.0);
    assert_eq!(sum(&[1e16, 1.0, -1e16, 1.0]), 2.0);
    assert_eq!(sum(&[f64::MAX, f64::MAX]), f64::INFINITY);
    assert_eq!(sum(&[f64::MAX, f64::MAX, -f64::MAX]), f64::INFINITY);
    assert!(sum(&[f64::NAN, 1.0]).is_nan());
    assert!(sum(&[f64::INFINITY, f64::NEG_INFINITY]).is_nan());
    assert_eq!(sum(&[-0.0]).to_bits(), (-0.0_f64).to_bits());
}

#[test]
fn product_does_not_depend_on_order() {
    assert_eq!(product(&[]), 1.0);
    assert_eq!(product(&[2.0, -3.0, 4.0]), -24.0);
    assert_eq!(product(&[1e308, 0.0, 1e308]), 0.0);
    assert_eq!(product(&[1e308, 1e308]), f64::INFINITY);
    assert_eq!(product(&[-2.0, 0.0]).to_bits(), (-0.0_f64).to_bits());
    assert!(product(&[0.0, f64::INFINITY]).is_nan());
    assert!(product(&[1.0, f64::NAN]).is_nan());
    // 1e308 * 1e10 * 1e-10 is finite. A plain product overflows on the first
    // pair and would turn the reversed vector into a different class.
    let ordered = [1e308, 1e10, -1e-10];
    let reversed = [-1e-10, 1e10, 1e308];
    let (forward, backward) = (product(&ordered), product(&reversed));
    assert!(
        forward.is_finite() && backward.is_finite(),
        "{forward} {backward}"
    );
    assert!(forward.is_sign_negative() && backward.is_sign_negative());
    assert!((forward / -1e308 - 1.0).abs() < 1e-9);
    assert!((backward / -1e308 - 1.0).abs() < 1e-9);
}

#[test]
fn dot_sums_the_products() {
    assert_eq!(dot(&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0]), Ok(32.0));
    assert_eq!(dot(&[], &[]), Ok(0.0));
    assert_eq!(dot(&[f64::MAX, f64::MAX], &[1.0, -1.0]), Ok(0.0));
    assert!(dot(&[1e308, 1e308], &[1e308, -1e308]).unwrap().is_nan());
    assert_eq!(dot(&[1.0], &[1.0, 2.0]), Err(LengthMismatch));
}

/// Factors past `i32::MAX / 997` powers of two: the exponent the product
/// carries must not wrap, so a long vector of large values is `INF` and of
/// small values is `0.0`.
const LONG_VECTOR_LENGTH: usize = 2_200_000;

#[test]
fn product_of_a_long_vector_keeps_its_class() {
    assert_eq!(product(&vec![1e300; LONG_VECTOR_LENGTH]), f64::INFINITY);
    assert_eq!(
        product(&vec![-1e300; LONG_VECTOR_LENGTH + 1]),
        f64::NEG_INFINITY
    );
    assert_eq!(product(&vec![1e-300; LONG_VECTOR_LENGTH]), 0.0);
    // The powers still cancel exactly across the whole vector.
    let mut balanced = vec![1e300; LONG_VECTOR_LENGTH];
    balanced.extend(std::iter::repeat_n(1e-300, LONG_VECTOR_LENGTH));
    let total = product(&balanced);
    assert!(total.is_finite() && total > 0.0, "{total}");
}

/// Subnormal factors and results: the split and the reassembly round once.
#[test]
fn product_crosses_the_subnormal_range_exactly() {
    let smallest = f64::from_bits(1);
    assert_eq!(product(&[smallest, 2.0]), f64::from_bits(2));
    assert_eq!(product(&[f64::MIN_POSITIVE, 0.5]), f64::MIN_POSITIVE / 2.0);
    assert_eq!(product(&[smallest, 0.5]), 0.0);
    assert_eq!(product(&[smallest, 0.75]), smallest);
    assert_eq!(product(&[1e-200, 1e-200]), 0.0);
    assert_eq!(product(&[f64::MAX, 0.5, 2.0]), f64::MAX);
    assert_eq!(product(&[f64::MAX, 2.0, 0.5]), f64::MAX);
}
