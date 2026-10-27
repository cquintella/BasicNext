// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Conformance programs: every `tests/conformance/**/<name>.bn` with a
//! `<name>.expected` beside it prints exactly that text on `bni` and on
//! `bnc`. The expected text is derived from the specification by hand (the
//! header of each program says how), never from a backend.

use super::support::{TestDir, bnc, bni, run, workspace_root};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(30);

const CONFORMANCE_DIRECTORY: &str = "tests/conformance";

const EXPECTED_EXTENSION: &str = "expected";

const SOURCE_EXTENSION: &str = "bn";

/// Programs whose expected output is known not to hold yet, with the reason.
/// A ratchet: a listed program must still fail on that backend, and an
/// unlisted one must pass (CLAUDE.md, "Known-failure lists").
const KNOWN_FAILURES: [(&str, &str, &str); 0] = [];

fn conformance_programs(directory: &Path, programs: &mut Vec<PathBuf>) {
    let mut entries = fs::read_dir(directory)
        .expect("conformance directory")
        .map(|entry| entry.expect("conformance entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            conformance_programs(&path, programs);
        } else if path.extension().is_some_and(|ext| ext == SOURCE_EXTENSION)
            && path.with_extension(EXPECTED_EXTENSION).is_file()
        {
            programs.push(path);
        }
    }
}

fn relative(path: &Path) -> String {
    path.strip_prefix(workspace_root())
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn interpreted_output(source: &Path) -> String {
    let output = run(bni().arg("run").arg(source), None, PROCESS_TIMEOUT).expect("run interpreter");
    String::from_utf8_lossy(&output.stdout).into_owned() + &String::from_utf8_lossy(&output.stderr)
}

fn native_output(source: &Path) -> String {
    let directory = TestDir::new("conformance").expect("create directory");
    let artifact = directory.join(format!("program{}", std::env::consts::EXE_SUFFIX));
    let built = bnc()
        .arg(source)
        .arg("-o")
        .arg(&artifact)
        .output()
        .expect("run bnc");
    if !built.status.success() {
        return String::from_utf8_lossy(&built.stderr).into_owned();
    }
    let output =
        run(&mut Command::new(&artifact), None, PROCESS_TIMEOUT).expect("run compiled artifact");
    String::from_utf8_lossy(&output.stdout).into_owned() + &String::from_utf8_lossy(&output.stderr)
}

#[test]
fn conformance_programs_print_their_expected_text_on_both_backends() {
    let mut programs = Vec::new();
    conformance_programs(&workspace_root().join(CONFORMANCE_DIRECTORY), &mut programs);
    assert!(!programs.is_empty(), "no conformance program found");
    let mut problems = Vec::new();
    for source in &programs {
        let name = relative(source);
        let expected =
            fs::read_to_string(source.with_extension(EXPECTED_EXTENSION)).expect("read expected");
        for (backend, actual) in [
            ("bni", interpreted_output(source)),
            ("bnc", native_output(source)),
        ] {
            let known = KNOWN_FAILURES
                .iter()
                .any(|(program, cell, _)| *program == name && *cell == backend);
            match (actual == expected, known) {
                (false, false) => problems.push(format!(
                    "{name} [{backend}]: expected\n{expected}got\n{actual}"
                )),
                (true, true) => problems.push(format!(
                    "{name} [{backend}] passes: remove it from KNOWN_FAILURES"
                )),
                _ => {}
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
