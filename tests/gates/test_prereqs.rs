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

/// The packages a `cargo build` command line names (`-p x`, `-px`, `-p=x`,
/// `--package x`, `--package=x`).
fn named_packages(arguments: &str) -> Vec<String> {
    let words: Vec<&str> = arguments.split_whitespace().collect();
    let mut packages = Vec::new();
    for (index, word) in words.iter().enumerate() {
        let named = match *word {
            "-p" | "--package" => words.get(index + 1).copied(),
            _ => word
                .strip_prefix("--package=")
                .or_else(|| word.strip_prefix("-p="))
                .or_else(|| word.strip_prefix("-p").filter(|rest| !rest.is_empty())),
        };
        if let Some(name) = named {
            packages.push(
                name.trim_matches(|character: char| "()'\";`".contains(character))
                    .to_owned(),
            );
        }
    }
    packages
}

/// A debug `cargo build` that may build a prerequisite: one that names a
/// prerequisite package, or names no package at all (the workspace default
/// members or `--workspace` include every prerequisite).
fn builds_a_prerequisite(line: &str) -> bool {
    let Some(arguments) = line.split_once("cargo build").map(|(_, rest)| rest) else {
        return false;
    };
    if arguments.contains("--release") {
        return false;
    }
    let packages = named_packages(arguments);
    packages.is_empty()
        || packages
            .iter()
            .any(|package| PREREQUISITE_PACKAGES.contains(&package.as_str()))
}

/// Command lines with shell (`\`) and PowerShell (`` ` ``) continuations
/// joined, each with the number of its first line.
fn command_lines(text: &str) -> Vec<(usize, String)> {
    let mut lines = Vec::new();
    let mut pending: Option<(usize, String)> = None;
    for (index, line) in text.lines().enumerate() {
        let continued = line.trim_end().ends_with(['\\', '`']);
        let content = line.trim_end().trim_end_matches(['\\', '`']);
        let (number, mut joined) = pending.take().unwrap_or((index + 1, String::new()));
        joined.push(' ');
        joined.push_str(content);
        if continued {
            pending = Some((number, joined));
        } else {
            lines.push((number, joined));
        }
    }
    lines.extend(pending);
    lines
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
        for (number, line) in command_lines(&text) {
            if builds_a_prerequisite(&line) {
                duplicates.push(format!("{relative}:{number}: {}", line.trim()));
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
    assert!(builds_a_prerequisite(
        "    cargo build --locked --workspace --bins"
    ));
    assert!(builds_a_prerequisite("        cargo build --locked"));
    assert!(builds_a_prerequisite("cargo build --package bn_rt"));
    assert!(builds_a_prerequisite("cargo build --package=bnc"));
    assert!(builds_a_prerequisite("cargo build -pbni"));
    let continued = command_lines("cargo build --locked \\\n    -p bn_web\nnext\n");
    assert_eq!(continued[0].0, 1);
    assert!(builds_a_prerequisite(&continued[0].1));
    assert!(!builds_a_prerequisite(&continued[1].1));
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
    super::support::git(workspace.path(), &["init", "--quiet"]);
    super::support::git(workspace.path(), &["add", "--all"]);
    super::support::git(
        workspace.path(),
        &["commit", "--quiet", "--message", "base"],
    );
    workspace
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
    super::support::git(workspace.path(), &["add", "source.rs"]);
    assert_ne!(stamp(&workspace), base, "a staged edit changes the stamp");
    super::support::git(workspace.path(), &["reset", "--quiet", "--hard"]);
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

    super::support::git(
        workspace.path(),
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

/// Whether the build script sees `workspace` as a git work tree of its own.
fn script_sees_git(workspace: &Path) -> bool {
    std::process::Command::new("bash")
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .current_dir(workspace)
        .args([
            "-c",
            "source scripts/build-test-prereqs.sh && test_prereqs_in_git",
        ])
        .status()
        .expect("run the build script")
        .success()
}

#[test]
fn a_source_copy_without_its_own_git_tree_is_not_stamped() {
    let repository = scratch_workspace("prereqs-git-top");
    assert!(script_sees_git(repository.path()));

    let copy = super::support::TestDir::new("prereqs-no-git").expect("plain copy");
    fs::create_dir_all(copy.join("scripts")).expect("scripts");
    fs::copy(repository.join(BUILD_SOURCE), copy.join(BUILD_SOURCE)).expect("copy script");
    assert!(!script_sees_git(copy.path()), "a copy without git");

    let nested = repository.join("unpacked");
    fs::create_dir_all(nested.join("scripts")).expect("nested scripts");
    fs::copy(repository.join(BUILD_SOURCE), nested.join(BUILD_SOURCE)).expect("nested script");
    assert!(
        !script_sees_git(&nested),
        "a copy inside another repository"
    );
}

/// The tree stamp printed by the script with extra environment variables.
fn stamp_with(workspace: &super::support::TestDir, variables: &[(&str, &str)]) -> String {
    let output = std::process::Command::new("bash")
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .envs(variables.iter().copied())
        .current_dir(workspace.path())
        .args([BUILD_SOURCE, "--print-stamp"])
        .output()
        .expect("run the build script");
    assert!(output.status.success(), "{output:?}");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

#[test]
fn an_external_diff_tool_does_not_hide_an_edit() {
    let workspace = scratch_workspace("prereqs-external-diff");
    let quiet_diff = [("GIT_EXTERNAL_DIFF", "true")];
    let base = stamp_with(&workspace, &quiet_diff);
    fs::write(workspace.join("source.rs"), "fn main() { () }\n").expect("edit");
    assert_ne!(stamp_with(&workspace, &quiet_diff), base);
}

#[test]
fn an_edit_inside_an_untracked_nested_repository_counts() {
    let workspace = scratch_workspace("prereqs-nested");
    let nested = workspace.join("vendored");
    fs::create_dir_all(&nested).expect("nested repository");
    fs::write(nested.join("lib.rs"), "// one\n").expect("nested source");
    super::support::git(&nested, &["init", "--quiet"]);
    super::support::git(&nested, &["add", "--all"]);
    super::support::git(&nested, &["commit", "--quiet", "--message", "nested"]);
    let before = stamp(&workspace);
    fs::write(nested.join("lib.rs"), "// two\n").expect("nested edit");
    assert_ne!(stamp(&workspace), before);
}

#[test]
fn a_failed_build_forgets_the_previous_stamp() {
    let workspace = scratch_workspace("prereqs-failed-build");
    let stamp_path = workspace
        .join("target/debug")
        .join(super::support::prerequisites::STAMP_FILE);
    fs::create_dir_all(stamp_path.parent().expect("profile directory")).expect("profile");
    fs::write(&stamp_path, format!("{}\n", stamp(&workspace))).expect("old stamp");
    // A cargo that answers `metadata` and fails every build half way.
    let tools = workspace.join("tools");
    fs::create_dir_all(&tools).expect("tools");
    // The metadata as cargo writes it: JSON, so a Windows path is escaped.
    let metadata = serde_json::json!({ "target_directory": workspace.join("target") }).to_string();
    fs::write(tools.join("metadata.json"), &metadata).expect("metadata");
    fs::write(
        tools.join("cargo"),
        format!(
            "#!/usr/bin/env bash\nif [[ $1 == metadata ]]; then\n  cat '{}'\n  exit 0\nfi\nexit 101\n",
            tools.join("metadata.json").display().to_string().replace('\\', "/")
        ),
    )
    .expect("fake cargo");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(tools.join("cargo"), fs::Permissions::from_mode(0o755))
            .expect("executable fake cargo");
    }
    let path = std::env::join_paths(std::iter::once(tools.clone()).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .expect("search path");
    let output = std::process::Command::new("bash")
        .env("PATH", path)
        .current_dir(workspace.path())
        .arg(BUILD_SOURCE)
        .output()
        .expect("run the build script");
    assert!(!output.status.success(), "the build failed: {output:?}");
    assert!(!stamp_path.exists(), "no stamp outlives a failed build");
}
