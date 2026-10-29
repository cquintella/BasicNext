// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The `Error` reports of the text built-ins `ASC` and `CHAR` (`error.md`):
//! their `Operation` and `Cause`, one rule for `bni` and compiled programs.

/// `Error.Operation` of a failed `ASC`.
#[must_use]
pub fn asc_operation() -> &'static str {
    bn_catalog::text("string", "builtin.asc.operation")
}

/// `Error.Operation` of a failed `CHAR`.
#[must_use]
pub fn char_operation() -> &'static str {
    bn_catalog::text("string", "builtin.char.operation")
}

/// `Error.Cause` of a failed text built-in named by `operation`; empty for
/// any other operation, whose producer supplies its own cause.
#[must_use]
pub fn failure_cause(operation: &str) -> &'static str {
    if operation == asc_operation() {
        bn_catalog::text("string", "builtin.asc.cause")
    } else if operation == char_operation() {
        bn_catalog::text("string", "builtin.char.cause")
    } else {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_builtin_has_a_cause_and_others_have_none() {
        assert!(!failure_cause(asc_operation()).is_empty());
        assert!(!failure_cause(char_operation()).is_empty());
        assert_ne!(
            failure_cause(asc_operation()),
            failure_cause(char_operation())
        );
        assert_eq!(failure_cause("BNCrypto.FromHex"), "");
    }
}
