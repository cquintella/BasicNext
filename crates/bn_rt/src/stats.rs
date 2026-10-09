// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::math::fail;

#[allow(unsafe_code)] // C ABI: INTEGER[] buffer from LLVM alloca.
pub fn i32_slice<'a>(ptr: *const i32, len: i32) -> &'a [i32] {
    if ptr.is_null() || len <= 0 {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(ptr, usize::try_from(len).unwrap_or(0)) }
}

pub fn vmin_i32(values: &[i32]) -> i32 {
    super::cpu::vmin_i32(values).unwrap_or_else(|| {
        fail(
            "INDEX_OUT_OF_BOUNDS",
            "BNMath reduction received an empty vector",
        )
    })
}

pub fn vmax_i32(values: &[i32]) -> i32 {
    super::cpu::vmax_i32(values).unwrap_or_else(|| {
        fail(
            "INDEX_OUT_OF_BOUNDS",
            "BNMath reduction received an empty vector",
        )
    })
}

pub use bn_core_math::{Reduction, reduce, reduce_f64};

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::{Reduction, reduce_f64};

    #[test]
    fn float_reductions_match_the_bnmath_contract() {
        let values = [1.0, 2.0, 2.0, 5.0];
        assert!(matches!(reduce_f64("MEAN", &values), Reduction::Float(value) if value == 2.5));
        assert!(matches!(reduce_f64("MEDIAN", &values), Reduction::Float(value) if value == 2.0));
        assert!(
            matches!(reduce_f64("QUARTILE1", &values), Reduction::Float(value) if value == 1.5)
        );
        assert!(
            matches!(reduce_f64("QUARTILE3", &values), Reduction::Float(value) if value == 3.5)
        );
        assert!(matches!(reduce_f64("MODE", &values), Reduction::Float(value) if value == 2.0));
    }

    #[test]
    fn float_reductions_preserve_nan_and_mode_na_rules() {
        assert!(
            matches!(reduce_f64("MEAN", &[f64::NAN, 1.0]), Reduction::Float(value) if value.is_nan())
        );
        assert!(matches!(reduce_f64("MODE", &[1.0, 2.0]), Reduction::Na));
        assert!(matches!(reduce_f64("MODE", &[]), Reduction::Na));
    }
}
