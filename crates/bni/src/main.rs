// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! `bni`: the Basic Next interpreter executable — `run | eval | check | lex
//! | lsp | dap` — composed from `bn_cli`, `bn_interpret_driver`, `bn_lsp`
//! and `bn_dap`. No compiler, no provider implementation: argument
//! acquisition, command selection, library calls and exit status.
use std::{env, process::ExitCode};

use bn_cli::{
    check::check,
    frontend::read_source,
    help::COMMON_OPTIONS,
    options::{OutputFormat, parse_options},
    output::{emit_output, tokens_text, tool_error},
};
use bn_interpret_driver::{eval::eval, run::run};

const VERSION: &str = concat!("bni ", env!("CARGO_PKG_VERSION"));

fn help() -> ExitCode {
    println!(
        "\
{VERSION}
usage: bni <eval|check|lex|run|lsp|dap> [options] <file.bn> [-- program-args]

commands:
  eval    evaluate one source fragment (SOURCE or --stdin)
  check   validate lexer, parser, and semantics (the IDE Problems baseline)
  lex     print the token stream
  run     execute FUNCTION Start through typed BN IR
  lsp     serve Language Server Protocol over stdio
  dap     serve Debug Adapter Protocol over stdio

options:
  --mode snippet|program       select eval fragment mode (eval only)
  --format text|json           select eval result format (eval only)
  --jupyter-stdin              announce INPUT requests on stderr (run/eval)
{COMMON_OPTIONS}
 `bni eval` accepts SOURCE or --stdin; extra program arguments follow --.
 For file-oriented commands, HOST.Args[0] is the source path. Extra program arguments follow --.
 Compilation is `bnc [compile-options] <entry.bn>`.
See also: man bni
"
    );
    ExitCode::SUCCESS
}

fn usage() -> ExitCode {
    bn_cli::help::usage(
        "usage: bni <eval|check|lex|run|lsp|dap> [options] <file.bn>\ntry: bni --help",
    )
}

fn protocol(name: &str, result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error[{name}]: {message}");
            tool_error()
        }
    }
}

fn main() -> ExitCode {
    let mut arguments = env::args().skip(1);
    let Some(command) = arguments.next() else {
        return usage();
    };
    match command.as_str() {
        "-h" | "--help" => return help(),
        "-V" | "--version" => {
            println!("{VERSION}");
            return ExitCode::SUCCESS;
        }
        "eval" => return eval(arguments.collect(), &mut ()),
        "lsp" => return protocol("LSP", bn_lsp::run_stdio()),
        "dap" => return protocol("DAP", bn_dap::run_stdio()),
        "check" | "lex" | "run" => {}
        _ => return usage(),
    }
    let options = match parse_options(arguments, &mut ()) {
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
        eprintln!("error: --format is available only with bni eval");
        return tool_error();
    }
    let (source, tokens) = match read_source(&options) {
        Ok(read) => read,
        Err(code) => return code,
    };
    match command.as_str() {
        "lex" => emit_output(tokens_text(&tokens), options.output.as_deref()),
        "check" => check(&source, &tokens, &options),
        _ => run(&source, &tokens, &options),
    }
}
