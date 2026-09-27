// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! `bnc`: the Basic Next compiler executable. Clang-like surface —
//! `bnc [compile-options] <entry.bn>` — over `bn_cli` (options, frontend,
//! diagnostics) and `bn_compile_driver` (build). No subcommands, no
//! interpreter: argument acquisition, one call, exit status.
use std::{env, process::ExitCode};

use bn_cli::{
    check::frontend_artifact,
    frontend::{load_frontend, read_source},
    help::COMMON_OPTIONS,
    options::{OutputFormat, parse_options},
    output::{emit_output, tool_error},
};
use bn_compile_driver::{build::build, options::BuildOptions};

const VERSION: &str = concat!("bnc ", env!("CARGO_PKG_VERSION"));

fn help() -> ExitCode {
    println!(
        "\
{VERSION}
usage: bnc [compile-options] <entry.bn>

Compiles the program to a native executable or a Wasm module. Without -o the
LLVM IR is written to standard output.

compile options:
  -o, --output <file>        write the artifact (or emitted IR) to <file>
  --target native|wasm32     select the target (default native)
  --opt none|1|2|3|s         optimization level (default 2)
  --emit ir                  print the validated BN IR instead of compiling
                             (tokens, ast and typed-ast are also accepted)
{COMMON_OPTIONS}
See also: man bnc
"
    );
    ExitCode::SUCCESS
}

fn usage() -> ExitCode {
    bn_cli::help::usage("usage: bnc [compile-options] <entry.bn>\ntry: bnc --help")
}

fn main() -> ExitCode {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    match arguments.first().map(String::as_str) {
        None => return usage(),
        Some("-h" | "--help") => return help(),
        Some("-V" | "--version") => {
            println!("{VERSION}");
            return ExitCode::SUCCESS;
        }
        Some(_) => {}
    }
    let mut build_options = BuildOptions::default();
    let options = match parse_options(arguments.into_iter(), &mut build_options) {
        Ok(options) => options,
        Err(message) => {
            if let Some(message) = message.strip_prefix("CONFIG_INVALID: ") {
                eprintln!("error[CONFIG_INVALID]: {message}");
            } else {
                eprintln!("error: {message}");
            }
            return usage();
        }
    };
    if options.output_format != OutputFormat::Text {
        eprintln!("error: --format json is an interpreter option (bni eval)");
        return tool_error();
    }
    let (source, tokens) = match read_source(&options) {
        Ok(read) => read,
        Err(code) => return code,
    };
    if let Some(emit) = options.emit {
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
