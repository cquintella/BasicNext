//! Integration tests of the `bnc` executable (Clang-like surface).
//! Relocated from the root `tests/bnc.rs` (bucket 0.6.0, 2.2/2.4); tests
//! of the deleted wrapper profiles (`-c`, `--check`, default interpret)
//! were retired with those flags (D-060-07).
use std::process::Command;

const WORKSPACE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn bnc() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bnc"));
    command.current_dir(WORKSPACE);
    command
}

#[test]
fn bnc_help_succeeds() {
    let output = bnc().arg("--help").output().expect("run bnc --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("usage: bnc [compile-options] <entry.bn>"));
    assert!(stdout.contains("--target native|wasm32"));
    assert!(
        !stdout.contains("-c, --compile"),
        "the wrapper profile flags are gone"
    );
}

#[test]
fn bnc_version_advertises_0_4_7() {
    let output = bnc().arg("--version").output().expect("run bnc --version");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        concat!("bnc ", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn bnc_missing_entry_fails() {
    let output = bnc().output().expect("run bnc without args");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn bnc_rejects_removed_wrapper_flags() {
    for flag in ["-c", "--check"] {
        let output = bnc()
            .args([flag, "tests/grammar/valid/print-integer.bn"])
            .output()
            .expect("run bnc with a removed flag");
        assert_eq!(output.status.code(), Some(2), "{flag}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("unknown or repeated option"));
    }
}

#[test]
fn bnc_compiles_wasm32() {
    let output = bnc()
        .args(["--target", "wasm32", "tests/grammar/valid/print-integer.bn"])
        .output()
        .expect("run bnc --target wasm32");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("define i32 @main"));
}

#[test]
fn bnc_builds_and_runs_a_native_executable() {
    let artifact = std::env::temp_dir().join(format!("bnc-native-{}", std::process::id()));
    let built = bnc()
        .args(["tests/grammar/valid/print-integer.bn", "-o"])
        .arg(&artifact)
        .output()
        .expect("run bnc -o");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let ran = Command::new(&artifact).output().expect("run the artifact");
    let _ = std::fs::remove_file(&artifact);
    assert!(ran.status.success());
    assert_eq!(String::from_utf8_lossy(&ran.stdout).trim(), "42");
}

#[test]
fn bnc_emit_ir_prints_validated_bn_ir_without_compiling() {
    let output = bnc()
        .args(["--emit", "ir", "tests/grammar/valid/print-integer.bn"])
        .output()
        .expect("run bnc --emit ir");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Print"), "BN IR, not LLVM: {stdout}");
    assert!(!stdout.contains("define i32 @main"));
}

#[test]
fn bnc_language_error_exits_one_with_a_diagnostic() {
    let output = bnc()
        .args(["tests/grammar/invalid/cross-type-equality.bn"])
        .output()
        .expect("run bnc on an invalid program");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("error["));
}
