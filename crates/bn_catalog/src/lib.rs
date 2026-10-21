// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The text catalog (`AGENTS.md`, "No hard-coded messages"; bucket 0.6.5
//! H). Every text a user can read and every contract name lives in
//! `share/bn/catalog/<domain>.txt`, one `key = value` per line. The build
//! script turns every entry into a constant, `bn_catalog::<domain>::<KEY>`
//! (AC2: a reference to a missing entry is a compile error); the files are
//! embedded, so `bni`, `bnc` and compiled programs need no file at run time.
//!
//! `tests/catalog_keys.rs` checks that every entry of a file is used.

mod entry_format;

use std::fmt::Display;

pub use entry_format::{const_name, escape};

include!(concat!(env!("OUT_DIR"), "/catalog.rs"));

/// The `key = value` entries of one catalog file, in file order, with each
/// value unescaped.
///
/// # Panics
///
/// When the file is malformed; the build script has already parsed every
/// embedded file, so this only fails on text that did not come from it.
#[must_use]
pub fn parse(contents: &str) -> Vec<(&str, String)> {
    entry_format::parse(contents).unwrap_or_else(|error| panic!("{error}"))
}

/// The text of `key` in `domain`.
///
/// # Panics
///
/// When the catalog has no such entry: a defect of the caller, which
/// `tests/catalog_keys.rs` catches before a release.
#[must_use]
pub fn text(domain: &str, key: &str) -> &'static str {
    let index = ENTRIES
        .binary_search_by(|(entry_domain, entry_key, _)| {
            (*entry_domain, *entry_key).cmp(&(domain, key))
        })
        .unwrap_or_else(|_| panic!("{domain}:{key}"));
    ENTRIES[index].2
}

/// The whitespace-separated words of `key` in `domain`.
pub fn words(domain: &str, key: &str) -> impl Iterator<Item = &'static str> {
    text(domain, key).split_whitespace()
}

/// The text of `key` with each `{name}` replaced by its value; a `{…}` with
/// no value stays as written.
#[must_use]
pub fn fill(domain: &str, key: &str, values: &[(&str, &dyn Display)]) -> String {
    format(text(domain, key), values)
}

/// `template` with each `{name}` replaced by its value; a `{…}` with no
/// value stays as written. `template` is a catalog constant.
#[must_use]
pub fn format(template: &str, values: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        if let Some(close) = after.find('}') {
            let name = &after[..close];
            match values.iter().find(|(wanted, _)| *wanted == name) {
                Some((_, value)) => out.push_str(&value.to_string()),
                None => out.push_str(&rest[open..=open + close + 1]),
            }
            rest = &after[close + 1..];
        } else {
            out.push_str(&rest[open..]);
            rest = "";
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn fill_replaces_named_values_and_keeps_unknown_braces() {
        assert_eq!(
            super::format("x {name} y {other} {name}", &[("name", &7)]),
            "x 7 y {other} 7"
        );
        let value = 7;
        let filled = super::fill("net", "cidr.host_bits", &[("name", &value)]);
        assert_eq!(filled, super::text("net", "cidr.host_bits"));
        assert_eq!(
            super::net::CIDR_HOST_BITS,
            super::text("net", "cidr.host_bits")
        );
    }

    #[test]
    fn the_entry_table_is_sorted_and_unique() {
        assert!(
            super::ENTRIES
                .windows(2)
                .all(|pair| (pair[0].0, pair[0].1) < (pair[1].0, pair[1].1))
        );
    }

    #[test]
    fn every_catalog_file_is_embedded() {
        let directory = concat!(env!("CARGO_MANIFEST_DIR"), "/../../share/bn/catalog");
        let mut on_disk: Vec<String> = std::fs::read_dir(directory)
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
        let embedded: Vec<&str> = super::FILES.iter().map(|(domain, _)| *domain).collect();
        assert_eq!(embedded, on_disk);
    }
}
