// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! REPL meta-commands implementation (`:help`, `:quit`, `:reset`, `:bindings`, `:type`, `:load`, `:policy`, `:clear`).

use std::fs;

use bn_cli::options::Options;
use bn_interpret_driver::options::InterpreterOptions;

use crate::session::{ExecutionOutcome, ReplSession};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandAction {
    Continue,
    Quit,
}

#[must_use]
pub fn is_meta_command(line: &str) -> bool {
    line.trim_start().starts_with(':')
}

#[allow(clippy::too_many_lines)]
pub fn handle_meta_command(
    line: &str,
    session: &mut ReplSession,
    options: &Options,
    interpreter: &InterpreterOptions,
) -> CommandAction {
    let trimmed = line.trim();
    let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
    let command = parts[0];
    let arg = parts.get(1).map_or("", |s| s.trim());

    match command {
        ":help" => {
            println!("{}", bn_catalog::text("cli", "msg.bnr_help_text"));
            CommandAction::Continue
        }
        ":quit" | ":q" => CommandAction::Quit,
        ":reset" => {
            if let Err(err) = session.reset(options, interpreter) {
                eprintln!("{err}");
            } else {
                println!("{}", bn_catalog::text("cli", "msg.bnr_reset_done"));
            }
            CommandAction::Continue
        }
        ":bindings" | ":vars" => {
            let bindings = session.bindings();
            if bindings.is_empty() {
                println!("{}", bn_catalog::text("cli", "msg.bnr_empty_bindings"));
            } else {
                println!("{}", bn_catalog::text("cli", "msg.bnr_active_bindings"));
                for b in bindings {
                    println!(
                        "{}",
                        bn_catalog::fill(
                            "cli",
                            "msg.bnr_binding_item",
                            &[
                                ("name", &b.name),
                                ("type", &b.type_name),
                                ("status", &b.status)
                            ]
                        )
                    );
                }
            }
            CommandAction::Continue
        }
        ":type" => {
            if arg.is_empty() {
                eprintln!("{}", bn_catalog::text("cli", "msg.bnr_type_missing_expr"));
            } else {
                match session.type_of(arg, options) {
                    Ok(ty) => println!("{ty}"),
                    Err(diag) => {
                        if !diag.is_empty() {
                            eprint!("{diag}");
                        }
                    }
                }
            }
            CommandAction::Continue
        }
        ":policy" => {
            println!("{}", bn_catalog::text("cli", "msg.bnr_active_policy"));
            println!(
                "  {}: {}",
                bn_catalog::text("cli", "msg.bnr_sandbox"),
                options.sandbox
            );
            println!(
                "  {}: {}",
                bn_catalog::text("cli", "msg.bnr_filesystem"),
                interpreter.filesystem
            );
            println!(
                "  {}: {:?}",
                bn_catalog::text("cli", "msg.bnr_read_roots"),
                options.read_roots
            );
            println!(
                "  {}: {:?}",
                bn_catalog::text("cli", "msg.bnr_write_roots"),
                options.write_roots
            );
            let paths: Vec<String> = options
                .module_paths
                .iter()
                .map(|p| p.path.display().to_string())
                .collect();
            println!(
                "  {}: {:?}",
                bn_catalog::text("cli", "msg.bnr_module_paths"),
                paths
            );
            let ffi_paths: Vec<String> = interpreter
                .ffi_paths
                .iter()
                .map(|p| p.display().to_string())
                .collect();
            println!(
                "  {}: {:?}",
                bn_catalog::text("cli", "msg.bnr_ffi_paths"),
                ffi_paths
            );
            CommandAction::Continue
        }
        ":load" => {
            if arg.is_empty() {
                eprintln!("{}", bn_catalog::text("cli", "msg.bnr_load_missing_path"));
            } else {
                match fs::read_to_string(arg) {
                    Ok(content) => match session.execute(&content, options) {
                        ExecutionOutcome::Success { stdout, .. } => {
                            if !stdout.is_empty() {
                                print!("{stdout}");
                            }
                        }
                        ExecutionOutcome::Failure { stdout, diagnostic } => {
                            if !stdout.is_empty() {
                                print!("{stdout}");
                            }
                            if let Some(diag) = diagnostic {
                                eprint!("{diag}");
                            }
                        }
                    },
                    Err(err) => eprintln!("{err}"),
                }
            }
            CommandAction::Continue
        }
        ":clear" => {
            print!("\x1b[2J\x1b[H");
            CommandAction::Continue
        }
        _ => {
            eprintln!(
                "{}",
                bn_catalog::fill("cli", "msg.bnr_unknown_command", &[("command", &command)])
            );
            CommandAction::Continue
        }
    }
}
