// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A `BNWeb` failure is typed where it happens: no code reads the words of a
//! message to choose an error code, and no message states a limit the
//! configuration can change.

use std::fs;
use std::path::{Path, PathBuf};

/// The provider and its two adapters.
const WEB_SOURCES: [&str; 2] = ["libraries/bn_web/src", "libraries/bn_lib_web/src"];
/// The call that turned a message back into a code.
const PARSED_FAILURE: &str = "from_message(";
/// Units a hard-coded limit is written in.
const LIMIT_UNITS: [&str; 3] = [" KiB", " MiB", " GiB"];

fn production_sources(directory: &Path, files: &mut Vec<PathBuf>) {
    let mut entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("{}: {error}", directory.display()))
        .map(|entry| entry.expect("directory entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if name != "tests" {
                production_sources(&path, files);
            }
        } else if path.extension().is_some_and(|extension| extension == "rs")
            && name != "tests.rs"
            && !name.ends_with("_tests.rs")
        {
            files.push(path);
        }
    }
}

/// The lines of a source before its inline test module (`#[cfg(test)]` on a
/// `mod … {`); a test-only item elsewhere stays, which only adds findings.
fn production_text(path: &Path) -> String {
    let text = fs::read_to_string(path).expect("read a source");
    let lines = text.lines().collect::<Vec<_>>();
    let end = lines
        .windows(2)
        .position(|pair| {
            pair[0].trim() == "#[cfg(test)]"
                && pair[1].trim_start().starts_with("mod ")
                && pair[1].trim_end().ends_with('{')
        })
        .unwrap_or(lines.len());
    lines[..end].join("\n")
}

/// `path:line` of every line holding `needle` in a string literal or call.
fn findings(sources: &[(String, String)], needle: impl Fn(&str) -> bool) -> Vec<String> {
    sources
        .iter()
        .flat_map(|(path, text)| {
            text.lines()
                .enumerate()
                .filter(|(_, line)| {
                    let line = line.trim_start();
                    !line.starts_with("//") && needle(line)
                })
                .map(move |(index, _)| format!("{path}:{}", index + 1))
        })
        .collect()
}

fn web_sources() -> Vec<(String, String)> {
    let mut files = Vec::new();
    for root in WEB_SOURCES {
        production_sources(Path::new(root), &mut files);
    }
    files
        .iter()
        .map(|path| {
            (
                path.to_string_lossy().replace('\\', "/"),
                production_text(path),
            )
        })
        .collect()
}

/// A line that writes a limit unit inside a string literal.
fn names_a_limit(line: &str) -> bool {
    line.split('"')
        .skip(1)
        .step_by(2)
        .any(|literal| LIMIT_UNITS.iter().any(|unit| literal.contains(unit)))
}

#[test]
fn web_failures_are_typed_where_they_happen() {
    let found = findings(&web_sources(), |line| line.contains(PARSED_FAILURE));
    assert_eq!(
        found,
        Vec::<String>::new(),
        "a failure parsed from its message"
    );
}

#[test]
fn no_web_message_states_a_limit() {
    let found = findings(&web_sources(), names_a_limit);
    assert_eq!(
        found,
        Vec::<String>::new(),
        "a limit written into a message"
    );
}

#[test]
fn a_planted_parse_and_a_planted_limit_are_found() {
    let sources = vec![(
        "planted.rs".to_owned(),
        "// from_message( in a comment\nlet f = WebFailure::from_message(&text);\nErr(\"body exceeds 8 MiB\".into())\n"
            .to_owned(),
    )];
    assert_eq!(
        findings(&sources, |line| line.contains(PARSED_FAILURE)),
        ["planted.rs:2"],
        "a comment is not a call"
    );
    assert_eq!(findings(&sources, names_a_limit), ["planted.rs:3"]);
}
