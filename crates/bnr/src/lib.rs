// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `bnr`: Basic Next interactive Read-Eval-Print Loop library and drivers.

#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "REPL executable interacts directly with user terminal via stdout and stderr"
)]
#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::string_slice,
    clippy::let_underscore_must_use,
    clippy::allow_attributes_without_reason,
    unreachable_pub,
    reason = "DEC-55 tier 3"
)]

pub mod cli;
pub mod commands;
pub mod format;
pub mod history;
pub mod repl;
pub mod session;
