// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNMath.Gcd` / `Lcm` (math.md, "Integer divisors"): exact on the whole
//! `INT64` range, the bounds included.

use bn_core_math::{gcd_magnitude, lcm_exact};

const TWO_TO_63: u64 = 1 << 63;

#[test]
fn gcd_is_exact_at_the_int64_bounds() {
    assert_eq!(gcd_magnitude(12, 18), 6);
    assert_eq!(gcd_magnitude(-12, 18), 6);
    assert_eq!(gcd_magnitude(0, 0), 0);
    assert_eq!(gcd_magnitude(0, -7), 7);
    assert_eq!(gcd_magnitude(i64::MIN, 0), TWO_TO_63);
    assert_eq!(gcd_magnitude(i64::MIN, i64::MIN), TWO_TO_63);
    assert_eq!(gcd_magnitude(i64::MIN, 2), 2);
    assert_eq!(gcd_magnitude(i64::MAX, i64::MAX), i64::MAX.unsigned_abs());
    // 2^63 and 2^63 - 1 are coprime.
    assert_eq!(gcd_magnitude(i64::MIN, i64::MAX), 1);
}

#[test]
fn lcm_is_exact_past_the_int64_bounds() {
    assert_eq!(lcm_exact(4, 6), 12);
    assert_eq!(lcm_exact(-4, 6), 12);
    assert_eq!(lcm_exact(0, 5), 0);
    assert_eq!(lcm_exact(0, 0), 0);
    assert_eq!(lcm_exact(i64::MAX, 1), i128::from(i64::MAX));
    assert_eq!(lcm_exact(i64::MIN, i64::MIN), i128::from(TWO_TO_63));
    // The largest result: 2^63 * (2^63 - 1), coprime operands.
    assert_eq!(
        lcm_exact(i64::MIN, i64::MAX),
        i128::from(TWO_TO_63) * i128::from(i64::MAX)
    );
}
