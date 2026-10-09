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
    frontend::{load_frontend, read_source},
    help::{BOOK_URL, COMMON_OPTIONS, Tool, foreign_option, option_error},
    options::parse_options,
    output::{emit_output, tool_error},
};

use crate::{build::build, options::BuildOptions};

const USAGE: &str = "usage: bnc [compile-options] <entry.bn>\ntry: bnc --help";

fn help(version: &str) -> ExitCode {
    println!(
        "\
{version}
usage: bnc [compile-options] <entry.bn>

Compiles the program to a native executable or a Wasm module.

compile options:
  -o, --output <file>        write the artifact to <file> (defaults to entry name)
  --target native|wasm32     select the target (default native)
  --opt none|1|2|3|s         optimization level (default 2)
  -g, --debug                emit debug information (DWARF; PDB on Windows)
  --cpu native|generic       target host CPU instructions (default generic)
  --emit llvm                print LLVM IR instead of compiling
  --emit ir                  print the validated BN IR instead of compiling
                             (tokens, ast and typed-ast are also accepted)
{COMMON_OPTIONS}
See also: man bnc
More information: {BOOK_URL}
"
    );
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
    let options = match parse_options(arguments.into_iter(), &mut build_options) {
        Ok(options) => options,
        Err(message) => return option_error(&message, USAGE),
    };
    if let Some(message) = foreign_option(Tool::Compiler, &options) {
        eprintln!("error: {message}");
        return tool_error();
    }
    let (source, tokens) = match read_source(&options) {
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
