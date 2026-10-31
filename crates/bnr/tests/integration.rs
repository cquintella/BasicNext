// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Integration test suite for the `bnr` interactive REPL executable.

use std::{
    fmt::Write as _,
    io::Write,
    process::{Command, Stdio},
};

fn bnr_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_bnr"))
}

fn run_piped_session(input: &str) -> (i32, String, String) {
    let mut child = bnr_cmd()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn bnr");

    {
        let stdin = child.stdin.as_mut().expect("stdin handle");
        stdin.write_all(input.as_bytes()).expect("write to stdin");
    }

    let output = child.wait_with_output().expect("wait for bnr");
    let status_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (status_code, stdout, stderr)
}

#[test]
fn bnr_version_and_help_flags() {
    let output = bnr_cmd().arg("--version").output().expect("bnr --version");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("bnr 0.6."));

    let output = bnr_cmd().arg("--help").output().expect("bnr --help");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Interactive Read-Eval-Print Loop (REPL) for Basic Next."));
    assert!(stdout.contains("--no-filesystem"));
    assert!(stdout.contains("--sandbox"));
}

#[test]
fn bnr_evaluates_piped_expressions_and_types() {
    let input = "10 + 25\n:type 10 + 25\n:quit\n";
    let (code, stdout, stderr) = run_piped_session(input);
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert!(stdout.contains("35 : INT32"));
    assert!(stdout.contains("INT32"));
}

#[test]
fn bnr_preserves_variables_across_turns() {
    let input = "LET a AS INTEGER = 100\nLET b AS INTEGER = 50\nPRINT a - b\n:bindings\n:quit\n";
    let (code, stdout, stderr) = run_piped_session(input);
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert!(stdout.contains("50\n"));
    assert!(stdout.contains("Active bindings:"));
    assert!(stdout.contains("LET a AS INT32 [ALIVE]"));
    assert!(stdout.contains("LET b AS INT32 [ALIVE]"));
}

#[test]
fn bnr_accumulates_multiline_functions_and_classes() {
    let input = "\
FUNCTION Factorial(n AS INTEGER) AS INTEGER
    IF n <= 1 THEN
        RETURN 1
    END IF
    RETURN n * Factorial(n - 1)
END FUNCTION
Factorial(5)
:quit
";
    let (code, stdout, stderr) = run_piped_session(input);
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert!(stdout.contains("120 : INT32"));
}

#[test]
fn bnr_recovers_gracefully_from_syntax_and_runtime_errors() {
    let input = "\
LET base AS INTEGER = 42
THIS IS NOT VALID BASIC NEXT
LET divider AS INTEGER = 0
PRINT base DIV divider
PRINT base + 8
:quit
";
    let (code, stdout, stderr) = run_piped_session(input);
    assert_eq!(code, 0);
    assert!(stderr.contains("syntax error") || stderr.contains("Syntax error"));
    assert!(stderr.contains("DIVISION_BY_ZERO") || stderr.contains("Division by zero"));
    assert!(stdout.contains("50\n"));
}

#[test]
fn bnr_meta_commands_and_load_file() {
    let temp_dir = std::env::temp_dir();
    let script_path = temp_dir.join(format!("bnr_test_script_{}.bn", std::process::id()));
    std::fs::write(
        &script_path,
        "FUNCTION Tripled(x AS INTEGER) AS INTEGER\n    RETURN x * 3\nEND FUNCTION\n",
    )
    .expect("write test script");

    let input = format!(
        ":policy\n:load {}\nTripled(7)\n:reset\n:bindings\n:quit\n",
        script_path.display()
    );
    let (code, stdout, stderr) = run_piped_session(&input);
    let _ = std::fs::remove_file(&script_path);

    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert!(stdout.contains("Active execution policy:"));
    assert!(stdout.contains("21 : INT32"));
    assert!(stdout.contains("Session state reset."));
    assert!(stdout.contains("No user bindings in current session."));
}

#[test]
fn bnr_underscore_and_structured_vectors() {
    let input = "10 + 20\n_ * 2\n[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]\n:quit\n";
    let (code, stdout, stderr) = run_piped_session(input);
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert!(stdout.contains("30 : INT32"));
    assert!(stdout.contains("60 : INT32"));
    assert!(stdout.contains("[0]: 1"));
    assert!(stdout.contains("[9]: 10"));
    assert!(stdout.contains("INT32[10]"));
}

#[test]
fn bnr_arc_and_bench_inspection() {
    let input = ":arc\n:bench 10 + 20\n:quit\n";
    let (code, stdout, stderr) = run_piped_session(input);
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert!(stdout.contains("ARC Memory Panel:"));
    assert!(stdout.contains("live objects:"));
    assert!(stdout.contains("Benchmarking '10 + 20' (100 iterations):"));
    assert!(stdout.contains("min:"));
    assert!(stdout.contains("median:"));
    assert!(stdout.contains("mean:"));
}

#[test]
fn bnr_ir_and_cfg_inspection() {
    let input = ":ir 15 + 25\n:cfg 15 + 25\n:quit\n";
    let (code, stdout, stderr) = run_piped_session(input);
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert!(stdout.contains("Validated BN IR:"));
    assert!(stdout.contains("Control Flow Graph:"));
    assert!(stdout.contains("block #0:"));
}

#[test]
fn bnr_doc_symbol_lookup() {
    let input = ":doc HOST.Net\n:quit\n";
    let (code, stdout, stderr) = run_piped_session(input);
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert!(stdout.contains("TCPConnect"));
    assert!(stdout.contains("TCPListen"));
}

#[test]
fn bnr_export_and_native_compilation() {
    let temp_dir = std::env::temp_dir();
    let export_path = temp_dir.join(format!("bnr_test_export_{}.bn", std::process::id()));
    let bin_path = temp_dir.join(format!("bnr_test_export_bin_{}", std::process::id()));

    let input = format!(
        "FUNCTION AddTen(n AS INTEGER) AS INTEGER\n    RETURN n + 10\nEND FUNCTION\nPRINT AddTen(32)\n:export {}\n:quit\n",
        export_path.display()
    );
    let (code, stdout, stderr) = run_piped_session(&input);
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert!(stdout.contains("42\n"));
    assert!(stdout.contains("Session exported to"));
    assert!(export_path.is_file());

    let target_dir = std::path::Path::new(env!("CARGO_BIN_EXE_bnr"))
        .parent()
        .expect("target debug dir");
    let interpreter_bin = target_dir.join("bni");
    let compiler_bin = target_dir.join("bnc");

    // 1. Verify with bni check
    let check_out = Command::new(&interpreter_bin)
        .arg("check")
        .arg(&export_path)
        .output()
        .expect("bni check export");
    assert_eq!(
        check_out.status.code(),
        Some(0),
        "bni check failed: {}",
        String::from_utf8_lossy(&check_out.stderr)
    );

    // 2. Compile natively with bnc
    let bnc_out = Command::new(&compiler_bin)
        .arg(&export_path)
        .arg("-o")
        .arg(&bin_path)
        .output()
        .expect("bnc compile export");
    assert_eq!(
        bnc_out.status.code(),
        Some(0),
        "bnc failed: {}",
        String::from_utf8_lossy(&bnc_out.stderr)
    );
    assert!(bin_path.is_file());

    // 3. Run compiled native binary
    let run_out = Command::new(&bin_path)
        .output()
        .expect("run compiled binary");
    assert_eq!(run_out.status.code(), Some(0));
    let run_stdout = String::from_utf8_lossy(&run_out.stdout);
    assert_eq!(run_stdout.trim(), "42");

    let _ = std::fs::remove_file(&export_path);
    let _ = std::fs::remove_file(&bin_path);
}

#[test]
fn bnr_accumulates_exported_classes_and_method_calls() {
    let input = "\
EXPORT CLASS Counter
    PUBLIC STATIC FUNCTION Sum(a AS INTEGER, b AS INTEGER) AS INTEGER
        RETURN a + b
    END FUNCTION
END CLASS
Counter.Sum(15, 27)
:quit
";
    let (code, stdout, stderr) = run_piped_session(input);
    assert_eq!(code, 0);
    assert!(stderr.is_empty(), "unexpected stderr: {stderr}");
    assert!(stdout.contains("42 : INT32"));
}

#[test]
fn bnr_non_interactive_exits_with_error_code_on_syntax_error() {
    let input = "THIS IS NOT VALID BASIC NEXT\n";
    let (code, _stdout, stderr) = run_piped_session(input);
    assert_ne!(code, 0);
    assert!(stderr.contains("syntax error") || stderr.contains("Syntax error"));
}

#[derive(Debug)]
struct FixtureLine {
    prompt: String,
    content: String,
}

#[derive(Debug)]
struct FixtureTurn {
    lines: Vec<FixtureLine>,
    expected_outputs: Vec<String>,
    expected_error: Option<String>,
}

fn parse_fixture(content: &str) -> Vec<FixtureTurn> {
    let mut turns = Vec::new();
    let mut current_turn: Option<FixtureTurn> = None;

    for raw_line in content.lines() {
        let trimmed = raw_line.trim_end();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }

        if let Some(cmd) = trimmed.strip_prefix("bnr> ") {
            if let Some(turn) = current_turn.take() {
                turns.push(turn);
            }
            current_turn = Some(FixtureTurn {
                lines: vec![FixtureLine {
                    prompt: "bnr> ".to_string(),
                    content: cmd.to_string(),
                }],
                expected_outputs: Vec::new(),
                expected_error: None,
            });
        } else if let Some(cmd) = trimmed.strip_prefix("...> ") {
            if let Some(ref mut turn) = current_turn {
                turn.lines.push(FixtureLine {
                    prompt: "...> ".to_string(),
                    content: cmd.to_string(),
                });
            } else {
                panic!("Encountered continuation prompt '...>' without preceding 'bnr>'");
            }
        } else if let Some(err) = trimmed.strip_prefix("!error") {
            if let Some(ref mut turn) = current_turn {
                turn.expected_error = Some(err.trim().to_string());
            }
        } else if let Some(ref mut turn) = current_turn {
            turn.expected_outputs.push(trimmed.to_string());
        }
    }

    if let Some(turn) = current_turn {
        turns.push(turn);
    }

    turns
}

#[allow(clippy::too_many_lines)]
fn run_fixture(path: &std::path::Path) {
    let content = std::fs::read_to_string(path).expect("read fixture file");
    let turns = parse_fixture(&content);

    let mut interpreter = bn_interpret_driver::options::InterpreterOptions::default();
    let options =
        bn_cli::options::parse_options(vec!["<repl>".to_string()].into_iter(), &mut interpreter)
            .expect("parse options");

    let mut session =
        bnr::session::ReplSession::new(&options, &interpreter).expect("initialize ReplSession");

    let mut buffer = String::new();

    for (turn_idx, turn) in turns.iter().enumerate() {
        let mut turn_stdout = String::new();
        let mut turn_stderr = String::new();

        for line in &turn.lines {
            let current_prompt = if buffer.is_empty() {
                bnr::repl::PROMPT_READY
            } else {
                bnr::repl::PROMPT_COLLECTING
            };
            assert_eq!(
                current_prompt,
                line.prompt,
                "Fixture '{}' turn {}: expected prompt '{}', but session was '{}'",
                path.display(),
                turn_idx + 1,
                line.prompt,
                current_prompt
            );

            if buffer.is_empty() && bnr::commands::is_meta_command(&line.content) {
                let mut out_bytes = Vec::new();
                let mut err_bytes = Vec::new();
                let action = bnr::commands::handle_meta_command_to(
                    &line.content,
                    &mut session,
                    &options,
                    &interpreter,
                    &mut out_bytes,
                    &mut err_bytes,
                );
                turn_stdout.push_str(&String::from_utf8_lossy(&out_bytes));
                turn_stderr.push_str(&String::from_utf8_lossy(&err_bytes));
                if action == bnr::commands::CommandAction::Quit {
                    return;
                }
                continue;
            }

            buffer.push_str(&line.content);
            buffer.push('\n');

            match session.classify(&buffer) {
                bn_frontend::session::SubmissionCompleteness::Incomplete { .. } => {
                    // Waiting for more lines in the multi-line block
                }
                bn_frontend::session::SubmissionCompleteness::Invalid { diagnostic, .. } => {
                    let source_file = bn_source::SourceFile::new("<session-classify>", &buffer);
                    let formatted = bn_cli::diagnostics::present_diagnostic(
                        &diagnostic,
                        &source_file,
                        None,
                        &options,
                    );
                    turn_stderr.push_str(&formatted);
                    buffer.clear();
                }
                bn_frontend::session::SubmissionCompleteness::Complete { .. } => {
                    match session.execute(&buffer, &options) {
                        bnr::session::ExecutionOutcome::Success {
                            stdout,
                            result_value,
                            result_type,
                            structured_preview,
                        } => {
                            if let Some(preview) = structured_preview {
                                if let Some(ty) = result_type {
                                    let _ = writeln!(turn_stdout, "{preview} : {ty}");
                                } else {
                                    let _ = writeln!(turn_stdout, "{preview}");
                                }
                            } else {
                                if !stdout.is_empty() {
                                    turn_stdout.push_str(&stdout);
                                }
                                if let (Some(val), Some(ty)) = (result_value, result_type) {
                                    let _ = writeln!(turn_stdout, "{val} : {ty}");
                                }
                            }
                        }
                        bnr::session::ExecutionOutcome::Failure { stdout, diagnostic } => {
                            if !stdout.is_empty() {
                                turn_stdout.push_str(&stdout);
                            }
                            if let Some(diag) = diagnostic {
                                turn_stderr.push_str(&diag);
                            }
                        }
                    }
                    buffer.clear();
                }
            }
        }

        for expected in &turn.expected_outputs {
            assert!(
                turn_stdout.contains(expected),
                "Fixture '{}' turn {}: expected stdout to contain '{}', but got stdout:\n{}\nstderr:\n{}",
                path.display(),
                turn_idx + 1,
                expected,
                turn_stdout,
                turn_stderr
            );
        }

        if let Some(ref expected_err) = turn.expected_error {
            assert!(
                turn_stderr.contains(expected_err) || turn_stdout.contains(expected_err),
                "Fixture '{}' turn {}: expected error '{}', but got stdout:\n{}\nstderr:\n{}",
                path.display(),
                turn_idx + 1,
                expected_err,
                turn_stdout,
                turn_stderr
            );
        }
    }
}

#[test]
fn bnr_all_transcript_fixtures() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixtures_dir = manifest_dir.join("tests/fixtures");
    assert!(fixtures_dir.is_dir(), "fixtures dir exists");

    let mut entries: Vec<_> = std::fs::read_dir(&fixtures_dir)
        .expect("read fixtures dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "repl"))
        .collect();
    entries.sort();

    assert!(!entries.is_empty(), "found .repl fixtures");
    for fixture_path in &entries {
        run_fixture(fixture_path);
    }
}
