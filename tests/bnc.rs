use std::process::Command;

fn bnc_bin() -> std::path::PathBuf {
    let mut path = std::env::current_exe().expect("current test exe");
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.join("bnc")
}

#[test]
fn bnc_help_succeeds() {
    let output = Command::new(bnc_bin())
        .arg("--help")
        .output()
        .expect("run bnc --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("bnc"));
    assert!(stdout.contains("Profiles (flag-shaped):"));
}

#[test]
fn bnc_version_advertises_0_4_7() {
    let output = Command::new(bnc_bin())
        .arg("--version")
        .output()
        .expect("run bnc --version");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "bnc 0.4.7");
}

#[test]
fn bnc_missing_entry_fails() {
    let output = Command::new(bnc_bin())
        .output()
        .expect("run bnc without args");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn bnc_conflicting_compile_and_check_fails() {
    let output = Command::new(bnc_bin())
        .args(["-c", "--check", "tests/grammar/valid/print-integer.bn"])
        .output()
        .expect("run bnc -c --check");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot specify both -c/--compile and --check"));
}

#[test]
fn bnc_target_without_compile_fails() {
    let output = Command::new(bnc_bin())
        .args(["--target", "wasm32", "tests/grammar/valid/print-integer.bn"])
        .output()
        .expect("run bnc --target without -c");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--target requires -c/--compile"));
}

#[test]
fn bnc_runs_interpreted_by_default() {
    let output = Command::new(bnc_bin())
        .arg("tests/grammar/valid/print-integer.bn")
        .output()
        .expect("run bnc default interpret");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "42");
}

#[test]
fn bnc_checks_program() {
    let output = Command::new(bnc_bin())
        .args(["--check", "tests/grammar/valid/print-integer.bn"])
        .output()
        .expect("run bnc --check");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("passed"));
}

#[test]
fn bnc_compiles_wasm32() {
    let output = Command::new(bnc_bin())
        .args([
            "-c",
            "--target",
            "wasm32",
            "tests/grammar/valid/print-integer.bn",
        ])
        .output()
        .expect("run bnc -c --target wasm32");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("define i32 @main"));
}
