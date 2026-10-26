// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Integration test suite for the `bnr` interactive REPL executable.

use std::{
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
