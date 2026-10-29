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
