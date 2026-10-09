// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Text and temporal core algorithms shared by the interpreter and native runtime.
//!
//! Holds:
//! - `parse_val`: string-to-float prefix parsing matching `VAL(...)` semantics.
//! - `is_timezone_id`: canonical IANA timezone identifier validation.
//! - Civil date/time conversions, RFC 3339 parsing and formatting.

pub mod civil;
pub mod temporal;

/// Parses a numeric prefix from `text`, matching Basic Next `VAL` semantics (R3).
/// Leading whitespace is trimmed. Trailing characters after the numeric prefix are ignored.
/// Returns `0.0` if no valid numeric prefix exists or if parsing fails.
#[must_use]
pub fn parse_val(text: &str) -> f64 {
    let text = text.trim_start();
    let bytes = text.as_bytes();
    let mut end = usize::from(bytes.first().is_some_and(|b| matches!(b, b'+' | b'-')));
    let digits = end;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
    }
    if end == digits || (end == digits + 1 && bytes.get(digits) == Some(&b'.')) {
        return 0.0;
    }
    text[..end].parse().unwrap_or(0.0)
}

/// A canonical IANA time-zone identifier as `TimeZone.Parse` accepts it (R4).
/// `UTC`, or two or more `/`-separated parts, each starting with a letter
/// and continuing with letters, digits, `_`, `-`, `+`.
#[must_use]
pub fn is_timezone_id(text: &str) -> bool {
    bn_types::text::is_timezone_id(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `VAL` results are exact parses, so equal bits are the right check.
    fn same(left: f64, right: f64) -> bool {
        left.to_bits() == right.to_bits()
    }

    #[test]
    fn parse_val_matches_spec() {
        assert!(same(parse_val("123"), 123.0));
        assert!(same(parse_val("  -45.67abc"), -45.67));
        assert!(same(parse_val("+0.5"), 0.5));
        assert!(same(parse_val("  +"), 0.0));
        assert!(same(parse_val("-.foo"), 0.0));
        assert!(same(parse_val("abc"), 0.0));
        assert!(same(parse_val(""), 0.0));
    }

    #[test]
    fn timezone_id_check() {
        assert!(is_timezone_id("UTC"));
        assert!(is_timezone_id("America/Sao_Paulo"));
        assert!(!is_timezone_id("InvalidZone"));
    }
}
