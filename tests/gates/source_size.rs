// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Source-size ratchet. A source file stays at or below 1000 lines (skill
//! `rust-low-level-development`); the files already above it are capped at
//! their size when the gate was added and may only shrink.

use std::{fs, path::Path};

const MAX_LINES: usize = 1000;

/// Files over `MAX_LINES` when the gate was added, with their size then.
/// Lower a ceiling when its file shrinks; drop the entry once it fits.
const CEILINGS: &[(&str, usize)] = &[
    ("crates/bn_diag/src/lib.rs", 2200),
    ("crates/bn_ir/src/validate.rs", 1024),
    ("crates/bn_rt/src/dataframe_abi.rs", 1528),
    ("crates/bn_llvm/src/lib.rs", 1065),
    ("crates/bn_host_net/src/lib.rs", 1029),
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

#[test]
fn source_files_stay_within_their_line_budget() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_sources(&root.join("src"), &mut files);
    for crate_dir in fs::read_dir(root.join("crates")).expect("crates directory") {
        let source = crate_dir.expect("crate entry").path().join("src");
        if source.is_dir() {
            rust_sources(&source, &mut files);
        }
    }
    let prefix = format!("{}/", root.to_string_lossy().replace('\\', "/"));
    let mut violations = Vec::new();
    for file in &files {
        let relative = file.strip_prefix(&prefix).unwrap_or(file);
        let lines = line_count(Path::new(file));
        let limit = CEILINGS
            .iter()
            .find(|(path, _)| *path == relative)
            .map_or(MAX_LINES, |(_, ceiling)| *ceiling);
        if lines > limit {
            violations.push(format!("{relative}: {lines} lines (limit {limit})"));
        }
    }
    for (path, _) in CEILINGS {
        assert!(
            root.join(path).is_file(),
            "{path} is listed in CEILINGS but does not exist; drop the entry"
        );
    }
    assert!(
        violations.is_empty(),
        "split these files by function and domain:\n{}",
        violations.join("\n")
    );
}
