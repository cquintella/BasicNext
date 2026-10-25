// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Command-line parsing and entry point for `bnr`.

use std::process::ExitCode;

use bn_cli::{
    help::{BOOK_URL, COMMON_OPTIONS, option_error},
    options::parse_options,
};
use bn_interpret_driver::options::InterpreterOptions;

use crate::repl::run_repl;

/// Returns the whole help text, formatted from catalog.
fn help_text(version: &str) -> String {
    bn_catalog::fill(
        "cli",
        "help.bnr",
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

/// Runs `bnr` with `arguments` (without the program name); `version` is the
/// line `--version` prints.
#[must_use]
pub fn main(version: &str, arguments: Vec<String>) -> ExitCode {
    match arguments.first().map(String::as_str) {
        Some("-h" | "--help") => return help(version),
        Some("-V" | "--version") => {
            println!("{version}");
            return ExitCode::SUCCESS;
        }
        _ => {}
    }
    let mut interpreter_options = InterpreterOptions::default();
    let options = match parse_options(arguments.clone().into_iter(), &mut interpreter_options) {
        Ok(options) => options,
        Err(message) if message.starts_with("missing") => {
            let mut fallback_args = arguments;
            let separator = fallback_args.iter().position(|a| a == "--");
            if let Some(idx) = separator {
                fallback_args.insert(idx, "<repl>".to_string());
            } else {
                fallback_args.push("<repl>".to_string());
            }
            let mut fallback_interp = InterpreterOptions::default();
            match parse_options(fallback_args.into_iter(), &mut fallback_interp) {
                Ok(mut opts) => {
                    interpreter_options = fallback_interp;
                    opts.path.clear();
                    opts
                }
                Err(msg) => return option_error(&msg, &help_text(version)),
            }
        }
        Err(message) => return option_error(&message, &help_text(version)),
    };
    run_repl(&options, &interpreter_options, version)
}
