// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! REPL meta-commands implementation.

use std::{
    fs,
    io::{self, Write},
};

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
    let mut out = io::stdout();
    let mut err = io::stderr();
    handle_meta_command_to(line, session, options, interpreter, &mut out, &mut err)
}

#[allow(clippy::too_many_lines)]
pub fn handle_meta_command_to(
    line: &str,
    session: &mut ReplSession,
    options: &Options,
    interpreter: &InterpreterOptions,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> CommandAction {
    let trimmed = line.trim();
    let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
    let command = parts[0];
    let arg = parts.get(1).map_or("", |s| s.trim());

    match command {
        ":help" => {
            let _ = writeln!(out, "{}", bn_catalog::text("cli", "msg.bnr_help_text"));
            CommandAction::Continue
        }
        ":quit" | ":q" => CommandAction::Quit,
        ":reset" => {
            if let Err(msg) = session.reset(options, interpreter) {
                let _ = writeln!(err, "{msg}");
            } else {
                let _ = writeln!(out, "{}", bn_catalog::text("cli", "msg.bnr_reset_done"));
            }
            CommandAction::Continue
        }
        ":bindings" | ":vars" => {
            let bindings = session.bindings();
            if bindings.is_empty() {
                let _ = writeln!(out, "{}", bn_catalog::text("cli", "msg.bnr_empty_bindings"));
            } else {
                let _ = writeln!(
                    out,
                    "{}",
                    bn_catalog::text("cli", "msg.bnr_active_bindings")
                );
                for b in bindings {
                    let _ = writeln!(
                        out,
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
                let _ = writeln!(
                    err,
                    "{}",
                    bn_catalog::text("cli", "msg.bnr_type_missing_expr")
                );
            } else {
                match session.type_of(arg, options) {
                    Ok(ty) => {
                        let _ = writeln!(out, "{ty}");
                    }
                    Err(diag) => {
                        if !diag.is_empty() {
                            let _ = write!(err, "{diag}");
                        }
                    }
                }
            }
            CommandAction::Continue
        }
        ":policy" => {
            let _ = writeln!(out, "{}", bn_catalog::text("cli", "msg.bnr_active_policy"));
            let _ = writeln!(
                out,
                "  {}: {}",
                bn_catalog::text("cli", "msg.bnr_sandbox"),
                options.sandbox
            );
            let _ = writeln!(
                out,
                "  {}: {}",
                bn_catalog::text("cli", "msg.bnr_filesystem"),
                interpreter.filesystem
            );
            let _ = writeln!(
                out,
                "  {}: {:?}",
                bn_catalog::text("cli", "msg.bnr_read_roots"),
                options.read_roots
            );
            let _ = writeln!(
                out,
                "  {}: {:?}",
                bn_catalog::text("cli", "msg.bnr_write_roots"),
                options.write_roots
            );
            let paths: Vec<String> = options
                .module_paths
                .iter()
                .map(|p| p.path.display().to_string())
                .collect();
            let _ = writeln!(
                out,
                "  {}: {:?}",
                bn_catalog::text("cli", "msg.bnr_module_paths"),
                paths
            );
            let ffi_paths: Vec<String> = interpreter
                .ffi_paths
                .iter()
                .map(|p| p.display().to_string())
                .collect();
            let _ = writeln!(
                out,
                "  {}: {:?}",
                bn_catalog::text("cli", "msg.bnr_ffi_paths"),
                ffi_paths
            );
            CommandAction::Continue
        }
        ":arc" => {
            let inspection = session.arc_inspection();
            let _ = writeln!(
                out,
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
                let _ = writeln!(out, "{}", bn_catalog::text("cli", "msg.bnr_arc_header"));
                for obj in &inspection.live_objects {
                    if arg.is_empty() || obj.class.contains(arg) {
                        let status = if obj.strong > 0 { "ALIVE" } else { "RELEASED" };
                        let slot_str = obj.id.bits().to_string();
                        let strong_str = obj.strong.to_string();
                        let _ = writeln!(
                            out,
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
            let _ = writeln!(out, "{}", bn_catalog::text("cli", "msg.bnr_ir_header"));
            let _ = writeln!(out, "{ir}");
            CommandAction::Continue
        }
        ":cfg" => {
            let cfg = session.cfg_of(arg);
            let _ = writeln!(out, "{}", bn_catalog::text("cli", "msg.bnr_cfg_header"));
            let _ = writeln!(out, "{cfg}");
            CommandAction::Continue
        }
        ":bench" => {
            if arg.is_empty() {
                let _ = writeln!(
                    err,
                    "{}",
                    bn_catalog::text("cli", "msg.bnr_bench_missing_expr")
                );
            } else {
                match session.bench(arg, 100, options) {
                    Ok(stats) => {
                        let iter_str = stats.iterations.to_string();
                        let min_str = stats.min_us.to_string();
                        let med_str = stats.median_us.to_string();
                        let mean_str = stats.mean_us.to_string();
                        let alloc_str = stats.alloc_delta.to_string();
                        let _ = writeln!(
                            out,
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
                            let _ = write!(err, "{diag}");
                        }
                    }
                }
            }
            CommandAction::Continue
        }
        ":doc" => {
            if arg.is_empty() {
                let _ = writeln!(
                    err,
                    "{}",
                    bn_catalog::text("cli", "msg.bnr_doc_missing_symbol")
                );
            } else {
                let docs = session.doc_symbol(arg, options);
                if docs.is_empty() {
                    let _ = writeln!(
                        err,
                        "{}",
                        bn_catalog::fill("cli", "msg.bnr_symbol_not_found", &[("symbol", &arg)])
                    );
                } else {
                    for line in docs {
                        let _ = writeln!(out, "  {line}");
                    }
                }
            }
            CommandAction::Continue
        }
        ":export" => {
            if arg.is_empty() {
                let _ = writeln!(
                    err,
                    "{}",
                    bn_catalog::text("cli", "msg.bnr_export_missing_path")
                );
            } else {
                let code = session.export_program();
                match fs::write(arg, code) {
                    Ok(()) => {
                        let _ = writeln!(
                            out,
                            "{}",
                            bn_catalog::fill("cli", "msg.bnr_export_success", &[("path", &arg)])
                        );
                    }
                    Err(io_err) => {
                        let _ = writeln!(err, "{io_err}");
                    }
                }
            }
            CommandAction::Continue
        }
        ":load" => {
            if arg.is_empty() {
                let _ = writeln!(
                    err,
                    "{}",
                    bn_catalog::text("cli", "msg.bnr_load_missing_path")
                );
            } else {
                match fs::read_to_string(arg) {
                    Ok(content) => match session.execute(&content, options) {
                        ExecutionOutcome::Success { stdout, .. } => {
                            if !stdout.is_empty() {
                                let _ = write!(out, "{stdout}");
                            }
                        }
                        ExecutionOutcome::Failure { stdout, diagnostic } => {
                            if !stdout.is_empty() {
                                let _ = write!(out, "{stdout}");
                            }
                            if let Some(diag) = diagnostic {
                                let _ = writeln!(err, "{diag}");
                            }
                        }
                    },
                    Err(io_err) => {
                        let _ = writeln!(err, "{io_err}");
                    }
                }
            }
            CommandAction::Continue
        }
        ":clear" => {
            let _ = write!(out, "\x1b[2J\x1b[H");
            CommandAction::Continue
        }
        _ => {
            let _ = writeln!(
                err,
                "{}",
                bn_catalog::fill("cli", "msg.bnr_unknown_command", &[("command", &command)])
            );
            CommandAction::Continue
        }
    }
}
