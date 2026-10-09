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
    let mut command = bni();
    command.args(["--unknown-opt", ENTRY]);
    assert_snapshot(command, "bni_unknown_opt.txt", "stderr", 2);
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
