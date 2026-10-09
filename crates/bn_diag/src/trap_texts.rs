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

/// "binding was already released"
pub const BINDING_ALREADY_RELEASED: &str = "binding was already released";

/// "converting a value to INTEGER"
pub const CONVERTING_TO_INTEGER: &str = "converting a value to INTEGER";
