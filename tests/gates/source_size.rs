// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Source-size ratchet. A source file stays at or below 1000 lines (skill
//! `rust-low-level-development`); the files already above it are capped at
//! their size when the gate was added and may only shrink.

use std::{fs, path::Path};

const MAX_LINES: usize = 1000;

/// The folders whose crates are measured (`<group>/<crate>/src`).
const SCAN_GROUPS: [&str; 2] = ["crates", "libraries"];

/// Files over `MAX_LINES` when the gate was added, with their size then.
/// Lower a ceiling when its file shrinks; drop the entry once it fits.
const CEILINGS: &[(&str, usize)] = &[
    ("crates/bn_diag/src/lib.rs", 2200),
    ("crates/bn_ir/src/validate.rs", 1024),
    ("libraries/bn_stdlib/src/dataframe_abi.rs", 1528),
    ("crates/bn_llvm/src/lib.rs", 1065),
    ("crates/bn_host_net/src/lib.rs", 1029),
    // Entered when the gate reached every `libraries/*/src` (0.6.15).
    ("libraries/bn_core_compress/src/lib.rs", 1012),
    ("libraries/bn_lib_crypto/src/lib.rs", 3136),
    ("libraries/bn_lib_web/src/lib.rs", 2717),
    ("libraries/bn_web/src/http/tests.rs", 1351),
    ("libraries/bn_web/src/web/tests.rs", 1025),
    ("libraries/bn_web/src/web_abi.rs", 2175),
];

fn rust_sources(directory: &Path, files: &mut Vec<String>) {
    let mut entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("{}: {error}", directory.display()))
        .map(|entry| entry.expect("directory entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            rust_sources(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path.to_string_lossy().replace('\\', "/"));
        }
    }
}

fn line_count(path: &Path) -> usize {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .count()
}

/// The `src` folder of every crate under `group` (`crates`, `libraries`).
fn group_sources(root: &Path, group: &str, files: &mut Vec<String>) {
    let directory = root.join(group);
    if !directory.is_dir() {
        return;
    }
    let mut crates = fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("{}: {error}", directory.display()))
        .map(|entry| entry.expect("crate entry").path())
        .collect::<Vec<_>>();
    crates.sort();
    for crate_dir in crates {
        let source = crate_dir.join("src");
        if source.is_dir() {
            rust_sources(&source, files);
        }
    }
}

/// Every source file the gate measures: the workspace `src`, and the `src`
/// of each crate of each group.
fn scanned_sources(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    let workspace_src = root.join("src");
    if workspace_src.is_dir() {
        rust_sources(&workspace_src, &mut files);
    }
    for group in SCAN_GROUPS {
        group_sources(root, group, &mut files);
    }
    files
}

/// The files of `root` above their limit, as `path: N lines (limit M)`.
fn violations(root: &Path, ceilings: &[(&str, usize)]) -> Vec<String> {
    let prefix = format!("{}/", root.to_string_lossy().replace('\\', "/"));
    scanned_sources(root)
        .iter()
        .filter_map(|file| {
            let relative = file.strip_prefix(&prefix).unwrap_or(file);
            let lines = line_count(Path::new(file));
            let limit = ceilings
                .iter()
                .find(|(path, _)| *path == relative)
                .map_or(MAX_LINES, |(_, ceiling)| *ceiling);
            (lines > limit).then(|| format!("{relative}: {lines} lines (limit {limit})"))
        })
        .collect()
}

#[test]
fn source_files_stay_within_their_line_budget() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (path, _) in CEILINGS {
        assert!(
            root.join(path).is_file(),
            "{path} is listed in CEILINGS but does not exist; drop the entry"
        );
    }
    let found = violations(root, CEILINGS);
    assert!(
        found.is_empty(),
        "split these files by function and domain:\n{}",
        found.join("\n")
    );
}

/// A file one line over the limit is reported wherever its crate lives: in
/// `libraries/` as in `crates/`.
#[test]
fn a_planted_long_file_fails_in_every_group() {
    let root = std::env::temp_dir().join(format!("bn-source-size-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let long = "// line\n".repeat(MAX_LINES + 1);
    for group in SCAN_GROUPS {
        let source = root.join(group).join("planted").join("src");
        fs::create_dir_all(&source).expect("create planted crate");
        fs::write(source.join("long.rs"), &long).expect("write planted file");
        fs::write(source.join("short.rs"), "// line\n").expect("write short file");
    }
    let found = violations(&root, &[]);
    fs::remove_dir_all(&root).expect("remove planted tree");
    let expected = SCAN_GROUPS
        .iter()
        .map(|group| {
            format!(
                "{group}/planted/src/long.rs: {} lines (limit {MAX_LINES})",
                MAX_LINES + 1
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        found, expected,
        "one violation per group, and only the long file"
    );
}
