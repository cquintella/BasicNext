// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::{
    collections::HashMap,
    fs,
    io::Write,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

fn bn() -> Command {
    Command::new(env!("CARGO_BIN_EXE_bn"))
}

#[test]
fn check_valid_program_exits_zero() {
    let status = bn()
        .args(["check", "examples/hello.bn"])
        .status()
        .expect("run bn check");
    assert_eq!(status.code(), Some(0));
}

#[test]
fn check_accepts_rpn_member_vector_assignment() {
    let output = bn()
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
    let output = bn()
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
fn build_executes_nullable_integer_collection_results_like_interpret() {
    let interpreted = bn()
        .args(["run", "examples/linear_collections.bn"])
        .output()
        .expect("interpret linear collections");
    assert_eq!(interpreted.status.code(), Some(0));

    let artifact =
        std::env::temp_dir().join(format!("bn-linear-collections-{}", std::process::id()));
    let built = bn()
        .args([
            "build",
            "examples/linear_collections.bn",
            "-o",
            artifact.to_str().expect("artifact path"),
        ])
        .output()
        .expect("build linear collections");
    assert_eq!(
        built.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&built.stderr).contains("warning[UNUSED_BINDING]"),
        "used object fields must not be diagnosed as unused: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    let compiled = Command::new(&artifact)
        .output()
        .expect("run compiled linear collections");
    assert_eq!(compiled.status.code(), Some(0));
    assert_eq!(compiled.stdout, interpreted.stdout);
    let _ = fs::remove_file(artifact);
}

#[test]
#[allow(clippy::too_many_lines)] // One end-to-end sandbox fixture exercises one compiled policy artifact.
fn compiled_sandbox_enforces_read_write_roots_and_symlink_escape() {
    let base = std::env::temp_dir().join(format!("bn-sandbox-matrix-{}", std::process::id()));
    let read_root = base.join("read");
    let write_root = base.join("write");
    let outside_root = base.join("outside");
    fs::create_dir_all(&read_root).expect("create read root");
    fs::create_dir_all(&write_root).expect("create write root");
    fs::create_dir_all(&outside_root).expect("create outside root");
    fs::write(read_root.join("allowed.txt"), "allowed\n").expect("write allowed input");
    fs::write(outside_root.join("outside.txt"), "outside\n").expect("write outside input");

    let fixture = base.join("sandbox.bn");
    fs::write(
        &fixture,
        "IMPORT BNData AS Data\n\
IMPORT HOST.FileSystem AS FS\n\
FUNCTION Start() AS VOID\n\
    LET mode AS STRING = HOST.Args[1]\n\
    LET path AS STRING = HOST.Args[2]\n\
    IF mode = \"read\" THEN\n\
        LET file AS FS.File OR Error = FS.Open(path, FS.READ)\n\
        IF file IS Error THEN\n\
            PRINT \"open-error\"\n\
        ELSE\n\
            PRINT \"read-ok\"\n\
            file.Close()\n\
            DELETE file\n\
        END IF\n\
    ELSE\n\
        LET file AS FS.File OR Error = FS.Open(path, FS.WRITE)\n\
        IF file IS Error THEN\n\
            PRINT \"open-error\"\n\
        ELSE\n\
            LET frame AS Data.DataFrame = NEW Data.DataFrame()\n\
            LET names AS STRING[1] = [\"sandbox\"]\n\
            frame.AddStringColumn(\"value\", names)\n\
            LET result AS VOID OR Error = Data.WriteCSV(file, frame, TRUE, \",\")\n\
            PRINT \"write-ok\"\n\
            DELETE frame\n\
            file.Close()\n\
            DELETE file\n\
        END IF\n\
    END IF\n\
END FUNCTION\n",
    )
    .expect("write sandbox fixture");

    let artifact = base.join("sandbox");
    let built = bn()
        .args([
            "build",
            "--sandbox",
            "--read-root",
            read_root.to_str().expect("read root path"),
            "--write-root",
            write_root.to_str().expect("write root path"),
            fixture.to_str().expect("fixture path"),
            "-o",
            artifact.to_str().expect("artifact path"),
        ])
        .output()
        .expect("build sandbox fixture");
    assert_eq!(
        built.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );

    let run = |mode: &str, path: &std::path::Path| {
        Command::new(&artifact)
            .args([mode, path.to_str().expect("test path")])
            .output()
            .expect("run sandbox artifact")
    };

    let allowed_read = run("read", &read_root.join("allowed.txt"));
    assert_eq!(allowed_read.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&allowed_read.stdout).contains("read-ok"));

    let denied_read = run("read", &outside_root.join("outside.txt"));
    assert_eq!(denied_read.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&denied_read.stderr).contains("EXECUTION_POLICY_DENIED"));

    let allowed_write = write_root.join("created.txt");
    let write_result = run("write", &allowed_write);
    assert_eq!(write_result.status.code(), Some(0));
    assert!(allowed_write.is_file());
    assert!(
        fs::read_to_string(&allowed_write)
            .expect("read created file")
            .contains("sandbox")
    );

    let denied_write = outside_root.join("forbidden.txt");
    let denied_write_result = run("write", &denied_write);
    assert_eq!(denied_write_result.status.code(), Some(0));
    assert!(!denied_write.exists());
    assert!(
        String::from_utf8_lossy(&denied_write_result.stderr).contains("EXECUTION_POLICY_DENIED")
    );

    #[cfg(unix)]
    {
        let link = read_root.join("escape.txt");
        std::os::unix::fs::symlink(outside_root.join("outside.txt"), &link)
            .expect("create symlink escape");
        let escaped = run("read", &link);
        assert!(String::from_utf8_lossy(&escaped.stderr).contains("EXECUTION_POLICY_DENIED"));
    }

    let deny_all_artifact = base.join("sandbox-deny-all");
    let deny_all_build = bn()
        .args([
            "build",
            "--sandbox",
            fixture.to_str().expect("fixture path"),
            "-o",
            deny_all_artifact.to_str().expect("deny-all artifact path"),
        ])
        .output()
        .expect("build deny-all sandbox fixture");
    assert_eq!(
        deny_all_build.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&deny_all_build.stderr)
    );
    let deny_all = Command::new(&deny_all_artifact)
        .args([
            "read",
            read_root
                .join("allowed.txt")
                .to_str()
                .expect("allowed path"),
        ])
        .output()
        .expect("run deny-all sandbox artifact");
    assert!(String::from_utf8_lossy(&deny_all.stderr).contains("EXECUTION_POLICY_DENIED"));

    let _ = fs::remove_dir_all(base);
}

#[test]
#[allow(clippy::too_many_lines)] // One end-to-end matrix proves environment policy never widens either backend.
fn filesystem_policy_environment_only_narrows_interpreted_and_compiled_execution() {
    let base = std::env::temp_dir().join(format!(
        "bn-filesystem-environment-policy-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("create fixture directory");
    let readable = base.join("readable.txt");
    let writable = base.join("writable.txt");
    fs::write(&readable, "readable\n").expect("write readable fixture");

    let fixture = base.join("filesystem-policy.bn");
    fs::write(
        &fixture,
        "IMPORT HOST.FileSystem AS FS\n\
FUNCTION Start() AS VOID\n\
    LET mode AS STRING = HOST.Args[1]\n\
    LET path AS STRING = HOST.Args[2]\n\
    IF mode = \"read\" THEN\n\
        LET file AS FS.File OR Error = FS.Open(path, FS.READ)\n\
        IF file IS Error THEN\n\
            PRINT \"denied\"\n\
        ELSE\n\
            PRINT \"opened\"\n\
            file.Close()\n\
            DELETE file\n\
        END IF\n\
    ELSE\n\
        LET file AS FS.File OR Error = FS.Open(path, FS.WRITE)\n\
        IF file IS Error THEN\n\
            PRINT \"denied\"\n\
        ELSE\n\
            PRINT \"opened\"\n\
            file.Close()\n\
            DELETE file\n\
        END IF\n\
    END IF\n\
END FUNCTION\n",
    )
    .expect("write filesystem policy fixture");

    let interpreted_deny = bn()
        .env("BN_FS_POLICY", "deny")
        .args([
            "run",
            fixture.to_str().expect("fixture path"),
            "--",
            "read",
            readable.to_str().expect("readable path"),
        ])
        .output()
        .expect("run interpreted deny fixture");
    assert_eq!(
        interpreted_deny.status.code(),
        Some(1),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&interpreted_deny.stdout),
        String::from_utf8_lossy(&interpreted_deny.stderr)
    );
    assert!(
        String::from_utf8_lossy(&interpreted_deny.stderr).contains("HOST_CAPABILITY_UNAVAILABLE")
    );

    let interpreted_read_only = bn()
        .env("BN_FS_POLICY", "read-only")
        .args([
            "run",
            fixture.to_str().expect("fixture path"),
            "--",
            "write",
            writable.to_str().expect("writable path"),
        ])
        .output()
        .expect("run interpreted read-only fixture");
    assert_eq!(
        interpreted_read_only.status.code(),
        Some(2),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&interpreted_read_only.stdout),
        String::from_utf8_lossy(&interpreted_read_only.stderr)
    );
    assert!(
        String::from_utf8_lossy(&interpreted_read_only.stderr).contains("EXECUTION_POLICY_DENIED")
    );
    assert!(!writable.exists());

    let artifact = base.join("filesystem-policy");
    let built = bn()
        .args([
            "build",
            fixture.to_str().expect("fixture path"),
            "-o",
            artifact.to_str().expect("artifact path"),
        ])
        .output()
        .expect("build filesystem policy fixture");
    assert_eq!(
        built.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );

    let compiled_deny = Command::new(&artifact)
        .env("BN_FS_POLICY", "deny")
        .args(["read", readable.to_str().expect("readable path")])
        .output()
        .expect("run compiled deny fixture");
    assert_eq!(compiled_deny.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&compiled_deny.stdout).contains("denied"));

    let compiled_read_only = Command::new(&artifact)
        .env("BN_FS_POLICY", "read-only")
        .args(["write", writable.to_str().expect("writable path")])
        .output()
        .expect("run compiled read-only fixture");
    assert_eq!(compiled_read_only.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&compiled_read_only.stdout).contains("denied"));
    assert!(!writable.exists());

    let sandbox_artifact = base.join("filesystem-policy-sandbox");
    let sandbox_build = bn()
        .args([
            "build",
            "--sandbox",
            fixture.to_str().expect("fixture path"),
            "-o",
            sandbox_artifact.to_str().expect("sandbox artifact path"),
        ])
        .output()
        .expect("build sandbox filesystem policy fixture");
    assert_eq!(
        sandbox_build.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&sandbox_build.stderr)
    );
    let compiled_cannot_widen = Command::new(&sandbox_artifact)
        .env("BN_FS_POLICY", "read-only")
        .args(["read", readable.to_str().expect("readable path")])
        .output()
        .expect("run sandbox read-only fixture");
    assert_eq!(compiled_cannot_widen.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&compiled_cannot_widen.stdout).contains("denied"));

    let _ = fs::remove_dir_all(base);
}

#[test]
fn check_accepts_scalar_float_initializer_in_div_example() {
    let output = bn()
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
    let output = bn()
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

    let cli = bn()
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
            bn::source::SourceFile::new(format!("file://{}", main_path.display()), main_text),
        ),
        (
            module_uri.clone(),
            bn::source::SourceFile::new(format!("file://{}", module_path.display()), module_text),
        ),
    ]);
    let mut session = bn::frontend_session::FrontendSession::default();
    let diagnostics = bn::lsp::diagnostics_for_documents(&main_path, &documents, &mut session);
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
    let default = bn()
        .args(["check", "tests/grammar/valid/unreachable-warning.bn"])
        .output()
        .expect("run default warning check");
    assert_eq!(default.status.code(), Some(0));
    let default_stderr = String::from_utf8_lossy(&default.stderr);
    assert!(default_stderr.contains("warning[UNREACHABLE_CODE]"));

    let allowed = bn()
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

    let denied = bn()
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
fn warning_analysis_emits_unused_binding() {
    let output = bn()
        .args(["check", "tests/grammar/valid/unused-binding-warning.bn"])
        .output()
        .expect("run unused binding check");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stderr).contains("warning[UNUSED_BINDING]"));
}

#[test]
fn warning_analysis_exempts_public_exported_class_bindings() {
    let output = bn()
        .args(["check", "examples/clock.bn"])
        .output()
        .expect("run clock check");
    assert_eq!(output.status.code(), Some(0));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("warning[UNUSED_BINDING]"));
}

#[test]
fn warning_analysis_emits_unused_import() {
    let output = bn()
        .args(["check", "tests/grammar/valid/unused-import-warning.bn"])
        .output()
        .expect("run unused import check");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stderr).contains("warning[UNUSED_IMPORT]"));
}

#[test]
fn build_writes_companion_process_log_next_to_artifact() {
    let directory = std::env::temp_dir().join(format!("bn-process-log-cli-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("process log test directory");
    let output_path = directory.join("hello");
    let output = bn()
        .args([
            "build",
            "examples/hello.bn",
            "--log-level",
            "debug",
            "-o",
            output_path.to_str().expect("UTF-8 output path"),
        ])
        .output()
        .expect("build with process log");
    assert_eq!(output.status.code(), Some(0));
    let log_path = output_path.with_extension("log");
    let log = fs::read_to_string(&log_path).expect("companion process log");
    assert!(log.contains("phase=pipeline"));
    assert!(log.contains("event=start") && log.contains("event=end"));
    assert!(log.contains("phase=frontend") && log.contains("phase=lower"));
    assert!(log.contains("phase=validate_for") && log.contains("phase=llvm_emit"));
    assert!(log.contains("phase=link"));
    assert!(log.contains("phase=diagnostic") && log.contains("event=summary"));
    assert!(log.contains("detail=errors=0\\swarnings=0\\sexit="));
    assert!(log.contains("phase=config") && log.contains("event=snapshot"));
    assert!(log.contains("event=modules") && log.contains("phase=external"));
    fs::remove_dir_all(directory).expect("remove process log test directory");
}

#[test]
fn warn_level_process_log_keeps_warning_events_and_summary() {
    let directory =
        std::env::temp_dir().join(format!("bn-process-log-warn-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("warn log directory");
    let log_path = directory.join("warn.log");
    let output = bn()
        .args([
            "build",
            "tests/grammar/valid/unused-binding-warning.bn",
            "--log-level",
            "warn",
            "--log-file",
            log_path.to_str().expect("UTF-8 log path"),
        ])
        .output()
        .expect("build with warn-level process log");
    assert_eq!(output.status.code(), Some(0));
    let log = fs::read_to_string(&log_path).expect("warn-level process log");
    assert!(log.contains("level=warn phase=diagnostic event=emit detail=code=UNUSED_BINDING"));
    assert!(log.contains(
        "level=warn phase=diagnostic event=summary detail=errors=0\\swarnings=1\\sexit="
    ));
    assert!(!log.contains("level=info"));
    fs::remove_dir_all(directory).expect("remove warn log directory");
}

#[test]
fn failed_build_writes_partial_process_log_with_failure_stage() {
    let directory =
        std::env::temp_dir().join(format!("bn-process-log-fail-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("failed build log directory");
    let log_path = directory.join("failed.log");
    let output = bn()
        .args([
            "build",
            "tests/grammar/valid/struct-return-lifetime-deferred.bn",
            "--allow",
            "UNUSED_BINDING",
            "--log-level",
            "error",
            "--log-file",
            log_path.to_str().expect("UTF-8 log path"),
        ])
        .output()
        .expect("failed build with process log");
    assert_eq!(output.status.code(), Some(2));
    let log = fs::read_to_string(&log_path).expect("partial process log");
    assert!(log.contains("phase=validate_for") && log.contains("event=fail"));
    assert!(log.contains("detail=errors=1\\swarnings=0\\sexit="));
    assert!(log.contains("phase=pipeline") && log.contains("event=end"));
    assert!(!log.contains("level=info"));
    fs::remove_dir_all(directory).expect("remove failed build log directory");
}

#[test]
fn no_log_disables_companion_and_unwritable_log_is_a_tool_error() {
    let directory =
        std::env::temp_dir().join(format!("bn-process-log-policy-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("policy log directory");
    let output_path = directory.join("hello");
    let no_log = bn()
        .args([
            "build",
            "examples/hello.bn",
            "--no-log",
            "-o",
            output_path.to_str().expect("UTF-8 output path"),
        ])
        .output()
        .expect("build without log");
    assert_eq!(no_log.status.code(), Some(0));
    assert!(!output_path.with_extension("log").exists());

    let not_directory = directory.join("not-directory");
    fs::write(&not_directory, "file").expect("blocking file");
    let unwritable = bn()
        .args([
            "build",
            "examples/hello.bn",
            "--log-file",
            not_directory
                .join("build.log")
                .to_str()
                .expect("UTF-8 log path"),
        ])
        .output()
        .expect("build with unwritable log");
    assert_eq!(unwritable.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&unwritable.stderr).contains("PROCESS_LOG_WRITE"));
    fs::remove_dir_all(directory).expect("remove policy log directory");
}

#[test]
fn config_file_controls_process_log_level_and_destination() {
    let directory =
        std::env::temp_dir().join(format!("bn-process-log-config-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("config log directory");
    let config_path = directory.join("config.toml");
    let output_path = directory.join("hello");
    let log_path = directory.join("configured.log");
    fs::write(
        &config_path,
        format!(
            "[logging]\nlevel = \"error\"\nfile = \"{}\"\n",
            log_path.display()
        ),
    )
    .expect("write logging config");
    let output = bn()
        .args([
            "build",
            "tests/grammar/valid/struct-return-lifetime-deferred.bn",
            "--allow",
            "UNUSED_BINDING",
            "--config",
            config_path.to_str().expect("UTF-8 config path"),
            "-o",
            output_path.to_str().expect("UTF-8 output path"),
        ])
        .output()
        .expect("build with configured process log");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("TARGET_UNSUPPORTED_OP"));
    let log = fs::read_to_string(&log_path).expect("configured process log");
    assert!(log.contains("level=error phase=pipeline event=end"));
    assert!(log.contains("phase=validate_for") && log.contains("event=fail"));
    assert!(!log.contains("level=info"));
    assert!(!output_path.with_extension("log").exists());
    fs::remove_dir_all(directory).expect("remove config log directory");
}

#[test]
fn cli_log_file_overrides_configured_disabled_logging() {
    let directory = std::env::temp_dir().join(format!(
        "bn-process-log-config-disabled-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("config log directory");
    let config_path = directory.join("config.toml");
    let log_path = directory.join("cli.log");
    fs::write(
        &config_path,
        format!(
            "[logging]\nenabled = false\nfile = \"{}\"\n",
            directory.join("config.log").display()
        ),
    )
    .expect("write disabled logging config");

    let output = bn()
        .args([
            "build",
            "examples/hello.bn",
            "--config",
            config_path.to_str().expect("UTF-8 config path"),
            "--log-file",
            log_path.to_str().expect("UTF-8 log path"),
        ])
        .output()
        .expect("build with CLI logging override");
    assert_eq!(output.status.code(), Some(0));
    assert!(log_path.exists());
    assert!(!directory.join("config.log").exists());

    fs::remove_dir_all(directory).expect("remove config log directory");
}

#[test]
fn cli_help_and_version_advertise_0_4_7() {
    let help = bn().arg("--help").output().expect("run bn help");
    assert_eq!(help.status.code(), Some(0));
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(help.contains("lsp") && help.contains("dap"));
    let version = bn().arg("--version").output().expect("run bn version");
    assert_eq!(version.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&version.stdout).trim(), "bn 0.4.7");
}

#[test]
fn check_network_client_server_examples_exit_zero() {
    let status = bn()
        .args(["check", "examples/socket.bn"])
        .status()
        .expect("run bn check");
    assert_eq!(status.code(), Some(0));
}

#[test]
fn check_icmp_ping_example_exits_zero() {
    let status = bn()
        .args(["check", "examples/icmp-ping.bn"])
        .status()
        .expect("check ICMP ping example");
    assert_eq!(status.code(), Some(0));
}

#[test]
fn socket_example_help_exits_zero() {
    let output = bn()
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
            let mut server_command = bn();
            server_command.args([
                "run",
                "examples/socket.bn",
                "--",
                protocol,
                "--server",
                "--log",
                &log,
            ]);
            let mut client_command = bn();
            client_command.args(["run", "examples/socket.bn", "--", protocol, "--client"]);
            if let Some(family) = family {
                server_command.arg(family);
                client_command.arg(family);
            }
            let server_process = server_command
                .stdout(Stdio::piped())
                .spawn()
                .expect("start server example");
            thread::sleep(Duration::from_millis(100));
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
    let status = bn()
        .args(["check", "tests/grammar/invalid/untyped-let.bn"])
        .status()
        .expect("run bn check");
    assert_eq!(status.code(), Some(1));
}

#[test]
fn missing_source_exits_two() {
    let status = bn()
        .args(["check", "no-such-file.bn"])
        .status()
        .expect("run bn check");
    assert_eq!(status.code(), Some(2));
}

#[test]
fn unknown_option_exits_two() {
    let status = bn()
        .args(["check", "--nope", "examples/hello.bn"])
        .status()
        .expect("run bn check");
    assert_eq!(status.code(), Some(2));
}

#[test]
fn missing_command_exits_two() {
    let status = bn().status().expect("run bn");
    assert_eq!(status.code(), Some(2));
}

#[test]
fn build_emits_llvm_for_finite_constant_loop() {
    let output = bn()
        .args(["build", "examples/hello.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("@printf"));
}

#[test]
fn build_emits_llvm_for_empty_start() {
    let output = bn()
        .args(["build", "tests/grammar/valid/empty-start.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("define i32 @main(i32 %argc, ptr %argv)")
    );
}

#[test]
fn build_emits_integer_start_exit_code() {
    let output = bn()
        .args(["build", "tests/grammar/valid/start-exit-code.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("ret i32 %ret"));
}

#[test]
fn build_kmp_compiles_through_native_backend() {
    let check = bn()
        .args(["check", "examples/kmp.bn"])
        .output()
        .expect("check KMP");
    assert_eq!(check.status.code(), Some(0));
    let run = bn()
        .args(["run", "examples/kmp.bn"])
        .output()
        .expect("run KMP");
    assert_eq!(run.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&run.stdout).contains("Encontrado padrao no indice  10"));
    let output = bn()
        .args(["build", "examples/kmp.bn"])
        .output()
        .expect("run bn build for KMP");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    native_matches_interpreter("examples/kmp.bn");
}

#[test]
fn build_executes_multidimensional_vector_like_interpreter() {
    native_matches_interpreter("tests/grammar/valid/multidimensional-vectors.bn");
}

#[test]
fn build_reports_the_type_for_unsupported_allocation_lowering() {
    let output = bn()
        .args(["build", "tests/grammar/valid/pointer-void.bn"])
        .output()
        .expect("run pointer build");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("call ptr @malloc"));
}

#[test]
fn build_emits_llvm_for_integer_print() {
    let output = bn()
        .args(["build", "tests/grammar/valid/print-integer.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    let llvm = String::from_utf8_lossy(&output.stdout);
    assert!(llvm.contains("@printf"));
    assert!(llvm.contains("add i64 0, 42"));
}

#[test]
fn build_emits_multiple_integer_prints() {
    let output = bn()
        .args(["build", "tests/grammar/valid/print-integers.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    let llvm = String::from_utf8_lossy(&output.stdout);
    assert_eq!(llvm.matches("@printf").count(), 4);
    assert!(llvm.contains("add i64 0, 1"));
    assert!(llvm.contains("add i64 0, 2"));
    assert!(llvm.contains("add i64 0, 3"));
}

fn native_matches_interpreter(path: &str) {
    let output_path = std::env::temp_dir().join(format!(
        "basicnext-euclid-{}-{}",
        std::process::id(),
        path.replace(['/', '.'], "_")
    ));
    let _ = fs::remove_file(&output_path);
    let built = bn()
        .args([
            "build",
            path,
            "-o",
            output_path.to_str().expect("temporary path"),
        ])
        .output()
        .expect("run bn build");
    assert_eq!(
        built.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let compiled = Command::new(&output_path)
        .output()
        .expect("run compiled artifact");
    let interpreted = bn().args(["run", path]).output().expect("run interpreter");
    assert_eq!(compiled.status.code(), interpreted.status.code(), "{path}");
    assert_eq!(compiled.stdout, interpreted.stdout, "{path}");
    let _ = fs::remove_file(output_path);
}

fn native_matches_interpreter_with_input(path: &str, input: &str) {
    let output_path = std::env::temp_dir().join(format!(
        "basicnext-input-parity-{}-{}",
        std::process::id(),
        path.replace(['/', '.'], "_")
    ));
    let _ = fs::remove_file(&output_path);
    let built = bn()
        .args([
            "build",
            path,
            "-o",
            output_path.to_str().expect("temporary path"),
        ])
        .output()
        .expect("run bn build");
    assert_eq!(
        built.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );

    let mut compiled = Command::new(&output_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("run compiled artifact");
    compiled
        .stdin
        .take()
        .expect("compiled stdin")
        .write_all(input.as_bytes())
        .expect("write compiled stdin");
    let compiled = compiled
        .wait_with_output()
        .expect("wait for native artifact");

    let mut interpreted = bn()
        .args(["run", path])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("run interpreter");
    interpreted
        .stdin
        .take()
        .expect("interpreter stdin")
        .write_all(input.as_bytes())
        .expect("write interpreter stdin");
    let interpreted = interpreted
        .wait_with_output()
        .expect("wait for interpreter");

    assert_eq!(compiled.status.code(), interpreted.status.code(), "{path}");
    assert_eq!(compiled.stdout, interpreted.stdout, "{path}");
    let _ = fs::remove_file(output_path);
}

#[test]
fn build_lowers_euclidean_div_and_remainder_matching_interpreter() {
    native_matches_interpreter("tests/grammar/valid/build-euclidean-div.bn");
    native_matches_interpreter("tests/grammar/valid/build-euclidean-rem.bn");
    native_matches_interpreter("tests/grammar/valid/build-euclidean-runtime.bn");
    native_matches_interpreter("tests/grammar/valid/build-euclidean-overflow.bn");
    native_matches_interpreter("tests/grammar/valid/build-divide-zero.bn");
}

#[test]
fn build_lowers_power_shift_not_and_string_concat_matching_interpreter() {
    native_matches_interpreter("tests/grammar/valid/build-power-shift.bn");
    native_matches_interpreter("tests/grammar/valid/build-power-shift-runtime.bn");
    native_matches_interpreter("tests/grammar/valid/build-invalid-exponent.bn");
    native_matches_interpreter("tests/grammar/valid/build-invalid-shift.bn");
}

#[test]
fn build_lowers_print_expression_and_argument_separators_matching_interpreter() {
    native_matches_interpreter("tests/grammar/valid/print-separators.bn");
}

#[test]
fn build_executes_dispatch_examples_with_equivalent_results() {
    for path in [
        "examples/parallel_pi.bn",
        "examples/parallel_work.bn",
        "examples/dispatch_game_tournament.bn",
        "examples/dispatch_cellular_automaton.bn",
        "examples/dispatch_reliability_simulation.bn",
    ] {
        let interpreted = bn()
            .args(["run", path])
            .output()
            .expect("run dispatch example");
        assert_eq!(interpreted.status.code(), Some(0), "{path}");
        let artifact = std::env::temp_dir().join(format!(
            "basicnext-dispatch-{}-{}",
            std::process::id(),
            path.replace(['/', '.'], "_")
        ));
        let _ = fs::remove_file(&artifact);
        let built = bn()
            .args(["build", path, "-o", artifact.to_str().expect("UTF-8 path")])
            .output()
            .expect("build dispatch example");
        assert_eq!(
            built.status.code(),
            Some(0),
            "{path}: {}",
            String::from_utf8_lossy(&built.stderr)
        );
        let compiled = Command::new(&artifact)
            .output()
            .expect("execute dispatch artifact");
        assert_eq!(compiled.status.code(), Some(0), "{path}");
        let interpreted_lines = interpreted
            .stdout
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>();
        let compiled_lines = compiled
            .stdout
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>();
        assert_eq!(interpreted_lines.len(), compiled_lines.len(), "{path}");
        assert_eq!(interpreted_lines.last(), compiled_lines.last(), "{path}");
        let mut interpreted_prefix = interpreted_lines[..interpreted_lines.len() - 1].to_vec();
        let mut compiled_prefix = compiled_lines[..compiled_lines.len() - 1].to_vec();
        interpreted_prefix.sort_unstable();
        compiled_prefix.sort_unstable();
        assert_eq!(interpreted_prefix, compiled_prefix, "{path}");
        let _ = fs::remove_file(&artifact);
    }
}

#[test]
fn build_lowers_all_numeric_widths_and_checked_casts() {
    native_matches_interpreter("tests/grammar/valid/build-widths.bn");
    native_matches_interpreter("tests/grammar/valid/build-cast-overflow.bn");
    native_matches_interpreter("tests/grammar/valid/integer-narrowing-conversion.bn");
}

#[test]
fn build_lowers_host_clock_and_console_through_bn_rt() {
    native_matches_interpreter("tests/grammar/valid/build-clock.bn");
    native_matches_interpreter("tests/grammar/valid/cls-and-beep.bn");
    native_matches_interpreter("tests/grammar/valid/console-size.bn");
    native_matches_interpreter("tests/grammar/valid/console-print-at.bn");
}

#[test]
fn build_invokes_object_destructor_before_delete() {
    native_matches_interpreter("tests/grammar/valid/build-destructor-delete.bn");
}

#[test]
fn build_preserves_dataframe_error_values() {
    native_matches_interpreter("tests/grammar/valid/build-dataframe-errors.bn");
}

#[test]
fn build_lowers_host_net_resolve_through_bn_rt() {
    native_matches_interpreter("tests/grammar/valid/build-net-resolve.bn");
    native_matches_interpreter("tests/grammar/valid/build-net-endpoint.bn");
    native_matches_interpreter("tests/grammar/valid/build-net-udp-bind.bn");
    native_matches_interpreter("tests/grammar/valid/build-net-tcp-connect.bn");
    native_matches_interpreter("tests/grammar/valid/build-net-udp-close.bn");
    native_matches_interpreter("tests/grammar/valid/build-net-udp-send.bn");
    native_matches_interpreter("tests/grammar/valid/build-net-udp-receive.bn");
    native_matches_interpreter("tests/grammar/valid/build-net-udp-packet.bn");
    native_matches_interpreter("tests/grammar/valid/build-net-tcp-listen.bn");
    native_matches_interpreter("tests/grammar/valid/build-net-tcp-accept.bn");
}

#[test]
fn compiled_console_tty_errors_match_interpreter() {
    let path = "tests/grammar/valid/console-size.bn";
    let interpreted = bn().args(["run", path]).output().expect("run interpreter");
    assert_eq!(interpreted.status.code(), Some(1));
    let interpreted_err = String::from_utf8_lossy(&interpreted.stderr);
    assert!(
        interpreted_err.contains("HOST_CAPABILITY_UNAVAILABLE"),
        "{interpreted_err}"
    );
    assert!(
        interpreted_err.contains("window size requires a TTY"),
        "{interpreted_err}"
    );

    let output_path =
        std::env::temp_dir().join(format!("basicnext-console-tty-{}", std::process::id()));
    let _ = fs::remove_file(&output_path);
    let built = bn()
        .args(["build", path, "-o", output_path.to_str().expect("path")])
        .output()
        .expect("build console-size");
    assert_eq!(
        built.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let compiled = Command::new(&output_path)
        .output()
        .expect("run compiled console-size");
    let _ = fs::remove_file(&output_path);
    assert_eq!(compiled.status.code(), Some(1));
    let compiled_err = String::from_utf8_lossy(&compiled.stderr);
    assert!(
        compiled_err.contains("HOST_CAPABILITY_UNAVAILABLE"),
        "{compiled_err}"
    );
    assert!(
        compiled_err.contains("window size requires a TTY"),
        "{compiled_err}"
    );
}

#[test]
fn build_constant_folds_integer_expression() {
    let output = bn()
        .args(["build", "tests/grammar/valid/print-expression.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("add i64 0, 14"));
}

#[test]
fn build_constant_folds_unary_integer_expression() {
    let output = bn()
        .args(["build", "tests/grammar/valid/print-unary.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("add i64 0, -5"));
}

#[test]
fn build_constant_propagates_integer_binding() {
    let output = bn()
        .args(["build", "tests/grammar/valid/print-variable.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("add i32 0, 3"));
}

#[test]
fn build_emits_boolean_prints() {
    let output = bn()
        .args(["build", "tests/grammar/valid/print-boolean.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    let llvm = String::from_utf8_lossy(&output.stdout);
    assert!(llvm.contains("@.bn_true"));
    assert!(llvm.contains("@.bn_false"));
}

#[test]
fn build_emits_float_print() {
    let output = bn()
        .args(["build", "tests/grammar/valid/print-float.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("fadd double 0.0, 3.75"));
}

#[test]
fn build_emits_string_print() {
    let output = bn()
        .args(["build", "tests/grammar/valid/print-string.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Hello LLVM"));
}

#[test]
fn build_constant_folds_if_branch() {
    let output = bn()
        .args(["build", "tests/grammar/valid/print-if-constant.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    let llvm = String::from_utf8_lossy(&output.stdout);
    assert!(llvm.contains("br i1 %v0, label %b2, label %b3"));
    assert!(llvm.contains("add i64 0, 7"));
    assert!(llvm.contains("add i64 0, 9"));
}

#[test]
fn build_constant_folds_relational_if_branch() {
    let output = bn()
        .args(["build", "tests/grammar/valid/print-if-comparison.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    let llvm = String::from_utf8_lossy(&output.stdout);
    assert!(llvm.contains("br i1 %v0, label %b2, label %b3"));
    assert!(llvm.contains("add i64 0, 7"));
    assert!(llvm.contains("add i64 0, 9"));
}

#[test]
fn build_constant_folds_short_circuit_if_branch() {
    let output = bn()
        .args([
            "build",
            "tests/grammar/valid/print-if-boolean-expression.bn",
        ])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    let llvm = String::from_utf8_lossy(&output.stdout);
    assert!(llvm.contains("yes"));
    assert!(llvm.contains("no"));
    assert!(llvm.contains("br i1 %v1"));
}

#[test]
fn build_escapes_string_literal_for_llvm() {
    let output = bn()
        .args(["build", "tests/grammar/valid/print-string-escaped.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    let llvm = String::from_utf8_lossy(&output.stdout);
    assert!(llvm.contains("A\\22B\\5CC"));
}

#[test]
fn build_emits_native_artifact_when_output_is_given() {
    let output_path = std::env::temp_dir().join(format!("basicnext-test-{}", std::process::id()));
    let output = bn()
        .args([
            "build",
            "tests/grammar/valid/print-integer.bn",
            "-o",
            output_path.to_str().expect("temporary path"),
        ])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    let run = std::process::Command::new(&output_path)
        .output()
        .expect("run native artifact");
    assert_eq!(run.stdout, b"42\n");
    let _ = std::fs::remove_file(output_path);
}

#[test]
fn build_emits_wasm_artifact_when_target_is_wasm32() {
    let output_path =
        std::env::temp_dir().join(format!("basicnext-test-{}.wasm", std::process::id()));
    let output = bn()
        .args([
            "build",
            "--target",
            "wasm32",
            "tests/grammar/valid/empty-start.bn",
            "-o",
            output_path.to_str().expect("temporary path"),
        ])
        .output()
        .expect("run wasm build");
    assert_eq!(output.status.code(), Some(0));
    let bytes = std::fs::read(&output_path).expect("read wasm artifact");
    assert_eq!(&bytes[..4], b"\0asm");
    let run = Command::new("node")
        .args(["bin/bn-wasm", output_path.to_str().expect("temporary path")])
        .output()
        .expect("run linked wasm module");
    assert_eq!(run.status.code(), Some(0));
    assert!(run.stdout.is_empty());
    let _ = std::fs::remove_file(output_path);
}

#[test]
fn wasm_build_emits_seeded_random_artifact() {
    let output_path =
        std::env::temp_dir().join(format!("basicnext-random-{}.wasm", std::process::id()));
    let _ = std::fs::remove_file(&output_path);
    let output = bn()
        .args([
            "build",
            "--target",
            "wasm32",
            "tests/grammar/valid/host-random.bn",
            "-o",
            output_path.to_str().expect("temporary path"),
        ])
        .output()
        .expect("run wasm random build");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = std::fs::read(&output_path).expect("read wasm random artifact");
    assert_eq!(&bytes[..4], b"\0asm");
    let run = Command::new("node")
        .args(["bin/bn-wasm", output_path.to_str().expect("temporary path")])
        .output()
        .expect("run linked wasm random module");
    assert_eq!(run.status.code(), Some(0));
    assert_eq!(run.stdout, b"0.28083505005035947\n");
    let _ = std::fs::remove_file(output_path);
}

#[test]
fn build_eliminates_constant_false_loop() {
    let output = bn()
        .args(["build", "tests/grammar/valid/print-while-false.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    let llvm = String::from_utf8_lossy(&output.stdout);
    assert!(!llvm.contains("i64 1"));
}

#[test]
fn build_emits_input_runtime_and_preserves_eof() {
    let output_path = std::env::temp_dir().join(format!("basicnext-input-{}", std::process::id()));
    let _ = std::fs::remove_file(&output_path);
    let output = bn()
        .args([
            "build",
            "tests/grammar/valid/build-input.bn",
            "-o",
            output_path.to_str().expect("temporary path"),
        ])
        .output()
        .expect("run bn build");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut child = Command::new(&output_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("run native input artifact");
    child
        .stdin
        .take()
        .expect("stdin pipe")
        .write_all(b"hello\n")
        .expect("write input");
    let result = child.wait_with_output().expect("collect input output");
    assert_eq!(result.status.code(), Some(0));
    assert_eq!(result.stdout, b"hello\n");
    let eof = Command::new(&output_path)
        .output()
        .expect("run native input artifact at eof");
    assert_eq!(eof.status.code(), Some(0));
    assert_eq!(eof.stdout, b"EOF\n");
    let _ = std::fs::remove_file(output_path);
}

#[test]
fn build_keeps_distinct_input_values_alive_across_later_reads() {
    native_matches_interpreter_with_input(
        "tests/grammar/valid/build-input-lifetime.bn",
        "first\nsecond\nreplacement\n",
    );
}

#[test]
fn build_emits_seeded_random_with_interpreter_sequence() {
    let output_path = std::env::temp_dir().join(format!("basicnext-random-{}", std::process::id()));
    let _ = std::fs::remove_file(&output_path);
    let output = bn()
        .args([
            "build",
            "tests/grammar/valid/host-random.bn",
            "-o",
            output_path.to_str().expect("temporary path"),
        ])
        .output()
        .expect("run random build");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let compiled = Command::new(&output_path)
        .output()
        .expect("run random artifact");
    let interpreted = bn()
        .args(["run", "tests/grammar/valid/host-random.bn"])
        .output()
        .expect("run random interpreter");
    assert_eq!(compiled.status.code(), interpreted.status.code());
    assert_eq!(compiled.stdout, interpreted.stdout);
    let _ = std::fs::remove_file(output_path);
}

#[test]
fn build_emits_two_seeded_random_values_in_sequence() {
    let output_path =
        std::env::temp_dir().join(format!("basicnext-random-two-{}", std::process::id()));
    let _ = std::fs::remove_file(&output_path);
    let output = bn()
        .args([
            "build",
            "tests/grammar/valid/host-random-twice.bn",
            "-o",
            output_path.to_str().expect("temporary path"),
        ])
        .output()
        .expect("run two-random build");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let compiled = Command::new(&output_path)
        .output()
        .expect("run two-random artifact");
    let interpreted = bn()
        .args(["run", "tests/grammar/valid/host-random-twice.bn"])
        .output()
        .expect("run two-random interpreter");
    assert_eq!(compiled.stdout, interpreted.stdout);
    let _ = std::fs::remove_file(output_path);
}

#[test]
fn build_emits_host_args_length() {
    let output = bn()
        .args(["build", "tests/grammar/valid/print-args-length.bn"])
        .output()
        .expect("run args length build");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let llvm = String::from_utf8_lossy(&output.stdout);
    assert!(llvm.contains("define i32 @main(i32 %argc"));
}

#[test]
fn build_emits_host_args_index_zero() {
    let output = bn()
        .args(["build", "tests/grammar/valid/print-arg0.bn"])
        .output()
        .expect("run args index build");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("getelementptr ptr, ptr %argv"));
}

#[test]
fn build_truncates_int64_host_argument_indices() {
    let output = bn()
        .args(["build", "examples/edit_distance.bn"])
        .output()
        .expect("emit edit-distance LLVM");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let llvm = String::from_utf8_lossy(&output.stdout);
    assert!(llvm.contains("trunc i64"), "{llvm}");
    assert!(!llvm.contains("sext i64"), "{llvm}");
}

#[test]
fn build_folds_relational_print() {
    let output = bn()
        .args(["build", "tests/grammar/valid/print-comparison.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("@.bn_true"));
}

#[test]
fn build_folds_pure_constant_function_call() {
    native_matches_interpreter("tests/grammar/valid/print-call.bn");
}

#[test]
fn build_folds_boolean_function_call() {
    native_matches_interpreter("tests/grammar/valid/print-predicate-call.bn");
}

#[test]
fn build_folds_pure_function_local_binding() {
    native_matches_interpreter("tests/grammar/valid/print-call-local.bn");
}

#[test]
fn build_folds_string_function_call() {
    native_matches_interpreter("tests/grammar/valid/print-string-call.bn");
}

#[test]
fn build_folds_nested_pure_function_calls() {
    native_matches_interpreter("tests/grammar/valid/print-call-nested.bn");
}

#[test]
fn build_lowers_factorial_recursion_matching_interpreter() {
    native_matches_interpreter("examples/factorial.bn");
}

#[test]
fn build_lowers_global_static_field_matching_interpreter() {
    native_matches_interpreter("examples/global.bn");
}

#[test]
fn build_lowers_indexed_object_vector_fields_matching_interpreter() {
    native_matches_interpreter("tests/grammar/valid/build-indexed-object-vector-field.bn");
    native_matches_interpreter_with_input("examples/rpn-calculator.bn", "3\n4\n+\n2\n*\n=\nQ\n");
}

#[test]
fn build_lowers_variables_vector_print_matching_interpreter() {
    native_matches_interpreter("examples/variables.bn");
}

#[test]
fn build_lowers_counted_for_control_flow_matching_interpreter() {
    native_matches_interpreter("examples/variables.bn");
    native_matches_interpreter("examples/control-flow.bn");
}

#[test]
fn build_lowers_type_test_matching_interpreter() {
    native_matches_interpreter("examples/type_test.bn");
}

#[test]
fn build_lowers_string_search_and_codec_examples_matching_interpreter() {
    for path in [
        "examples/naive_search.bn",
        "examples/boyer-moore.bn",
        "examples/rabin-karp.bn",
        "examples/huffman.bn",
        "examples/shortest_path.bn",
        "examples/lexical.bn",
    ] {
        native_matches_interpreter(path);
    }
}

#[test]
fn build_lowers_asc_and_char_matching_interpreter() {
    native_matches_interpreter("tests/grammar/valid/build-asc-char.bn");
}

#[test]
fn build_lowers_bnmath_scalars_matching_interpreter() {
    native_matches_interpreter("tests/grammar/valid/build-bnmath-scalar.bn");
}

#[test]
fn build_lowers_bnmath_float_vectors_matching_interpreter() {
    native_matches_interpreter("tests/grammar/valid/build-bnmath-float-vector.bn");
}

#[test]
fn build_lowers_bndata_empty_frame_lifecycle_matching_interpreter() {
    native_matches_interpreter("tests/grammar/valid/bndata-import.bn");
}

#[test]
fn build_rejects_recursive_constant_call_without_stack_overflow() {
    let output = bn()
        .args(["build", "tests/grammar/valid/build-recursive.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("call i32 @bn_Loop"),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn build_constant_folds_or_short_circuit_branch() {
    let output = bn()
        .args(["build", "tests/grammar/valid/print-if-or.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    let llvm = String::from_utf8_lossy(&output.stdout);
    assert!(llvm.contains("yes-or"));
    assert!(llvm.contains("no-or"));
    assert!(llvm.contains("br i1 %v1"));
}

#[test]
fn wasm_build_supports_host_console_capability() {
    let output = bn()
        .args([
            "build",
            "--target",
            "wasm32",
            "tests/grammar/valid/cls-and-beep.bn",
        ])
        .output()
        .expect("run wasm build");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn wasm_build_rejects_host_net_with_an_explicit_capability_diagnostic() {
    let output = bn()
        .args([
            "build",
            "--target",
            "wasm32",
            "tests/grammar/valid/build-net-resolve.bn",
        ])
        .output()
        .expect("run wasm HOST.Net build");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("TARGET_UNSUPPORTED_HOST"));
    assert!(stderr.contains("HOST.Net"));
    assert!(stderr.contains("HOST.Console is supported"));
}

#[test]
fn wasm_build_allows_host_capability_names_in_strings() {
    let output = bn()
        .args([
            "build",
            "--target",
            "wasm32",
            "tests/grammar/valid/wasm-host-name-string.bn",
        ])
        .output()
        .expect("run wasm build");
    assert_eq!(output.status.code(), Some(0));
}

#[cfg(unix)]
#[test]
fn console_size_uses_stdout_when_stdin_is_piped() {
    let output = Command::new("python3")
        .arg("tests/console_stdout_tty.py")
        .env("BN", env!("CARGO_BIN_EXE_bn"))
        .env("BN_PROGRAM", "tests/grammar/valid/console-size.bn")
        .output()
        .expect("run PTY console-size helper");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout).replace('\r', "");
    assert_eq!(stdout, "80 24\n");
}

#[test]
fn run_without_filesystem_rejects_an_unused_import() {
    let output = bn()
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
