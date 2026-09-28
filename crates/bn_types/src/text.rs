// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Canonical text of scalar values: what `PRINT` writes and what `AS STRING`
//! yields (console.md, "Stream macros"). The interpreter and the native
//! runtime both call these, so the two backends cannot drift.

use crate::FloatType;

/// `INTEGER` family: plain decimal with a leading `-` when negative.
#[must_use]
pub fn integer(value: i128) -> String {
    value.to_string()
}

/// `FLOAT` family: the shortest decimal that round-trips to `ty`, always
/// with a decimal point or exponent, and `NAN`, `INF`, `-INF` for special
/// values. A `FLOAT32` value is carried widened in `value`; narrowing it back
/// is exact.
#[must_use]
pub fn float(value: f64, ty: FloatType) -> String {
    if value.is_nan() {
        return "NAN".into();
    }
    if value == f64::INFINITY {
        return "INF".into();
    }
    if value == f64::NEG_INFINITY {
        return "-INF".into();
    }
    #[allow(clippy::cast_possible_truncation)] // exact: `value` holds an f32
    let mut text = match ty {
        FloatType::Float32 => (value as f32).to_string(),
        FloatType::Float64 => value.to_string(),
    };
    if !text.contains(['.', 'e', 'E']) {
        text.push_str(".0");
    }
    text
}

/// `PRINT` of an `Error` (language/0.6/error.md):
/// `Error <Code> in <Operation>: <Message> (cause: <Cause>)`. An empty
/// `Operation` or `Cause`, from a producer not yet migrated, is left out.
#[must_use]
pub fn error(code: i64, operation: &str, message: &str, cause: &str) -> String {
    let mut text = format!("Error {code}");
    if !operation.is_empty() {
        text.push_str(" in ");
        text.push_str(operation);
    }
    text.push_str(": ");
    text.push_str(message);
    if !cause.is_empty() {
        text.push_str(" (cause: ");
        text.push_str(cause);
        text.push(')');
    }
    text
}

/// `BOOLEAN`: `TRUE` or `FALSE`.
#[must_use]
pub const fn boolean(value: bool) -> &'static str {
    if value { "TRUE" } else { "FALSE" }
}

#[cfg(test)]
mod tests {
    use super::{boolean, error, float, integer};
    use crate::FloatType::{Float32, Float64};

    #[test]
    fn scalar_text_matches_the_print_contract() {
        assert_eq!(integer(-42), "-42");
        assert_eq!(float(2.0, Float64), "2.0");
        assert_eq!(float(0.1 + 0.2, Float64), "0.30000000000000004");
        assert_eq!(float(-2.5, Float64), "-2.5");
        assert_eq!(float(f64::NAN, Float64), "NAN");
        assert_eq!(float(f64::NEG_INFINITY, Float64), "-INF");
        assert_eq!(float(f64::from(0.1_f32), Float32), "0.1");
        assert_eq!(float(f64::from(16_777_216_f32), Float32), "16777216.0");
        assert_eq!(boolean(true), "TRUE");
        assert_eq!(
            error(
                2,
                "HOST.FileSystem.Open",
                "cannot open \"a\" for READ",
                "not found"
            ),
            "Error 2 in HOST.FileSystem.Open: cannot open \"a\" for READ (cause: not found)"
        );
        assert_eq!(error(1, "", "failed", ""), "Error 1: failed");
    }
}
