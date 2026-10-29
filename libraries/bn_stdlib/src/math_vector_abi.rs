// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Native `BNMath` statistics over a vector or region of any integer or
//! floating element type (math.md, "Descriptive statistics"). `bnc` passes
//! the element and statistic codes of `bn_types::numeric_abi`; the elements
//! are read at their own width and the one rule of `bn_core_math` applies.

use std::ffi::c_char;

use bn_core_math::{Reduction, Statistic};
use bn_types::numeric_abi::{NumericElement, statistic_name};
use bn_types::{FloatType, IntegerType};

use super::math_abi::empty_reduction;

/// The elements of a numeric vector: integers exactly, floats widened.
enum Elements {
    Integers(Vec<i128>),
    Floats(Vec<f64>),
}

impl Elements {
    /// Integers become `FLOAT` element by element (math.md).
    #[allow(clippy::cast_precision_loss)] // the specified conversion to FLOAT
    fn into_floats(self) -> Vec<f64> {
        match self {
            Self::Integers(values) => values.into_iter().map(|value| value as f64).collect(),
            Self::Floats(values) => values,
        }
    }
}

/// Reads `len` elements of the element type coded `element` at `ptr`.
#[allow(unsafe_code)]
fn read_elements(ptr: *const u8, len: i32, element: i32) -> Elements {
    let element = NumericElement::from_code(element).expect("bnc passes a numeric element code");
    let count = if ptr.is_null() {
        0
    } else {
        usize::try_from(len).unwrap_or(0)
    };
    macro_rules! values {
        ($element:ty) => {
            (0..count).map(|index| {
                // SAFETY: bnc passes the data pointer and the length of a vector or region whose
                // `count` elements have the type `element` names, stored contiguously at their
                // natural width; `index < count` keeps the read inside that buffer, which the
                // caller keeps alive for this call, and `read_unaligned` needs no alignment.
                unsafe {
                    ptr.add(index * std::mem::size_of::<$element>())
                        .cast::<$element>()
                        .read_unaligned()
                }
            })
        };
    }
    macro_rules! integers {
        ($element:ty) => {
            Elements::Integers(values!($element).map(i128::from).collect())
        };
    }
    match element {
        NumericElement::Integer(IntegerType::Byte) => integers!(u8),
        NumericElement::Integer(IntegerType::Int8) => integers!(i8),
        NumericElement::Integer(IntegerType::Int16) => integers!(i16),
        NumericElement::Integer(IntegerType::Int32) => integers!(i32),
        NumericElement::Integer(IntegerType::Int64) => integers!(i64),
        NumericElement::Integer(IntegerType::UInt16) => integers!(u16),
        NumericElement::Integer(IntegerType::UInt32) => integers!(u32),
        NumericElement::Integer(IntegerType::UInt64) => integers!(u64),
        NumericElement::Float(FloatType::Float32) => {
            Elements::Floats(values!(f32).map(f64::from).collect())
        }
        NumericElement::Float(FloatType::Float64) => Elements::Floats(values!(f64).collect()),
    }
}

fn statistic(code: i32) -> Statistic {
    statistic_name(code)
        .and_then(Statistic::from_name)
        .expect("bnc passes a statistic code")
}

/// `MEAN`, `MEDIAN`, `QUARTILE1`, `QUARTILE3`, `RANGE`, `STDEV` or
/// `VARIANCE` of the vector; `NA` (only `MODE` has it) reads as `NAN`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_vector_statistic(
    ptr: *const u8,
    len: i32,
    element: i32,
    statistic_code: i32,
) -> f64 {
    let values = read_elements(ptr, len, element).into_floats();
    match bn_core_math::reduce_f64(statistic(statistic_code), &values) {
        Reduction::Float(value) => value,
        Reduction::Na => f64::NAN,
    }
}

/// `MODE` of the vector: writes the value to `out` and returns 0, or
/// returns 1 for `NA`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_vector_mode(
    ptr: *const u8,
    len: i32,
    element: i32,
    out: *mut f64,
) -> i32 {
    let values = read_elements(ptr, len, element).into_floats();
    match bn_core_math::reduce_f64(Statistic::Mode, &values) {
        Reduction::Na => 1,
        Reduction::Float(value) => {
            if !out.is_null() {
                // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
                unsafe { out.write(value) };
            }
            0
        }
    }
}

/// The integer element `pick` selects, as the 64 bits of its type (`bnc`
/// narrows them back to the element width; `UINT64` keeps its bits).
#[allow(clippy::cast_possible_truncation)] // every element fits 64 bits
fn integer_extreme(
    ptr: *const u8,
    len: i32,
    element: i32,
    pick: fn(Vec<i128>) -> Option<i128>,
) -> i64 {
    let Elements::Integers(values) = read_elements(ptr, len, element) else {
        panic!("bnc passes an integer element code to an integer extreme");
    };
    pick(values).expect("an empty vector was rejected") as i64
}

/// The floating element `pick` selects, with the `BNMath` `NAN` rule.
fn float_extreme(ptr: *const u8, len: i32, element: i32, pick: fn(&[f64]) -> Option<f64>) -> f64 {
    pick(&read_elements(ptr, len, element).into_floats()).expect("an empty vector was rejected")
}

fn least(values: Vec<i128>) -> Option<i128> {
    values.into_iter().min()
}

fn greatest(values: Vec<i128>) -> Option<i128> {
    values.into_iter().max()
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_vector_min_integer(
    ptr: *const u8,
    len: i32,
    element: i32,
    trap: *const c_char,
) -> i64 {
    empty_reduction(len, trap);
    integer_extreme(ptr, len, element, least)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_vector_max_integer(
    ptr: *const u8,
    len: i32,
    element: i32,
    trap: *const c_char,
) -> i64 {
    empty_reduction(len, trap);
    integer_extreme(ptr, len, element, greatest)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_vector_min_float(
    ptr: *const u8,
    len: i32,
    element: i32,
    trap: *const c_char,
) -> f64 {
    empty_reduction(len, trap);
    float_extreme(ptr, len, element, bn_core_math::vmin_f64)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_vector_max_float(
    ptr: *const u8,
    len: i32,
    element: i32,
    trap: *const c_char,
) -> f64 {
    empty_reduction(len, trap);
    float_extreme(ptr, len, element, bn_core_math::vmax_f64)
}

/// `Sum` of the vector (math.md, "Algebraic reductions").
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_vector_sum(ptr: *const u8, len: i32, element: i32) -> f64 {
    bn_core_math::sum(&read_elements(ptr, len, element).into_floats())
}

/// `Product` of the vector (math.md, "Algebraic reductions").
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_vector_product(ptr: *const u8, len: i32, element: i32) -> f64 {
    bn_core_math::product(&read_elements(ptr, len, element).into_floats())
}

/// `Quantile` of the vector at `p`, R type 7 (math.md); the elements are
/// read at their own width and widened to `FLOAT`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_vector_quantile(
    ptr: *const u8,
    len: i32,
    element: i32,
    probability: f64,
) -> f64 {
    let values = read_elements(ptr, len, element).into_floats();
    bn_core_math::quantile_f64(&values, probability)
}

/// `Covariance` or `Correlation` of two vectors of any numeric element
/// types (math.md); different lengths print the call site's
/// `VECTOR_LENGTH_MISMATCH` through `trap` and stop.
fn association(
    statistic: fn(&[f64], &[f64]) -> Result<f64, bn_core_math::LengthMismatch>,
    x: (*const u8, i32, i32),
    y: (*const u8, i32, i32),
    trap: *const c_char,
) -> f64 {
    let x = read_elements(x.0, x.1, x.2).into_floats();
    let y = read_elements(y.0, y.1, y.2).into_floats();
    statistic(&x, &y).unwrap_or_else(|bn_core_math::LengthMismatch| {
        super::math::fail_at(
            trap,
            VECTOR_LENGTH_MISMATCH,
            bn_catalog::frontend::MSG_STATISTIC_VECTORS_DIFFER_IN_LENGTH,
        )
    })
}

/// The diagnostic code of two statistic vectors of different lengths.
const VECTOR_LENGTH_MISMATCH: &str = "VECTOR_LENGTH_MISMATCH";

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_vector_covariance(
    x_ptr: *const u8,
    x_len: i32,
    x_element: i32,
    y_ptr: *const u8,
    y_len: i32,
    y_element: i32,
    trap: *const c_char,
) -> f64 {
    association(
        bn_core_math::covariance,
        (x_ptr, x_len, x_element),
        (y_ptr, y_len, y_element),
        trap,
    )
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_vector_correlation(
    x_ptr: *const u8,
    x_len: i32,
    x_element: i32,
    y_ptr: *const u8,
    y_len: i32,
    y_element: i32,
    trap: *const c_char,
) -> f64 {
    association(
        bn_core_math::correlation,
        (x_ptr, x_len, x_element),
        (y_ptr, y_len, y_element),
        trap,
    )
}

/// `Dot` of two vectors (math.md, "Algebraic reductions"); different
/// lengths print the call site's `VECTOR_LENGTH_MISMATCH` and stop.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_vector_dot(
    x_ptr: *const u8,
    x_len: i32,
    x_element: i32,
    y_ptr: *const u8,
    y_len: i32,
    y_element: i32,
    trap: *const c_char,
) -> f64 {
    association(
        bn_core_math::dot,
        (x_ptr, x_len, x_element),
        (y_ptr, y_len, y_element),
        trap,
    )
}
