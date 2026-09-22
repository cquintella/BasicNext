//! Integration tests of the `bni` executable surface (bucket 0.6.0, 2.1).
//! The interpreter behaviour itself is covered by the relocated `tests/cli.rs`
//! suite (activity 2.4); here: command set, version, JSON channel purity and
//! the rejection of compiler-only flags.
use std::process::Command;

const WORKSPACE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn bni() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bni"));
    command.current_dir(WORKSPACE);
    command
}

#[test]
fn bni_help_lists_only_interpreter_commands() {
    let output = bni().arg("--help").output().expect("run bni --help");
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    for command in ["eval", "check", "lex", "run", "lsp", "dap"] {
        assert!(
            help.contains(&format!("\n  {command} ")),
            "help missing {command}"
        );
    }
    assert!(!help.contains("\n  build "), "build belongs to bnc");
    assert!(!help.contains("--target"), "compiler flags belong to bnc");
    assert!(help.contains("--module-path"));
}

#[test]
fn bni_version_matches_the_crate() {
    let output = bni().arg("--version").output().expect("run bni --version");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        concat!("bni ", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn bni_runs_and_checks_programs() {
    let ran = bni()
        .args(["run", "tests/grammar/valid/print-integer.bn"])
        .output()
        .expect("run bni run");
    assert!(ran.status.success());
    assert_eq!(String::from_utf8_lossy(&ran.stdout).trim(), "42");
    let checked = bni()
        .args(["check", "tests/grammar/valid/print-integer.bn"])
        .output()
        .expect("run bni check");
    assert!(checked.status.success());
    assert!(String::from_utf8_lossy(&checked.stdout).contains("checks passed"));
}

#[test]
fn bni_eval_json_owns_stdout_and_keeps_stderr_clean() {
    let output = bni()
        .args(["eval", "-vv", "--format", "json", "PRINT 1"])
        .output()
        .expect("run bni eval JSON");
    assert_eq!(output.status.code(), Some(0));
    assert!(
        output.stderr.is_empty(),
        "stderr must stay clean in JSON mode"
    );
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one JSON document on stdout");
    assert_eq!(envelope["schema_version"], 1);
    assert_eq!(envelope["stdout"], "1\n");
}

#[test]
fn bni_rejects_compiler_only_flags_and_the_build_command() {
    let target = bni()
        .args([
            "run",
            "--target",
            "wasm32",
            "tests/grammar/valid/print-integer.bn",
        ])
        .output()
        .expect("run bni --target");
    assert_eq!(target.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&target.stderr).contains("unknown or repeated option '--target'")
    );
    let build = bni()
        .args(["build", "tests/grammar/valid/print-integer.bn"])
        .output()
        .expect("run bni build");
    assert_eq!(build.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&build.stderr).contains("try: bni --help"));
}
