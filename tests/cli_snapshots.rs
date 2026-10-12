// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The user-visible text of `bni` and `bnc` (help, option errors) is fixed by
//! `tests/snapshots/` (bucket 0.6.5c S4.a): moving it out of the two
//! `main.rs` files must not change one byte. `{VERSION}` stands for the
//! package version.

mod support;

use std::process::Command;

use support::{bnc, bni, workspace_root};

/// Runs `command`, then compares its status, standard output and standard
/// error with `snapshot` (written to the stream named by `stream`).
fn assert_snapshot(mut command: Command, snapshot: &str, stream: &str, status: i32) {
    let output = command
        .current_dir(workspace_root())
        .output()
        .expect("run the tool");
    let expected = std::fs::read_to_string(workspace_root().join("tests/snapshots").join(snapshot))
        .expect("read snapshot")
        .replace("{VERSION}", env!("CARGO_PKG_VERSION"));
    let (stdout, stderr) = (
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let (actual, other) = if stream == "stdout" {
        (stdout, stderr)
    } else {
        (stderr, stdout)
    };
    assert_eq!(actual, expected, "{snapshot}");
    assert!(other.is_empty(), "{snapshot}: unexpected {other}");
    assert_eq!(output.status.code(), Some(status), "{snapshot}");
}

const ENTRY: &str = "tests/grammar/valid/print-integer.bn";

#[test]
fn help_text_is_unchanged() {
    let mut command = bni();
    command.arg("--help");
    assert_snapshot(command, "bni_help.txt", "stdout", 0);
    let mut command = bnc();
    command.arg("--help");
    assert_snapshot(command, "bnc_help.txt", "stdout", 0);
}

#[test]
fn option_errors_are_unchanged() {
    for (arguments, snapshot, status) in [
        (&["--trace", ENTRY][..], "bnc_trace.txt", 2),
        (&["--no-filesystem", ENTRY][..], "bnc_no_filesystem.txt", 2),
        (&["--jupyter-stdin", ENTRY][..], "bnc_jupyter_stdin.txt", 2),
        (&["--format", "json", ENTRY][..], "bnc_format_json.txt", 2),
        (&["--unknown-opt", ENTRY][..], "bnc_unknown_opt.txt", 2),
        (&["--opt", "bogus", ENTRY][..], "bnc_config_invalid.txt", 2),
    ] {
        let mut command = bnc();
        command.args(arguments);
        assert_snapshot(command, snapshot, "stderr", status);
    }
    // Each option has one owner (known issue K3): an option of the other tool
    // is unknown, and the error is followed by the tool's own help.
    for (arguments, snapshot) in [
        (&["--unknown-opt", ENTRY][..], "bni_unknown_opt.txt"),
        (&["run", "--target", "wasm32", ENTRY][..], "bni_target.txt"),
    ] {
        let mut command = bni();
        command.args(arguments);
        assert_snapshot(command, snapshot, "stderr", 2);
    }
}

/// Bucket 0.6.5c AC6: the two `main.rs` hold only the version line and one
/// call into their driver: together at most 80 lines, no option name, help
/// line or error text.
#[test]
fn both_main_files_stay_minimal() {
    let mut total = 0;
    for path in ["crates/bni/src/main.rs", "crates/bnc/src/main.rs"] {
        let text = std::fs::read_to_string(workspace_root().join(path)).expect("read main.rs");
        total += text.lines().count();
        for forbidden in ["\"--", "\"-", "eprintln!", "println!", "usage:"] {
            assert!(!text.contains(forbidden), "{path} contains {forbidden}");
        }
    }
    assert!(
        total <= 80,
        "bni and bnc main.rs have {total} lines (limit 80)"
    );
}

/// The missing-initializer diagnostic of an alternative names the whole type,
/// not its first member (known issue K20).
#[test]
fn missing_initializer_names_the_whole_alternative() {
    let output = bni()
        .args([
            "check",
            "tests/grammar/invalid/alternative-without-initializer.bn",
        ])
        .current_dir(workspace_root())
        .output()
        .expect("run bni");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("missing initializer in INTEGER(INT32) OR NULL binding."),
        "{stderr}"
    );
}

/// A `STRUCT` field read through a value is not an unused binding (known
/// issue K54): `bni check` passes without a warning.
#[test]
fn read_struct_fields_are_not_unused_bindings() {
    let output = bni()
        .args(["check", "tests/grammar/valid/struct-field-read.bn"])
        .current_dir(workspace_root())
        .output()
        .expect("run bni");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    assert!(!stderr.contains("UNUSED_BINDING"), "{stderr}");
}

/// A vector literal without a declared destination takes the first element's
/// type; `[1, 2.5]` is a `TYPE_MISMATCH` naming the element, not an
/// `INVALID_IR` (decision D-2; known issue K31).
#[test]
fn mixed_numeric_vector_literal_is_a_type_mismatch() {
    for tool in [bni(), bnc()] {
        let mut command = tool;
        if command.get_program().to_string_lossy().contains("bni") {
            command.arg("check");
        } else {
            command.args(["--emit", "llvm"]);
        }
        let output = command
            .arg("tests/grammar/invalid/vector-literal-mixed-numeric.bn")
            .current_dir(workspace_root())
            .output()
            .expect("run the tool");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_ne!(output.status.code(), Some(0), "{stderr}");
        assert!(
            stderr.contains("error[TYPE_MISMATCH]")
                && stderr.contains("Expected INTEGER(INT32)")
                && !stderr.contains("INVALID_IR"),
            "{stderr}"
        );
    }
}
