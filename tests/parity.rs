// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Cross-backend tests that need both executables (interpreter and
//! compiler parity, shared-fixture matrices, help/man consistency). They
//! drive `bni` and `bnc` from the workspace target directory (bucket 0.6.0,
//! 2.4; the `bn` dispatcher was removed in 0.6.0 by Carlos's decision).

// Multi-line raw-string BN program templates read more clearly with named
// placeholders than with inlined path expressions.
#![allow(clippy::uninlined_format_args)]

use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};

/// The executables under test live in the workspace `target/<profile>/`;
/// this package has no binary of its own, so build them first
/// (`cargo build -p bni -p bnc`; `scripts/test-battery.sh` does).
fn executable(name: &str) -> Command {
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target");
    path.push(if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    });
    path.push(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    assert!(
        path.is_file(),
        "{name} must be built first: cargo build -p bni -p bnc"
    );
    Command::new(path)
}

fn bni() -> Command {
    executable("bni")
}

fn bnc() -> Command {
    executable("bnc")
}

fn module_roots_snapshot(log: &str) -> Vec<String> {
    // The process log escapes whitespace as `\s` inside values.
    let unescaped = log.replace("\\s", " ");
    unescaped
        .split("module_roots=[")
        .nth(1)
        .and_then(|tail| tail.split(']').next())
        .expect("module_roots snapshot")
        .split("\", \"")
        .map(|item| item.trim().trim_matches(['"', '\\']).to_string())
        .collect()
}

fn native_matches_interpreter(path: &str) {
    let output_path = std::env::temp_dir().join(format!(
        "basicnext-euclid-{}-{}",
        std::process::id(),
        path.replace(['/', '.'], "_")
    ));
    let _ = fs::remove_file(&output_path);
    let built = bnc()
        .args([path, "-o", output_path.to_str().expect("temporary path")])
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
    let interpreted = bni().args(["run", path]).output().expect("run interpreter");
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
    let built = bnc()
        .args([path, "-o", output_path.to_str().expect("temporary path")])
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

    let mut interpreted = bni()
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
#[allow(clippy::too_many_lines)]
fn module_path_is_repeatable_and_first_directory_wins_for_check() {
    let base = std::env::temp_dir().join(format!("bn-module-path-{}", std::process::id()));
    let first = base.join("first");
    let second = base.join("second");
    fs::create_dir_all(&first).expect("first module path");
    fs::create_dir_all(&second).expect("second module path");
    let entry = base.join("main.bn");
    fs::write(
        first.join("Greeting.bn"),
        "EXPORT FUNCTION Value() AS INTEGER\n    RETURN 1\nEND FUNCTION\n",
    )
    .expect("first module");
    fs::write(
        second.join("Greeting.bn"),
        "EXPORT FUNCTION Value() AS INTEGER\n    RETURN 2\nEND FUNCTION\n",
    )
    .expect("second module");
    fs::write(
        &entry,
        "IMPORT Greeting AS G\nFUNCTION Start() AS VOID\n    PRINT G.Value()\nEND FUNCTION\n",
    )
    .expect("entry source");
    let output = bni()
        .args([
            "check",
            "--module-path",
            first.to_str().expect("first path"),
            "--module-path",
            second.to_str().expect("second path"),
            entry.to_str().expect("entry path"),
        ])
        .output()
        .expect("check with module paths");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let run_output = bni()
        .args([
            "run",
            "--module-path",
            first.to_str().expect("first path"),
            "--module-path",
            second.to_str().expect("second path"),
            entry.to_str().expect("entry path"),
        ])
        .output()
        .expect("run with module paths");
    assert_eq!(run_output.status.code(), Some(0));
    assert_eq!(run_output.stdout, b"1\n");
    let reversed = bni()
        .args([
            "run",
            "--module-path",
            second.to_str().expect("second path"),
            "--module-path",
            first.to_str().expect("first path"),
            entry.to_str().expect("entry path"),
        ])
        .output()
        .expect("run with reversed module paths");
    assert_eq!(reversed.status.code(), Some(0));
    assert_eq!(reversed.stdout, b"2\n");
    let artifact = base.join("main");
    let build_output = bnc()
        .args([
            "--module-path",
            first.to_str().expect("first path"),
            "--module-path",
            second.to_str().expect("second path"),
            entry.to_str().expect("entry path"),
            "-o",
            artifact.to_str().expect("artifact path"),
            "--log-file",
            base.join("main.log").to_str().expect("log path"),
            "--log-level",
            "debug",
        ])
        .output()
        .expect("build with module paths");
    assert_eq!(
        build_output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&build_output.stderr)
    );
    let built = std::process::Command::new(&artifact)
        .output()
        .expect("run built module-path artifact");
    assert_eq!(built.status.code(), Some(0));
    assert_eq!(built.stdout, b"1\n");
    let log = fs::read_to_string(base.join("main.log")).expect("module-path process log");
    let canonical_base = fs::canonicalize(&base).expect("canonical base");
    let expected: Vec<String> = [
        (canonical_base.clone(), "entry-dir"),
        (canonical_base.join("modules"), "entry-dir"),
        (
            std::env::current_dir()
                .expect("repository directory")
                .join("modules/bn"),
            "cwd-ancestor",
        ),
        (
            fs::canonicalize(&first).expect("canonical first"),
            "cli-flag",
        ),
        (
            fs::canonicalize(&second).expect("canonical second"),
            "cli-flag",
        ),
    ]
    .iter()
    .map(|(root, provenance)| format!("{} ({provenance})", root.display()))
    .collect();
    assert_eq!(
        module_roots_snapshot(&log),
        expected,
        "effective module-root snapshot with provenance"
    );
    let unescaped = log.replace("\\s", " ");
    assert!(
        unescaped.contains(&format!(
            "Greeting.bn <- {} (cli-flag)",
            fs::canonicalize(&first).expect("canonical first").display()
        )),
        "per-module winning root missing from log: {log}"
    );
    fs::write(
        base.join("config.toml"),
        format!("module-path = [\"{}\"]\n", first.display()),
    )
    .expect("module-path config");
    let configured = bni()
        .args([
            "check",
            "--config",
            base.join("config.toml").to_str().expect("config path"),
            entry.to_str().expect("entry path"),
        ])
        .output()
        .expect("check with configured module path");
    assert_eq!(configured.status.code(), Some(0));
    let merged = bni()
        .args([
            "run",
            "--config",
            base.join("config.toml").to_str().expect("config path"),
            "--module-path",
            second.to_str().expect("second path"),
            entry.to_str().expect("entry path"),
        ])
        .output()
        .expect("run with config and CLI module paths");
    assert_eq!(merged.status.code(), Some(0));
    assert_eq!(merged.stdout, b"1\n");
    let _ = fs::remove_dir_all(base);
}

#[test]
#[allow(clippy::too_many_lines)] // three real CLI scenarios share one fixture tree
fn bn_home_overrides_ancestor_stdlib_and_is_logged() {
    let base = std::env::temp_dir().join(format!("bn-home-cli-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    // Hijack layout: the entry's parent chain carries its own modules/bn.
    let hijack_stdlib = base.join("project/modules/bn");
    let entry_dir = base.join("project/src");
    fs::create_dir_all(&hijack_stdlib).expect("hijack stdlib");
    fs::create_dir_all(&entry_dir).expect("entry dir");
    fs::write(
        hijack_stdlib.join("Shim.bn"),
        "EXPORT FUNCTION Tag() AS INTEGER\n    RETURN 1\nEND FUNCTION\n",
    )
    .expect("hijack module");
    let home_stdlib = base.join("home/modules/bn");
    fs::create_dir_all(&home_stdlib).expect("home stdlib");
    fs::write(
        home_stdlib.join("Shim.bn"),
        "EXPORT FUNCTION Tag() AS INTEGER\n    RETURN 2\nEND FUNCTION\n",
    )
    .expect("home module");
    let entry = entry_dir.join("main.bn");
    fs::write(
        &entry,
        "IMPORT Shim AS M\nFUNCTION Start() AS VOID\n    PRINT M.Tag()\nEND FUNCTION\n",
    )
    .expect("entry");
    let entry_arg = entry.to_str().expect("entry");

    // `run` shows which module won; `build` writes the process-log snapshot.
    let build_log = |label: &str, home: Option<&std::path::Path>| -> String {
        let log_path = base.join(format!("{label}.log"));
        let mut command = bnc();
        match home {
            Some(home) => command.env("BN_HOME", home),
            None => command.env_remove("BN_HOME"),
        };
        let output = command
            .args([
                entry_arg,
                "-o",
                base.join(format!("{label}.bin"))
                    .to_str()
                    .expect("artifact"),
                "--log-file",
                log_path.to_str().expect("log"),
                "--log-level",
                "debug",
            ])
            .output()
            .expect("build for provenance log");
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        fs::read_to_string(&log_path)
            .expect("process log")
            .replace("\\s", " ")
    };

    // Without BN_HOME the ancestor modules/bn wins and the log says so.
    let output = bni()
        .env_remove("BN_HOME")
        .args(["run", entry_arg])
        .output()
        .expect("run without BN_HOME");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"1\n");
    let log = build_log("ancestor", None);
    assert!(log.contains("bn_home=false"), "{log}");
    let hijack_root = format!(
        "{} (entry-ancestor)",
        fs::canonicalize(&hijack_stdlib)
            .expect("canonical hijack")
            .display()
    );
    assert!(log.contains(&hijack_root), "{log}");
    assert!(log.contains(&format!("Shim.bn <- {hijack_root}")), "{log}");

    // With BN_HOME the explicit home wins over the ancestor.
    let home = base.join("home");
    let output = bni()
        .env("BN_HOME", &home)
        .args(["run", entry_arg])
        .output()
        .expect("run with BN_HOME");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"2\n");
    let log = build_log("home", Some(&home));
    assert!(log.contains("bn_home=true"), "{log}");
    let home_root = format!(
        "{} (BN_HOME)",
        fs::canonicalize(&home_stdlib)
            .expect("canonical home")
            .display()
    );
    assert!(log.contains(&home_root), "{log}");
    assert!(log.contains(&format!("Shim.bn <- {home_root}")), "{log}");
    assert!(!log.contains("(entry-ancestor)"), "{log}");

    // BN_HOME pointing nowhere never falls through to the ancestor stdlib.
    let output = bni()
        .env("BN_HOME", base.join("nowhere"))
        .args(["check", entry_arg])
        .output()
        .expect("check with bad BN_HOME");
    assert_ne!(
        output.status.code(),
        Some(0),
        "bad BN_HOME must not resolve via ancestor"
    );
    fs::remove_dir_all(&base).ok();
}
#[test]
fn build_executes_nullable_integer_collection_results_like_interpret() {
    let interpreted = bni()
        .args(["run", "examples/linear_collections.bn"])
        .output()
        .expect("interpret linear collections");
    assert_eq!(interpreted.status.code(), Some(0));

    let artifact =
        std::env::temp_dir().join(format!("bn-linear-collections-{}", std::process::id()));
    let built = bnc()
        .args([
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
            RELEASE file\n\
        END IF\n\
    ELSE\n\
        LET file AS FS.File OR Error = FS.Open(path, FS.WRITE)\n\
        IF file IS Error THEN\n\
            PRINT \"denied\"\n\
        ELSE\n\
            PRINT \"opened\"\n\
            file.Close()\n\
            RELEASE file\n\
        END IF\n\
    END IF\n\
END FUNCTION\n",
    )
    .expect("write filesystem policy fixture");

    let interpreted_deny = bni()
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

    let interpreted_read_only = bni()
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
    let built = bnc()
        .args([
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
    let sandbox_build = bnc()
        .args([
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
fn eval_help_and_manpage_advertise_the_same_entrypoints() {
    // Each executable's help and its man page name the same entrypoints.
    let interpreter_help = bni().arg("--help").output().expect("run bni help");
    assert_eq!(interpreter_help.status.code(), Some(0));
    let interpreter_help = String::from_utf8_lossy(&interpreter_help.stdout);
    let interpreter_man =
        fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/man/bni.1"))
            .expect("read bni man page");
    for command in ["eval", "run", "check", "lex", "lsp", "dap"] {
        assert!(
            interpreter_help.contains(command),
            "bni help missing {command}"
        );
        assert!(
            interpreter_man.contains(command),
            "bni man page missing {command}"
        );
    }
    assert!(
        interpreter_help.contains("--module-path") && interpreter_man.contains("--module-path")
    );
    let compiler_help = bnc().arg("--help").output().expect("run bnc help");
    assert_eq!(compiler_help.status.code(), Some(0));
    let compiler_help = String::from_utf8_lossy(&compiler_help.stdout);
    let compiler_man = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/man/bnc.1"))
        .expect("read bnc man page");
    for flag in ["--target", "--opt", "-o", "--emit"] {
        assert!(compiler_help.contains(flag), "bnc help missing {flag}");
        assert!(compiler_man.contains(flag), "bnc man page missing {flag}");
    }
}

#[test]
fn cli_help_and_version_advertise_0_4_7() {
    for (mut command, name) in [(bni(), "bni"), (bnc(), "bnc")] {
        let version = command.arg("--version").output().expect("run --version");
        assert_eq!(version.status.code(), Some(0));
        assert_eq!(
            String::from_utf8_lossy(&version.stdout).trim(),
            format!("{name} {}", env!("CARGO_PKG_VERSION"))
        );
    }
}

#[test]
fn missing_command_exits_two() {
    assert_eq!(bni().status().expect("run bni").code(), Some(2));
    assert_eq!(bnc().status().expect("run bnc").code(), Some(2));
}

#[test]
fn build_kmp_compiles_through_native_backend() {
    let check = bni()
        .args(["check", "examples/kmp.bn"])
        .output()
        .expect("check KMP");
    assert_eq!(check.status.code(), Some(0));
    let run = bni()
        .args(["run", "examples/kmp.bn"])
        .output()
        .expect("run KMP");
    assert_eq!(run.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&run.stdout).contains("Encontrado padrao no indice  10"));
    let output = bnc()
        .args(["examples/kmp.bn"])
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
fn build_lowers_euclidean_div_and_remainder_matching_interpreter() {
    // Overlapping smoke (div/rem/runtime) lives in tests/test_compiler_parity.py.
    native_matches_interpreter("tests/grammar/valid/build-euclidean-overflow.bn");
    native_matches_interpreter("tests/grammar/valid/build-divide-zero.bn");
}

#[test]
fn build_lowers_power_shift_not_and_string_concat_matching_interpreter() {
    // Overlapping smoke (power-shift*) lives in tests/test_compiler_parity.py.
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
        let interpreted = bni()
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
        let built = bnc()
            .args([path, "-o", artifact.to_str().expect("UTF-8 path")])
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
    // Overlapping smoke (build-widths) lives in tests/test_compiler_parity.py.
    native_matches_interpreter("tests/grammar/valid/build-cast-overflow.bn");
    native_matches_interpreter("tests/grammar/valid/integer-narrowing-conversion.bn");
}

#[test]
fn build_lowers_host_clock_and_console_through_bn_rt() {
    // Overlapping smoke (build-clock, cls-and-beep) lives in tests/test_compiler_parity.py.
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
    let interpreted = bni().args(["run", path]).output().expect("run interpreter");
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
    let built = bnc()
        .args([path, "-o", output_path.to_str().expect("path")])
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
fn build_keeps_distinct_input_values_alive_across_later_reads() {
    native_matches_interpreter_with_input(
        "tests/grammar/valid/build-input-lifetime.bn",
        "first\nsecond\nreplacement\n",
    );
}

#[test]
fn build_folds_pure_function_local_binding() {
    native_matches_interpreter("tests/grammar/valid/print-call-local.bn");
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
fn build_arc_and_dispatch_counterexamples_match_interpreter() {
    for path in [
        "tests/grammar/valid/arc-returned-object.bn",
        "tests/grammar/valid/arc-vector-aliases.bn",
        "tests/grammar/valid/arc-weak-survives-unrelated-allocation.bn",
        "tests/grammar/valid/release-unused-primary.bn",
        "tests/grammar/valid/dispatch-worker-error.bn",
    ] {
        native_matches_interpreter(path);
    }
}

#[test]
fn forbidden_dependency_gate_and_its_negatives_hold() {
    // Fails closed: the harness needs ripgrep and must never be skipped.
    let output = Command::new("bash")
        .arg("tests/check-forbidden-deps.sh")
        .output()
        .expect("run forbidden-deps harness");
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn build_arc_pointer_alias_release_matches_interpreter() {
    native_matches_interpreter("tests/grammar/valid/arc-pointer-alias-release.bn");
}

#[test]
fn build_typed_dispatch_supports_all_mvp_scalar_results() {
    native_matches_interpreter("tests/grammar/valid/dispatch-mvp-scalars.bn");
}

#[test]
fn build_typed_dispatch_supports_all_mvp_scalar_arguments() {
    native_matches_interpreter("tests/grammar/valid/dispatch-mvp-arguments.bn");
}

#[cfg(unix)]
#[test]
fn console_size_uses_stdout_when_stdin_is_piped() {
    let output = Command::new("python3")
        .arg("tests/console_stdout_tty.py")
        .env("BN", bni().get_program())
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

/// Bucket 0.5.2a (2.3 / 2.5, D-P-04) — a malformed policy input stops the
/// process before `Start` on both backends: `CONFIG_INVALID`, exit 2, no
/// program output, never a silent fallback to the defaults.
#[test]
fn malformed_policy_input_is_fail_closed_on_both_backends() {
    let base =
        std::env::temp_dir().join(format!("basicnext-malformed-policy-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("create directory");
    let source = base.join("program.bn");
    fs::write(
        &source,
        "FUNCTION Start() AS VOID\nPRINT \"ran\"\nEND FUNCTION\n",
    )
    .expect("write program");
    let cases = [
        ("BN_EXEC_TIMEOUT_MS", "abc"),
        ("BN_EXEC_CAPTURE_LIMIT", "-5"),
        ("BN_EXEC_POLICY", "allow"),
        ("BN_FS_POLICY", "bogus"),
    ];
    for (variable, value) in cases {
        let run = bni()
            .args(["run"])
            .arg(&source)
            .env(variable, value)
            .output()
            .expect("run with malformed policy");
        assert_eq!(run.status.code(), Some(2), "{variable}={value}");
        assert!(run.stdout.is_empty(), "{variable}={value}");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(stderr.contains(variable), "{variable}={value}: {stderr}");

        let json = bni()
            .args(["eval", "--format", "json", "PRINT 1"])
            .env(variable, value)
            .output()
            .expect("run json with malformed policy");
        assert_eq!(json.status.code(), Some(2), "{variable}={value}");
        let envelope: serde_json::Value =
            serde_json::from_slice(&json.stdout).expect("one JSON envelope");
        assert_eq!(envelope["diagnostics"][0]["code"], "CONFIG_INVALID");
    }

    // Native: the emitted Start checks bn_rt_policy_init and stops. A program
    // that imports a HOST capability is required for the policy prologue.
    let native_source = base.join("native.bn");
    fs::write(
        &native_source,
        "IMPORT HOST.Clock AS Clock\nFUNCTION Start() AS VOID\nIF Clock.Now() > 0 THEN\nPRINT \"ran\"\nEND IF\nEND FUNCTION\n",
    )
    .expect("write native program");
    let binary = base.join("native");
    let build = bnc()
        .args(["-o", binary.to_str().expect("UTF-8 binary path")])
        .arg(&native_source)
        .output()
        .expect("build native program");
    assert_eq!(
        build.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    for (variable, value) in cases {
        let run = std::process::Command::new(&binary)
            .env(variable, value)
            .output()
            .expect("run native with malformed policy");
        assert_eq!(run.status.code(), Some(2), "native {variable}={value}");
        assert!(run.stdout.is_empty(), "native {variable}={value}");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(
            stderr.contains("CONFIG_INVALID") && stderr.contains(variable),
            "native {variable}={value}: {stderr}"
        );
    }
    let ok = std::process::Command::new(&binary)
        .output()
        .expect("run native without policy");
    assert_eq!(String::from_utf8_lossy(&ok.stdout), "ran\n");
    let _ = fs::remove_dir_all(&base);
}
