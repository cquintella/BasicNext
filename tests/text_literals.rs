// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Text-literal ratchet (bucket 0.6.5 H.S1; `AGENTS.md`, "No hard-coded
//! messages"). Every text a user can read and every contract identifier
//! lives in a data file under `share/bn/`, not in a string literal of
//! production code. The literals that existed when the gate was added are
//! listed in `tests/text_literals.list`; the list only shrinks:
//!
//! - a literal in production code that is not listed fails (new work goes
//!   to a data file);
//! - a listed literal that no longer exists fails (remove its line).
//!
//! What is scanned: `crates/*/src/**/*.rs`, minus test code (`#[cfg(test)]`
//! items and the files they name, `tests.rs`, `*_tests.rs`,
//! `test_support.rs`) and `build.rs`. What counts:
//!
//! - **identifier**: a contract name — `HOST` or `BN<Module>` with dotted
//!   members (`HOST.Net.TCPStream.Read`, `BNWeb`), or an upper-case code
//!   with an underscore (`MODULE_NOT_FOUND`);
//! - **prose**: two or more words of letters, once `{…}` placeholders are
//!   removed.
//!
//! Declared exclusion: LLVM IR templates in `crates/bn_llvm` (a literal with
//! an LLVM sigil `%`, `@` or `!`, or starting with an LLVM keyword) are
//! machine text, not prose.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const LIST: &str = "tests/text_literals.list";

/// Words that start an LLVM IR line or instruction.
const LLVM_KEYWORDS: &[&str] = &[
    "declare",
    "define",
    "target",
    "attributes",
    "source_filename",
    "call",
    "ret",
    "br",
    "switch",
    "store",
    "load",
    "alloca",
    "getelementptr",
    "icmp",
    "fcmp",
    "phi",
    "select",
    "unreachable",
    "zext",
    "sext",
    "trunc",
    "bitcast",
    "ptrtoint",
    "inttoptr",
    "fadd",
    "fsub",
    "fmul",
    "fdiv",
    "frem",
    "fneg",
    "add",
    "sub",
    "mul",
    "sdiv",
    "udiv",
    "srem",
    "urem",
    "and",
    "or",
    "xor",
    "shl",
    "lshr",
    "ashr",
    "extractvalue",
    "insertvalue",
    "fptosi",
    "fptoui",
    "sitofp",
    "uitofp",
    "fpext",
    "fptrunc",
    "private",
    "internal",
    "constant",
    "global",
    "type",
    "i1",
    "i8",
    "i16",
    "i32",
    "i64",
    "i128",
    "ptr",
    "void",
    "double",
    "float",
    "label",
];

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Kind {
    Identifier,
    Prose,
}

impl Kind {
    const fn name(self) -> &'static str {
        match self {
            Self::Identifier => "identifier",
            Self::Prose => "prose",
        }
    }
}

/// A string literal of the source: its text as written between the quotes,
/// and whether it sits in test-only code.
struct Literal {
    text: String,
    in_test: bool,
}

/// What a scan of one file finds: its literals, and the files that
/// `#[cfg(test)] mod name;` declarations name (test files).
struct Scan {
    literals: Vec<Literal>,
    test_modules: Vec<String>,
}

/// A small Rust lexer: enough to find string literals outside comments,
/// character literals and lifetimes, and to know which ones belong to an
/// item under `#[cfg(test)]`.
fn scan(source: &str) -> Scan {
    let bytes = source.as_bytes();
    let mut literals = Vec::new();
    let mut test_modules = Vec::new();
    let mut index = 0;
    let mut depth = 0usize;
    // Depth at which the current `#[cfg(test)]` item started, while inside it.
    let mut test_depth: Option<usize> = None;
    // A `#[cfg(test)]` was seen; the next item is test code.
    let mut pending_test = false;
    let mut item_start = 0;
    while index < bytes.len() {
        let rest = &source[index..];
        let byte = bytes[index];
        if rest.starts_with("//") {
            index += rest.find('\n').unwrap_or(rest.len());
        } else if rest.starts_with("/*") {
            let mut nesting = 0usize;
            while index < bytes.len() {
                if source[index..].starts_with("/*") {
                    nesting += 1;
                    index += 2;
                } else if source[index..].starts_with("*/") {
                    nesting -= 1;
                    index += 2;
                    if nesting == 0 {
                        break;
                    }
                } else {
                    index += 1;
                }
            }
        } else if rest.starts_with("#[cfg(test)]") {
            pending_test = true;
            item_start = index + "#[cfg(test)]".len();
            index = item_start;
        } else if byte == b'\'' {
            index = skip_char_or_lifetime(bytes, index);
        } else if let Some((text, end, is_bytes)) = string_at(source, index) {
            if !is_bytes {
                literals.push(Literal {
                    text,
                    in_test: test_depth.is_some() || pending_test,
                });
            }
            index = end;
        } else if byte == b'{' {
            if pending_test && test_depth.is_none() {
                test_depth = Some(depth);
                pending_test = false;
            }
            depth += 1;
            index += 1;
        } else if byte == b'}' {
            depth = depth.saturating_sub(1);
            if test_depth == Some(depth) {
                test_depth = None;
            }
            index += 1;
        } else if byte == b';' {
            if pending_test && test_depth.is_none() {
                // `#[cfg(test)] mod name;` names a test file; `use …;` does not.
                let item = source[item_start..index].trim();
                if let Some(name) = item.strip_prefix("mod ") {
                    test_modules.push(name.trim().to_owned());
                }
                pending_test = false;
            }
            index += 1;
        } else {
            index += 1;
        }
    }
    Scan {
        literals,
        test_modules,
    }
}

/// Skips `'x'`, `'\n'`, `'\u{..}'`, or the lifetime `'a`.
fn skip_char_or_lifetime(bytes: &[u8], start: usize) -> usize {
    if bytes.get(start + 1) == Some(&b'\\') {
        let mut index = start + 2;
        while index < bytes.len() && bytes[index] != b'\'' {
            index += 1;
        }
        return index + 1;
    }
    // One character (possibly multi-byte UTF-8) followed by a quote.
    let mut index = start + 1;
    if index < bytes.len() {
        index += 1;
        while index < bytes.len() && (bytes[index] & 0xC0) == 0x80 {
            index += 1;
        }
        if bytes.get(index) == Some(&b'\'') {
            return index + 1;
        }
    }
    start + 1
}

/// The string literal starting at `start`, if any: its text, the index after
/// it, and whether it is a byte string (`b"…"`, `br"…"`).
fn string_at(source: &str, start: usize) -> Option<(String, usize, bool)> {
    let bytes = source.as_bytes();
    let previous_is_ident =
        start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_');
    let mut index = start;
    let mut is_bytes = false;
    if !previous_is_ident && (bytes[index] == b'b' || bytes[index] == b'c') {
        is_bytes = bytes[index] == b'b';
        index += 1;
    }
    let raw = !previous_is_ident && bytes.get(index) == Some(&b'r');
    if raw {
        index += 1;
        let hashes = bytes[index..]
            .iter()
            .take_while(|byte| **byte == b'#')
            .count();
        index += hashes;
        if bytes.get(index) != Some(&b'"') {
            return None;
        }
        let close = format!("\"{}", "#".repeat(hashes));
        let body_start = index + 1;
        let end = source[body_start..].find(&close)? + body_start;
        return Some((
            source[body_start..end].to_owned(),
            end + close.len(),
            is_bytes,
        ));
    }
    if bytes.get(index) != Some(&b'"') || (index != start && !is_bytes && bytes[start] != b'c') {
        if bytes[start] == b'"' {
            index = start;
        } else {
            return None;
        }
    }
    let body_start = index + 1;
    let mut end = body_start;
    while end < bytes.len() && bytes[end] != b'"' {
        end += if bytes[end] == b'\\' { 2 } else { 1 };
    }
    Some((source[body_start..end].to_owned(), end + 1, is_bytes))
}

fn is_identifier(text: &str) -> bool {
    let contract = text == "HOST"
        || text.starts_with("HOST.")
        || (text.len() > 2
            && text.starts_with("BN")
            && text.as_bytes()[2].is_ascii_uppercase()
            && text.as_bytes()[3..]
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'.' || *byte == b'_'));
    let code = text.contains('_')
        && text.as_bytes()[0].is_ascii_uppercase()
        && text
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_');
    (contract
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'_'))
        || code
}

fn is_prose(text: &str) -> bool {
    let mut without_placeholders = String::new();
    let mut nesting = 0usize;
    for character in text.chars() {
        match character {
            '{' => nesting += 1,
            '}' => nesting = nesting.saturating_sub(1),
            _ if nesting == 0 => without_placeholders.push(character),
            _ => {}
        }
    }
    let words = without_placeholders
        .split(|character: char| !character.is_ascii_alphabetic())
        .filter(|word| word.len() >= 2)
        .count();
    words >= 2 && without_placeholders.contains(' ')
}

fn is_llvm_template(text: &str) -> bool {
    let trimmed = text.trim_start();
    text.contains(['%', '@', '!'])
        || [" = private ", "unnamed_addr", " x i8]"]
            .iter()
            .any(|marker| text.contains(marker))
        || LLVM_KEYWORDS.iter().any(|keyword| {
            trimmed
                .strip_prefix(keyword)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', ',', '(']))
        })
}

fn classify(path: &str, text: &str) -> Option<Kind> {
    if path.starts_with("crates/bn_llvm/") && is_llvm_template(text) {
        return None;
    }
    // Wire-protocol framing (`Content-Length: …\r\n\r\n` of LSP and DAP) is
    // machine text, not prose.
    if text.contains("\\r\\n") {
        return None;
    }
    if is_identifier(text) {
        Some(Kind::Identifier)
    } else if is_prose(text) {
        Some(Kind::Prose)
    } else {
        None
    }
}

fn rust_sources(directory: &Path, files: &mut Vec<PathBuf>) {
    let mut entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("{}: {error}", directory.display()))
        .map(|entry| entry.expect("directory entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            rust_sources(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

fn is_test_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    name == "tests.rs"
        || name == "test_support.rs"
        || name == "build.rs"
        || name.ends_with("_tests.rs")
        || path.components().any(|part| part.as_os_str() == "tests")
}

/// The file a `mod name;` in `file` refers to.
fn module_files(file: &Path, name: &str) -> [PathBuf; 2] {
    let parent = file.parent().unwrap_or(Path::new("."));
    let stem = file
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    let directory = if matches!(stem, "lib" | "main" | "mod") {
        parent.to_path_buf()
    } else {
        parent.join(stem)
    };
    [
        directory.join(format!("{name}.rs")),
        directory.join(name).join("mod.rs"),
    ]
}

/// Every production literal that counts, as `(path, kind, text)` with its
/// number of occurrences.
fn production_literals() -> BTreeMap<(String, Kind, String), usize> {
    let mut files = Vec::new();
    for crate_dir in fs::read_dir("crates").expect("crates directory") {
        let source = crate_dir.expect("crate entry").path().join("src");
        if source.is_dir() {
            rust_sources(&source, &mut files);
        }
    }
    let scans: Vec<(PathBuf, Scan)> = files
        .into_iter()
        .map(|path| {
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let scan = scan(&source);
            (path, scan)
        })
        .collect();
    let test_files: Vec<PathBuf> = scans
        .iter()
        .flat_map(|(path, scan)| {
            scan.test_modules
                .iter()
                .flat_map(|name| module_files(path, name))
                .collect::<Vec<_>>()
        })
        .collect();
    let mut found = BTreeMap::new();
    for (path, scan) in scans {
        if is_test_file(&path) || test_files.contains(&path) {
            continue;
        }
        let path = path.to_string_lossy().replace('\\', "/");
        for literal in scan.literals.iter().filter(|literal| !literal.in_test) {
            if let Some(kind) = classify(&path, &literal.text) {
                *found
                    .entry((path.clone(), kind, literal.text.clone()))
                    .or_insert(0) += 1;
            }
        }
    }
    found
}

/// One list line: path, kind and the literal as written, with backslash,
/// newline and tab escaped so that every literal fits one line.
fn line(path: &str, kind: Kind, text: &str) -> String {
    let text = text
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\t', "\\t");
    format!("{path}\t{}\t{text}", kind.name())
}

#[test]
fn production_code_has_no_new_text_literal() {
    let found = production_literals();
    let Ok(listed_text) = fs::read_to_string(LIST) else {
        let mut lines: Vec<String> = Vec::new();
        for ((path, kind, text), count) in &found {
            lines.extend(std::iter::repeat_n(line(path, *kind, text), *count));
        }
        fs::write(LIST, lines.join("\n") + "\n").expect("write the list");
        panic!(
            "{LIST} did not exist: written with {} literals; review it",
            lines.len()
        );
    };
    let mut listed: BTreeMap<String, usize> = BTreeMap::new();
    for entry in listed_text
        .lines()
        .filter(|entry| !entry.is_empty() && !entry.starts_with('#'))
    {
        *listed.entry(entry.to_owned()).or_insert(0) += 1;
    }
    let mut actual: BTreeMap<String, usize> = BTreeMap::new();
    for ((path, kind, text), count) in &found {
        *actual.entry(line(path, *kind, text)).or_insert(0) += count;
    }
    let mut new = Vec::new();
    let mut stale = Vec::new();
    for (entry, count) in &actual {
        let allowed = listed.get(entry).copied().unwrap_or(0);
        new.extend(std::iter::repeat_n(
            entry.clone(),
            count.saturating_sub(allowed),
        ));
    }
    for (entry, count) in &listed {
        let present = actual.get(entry).copied().unwrap_or(0);
        stale.extend(std::iter::repeat_n(
            entry.clone(),
            count.saturating_sub(present),
        ));
    }
    let mut report = Vec::new();
    if !new.is_empty() {
        report.push(format!(
            "{} new text literal(s) in production code; put the text in a data file under share/bn/ (Fluent, txt, json) and refer to it by key:\n  {}\n",
            new.len(),
            new.join("\n  ")
        ));
    }
    if !stale.is_empty() {
        report.push(format!(
            "{} entr(ies) of {LIST} no longer occur; remove them (the list only shrinks):\n  {}\n",
            stale.len(),
            stale.join("\n  ")
        ));
    }
    assert!(report.is_empty(), "{}", report.concat());
}

/// The lexer and the classification (H.S1.b proves the gate itself on the
/// tree; these pin the rules).
#[test]
fn the_scanner_finds_production_literals_and_skips_test_code() {
    let source = r##"
// "in a comment"
/* "in a block" */
fn f() -> &'static str {
    let c = '"';
    let raw = r#"server is "already" started"#;
    let bytes = b"not counted text here";
    "HOST.Net.TCPStream.Read"
}
#[cfg(test)]
mod tests {
    fn g() { let _ = "test only text here"; }
}
#[cfg(test)]
mod extra_tests;
fn h() { let _ = "{name}: {error}"; let _ = "MODULE_NOT_FOUND"; }
"##;
    let scan = scan(source);
    let production: Vec<&str> = scan
        .literals
        .iter()
        .filter(|literal| !literal.in_test)
        .map(|literal| literal.text.as_str())
        .collect();
    assert_eq!(
        production,
        [
            r#"server is "already" started"#,
            "HOST.Net.TCPStream.Read",
            "{name}: {error}",
            "MODULE_NOT_FOUND"
        ]
    );
    assert_eq!(scan.test_modules, ["extra_tests"]);
    assert_eq!(
        classify("crates/x/src/a.rs", production[0]),
        Some(Kind::Prose)
    );
    assert_eq!(
        classify("crates/x/src/a.rs", production[1]),
        Some(Kind::Identifier)
    );
    assert_eq!(classify("crates/x/src/a.rs", production[2]), None);
    assert_eq!(
        classify("crates/x/src/a.rs", production[3]),
        Some(Kind::Identifier)
    );
    assert_eq!(
        classify("crates/x/src/a.rs", "BNWeb"),
        Some(Kind::Identifier)
    );
    assert_eq!(classify("crates/x/src/a.rs", "GET"), None);
    assert_eq!(
        classify("crates/bn_llvm/src/a.rs", "declare i32 @bn_rt_x(i64)"),
        None
    );
    assert_eq!(
        classify(
            "crates/bn_llvm/src/a.rs",
            "calls to user-defined function are unavailable"
        ),
        Some(Kind::Prose)
    );
}
