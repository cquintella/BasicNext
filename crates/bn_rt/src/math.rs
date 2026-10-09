// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::process;

pub(crate) fn fail(code: &str, message: &str) -> ! {
    eprintln!("{}", format_failure(code, message));
    process::exit(1);
}

/// Ends the program with the diagnostic `bnc` rendered for the calling site
/// (`trap`, all facts known when compiling), as `bni` reports the failure;
/// `code` and `message` only serve a caller that passed no site.
pub(crate) fn fail_at(trap: *const std::ffi::c_char, code: &str, message: &str) -> ! {
    if trap.is_null() {
        fail(code, message);
    }
    super::trap_abi::report(trap, [0, 0]);
    process::exit(1);
}

fn format_failure(code: &str, message: &str) -> String {
    format!("error[{code}]: {message}")
}

pub fn parse_val(text: &str) -> f64 {
    bn_core_text::parse_val(text)
}

pub fn iabs(value: i64) -> i64 {
    bn_core_math::iabs(value).unwrap_or_else(|| fail("NUMERIC_OVERFLOW", "BNMath.ABS overflowed"))
}

pub fn isign(value: i64) -> i64 {
    bn_core_math::isign(value)
}

pub fn tohour(milliseconds: i64) -> i32 {
    bn_core_math::tohour(milliseconds)
}

pub fn toweekday(milliseconds: i64) -> i32 {
    bn_core_math::toweekday(milliseconds)
}

pub fn fsign(value: f64) -> f64 {
    bn_core_math::fsign(value)
}

pub fn fmin(left: f64, right: f64) -> f64 {
    bn_core_math::fmin(left, right)
}

pub fn fmax(left: f64, right: f64) -> f64 {
    bn_core_math::fmax(left, right)
}

pub fn round_ties_even(value: f64, digits: f64) -> f64 {
    bn_core_math::round_ties_even(value, digits)
}
