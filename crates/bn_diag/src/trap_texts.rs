// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Shared string literals for runtime traps, numeric conversions and language intrinsics (R7).
//!
//! One single source of truth used across `bn_interp`, `bn_llvm` and `bn_rt`.

/// "ASC requires a non-empty STRING"
pub const ASC_EMPTY_STRING: &str = "ASC requires a non-empty STRING";

/// "CHAR code is not a Unicode scalar"
pub const CHAR_NOT_UNICODE: &str = "CHAR code is not a Unicode scalar";

/// "integer exponent is too large"
pub const EXPONENT_TOO_LARGE: &str = "integer exponent is too large";

/// "integer exponent cannot be negative"
pub const EXPONENT_NEGATIVE: &str = "integer exponent cannot be negative";

/// "performing an integer operation"
pub const PERFORMING_INTEGER_OP: &str = "performing an integer operation";

/// "NAN and infinity cannot convert to an integer"
pub const NAN_OR_INF_TO_INT: &str = "NAN and infinity cannot convert to an integer";

/// "pointer element type does not match destination type"
pub const POINTER_TYPE_MISMATCH: &str = "pointer element type does not match destination type";

/// "pointer length does not fit INTEGER"
pub const POINTER_LENGTH_HOST: &str = "pointer length does not fit INTEGER";

/// "cannot index a NULL pointer"
pub const NULL_POINTER_INDEX: &str = "cannot index a NULL pointer";

/// "binding was already released"
pub const BINDING_ALREADY_RELEASED: &str = "binding was already released";

/// "converting a value to INTEGER"
pub const CONVERTING_TO_INTEGER: &str = "converting a value to INTEGER";

/// "HOST.Args": what an index outside `HOST.Args` names in its diagnostic.
pub const HOST_ARGS_CONTEXT: &str = "HOST.Args";

/// "FOR STEP cannot be zero"
pub const FOR_STEP_ZERO: &str = bn_catalog::frontend::MSG_FOR_STEP_CANNOT_BE_ZERO;
/// "FLOAT64 value does not fit FLOAT32"
pub const FLOAT64_TO_FLOAT32: &str = bn_catalog::frontend::MSG_FLOAT64_VALUE_DOES_NOT_FIT_FLOAT32;
/// "the two vectors of a `BNMath` statistic differ in length"
pub const STATISTIC_VECTORS_DIFFER: &str =
    bn_catalog::frontend::MSG_STATISTIC_VECTORS_DIFFER_IN_LENGTH;
/// "exit code must be in 0..255"
pub const EXIT_CODE_RANGE: &str = bn_catalog::frontend::MSG_EXIT_CODE_MUST_BE_IN_0_255;

#[cfg(test)]
mod tests {
    #[test]
    fn trap_texts_are_the_catalog_entries() {
        assert_eq!(
            super::FOR_STEP_ZERO,
            bn_catalog::text("frontend", "msg.for_step_cannot_be_zero")
        );
        assert_eq!(
            super::FLOAT64_TO_FLOAT32,
            bn_catalog::text("frontend", "msg.float64_value_does_not_fit_float32")
        );
        assert_eq!(
            super::STATISTIC_VECTORS_DIFFER,
            bn_catalog::text("frontend", "msg.statistic_vectors_differ_in_length")
        );
        assert_eq!(
            super::EXIT_CODE_RANGE,
            bn_catalog::text("frontend", "msg.exit_code_must_be_in_0_255")
        );
    }
}
