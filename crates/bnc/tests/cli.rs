// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Integration tests of the `bnc` executable relocated from the root
//! `tests/cli.rs` (bucket 0.6.0, 2.4 / D-060-05): `bn build <args>` became
//! `bnc <args>` (D-060-07), everything else verbatim.

// Multi-line raw-string BN program templates read more clearly with named
// placeholders than with inlined path expressions.
#![allow(clippy::uninlined_format_args)]

use std::{fs, io::Write, process::Command};

const WORKSPACE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// Runs `bnc` from the workspace root (the fixtures use workspace-relative
/// paths; crate integration tests start in the crate directory).
fn bnc() -> Command {
    let _ = std::env::set_current_dir(WORKSPACE);
    let mut command = Command::new(env!("CARGO_BIN_EXE_bnc"));
    command.current_dir(WORKSPACE);
    command
}

/// Compiles the deterministic exec helper, substitutes its path into `body`
/// (placeholder `{HELPER}`), builds the BN program natively, runs it under
/// `env`, and returns the child stdout. Real `rustc` + real `bn build` + real
/// child process: the native HOST.Exec seam is exercised without mocks (4.3).
fn native_exec_program(label: &str, body: &str, env: &[(&str, &str)]) -> String {
    let base = std::env::temp_dir().join(format!("basicnext-nexec-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("create native exec dir");
    let helper = base.join("helper");
    let status = Command::new("rustc")
        .args(["--edition=2021", "tests/fixtures/host_exec_helper.rs", "-o"])
        .arg(&helper)
        .status()
        .expect("compile host exec helper");
    assert!(status.success(), "rustc must build host_exec_helper");
    let source = base.join("program.bn");
    let binary = base.join("program");
    let helper_bn = helper.to_string_lossy().replace('\\', "\\\\");
    fs::write(&source, body.replace("{HELPER}", &helper_bn)).expect("write native exec program");
    let build = bnc()
        .args(["-o", binary.to_str().expect("UTF-8 binary path")])
        .arg(&source)
        .output()
        .expect("native build");
    assert_eq!(
        build.status.code(),
        Some(0),
        "build: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let mut command = Command::new(&binary);
    for (key, value) in env {
        command.env(key, value);
    }
    let run = command.output().expect("run native exec program");
    assert_eq!(
        run.status.code(),
        Some(0),
        "run: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8(run.stdout).expect("UTF-8 program stdout");
    let _ = fs::remove_dir_all(&base);
    stdout
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
            RELEASE file\n\
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
            RELEASE frame\n\
            file.Close()\n\
            RELEASE file\n\
        END IF\n\
    END IF\n\
END FUNCTION\n",
    )
    .expect("write sandbox fixture");

    let artifact = base.join("sandbox");
    let built = bnc()
        .args([
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
    let deny_all_build = bnc()
        .args([
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
fn build_writes_companion_process_log_next_to_artifact() {
    let directory = std::env::temp_dir().join(format!("bn-process-log-cli-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("process log test directory");
    let output_path = directory.join("hello");
    let output = bnc()
        .args([
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
    let output = bnc()
        .args([
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
    let output = bnc()
        .args([
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
    let no_log = bnc()
        .args([
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
    let unwritable = bnc()
        .args([
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
    let output = bnc()
        .args([
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

    let output = bnc()
        .args([
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
fn build_emits_llvm_for_finite_constant_loop() {
    let output = bnc()
        .args(["examples/hello.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("@printf"));
}

#[test]
fn build_emits_llvm_for_empty_start() {
    let output = bnc()
        .args(["tests/grammar/valid/empty-start.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("define i32 @main(i32 %argc, ptr %argv)")
    );
}

#[test]
fn build_emits_integer_start_exit_code() {
    let output = bnc()
        .args(["tests/grammar/valid/start-exit-code.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("ret i32 %ret"));
}

#[test]
fn build_reports_the_type_for_unsupported_allocation_lowering() {
    let output = bnc()
        .args(["tests/grammar/valid/pointer-void.bn"])
        .output()
        .expect("run pointer build");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let llvm = String::from_utf8_lossy(&output.stdout);
    // A region is a counted allocation: header + elements, count stored at +8.
    assert!(llvm.contains("call ptr @calloc"), "{llvm}");
    assert!(llvm.contains("store i64 1, ptr %alloccount"), "{llvm}");
}

#[test]
fn build_emits_llvm_for_integer_print() {
    let output = bnc()
        .args(["tests/grammar/valid/print-integer.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    let llvm = String::from_utf8_lossy(&output.stdout);
    assert!(llvm.contains("@printf"));
    assert!(llvm.contains("add i64 0, 42"));
}

#[test]
fn build_emits_multiple_integer_prints() {
    let output = bnc()
        .args(["tests/grammar/valid/print-integers.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    let llvm = String::from_utf8_lossy(&output.stdout);
    assert_eq!(llvm.matches("@printf").count(), 4);
    assert!(llvm.contains("add i64 0, 1"));
    assert!(llvm.contains("add i64 0, 2"));
    assert!(llvm.contains("add i64 0, 3"));
}

#[test]
fn build_constant_folds_integer_expression() {
    let output = bnc()
        .args(["tests/grammar/valid/print-expression.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("add i64 0, 14"));
}

#[test]
fn build_constant_folds_unary_integer_expression() {
    let output = bnc()
        .args(["tests/grammar/valid/print-unary.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("add i64 0, -5"));
}

#[test]
fn build_constant_propagates_integer_binding() {
    let output = bnc()
        .args(["tests/grammar/valid/print-variable.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("add i32 0, 3"));
}

#[test]
fn build_emits_boolean_prints() {
    let output = bnc()
        .args(["tests/grammar/valid/print-boolean.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    let llvm = String::from_utf8_lossy(&output.stdout);
    assert!(llvm.contains("@.bn_true"));
    assert!(llvm.contains("@.bn_false"));
}

#[test]
fn build_emits_float_print() {
    let output = bnc()
        .args(["tests/grammar/valid/print-float.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("fadd double 0.0, 3.75"));
}

#[test]
fn build_emits_string_print() {
    let output = bnc()
        .args(["tests/grammar/valid/print-string.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Hello LLVM"));
}

#[test]
fn build_constant_folds_if_branch() {
    let output = bnc()
        .args(["tests/grammar/valid/print-if-constant.bn"])
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
    let output = bnc()
        .args(["tests/grammar/valid/print-if-comparison.bn"])
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
    let output = bnc()
        .args(["tests/grammar/valid/print-if-boolean-expression.bn"])
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
    let output = bnc()
        .args(["tests/grammar/valid/print-string-escaped.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    let llvm = String::from_utf8_lossy(&output.stdout);
    assert!(llvm.contains("A\\22B\\5CC"));
}

#[test]
fn build_emits_native_artifact_when_output_is_given() {
    let output_path = std::env::temp_dir().join(format!("basicnext-test-{}", std::process::id()));
    let output = bnc()
        .args([
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
    let output = bnc()
        .args([
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
    let output = bnc()
        .args([
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
    let output = bnc()
        .args(["tests/grammar/valid/print-while-false.bn"])
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
    let output = bnc()
        .args([
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
fn build_emits_host_args_length() {
    let output = bnc()
        .args(["tests/grammar/valid/print-args-length.bn"])
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
    let output = bnc()
        .args(["tests/grammar/valid/print-arg0.bn"])
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
    let output = bnc()
        .args(["examples/edit_distance.bn"])
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
    let output = bnc()
        .args(["tests/grammar/valid/print-comparison.bn"])
        .output()
        .expect("run bn build");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("@.bn_true"));
}

#[test]
fn native_released_pointer_binding_is_diagnosed_at_runtime() {
    let base = std::env::temp_dir().join(format!("bn-ptr-f15-{}", std::process::id()));
    fs::create_dir_all(&base).expect("scratch dir");
    for (label, tail, code) in [
        ("index", "PRINT p[0]", "USE_AFTER_RELEASE"),
        ("len", "PRINT LEN(p)", "USE_AFTER_RELEASE"),
        ("double", "RELEASE p", "DOUBLE_RELEASE"),
    ] {
        let source = base.join(format!("{label}.bn"));
        fs::write(
            &source,
            format!(
                "FUNCTION Start() AS VOID\n    LET p AS POINTER TO INTEGER[] = NEW INTEGER[2]\n    RELEASE p\n    {tail}\nEND FUNCTION\n"
            ),
        )
        .expect("write fixture");
        let artifact = base.join(format!("{label}.bin"));
        let built = bnc()
            .args([
                source.to_str().expect("source"),
                "-o",
                artifact.to_str().expect("artifact"),
            ])
            .output()
            .expect("build");
        assert_eq!(
            built.status.code(),
            Some(0),
            "{label}: {}",
            String::from_utf8_lossy(&built.stderr)
        );
        let ran = Command::new(&artifact).output().expect("run artifact");
        assert_eq!(ran.status.code(), Some(1), "{label}: native must fail");
        let printed = format!(
            "{}{}",
            String::from_utf8_lossy(&ran.stdout),
            String::from_utf8_lossy(&ran.stderr)
        );
        assert!(
            printed.contains(code),
            "{label}: expected {code}, got: {printed}"
        );
    }
    fs::remove_dir_all(&base).ok();
}

#[test]
fn build_rejects_recursive_constant_call_without_stack_overflow() {
    let output = bnc()
        .args(["tests/grammar/valid/build-recursive.bn"])
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
    let output = bnc()
        .args(["tests/grammar/valid/print-if-or.bn"])
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
    let output = bnc()
        .args(["--target", "wasm32", "tests/grammar/valid/cls-and-beep.bn"])
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
    let output = bnc()
        .args([
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
    let output = bnc()
        .args([
            "--target",
            "wasm32",
            "tests/grammar/valid/wasm-host-name-string.bn",
        ])
        .output()
        .expect("run wasm build");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn native_build_executes_host_exec_result_accessors() {
    let output_path =
        std::env::temp_dir().join(format!("basicnext-host-exec-{}", std::process::id()));
    let output = bnc()
        .args([
            "-o",
            output_path.to_str().expect("UTF-8 output path"),
            "tests/grammar/valid/build-host-exec.bn",
        ])
        .output()
        .expect("run native HOST.Exec build");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let run = std::process::Command::new(&output_path)
        .output()
        .expect("run native HOST.Exec executable");
    assert_eq!(run.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "0\n\n\n\n");
    let _ = fs::remove_file(output_path);
}

#[test]
fn native_build_executes_host_exec_helper_streams_and_status() {
    let base =
        std::env::temp_dir().join(format!("basicnext-host-exec-matrix-{}", std::process::id()));
    fs::create_dir_all(&base).expect("create native exec matrix directory");
    let helper = base.join("helper");
    let source = base.join("program.bn");
    let binary = base.join("program");
    let status = std::process::Command::new("rustc")
        .args(["--edition=2021", "tests/fixtures/host_exec_helper.rs", "-o"])
        .arg(&helper)
        .status()
        .expect("compile host exec helper");
    assert!(status.success());
    let helper_text = helper
        .to_str()
        .expect("UTF-8 helper path")
        .replace('"', "\\\"");
    fs::write(
        &source,
        format!(
            "IMPORT HOST.Exec AS Exec\nFUNCTION Start() AS VOID\nLET args AS STRING[2]\nargs[0] = \"stdout\"\nargs[1] = \"native-out\"\nLET r AS Exec.Result OR Error = Exec.Run(\"{helper_text}\", args)\nIF r IS Error THEN\nPRINT \"error\", r.Code\nELSE\nPRINT r.ReturnCode, r.Stdout, r.Stderr\nEND IF\nLET fail AS Exec.Result OR Error = Exec.Run(\"{helper_text}\", [\"status\", \"7\"])\nIF fail IS Error THEN\nPRINT \"fail-error\", fail.Code\nELSE\nPRINT \"status\", fail.ReturnCode\nEND IF\nEND FUNCTION\n"
        ),
    )
    .expect("write native exec matrix program");
    let output = bnc()
        .args(["-o", binary.to_str().expect("UTF-8 binary path")])
        .arg(&source)
        .output()
        .expect("build native exec matrix program");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let run = std::process::Command::new(&binary)
        .output()
        .expect("run native exec matrix program");
    assert_eq!(run.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "0 native-out \nstatus 7\n"
    );
    let _ = fs::remove_dir_all(base);
}

#[test]
fn native_build_reports_host_exec_argument_and_spawn_errors() {
    let output_path =
        std::env::temp_dir().join(format!("basicnext-host-exec-errors-{}", std::process::id()));
    let output = bnc()
        .args([
            "-o",
            output_path.to_str().expect("UTF-8 output path"),
            "tests/grammar/valid/build-host-exec-errors.bn",
        ])
        .output()
        .expect("build native HOST.Exec errors fixture");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let run = std::process::Command::new(&output_path)
        .output()
        .expect("run native HOST.Exec errors fixture");
    assert_eq!(run.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "empty 1\nmissing 2\n");
    let _ = fs::remove_file(output_path);
}

#[test]
fn native_host_exec_policy_denial_happens_before_spawn() {
    let output_path =
        std::env::temp_dir().join(format!("basicnext-host-exec-policy-{}", std::process::id()));
    let output = bnc()
        .args([
            "-o",
            output_path.to_str().expect("UTF-8 output path"),
            "tests/grammar/valid/build-host-exec-policy.bn",
        ])
        .output()
        .expect("build native HOST.Exec policy fixture");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let run = std::process::Command::new(&output_path)
        .env("BN_EXEC_POLICY", "deny")
        .output()
        .expect("run native HOST.Exec policy fixture");
    assert_eq!(run.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "denied 11\n");
    let _ = fs::remove_file(output_path);
}

#[test]
fn native_host_exec_rejects_invalid_utf8_capture() {
    let base =
        std::env::temp_dir().join(format!("basicnext-host-exec-utf8-{}", std::process::id()));
    fs::create_dir_all(&base).expect("create UTF-8 fixture directory");
    let helper = base.join("helper");
    let source = base.join("program.bn");
    let binary = base.join("program");
    let status = std::process::Command::new("rustc")
        .args(["--edition=2021", "tests/fixtures/host_exec_helper.rs", "-o"])
        .arg(&helper)
        .status()
        .expect("compile host exec helper");
    assert!(status.success());
    fs::write(
        &source,
        format!(
            r#"IMPORT HOST.Exec AS Exec
FUNCTION Start() AS VOID
LET r AS Exec.Result OR Error = Exec.Run("{helper}", ["invalid-utf8"])
IF r IS Error THEN
PRINT "invalid", r.Code
END IF
END FUNCTION
"#,
            helper = helper.to_str().expect("UTF-8 helper path")
        ),
    )
    .expect("write UTF-8 fixture program");
    let output = bnc()
        .args(["-o", binary.to_str().expect("UTF-8 binary path")])
        .arg(&source)
        .output()
        .expect("build invalid UTF-8 fixture");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let run = std::process::Command::new(&binary)
        .output()
        .expect("run invalid UTF-8 fixture");
    assert_eq!(run.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "invalid 7\n");
    let _ = fs::remove_dir_all(base);
}

/// E06 (native) — spaces, empty args, quotes, Unicode and shell metacharacters
/// stay literal argv elements; there is no shell between BN and the child.
#[test]
fn native_host_exec_e06_literal_argv_elements() {
    let body = r#"IMPORT HOST.Exec AS Exec
FUNCTION Start() AS VOID
LET r AS Exec.Result OR Error = Exec.Run("{HELPER}", ["argv", "hello world", "", "café", "; rm -rf /", "\"quoted\"", "$HOME"])
IF r IS Error THEN
PRINT "err", r.Code
ELSE
PRINT r.Stdout
END IF
END FUNCTION
"#;
    assert_eq!(
        native_exec_program("e06", body, &[]),
        "0=hello world\n1=\n2=café\n3=; rm -rf /\n4=\"quoted\"\n5=$HOME\n\n"
    );
}

/// E08 (native) — child stdin is immediately EOF (closed / null device).
#[test]
fn native_host_exec_e08_child_stdin_is_eof() {
    let body = r#"IMPORT HOST.Exec AS Exec
FUNCTION Start() AS VOID
LET r AS Exec.Result OR Error = Exec.Run("{HELPER}", ["echo-stdin"])
IF r IS Error THEN
PRINT "err", r.Code
ELSE
PRINT "eof", r.Stdout, r.ReturnCode
END IF
END FUNCTION
"#;
    assert_eq!(native_exec_program("e08", body, &[]), "eof  0\n");
}

/// E10 (native) — concurrent stdout/stderr beyond pipe capacity drain without
/// deadlock; a reduced policy capture ceiling returns `CAPTURE_LIMIT`=8.
#[test]
fn native_host_exec_e10_concurrent_streams_and_capture_limit() {
    let concurrent = r#"IMPORT HOST.Exec AS Exec
FUNCTION Start() AS VOID
LET r AS Exec.Result OR Error = Exec.Run("{HELPER}", ["both", "262144"])
IF r IS Error THEN
PRINT "err", r.Code
ELSE
PRINT "ok", LEN(r.Stdout), LEN(r.Stderr), r.ReturnCode
END IF
END FUNCTION
"#;
    assert_eq!(
        native_exec_program("e10a", concurrent, &[]),
        "ok 262144 262144 0\n"
    );

    let limited = r#"IMPORT HOST.Exec AS Exec
FUNCTION Start() AS VOID
LET r AS Exec.Result OR Error = Exec.Run("{HELPER}", ["stdout-bytes", "4096"])
IF r IS Error THEN
PRINT "limit", r.Code
ELSE
PRINT "unexpected", LEN(r.Stdout)
END IF
END FUNCTION
"#;
    assert_eq!(
        native_exec_program("e10b", limited, &[("BN_EXEC_CAPTURE_LIMIT", "1024")]),
        "limit 8\n"
    );
}

/// E11 (native) — POSIX signal termination yields `ReturnCode` = -signal (D-H1-01).
#[cfg(unix)]
#[test]
fn native_host_exec_e11_signal_termination_negative_return_code() {
    let body = r#"IMPORT HOST.Exec AS Exec
FUNCTION Start() AS VOID
LET r AS Exec.Result OR Error = Exec.Run("{HELPER}", ["signal"])
IF r IS Error THEN
PRINT "err", r.Code
ELSE
PRINT "sig", r.ReturnCode
END IF
END FUNCTION
"#;
    assert_eq!(native_exec_program("e11", body, &[]), "sig -15\n");
}

/// E12 (native) — cwd inheritance, environment inheritance and spaces in the
/// executable path. The helper lives under a spaced directory and the child
/// inherits the parent's controlled working directory and marker variable.
#[test]
fn native_host_exec_e12_cwd_env_and_spaced_path() {
    let base = std::env::temp_dir().join(format!("basicnext-nexec-e12-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let spaced_dir = base.join("host exec dir");
    fs::create_dir_all(&spaced_dir).expect("create spaced dir");
    let helper = spaced_dir.join("helper bin");
    let status = Command::new("rustc")
        .args(["--edition=2021", "tests/fixtures/host_exec_helper.rs", "-o"])
        .arg(&helper)
        .status()
        .expect("compile host exec helper");
    assert!(status.success(), "rustc must build host_exec_helper");
    let helper_bn = helper.to_string_lossy().replace('\\', "\\\\");
    let source = base.join("program.bn");
    let binary = base.join("program");
    let body = format!(
        r#"IMPORT HOST.Exec AS Exec
FUNCTION Start() AS VOID
LET cwd AS Exec.Result OR Error = Exec.Run("{helper}", ["cwd"])
IF cwd IS Error THEN
PRINT "cwd-err", cwd.Code
ELSE
PRINT "cwd", cwd.Stdout
END IF
LET env AS Exec.Result OR Error = Exec.Run("{helper}", ["env", "BN_E12_MARKER"])
IF env IS Error THEN
PRINT "env-err", env.Code
ELSE
PRINT "env", env.Stdout
END IF
END FUNCTION
"#,
        helper = helper_bn
    );
    fs::write(&source, body).expect("write E12 program");
    let build = bnc()
        .args(["-o", binary.to_str().expect("UTF-8 binary path")])
        .arg(&source)
        .output()
        .expect("build E12 program");
    assert_eq!(
        build.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(&binary)
        .current_dir(&base)
        .env("BN_E12_MARKER", "inherited-value")
        .output()
        .expect("run E12 program");
    assert_eq!(
        run.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    // getcwd() in the child resolves symlinks (e.g. macOS /var -> /private/var),
    // so compare against the canonicalized controlled working directory.
    let cwd_expected = base.canonicalize().unwrap_or_else(|_| base.clone());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        format!("cwd {}\nenv inherited-value\n", cwd_expected.display())
    );
    let _ = fs::remove_dir_all(&base);
}

/// E13 (native) — repeated calls reclaim child resources without leak or hang;
/// Result copies observe the captured value (ARC-managed heap handle on native).
#[test]
fn native_host_exec_e13_repeated_calls_and_release() {
    let body = r#"IMPORT HOST.Exec AS Exec
FUNCTION Start() AS VOID
LET last AS INT64 = 0
FOR i AS INTEGER = 1 TO 8
LET r AS Exec.Result OR Error = Exec.Run("{HELPER}", ["stdout", "x"])
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
"#;
    assert_eq!(native_exec_program("e13", body, &[]), "done 0\n");
}

/// E14 (native) — a reduced policy timeout returns TIMEOUT=9 and the child is
/// reaped (the test completing rather than hanging is the watchdog evidence).
#[test]
fn native_host_exec_e14_timeout_returns_error() {
    let body = r#"IMPORT HOST.Exec AS Exec
FUNCTION Start() AS VOID
LET r AS Exec.Result OR Error = Exec.Run("{HELPER}", ["block"])
IF r IS Error THEN
PRINT "timeout", r.Code
ELSE
PRINT "unexpected", r.ReturnCode
END IF
END FUNCTION
"#;
    assert_eq!(
        native_exec_program("e14", body, &[("BN_EXEC_TIMEOUT_MS", "200")]),
        "timeout 9\n"
    );
}

/// Native `HOST.Net.Neighbor` on loopback: the compiled runtime shares the
/// interpreter's core (bucket 0.5.2a, SPRINT 1), including trusted-path
/// `arp`/`ndp` resolution, so the typed result matches `tests/runtime.rs`.
#[test]
fn native_host_net_neighbor_loopback_is_a_typed_result() {
    let base =
        std::env::temp_dir().join(format!("basicnext-native-neighbor-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("create native neighbor directory");
    let source = base.join("program.bn");
    let binary = base.join("program");
    fs::write(
        &source,
        "IMPORT HOST.Net AS Net\nFUNCTION Start() AS VOID\nLET address AS Net.Address OR Error = Net.Address.Parse(\"127.0.0.1\")\nIF address IS Error THEN\nPRINT \"parse-error\"\nELSE\nLET neighbor AS Net.Address OR Error = Net.Neighbor(address)\nPRINT neighbor IS Error\nEND IF\nEND FUNCTION\n",
    )
    .expect("write native neighbor program");
    let build = bnc()
        .args(["-o", binary.to_str().expect("UTF-8 binary path")])
        .arg(&source)
        .output()
        .expect("build native neighbor program");
    assert_eq!(
        build.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = std::process::Command::new(&binary)
        .output()
        .expect("run native neighbor program");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "FALSE\n",
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let _ = fs::remove_dir_all(&base);
}

/// The compiled path produces the same `BNCrypto` digests as the interpreter:
/// both backends route to the single `bn_rt::crypto` implementation, so any
/// drift between them fails here. Mirrors `bncrypto_digests_match_fips_vectors`
/// in the `bni` suite; the expectations are sourced FIPS 180-4 vectors and must
/// not be edited to match output.
#[test]
fn compiled_bncrypto_digests_match_the_interpreter() {
    let directory = std::env::temp_dir().join(format!("bn-crypto-parity-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("crypto parity directory");
    let artifact = directory.join("digests");
    let built = bnc()
        .args(["tests/modules/bncrypto-digests/main.bn", "-o"])
        .arg(&artifact)
        .output()
        .expect("compile the BNCrypto digest fixture");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let run = Command::new(&artifact)
        .output()
        .expect("run the compiled digest fixture");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n\
         ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\n\
         cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce\
         47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e\n\
         ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
         2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f\n"
    );
}

/// `BNCrypto.Bytes` handles survive compilation: the buffer is created, measured,
/// rendered and released through `bn_rt`, and the native binary prints exactly
/// what the interpreter prints (see `bncrypto_bytes_portable_subset_interprets`).
#[test]
fn compiled_bncrypto_bytes_match_the_interpreter() {
    let directory = std::env::temp_dir().join(format!("bn-crypto-bytes-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("crypto bytes directory");
    let artifact = directory.join("bytes");
    let built = bnc()
        .args(["tests/modules/bncrypto-bytes-portable/main.bn", "-o"])
        .arg(&artifact)
        .output()
        .expect("compile the portable BNCrypto bytes fixture");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let run = Command::new(&artifact)
        .output()
        .expect("run the compiled bytes fixture");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "3\n616263\n");
}

/// The whole `BNCrypto.Bytes` surface compiles, including `FromHex` and method
/// calls on a value narrowed out of `Bytes OR Error`. The native binary must
/// print exactly what the interpreter prints.
#[test]
fn compiled_bncrypto_bytes_full_surface_matches_the_interpreter() {
    let directory = std::env::temp_dir().join(format!("bn-crypto-full-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("crypto full directory");
    let artifact = directory.join("full");
    let built = bnc()
        .args(["tests/modules/bncrypto-bytes/main.bn", "-o"])
        .arg(&artifact)
        .output()
        .expect("compile the full BNCrypto bytes fixture");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let run = Command::new(&artifact)
        .output()
        .expect("run the compiled full bytes fixture");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "3\n616263\n3\n000fff\nodd-length-rejected\n"
    );
}

/// AEAD survives compilation: a native binary seals and opens with both ciphers
/// and rejects a changed AAD, printing exactly what the interpreter prints.
#[test]
fn compiled_bncrypto_aead_matches_the_interpreter() {
    let directory = std::env::temp_dir().join(format!("bn-crypto-aead-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("crypto aead directory");
    let artifact = directory.join("aead");
    let built = bnc()
        .args(["tests/modules/bncrypto-aead/main.bn", "-o"])
        .arg(&artifact)
        .output()
        .expect("compile the BNCrypto AEAD fixture");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let run = Command::new(&artifact)
        .output()
        .expect("run the compiled AEAD fixture");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "f8cb8441c9b57bd8fd62663940f843f4e3cf1f\n616263\n25a94560bf99e02777736ea20551c7a02d3f86\n616263\ntamper-rejected\n"
    );
}

/// HMAC-SHA-256 and Argon2id survive compilation, including the mixed
/// handle-and-integer argument list Argon2id needs. The native binary prints
/// exactly what the interpreter prints.
#[test]
fn compiled_bncrypto_mac_and_kdf_match_the_interpreter() {
    let directory = std::env::temp_dir().join(format!("bn-crypto-mac-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("crypto mac directory");
    let artifact = directory.join("mac");
    let built = bnc()
        .args(["tests/modules/bncrypto-mac-kdf/main.bn", "-o"])
        .arg(&artifact)
        .output()
        .expect("compile the BNCrypto MAC/KDF fixture");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let run = Command::new(&artifact)
        .output()
        .expect("run the compiled MAC/KDF fixture");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "a60c859a6827c5ea576a48d8d368672fbfe4667c6a927428284a0cb3859cc1d6\nTRUE\nFALSE\n32\nargon-params-rejected\n"
    );
}

/// Ed25519 and ECDSA P-256 survive compilation and print what the
/// interpreter prints.
#[test]
fn compiled_bncrypto_signatures_match_the_interpreter() {
    let directory = std::env::temp_dir().join(format!(
        "bn-compiled_bncrypto_signatures_match_the_interpreter-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("fixture directory");
    let artifact = directory.join("artifact");
    let built = bnc()
        .args(["tests/modules/bncrypto-signatures/main.bn", "-o"])
        .arg(&artifact)
        .output()
        .expect("compile the fixture");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let run = Command::new(&artifact)
        .output()
        .expect("run the compiled fixture");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "23bc54912c1e6e92c4a86825c867e27ffdc555bffbd4244f17a26abfffee965d\ndb2b1ee48bbbb9af4a2e038310db3cd4e36e081b5537c348adfc95447fa8de2a7a971a889fd4c8d562a3a474db1e89f4f082cc3bc19f019951206f0b99aa8103\nTRUE\nFALSE\n65\nTRUE\nFALSE\n"
    );
}

/// ML-KEM-768 and ML-DSA-65 survive compilation, including the concatenated
/// pair returns that `Slice` splits.
#[test]
fn compiled_bncrypto_post_quantum_matches_the_interpreter() {
    let directory = std::env::temp_dir().join(format!(
        "bn-compiled_bncrypto_post_quantum_matches_the_interpreter-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("fixture directory");
    let artifact = directory.join("artifact");
    let built = bnc()
        .args(["tests/modules/bncrypto-pqc/main.bn", "-o"])
        .arg(&artifact)
        .output()
        .expect("compile the fixture");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let run = Command::new(&artifact)
        .output()
        .expect("run the compiled fixture");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "1248\nkem-agree\n1984\n3309\nTRUE\nFALSE\n"
    );
}

/// `BNJson` survives compilation: the DOM slice links against the `bn_rt`
/// document table and the native binary prints what the interpreter prints.
/// Before bucket 0.6.1c the backend had no `BNJson` support at all.
#[test]
fn compiled_bnjson_dom_slice_matches_the_interpreter() {
    let directory = std::env::temp_dir().join(format!("bn-json-slice-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("json slice directory");
    let artifact = directory.join("slice");
    let built = bnc()
        .args(["tests/modules/bnjson-dom-slice/main.bn", "-o"])
        .arg(&artifact)
        .output()
        .expect("compile the BNJson DOM slice");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let run = Command::new(&artifact)
        .output()
        .expect("run the compiled DOM slice");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "pardal\nmissing-key-rejected\n{\"name\":\"pardal\"}\n"
    );
}

/// The scalar and inspection surface compiles and matches the interpreter,
/// including the fail-closed reads.
#[test]
fn compiled_bnjson_dom_scalars_match_the_interpreter() {
    let directory = std::env::temp_dir().join(format!("bn-json-scalars-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("json scalars directory");
    let artifact = directory.join("scalars");
    let built = bnc()
        .args(["tests/modules/bnjson-dom-scalars/main.bn", "-o"])
        .arg(&artifact)
        .output()
        .expect("compile the BNJson scalar fixture");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let run = Command::new(&artifact)
        .output()
        .expect("run the compiled scalar fixture");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "object\nTRUE\nFALSE\n4\n3\nTRUE\n3.5\nwrong-kind-rejected\nfloat-wrong-kind-rejected\nTRUE\n"
    );
}

/// Array Append* / Get*At / Set*At compile and match the interpreter.
#[test]
fn compiled_bnjson_dom_array_matches_the_interpreter() {
    let directory = std::env::temp_dir().join(format!("bn-json-array-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("json array directory");
    let artifact = directory.join("array");
    let built = bnc()
        .args(["tests/modules/bnjson-dom-array/main.bn", "-o"])
        .arg(&artifact)
        .output()
        .expect("compile the BNJson array fixture");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let run = Command::new(&artifact)
        .output()
        .expect("run the compiled array fixture");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "array\n5\nalpha\n7\nTRUE\n1.25\n9\noob-rejected\noob-set-rejected\narray-wrong-kind-rejected\n"
    );
}

/// Nested `SetJson` / `GetJson` / Clone compile and match the interpreter.
#[test]
fn compiled_bnjson_dom_nested_matches_the_interpreter() {
    let directory = std::env::temp_dir().join(format!("bn-json-nested-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("json nested directory");
    let artifact = directory.join("nested");
    let built = bnc()
        .args(["tests/modules/bnjson-dom-nested/main.bn", "-o"])
        .arg(&artifact)
        .output()
        .expect("compile the BNJson nested fixture");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let run = Command::new(&artifact)
        .output()
        .expect("run the compiled nested fixture");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "pardal\nparent-intact\npardal\nmissing-nested-rejected\nv\n"
    );
}

/// Companion Encode/Decode compiles and matches the interpreter (A8 / A4).
#[test]
fn compiled_bnjson_companion_matches_the_interpreter() {
    let directory = std::env::temp_dir().join(format!("bn-json-companion-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("json companion directory");
    let artifact = directory.join("companion");
    let built = bnc()
        .args(["tests/modules/bnjson-companion/main.bn", "-o"])
        .arg(&artifact)
        .output()
        .expect("compile the BNJson companion fixture");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let run = Command::new(&artifact)
        .output()
        .expect("run the compiled companion fixture");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "{\"name\":\"eagle\",\"wings\":2}\neagle\n2\ndecode-error TRUE\n"
    );
}
