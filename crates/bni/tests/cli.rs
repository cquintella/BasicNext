// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Integration tests of the `bni` executable relocated verbatim from the
//! root `tests/cli.rs` (bucket 0.6.0, 2.4 / D-060-05): the command words are
//! unchanged, only the binary and the working directory differ.

// Multi-line raw-string BN program templates read more clearly with named
// placeholders than with inlined path expressions.
#![allow(clippy::uninlined_format_args)]

use std::{
    collections::HashMap,
    fs,
    io::Write,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

const WORKSPACE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// Runs `bni` from the workspace root (the fixtures use workspace-relative
/// paths; crate integration tests start in the crate directory).
fn bni() -> Command {
    let _ = std::env::set_current_dir(WORKSPACE);
    let mut command = Command::new(env!("CARGO_BIN_EXE_bni"));
    command.current_dir(WORKSPACE);
    command
}

/// Compiles the deterministic exec helper into `dir` and returns its path.
fn compile_exec_helper(dir: &std::path::Path) -> std::path::PathBuf {
    let helper = dir.join("helper");
    let status = Command::new("rustc")
        .args(["--edition=2021", "tests/fixtures/host_exec_helper.rs", "-o"])
        .arg(&helper)
        .status()
        .expect("compile host exec helper");
    assert!(status.success(), "rustc must build host_exec_helper");
    helper
}

/// 0.6.1 S1': prefix yields the new value, postfix the old value; the
/// expected lines are the ones fixed in `language/0.6/0.6.md`.
#[test]
fn increment_expressions_yield_new_value_prefix_and_old_value_postfix() {
    let output = bni()
        .args(["run", "tests/grammar/valid/increment-expression.bn"])
        .output()
        .expect("run increment fixture");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "101\n101\n102\n101\n101\n100\n200 101\n204 102\n0 2\n20 18 18\n"
    );
    let overflow = bni()
        .args(["eval", "LET x AS INT8 = 127\nPRINT x++"])
        .output()
        .expect("run overflow eval");
    assert_eq!(overflow.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&overflow.stderr).contains("NUMERIC_OVERFLOW"));
    assert!(
        overflow.stdout.is_empty(),
        "postfix overflow must not yield the old value"
    );
}

#[test]
fn eval_json_owns_both_process_channels_even_with_verbosity() {
    let output = bni()
        .args(["eval", "-vv", "--format", "json", "PRINT 1"])
        .output()
        .expect("run bn eval JSON");
    assert_eq!(output.status.code(), Some(0));
    assert!(
        output.stderr.is_empty(),
        "stderr leaked: {:?}",
        output.stderr
    );
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one JSON envelope");
    assert_eq!(envelope["stdout"], "1\n");
    assert_eq!(envelope["diagnostics"].as_array().map(Vec::len), Some(0));
}

#[test]
fn eval_json_wraps_late_environment_policy_errors() {
    let output = bni()
        .args(["eval", "--format", "json", "PRINT 1"])
        .env("BN_FS_POLICY", "invalid-policy")
        .output()
        .expect("run bn eval with invalid policy");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one JSON envelope");
    assert_eq!(envelope["diagnostics"][0]["code"], "CONFIG_INVALID");
}

#[test]
fn eval_promotes_only_top_level_start_and_reports_one_warning() {
    let program = "FUNCTION Start() AS VOID\n    PRINT 1\nEND FUNCTION\n";
    let output = bni()
        .args(["eval", "--format", "json", program])
        .output()
        .expect("eval program");
    assert_eq!(output.status.code(), Some(0));
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one JSON envelope");
    assert_eq!(envelope["stdout"], "1\n");
    assert_eq!(envelope["diagnostics"].as_array().map(Vec::len), Some(1));
    assert_eq!(envelope["diagnostics"][0]["code"], "EVAL_START_PROMOTED");

    let nested =
        "CLASS C\n    FUNCTION Start() AS VOID\n        PRINT 1\n    END FUNCTION\nEND CLASS\n";
    let nested_output = bni()
        .args(["eval", "--format", "json", nested])
        .output()
        .expect("eval nested declaration");
    let nested_envelope: serde_json::Value =
        serde_json::from_slice(&nested_output.stdout).expect("one JSON envelope");
    assert!(
        nested_envelope["diagnostics"]
            .as_array()
            .is_some_and(|items| items
                .iter()
                .all(|item| item["code"] != "EVAL_START_PROMOTED"))
    );
}

#[test]
fn eval_does_not_promote_start_text_inside_string_or_comment() {
    let source = "// FUNCTION Start() AS VOID\nPRINT \"FUNCTION Start() AS VOID\"\n";
    let output = bni()
        .args(["eval", "--format", "json", source])
        .output()
        .expect("eval text containing Start");
    assert_eq!(output.status.code(), Some(0));
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one JSON envelope");
    assert_eq!(envelope["stdout"], "FUNCTION Start() AS VOID\n");
    assert_eq!(envelope["diagnostics"].as_array().map(Vec::len), Some(0));
}

#[test]
fn check_valid_program_exits_zero() {
    let status = bni()
        .args(["check", "examples/hello.bn"])
        .status()
        .expect("run bn check");
    assert_eq!(status.code(), Some(0));
}

#[test]
fn eval_rejects_imports_after_the_leading_import_block() {
    let output = bni()
        .args([
            "eval",
            "--format",
            "json",
            "PRINT 1\nIMPORT BNMath AS M\nPRINT 2",
        ])
        .output()
        .expect("run eval with late import");
    assert_eq!(output.status.code(), Some(1));
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one JSON envelope");
    assert_eq!(envelope["diagnostics"][0]["code"], "E0100");
    assert_eq!(envelope["diagnostics"][0]["labels"][0]["start"]["line"], 2);
    assert_eq!(
        envelope["diagnostics"][0]["labels"][0]["start"]["offset"],
        8
    );
}

#[test]
fn eval_allows_leading_block_comments_before_imports() {
    for source in [
        "/* heading */\nIMPORT BNMath AS M\nPRINT 1",
        "/* heading\ncontinued */\nIMPORT BNMath AS M\nPRINT 1",
        "/* heading */ IMPORT BNMath AS M\nPRINT 1",
        "/* heading\n*/ IMPORT BNMath AS M\nPRINT 1",
    ] {
        let output = bni()
            .args(["eval", "--format", "json", source])
            .output()
            .expect("run eval with leading block comment");
        assert_eq!(output.status.code(), Some(0));
        let envelope: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("one JSON envelope");
        assert_eq!(envelope["stdout"], "1\n");
    }
}

#[test]
fn eval_stdin_source_is_consumed_before_runtime_input() {
    let mut child = bni()
        .args(["eval", "--stdin", "--format", "json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn eval stdin");
    child
        .stdin
        .take()
        .expect("stdin pipe")
        .write_all(
            b"LET value AS STRING OR EOF = INPUT()\nIF value IS EOF THEN\n PRINT \"EOF\"\nEND IF\n",
        )
        .expect("write source");
    let output = child.wait_with_output().expect("wait eval stdin");
    assert_eq!(output.status.code(), Some(0));
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one JSON envelope");
    assert_eq!(envelope["stdout"], "EOF\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn eval_preserves_host_args_after_separator() {
    let output = bni()
        .args([
            "eval",
            "--format",
            "json",
            "PRINT HOST.Args[1]",
            "--",
            "payload",
        ])
        .output()
        .expect("run eval with program arguments");
    assert_eq!(output.status.code(), Some(0));
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one JSON envelope");
    assert_eq!(envelope["stdout"], "payload\n");
}

#[test]
fn eval_warning_policy_controls_promotion_execution() {
    let source = "FUNCTION Start() AS VOID\n    PRINT 1\nEND FUNCTION\n";
    let denied = bni()
        .args([
            "eval",
            "--format",
            "json",
            "--deny",
            "EVAL_START_PROMOTED",
            source,
        ])
        .output()
        .expect("deny promotion");
    assert_eq!(denied.status.code(), Some(1));
    let denied_envelope: serde_json::Value =
        serde_json::from_slice(&denied.stdout).expect("one JSON envelope");
    assert_eq!(denied_envelope["stdout"], "");
    assert_eq!(denied_envelope["diagnostics"][0]["severity"], "error");

    let allowed = bni()
        .args([
            "eval",
            "--format",
            "json",
            "--allow",
            "EVAL_START_PROMOTED",
            source,
        ])
        .output()
        .expect("allow promotion");
    assert_eq!(allowed.status.code(), Some(0));
    let allowed_envelope: serde_json::Value =
        serde_json::from_slice(&allowed.stdout).expect("one JSON envelope");
    assert_eq!(
        allowed_envelope["diagnostics"].as_array().map(Vec::len),
        Some(0)
    );
}

#[test]
fn eval_rejects_conflicting_source_forms_in_json() {
    for args in [
        vec!["eval", "--format", "json"],
        vec!["eval", "--format", "json", "--stdin", "PRINT 1"],
        vec!["eval", "--format", "json", "--session", "PRINT 1"],
    ] {
        let output = bni().args(args).output().expect("run invalid eval form");
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stderr.is_empty());
        let envelope: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("one JSON envelope");
        assert_eq!(envelope["ok"], false);
        assert_eq!(envelope["diagnostics"][0]["code"], "CONFIG_INVALID");
    }
}

#[test]
fn eval_supports_scalar_values_and_stop_status_without_protocol_leakage() {
    for source in ["PRINT TRUE", "PRINT \"á\"", "PRINT [1,2,3]"] {
        let output = bni()
            .args(["eval", "--format", "json", source])
            .output()
            .expect("eval scalar");
        assert_eq!(output.status.code(), Some(0));
        let envelope: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("one JSON envelope");
        assert!(
            envelope["stdout"]
                .as_str()
                .is_some_and(|text| !text.is_empty())
        );
        assert!(output.stderr.is_empty());
    }
    let stopped = bni()
        .args(["eval", "--format", "json", "PRINT 1\nPRINT 2\nSTOP 7"])
        .output()
        .expect("eval stop");
    assert_eq!(stopped.status.code(), Some(7));
    let envelope: serde_json::Value =
        serde_json::from_slice(&stopped.stdout).expect("one JSON envelope");
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["stdout"], "1\n2\n");
}

#[test]
fn eval_preserves_partial_output_before_an_unhandled_runtime_error() {
    let output = bni()
        .args(["eval", "--format", "json", "PRINT 1\nPRINT 1 DIV 0"])
        .output()
        .expect("eval runtime failure");
    assert_eq!(output.status.code(), Some(1));
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("JSON envelope");
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["stdout"], "1\n");
    assert_eq!(envelope["diagnostics"][0]["code"], "DIVISION_BY_ZERO");
    assert!(output.stderr.is_empty());
}

#[test]
fn eval_treats_a_source_starting_with_dash_as_source_text() {
    let output = bni()
        .args(["eval", "--format", "json", "-1"])
        .output()
        .expect("eval negative expression");
    assert_eq!(output.status.code(), Some(0));
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("JSON envelope");
    assert_eq!(envelope["stdout"], "-1\n");
}

#[test]
fn eval_reports_unknown_import_and_import_cycle_with_stable_codes() {
    let missing = bni()
        .args(["eval", "--format", "json", "IMPORT Missing AS M\nPRINT 1"])
        .output()
        .expect("unknown import");
    assert_eq!(missing.status.code(), Some(1));
    let missing_json: serde_json::Value =
        serde_json::from_slice(&missing.stdout).expect("JSON envelope");
    assert_eq!(missing_json["diagnostics"][0]["code"], "MODULE_NOT_FOUND");

    let root = std::env::temp_dir().join(format!("bn-eval-cycle-{}", std::process::id()));
    fs::create_dir_all(&root).expect("cycle directory");
    fs::write(root.join("A.bn"), "IMPORT B AS B\n").expect("module A");
    fs::write(root.join("B.bn"), "IMPORT A AS A\n").expect("module B");
    let entry = root.join("main.bn");
    fs::write(
        &entry,
        "IMPORT A AS A\nFUNCTION Start() AS VOID\nEND FUNCTION\n",
    )
    .expect("cycle entry");
    let cycle = bni()
        .args(["run", entry.to_str().expect("entry path")])
        .output()
        .expect("import cycle");
    assert_eq!(cycle.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&cycle.stderr).contains("IMPORT_CYCLE"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn eval_json_preserves_crlf_diagnostic_coordinates() {
    let mut child = bni()
        .args(["eval", "--stdin", "--format", "json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn CRLF eval");
    child
        .stdin
        .take()
        .expect("stdin pipe")
        .write_all(b"PRINT @\r\nPRINT 2\r\n")
        .expect("write CRLF source");
    let output = child.wait_with_output().expect("wait CRLF eval");
    assert_eq!(output.status.code(), Some(1));
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one JSON envelope");
    let label = &envelope["diagnostics"][0]["labels"][0];
    assert_eq!(label["start"]["line"], 1);
    assert_eq!(label["start"]["offset"], 6);
    assert!(output.stderr.is_empty());
}

#[test]
fn selected_config_applies_relative_diagnostic_overlay() {
    let root = std::env::temp_dir().join(format!("bn-diag-cli-{}", std::process::id()));
    let messages = root.join("messages");
    fs::create_dir_all(&messages).expect("diagnostic overlay directory");
    fs::write(
        root.join("config.toml"),
        "[diagnostics]\ndir = \"messages\"\n",
    )
    .expect("diagnostic config");
    fs::write(
        messages.join("parse.ftl"),
        "parse-error = Expected {$expected} in {$context}.\n    .title = Custom syntax title\n    .code = E0100\n",
    )
    .expect("diagnostic overlay");
    let output = bni()
        .args([
            "check",
            "--config",
            root.join("config.toml").to_str().expect("config path"),
            "tests/grammar/invalid/untyped-let.bn",
        ])
        .output()
        .expect("check with overlay");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Custom syntax title"));
    fs::remove_dir_all(root).expect("remove diagnostic fixture");
}

#[test]
fn malformed_diagnostic_overlay_is_an_eager_stable_tool_error() {
    let root = std::env::temp_dir().join(format!("bn-diag-invalid-{}", std::process::id()));
    let messages = root.join("messages");
    fs::create_dir_all(&messages).expect("diagnostic overlay directory");
    fs::write(
        root.join("config.toml"),
        "[diagnostics]\ndir = \"messages\"\n",
    )
    .expect("diagnostic config");
    fs::write(messages.join("parse.ftl"), "not valid Fluent\n")
        .expect("invalid diagnostic overlay");
    let output = bni()
        .args([
            "check",
            "--config",
            root.join("config.toml").to_str().expect("config path"),
            "examples/hello.bn",
        ])
        .output()
        .expect("check with invalid overlay");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error[CONFIG_INVALID]"));
    assert!(stderr.contains("cannot load diagnostic catalog"));
    fs::remove_dir_all(root).expect("remove invalid diagnostic fixture");
}

#[test]
fn missing_diagnostic_overlay_is_an_eager_stable_tool_error() {
    let root = std::env::temp_dir().join(format!("bn-diag-missing-{}", std::process::id()));
    fs::create_dir_all(&root).expect("diagnostic config directory");
    fs::write(
        root.join("config.toml"),
        "[diagnostics]\ndir = \"missing\"\n",
    )
    .expect("diagnostic config");
    let output = bni()
        .args([
            "check",
            "--config",
            root.join("config.toml").to_str().expect("config path"),
            "examples/hello.bn",
        ])
        .output()
        .expect("check with missing overlay");
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("error[CONFIG_INVALID]"));
    fs::remove_dir_all(root).expect("remove missing diagnostic fixture");
}

#[test]
fn check_accepts_rpn_member_vector_assignment() {
    let output = bni()
        .args(["check", "examples/rpn-calculator.bn"])
        .output()
        .expect("check RPN calculator");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_accepts_nullable_returns_in_linear_collections() {
    let output = bni()
        .args(["check", "examples/linear_collections.bn"])
        .output()
        .expect("check linear collections");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_accepts_scalar_float_initializer_in_div_example() {
    let output = bni()
        .args(["check", "examples/div.bn"])
        .output()
        .expect("check division example");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_accepts_dispatch_task_returning_void() {
    let output = bni()
        .args(["check", "examples/parallel_work.bn"])
        .output()
        .expect("check parallel work example");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_and_lsp_report_the_same_multi_file_diagnostic_owner() {
    let directory = std::env::temp_dir().join(format!("bn-cli-lsp-parity-{}", std::process::id()));
    fs::create_dir_all(&directory).expect("create parity fixture directory");
    let main_path = directory.join("main.bn");
    let module_path = directory.join("Module.bn");
    let main_text = "IMPORT Module AS Module\nFUNCTION Start() AS INTEGER\nRETURN Module.Value()\nEND FUNCTION\n";
    let module_text = "EXPORT FUNCTION Value() AS INTEGER\nRETURN \"bad\"\nEND FUNCTION\n";
    fs::write(&main_path, main_text).expect("write main parity fixture");
    fs::write(&module_path, module_text).expect("write module parity fixture");

    let cli = bni()
        .args(["check", main_path.to_str().expect("main path")])
        .output()
        .expect("run CLI parity check");
    assert_eq!(cli.status.code(), Some(1));
    let cli_stderr = String::from_utf8_lossy(&cli.stderr);
    assert!(cli_stderr.contains("TYPE_MISMATCH"));
    assert!(cli_stderr.contains("Module.bn:2"), "{cli_stderr}");

    let main_uri = format!("file://{}", main_path.display());
    let module_uri = format!("file://{}", module_path.display());
    let documents = HashMap::from([
        (
            main_uri,
            bn_source::SourceFile::new(format!("file://{}", main_path.display()), main_text),
        ),
        (
            module_uri.clone(),
            bn_source::SourceFile::new(format!("file://{}", module_path.display()), module_text),
        ),
    ]);
    let mut session = bn_frontend::frontend_session::FrontendSession::default();
    let diagnostics = bn_lsp::diagnostics_for_documents(&main_path, &documents, &mut session);
    let module_diagnostics = diagnostics.get(&module_uri).expect("module diagnostics");
    assert_eq!(module_diagnostics.len(), 1);
    assert_eq!(
        module_diagnostics[0].code,
        Some(lsp_types::NumberOrString::String("TYPE_MISMATCH".into()))
    );
    assert_eq!(module_diagnostics[0].range.start.line, 1);
    assert_eq!(module_diagnostics[0].range.start.character, 7);
}

#[test]
fn warning_policy_controls_unreachable_diagnostic_end_to_end() {
    let default = bni()
        .args(["check", "tests/grammar/valid/unreachable-warning.bn"])
        .output()
        .expect("run default warning check");
    assert_eq!(default.status.code(), Some(0));
    let default_stderr = String::from_utf8_lossy(&default.stderr);
    assert!(default_stderr.contains("warning[UNREACHABLE_CODE]"));

    let allowed = bni()
        .args([
            "check",
            "--allow",
            "UNREACHABLE_CODE",
            "tests/grammar/valid/unreachable-warning.bn",
        ])
        .output()
        .expect("run allowed warning check");
    assert_eq!(allowed.status.code(), Some(0));
    assert!(!String::from_utf8_lossy(&allowed.stderr).contains("UNREACHABLE_CODE"));

    let denied = bni()
        .args([
            "check",
            "--deny",
            "UNREACHABLE_CODE",
            "tests/grammar/valid/unreachable-warning.bn",
        ])
        .output()
        .expect("run denied warning check");
    assert_eq!(denied.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&denied.stderr).contains("error[UNREACHABLE_CODE]"));
}

#[test]
fn warning_flags_cannot_demote_hard_type_errors() {
    let path = std::env::temp_dir().join(format!(
        "basicnext-hard-error-policy-{}.bn",
        std::process::id()
    ));
    fs::write(
        &path,
        "FUNCTION Start() AS VOID\nDIM value AS INTEGER\nvalue = \"bad\"\nEND FUNCTION\n",
    )
    .expect("write hard-error fixture");
    let output = bni()
        .args(["check", "--allow", "TYPE_MISMATCH"])
        .arg(&path)
        .output()
        .expect("run hard-error warning policy check");
    let _ = fs::remove_file(&path);
    assert_ne!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stderr).contains("TYPE_MISMATCH"));
}

#[test]
fn cli_warning_policy_overrides_config_per_code() {
    let config_path = std::env::temp_dir().join(format!(
        "basicnext-warning-policy-{}.toml",
        std::process::id()
    ));
    fs::write(
        &config_path,
        "[warnings]\ndefault = \"warn\"\n[warnings.levels]\nUNUSED_BINDING = \"error\"\n",
    )
    .expect("write warning policy config");

    let config_deny = bni()
        .args([
            "check",
            "--config",
            config_path.to_str().expect("config path"),
            "tests/grammar/valid/unused-binding-warning.bn",
        ])
        .output()
        .expect("run config warning policy");
    assert_eq!(
        config_deny.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&config_deny.stderr)
    );
    assert!(String::from_utf8_lossy(&config_deny.stderr).contains("error[UNUSED_BINDING]"));

    let cli_allow = bni()
        .args([
            "check",
            "--config",
            config_path.to_str().expect("config path"),
            "--allow",
            "UNUSED_BINDING",
            "tests/grammar/valid/unused-binding-warning.bn",
        ])
        .output()
        .expect("run CLI-overridden warning policy");
    let _ = fs::remove_file(&config_path);
    assert_eq!(cli_allow.status.code(), Some(0));
    assert!(!String::from_utf8_lossy(&cli_allow.stderr).contains("UNUSED_BINDING"));
}

#[test]
fn warning_analysis_emits_unused_binding() {
    let output = bni()
        .args(["check", "tests/grammar/valid/unused-binding-warning.bn"])
        .output()
        .expect("run unused binding check");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stderr).contains("warning[UNUSED_BINDING]"));
}

#[test]
fn warning_analysis_exempts_public_exported_class_bindings() {
    let output = bni()
        .args(["check", "examples/clock.bn"])
        .output()
        .expect("run clock check");
    assert_eq!(output.status.code(), Some(0));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("warning[UNUSED_BINDING]"));
}

#[test]
fn warning_analysis_emits_unused_import() {
    let output = bni()
        .args(["check", "tests/grammar/valid/unused-import-warning.bn"])
        .output()
        .expect("run unused import check");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stderr).contains("warning[UNUSED_IMPORT]"));
}

#[test]
fn check_network_client_server_examples_exit_zero() {
    let status = bni()
        .args(["check", "examples/socket.bn"])
        .status()
        .expect("run bn check");
    assert_eq!(status.code(), Some(0));
}

#[test]
fn check_icmp_ping_example_exits_zero() {
    let status = bni()
        .args(["check", "examples/icmp-ping.bn"])
        .status()
        .expect("check ICMP ping example");
    assert_eq!(status.code(), Some(0));
}

#[test]
fn socket_example_help_exits_zero() {
    let output = bni()
        .args(["run", "examples/socket.bn", "--", "--help"])
        .output()
        .expect("run socket example help");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage:"));
}

#[test]
fn socket_examples_exchange_tcp_and_udp_messages() {
    for protocol in ["--tcp", "--udp"] {
        for family in [None, Some("--ipv6")] {
            let log = format!("socket-{protocol}-{family:?}.jsonl");
            let _ = fs::remove_file(&log);
            let mut server_command = bni();
            server_command.args([
                "run",
                "examples/socket.bn",
                "--",
                protocol,
                "--server",
                "--log",
                &log,
            ]);
            let mut client_command = bni();
            client_command.args(["run", "examples/socket.bn", "--", protocol, "--client"]);
            if let Some(family) = family {
                server_command.arg(family);
                client_command.arg(family);
            }
            let server_process = server_command
                .stdout(Stdio::piped())
                .spawn()
                .expect("start server example");
            // A debug `bn run` takes ~85 ms to reach the listen call; 100 ms
            // raced it and failed under load. UDP has nothing to probe, so wait.
            thread::sleep(Duration::from_millis(1000));
            let client = client_command.output().expect("run client example");
            let server = server_process
                .wait_with_output()
                .expect("wait for server example");
            if String::from_utf8_lossy(&server.stdout).contains("Operation not permitted") {
                let _ = fs::remove_file(&log);
                return;
            }
            assert_eq!(
                client.status.code(),
                Some(0),
                "client={client:?}; server={server:?}"
            );
            assert_eq!(server.status.code(), Some(0), "server={server:?}");
            assert!(String::from_utf8_lossy(&client.stdout).contains("reply verified"));
            assert!(String::from_utf8_lossy(&server.stdout).contains("request accepted"));
            let log_contents = fs::read_to_string(&log).expect("server connection log");
            assert!(log_contents.contains("connection accepted"));
            let _ = fs::remove_file(log);
        }
    }
}

#[test]
fn language_error_exits_one() {
    let status = bni()
        .args(["check", "tests/grammar/invalid/untyped-let.bn"])
        .status()
        .expect("run bn check");
    assert_eq!(status.code(), Some(1));
}

#[test]
fn missing_source_exits_two() {
    let status = bni()
        .args(["check", "no-such-file.bn"])
        .status()
        .expect("run bn check");
    assert_eq!(status.code(), Some(2));
}

#[test]
fn unknown_option_exits_two() {
    let status = bni()
        .args(["check", "--nope", "examples/hello.bn"])
        .status()
        .expect("run bn check");
    assert_eq!(status.code(), Some(2));
}

/// 4.4 — `bn eval` runs a HOST.Exec program and captures the child's stdout in
/// the JSON envelope's program-stdout field. The captured stream must not leak
/// onto the process channels or corrupt the single JSON object (D-T1-03).
#[test]
fn eval_host_exec_produces_structured_json_without_channel_corruption() {
    let base = std::env::temp_dir().join(format!("basicnext-eval-exec-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("create eval exec dir");
    let helper = compile_exec_helper(&base);
    let helper_bn = helper.to_string_lossy().replace('\\', "\\\\");
    let program = format!(
        "IMPORT HOST.Exec AS Exec\nFUNCTION Start() AS VOID\nLET r AS Exec.Result OR Error = Exec.Run(\"{helper_bn}\", [\"stdout\", \"hi-from-child\"])\nIF r IS Error THEN\nPRINT \"error\", r.Code\nELSE\nPRINT r.Stdout\nEND IF\nEND FUNCTION\n"
    );
    // Explicit program mode: no promotion warning (D-T1-02), so the envelope's
    // diagnostics stay empty and the test isolates the exec-capture concern.
    let output = bni()
        .args(["eval", "--format", "json", "--mode", "program", &program])
        .output()
        .expect("run bn eval HOST.Exec");
    assert_eq!(output.status.code(), Some(0));
    assert!(
        output.stderr.is_empty(),
        "process stderr leaked: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("exactly one JSON envelope");
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["exit_code"], 0);
    assert_eq!(envelope["stdout"], "hi-from-child\n");
    assert_eq!(envelope["diagnostics"].as_array().map(Vec::len), Some(0));
    let _ = fs::remove_dir_all(&base);
}

/// 4.4 — a restricted profile denies HOST.Exec in eval exactly as in `bn run`
/// and native artifacts: the call returns Error 11 and the child side-effect
/// marker is never produced (GC-POL: denial is observed, not just reported).
#[test]
fn eval_host_exec_restricted_profile_denies_execution_and_side_effect() {
    let base = std::env::temp_dir().join(format!("basicnext-eval-deny-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("create eval deny dir");
    let helper = compile_exec_helper(&base);
    let helper_bn = helper.to_string_lossy().replace('\\', "\\\\");
    let marker = base.join("marker");
    let marker_bn = marker.to_string_lossy().replace('\\', "\\\\");
    let program = format!(
        "IMPORT HOST.Exec AS Exec\nFUNCTION Start() AS VOID\nLET r AS Exec.Result OR Error = Exec.Run(\"{helper_bn}\", [\"touch\", \"{marker_bn}\"])\nIF r IS Error THEN\nPRINT \"denied\", r.Code\nELSE\nPRINT \"ran\", r.ReturnCode\nEND IF\nEND FUNCTION\n"
    );
    let output = bni()
        .args(["eval", "--format", "json", &program])
        .env("BN_EXEC_POLICY", "deny")
        .output()
        .expect("run bn eval denied HOST.Exec");
    assert_eq!(output.status.code(), Some(0));
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("exactly one JSON envelope");
    assert_eq!(envelope["stdout"], "denied 11\n");
    assert!(
        !marker.exists(),
        "denied HOST.Exec must not spawn the child or create its side-effect marker"
    );
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn run_without_filesystem_rejects_an_unused_import() {
    let output = bni()
        .args([
            "run",
            "--no-filesystem",
            "tests/grammar/valid/filesystem-import-only.bn",
        ])
        .output()
        .expect("run import-only FileSystem without capability");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("HOST_CAPABILITY_UNAVAILABLE"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("ran"));
}

/// Bucket 0.5.2a (2.5) — `bn run` honours `BN_EXEC_CAPTURE_LIMIT` and
/// `BN_EXEC_TIMEOUT_MS` with the same observables as the native E10b/E14
/// fixtures: one policy parser on both backends.
#[test]
fn interpreter_honours_exec_ceiling_env_inputs_like_native() {
    let base = std::env::temp_dir().join(format!(
        "basicnext-interp-exec-policy-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("create directory");
    let helper = compile_exec_helper(&base);
    let helper_bn = helper.to_string_lossy().replace('\\', "\\\\");
    let limited = base.join("limited.bn");
    fs::write(
        &limited,
        format!(
            "IMPORT HOST.Exec AS Exec\nFUNCTION Start() AS VOID\nLET r AS Exec.Result OR Error = Exec.Run(\"{helper_bn}\", [\"stdout-bytes\", \"4096\"])\nIF r IS Error THEN\nPRINT \"limit\", r.Code\nELSE\nPRINT \"unexpected\", LEN(r.Stdout)\nEND IF\nEND FUNCTION\n"
        ),
    )
    .expect("write limited program");
    let run = bni()
        .args(["run"])
        .arg(&limited)
        .env("BN_EXEC_CAPTURE_LIMIT", "1024")
        .output()
        .expect("run limited program");
    assert_eq!(
        run.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "limit 8\n");

    let slow = base.join("slow.bn");
    fs::write(
        &slow,
        format!(
            "IMPORT HOST.Exec AS Exec\nFUNCTION Start() AS VOID\nLET r AS Exec.Result OR Error = Exec.Run(\"{helper_bn}\", [\"block\"])\nIF r IS Error THEN\nPRINT \"timeout\", r.Code\nELSE\nPRINT \"unexpected\", r.ReturnCode\nEND IF\nEND FUNCTION\n"
        ),
    )
    .expect("write slow program");
    let run = bni()
        .args(["run"])
        .arg(&slow)
        .env("BN_EXEC_TIMEOUT_MS", "200")
        .output()
        .expect("run slow program");
    assert_eq!(
        run.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "timeout 9\n");
    let _ = fs::remove_dir_all(&base);
}

/// `BNCrypto` digests reach the interpreter through the provider seam and match
/// the FIPS 180-4 example vectors mirrored in
/// `tests/fixtures/crypto/sha-vectors.json`. A failure here is an implementation
/// bug; the expectation is sourced and must not be edited to match output.
#[test]
fn bncrypto_digests_match_fips_vectors() {
    let output = bni()
        .args(["run", "tests/modules/bncrypto-digests/main.bn"])
        .output()
        .expect("run the BNCrypto digest fixture");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n\
         ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\n\
         cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce\
         47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e\n\
         ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
         2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f\n"
    );
}

/// `BNCrypto.Bytes` is an opaque buffer: it round-trips through text and hex,
/// reports its length, is freed by `RELEASE`, and rejects odd-length hex with
/// an `Error` instead of a truncated buffer.
#[test]
fn bncrypto_bytes_round_trip_and_reject_malformed_hex() {
    let output = bni()
        .args(["run", "tests/modules/bncrypto-bytes/main.bn"])
        .output()
        .expect("run the BNCrypto bytes fixture");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "3\n616263\n3\n000fff\nodd-length-rejected\n"
    );
}

/// The `BNCrypto.Bytes` subset both backends support, interpreted. The compiled
/// half of this pair lives in the `bnc` suite; the two must print the same.
#[test]
fn bncrypto_bytes_portable_subset_interprets() {
    let output = bni()
        .args(["run", "tests/modules/bncrypto-bytes-portable/main.bn"])
        .output()
        .expect("run the portable BNCrypto bytes fixture");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "3\n616263\n");
}

/// AEAD round-trips under the interpreter and fails closed on a changed AAD.
/// The ciphertexts come from python-cryptography (OpenSSL), independent of the
/// Rust crates under test, and are mirrored in
/// `tests/fixtures/crypto/aead-vectors.json`.
#[test]
fn bncrypto_aead_round_trips_and_rejects_tampering() {
    let output = bni()
        .args(["run", "tests/modules/bncrypto-aead/main.bn"])
        .output()
        .expect("run the BNCrypto AEAD fixture");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "f8cb8441c9b57bd8fd62663940f843f4e3cf1f\n616263\n25a94560bf99e02777736ea20551c7a02d3f86\n616263\ntamper-rejected\n"
    );
}

/// HMAC-SHA-256 matches an independently produced tag, verification
/// distinguishes a tampered message, and Argon2id yields a 32-byte tag while
/// refusing cost parameters below the algorithm's range.
#[test]
fn bncrypto_mac_and_kdf_interpret() {
    let output = bni()
        .args(["run", "tests/modules/bncrypto-mac-kdf/main.bn"])
        .output()
        .expect("run the BNCrypto MAC/KDF fixture");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "a60c859a6827c5ea576a48d8d368672fbfe4667c6a927428284a0cb3859cc1d6\nTRUE\nFALSE\n32\nargon-params-rejected\n"
    );
}

/// Ed25519 is deterministic, so the key and signature are byte-exact against
/// python-cryptography. P-256 is asserted by verification, since ECDSA nonces
/// differ between implementations.
#[test]
fn bncrypto_signatures_interpret() {
    let output = bni()
        .args(["run", "tests/modules/bncrypto-signatures/main.bn"])
        .output()
        .expect("run the BNCrypto signature fixture");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "23bc54912c1e6e92c4a86825c867e27ffdc555bffbd4244f17a26abfffee965d\ndb2b1ee48bbbb9af4a2e038310db3cd4e36e081b5537c348adfc95447fa8de2a7a971a889fd4c8d562a3a474db1e89f4f082cc3bc19f019951206f0b99aa8103\nTRUE\nFALSE\n65\nTRUE\nFALSE\n"
    );
}

/// ML-KEM-768 and ML-DSA-65. Encapsulation uses fresh randomness, so the
/// fixture compares the two sides internally and prints a stable verdict; the
/// object sizes are the FIPS 203 / FIPS 204 published parameters.
#[test]
fn bncrypto_post_quantum_interprets() {
    let output = bni()
        .args(["run", "tests/modules/bncrypto-pqc/main.bn"])
        .output()
        .expect("run the BNCrypto post-quantum fixture");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "1248\nkem-agree\n1984\n3309\nTRUE\nFALSE\n"
    );
}

/// The `BNJson` DOM slice: build an object, write and read a member, and reject
/// a missing key instead of yielding an empty STRING. The compiled twin is in
/// the `bnc` suite; the two must print the same.
#[test]
fn bnjson_dom_slice_interprets() {
    let output = bni()
        .args(["run", "tests/modules/bnjson-dom-slice/main.bn"])
        .output()
        .expect("run the BNJson DOM slice");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "pardal\nmissing-key-rejected\n{\"name\":\"pardal\"}\n"
    );
}

/// Typed scalars and inspection. Every read is fail-closed: `GetInteger` over a
/// BOOLEAN member is an `Error`, not a zero, and present-and-null is distinct
/// from absent.
#[test]
fn bnjson_dom_scalars_interpret() {
    let output = bni()
        .args(["run", "tests/modules/bnjson-dom-scalars/main.bn"])
        .output()
        .expect("run the BNJson scalar fixture");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "object\nTRUE\nFALSE\n4\n3\nTRUE\n3.5\nwrong-kind-rejected\nfloat-wrong-kind-rejected\nTRUE\n"
    );
}

/// Array Append* / Get*At / Set*At with OOB and wrong-kind fail-closed.
#[test]
fn bnjson_dom_array_interprets() {
    let output = bni()
        .args(["run", "tests/modules/bnjson-dom-array/main.bn"])
        .output()
        .expect("run the BNJson array fixture");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "array\n5\nalpha\n7\nTRUE\n1.25\n9\noob-rejected\noob-set-rejected\narray-wrong-kind-rejected\n"
    );
}

/// Nested `SetJson` (move) / `GetJson` / Clone / `AppendJson`.
#[test]
fn bnjson_dom_nested_interprets() {
    let output = bni()
        .args(["run", "tests/modules/bnjson-dom-nested/main.bn"])
        .output()
        .expect("run the BNJson nested fixture");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "pardal\nparent-intact\npardal\nmissing-nested-rejected\nv\n"
    );
}

/// Companion Encode/Decode round-trip (Bird + `BirdJson`). Domain module has no
/// `IMPORT BNJson`; the codec lives in the sibling companion (S-3).
#[test]
fn bnjson_companion_interprets() {
    let output = bni()
        .args(["run", "tests/modules/bnjson-companion/main.bn"])
        .output()
        .expect("run the BNJson companion fixture");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "{\"name\":\"eagle\",\"wings\":2}\neagle\n2\ndecode-error TRUE\n"
    );
}
