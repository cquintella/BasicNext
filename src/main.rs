// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! `bn`: the combined driver (eval | check | lex | run | build | lsp | dap).
//! Argument acquisition, command selection and exit status only; every
//! command is implemented by `bn_cli`, `bn_interpret_driver`,
//! `bn_compile_driver`, `bn_lsp` or `bn_dap` (bucket 0.6.0, SPRINT 1).
use std::{env, process::ExitCode};

use bn_cli::{
    check::check,
    frontend::read_source,
    help::COMMON_OPTIONS,
    options::{OutputFormat, parse_options},
    output::{emit_output, tokens_text, tool_error},
};
use bn_compile_driver::{build::build, options::BuildOptions};
use bn_interpret_driver::{eval::eval, run::run};
const VERSION: &str = concat!("bn ", env!("CARGO_PKG_VERSION"));

fn help() -> ExitCode {
    println!(
        "\
{VERSION}
usage: bn <eval|check|lex|run|build|lsp|dap> [options] <file.bn> [-- program-args]

commands:
  eval    evaluate one source fragment (SOURCE or --stdin) through the interpreter
  check   validate lexer, parser, and semantics
  lex     print the token stream
  run     execute FUNCTION Start through typed BN IR
  build   compile the supported typed BN IR subset with LLVM
  lsp     serve Language Server Protocol over stdio
  dap     serve Debug Adapter Protocol over stdio

options:
  --mode snippet|program       select eval fragment mode (eval only)
  --format text|json           select eval result format (eval only)
  --target native|wasm32     select the build target (build only)
  --opt none|1|2|3|s         optimization level for native/Wasm builds (default 2)
{COMMON_OPTIONS}
 `bn eval` accepts SOURCE or --stdin; extra program arguments follow --.
 For file-oriented commands, HOST.Args[0] is the source path. Extra program arguments follow --.
See also: man bn
"
    );
    ExitCode::SUCCESS
}

fn usage() -> ExitCode {
    bn_cli::help::usage(
        "usage: bn <eval|check|lex|run|build|lsp|dap> [options] <file.bn>\ntry: bn --help",
    )
}

fn main() -> ExitCode {
    let mut arguments = env::args().skip(1);
    let Some(command) = arguments.next() else {
        return usage();
    };
    if command == "eval" {
        return eval(arguments.collect(), &mut BuildOptions::default());
    }
    match command.as_str() {
        "-h" | "--help" => return help(),
        "-V" | "--version" => {
            println!("{VERSION}");
            return ExitCode::SUCCESS;
        }
        _ => {} // if there is any extra arg, do nothing
    }

    if command == "lsp" {
        // Language Server Protocol
        return match bn_lsp::run_stdio() {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("error[LSP]: {message}");
                tool_error()
            }
        };
    }

    if command == "dap" {
        // Debug Adapter Protocol
        return match bn_dap::run_stdio() {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("error[DAP]: {message}");
                tool_error()
            }
        };
    }
    let mut build_options = BuildOptions::default();
    let options = match parse_options(arguments, &mut build_options) {
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
        eprintln!("error: --format is available only with bn eval");
        return tool_error();
    }

    let (source, tokens) = match read_source(&options) {
        Ok(read) => read,
        Err(code) => return code,
    };
    match command.as_str() {
        "lex" => emit_output(tokens_text(&tokens), options.output.as_deref()),
        "check" => check(&source, &tokens, &options),
        "run" => run(&source, &tokens, &options),
        "build" => build(&source, &options, build_options),
        _ => usage(),
    }
}
