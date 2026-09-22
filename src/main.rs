// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! `bn`: compatibility dispatcher for the 0.6 line (D-060-01). Routes
//! `run|eval|check|lex|lsp|dap` to the sibling `bni` and `build` to the
//! sibling `bnc`, translating the 0.5 argument shapes, forwarding stdio and
//! the exit status byte-for-byte. Retires in 0.7. No language library is
//! linked here.
use std::{
    env,
    io::IsTerminal,
    path::PathBuf,
    process::{Command, ExitCode},
};

const VERSION: &str = concat!("bn ", env!("CARGO_PKG_VERSION"));
const INTERPRETER_COMMANDS: [&str; 6] = ["run", "eval", "check", "lex", "lsp", "dap"];

fn help() -> ExitCode {
    println!(
        "\
{VERSION} (compatibility dispatcher; retires in 0.7)
usage: bn <eval|check|lex|run|build|lsp|dap> [options] <file.bn> [-- program-args]

  bn run|eval|check|lex|lsp|dap …   forwards to:  bni <command> …
  bn build [options] <file.bn>      forwards to:  bnc [options] <file.bn>

Use `bni --help` and `bnc --help` for the options of each executable.
See also: man bni, man bnc
"
    );
    ExitCode::SUCCESS
}

fn usage() -> ExitCode {
    eprintln!("usage: bn <eval|check|lex|run|build|lsp|dap> [options] <file.bn>\ntry: bn --help");
    ExitCode::from(2)
}

/// The executable next to this one, or the bare name for `PATH` lookup.
fn sibling(name: &str) -> PathBuf {
    let file = format!("{name}{}", env::consts::EXE_SUFFIX);
    env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(&file)))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from(file))
}

/// `bn run|eval|…` accepted the compiler flags `--target`/`--opt` and
/// ignored them; `bni` rejects them, so they are dropped here.
fn strip_compiler_flags(arguments: Vec<String>) -> Vec<String> {
    let mut kept = Vec::with_capacity(arguments.len());
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--" => {
                kept.push(argument);
                kept.extend(arguments);
                break;
            }
            "--target" | "--opt" => {
                let _ = arguments.next();
            }
            _ => kept.push(argument),
        }
    }
    kept
}

fn main() -> ExitCode {
    let mut arguments = env::args().skip(1);
    let Some(command) = arguments.next() else {
        return usage();
    };
    let rest = arguments.collect::<Vec<_>>();
    let (target, forwarded) = match command.as_str() {
        "-h" | "--help" => return help(),
        "-V" | "--version" => {
            println!("{VERSION}");
            return ExitCode::SUCCESS;
        }
        "build" => ("bnc", rest),
        name if INTERPRETER_COMMANDS.contains(&name) => {
            let mut forwarded = vec![command.clone()];
            forwarded.extend(strip_compiler_flags(rest));
            ("bni", forwarded)
        }
        _ => return usage(),
    };
    if std::io::stderr().is_terminal() {
        // Never on a pipe: `bn eval --format json` keeps both channels pure.
        eprintln!("bn: deprecated dispatcher, use `{target}` directly (bn retires in 0.7)");
    }
    match Command::new(sibling(target)).args(&forwarded).status() {
        Ok(status) => status.code().map_or(ExitCode::from(2), |code| {
            ExitCode::from(u8::try_from(code).unwrap_or(2))
        }),
        Err(error) => {
            eprintln!("error[BN_DISPATCH]: cannot execute {target}: {error}");
            ExitCode::from(2)
        }
    }
}
