// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! C ABI for the text of scalar values: `PRINT` of floats and `AS STRING`.
//! The text is `bn_types::text`, shared with the interpreter.

use std::ffi::c_char;
use std::io::Write as _;

use super::{LibcStdout, c_string};

#[allow(unsafe_code)] // C ABI: PRINT FLOAT with interpreter formatting.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_print_float(value: f64) {
    let text = bn_types::text::float(value, bn_types::FloatType::Float64);
    let _ = LibcStdout.write_all(text.as_bytes());
}

#[allow(unsafe_code)] // C ABI: PRINT FLOAT32; `value` is the widened f32.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_print_float32(value: f64) {
    let text = bn_types::text::float(value, bn_types::FloatType::Float32);
    let _ = LibcStdout.write_all(text.as_bytes());
}

/// `AS STRING` (C3): the text `PRINT` writes, owned like other rt strings.
/// Narrower integers arrive sign- or zero-extended to 64 bits.
#[allow(unsafe_code)] // C ABI: owned UTF-8 STRING.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_text_int(value: i64) -> *mut c_char {
    c_string(&bn_types::text::integer(value.into()))
}

#[allow(unsafe_code)] // C ABI: owned UTF-8 STRING.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_text_uint(value: u64) -> *mut c_char {
    c_string(&bn_types::text::integer(value.into()))
}

#[allow(unsafe_code)] // C ABI: owned UTF-8 STRING.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_text_float(value: f64) -> *mut c_char {
    c_string(&bn_types::text::float(value, bn_types::FloatType::Float64))
}

#[allow(unsafe_code)] // C ABI: owned UTF-8 STRING; `value` is the widened f32.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_text_float32(value: f64) -> *mut c_char {
    c_string(&bn_types::text::float(value, bn_types::FloatType::Float32))
}
