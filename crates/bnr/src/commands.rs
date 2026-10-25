// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! REPL meta-commands implementation.

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
        ":arc" => {
            let inspection = session.arc_inspection();
            println!(
                "{}",
                bn_catalog::fill(
                    "cli",
                    "msg.bnr_arc_panel",
                    &[
                        ("objects", &inspection.total_live_objects.to_string()),
                        (
                            "allocations",
                            &inspection.live_memory_allocations.to_string()
                        ),
                        ("instances", &inspection.live_instances.to_string()),
                    ]
                )
            );
            if !inspection.live_objects.is_empty() {
                println!("{}", bn_catalog::text("cli", "msg.bnr_arc_header"));
                for obj in &inspection.live_objects {
                    if arg.is_empty() || obj.class.contains(arg) {
                        let status = if obj.strong > 0 { "ALIVE" } else { "RELEASED" };
                        let slot_str = obj.id.bits().to_string();
                        let strong_str = obj.strong.to_string();
                        println!(
                            "{}",
                            bn_catalog::fill(
                                "cli",
                                "msg.bnr_arc_row",
                                &[
                                    ("slot", &slot_str),
                                    ("class", &obj.class),
                                    ("strong", &strong_str),
                                    ("status", &status),
                                ]
                            )
                        );
                    }
                }
            }
            CommandAction::Continue
        }
        ":ir" => {
            let ir = session.ir_of(arg);
            println!("{}", bn_catalog::text("cli", "msg.bnr_ir_header"));
            println!("{ir}");
            CommandAction::Continue
        }
        ":cfg" => {
            let cfg = session.cfg_of(arg);
            println!("{}", bn_catalog::text("cli", "msg.bnr_cfg_header"));
            println!("{cfg}");
            CommandAction::Continue
        }
        ":bench" => {
            if arg.is_empty() {
                eprintln!("{}", bn_catalog::text("cli", "msg.bnr_bench_missing_expr"));
            } else {
                match session.bench(arg, 100, options) {
                    Ok(stats) => {
                        let iter_str = stats.iterations.to_string();
                        let min_str = stats.min_us.to_string();
                        let med_str = stats.median_us.to_string();
                        let mean_str = stats.mean_us.to_string();
                        let alloc_str = stats.alloc_delta.to_string();
                        println!(
                            "{}",
                            bn_catalog::fill(
                                "cli",
                                "msg.bnr_bench_results",
                                &[
                                    ("expr", &arg),
                                    ("iterations", &iter_str),
                                    ("min", &min_str),
                                    ("median", &med_str),
                                    ("mean", &mean_str),
                                    ("alloc_delta", &alloc_str),
                                ]
                            )
                        );
                    }
                    Err(diag) => {
                        if !diag.is_empty() {
                            eprint!("{diag}");
                        }
                    }
                }
            }
            CommandAction::Continue
        }
        ":doc" => {
            if arg.is_empty() {
                eprintln!("{}", bn_catalog::text("cli", "msg.bnr_doc_missing_symbol"));
            } else {
                let docs = session.doc_symbol(arg, options);
                if docs.is_empty() {
                    eprintln!(
                        "{}",
                        bn_catalog::fill("cli", "msg.bnr_symbol_not_found", &[("symbol", &arg)])
                    );
                } else {
                    for line in docs {
                        println!("  {line}");
                    }
                }
            }
            CommandAction::Continue
        }
        ":export" => {
            if arg.is_empty() {
                eprintln!("{}", bn_catalog::text("cli", "msg.bnr_export_missing_path"));
            } else {
                let code = session.export_program();
                match fs::write(arg, code) {
                    Ok(()) => {
                        println!(
                            "{}",
                            bn_catalog::fill("cli", "msg.bnr_export_success", &[("path", &arg)])
                        );
                    }
                    Err(err) => eprintln!("{err}"),
                }
            }
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
                                eprintln!("{diag}");
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
