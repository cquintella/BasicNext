// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `bnr`: the Basic Next interactive REPL executable.

#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::string_slice,
    clippy::let_underscore_must_use,
    clippy::allow_attributes_without_reason,
    unreachable_pub,
    reason = "DEC-55 tier 3"
)]

use std::process::ExitCode;

const VERSION: &str = concat!("bnr ", env!("CARGO_PKG_VERSION"));

fn main() -> ExitCode {
    bnr::cli::main(VERSION, std::env::args().skip(1).collect())
}
