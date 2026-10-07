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
    help::{BOOK_URL, COMMON_OPTIONS},
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
  --mode snippet|program     select eval fragment mode (eval only)
  --format text|json         select eval result format (eval only)
  --jupyter-stdin            announce INPUT requests on stderr (run/eval)
  -o, --output <file>        write the token stream of lex to <file>
  --trace                    report the execution entry point
  --no-filesystem            deny HOST.FileSystem imports (run only)
{COMMON_OPTIONS}
 `bni eval` accepts SOURCE or --stdin; extra program arguments follow --.
 For file-oriented commands, HOST.Args[0] is the source path. Extra program arguments follow --.
 Compilation is `bnc [compile-options] <entry.bn>`.
See also: man bni
More information: {BOOK_URL}
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

/// Stack for the command thread. The tree-walking interpreter recurses once
/// per BN call; the main thread's default (1 MiB on Windows, 8 MiB on Unix)
/// made the recursion depth platform-dependent, and a debug build overflowed
/// on Windows at `Factorial(10)`.
const COMMAND_STACK: usize = 64 * 1024 * 1024;

fn main() -> ExitCode {
    std::thread::Builder::new()
        .name("bni".into())
        .stack_size(COMMAND_STACK)
        .spawn(command)
        .map_or_else(
            |error| {
                eprintln!("error: cannot start the interpreter thread: {error}");
                tool_error()
            },
            // A panic keeps its message and exit status, as on the main thread.
            |thread| {
                thread
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            },
        )
}

fn command() -> ExitCode {
    let mut raw_args = env::args().skip(1).collect::<Vec<_>>();
    let Some(first) = raw_args.first().cloned() else {
        return usage();
    };
    match first.as_str() {
        "-h" | "--help" => return help(),
        "-V" | "--version" => {
            println!("{VERSION}");
            return ExitCode::SUCCESS;
        }
        "eval" => return eval(raw_args[1..].to_vec(), &mut ()),
        "lsp" => return protocol("LSP", bn_lsp::run_stdio()),
        "dap" => return protocol("DAP", bn_dap::run_stdio()),
        _ => {}
    }
    let (subcommand, args) = match first.as_str() {
        "check" | "lex" | "run" => {
            raw_args.remove(0);
            (first, raw_args)
        }
        _ => ("run".to_string(), raw_args),
    };
    let options = match parse_options(args.into_iter(), &mut ()) {
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
    // Frontend artifacts are a compiler surface (BDFL, 2026-10-07).
    if options.emit.is_some() {
        eprintln!("error: --emit is a compiler option (bnc --emit tokens|ast|typed-ast|ir)");
        return tool_error();
    }
    let (source, tokens) = match read_source(&options) {
        Ok(read) => read,
        Err(code) => return code,
    };
    match subcommand.as_str() {
        "lex" => emit_output(tokens_text(&tokens), options.output.as_deref()),
        "check" => check(&source, &tokens, &options),
        _ => run(&source, &tokens, &options),
    }
}
