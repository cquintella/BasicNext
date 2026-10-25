// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Suppression gate (bucket 0.6.5 H.S3.b, AC4). `#[allow(dead_code)]` and
//! `#[allow(unused…)]` hide code from the compiler, so each one in
//! `crates/*/src` is listed in `tests/suppressions.list` as
//! `path<TAB>lint<TAB>reason`. The list only shrinks: an unlisted
//! suppression fails (delete or use the code instead), and a listed one that
//! is gone fails (remove its line).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const LIST: &str = "tests/suppressions.list";
const LINTS: &[&str] = &[
    "dead_code",
    "unused_imports",
    "unused_mut",
    "unused_variables",
    "unused",
];

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

/// `(path, lint)` → number of suppressions of that lint in that file.
fn suppressions() -> BTreeMap<(String, String), usize> {
    let mut files = Vec::new();
    for crate_dir in fs::read_dir("crates").expect("crates directory") {
        rust_sources(&crate_dir.expect("crate").path().join("src"), &mut files);
    }
    let mut found = BTreeMap::new();
    for path in files {
        let text = fs::read_to_string(&path).expect("read source");
        let path = path.to_string_lossy().replace('\\', "/");
        for line in text.lines().map(str::trim) {
            if line.starts_with("//") || !line.contains("allow(") {
                continue;
            }
            for lint in LINTS {
                let written = format!("allow({lint})");
                let in_list = format!("allow({lint},");
                let cfg_attr = format!(", allow({lint}))");
                if line.contains(&written) || line.contains(&in_list) || line.contains(&cfg_attr) {
                    *found.entry((path.clone(), (*lint).to_owned())).or_insert(0) += 1;
                }
            }
        }
    }
    found
}

#[test]
fn every_suppression_is_listed_with_its_reason() {
    let found = suppressions();
    let text = fs::read_to_string(LIST).expect("read the list");
    let mut listed: BTreeMap<(String, String), usize> = BTreeMap::new();
    for entry in text
        .lines()
        .filter(|entry| !entry.is_empty() && !entry.starts_with('#'))
    {
        let mut parts = entry.splitn(3, '\t');
        let (Some(path), Some(lint), Some(reason)) = (parts.next(), parts.next(), parts.next())
        else {
            panic!("{LIST}: `{entry}` is not `path<TAB>lint<TAB>reason`");
        };
        assert!(
            !reason.trim().is_empty(),
            "{LIST}: `{entry}` needs a reason"
        );
        *listed
            .entry((path.to_owned(), lint.to_owned()))
            .or_insert(0) += 1;
    }
    let mut report = Vec::new();
    for (key, count) in &found {
        let allowed = listed.get(key).copied().unwrap_or(0);
        if *count > allowed {
            report.push(format!(
                "{} allow({}) x{}: not listed; delete or use the hidden code",
                key.0,
                key.1,
                count - allowed
            ));
        }
    }
    for (key, count) in &listed {
        let present = found.get(key).copied().unwrap_or(0);
        if *count > present {
            report.push(format!(
                "{} allow({}): listed but gone; remove its line",
                key.0, key.1
            ));
        }
    }
    assert!(report.is_empty(), "{}", report.join("\n"));

    let mut prod_files = Vec::new();
    for root_dir in ["crates", "libraries"] {
        if let Ok(entries) = fs::read_dir(root_dir) {
            for crate_dir in entries.flatten() {
                rust_sources(&crate_dir.path().join("src"), &mut prod_files);
            }
        }
    }
    let mut poison_panics = Vec::new();
    for path in prod_files {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if name == "tests.rs" || name.ends_with("_tests.rs") || name == "test_support.rs" {
            continue;
        }
        let text = fs::read_to_string(&path).expect("read source");
        let production = text
            .find("\n#[cfg(test)]")
            .map_or(text.as_str(), |end| &text[..end]);
        for (line_no, line) in production.lines().enumerate() {
            let trimmed = line.trim();
            if !trimmed.starts_with("//")
                && (trimmed.contains(".lock().unwrap(") || trimmed.contains(".lock().expect("))
            {
                poison_panics.push(format!(
                    "{}:{}: {}",
                    path.to_string_lossy().replace('\\', "/"),
                    line_no + 1,
                    trimmed
                ));
            }
        }
    }
    assert!(
        poison_panics.is_empty(),
        "production mutex lock must recover via PoisonError::into_inner instead of .lock().unwrap/expect:\n{}",
        poison_panics.join("\n")
    );

    // Cross-platform test determinism gate (bucket 0.6.7c): forbid inline `cfg!(target_os`
    // or `if std::mem::size_of::<` test branch bifurcations in FFI and compile-driver tests.
    for test_file in [
        "libraries/bn_stdlib/src/ffi_tests.rs",
        "crates/bn_compile_driver/src/tests.rs",
        "tests/ffi.rs",
    ] {
        let text = fs::read_to_string(test_file).expect("read test file");
        for (line_no, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            assert!(
                !trimmed.contains("cfg!(target_os") && !trimmed.contains("if std::mem::size_of::<"),
                "{test_file}:{} contains a host-dependent test branch (`{trimmed}`); parameterize by OS/width so all branches execute on every host",
                line_no + 1
            );
        }
    }
}
