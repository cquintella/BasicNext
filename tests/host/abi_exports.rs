// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Dead C ABI gate (bucket 0.6.5 H.S3.a, AC3). Every C export of `bn_rt`
//! (`#[unsafe(no_mangle)] pub extern "C" fn`) is reached by code `bnc`
//! emits (`crates/bn_llvm/src`) or by the Wasm host (`bin/`), or is listed
//! in `tests/abi_exports.list` with a category and a reason:
//!
//! - `debugger`: called by a person from a native debugger, as the book
//!   teaches (chapter 20);
//! - `pending`: the half of a feature still to be wired, naming its bucket
//!   item. The `pending` entries only shrink.
//!
//! An unlisted export nothing reaches fails (remove it, or wire it); a
//! listed export that is now reached, or no longer exists, fails (remove its
//! line).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const LIST: &str = "tests/abi_exports.list";

fn files(directory: &Path, extensions: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.map(|entry| entry.expect("entry").path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            files(&path, extensions, out);
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extensions.contains(&extension))
        {
            out.push(path);
        }
    }
}

fn archive_exports(archive: &Path) -> BTreeSet<String> {
    let nm = ["llvm-nm", "nm"]
        .into_iter()
        .find_map(|name| {
            std::env::var_os("PATH").and_then(|paths| {
                std::env::split_paths(&paths)
                    .map(|directory| {
                        directory.join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
                    })
                    .find(|path| path.is_file())
            })
        })
        .expect("llvm-nm or nm is required for the dead C ABI gate");
    let output = std::process::Command::new(nm)
        .arg("-g")
        .arg(archive)
        .output()
        .expect("run nm");
    assert!(
        output.status.success(),
        "nm {}: {}",
        archive.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            fields.windows(2).find_map(|pair| {
                (pair[0] == "T")
                    .then(|| pair[1].trim_start_matches('_'))
                    .filter(|name| name.starts_with("bn_rt_"))
                    .map(str::to_owned)
            })
        })
        .collect()
}

/// The C exports of `bn_rt` and `bn_stdlib`, read from the built archives
/// (`nm -g`, as the ABI gate of `compiler_capabilities.rs` does), so the
/// exports a macro generates are seen too. Run `cargo build -p bn_rt -p bn_stdlib`
/// first (or set `BN_RT_LIB` / `BN_STDLIB_LIB`).
fn exports() -> BTreeSet<String> {
    let rt_archive = std::env::var_os("BN_RT_LIB").map_or_else(
        || {
            let name = if cfg!(windows) {
                "bn_rt.lib"
            } else {
                "libbn_rt.a"
            };
            PathBuf::from("target/debug").join(name)
        },
        PathBuf::from,
    );
    let stdlib_archive = std::env::var_os("BN_STDLIB_LIB").map_or_else(
        || {
            let name = if cfg!(windows) {
                "bn_stdlib.lib"
            } else {
                "libbn_stdlib.a"
            };
            PathBuf::from("target/debug").join(name)
        },
        PathBuf::from,
    );
    let mut symbols = archive_exports(&rt_archive);
    if stdlib_archive.is_file() {
        symbols.extend(archive_exports(&stdlib_archive));
    }
    symbols
}

/// Every word of the code that can reach an export.
fn reaching_words() -> BTreeSet<String> {
    let mut sources = Vec::new();
    files(Path::new("crates/bn_llvm/src"), &["rs"], &mut sources);
    files(Path::new("bin"), &["rs", "js", "mjs"], &mut sources);
    sources
        .iter()
        .flat_map(|path| {
            fs::read_to_string(path)
                .expect("read reaching source")
                .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
                .filter(|word| word.starts_with("bn_rt_"))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn every_bn_rt_export_is_reached_or_listed() {
    let exports = exports();
    let reached = reaching_words();
    let listed_text = fs::read_to_string(LIST).expect("read the list");
    let mut listed = BTreeMap::new();
    for entry in listed_text
        .lines()
        .filter(|entry| !entry.is_empty() && !entry.starts_with('#'))
    {
        let mut parts = entry.splitn(3, '\t');
        let (Some(symbol), Some(category), Some(reason)) =
            (parts.next(), parts.next(), parts.next())
        else {
            panic!("{LIST}: `{entry}` is not `symbol<TAB>category<TAB>reason`");
        };
        assert!(
            matches!(category, "debugger" | "pending") && !reason.trim().is_empty(),
            "{LIST}: `{entry}` needs a category (debugger, pending) and a reason"
        );
        listed.insert(symbol.to_owned(), category.to_owned());
    }
    let mut report = Vec::new();
    for symbol in &exports {
        if !reached.contains(symbol) && !listed.contains_key(symbol) {
            report.push(format!(
                "{symbol}: no emitted code or Wasm host reaches it; remove it or wire it"
            ));
        }
    }
    for symbol in listed.keys() {
        if !exports.contains(symbol) {
            report.push(format!(
                "{symbol}: listed but no longer exported; remove its line"
            ));
        } else if reached.contains(symbol) {
            report.push(format!(
                "{symbol}: now reached by emitted code; remove its line"
            ));
        }
    }
    assert!(report.is_empty(), "{}", report.join("\n"));
}
