//! The `bn` compatibility dispatcher (bucket 0.6.0, 2.3 / D-060-01): every
//! subcommand reaches the right sibling executable, stdout/stderr and the
//! exit status pass through unchanged, and the deprecation line never
//! appears on a piped stderr. Needs `bni` and `bnc` built next to `bn`
//! (`cargo build -p bni -p bnc`; the workspace battery does this).
use std::{path::Path, process::Command};

fn bn() -> Command {
    let bn = Path::new(env!("CARGO_BIN_EXE_bn"));
    for sibling in ["bni", "bnc"] {
        assert!(
            bn.with_file_name(sibling).is_file(),
            "{sibling} must be built next to bn: cargo build -p bni -p bnc"
        );
    }
    Command::new(bn)
}

#[test]
fn interpreter_commands_reach_bni_with_clean_channels() {
    let output = bn()
        .args(["eval", "--format", "json", "PRINT 1+1"])
        .output()
        .expect("run bn eval");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty(), "no deprecation line on a pipe");
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("bni's JSON document passes through");
    assert_eq!(envelope["stdout"], "2\n");

    let checked = bn()
        .args(["check", "tests/grammar/valid/print-integer.bn"])
        .output()
        .expect("run bn check");
    assert!(checked.status.success());
    assert!(String::from_utf8_lossy(&checked.stdout).contains("checks passed"));
}

#[test]
fn build_reaches_bnc_with_the_compile_flags() {
    let output = bn()
        .args([
            "build",
            "--target",
            "wasm32",
            "tests/grammar/valid/print-integer.bn",
        ])
        .output()
        .expect("run bn build");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("define i32 @main"));
}

#[test]
fn legacy_compiler_flags_on_interpreter_commands_are_dropped() {
    let output = bn()
        .args([
            "run",
            "--target",
            "wasm32",
            "--opt",
            "3",
            "tests/grammar/valid/print-integer.bn",
        ])
        .output()
        .expect("run bn run --target");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "42");
}

#[test]
fn exit_status_and_stderr_pass_through() {
    let output = bn()
        .args(["run", "tests/grammar/invalid/cross-type-equality.bn"])
        .output()
        .expect("run bn on an invalid program");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error["), "{stderr}");
    assert!(!stderr.contains("deprecated"), "{stderr}");
    assert_eq!(
        bn().args(["frob"])
            .output()
            .expect("run bn frob")
            .status
            .code(),
        Some(2)
    );
    assert_eq!(bn().output().expect("run bn").status.code(), Some(2));
}
