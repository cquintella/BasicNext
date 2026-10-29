// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! One build source for test prerequisites. A second debug build of the
//! test executables or runtime archives drifts from the first (CI once built
//! only part of them), so every script and workflow calls
//! `scripts/build-test-prereqs.sh`. Release builds (installers, release
//! assets) are a different product and stay where they are.

use std::{fs, path::Path};

const BUILD_SOURCE: &str = "scripts/build-test-prereqs.sh";
/// Installers build release products from a source checkout; their log and
/// error lines name `cargo build` without being a test build.
const RELEASE_INSTALLERS: &[&str] = &["scripts/install.sh", "scripts/install.ps1"];
const SCANNED_DIRECTORIES: &[&str] = &["scripts", ".github/workflows", "tests"];
const PREREQUISITE_PACKAGES: &[&str] = &["bni", "bnc", "bn_rt", "bn_stdlib", "bn_web"];

fn scanned_files(directory: &Path, files: &mut Vec<std::path::PathBuf>) {
    let mut entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("{}: {error}", directory.display()))
        .map(|entry| entry.expect("directory entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            scanned_files(&path, files);
        } else if path
            .extension()
            .is_some_and(|extension| matches!(extension.to_str(), Some("sh" | "ps1" | "yml")))
        {
            files.push(path);
        }
    }
}

/// A debug `cargo build` that names a prerequisite package.
fn builds_a_prerequisite(line: &str) -> bool {
    let Some(arguments) = line.split_once("cargo build").map(|(_, rest)| rest) else {
        return false;
    };
    if arguments.contains("--release") {
        return false;
    }
    let words: Vec<&str> = arguments.split_whitespace().collect();
    words.windows(2).any(|pair| {
        pair[0] == "-p" && PREREQUISITE_PACKAGES.contains(&pair[1].trim_end_matches(')'))
    })
}

fn duplicate_builds(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    for directory in SCANNED_DIRECTORIES {
        scanned_files(&root.join(directory), &mut files);
    }
    let mut duplicates = Vec::new();
    for path in files {
        let relative = path
            .strip_prefix(root)
            .expect("scanned file under the root")
            .to_string_lossy()
            .replace('\\', "/");
        if relative == BUILD_SOURCE || RELEASE_INSTALLERS.contains(&relative.as_str()) {
            continue;
        }
        let text =
            fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        for (index, line) in text.lines().enumerate() {
            if builds_a_prerequisite(line) {
                duplicates.push(format!("{relative}:{}: {}", index + 1, line.trim()));
            }
        }
    }
    duplicates
}

#[test]
fn test_prerequisites_are_built_by_one_script() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let duplicates = duplicate_builds(root);
    assert!(
        duplicates.is_empty(),
        "build test prerequisites only through {BUILD_SOURCE}:\n{}",
        duplicates.join("\n")
    );
}

#[test]
fn the_one_build_gate_rejects_a_second_prerequisite_build() {
    assert!(builds_a_prerequisite(
        "          cargo build --locked -p bn_rt -p bn_stdlib -p bn_web"
    ));
    assert!(builds_a_prerequisite(
        "    (cd \"$ROOT_DIR\" && cargo build -p bnr -p bni)"
    ));
    assert!(!builds_a_prerequisite(
        "  cargo build -p bn_rt -p bn_stdlib --release"
    ));
    assert!(!builds_a_prerequisite("    cargo build --locked -p bnr"));
}

/// A scratch git repository holding the build script, as a workspace.
fn scratch_workspace(label: &str) -> super::support::TestDir {
    let workspace = super::support::TestDir::new(label).expect("scratch workspace");
    fs::create_dir_all(workspace.join("scripts")).expect("scripts directory");
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join(BUILD_SOURCE),
        workspace.join(BUILD_SOURCE),
    )
    .expect("copy the build script");
    fs::write(workspace.join("source.rs"), "fn main() {}\n").expect("write a source");
    fs::write(workspace.join(".gitignore"), "/ignored/\n/target/\n").expect("write .gitignore");
    git(&workspace, &["init", "--quiet"]);
    git(&workspace, &["add", "--all"]);
    git(&workspace, &["commit", "--quiet", "--message", "base"]);
    workspace
}

fn git(workspace: &super::support::TestDir, arguments: &[&str]) {
    let output = std::process::Command::new("git")
        .current_dir(workspace.path())
        .args([
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@example.invalid",
        ])
        .args(["-c", "commit.gpgsign=false"])
        .args(arguments)
        .output()
        .expect("run git");
    assert!(output.status.success(), "git {arguments:?}: {output:?}");
}

fn stamp(workspace: &super::support::TestDir) -> String {
    super::support::prerequisites::current_stamp(workspace.path()).expect("tree stamp")
}

#[test]
fn the_tree_stamp_follows_every_source_change() {
    let workspace = scratch_workspace("prereqs-stamp");
    let base = stamp(&workspace);
    assert_eq!(stamp(&workspace), base, "the stamp is deterministic");

    fs::write(workspace.join("source.rs"), "fn main() { () }\n").expect("edit");
    let unstaged = stamp(&workspace);
    assert_ne!(unstaged, base, "an unstaged edit changes the stamp");
    git(&workspace, &["add", "source.rs"]);
    assert_ne!(stamp(&workspace), base, "a staged edit changes the stamp");
    git(&workspace, &["reset", "--quiet", "--hard"]);
    assert_eq!(stamp(&workspace), base, "reverting restores the stamp");

    fs::write(workspace.join("new.rs"), "// new\n").expect("untracked file");
    let untracked = stamp(&workspace);
    assert_ne!(untracked, base, "an untracked file changes the stamp");
    fs::write(workspace.join("new.rs"), "// edited\n").expect("edit untracked");
    assert_ne!(
        stamp(&workspace),
        untracked,
        "its content counts, not only its name"
    );
    fs::remove_file(workspace.join("new.rs")).expect("remove untracked");

    fs::create_dir_all(workspace.join("ignored")).expect("ignored directory");
    fs::write(workspace.join("ignored/build.log"), "output\n").expect("ignored file");
    assert_eq!(stamp(&workspace), base, "an ignored file does not count");

    git(
        &workspace,
        &["commit", "--quiet", "--allow-empty", "--message", "next"],
    );
    assert_ne!(stamp(&workspace), base, "a new HEAD changes the stamp");
}

#[test]
fn a_stale_or_missing_build_is_refused() {
    use super::support::prerequisites::{STALE_MESSAGE, STAMP_FILE, check};
    let workspace = scratch_workspace("prereqs-check");
    let profile = workspace.join("target/debug");
    fs::create_dir_all(&profile).expect("profile directory");

    let missing = check(workspace.path(), &profile).expect_err("no stamp yet");
    assert!(missing.contains(STALE_MESSAGE), "{missing}");

    fs::write(profile.join(STAMP_FILE), format!("{}\n", stamp(&workspace))).expect("stamp");
    check(workspace.path(), &profile).expect("a build of this tree is fresh");

    fs::write(workspace.join("source.rs"), "fn main() { () }\n").expect("edit");
    let stale = check(workspace.path(), &profile).expect_err("the tree moved on");
    assert!(stale.contains(STALE_MESSAGE), "{stale}");
}

#[test]
fn every_override_set_to_a_file_skips_the_stamp() {
    use super::support::prerequisites::{OVERRIDES, overrides_complete};
    use std::ffi::OsString;
    let existing = OsString::from(env!("CARGO_MANIFEST_DIR").to_owned() + "/Cargo.toml");
    assert!(overrides_complete(|_| Some(existing.clone())));
    assert!(!overrides_complete(
        |name| (name != OVERRIDES[2]).then(|| existing.clone())
    ));
    assert!(!overrides_complete(|_| Some(OsString::from(
        "no/such/archive.a"
    ))));
}
