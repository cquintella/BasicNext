// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::{
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
fn cli_help_and_version_advertise_current_commands() {
    let help = bn().arg("--help").output().expect("run bn help");
    assert_eq!(help.status.code(), Some(0));
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(help.contains("lsp") && help.contains("dap"));
    let version = bn().arg("--version").output().expect("run bn version");
    assert_eq!(version.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&version.stdout).trim(), "bn 0.4.3");
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
    assert!(String::from_utf8_lossy(&run.stdout).contains("Encontrado padrao no indice 10"));
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
fn build_reports_the_type_for_unsupported_vector_lowering() {
    let output = bn()
        .args(["build", "tests/grammar/valid/multidimensional-vectors.bn"])
        .output()
        .expect("run vector build");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("LLVM lowering for vector type"));
    assert!(stderr.contains("INTEGER(INT32)[2][3]"));
    assert!(stderr.contains("FUNCTION Start"));
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
fn build_lowers_bnmath_scalars_matching_interpreter() {
    native_matches_interpreter("tests/grammar/valid/build-bnmath-scalar.bn");
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
    assert_eq!(stdout, "8024\n");
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
