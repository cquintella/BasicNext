#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::string_slice,
    clippy::let_underscore_must_use,
    clippy::allow_attributes_without_reason,
    unreachable_pub,
    reason = "DEC-55 tier 3"
)]
// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Empty library: the workspace root package exists to host the
//! cross-crate integration tests under `tests/` (parity, IR, codegen, …).
//! The executables are `crates/bni` and `crates/bnc`; the 0.5 `bn` command
//! was removed in 0.6.0.
