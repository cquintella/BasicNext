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
    help::{BOOK_URL, COMMON_OPTIONS},
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

Compiles the program to a native executable or a Wasm module.

compile options:
  -o, --output <file>        write the artifact to <file> (defaults to entry name)
  --target native|wasm32     select the target (default native)
  --opt none|1|2|3|s         optimization level (default 2)
  -g, --debug                emit debug information (DWARF; PDB on Windows)
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
    // Accepted by the shared parser, honored only by the interpreter.
    if options.trace {
        eprintln!("error: --trace is an interpreter option (bni run)");
        return tool_error();
    }
    if !options.filesystem {
        eprintln!("error: --no-filesystem is an interpreter option (bni run)");
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
