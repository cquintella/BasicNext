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
    *values.iter().min().unwrap_or_else(|| {
        fail(
            "INDEX_OUT_OF_BOUNDS",
            "BNMath reduction received an empty vector",
        )
    })
}

pub fn vmax_i32(values: &[i32]) -> i32 {
    *values.iter().max().unwrap_or_else(|| {
        fail(
            "INDEX_OUT_OF_BOUNDS",
            "BNMath reduction received an empty vector",
        )
    })
}

pub enum Reduction {
    Float(f64),
    Na,
}

#[allow(clippy::cast_precision_loss)]
pub fn reduce(name: &str, values: &[i32]) -> Reduction {
    let numbers = values
        .iter()
        .map(|value| f64::from(*value))
        .collect::<Vec<_>>();
    reduce_f64(name, &numbers)
}

#[allow(clippy::cast_precision_loss, clippy::float_cmp)]
pub fn reduce_f64(name: &str, values: &[f64]) -> Reduction {
    let mut numbers = values.to_vec();
    if numbers.iter().any(|value| value.is_nan()) {
        return Reduction::Float(f64::NAN);
    }
    if name == "MODE" && numbers.is_empty() {
        return Reduction::Na;
    }
    if numbers.is_empty() || (matches!(name, "STDEV" | "VARIANCE") && numbers.len() < 2) {
        return Reduction::Float(f64::NAN);
    }
    if matches!(name, "QUARTILE1" | "QUARTILE3") && numbers.len() < 2 {
        return Reduction::Float(f64::NAN);
    }
    numbers.sort_by(f64::total_cmp);
    let median = |xs: &[f64]| {
        if xs.len() % 2 == 1 {
            xs[xs.len() / 2]
        } else {
            xs[xs.len() / 2 - 1].midpoint(xs[xs.len() / 2])
        }
    };
    match name {
        "MEAN" => Reduction::Float(numbers.iter().sum::<f64>() / numbers.len() as f64),
        "MEDIAN" => Reduction::Float(median(&numbers)),
        "QUARTILE1" => Reduction::Float(median(&numbers[..numbers.len() / 2])),
        "QUARTILE3" => Reduction::Float(median(&numbers[numbers.len().div_ceil(2)..])),
        "RANGE" => Reduction::Float(numbers[numbers.len() - 1] - numbers[0]),
        "VARIANCE" | "STDEV" => {
            let mean = numbers.iter().sum::<f64>() / numbers.len() as f64;
            let variance = numbers
                .iter()
                .map(|value| (value - mean).powi(2))
                .sum::<f64>()
                / (numbers.len() - 1) as f64;
            Reduction::Float(if name == "STDEV" {
                variance.sqrt()
            } else {
                variance
            })
        }
        "MODE" => {
            let mut best = None;
            let mut count = 0;
            let mut tie = false;
            for &value in &numbers {
                let seen = numbers
                    .iter()
                    .filter(|candidate| **candidate == value)
                    .count();
                if seen > count {
                    best = Some(value);
                    count = seen;
                    tie = false;
                } else if seen == count && best != Some(value) {
                    tie = true;
                }
            }
            if tie {
                Reduction::Na
            } else {
                Reduction::Float(best.unwrap_or(f64::NAN))
            }
        }
        _ => Reduction::Float(f64::NAN),
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // Contract fixtures use exactly representable values.
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
