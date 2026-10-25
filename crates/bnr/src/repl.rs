// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Interactive Read-Eval-Print Loop terminal controller.

use std::{
    fs,
    io::{self, BufRead, IsTerminal},
    path::PathBuf,
    process::ExitCode,
};

use bn_cli::{
    diagnostics::present_diagnostic, help::option_error, options::Options, output::tool_error,
};
use bn_frontend::SubmissionCompleteness;
use bn_interpret_driver::options::InterpreterOptions;
use bn_source::SourceFile;
use rustyline::{DefaultEditor, error::ReadlineError};

use crate::{
    commands::{CommandAction, handle_meta_command, is_meta_command},
    session::{ExecutionOutcome, ReplSession},
};

const PROMPT_READY: &str = "bnr> ";
const PROMPT_COLLECTING: &str = "...> ";

fn resolve_history_path() -> Option<PathBuf> {
    let env_key = bn_catalog::text("cli", "msg.bnr_history_file");
    if let Some(path) = std::env::var_os(env_key) {
        return Some(PathBuf::from(path));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".bn_history"))
}

#[must_use]
#[allow(clippy::too_many_lines)]
pub fn run_repl(options: &Options, interpreter: &InterpreterOptions, version: &str) -> ExitCode {
    let mut session = match ReplSession::new(options, interpreter) {
        Ok(s) => s,
        Err(err) => {
            let help = bn_catalog::fill(
                "cli",
                "help.bnr",
                &[
                    ("version", &version),
                    ("common_options", &bn_cli::help::COMMON_OPTIONS),
                    ("book_url", &bn_cli::help::BOOK_URL),
                ],
            );
            return option_error(&err, &help);
        }
    };

    if !options.path.is_empty() && options.path != "." {
        match fs::read_to_string(&options.path) {
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
                    return tool_error();
                }
            },
            Err(err) => {
                eprintln!("{err}");
                return tool_error();
            }
        }
    }

    if !io::stdin().is_terminal() {
        return run_non_interactive(&mut session, options);
    }

    let target = format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS);
    println!(
        "{}",
        bn_catalog::fill(
            "cli",
            "msg.bnr_banner",
            &[("version", &version), ("target", &target)]
        )
    );

    let mut rl = match DefaultEditor::new() {
        Ok(editor) => editor,
        Err(err) => {
            eprintln!("{err}");
            return tool_error();
        }
    };

    let history_file = resolve_history_path();
    if let Some(ref path) = history_file {
        let _ = rl.load_history(path);
    }

    let mut buffer = String::new();
    loop {
        let prompt = if buffer.is_empty() {
            PROMPT_READY
        } else {
            PROMPT_COLLECTING
        };

        match rl.readline(prompt) {
            Ok(line) => {
                if buffer.is_empty() && is_meta_command(&line) {
                    let _ = rl.add_history_entry(&line);
                    match handle_meta_command(&line, &mut session, options, interpreter) {
                        CommandAction::Quit => break,
                        CommandAction::Continue => continue,
                    }
                }

                if buffer.is_empty() && line.trim().is_empty() {
                    continue;
                }

                buffer.push_str(&line);
                buffer.push('\n');

                match session.classify(&buffer) {
                    SubmissionCompleteness::Incomplete { .. } => {}
                    SubmissionCompleteness::Invalid { diagnostic, .. } => {
                        let source_file = SourceFile::new("<session-classify>", &buffer);
                        let formatted =
                            present_diagnostic(&diagnostic, &source_file, None, options);
                        eprint!("{formatted}");
                        buffer.clear();
                    }
                    SubmissionCompleteness::Complete { .. } => {
                        let _ = rl.add_history_entry(buffer.trim_end());
                        match session.execute(&buffer, options) {
                            ExecutionOutcome::Success {
                                stdout,
                                result_value,
                                result_type,
                            } => {
                                if !stdout.is_empty() {
                                    print!("{stdout}");
                                }
                                if let (Some(val), Some(ty)) = (result_value, result_type) {
                                    println!("{val} : {ty}");
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
                        }
                        buffer.clear();
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                if buffer.is_empty() {
                    println!("{}", bn_catalog::text("cli", "msg.bnr_ctrl_c_hint"));
                } else {
                    println!("^C");
                    buffer.clear();
                }
            }
            Err(ReadlineError::Eof) => break,
            Err(err) => {
                eprintln!("{err}");
                return tool_error();
            }
        }
    }

    if let Some(ref path) = history_file {
        let _ = rl.save_history(path);
    }

    ExitCode::SUCCESS
}

fn run_non_interactive(session: &mut ReplSession, options: &Options) -> ExitCode {
    let stdin = io::stdin();
    let mut buffer = String::new();

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if buffer.is_empty() && is_meta_command(&line) {
            let default_interp = InterpreterOptions::default();
            if let CommandAction::Quit =
                handle_meta_command(&line, session, options, &default_interp)
            {
                break;
            }
            continue;
        }

        if buffer.is_empty() && line.trim().is_empty() {
            continue;
        }

        buffer.push_str(&line);
        buffer.push('\n');

        match session.classify(&buffer) {
            SubmissionCompleteness::Incomplete { .. } => {}
            SubmissionCompleteness::Invalid { diagnostic, .. } => {
                let source_file = SourceFile::new("<session-classify>", &buffer);
                let formatted = present_diagnostic(&diagnostic, &source_file, None, options);
                eprint!("{formatted}");
                buffer.clear();
            }
            SubmissionCompleteness::Complete { .. } => {
                match session.execute(&buffer, options) {
                    ExecutionOutcome::Success {
                        stdout,
                        result_value,
                        result_type,
                    } => {
                        if !stdout.is_empty() {
                            print!("{stdout}");
                        }
                        if let (Some(val), Some(ty)) = (result_value, result_type) {
                            println!("{val} : {ty}");
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
                }
                buffer.clear();
            }
        }
    }

    ExitCode::SUCCESS
}
