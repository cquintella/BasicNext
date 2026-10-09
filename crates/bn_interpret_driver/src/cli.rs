// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The `bni` command line: help, command selection, option handling, and the
//! large-stack thread the interpreter runs on. `bni/src/main.rs` only names
//! its version and the protocol servers and calls [`main`] (bucket 0.6.5c
//! S4).

use std::process::ExitCode;

use bn_cli::{
    check::check,
    frontend::read_source,
    help::{BOOK_URL, COMMON_OPTIONS, Tool, foreign_option, option_error},
    options::parse_options,
    output::{emit_output, tokens_text, tool_error},
};

use crate::{eval::eval, run::run};

const USAGE: &str = "usage: bni <eval|check|lex|run|lsp|dap> [options] <file.bn>\ntry: bni --help";

/// The protocol servers `bni lsp` and `bni dap` start. They are passed in
/// because `bn_dap` depends on this crate.
#[derive(Clone, Copy, Debug)]
pub struct Protocols {
    pub lsp: fn() -> Result<(), String>,
    pub dap: fn() -> Result<(), String>,
}

fn help(version: &str) -> ExitCode {
    println!(
        "\
{version}
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

/// Runs `bni` with `arguments` (without the program name) on a thread with
/// [`COMMAND_STACK`]; `version` is the line `--version` prints.
#[must_use]
pub fn main(version: &'static str, arguments: Vec<String>, protocols: Protocols) -> ExitCode {
    std::thread::Builder::new()
        .name("bni".into())
        .stack_size(COMMAND_STACK)
        .spawn(move || command(version, arguments, protocols))
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

fn command(version: &str, mut raw_args: Vec<String>, protocols: Protocols) -> ExitCode {
    let Some(first) = raw_args.first().cloned() else {
        return bn_cli::help::usage(USAGE);
    };
    match first.as_str() {
        "-h" | "--help" => return help(version),
        "-V" | "--version" => {
            println!("{version}");
            return ExitCode::SUCCESS;
        }
        "eval" => return eval(raw_args[1..].to_vec(), &mut ()),
        "lsp" => return protocol("LSP", (protocols.lsp)()),
        "dap" => return protocol("DAP", (protocols.dap)()),
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
        Err(message) => return option_error(&message, USAGE),
    };
    // Frontend artifacts are a compiler surface (BDFL, 2026-10-07).
    if let Some(message) = foreign_option(Tool::Interpreter, &options) {
        eprintln!("error: {message}");
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
