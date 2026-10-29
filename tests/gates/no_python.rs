// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! No Python: the toolchain and its tooling are Rust, Basic Next and shell.
//! The gate fails on a `.py` file anywhere in the source directories and on
//! a script, workflow or test script that runs a Python interpreter. Text
//! that only names Python (a fixture's provenance) is not a call.

use std::{
    fs,
    path::{Path, PathBuf},
};

const SOURCE_DIRECTORIES: &[&str] = &[
    "src",
    "crates",
    "libraries",
    "tests",
    "scripts",
    "fuzz",
    "examples",
    "modules",
    ".github",
];
const SKIPPED_DIRECTORIES: &[&str] = &["target", "corpus", "artifacts"];
const COMMAND_FILE_EXTENSIONS: &[&str] = &["sh", "ps1", "yml", "yaml"];
const INTERPRETER: &str = "python";

fn files_under(directory: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut entries = entries
        .map(|entry| entry.expect("directory entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        let skipped = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| SKIPPED_DIRECTORIES.contains(&name));
        if path.is_dir() && !skipped {
            files_under(&path, files);
        } else if path.is_file() {
            files.push(path);
        }
    }
}

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extensions.contains(&extension))
}

/// A word `python`, `python3` or `python3.12` in a command line.
fn calls_python(line: &str) -> bool {
    line.split(|character: char| !(character.is_ascii_alphanumeric() || character == '.'))
        .any(|word| {
            word.strip_prefix(INTERPRETER).is_some_and(|version| {
                version
                    .chars()
                    .all(|character| character.is_ascii_digit() || character == '.')
            })
        })
}

fn python_findings(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    for directory in SOURCE_DIRECTORIES {
        files_under(&root.join(directory), &mut files);
    }
    let mut findings = Vec::new();
    for path in files {
        let relative = path
            .strip_prefix(root)
            .expect("file under the root")
            .to_string_lossy()
            .replace('\\', "/");
        if has_extension(&path, &["py"]) {
            findings.push(format!("{relative}: Python source"));
            continue;
        }
        if !has_extension(&path, COMMAND_FILE_EXTENSIONS) {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        for (index, line) in text.lines().enumerate() {
            if calls_python(line) {
                findings.push(format!("{relative}:{}: {}", index + 1, line.trim()));
            }
        }
    }
    findings
}

#[test]
fn the_tree_has_no_python() {
    let findings = python_findings(Path::new(env!("CARGO_MANIFEST_DIR")));
    assert!(
        findings.is_empty(),
        "Python is not a project language (Rust, Basic Next, shell):\n{}",
        findings.join("\n")
    );
}

#[test]
fn the_no_python_gate_finds_a_python_file_and_a_python_call() {
    let tree = super::support::TestDir::new("no-python").expect("scratch tree");
    fs::create_dir_all(tree.join("scripts")).expect("scripts");
    fs::create_dir_all(tree.join("tests/fixtures")).expect("fixtures");
    fs::write(tree.join("scripts/tool.py"), "print(1)\n").expect("python file");
    fs::write(tree.join("scripts/run.sh"), "python3 -c 'print(1)'\n").expect("python call");
    fs::write(
        tree.join("tests/fixtures/vectors.json"),
        "{\"_provenance\": \"computed by Python hashlib\"}\n",
    )
    .expect("provenance text");
    let findings = python_findings(tree.path());
    assert_eq!(findings.len(), 2, "{findings:?}");
    assert!(findings[0].starts_with("scripts/run.sh:1:"), "{findings:?}");
    assert_eq!(findings[1], "scripts/tool.py: Python source");
    assert!(!calls_python("pythonic = 1"));
    assert!(calls_python("    python3.12 -m venv .venv"));
}
