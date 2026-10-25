// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Catalog keys (bucket 0.6.5 H.S4.0; AC2's unused-entry half). A text the
//! code names by `bn_catalog::text`, `fill` or `words` (or, inside
//! `bn_core_log`, by its `catalog::` wrappers) exists in its
//! `share/bn/catalog/<domain>.txt`; and every entry of a catalog file is
//! named somewhere in production code (by such a call, or as a key literal
//! in a table the code walks).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn rust_sources(directory: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.map(|entry| entry.expect("entry").path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            rust_sources(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

/// The string literals that follow `start` in `source`, up to `count`.
fn literals_after(source: &str, start: usize, count: usize) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = &source[start..];
    while found.len() < count {
        let Some(open) = rest.find(['"', ')']) else {
            break;
        };
        if rest.as_bytes()[open] == b')' {
            break;
        }
        let body = &rest[open + 1..];
        let Some(close) = body.find('"') else {
            break;
        };
        found.push(body[..close].to_owned());
        rest = &body[close + 1..];
    }
    found
}

/// `(domain, key)` of every catalog call with literal arguments, and every
/// string literal of production code.
fn references() -> (BTreeSet<(String, String)>, BTreeSet<String>) {
    let mut files = Vec::new();
    for crate_dir in fs::read_dir("crates").expect("crates directory") {
        rust_sources(&crate_dir.expect("crate").path().join("src"), &mut files);
    }
    if let Ok(lib_dirs) = fs::read_dir("libraries") {
        for entry in lib_dirs.flatten() {
            rust_sources(&entry.path().join("src"), &mut files);
        }
    }
    let mut calls = BTreeSet::new();
    let mut literals = BTreeSet::new();
    for path in files {
        let source = fs::read_to_string(&path).expect("read source");
        for call in [
            "bn_catalog::text(",
            "bn_catalog::fill(",
            "bn_catalog::words(",
        ] {
            for (index, _) in source.match_indices(call) {
                if let [domain, key] = literals_after(&source, index + call.len(), 2).as_slice() {
                    calls.insert((domain.clone(), key.clone()));
                }
            }
        }
        if path.starts_with("crates/bn_core_log") || path.starts_with("libraries/bn_core_log") {
            for call in ["catalog::text(", "catalog::fill(", "catalog::words("] {
                for (index, _) in source.match_indices(call) {
                    if source[..index].ends_with("bn_") {
                        continue;
                    }
                    if let [key] = literals_after(&source, index + call.len(), 1).as_slice() {
                        calls.insert(("bnlog".to_owned(), key.clone()));
                    }
                }
            }
        }
        let mut rest = source.as_str();
        while let Some(open) = rest.find('"') {
            let body = &rest[open + 1..];
            let Some(close) = body.find('"') else {
                break;
            };
            literals.insert(body[..close].to_owned());
            rest = &body[close + 1..];
        }
    }
    (calls, literals)
}

fn catalog() -> BTreeSet<(String, String)> {
    let mut entries = BTreeSet::new();
    for entry in fs::read_dir("share/bn/catalog").expect("catalog directory") {
        let path = entry.expect("entry").path();
        let domain = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_owned();
        let text = fs::read_to_string(&path).expect("read catalog");
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, _) = line
                .split_once(" = ")
                .unwrap_or_else(|| panic!("{}: `{line}` is not `key = value`", path.display()));
            entries.insert((domain.clone(), key.trim().to_owned()));
        }
    }
    entries
}

#[test]
fn every_named_text_exists_and_every_entry_is_used() {
    let (calls, literals) = references();
    let entries = catalog();
    let mut report = Vec::new();
    for (domain, key) in &calls {
        if !entries.contains(&(domain.clone(), key.clone())) {
            report.push(format!(
                "{domain}:{key} is named by the code but missing from share/bn/catalog/{domain}.txt"
            ));
        }
    }
    for (domain, key) in &entries {
        if !calls.contains(&(domain.clone(), key.clone())) && !literals.contains(key) {
            report.push(format!(
                "{domain}:{key} is in share/bn/catalog/{domain}.txt but no code names it; remove it"
            ));
        }
    }
    assert!(report.is_empty(), "{}", report.join("\n"));
}

/// Every catalog file is embedded in `bn_catalog` (a file the build script
/// misses is absent from `FILES`, and every key of it fails at run time).
#[test]
fn every_catalog_file_is_embedded() {
    let mut on_disk: Vec<String> = fs::read_dir("share/bn/catalog")
        .expect("catalog directory")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "txt"))
        .filter_map(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_owned)
        })
        .collect();
    on_disk.sort();
    let embedded: Vec<&str> = bn_catalog::FILES
        .iter()
        .map(|(domain, _)| *domain)
        .collect();
    assert_eq!(embedded, on_disk);
}
