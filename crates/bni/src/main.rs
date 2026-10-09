// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! `bni`: the Basic Next interpreter executable. Its command line lives in
//! `bn_interpret_driver::cli`; this file names the version and the protocol
//! servers (`bn_lsp`, `bn_dap`) and calls it.
use std::process::ExitCode;

use bn_interpret_driver::cli::{Protocols, main as bni};

const VERSION: &str = concat!("bni ", env!("CARGO_PKG_VERSION"));

fn main() -> ExitCode {
    let protocols = Protocols {
        lsp: bn_lsp::run_stdio,
        dap: bn_dap::run_stdio,
    };
    bni(VERSION, std::env::args().skip(1).collect(), protocols)
}
