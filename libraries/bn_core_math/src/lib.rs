// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Mathematical scalar and statistical reduction core.
//!
//! Shared by `bn_lib_math` (interpreter adapter) and `bn_rt` (native runtime C ABI).
//! Implements safe mathematical functions with exact parity across backends.

#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::string_slice,
    clippy::let_underscore_must_use,
    clippy::allow_attributes_without_reason,
    unreachable_pub,
    reason = "DEC-55 tier 3"
)]

pub mod algebra;
pub mod association;
pub(crate) mod binary64;
pub mod reduce;
pub(crate) mod scalar;
pub(crate) mod shape;
pub mod student;

pub use algebra::{dot, product, sum};
pub use association::{LengthMismatch, correlation, covariance};
pub use reduce::{
    Reduction, Statistic, quantile, quantile_f64, reduce, reduce_f64, vmax_f64, vmax_i32, vmin_f64,
    vmin_i32,
};
pub use scalar::*;
pub use student::{student_t_cdf, student_t_pvalue, student_t_quantile};
