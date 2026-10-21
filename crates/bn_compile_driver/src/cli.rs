// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The `bnc` command line: help, option handling and the call into the
//! build. `bnc/src/main.rs` only names its version and calls [`main`]
//! (bucket 0.6.5c S4).

use std::process::ExitCode;

use bn_cli::{
    check::frontend_artifact,
    frontend::load_frontend,
    help::{BOOK_URL, COMMON_OPTIONS, option_error, tool_source},
    options::parse_options,
    output::emit_output,
};

use crate::{build::build, options::BuildOptions};

const USAGE: &str = "usage: bnc [compile-options] <entry.bn>\ntry: bnc --help";

/// The whole help, printed by `--help` and after an option error.
fn help_text(version: &str) -> String {
    bn_catalog::fill(
        "cli",
        "help.bnc",
        &[
            ("version", &version),
            ("common_options", &COMMON_OPTIONS),
            ("book_url", &BOOK_URL),
        ],
    )
}

fn help(version: &str) -> ExitCode {
    println!("{}", help_text(version));
    ExitCode::SUCCESS
}

/// Runs `bnc` with `arguments` (without the program name); `version` is the
/// line `--version` prints.
#[must_use]
pub fn main(version: &str, arguments: Vec<String>) -> ExitCode {
    match arguments.first().map(String::as_str) {
        None => return bn_cli::help::usage(USAGE),
        Some("-h" | "--help") => return help(version),
        Some("-V" | "--version") => {
            println!("{version}");
            return ExitCode::SUCCESS;
        }
        Some(_) => {}
    }
    let mut build_options = BuildOptions::default();
    let mut options = match parse_options(arguments.into_iter(), &mut build_options) {
        Ok(options) => options,
        Err(message) => return option_error(&message, &help_text(version)),
    };
    options.emit = build_options.emit;
    let (source, tokens) = match tool_source(&options) {
        Ok(read) => read,
        Err(code) => return code,
    };
    if let Some(emit) = options.emit
        && emit != bn_cli::options::Emit::Llvm
    {
        // Frontend artifacts only; nothing is compiled.
        let frontend = match load_frontend(&source, &options) {
            Ok(frontend) => frontend,
            Err(code) => return code,
        };
        return match frontend_artifact(&frontend, &tokens, emit) {
            Ok(text) => emit_output(text, options.output.as_deref()),
            Err(code) => code,
        };
    }
    build(&source, &options, build_options)
}
