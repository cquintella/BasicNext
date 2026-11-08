// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The pre-commit hook (`scripts/pre-commit`) checks the index, not the
//! working tree: a staged unformatted file is refused, an unstaged one
//! elsewhere does not block, and a staged formatted copy passes even when
//! its working copy is not. Run in a scratch repository whose
//! forbidden-dependency gate is a stub (the real one reads the whole
//! workspace and is checked on the index by `repository-gates.sh` users).

#![allow(
    clippy::disallowed_methods,
    reason = "tests read back the fixture files they write"
)]
use std::{fs, path::Path, process::Command};

use super::support::{TestDir, git};

const HOOK: &str = "scripts/pre-commit";
const FORMATTED: &str = "fn main() {}\n";
const UNFORMATTED: &str = "fn   main( ) {   }\n";

fn scratch_repository(label: &str) -> TestDir {
    let repository = TestDir::new(label).expect("scratch repository");
    fs::create_dir_all(repository.join("scripts")).expect("scripts");
    fs::create_dir_all(repository.join("tests")).expect("tests");
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join(HOOK),
        repository.join(HOOK),
    )
    .expect("copy the hook");
    fs::write(
        repository.join("tests/check-forbidden-deps.sh"),
        "#!/usr/bin/env bash\nexit 0\n",
    )
    .expect("stub gate");
    fs::write(
        repository.join("Cargo.toml"),
        "[workspace.package]\nedition = \"2024\"\n",
    )
    .expect("manifest");
    fs::write(repository.join("first.rs"), FORMATTED).expect("first");
    fs::write(repository.join("second.rs"), FORMATTED).expect("second");
    git(repository.path(), &["init", "--quiet"]);
    git(repository.path(), &["add", "--all"]);
    git(
        repository.path(),
        &["commit", "--quiet", "--message", "base"],
    );
    repository
}

fn hook_passes(repository: &TestDir) -> bool {
    let output = Command::new("bash")
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .current_dir(repository.path())
        .arg(HOOK)
        .output()
        .expect("run the hook");
    output.status.success()
}

#[test]
fn the_hook_refuses_a_staged_unformatted_file() {
    let repository = scratch_repository("pre-commit-unformatted");
    fs::write(repository.join("first.rs"), UNFORMATTED).expect("edit");
    git(repository.path(), &["add", "first.rs"]);
    assert!(!hook_passes(&repository));
}

#[test]
fn the_hook_ignores_an_unstaged_unformatted_file_elsewhere() {
    let repository = scratch_repository("pre-commit-elsewhere");
    fs::write(repository.join("second.rs"), UNFORMATTED).expect("unstaged edit");
    fs::write(
        repository.join("first.rs"),
        "fn main() {\n    let _ = 1;\n}\n",
    )
    .expect("edit");
    git(repository.path(), &["add", "first.rs"]);
    assert!(hook_passes(&repository));
}

#[test]
fn the_hook_reads_the_staged_copy_not_the_working_copy() {
    let repository = scratch_repository("pre-commit-index");
    fs::write(
        repository.join("first.rs"),
        "fn main() {\n    let _ = 1;\n}\n",
    )
    .expect("edit");
    git(repository.path(), &["add", "first.rs"]);
    fs::write(repository.join("first.rs"), UNFORMATTED).expect("working copy");
    assert!(hook_passes(&repository));
}

#[test]
fn the_hook_refuses_a_staged_whitespace_error() {
    let repository = scratch_repository("pre-commit-whitespace");
    fs::write(repository.join("notes.md"), "trailing space \n").expect("notes");
    git(repository.path(), &["add", "notes.md"]);
    assert!(!hook_passes(&repository));
}

const COPY_SKIPPED: &[&str] = &["target", ".git"];
const BNC_MANIFEST: &str = "crates/bnc/Cargo.toml";
const BNC_DEPENDENCIES: &str = "[dependencies]\n";
const FORBIDDEN_WEB_DEPENDENCY: &str = "bn_web = { path = \"../../libraries/bn_web\" }\n";

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("copy directory");
    let mut entries = fs::read_dir(source)
        .expect("read source directory")
        .map(|entry| entry.expect("directory entry"))
        .collect::<Vec<_>>();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let target = destination.join(entry.file_name());
        if path.is_dir() {
            copy_tree(&path, &target);
        } else if path.is_file() {
            fs::copy(&path, &target).expect("copy file");
        }
    }
}

/// The workspace sources (no build output, no history) as a fresh repository,
/// so the index can differ from the working tree on purpose.
fn workspace_repository(label: &str) -> TestDir {
    let copy = TestDir::new(label).expect("workspace copy");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut entries = fs::read_dir(root)
        .expect("read workspace")
        .map(|entry| entry.expect("workspace entry"))
        .collect::<Vec<_>>();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let name = entry.file_name();
        if COPY_SKIPPED.iter().any(|skipped| name == *skipped) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            copy_tree(&path, &copy.join(&name));
        } else if path.is_file() {
            fs::copy(&path, copy.join(&name)).expect("copy file");
        }
    }
    git(copy.path(), &["init", "--quiet"]);
    git(copy.path(), &["add", "--all"]);
    git(copy.path(), &["commit", "--quiet", "--message", "base"]);
    copy
}

fn with_forbidden_dependency(manifest: &str) -> String {
    assert!(
        manifest.contains(BNC_DEPENDENCIES),
        "bnc has a dependency table"
    );
    manifest.replacen(
        BNC_DEPENDENCIES,
        &format!("{BNC_DEPENDENCIES}{FORBIDDEN_WEB_DEPENDENCY}"),
        1,
    )
}

fn cached_dependency_gate_passes(repository: &TestDir) -> bool {
    Command::new("bash")
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .current_dir(repository.path())
        .args(["tests/check-forbidden-deps.sh", "--cached"])
        .output()
        .expect("run the dependency gate")
        .status
        .success()
}

#[test]
fn the_cached_dependency_gate_reads_the_staged_manifest_and_keeps_the_lock() {
    let repository = workspace_repository("pre-commit-dependencies");
    let manifest = fs::read_to_string(repository.join(BNC_MANIFEST)).expect("manifest");
    let lock = fs::read(repository.join("Cargo.lock")).expect("lock");

    fs::write(
        repository.join(BNC_MANIFEST),
        with_forbidden_dependency(&manifest),
    )
    .expect("forbidden dependency");
    git(repository.path(), &["add", BNC_MANIFEST]);
    fs::write(repository.join(BNC_MANIFEST), &manifest).expect("clean working copy");
    assert!(
        !cached_dependency_gate_passes(&repository),
        "a staged forbidden dependency is refused"
    );

    git(repository.path(), &["reset", "--quiet", "--hard"]);
    fs::write(
        repository.join(BNC_MANIFEST),
        with_forbidden_dependency(&manifest),
    )
    .expect("unstaged forbidden dependency");
    assert!(
        cached_dependency_gate_passes(&repository),
        "an unstaged change does not block a clean index"
    );
    assert_eq!(
        fs::read(repository.join("Cargo.lock")).expect("lock after"),
        lock,
        "the gate never rewrites Cargo.lock"
    );
}
