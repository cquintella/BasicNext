// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Digests that tie a written graph to the sources and the generator, and
//! the `--check` comparison.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use crate::model::{packages, rust_files};

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// One digest over every (path, file digest) pair, in path order.
pub fn sources_digest<'a>(pairs: impl Iterator<Item = (&'a str, &'a str)>) -> String {
    let mut pairs: Vec<_> = pairs.collect();
    pairs.sort_unstable();
    let mut text = String::new();
    for (path, digest) in pairs {
        let _ = writeln!(text, "{path}\0{digest}");
    }
    sha256_hex(text.as_bytes())
}

/// Every source file of this generator, in a fixed order.
const GENERATOR_SOURCES: &[&str] = &[
    include_str!("main.rs"),
    include_str!("model.rs"),
    include_str!("collect.rs"),
    include_str!("resolve.rs"),
    include_str!("output.rs"),
    include_str!("query.rs"),
    include_str!("unused_pub.rs"),
    include_str!("fresh.rs"),
];

/// Digest of this generator's source: a graph from another version of the
/// generator is stale even when no source file changed.
pub fn generator_digest() -> String {
    sha256_hex(GENERATOR_SOURCES.concat().as_bytes())
}

/// Current `src/**/*.rs` files and their digests, keyed by relative path.
fn source_hashes(root: &Path) -> BTreeMap<String, String> {
    let mut hashes = BTreeMap::new();
    for package in packages(root) {
        let mut paths = Vec::new();
        rust_files(&package.src, &mut paths);
        for path in paths {
            let bytes = std::fs::read(&path).expect("read source");
            let rel = path.strip_prefix(root).unwrap().display().to_string();
            hashes.insert(rel, sha256_hex(&bytes));
        }
    }
    hashes
}

/// Compares a written graph with the current sources. Prints what changed
/// and returns whether the graph is up to date.
pub fn check(root: &Path, graph: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(graph) else {
        println!("missing: {}", graph.display());
        return false;
    };
    let quoted = |line: &str, key: &str| -> Option<String> {
        let value = line.trim_start().strip_prefix(key)?.strip_prefix(" \"")?;
        Some(
            value
                .strip_suffix('"')?
                .replace("&quot;", "\"")
                .replace("&amp;", "&"),
        )
    };
    let mut recorded = BTreeMap::new();
    let mut generator = None;
    let mut label = None;
    for line in text.lines() {
        if let Some(value) = quoted(line, "generatorSha256") {
            generator = Some(value);
        } else if let Some(value) = quoted(line, "label") {
            label = Some(value);
        } else if let Some(value) = quoted(line, "sha256")
            && let Some(path) = label.take()
        {
            recorded.insert(path, value);
        }
    }
    let current = source_hashes(root);
    let mut fresh = true;
    if generator.as_deref() != Some(generator_digest().as_str()) {
        println!("generator: changed");
        fresh = false;
    }
    for (path, digest) in &current {
        match recorded.get(path) {
            None => println!("added: {path}"),
            Some(old) if old != digest => println!("changed: {path}"),
            Some(_) => continue,
        }
        fresh = false;
    }
    for path in recorded.keys().filter(|path| !current.contains_key(*path)) {
        println!("removed: {path}");
        fresh = false;
    }
    if fresh {
        println!("up to date: {}", graph.display());
    }
    fresh
}
