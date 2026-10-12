// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The text catalog (`AGENTS.md`, "No hard-coded messages"; bucket 0.6.5
//! H). Every text a user can read and every contract name lives in
//! `share/bn/catalog/<domain>.txt`, one `key = value` per line; code names
//! a text by its domain and key. The files are embedded at build time, so
//! `bni`, `bnc` and compiled programs need no file at run time.
//!
//! `tests/catalog_keys.rs` checks that every key the code names exists and
//! that every key of a file is used.

use std::collections::BTreeMap;
use std::fmt::Display;
use std::sync::OnceLock;

/// `(domain, contents)` of every catalog file.
pub const FILES: &[(&str, &str)] = &[
    ("bnlog", include_str!("../../../share/bn/catalog/bnlog.txt")),
    ("cli", include_str!("../../../share/bn/catalog/cli.txt")),
    (
        "config",
        include_str!("../../../share/bn/catalog/config.txt"),
    ),
    (
        "crypto",
        include_str!("../../../share/bn/catalog/crypto.txt"),
    ),
    ("data", include_str!("../../../share/bn/catalog/data.txt")),
    ("env", include_str!("../../../share/bn/catalog/env.txt")),
    ("exec", include_str!("../../../share/bn/catalog/exec.txt")),
    ("file", include_str!("../../../share/bn/catalog/file.txt")),
    (
        "frontend",
        include_str!("../../../share/bn/catalog/frontend.txt"),
    ),
    ("json", include_str!("../../../share/bn/catalog/json.txt")),
    ("llvm", include_str!("../../../share/bn/catalog/llvm.txt")),
    ("net", include_str!("../../../share/bn/catalog/net.txt")),
    ("web", include_str!("../../../share/bn/catalog/web.txt")),
];

/// The `key = value` entries of one catalog file, in file order.
#[must_use]
pub fn parse(contents: &str) -> Vec<(&str, &str)> {
    contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_once(" = "))
        .map(|(key, value)| (key.trim(), value.trim()))
        .collect()
}

fn entries() -> &'static BTreeMap<(&'static str, &'static str), &'static str> {
    static ENTRIES: OnceLock<BTreeMap<(&'static str, &'static str), &'static str>> =
        OnceLock::new();
    ENTRIES.get_or_init(|| {
        FILES
            .iter()
            .flat_map(|(domain, contents)| {
                parse(contents)
                    .into_iter()
                    .map(move |(key, value)| ((*domain, key), value))
            })
            .collect()
    })
}

/// The text of `key` in `domain`.
///
/// # Panics
///
/// When the catalog has no such entry: a defect of the caller, which
/// `tests/catalog_keys.rs` catches before a release.
#[must_use]
pub fn text(domain: &str, key: &str) -> &'static str {
    entries()
        .get(&(domain, key))
        .copied()
        .unwrap_or_else(|| panic!("{domain}:{key}"))
}

/// The whitespace-separated words of `key` in `domain`.
pub fn words(domain: &str, key: &str) -> impl Iterator<Item = &'static str> {
    text(domain, key).split_whitespace()
}

/// The text of `key` with each `{name}` replaced by its value; a `{…}` with
/// no value stays as written.
#[must_use]
pub fn fill(domain: &str, key: &str, values: &[(&str, &dyn Display)]) -> String {
    let template = text(domain, key);
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
        let entries = super::parse("a = x {name} y {other} {name}\n# c\nb=no space\n");
        assert_eq!(entries, [("a", "x {name} y {other} {name}")]);
        let value = 7;
        let filled = super::fill("net", "cidr.host_bits", &[("name", &value)]);
        assert_eq!(filled, super::text("net", "cidr.host_bits"));
    }

    #[test]
    fn every_file_parses_without_a_duplicate_key() {
        for (domain, contents) in super::FILES {
            let entries = super::parse(contents);
            let mut keys: Vec<&str> = entries.iter().map(|(key, _)| *key).collect();
            keys.sort_unstable();
            let count = keys.len();
            keys.dedup();
            assert_eq!(keys.len(), count, "{domain} has a duplicate key");
        }
    }
}
