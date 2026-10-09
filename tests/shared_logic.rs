// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Gate against logic duplicated between the interpreter side and the native
//! side (bucket 0.6.5c S5.a, AC7). A rule lives once, in a shared core
//! (`bn_core_*`, `bn_host_*` cores, `bn_types`, `bn_ir`, `bn_diag`); the two
//! sides only adapt it. Two signals, both from the §1.2 scan of the bucket:
//! an identical window of six normalized lines, and an identical string
//! literal of 24 characters or more, found on both sides. Test code is not
//! scanned. `tests/shared_logic.allow` lists the accepted findings, each with
//! its reason; it only shrinks: a finding not listed fails, and so does a
//! listed one that no longer occurs.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

/// Interpreter side: the interpreter, its library adapters, its driver and
/// the HOST adapters that are not cores yet (bucket 0.6.5c D4, §5).
const INTERPRETER_SIDE: &[&str] = &[
    "crates/bn_interp/src",
    "crates/bn_interpret_driver/src",
    "crates/bn_host_fs/src",
    "crates/bn_host_net/src",
    "crates/bn_lib_crypto/src",
    "crates/bn_lib_data/src",
    "crates/bn_lib_dispatch/src",
    "crates/bn_lib_json/src",
    "crates/bn_lib_log/src",
    "crates/bn_lib_math/src",
    "crates/bn_lib_sqlite/src",
    "crates/bn_lib_web/src",
];

/// Native side: the runtime ABI, the LLVM backend and the compile driver.
const NATIVE_SIDE: &[&str] = &[
    "crates/bn_rt/src",
    "crates/bn_llvm/src",
    "crates/bn_compile_driver/src",
];

const WINDOW: usize = 6;
const MIN_LITERAL: usize = 24;
/// A window must carry this much code (non-space characters) to count:
/// six closing braces are not a duplicated rule.
const MIN_WINDOW_CODE: usize = 120;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The production Rust sources under `directories`: test files and the
/// `#[cfg(test)]` tail of a file are skipped.
fn sources(directories: &[&str]) -> Vec<(String, String)> {
    let mut files = Vec::new();
    for directory in directories {
        collect(&root().join(directory), &mut files);
    }
    files.sort();
    files
        .into_iter()
        .filter_map(|path| {
            let name = path.file_name()?.to_string_lossy().into_owned();
            if name == "tests.rs" || name.ends_with("_tests.rs") {
                return None;
            }
            let text = fs::read_to_string(&path).ok()?;
            let production = text
                .find("\n#[cfg(test)]")
                .map_or(text.as_str(), |end| &text[..end]);
            let relative = path
                .strip_prefix(root())
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            Some((relative, production.to_string()))
        })
        .collect()
}

fn collect(directory: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "tests") {
                continue;
            }
            collect(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

/// Code lines without indentation, blank lines and line comments.
fn normalized_lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//"))
        .collect()
}

/// FNV-1a, 64 bits: a stable key for a window in the allow list.
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Every qualifying six-line window of one side, keyed by its hash, with
/// the first file it was seen in.
fn windows(files: &[(String, String)]) -> BTreeMap<u64, (String, String)> {
    let mut found = BTreeMap::new();
    for (path, text) in files {
        let lines = normalized_lines(text);
        for window in lines.windows(WINDOW) {
            let joined = window.join("\n");
            let code = joined.chars().filter(|c| !c.is_whitespace()).count();
            if code >= MIN_WINDOW_CODE {
                found
                    .entry(fnv1a(&joined))
                    .or_insert_with(|| (path.clone(), joined));
            }
        }
    }
    found
}

/// The string literals of `text` (escapes kept as written), raw strings and
/// byte strings included only in their plain `"…"` part.
fn literals(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut characters = text.chars().peekable();
    let mut previous = ' ';
    while let Some(character) = characters.next() {
        // A line comment (doc comments included) holds prose, not literals.
        if character == '/' && characters.peek() == Some(&'/') {
            for skipped in characters.by_ref() {
                if skipped == '\n' {
                    break;
                }
            }
            previous = '\n';
            continue;
        }
        // A `'"'` character literal is not the start of a string.
        if character == '"' && previous != '\'' {
            let mut literal = String::new();
            let mut escaped = false;
            for inner in characters.by_ref() {
                if escaped {
                    literal.push(inner);
                    escaped = false;
                } else if inner == '\\' {
                    literal.push(inner);
                    escaped = true;
                } else if inner == '"' {
                    break;
                } else {
                    literal.push(inner);
                }
            }
            found.push(literal);
            previous = '"';
        } else {
            previous = character;
        }
    }
    found
}

fn long_literals(files: &[(String, String)]) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    for (path, text) in files {
        for literal in literals(text) {
            if literal.chars().count() >= MIN_LITERAL {
                found.entry(literal).or_insert_with(|| path.clone());
            }
        }
    }
    found
}

/// `kind<TAB>key<TAB>reason` lines; `#` starts a comment line.
fn allow_list() -> BTreeSet<(String, String)> {
    let text = fs::read_to_string(root().join("tests/shared_logic.allow")).unwrap_or_default();
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            let mut fields = line.splitn(3, '\t');
            let kind = fields.next().unwrap_or_default().to_string();
            let key = fields.next().unwrap_or_default().to_string();
            let reason = fields.next().unwrap_or_default().trim();
            assert!(!reason.is_empty(), "allow entry without a reason: {line}");
            (kind, key)
        })
        .collect()
}

#[test]
fn no_rule_is_written_twice_across_the_backends() {
    let interpreter = sources(INTERPRETER_SIDE);
    let native = sources(NATIVE_SIDE);
    assert!(
        interpreter.len() > 20 && native.len() > 20,
        "scan found too few files"
    );

    let mut findings = BTreeMap::new();
    let native_windows = windows(&native);
    for (hash, (path, text)) in windows(&interpreter) {
        if let Some((native_path, _)) = native_windows.get(&hash) {
            findings.insert(
                ("window".to_string(), format!("{hash:016x}")),
                format!("{path} and {native_path}:\n{text}"),
            );
        }
    }
    let native_literals = long_literals(&native);
    for (literal, path) in long_literals(&interpreter) {
        if let Some(native_path) = native_literals.get(&literal) {
            findings.insert(
                ("literal".to_string(), literal.clone()),
                format!("{path} and {native_path}"),
            );
        }
    }

    let allowed = allow_list();
    let new = findings
        .iter()
        .filter(|(key, _)| !allowed.contains(*key))
        .map(|((kind, key), place)| format!("{kind}\t{key}\t<reason>   # {place}"))
        .collect::<Vec<_>>();
    let stale = allowed
        .iter()
        .filter(|key| !findings.contains_key(*key))
        .map(|(kind, key)| format!("{kind}\t{key}"))
        .collect::<Vec<_>>();
    assert!(
        new.is_empty(),
        "logic duplicated across the backends: move it to a shared core, or \
         list it in tests/shared_logic.allow with its reason:\n{}",
        new.join("\n")
    );
    assert!(
        stale.is_empty(),
        "tests/shared_logic.allow lists findings that no longer occur; remove \
         them (the list only shrinks):\n{}",
        stale.join("\n")
    );
}
