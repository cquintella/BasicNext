// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! C ABI for the text of scalar values: `PRINT` of floats and `AS STRING`.
//! The text is `bn_types::text`, shared with the interpreter.

use std::ffi::c_char;
use std::io::Write as _;

use super::{LibcStdout, c_string};

/// The text of the support stop for a STRING that would contain U+0000.
fn nul_message(operation: &str) -> String {
    format!(
        "{operation}: a STRING containing NUL (U+0000) is not supported by native builds yet; run the program with bni"
    )
}

/// Stops the program when `bytes`, about to become a native STRING, hold
/// U+0000. A native STRING is NUL-terminated, so the value would be cut
/// short: the backend never substitutes a different result (AGENTS.md;
/// bucket 0.6.5b S7.c, option C). `bni` keeps the NUL.
pub(crate) fn reject_nul(operation: &str, bytes: &[u8]) {
    if bytes.contains(&0) {
        super::math::fail("TARGET_UNSUPPORTED_TYPE", &nul_message(operation));
    }
}

/// `text` as an owned C string, or the support stop when it holds U+0000.
pub(crate) fn c_string_or_stop(operation: &str, text: String) -> std::ffi::CString {
    std::ffi::CString::new(text)
        .unwrap_or_else(|_| super::math::fail("TARGET_UNSUPPORTED_TYPE", &nul_message(operation)))
}

/// A `malloc`ed NUL-terminated copy of `value` for the emitted code to own,
/// or null when memory runs out; stops on U+0000 like [`reject_nul`].
#[allow(unsafe_code)] // malloc and a bounded copy into the new block.
pub(crate) fn owned_c_string(operation: &str, value: &[u8]) -> *mut c_char {
    reject_nul(operation, value);
    let Some(length) = value.len().checked_add(1) else {
        return std::ptr::null_mut();
    };
    // SAFETY: `length` is the byte count plus the terminator; the copy
    // writes `value.len()` bytes and the terminator into the new block.
    let pointer = unsafe { libc::malloc(length) }.cast::<u8>();
    if pointer.is_null() {
        return std::ptr::null_mut();
    }
    unsafe {
        std::ptr::copy_nonoverlapping(value.as_ptr(), pointer, value.len());
        pointer.add(value.len()).write(0);
    }
    pointer.cast()
}

/// `INPUT` read a NUL byte (called by the emitted `@bn_input`).
#[allow(unsafe_code)] // C ABI: the emitted INPUT reader stops here.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_input_nul() {
    reject_nul("INPUT", &[0]);
}

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

#[cfg(test)]
mod nul_tests {
    use super::{c_string_or_stop, nul_message, reject_nul};

    /// Text without U+0000 passes through; the stop itself ends the process
    /// and is covered by `nul_in_a_string_stops_native_builds_and_not_the_interpreter`
    /// (`tests/compiler_parity.rs`).
    #[test]
    fn text_without_nul_passes_and_the_message_names_the_operation() {
        reject_nul("CHAR", "abc".as_bytes());
        reject_nul("CHAR", &[]);
        assert_eq!(
            c_string_or_stop("HOST.Exec.Run", "ok".into()).as_bytes(),
            b"ok"
        );
        assert_eq!(
            nul_message("INPUT"),
            "INPUT: a STRING containing NUL (U+0000) is not supported by native builds yet; run the program with bni"
        );
    }
}
