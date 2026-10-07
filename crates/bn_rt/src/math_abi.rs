// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! C ABI exports for `BNMath` functions and vector reductions.

use std::ffi::c_char;

use super::{civil, math, stats};

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_iabs(value: i64) -> i64 {
    math::iabs(value)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_isign(value: i64) -> i64 {
    math::isign(value)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_imin(left: i64, right: i64) -> i64 {
    left.min(right)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_imax(left: i64, right: i64) -> i64 {
    left.max(right)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_tohour(milliseconds: i64) -> i32 {
    math::tohour(milliseconds)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_toweekday(milliseconds: i64) -> i32 {
    math::toweekday(milliseconds)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_val(text: *const c_char) -> f64 {
    if text.is_null() {
        return 0.0;
    }
    unsafe {
        std::ffi::CStr::from_ptr(text)
            .to_str()
            .map_or(0.0, math::parse_val)
    }
}

macro_rules! unary_f64 {
    ($export:ident, $body:expr) => {
        #[allow(unsafe_code)]
        #[unsafe(no_mangle)]
        pub extern "C" fn $export(value: f64) -> f64 {
            $body(value)
        }
    };
}

macro_rules! binary_f64 {
    ($export:ident, $body:expr) => {
        #[allow(unsafe_code)]
        #[unsafe(no_mangle)]
        pub extern "C" fn $export(left: f64, right: f64) -> f64 {
            $body(left, right)
        }
    };
}

unary_f64!(bn_rt_math_fabs, f64::abs);
unary_f64!(bn_rt_math_fsign, math::fsign);
unary_f64!(bn_rt_math_floor, f64::floor);
unary_f64!(bn_rt_math_ceil, f64::ceil);
unary_f64!(bn_rt_math_trunc, f64::trunc);
unary_f64!(bn_rt_math_exp, f64::exp);
unary_f64!(bn_rt_math_log, f64::ln);
unary_f64!(bn_rt_math_log10, f64::log10);
unary_f64!(bn_rt_math_log2, f64::log2);
unary_f64!(bn_rt_math_sin, f64::sin);
unary_f64!(bn_rt_math_cos, f64::cos);
unary_f64!(bn_rt_math_tan, f64::tan);
unary_f64!(bn_rt_math_asin, f64::asin);
unary_f64!(bn_rt_math_acos, f64::acos);
unary_f64!(bn_rt_math_atan, f64::atan);
unary_f64!(bn_rt_math_sqrt, f64::sqrt);

binary_f64!(bn_rt_math_pow, f64::powf);
binary_f64!(bn_rt_math_atan2, f64::atan2);
binary_f64!(bn_rt_math_hypot, f64::hypot);
binary_f64!(bn_rt_math_fmin, math::fmin);
binary_f64!(bn_rt_math_fmax, math::fmax);
binary_f64!(bn_rt_math_round, math::round_ties_even);

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_fma(x: f64, y: f64, z: f64) -> f64 {
    x.mul_add(y, z)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_vmin_i32(ptr: *const i32, len: i32, trap: *const c_char) -> i32 {
    empty_reduction(len, trap);
    stats::vmin_i32(stats::i32_slice(ptr, len))
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_vmin_f64(ptr: *const f64, len: i32, trap: *const c_char) -> f64 {
    empty_reduction(len, trap);
    float_slice(ptr, len)
        .iter()
        .copied()
        .reduce(|left, right| {
            if left.is_nan() || right.is_nan() {
                f64::NAN
            } else {
                left.min(right)
            }
        })
        .unwrap_or_else(|| {
            math::fail(
                "INDEX_OUT_OF_BOUNDS",
                "BNMath reduction received an empty vector",
            )
        })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_vmax_i32(ptr: *const i32, len: i32, trap: *const c_char) -> i32 {
    empty_reduction(len, trap);
    stats::vmax_i32(stats::i32_slice(ptr, len))
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_vmax_f64(ptr: *const f64, len: i32, trap: *const c_char) -> f64 {
    empty_reduction(len, trap);
    float_slice(ptr, len)
        .iter()
        .copied()
        .reduce(|left, right| {
            if left.is_nan() || right.is_nan() {
                f64::NAN
            } else {
                left.max(right)
            }
        })
        .unwrap_or_else(|| {
            math::fail(
                "INDEX_OUT_OF_BOUNDS",
                "BNMath reduction received an empty vector",
            )
        })
}

fn empty_reduction(len: i32, trap: *const c_char) {
    if len <= 0 {
        math::fail_at(
            trap,
            "INDEX_OUT_OF_BOUNDS",
            "BNMath reduction received an empty vector",
        );
    }
}

fn reduce_i32(name: &str, ptr: *const i32, len: i32) -> f64 {
    match stats::reduce(name, stats::i32_slice(ptr, len)) {
        stats::Reduction::Float(value) => value,
        stats::Reduction::Na => f64::NAN,
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_mean_i32(ptr: *const i32, len: i32) -> f64 {
    reduce_i32("MEAN", ptr, len)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_median_i32(ptr: *const i32, len: i32) -> f64 {
    reduce_i32("MEDIAN", ptr, len)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_quartile1_i32(ptr: *const i32, len: i32) -> f64 {
    reduce_i32("QUARTILE1", ptr, len)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_quartile3_i32(ptr: *const i32, len: i32) -> f64 {
    reduce_i32("QUARTILE3", ptr, len)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_range_i32(ptr: *const i32, len: i32) -> f64 {
    reduce_i32("RANGE", ptr, len)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_stdev_i32(ptr: *const i32, len: i32) -> f64 {
    reduce_i32("STDEV", ptr, len)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_variance_i32(ptr: *const i32, len: i32) -> f64 {
    reduce_i32("VARIANCE", ptr, len)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_mode_i32(ptr: *const i32, len: i32, out: *mut f64) -> i32 {
    match stats::reduce("MODE", stats::i32_slice(ptr, len)) {
        stats::Reduction::Na => 1,
        stats::Reduction::Float(value) => {
            if !out.is_null() {
                unsafe { out.write(value) };
            }
            0
        }
    }
}

fn reduce_f64(name: &str, ptr: *const f64, len: i32) -> stats::Reduction {
    stats::reduce_f64(name, float_slice(ptr, len))
}

#[allow(unsafe_code)]
fn float_slice<'a>(ptr: *const f64, len: i32) -> &'a [f64] {
    if ptr.is_null() || len <= 0 {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(ptr, usize::try_from(len).unwrap_or(0)) }
}

macro_rules! float_reduction {
    ($export:ident, $name:literal) => {
        #[allow(unsafe_code)]
        #[unsafe(no_mangle)]
        pub extern "C" fn $export(ptr: *const f64, len: i32) -> f64 {
            match reduce_f64($name, ptr, len) {
                stats::Reduction::Float(value) => value,
                stats::Reduction::Na => f64::NAN,
            }
        }
    };
}

float_reduction!(bn_rt_math_mean_f64, "MEAN");
float_reduction!(bn_rt_math_median_f64, "MEDIAN");
float_reduction!(bn_rt_math_quartile1_f64, "QUARTILE1");
float_reduction!(bn_rt_math_quartile3_f64, "QUARTILE3");
float_reduction!(bn_rt_math_range_f64, "RANGE");
float_reduction!(bn_rt_math_stdev_f64, "STDEV");
float_reduction!(bn_rt_math_variance_f64, "VARIANCE");

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_mode_f64(ptr: *const f64, len: i32, out: *mut f64) -> i32 {
    match reduce_f64("MODE", ptr, len) {
        stats::Reduction::Na => 1,
        stats::Reduction::Float(value) => {
            if !out.is_null() {
                unsafe { out.write(value) };
            }
            0
        }
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_todate(timestamp: i64, trap: *const c_char) -> i32 {
    civil::split_at(timestamp, trap).0
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_totime(timestamp: i64, trap: *const c_char) -> i32 {
    i32::try_from(civil::split_at(timestamp, trap).1).unwrap_or(0)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_totimestamp(days: i32, millis: i32) -> i64 {
    civil::totimestamp(days, millis)
}
