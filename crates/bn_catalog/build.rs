// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Generates the catalog constants (bucket 0.6.5h AC2): one module per file
//! of `share/bn/catalog/`, one `&str` constant per entry, so a reference to
//! a missing entry is a compile error; plus the embedded files and the
//! sorted entry table that `text` searches.

#[path = "src/entry_format.rs"]
#[allow(dead_code)]
mod entry_format;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let catalog = manifest.join("../../share/bn/catalog");
    let catalog = catalog.canonicalize().unwrap_or(catalog);
    println!("cargo:rerun-if-changed={}", catalog.display());
    println!("cargo:rerun-if-changed=src/entry_format.rs");
    let mut files: Vec<PathBuf> = fs::read_dir(&catalog)
        .unwrap_or_else(|error| panic!("{}: {error}", catalog.display()))
        .map(|entry| entry.expect("catalog entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "txt"))
        .collect();
    files.sort();
    let mut out = String::new();
    let mut table: Vec<(String, String, String)> = Vec::new();
    out.push_str(
        "/// `(domain, contents)` of every catalog file.\npub const FILES: &[(&str, &str)] = &[\n",
    );
    for path in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        let _ = writeln!(
            out,
            "    ({:?}, include_str!({:?})),",
            domain(path),
            path.display().to_string()
        );
    }
    out.push_str("];\n\n");
    for path in &files {
        let domain = domain(path);
        let contents =
            fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let entries = entry_format::parse(&contents)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let mut names: BTreeMap<String, &str> = BTreeMap::new();
        let _ = writeln!(
            out,
            "/// The entries of `share/bn/catalog/{domain}.txt`.\npub mod {domain} {{"
        );
        for (key, value) in &entries {
            let name = entry_format::const_name(key);
            if let Some(previous) = names.insert(name.clone(), key) {
                panic!(
                    "{}: keys `{previous}` and `{key}` both name the constant {name}",
                    path.display()
                );
            }
            let _ = writeln!(
                out,
                "    /// `{key}`\n    pub const {name}: &str = {value:?};"
            );
            table.push((domain.clone(), (*key).to_owned(), value.clone()));
        }
        out.push_str("}\n\n");
    }
    table.sort();
    out.push_str("/// Every `(domain, key, value)`, sorted by domain and key.\npub(crate) const ENTRIES: &[(&str, &str, &str)] = &[\n");
    for (domain, key, value) in &table {
        let _ = writeln!(out, "    ({domain:?}, {key:?}, {value:?}),");
    }
    out.push_str("];\n");
    let target = PathBuf::from(std::env::var_os("OUT_DIR").expect("out dir")).join("catalog.rs");
    fs::write(&target, out).unwrap_or_else(|error| panic!("{}: {error}", target.display()));
}

/// The domain of a catalog file: its stem, which must be a Rust identifier.
fn domain(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    assert!(
        !stem.is_empty()
            && stem.starts_with(|character: char| character.is_ascii_lowercase())
            && stem.chars().all(|character| character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '_'),
        "{}: a catalog file name is a lower-case identifier",
        path.display()
    );
    stem.to_owned()
}
