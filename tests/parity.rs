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

#[cfg(unix)]
use std::{fs::File, io::Read, os::unix::ffi::OsStrExt, path::Path, thread, time::Duration};

#[cfg(unix)]
use rustix::{
    pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt},
    termios::{Winsize, tcsetwinsize},
};
#[cfg(unix)]
use wait_timeout::ChildExt;

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

/// The process log's text with its field escapes undone (`\\` is `\`, `\s`
/// is a space) and every path separator written `/`: values may hold
/// `Debug` text, which doubles a Windows `\` once more.
fn log_text(log: &str) -> String {
    let mut text = String::with_capacity(log.len());
    let mut characters = log.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            text.push(character);
            continue;
        }
        match characters.next() {
            Some('s') => text.push(' '),
            Some('n') => text.push('\n'),
            Some('r') => text.push('\r'),
            Some(other) => text.push(other),
            None => text.push('\\'),
        }
    }
    slashes(&text)
}

/// `text` with `\` separators as `/` and repeated separators collapsed, so a
/// path compares equal whether it was printed with `Display` or `Debug`.
fn slashes(text: &str) -> String {
    let mut text = text.replace('\\', "/");
    while text.contains("//") {
        text = text.replace("//", "/");
    }
    text
}

fn module_roots_snapshot(log: &str) -> Vec<String> {
    log_text(log)
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
    .map(|(root, provenance)| slashes(&format!("{} ({provenance})", root.display())))
    .collect();
    assert_eq!(
        module_roots_snapshot(&log),
        expected,
        "effective module-root snapshot with provenance"
    );
    let unescaped = log_text(&log);
    assert!(
        unescaped.contains(&slashes(&format!(
            "Greeting.bn <- {} (cli-flag)",
            fs::canonicalize(&first).expect("canonical first").display()
        ))),
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
        log_text(&fs::read_to_string(&log_path).expect("process log"))
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
    let hijack_root = slashes(&format!(
        "{} (entry-ancestor)",
        fs::canonicalize(&hijack_stdlib)
            .expect("canonical hijack")
            .display()
    ));
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
    let home_root = slashes(&format!(
        "{} (BN_HOME)",
        fs::canonicalize(&home_stdlib)
            .expect("canonical home")
            .display()
    ));
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
    // 0.6.md "`HOST.FileSystem` execution policy": a denied operation returns
    // `Error` on both backends.
    assert_eq!(
        interpreted_read_only.status.code(),
        Some(0),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&interpreted_read_only.stdout),
        String::from_utf8_lossy(&interpreted_read_only.stderr)
    );
    assert!(String::from_utf8_lossy(&interpreted_read_only.stdout).contains("denied"));
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
    let interpreter_man = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/man/bni.1"))
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
    let compiler_man = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/man/bnc.1"))
        .expect("read bnc man page");
    for flag in ["--target", "--opt", "-o", "--emit"] {
        assert!(compiler_help.contains(flag), "bnc help missing {flag}");
        assert!(compiler_man.contains(flag), "bnc man page missing {flag}");
    }
}

#[test]
fn cli_version_advertises_package_version() {
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

/// `IS` on HOST types through an import alias: the frontend resolves the
/// canonical name once, and both backends test it.
#[test]
fn is_on_host_types_matches_across_backends() {
    let path = "tests/grammar/valid/is-host-types.bn";
    let output = bni().args(["run", path]).output().expect("run fixture");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "TRUE FALSE\nFALSE TRUE\nTRUE\n"
    );
    native_matches_interpreter(path);
}

/// 0.6.2 C3: `AS STRING` yields the `PRINT` text on both backends, and
/// `FLOAT32` text round-trips to `FLOAT32` (console.md).
#[test]
fn as_string_matches_print_text_across_backends() {
    let path = "tests/grammar/valid/as-string.bn";
    let output = bni().args(["run", path]).output().expect("run fixture");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "[-42]\n[9000000000] [200] [18446744073709551615]\n[0.25] [2.0] [0.5]\n\
         [0.30000000000000004] [7] [TRUE]\n[INF] [-INF] [NAN]\n[0.1] [-128] [FALSE]\n\
         -42 9000000000 200 18446744073709551615 0.25 2.0 0.5 0.30000000000000004 TRUE 0.1 -128\n"
    );
    native_matches_interpreter(path);
}

/// `Error.Operation` and `Error.Cause` exist on both backends and read alike
/// (the native value is a `bn_rt` error record).
#[test]
fn error_fields_read_alike_across_backends() {
    native_matches_interpreter("tests/grammar/valid/error-fields.bn");
}

/// `IS T` on `STRING`/`BOOLEAN`/`FLOAT OR Error`, and a narrowed STRING used
/// as a STRING: `bnc` answered FALSE for every such test before.
#[test]
fn is_on_error_alternatives_matches_across_backends() {
    let path = "tests/grammar/valid/is-error-alternatives.bn";
    let output = bni().args(["run", path]).output().expect("run fixture");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "FALSE FALSE FALSE\nTRUE TRUE TRUE FALSE\npardal! 6\nFALSE TRUE\n"
    );
    native_matches_interpreter(path);
}

/// Every HOST.FileSystem error code a program can provoke portably, with its
/// operation, message, and cause, is identical on both backends
/// (language/0.6/host.md, "File system errors"; error.md). A denied
/// operation returns `Error` (0.6.md).
#[test]
fn filesystem_errors_report_code_operation_message_and_cause() {
    let fixture = std::path::absolute("tests/host/fs_errors.bn").expect("fixture path");
    let directory =
        std::env::temp_dir().join(format!("basicnext-fs-errors-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("create directory");
    fs::write(directory.join("bytes.bin"), [0x61, 0xff]).expect("write bytes");
    let artifact = directory.join(format!("fs-errors{}", std::env::consts::EXE_SUFFIX));
    let built = bnc()
        .arg(&fixture)
        .arg("-o")
        .arg(&artifact)
        .output()
        .expect("run bnc");
    assert_eq!(
        built.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let interpreted = bni()
        .arg("run")
        .arg(&fixture)
        .current_dir(&directory)
        .env("BN_FS_POLICY", "read-only")
        .output()
        .expect("run bni");
    let compiled = Command::new(&artifact)
        .current_dir(&directory)
        .env("BN_FS_POLICY", "read-only")
        .output()
        .expect("run artifact");
    assert_eq!(interpreted.status.code(), Some(0));
    assert_eq!(compiled.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&interpreted.stdout),
        "NOT_FOUND 2 HOST.FileSystem.Open\n  cannot open \"no-such-file.txt\" for READ\nTRUE TRUE\n\
         POLICY_DENIED 9 HOST.FileSystem.Open\n  cannot open \"fs-errors.txt\" for WRITE\n  the execution policy allows reads only\n\
         IS_DIRECTORY 4 HOST.FileSystem.Open\n  cannot open \".\" for READ\n  the path is a directory, not a file\n\
         INVALID_ARGUMENT 1 HOST.FileSystem.Open\n  cannot open \"bytes.bin\" in mode 7\n  the mode must be FS.READ (0), FS.WRITE (1), or FS.APPEND (2)\n\
         INVALID_UTF8 TRUE HOST.FileSystem.File.ReadAll\n  cannot read \"bytes.bin\"\n\
         CLOSED 5 HOST.FileSystem.File.ReadAll\n  cannot read \"bytes.bin\"\n  the file is closed\n\
         WRONG_FAMILY 6 HOST.FileSystem.File.ReadLine\n  cannot read a line from \"bytes.bin\"\n  the file is in binary use; a file keeps the family of its first successful method until Close\n\
         1\n\
         Error 9 in HOST.FileSystem.DeleteFile: cannot delete \"fs-errors.txt\" (cause: the execution policy allows reads only)\n"
    );
    assert_eq!(compiled.stdout, interpreted.stdout);
    let _ = fs::remove_dir_all(directory);
}

/// HOST.Net errors that need no network (argument checks before any socket
/// or resolver call) report the same code, operation, message, and cause on
/// both backends (host-net.md "Errors"; error.md).
#[test]
fn net_argument_errors_match_across_backends() {
    let path = "tests/host/net_errors.bn";
    let output = bni().args(["run", path]).output().expect("run fixture");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Error 1 in HOST.Net.Address.Parse: cannot parse \"10.0.0.300\" as an IP address (cause: an address is IPv4 (four decimal octets, 192.0.2.1) or IPv6 (hexadecimal groups, 2001:db8::1), without a port or a name)\n\
         Error 1 in HOST.Net.Ping: cannot ping 127.0.0.1 (cause: the timeout must be within 1..60000 ms; got 0)\n\
         Error 1 in HOST.Net.Reverse: cannot find the name of 127.0.0.1 (cause: the timeout must be within 1..60000 ms; got 60001)\n\
         TRUE HOST.Net.Resolve\n\
         cannot resolve \"localhost\" | the timeout must be within 1..60000 ms; got -5\n"
    );
    native_matches_interpreter(path);
}

/// TCP errors provoked on loopback (address in use, read after Close,
/// refused connection) carry the same codes, operations, and causes on both
/// backends.
#[test]
fn tcp_errors_match_across_backends() {
    let path = "tests/host/net_tcp_errors.bn";
    let output = bni().args(["run", path]).output().expect("run fixture");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "TRUE HOST.Net.TCPListen\n\
         TRUE HOST.Net.TCPStream.Read it was closed by Close\n\
         TRUE HOST.Net.TCPConnect\n"
    );
    native_matches_interpreter(path);
}

/// UDP results on loopback (address in use, timeout, invalid arguments, a
/// broadcast destination, a truncated datagram, use after Close, idempotent
/// Close) are the same on both backends (host-net.md "UDP", "Errors").
#[test]
fn udp_results_match_across_backends() {
    let path = "tests/host/net_udp_errors.bn";
    let output = bni().args(["run", path]).output().expect("run fixture");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "TRUE HOST.Net.UDPBind\n\
         TRUE no answer within 1 ms\n\
         TRUE a receive takes at least 1 byte; got 0\n\
         TRUE HOST.Net.UDPSocket.SendTo\n\
         received 3 TRUE 3 104 108\n\
         TRUE HOST.Net.UDPSocket.Receive it was closed by Close\n\
         TRUE HOST.Net.UDPSocket.SendTo\n"
    );
    native_matches_interpreter(path);
}

/// HOST.Exec failures carry the shared `bn_host_exec` report (Code,
/// Operation, Message naming the program, Cause) on both backends, and
/// `PRINT` of an `Error` narrowed out of `Exec.Result OR Error` prints the
/// report natively too (it printed the record pointer as text).
#[test]
fn exec_errors_match_across_backends() {
    let path = "tests/host/exec_errors.bn";
    let output = bni().args(["run", path]).output().expect("run fixture");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "2 HOST.Exec.Run cannot run \"bn-exec-errors-no-such-program\"\n\
         Error 1 in HOST.Exec.Run: cannot run \"\" (cause: the program must be non-empty and contain no NUL)\n"
    );
    native_matches_interpreter(path);
}

/// A byte count past the BN buffer stops both backends before any I/O; the
/// native runtime would otherwise read past the vector.
#[test]
fn network_byte_counts_past_the_buffer_stop_both_backends() {
    let path = "tests/host/net_buffer_bound.bn";
    let output = bni().args(["run", path]).output().expect("run fixture");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "before\n");
    native_matches_interpreter(path);
}

/// `examples/conversions.bn`: every `AS` conversion and the `PRINT` text of
/// each type. `tenth AS FLOAT64` also guards native constant folding, which
/// must keep a `FLOAT32` constant at f32 precision.
#[test]
fn conversions_example_matches_spec_and_native() {
    let path = "examples/conversions.bn";
    let output = bni().args(["run", path]).output().expect("run example");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "PRINT 1 2.5 TRUE text\n\nwiden 200 narrow 1000\nto float 7.0\ntruncate 3 -3\n\
         float 2.0 0.30000000000000004 0.1 0.10000000149011612\nspecial NAN INF -INF\n\
         boolean FALSE TRUE FALSE\nboolean FALSE TRUE\nitems: 7, ratio: 0.25\n[0.1] [TRUE]\n"
    );
    native_matches_interpreter(path);
}

/// 0.6.2 C1/C2: negative constant literals and integer literals for FLOAT
/// bindings; expected lines are fixed in `language/0.6/0.6.md`.
#[test]
fn constant_literals_match_spec_and_native() {
    for (path, expected) in [
        (
            "tests/modules/constants/main.bn",
            "-1   INT32\n-255   INT32\n-2.5   FLOAT64\n-9000000000   INT64\n\
             4294967295   UINT32\n255.0   FLOAT64\n-16.0   FLOAT64\n\
             3.141592653589793   6.283185307179586\n",
        ),
        (
            "tests/grammar/valid/integer-literal-float-binding.bn",
            "255.0   255.0   -5.0\n3.0   FLOAT32\n3.0   16777216.0\n",
        ),
        (
            "tests/grammar/valid/negative-local-constants.bn",
            "-1   INT32\n-2.5   FLOAT64\n-255   -256\n",
        ),
    ] {
        let output = bni().args(["run", path]).output().expect("run fixture");
        assert_eq!(output.status.code(), Some(0), "{path}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{path}");
        native_matches_interpreter(path);
    }
}

#[test]
fn build_lowers_euclidean_div_and_remainder_matching_interpreter() {
    // Overlapping smoke (div/rem/runtime) lives in tests/compiler_parity.rs.
    native_matches_interpreter("tests/grammar/valid/build-euclidean-overflow.bn");
    native_matches_interpreter("tests/grammar/valid/build-divide-zero.bn");
}

#[test]
fn build_lowers_power_shift_not_and_string_concat_matching_interpreter() {
    // Overlapping smoke (power-shift*) lives in tests/compiler_parity.rs.
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
    // Overlapping smoke (build-widths) lives in tests/compiler_parity.rs.
    native_matches_interpreter("tests/grammar/valid/build-cast-overflow.bn");
    native_matches_interpreter("tests/grammar/valid/integer-narrowing-conversion.bn");
}

#[test]
fn build_lowers_host_clock_and_console_through_bn_rt() {
    // Overlapping smoke (build-clock, cls-and-beep) lives in tests/compiler_parity.rs.
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
fn build_lowers_bnjson_parse_stringify_matching_interpreter() {
    native_matches_interpreter("tests/grammar/valid/bnjson-parse-stringify.bn");
}

#[test]
fn build_lowers_bnjson_dom_object_matching_interpreter() {
    native_matches_interpreter("tests/grammar/valid/bnjson-dom-object.bn");
}

#[test]
fn build_lowers_bnjson_companion_matching_interpreter() {
    native_matches_interpreter("tests/modules/bnjson-companion/main.bn");
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
    // An explicit PATH makes Windows search it before System32, whose
    // `bash.exe` is the WSL launcher, not the Git Bash that runs CI.
    let output = Command::new("bash")
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
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
    let winsize = Winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let master =
        openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).expect("open pseudo-terminal master");
    grantpt(&master).expect("grant pseudo-terminal slave");
    unlockpt(&master).expect("unlock pseudo-terminal slave");
    let slave_name = ptsname(&master, Vec::new()).expect("resolve pseudo-terminal slave");
    let slave_path = Path::new(std::ffi::OsStr::from_bytes(slave_name.to_bytes()));
    let slave = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(slave_path)
        .expect("open pseudo-terminal slave");
    tcsetwinsize(&slave, winsize).expect("set pseudo-terminal to 80x24");
    let mut master = File::from(master);

    // Only stdout is the PTY; stdin stays a pipe (closed at once).
    let mut child = bni()
        .args(["run", "tests/grammar/valid/console-size.bn"])
        .stdin(Stdio::piped())
        .stdout(Stdio::from(slave))
        .stderr(Stdio::piped())
        .spawn()
        .expect("run console-size with stdout attached to a PTY");
    drop(child.stdin.take());

    let stdout_reader = thread::spawn(move || {
        let mut output = Vec::new();
        let mut buffer = [0_u8; 256];
        loop {
            match master.read(&mut buffer) {
                // PTY masters commonly report EIO after the slave closes.
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    output.extend_from_slice(&buffer[..read]);
                    if output.contains(&b'\n') {
                        break;
                    }
                }
            }
        }
        output
    });
    let mut stderr = child.stderr.take().expect("capture bni stderr");
    let stderr_reader = thread::spawn(move || {
        let mut output = Vec::new();
        stderr
            .read_to_end(&mut output)
            .expect("read bni stderr to EOF");
        output
    });

    let Some(status) = child
        .wait_timeout(Duration::from_secs(5))
        .expect("wait for console-size")
    else {
        child.kill().expect("kill timed-out console-size process");
        let _ = child.wait();
        panic!("console-size timed out after 5 seconds");
    };
    let stdout = stdout_reader.join().expect("join PTY reader");
    let stderr = stderr_reader.join().expect("join stderr reader");
    assert_eq!(
        status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&stderr)
    );
    let stdout = String::from_utf8_lossy(&stdout).replace('\r', "");
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
