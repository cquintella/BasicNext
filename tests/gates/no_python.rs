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
    process::Command,
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
    "language",
    "share",
    "man",
    "config",
    ".github",
];
/// Build and fuzzing output inside the source directories, never sources.
const SKIPPED_PATHS: &[&str] = &["fuzz/target", "fuzz/corpus", "fuzz/artifacts"];
const COMMAND_FILE_EXTENSIONS: &[&str] = &["sh", "ps1", "yml", "yaml"];
const PYTHON_EXTENSIONS: &[&str] = &["py", "pyw", "pyi", "ipynb"];
const INTERPRETER: &str = "python";
const WINDOWS_LAUNCHER: &str = "py";
const SHEBANG: &str = "#!";
const RUST_SPAWN: &str = "Command::new(\"";

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .expect("file under the root")
        .to_string_lossy()
        .replace('\\', "/")
}

fn files_under(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut entries = entries
        .map(|entry| entry.expect("directory entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if SKIPPED_PATHS.contains(&relative(root, &path).as_str()) {
            continue;
        }
        if path.is_dir() {
            files_under(root, &path, files);
        } else if path.is_file() {
            files.push(path);
        }
    }
}

/// The tracked files when `root` is the top of a git work tree (a local
/// `.venv` or scratch file is not the repository's); else every file.
fn candidate_files(root: &Path) -> Vec<PathBuf> {
    let tracked = Command::new("git")
        .current_dir(root)
        .args(["rev-parse", "--show-prefix"])
        .output()
        .ok()
        .filter(|output| output.status.success() && output.stdout.trim_ascii().is_empty())
        .and_then(|_| {
            Command::new("git")
                .current_dir(root)
                .args(["ls-files", "-z"])
                .output()
                .ok()
        });
    let mut files = Vec::new();
    if let Some(listing) = tracked {
        for name in listing
            .stdout
            .split(|byte| *byte == 0)
            .filter(|name| !name.is_empty())
        {
            let name = String::from_utf8_lossy(name).into_owned();
            let in_sources = SOURCE_DIRECTORIES
                .iter()
                .any(|directory| name.starts_with(&format!("{directory}/")));
            let at_root = !name.contains('/');
            if (in_sources || at_root) && !SKIPPED_PATHS.iter().any(|path| name.starts_with(path)) {
                files.push(root.join(name));
            }
        }
        return files;
    }
    for directory in SOURCE_DIRECTORIES {
        files_under(root, &root.join(directory), &mut files);
    }
    if let Ok(entries) = fs::read_dir(root) {
        files.extend(
            entries
                .map(|entry| entry.expect("root entry").path())
                .filter(|path| path.is_file()),
        );
    }
    files.sort();
    files
}

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extensions.contains(&extension.to_ascii_lowercase().as_str()))
}

/// A word `python`, `python3`, `Python3.12`, or the Windows launcher `py`
/// with an option or `py.exe`, in a command line.
fn calls_python(line: &str) -> bool {
    let lowered = line.to_ascii_lowercase();
    let words: Vec<&str> = lowered
        .split(|character: char| !(character.is_ascii_alphanumeric() || ".-".contains(character)))
        .filter(|word| !word.is_empty())
        .collect();
    words.iter().enumerate().any(|(index, word)| {
        let versioned = word.strip_prefix(INTERPRETER).is_some_and(|version| {
            version
                .trim_end_matches(".exe")
                .chars()
                .all(|character| character.is_ascii_digit() || character == '.')
        });
        let launcher = *word == "py.exe"
            || (*word == WINDOWS_LAUNCHER
                && words
                    .get(index + 1)
                    .is_some_and(|next| next.starts_with('-')));
        versioned || launcher
    })
}

/// A Rust line that starts a Python process.
fn spawns_python(line: &str) -> bool {
    line.split(RUST_SPAWN).skip(1).any(|rest| {
        let program = rest.split('"').next().unwrap_or_default();
        calls_python(program)
    })
}

fn python_findings(root: &Path) -> Vec<String> {
    let mut findings = Vec::new();
    for path in candidate_files(root) {
        let name = relative(root, &path);
        if has_extension(&path, PYTHON_EXTENSIONS) {
            findings.push(format!("{name}: Python source"));
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if text
            .lines()
            .next()
            .is_some_and(|first| first.starts_with(SHEBANG) && calls_python(first))
        {
            findings.push(format!("{name}:1: Python shebang"));
            continue;
        }
        let rust = has_extension(&path, &["rs"]);
        if !rust && !has_extension(&path, COMMAND_FILE_EXTENSIONS) {
            continue;
        }
        for (index, line) in text.lines().enumerate() {
            let found = if rust {
                spawns_python(line)
            } else {
                calls_python(line)
            };
            if found {
                findings.push(format!("{name}:{}: {}", index + 1, line.trim()));
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

#[test]
fn the_no_python_gate_finds_the_audited_spellings() {
    let tree = super::support::TestDir::new("no-python-spellings").expect("scratch tree");
    for directory in [
        "scripts",
        "tests/tools",
        "crates/tool/src",
        "fuzz/corpus",
        "examples",
    ] {
        fs::create_dir_all(tree.join(directory)).expect("directory");
    }
    fs::write(
        tree.join("scripts/report"),
        "#!/usr/bin/env python3\nprint(1)\n",
    )
    .expect("shebang");
    fs::write(tree.join("tests/tools/notebook.ipynb"), "{}\n").expect("notebook");
    fs::write(tree.join("scripts/run.ps1"), "py -3 tool\n").expect("launcher");
    fs::write(tree.join("scripts/other.sh"), "Python3 tool\n").expect("capitalised");
    fs::write(
        tree.join("crates/tool/src/lib.rs"),
        "fn f() { let _ = std::process::Command::new(\"python3\"); }\n",
    )
    .expect("spawn");
    fs::write(tree.join("fuzz/corpus/seed.py"), "fuzz input\n").expect("fuzz corpus");
    fs::write(tree.join("examples/corpus.py"), "print(1)\n").expect("not a fuzz corpus");
    let findings = python_findings(tree.path());
    let names: Vec<&str> = findings
        .iter()
        .map(|finding| finding.split(':').next().unwrap_or_default())
        .collect();
    assert_eq!(
        names,
        [
            "crates/tool/src/lib.rs",
            "examples/corpus.py",
            "scripts/other.sh",
            "scripts/report",
            "scripts/run.ps1",
            "tests/tools/notebook.ipynb",
        ],
        "{findings:?}"
    );
    assert!(!calls_python("the pythonic way"));
    assert!(!calls_python("copy -r src dst"));
}
