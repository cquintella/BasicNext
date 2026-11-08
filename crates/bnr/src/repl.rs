// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Interactive Read-Eval-Print Loop terminal controller with autocomplete and history.

use std::{
    fs,
    io::{self, BufRead, IsTerminal},
    path::PathBuf,
    process::ExitCode,
    sync::{Arc, Mutex},
};

use bn_cli::{
    diagnostics::present_diagnostic, help::option_error, options::Options, output::tool_error,
};
use bn_frontend::{SubmissionCompleteness, token::reserved_words};
use bn_interpret_driver::options::InterpreterOptions;
use bn_source::SourceFile;
use rustyline::{
    Helper,
    completion::{Completer, Pair},
    error::ReadlineError,
    highlight::Highlighter,
    hint::Hinter,
    validate::Validator,
};

use crate::{
    commands::{CommandAction, handle_meta_command, is_meta_command},
    history::BnrHistory,
    session::{ExecutionOutcome, ReplSession},
};

pub const PROMPT_READY: &str = "bnr> ";
pub const PROMPT_COLLECTING: &str = "...> ";

struct ReplHelper {
    commands: Vec<String>,
    keywords: Vec<String>,
    context: Arc<Mutex<String>>,
    workspace_root: Option<PathBuf>,
}

impl Completer for ReplHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let safe_pos = if line.is_char_boundary(pos) {
            pos
        } else {
            line.char_indices()
                .map(|(i, _)| i)
                .take_while(|&i| i <= pos)
                .last()
                .unwrap_or(0)
        };
        let prefix = &line[..safe_pos];
        let word_start = prefix
            .char_indices()
            .rfind(|(_, c)| !c.is_alphanumeric() && *c != '_' && *c != ':' && *c != '.')
            .map_or(0, |(i, c)| i + c.len_utf8());
        let word = &prefix[word_start..];
        let mut matches = Vec::new();

        if word.starts_with(':') {
            for cmd in &self.commands {
                if cmd.starts_with(word) {
                    matches.push(Pair {
                        display: cmd.clone(),
                        replacement: cmd.clone(),
                    });
                }
            }
            return Ok((word_start, matches));
        }

        let ctx = self
            .context
            .lock()
            .map_or_else(|e| e.into_inner().clone(), |g| g.clone());
        let (repl_start, items) = bn_lsp::completion::complete_interactive(
            line,
            safe_pos,
            Some(&ctx),
            self.workspace_root.as_deref(),
        );

        if !items.is_empty() {
            for item in items {
                matches.push(Pair {
                    display: item.clone(),
                    replacement: item,
                });
            }
            return Ok((repl_start, matches));
        }

        if !word.is_empty() {
            for kw in &self.keywords {
                if kw.starts_with(word) {
                    matches.push(Pair {
                        display: kw.clone(),
                        replacement: kw.clone(),
                    });
                }
            }
        }

        Ok((word_start, matches))
    }
}

impl Hinter for ReplHelper {
    type Hint = String;
}

impl Highlighter for ReplHelper {}
impl Validator for ReplHelper {}
impl Helper for ReplHelper {}

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

    let config = rustyline::Config::default();
    let history = BnrHistory::with_config(config);
    let mut rl = match rustyline::Editor::with_history(config, history) {
        Ok(editor) => editor,
        Err(err) => {
            eprintln!("{err}");
            return tool_error();
        }
    };

    let mut helper_keywords: Vec<String> =
        reserved_words().iter().map(ToString::to_string).collect();
    let modules_text = bn_catalog::text("cli", "msg.bnr_autocomplete_modules");
    for m in modules_text.split(',') {
        helper_keywords.push(m.to_string());
    }

    let repl_context = Arc::new(Mutex::new(session.export_program()));
    let workspace_root = options.module_paths.first().map(|m| m.path.clone());
    let helper = ReplHelper {
        commands: vec![
            ":help".to_string(),
            ":quit".to_string(),
            ":q".to_string(),
            ":reset".to_string(),
            ":bindings".to_string(),
            ":vars".to_string(),
            ":type".to_string(),
            ":policy".to_string(),
            ":load".to_string(),
            ":clear".to_string(),
            ":arc".to_string(),
            ":ir".to_string(),
            ":cfg".to_string(),
            ":bench".to_string(),
            ":doc".to_string(),
            ":export".to_string(),
        ],
        keywords: helper_keywords,
        context: Arc::clone(&repl_context),
        workspace_root,
    };
    rl.set_helper(Some(helper));

    let update_context = |ctx: &Arc<Mutex<String>>, s: &ReplSession| {
        if let Ok(mut lock) = ctx.lock() {
            *lock = s.export_program();
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
                    match handle_meta_command(&line, &mut session, options, interpreter) {
                        CommandAction::Quit => {
                            let _ = rl.add_history_entry(&line);
                            break;
                        }
                        CommandAction::Continue => {
                            let _ = rl.add_history_entry(&line);
                            update_context(&repl_context, &session);
                            continue;
                        }
                        CommandAction::Failure => {
                            rl.history_mut().record_failure(&line);
                            continue;
                        }
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
                        rl.history_mut().record_failure(buffer.trim_end());
                        buffer.clear();
                    }
                    SubmissionCompleteness::Complete { .. } => {
                        let submission = buffer.trim_end().to_string();
                        match session.execute(&buffer, options) {
                            ExecutionOutcome::Success {
                                stdout,
                                result_value,
                                result_type,
                                structured_preview,
                            } => {
                                let _ = rl.add_history_entry(&submission);
                                update_context(&repl_context, &session);
                                if let Some(preview) = structured_preview {
                                    if let Some(ty) = result_type {
                                        println!("{preview} : {ty}");
                                    } else {
                                        println!("{preview}");
                                    }
                                } else {
                                    if !stdout.is_empty() {
                                        print!("{stdout}");
                                    }
                                    if let (Some(val), Some(ty)) = (result_value, result_type) {
                                        println!("{val} : {ty}");
                                    }
                                }
                            }
                            ExecutionOutcome::Failure { stdout, diagnostic } => {
                                rl.history_mut().record_failure(&submission);
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
    let mut had_error = false;

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if buffer.is_empty() && is_meta_command(&line) {
            let default_interp = InterpreterOptions::default();
            match handle_meta_command(&line, session, options, &default_interp) {
                CommandAction::Quit => return ExitCode::SUCCESS,
                CommandAction::Continue => {}
                CommandAction::Failure => had_error = true,
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
                had_error = true;
            }
            SubmissionCompleteness::Complete { .. } => {
                match session.execute(&buffer, options) {
                    ExecutionOutcome::Success {
                        stdout,
                        result_value,
                        result_type,
                        structured_preview,
                    } => {
                        if let Some(preview) = structured_preview {
                            if let Some(ty) = result_type {
                                println!("{preview} : {ty}");
                            } else {
                                println!("{preview}");
                            }
                        } else {
                            if !stdout.is_empty() {
                                print!("{stdout}");
                            }
                            if let (Some(val), Some(ty)) = (result_value, result_type) {
                                println!("{val} : {ty}");
                            }
                        }
                    }
                    ExecutionOutcome::Failure { stdout, diagnostic } => {
                        if !stdout.is_empty() {
                            print!("{stdout}");
                        }
                        if let Some(diag) = diagnostic {
                            eprint!("{diag}");
                        }
                        had_error = true;
                    }
                }
                buffer.clear();
            }
        }
    }

    if had_error || !buffer.trim().is_empty() {
        tool_error()
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repl_helper_completion() {
        let context = Arc::new(Mutex::new(
            "CLASS Person\n    PUBLIC name AS STRING\n    PUBLIC FUNCTION Greet() AS VOID\n    END FUNCTION\n    PUBLIC FUNCTION CONSTRUCTOR()\n    END FUNCTION\nEND CLASS\nFUNCTION Start() AS VOID\n    LET p AS Person = NEW Person()\nEND FUNCTION\n".to_string(),
        ));
        let helper = ReplHelper {
            commands: vec![":quit".to_string(), ":reset".to_string()],
            keywords: vec!["PRINT".to_string()],
            context,
            workspace_root: None,
        };
        let history = rustyline::history::DefaultHistory::new();
        let rl_ctx = rustyline::Context::new(&history);

        let (pos, candidates) = helper.complete(":q", 2, &rl_ctx).expect("completion");
        assert_eq!(pos, 0);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].display, ":quit");

        let (pos, candidates) = helper.complete("p.", 2, &rl_ctx).expect("completion");
        assert_eq!(pos, 2);
        let displays: Vec<_> = candidates.iter().map(|c| c.display.as_str()).collect();
        assert!(displays.contains(&"Greet"), "{displays:?}");
        assert!(displays.contains(&"name"), "{displays:?}");

        let (pos, candidates) = helper.complete("PRI", 3, &rl_ctx).expect("completion");
        assert_eq!(pos, 0);
        let displays: Vec<_> = candidates.iter().map(|c| c.display.as_str()).collect();
        assert!(displays.contains(&"PRINT"), "{displays:?}");
    }
}
