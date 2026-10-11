// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `--unused-pub`: `pub` items of a library crate whose name no other
//! crate writes — candidates for `pub(crate)` (bucket 0.6.5h S3.c, AC5).
//!
//! The scan is by identifier, over the token stream (comments and string
//! literals do not count): a name written anywhere in a dependent crate —
//! `src/`, `tests/`, `benches/`, `examples/`, `build.rs`, `#[cfg(test)]`
//! code, macro bodies — counts as a use. It over-approximates use, so a
//! listed item is a candidate, not a verdict: the compiler decides once the
//! item is `pub(crate)`. C exports (`#[no_mangle]`, `extern "C"`) are not
//! listed; they belong to the ABI gate (S3.a). Consumers outside the
//! workspace other than `fuzz/` (sibling repositories) are not scanned.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::model::{FileInfo, Package, rust_files};

fn idents(tokens: proc_macro2::TokenStream, out: &mut BTreeSet<String>) {
    for token in tokens {
        match token {
            proc_macro2::TokenTree::Ident(ident) => {
                let text = ident.to_string();
                out.insert(text.strip_prefix("r#").map(str::to_string).unwrap_or(text));
            }
            proc_macro2::TokenTree::Group(group) => idents(group.stream(), out),
            _ => {}
        }
    }
}

/// Identifiers written in the `.rs` files under `dirs`.
fn identifiers_in(dirs: &[PathBuf]) -> BTreeSet<String> {
    let mut paths = Vec::new();
    for dir in dirs {
        if dir.is_file() {
            paths.push(dir.clone());
        } else {
            rust_files(dir, &mut paths);
        }
    }
    let mut out = BTreeSet::new();
    for path in paths {
        let source = std::fs::read_to_string(&path).expect("read source");
        match source.parse::<proc_macro2::TokenStream>() {
            Ok(tokens) => idents(tokens, &mut out),
            Err(error) => panic!("{}: cannot tokenize: {error}", path.display()),
        }
    }
    out
}

/// Separate crates a package builds besides its library.
fn extra_targets(package: &Package) -> Vec<PathBuf> {
    ["tests", "benches", "examples", "build.rs"]
        .iter()
        .map(|name| package.dir.join(name))
        .filter(|path| path.exists())
        .collect()
}

/// Prints the candidates (only for crate `only`, when given) and returns
/// how many there are.
pub fn report(root: &Path, packages: &[Package], files: &[FileInfo], only: Option<&str>) -> usize {
    let own: BTreeMap<&str, BTreeSet<String>> = packages
        .iter()
        .map(|package| {
            (
                package.name.as_str(),
                identifiers_in(std::slice::from_ref(&package.src)),
            )
        })
        .collect();
    let extra: BTreeMap<&str, BTreeSet<String>> = packages
        .iter()
        .map(|package| {
            (
                package.name.as_str(),
                identifiers_in(&extra_targets(package)),
            )
        })
        .collect();
    // fuzz/ is its own Cargo project outside the workspace; it depends on
    // frontend crates, and is counted as a consumer of every crate.
    let fuzz = identifiers_in(&[root.join("fuzz/fuzz_targets")]);

    let mut total = 0;
    for package in packages.iter().filter(|package| package.is_lib()) {
        if only.is_some_and(|name| name != package.name) {
            continue;
        }
        let mut used = fuzz.clone();
        used.extend(extra[package.name.as_str()].iter().cloned());
        for consumer in packages {
            if consumer.name != package.name && consumer.deps.contains(&package.name) {
                used.extend(own[consumer.name.as_str()].iter().cloned());
                used.extend(extra[consumer.name.as_str()].iter().cloned());
            }
        }
        let mut found = Vec::new();
        for file in files.iter().filter(|file| file.krate == package.name) {
            for item in file
                .pub_items
                .iter()
                .filter(|item| !used.contains(&item.name))
            {
                found.push(format!(
                    "{}:{}: {} {}",
                    file.rel, item.line, item.kind, item.label
                ));
            }
        }
        if found.is_empty() {
            continue;
        }
        println!("{} ({} candidates)", package.name, found.len());
        for line in &found {
            println!("  {line}");
        }
        println!();
        total += found.len();
    }
    println!("Total: {total} pub items not named outside their crate (candidates for pub(crate)).");
    total
}
