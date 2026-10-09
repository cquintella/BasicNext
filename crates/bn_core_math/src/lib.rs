// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Mathematical scalar and statistical reduction core (R5).
//!
//! Shared by `bn_lib_math` (interpreter adapter) and `bn_rt` (native runtime C ABI).
//! Implements safe mathematical functions with exact parity across backends.

pub mod reduce;
pub mod scalar;

pub use reduce::{Reduction, reduce, reduce_f64, vmax_f64, vmax_i32, vmin_f64, vmin_i32};
pub use scalar::*;
