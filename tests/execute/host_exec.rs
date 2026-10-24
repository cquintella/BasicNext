// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::support::*;
use std::fs;
use std::path::Path;
/// E01 — exit 0 with exact stdout/stderr and Result fields (interpret).
#[test]
fn host_exec_e01_literal_argv_returns_result_fields() {
    let _guard = host_exec_test_lock();
    let helper = build_host_exec_helper();
    let helper_path = bn_path(&helper);
    let source = format!(
        r#"IMPORT HOST.Exec AS Exec
FUNCTION Start() AS VOID
LET r AS Exec.Result OR Error = Exec.Run("{helper}", ["stdout", "exact-out"])
IF r IS Error THEN
PRINT "err", r.Code
ELSE
PRINT r.ReturnCode, r.Stdout, r.Stderr
END IF
LET status_result AS Exec.Result OR Error = Exec.Run("{helper}", ["status", "0"])
IF status_result IS Error THEN
PRINT "err", status_result.Code
ELSE
PRINT status_result.ReturnCode
END IF
LET err_stream AS Exec.Result OR Error = Exec.Run("{helper}", ["stderr", "exact-err"])
IF err_stream IS Error THEN
PRINT "err", err_stream.Code
ELSE
PRINT err_stream.ReturnCode, err_stream.Stdout, err_stream.Stderr
END IF
END FUNCTION
"#,
        helper = helper_path
    );
    let (_, output) = run(&source, "").expect("execute HOST.Exec E01");
    assert_eq!(output, "0 exact-out \n0\n0  exact-err\n");
    // shared helper retained for suite
}

/// E02 — nonzero child exit remains Result; BN caller continues.
#[test]
fn host_exec_e02_nonzero_exit_remains_result() {
    let _guard = host_exec_test_lock();
    let helper = build_host_exec_helper();
    let helper_path = bn_path(&helper);
    let source = format!(
        r#"IMPORT HOST.Exec AS Exec
FUNCTION Start() AS VOID
LET r AS Exec.Result OR Error = Exec.Run("{helper}", ["status", "7"])
IF r IS Error THEN
PRINT "err", r.Code
ELSE
PRINT "ok", r.ReturnCode
END IF
PRINT "continued"
END FUNCTION
"#,
        helper = helper_path
    );
    let (_, output) = run(&source, "").expect("execute HOST.Exec E02");
    assert_eq!(output, "ok 7\ncontinued\n");
    // shared helper retained for suite
}

/// E03 — missing executable / permission failure → portable Error codes.
#[test]
fn host_exec_e03_missing_and_permission_errors() {
    let _guard = host_exec_test_lock();
    let helper = build_host_exec_helper();
    let helper_path = bn_path(&helper);
    let missing = unique_temp("host-exec-missing-bin");
    let missing_path = bn_path(&missing);
    // Non-executable file at a real path (permission denied on spawn).
    let denied = unique_temp("host-exec-not-exec");
    fs::write(&denied, b"#!/bin/sh\necho no\n").expect("write non-exec");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&denied).expect("meta").permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&denied, perms).expect("chmod");
    }
    let denied_path = bn_path(&denied);
    let source = format!(
        r#"IMPORT HOST.Exec AS Exec
FUNCTION Start() AS VOID
LET missing AS Exec.Result OR Error = Exec.Run("{missing}", [])
IF missing IS Error THEN
PRINT "missing", missing.Code
ELSE
PRINT "missing-ok"
END IF
LET denied AS Exec.Result OR Error = Exec.Run("{denied}", [])
IF denied IS Error THEN
PRINT "denied", denied.Code
ELSE
PRINT "denied-ok", denied.ReturnCode
END IF
LET known AS Exec.Result OR Error = Exec.Run("{helper}", ["status", "0"])
IF known IS Error THEN
PRINT "helper-err", known.Code
ELSE
PRINT "helper-ok", known.ReturnCode
END IF
END FUNCTION
"#,
        missing = missing_path,
        denied = denied_path,
        helper = helper_path
    );
    let (_, output) = run(&source, "").expect("execute HOST.Exec E03");
    // PROGRAM_NOT_FOUND=2; PERMISSION_DENIED=3 (or SPAWN_FAILED=4 on platforms
    // that report a different errno for non-exec files).
    assert!(
        output.starts_with("missing 2\n"),
        "missing executable must be PROGRAM_NOT_FOUND=2, got {output:?}"
    );
    assert!(
        output.contains("denied 3\n") || output.contains("denied 4\n"),
        "non-executable must be PERMISSION_DENIED=3 or SPAWN_FAILED=4, got {output:?}"
    );
    assert!(
        output.ends_with("helper-ok 0\n"),
        "helper must still run after errors, got {output:?}"
    );
    // shared helper retained for suite
    let _ = fs::remove_file(&denied);
}

/// E04 — empty program; embedded NUL in program/argument → INVALID_ARGUMENT=1.
#[test]
fn host_exec_e04_empty_and_nul_invalid_argument() {
    let _guard = host_exec_test_lock();
    let helper = build_host_exec_helper();
    let helper_path = bn_path(&helper);
    let source = format!(
        r#"IMPORT HOST.Exec AS Exec
FUNCTION Start() AS VOID
LET empty AS Exec.Result OR Error = Exec.Run("", [])
IF empty IS Error THEN
PRINT "empty", empty.Code
ELSE
PRINT "empty-ok"
END IF
LET nul AS STRING OR Error = CHAR(0)
IF nul IS STRING THEN
LET prog_nul AS Exec.Result OR Error = Exec.Run(nul, [])
IF prog_nul IS Error THEN
PRINT "prog-nul", prog_nul.Code
ELSE
PRINT "prog-nul-ok"
END IF
LET arg_nul AS Exec.Result OR Error = Exec.Run("{helper}", [nul])
IF arg_nul IS Error THEN
PRINT "arg-nul", arg_nul.Code
ELSE
PRINT "arg-nul-ok"
END IF
ELSE
PRINT "char-err"
END IF
END FUNCTION
"#,
        helper = helper_path
    );
    let (_, output) = run(&source, "").expect("execute HOST.Exec E04");
    assert_eq!(output, "empty 1\nprog-nul 1\narg-nul 1\n");
    // shared helper retained for suite
}

/// E05 — policy deny returns POLICY_DENIED=11; child side-effect marker absent.
#[test]
fn host_exec_e05_policy_denied_no_side_effect() {
    let _guard = host_exec_test_lock();
    let helper = build_host_exec_helper();
    let helper_path = bn_path(&helper);
    let marker = unique_temp("host-exec-marker.txt");
    let marker_path = bn_path(&marker);
    let _ = fs::remove_file(&marker);

    let denied_source = format!(
        r#"IMPORT HOST.Exec AS Exec
FUNCTION Start() AS VOID
LET r AS Exec.Result OR Error = Exec.Run("{helper}", ["touch", "{marker}"])
IF r IS Error THEN
PRINT "denied", r.Code
ELSE
PRINT "ran", r.ReturnCode
END IF
END FUNCTION
"#,
        helper = helper_path,
        marker = marker_path
    );
    let host = fixed(vec!["runtime.bn".into()], 0, 0).without_exec();
    let (_, output) = run_with_host(&denied_source, "", &host).expect("E05 denied");
    assert_eq!(output, "denied 11\n");
    assert!(
        !marker.exists(),
        "policy deny must not spawn the child (marker must be absent)"
    );

    let allowed_source = denied_source.clone();
    let (_, output) = run(&allowed_source, "").expect("E05 allowed");
    assert_eq!(output, "ran 0\n");
    assert!(
        marker.exists(),
        "allowed Run must create the side-effect marker"
    );
    // shared helper retained for suite
    let _ = fs::remove_file(&marker);
}

/// E06 — spaces, empty args, quotes, Unicode and shell metacharacters stay literal.
#[test]
fn host_exec_e06_literal_argv_elements() {
    let _guard = host_exec_test_lock();
    let helper = build_host_exec_helper();
    let helper_path = bn_path(&helper);
    let source = format!(
        r#"IMPORT HOST.Exec AS Exec
FUNCTION Start() AS VOID
LET r AS Exec.Result OR Error = Exec.Run("{helper}", ["argv", "hello world", "", "café", "; rm -rf /", "\"quoted\"", "$HOME"])
IF r IS Error THEN
PRINT "err", r.Code
ELSE
PRINT r.Stdout
END IF
END FUNCTION
"#,
        helper = helper_path
    );
    let (_, output) = run(&source, "").expect("execute HOST.Exec E06");
    assert_eq!(
        output,
        "0=hello world\n1=\n2=café\n3=; rm -rf /\n4=\"quoted\"\n5=$HOME\n\n"
    );
    // shared helper retained for suite
}

/// E07 — interpret half of the matrix is covered by E01–E06/E08–E14 in this file.
/// Native/LLVM parity remains ACTIVITY 4.3 (no fake green).
#[test]
fn host_exec_e07_interpret_matrix_documented() {
    let _guard = host_exec_test_lock();
    // This test only records that the interpret suite exists as executable
    // tests above/below; native comparison is explicitly out of this turn.
    assert!(
        Path::new("tests/fixtures/host_exec_helper.rs").is_file(),
        "helper fixture must exist for interpret E-matrix"
    );
}

/// E08 — child stdin is immediately EOF (closed / null device).
#[test]
fn host_exec_e08_child_stdin_is_eof() {
    let _guard = host_exec_test_lock();
    let helper = build_host_exec_helper();
    let helper_path = bn_path(&helper);
    let source = format!(
        r#"IMPORT HOST.Exec AS Exec
FUNCTION Start() AS VOID
LET r AS Exec.Result OR Error = Exec.Run("{helper}", ["echo-stdin"])
IF r IS Error THEN
PRINT "err", r.Code
ELSE
PRINT "len", LEN(r.Stdout), "code", r.ReturnCode
END IF
END FUNCTION
"#,
        helper = helper_path
    );
    let (_, output) =
        run(&source, "parent-stdin-must-not-reach-child\n").expect("execute HOST.Exec E08");
    assert_eq!(output, "len 0 code 0\n");
    // shared helper retained for suite
}

/// E09 — invalid UTF-8 on either captured stream → INVALID_UTF8=7.
#[test]
fn host_exec_e09_invalid_utf8_errors() {
    let _guard = host_exec_test_lock();
    let helper = build_host_exec_helper();
    let helper_path = bn_path(&helper);
    let source = format!(
        r#"IMPORT HOST.Exec AS Exec
FUNCTION Start() AS VOID
LET out AS Exec.Result OR Error = Exec.Run("{helper}", ["invalid-utf8"])
IF out IS Error THEN
PRINT "stdout", out.Code
ELSE
PRINT "stdout-ok", out.Stdout
END IF
LET err AS Exec.Result OR Error = Exec.Run("{helper}", ["invalid-utf8-stderr"])
IF err IS Error THEN
PRINT "stderr", err.Code
ELSE
PRINT "stderr-ok", err.Stderr
END IF
END FUNCTION
"#,
        helper = helper_path
    );
    let (_, output) = run(&source, "").expect("execute HOST.Exec E09");
    assert_eq!(output, "stdout 7\nstderr 7\n");
    // shared helper retained for suite
}

/// E10 — simultaneous stdout/stderr beyond pipe capacity; reduced capture limit.
#[test]
fn host_exec_e10_concurrent_streams_and_capture_limit() {
    let _guard = host_exec_test_lock();
    let helper = build_host_exec_helper();
    let helper_path = bn_path(&helper);
    // 256 KiB per stream exceeds typical pipe capacity (~64 KiB) without
    // needing the full 16 MiB default ceiling.
    let source = format!(
        r#"IMPORT HOST.Exec AS Exec
FUNCTION Start() AS VOID
LET r AS Exec.Result OR Error = Exec.Run("{helper}", ["both", "262144"])
IF r IS Error THEN
PRINT "err", r.Code
ELSE
PRINT "ok", LEN(r.Stdout), LEN(r.Stderr), r.ReturnCode
END IF
END FUNCTION
"#,
        helper = helper_path
    );
    let (_, output) = run(&source, "").expect("execute HOST.Exec E10 concurrent");
    assert_eq!(output, "ok 262144 262144 0\n");

    let limit_source = format!(
        r#"IMPORT HOST.Exec AS Exec
FUNCTION Start() AS VOID
LET r AS Exec.Result OR Error = Exec.Run("{helper}", ["stdout-bytes", "4096"])
IF r IS Error THEN
PRINT "limit", r.Code
ELSE
PRINT "unexpected", LEN(r.Stdout)
END IF
END FUNCTION
"#,
        helper = helper_path
    );
    let host = fixed(vec!["runtime.bn".into()], 0, 0).with_exec_capture_limit(1024);
    let (_, output) = run_with_host(&limit_source, "", &host).expect("E10 limit");
    assert_eq!(output, "limit 8\n");
    // shared helper retained for suite
}

/// E11 — POSIX signal termination → ReturnCode = -signal (D-H1-01).
#[cfg(unix)]
#[test]
fn host_exec_e11_signal_termination_negative_return_code() {
    let _guard = host_exec_test_lock();
    let helper = build_host_exec_helper();
    let helper_path = bn_path(&helper);
    let source = format!(
        r#"IMPORT HOST.Exec AS Exec
FUNCTION Start() AS VOID
LET r AS Exec.Result OR Error = Exec.Run("{helper}", ["signal"])
IF r IS Error THEN
PRINT "err", r.Code
ELSE
PRINT "sig", r.ReturnCode
END IF
END FUNCTION
"#,
        helper = helper_path
    );
    let (_, output) = run(&source, "").expect("execute HOST.Exec E11");
    // SIGTERM = 15 → ReturnCode = -15
    assert_eq!(output, "sig -15\n");
    // shared helper retained for suite
}

/// E12 — cwd inheritance, environment inheritance, spaces in executable path.
#[test]
fn host_exec_e12_cwd_env_and_spaced_path() {
    let _guard = host_exec_test_lock();
    let helper = build_host_exec_helper();
    let spaced_dir = unique_temp("host exec dir");
    fs::create_dir_all(&spaced_dir).expect("mkdir spaced");
    let spaced_helper = spaced_dir.join("helper bin");
    fs::copy(&helper, &spaced_helper).expect("copy helper to spaced path");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&spaced_helper).expect("meta").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&spaced_helper, perms).expect("chmod spaced");
    }
    let spaced_path = bn_path(&spaced_helper);
    // Printed text, not a BN literal: no escaping.
    let cwd_expected = std::env::current_dir().expect("cwd").display().to_string();
    // Prove inheritance without mutating process environment (crate forbids
    // unsafe set_var): HOME is present on supported developer hosts.
    let home = std::env::var("HOME").expect("HOME must be set for E12 inheritance evidence");

    let source = format!(
        r#"IMPORT HOST.Exec AS Exec
FUNCTION Start() AS VOID
LET cwd AS Exec.Result OR Error = Exec.Run("{spaced}", ["cwd"])
IF cwd IS Error THEN
PRINT "cwd-err", cwd.Code
ELSE
PRINT "cwd", cwd.Stdout
END IF
LET env AS Exec.Result OR Error = Exec.Run("{spaced}", ["env", "HOME"])
IF env IS Error THEN
PRINT "env-err", env.Code
ELSE
PRINT "env", env.Stdout
END IF
END FUNCTION
"#,
        spaced = spaced_path
    );
    let (_, output) = run(&source, "").expect("execute HOST.Exec E12");
    assert_eq!(output, format!("cwd {cwd_expected}\nenv {home}\n"));
    // shared helper retained for suite
    let _ = fs::remove_file(&spaced_helper);
    let _ = fs::remove_dir_all(&spaced_dir);
}

/// E13 — repeated calls reclaim child resources; Result is a value record
/// (not a CLASS reference), so RELEASE is intentionally not applicable here.
/// Full ARC/RELEASE parity for Result copies remains an open 4.2/4.3 note.
#[test]
fn host_exec_e13_repeated_calls_and_release() {
    let _guard = host_exec_test_lock();
    let helper = build_host_exec_helper();
    let helper_path = bn_path(&helper);
    let source = format!(
        r#"IMPORT HOST.Exec AS Exec
FUNCTION Start() AS VOID
LET last AS INT64 = 0
FOR i AS INTEGER = 1 TO 8
LET r AS Exec.Result OR Error = Exec.Run("{helper}", ["stdout", "x"])
IF r IS Error THEN
PRINT "err", r.Code
STOP 1
END IF
IF r.ReturnCode <> 0 THEN
PRINT "bad", r.ReturnCode
STOP 1
END IF
last = r.ReturnCode
LET copy AS Exec.Result OR Error = r
IF copy IS Exec.Result THEN
IF copy.Stdout <> "x" THEN
PRINT "copy-bad"
STOP 1
END IF
END IF
END FOR
PRINT "done", last
END FUNCTION
"#,
        helper = helper_path
    );
    let (_, output) = run(&source, "").expect("execute HOST.Exec E13");
    assert_eq!(output, "done 0\n");
    // shared helper retained for suite
}

/// E14 — reduced policy timeout returns TIMEOUT=9; child is reaped (no hang).
#[test]
fn host_exec_e14_timeout_returns_error() {
    let _guard = host_exec_test_lock();
    let helper = build_host_exec_helper();
    let helper_path = bn_path(&helper);
    let source = format!(
        r#"IMPORT HOST.Exec AS Exec
FUNCTION Start() AS VOID
LET r AS Exec.Result OR Error = Exec.Run("{helper}", ["block"])
IF r IS Error THEN
PRINT "timeout", r.Code
ELSE
PRINT "unexpected", r.ReturnCode
END IF
END FUNCTION
"#,
        helper = helper_path
    );
    let host = fixed(vec!["runtime.bn".into()], 0, 0).with_exec_timeout_secs(1);
    let (_, output) = run_with_host(&source, "", &host).expect("execute HOST.Exec E14");
    assert_eq!(output, "timeout 9\n");
    // shared helper retained for suite
}
