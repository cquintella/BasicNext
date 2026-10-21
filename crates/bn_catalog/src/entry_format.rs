// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The catalog file format, shared by the build script (which generates one
//! constant per entry), the crate (which looks entries up by key) and the
//! migration tool (`scripts/catalog-migrate`, which writes entries).
//!
//! One `key = value` per line; empty lines and `#` lines are ignored. A
//! value is escaped: `\\` is a backslash, `\n` a newline, `\t` a tab, `\r` a
//! carriage return, and `\s` a space (written at the edges of a value, where
//! an editor would strip a plain one).

/// The `key = value` entries of one catalog file, in file order, with each
/// value unescaped.
///
/// # Errors
///
/// A line without ` = `, or a value with an unknown escape, names the line.
pub fn parse(contents: &str) -> Result<Vec<(&str, String)>, String> {
    let mut entries = Vec::new();
    for line in contents.lines() {
        let line = line.trim_end();
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (key, value) = trimmed
            .split_once(" = ")
            .ok_or_else(|| note("not-key-value").replace("{line}", line))?;
        entries.push((
            key.trim(),
            unescape(value).map_err(|error| format!("`{line}`: {error}"))?,
        ));
    }
    Ok(entries)
}

/// The value of a catalog line as it is read back.
///
/// # Errors
///
/// An unknown escape or a trailing lone backslash.
pub(crate) fn unescape(value: &str) -> Result<String, String> {
    let mut out = String::with_capacity(value.len());
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match characters.next() {
            Some('\\') => out.push('\\'),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('s') => out.push(' '),
            Some(other) => {
                return Err(note("unknown-escape").replace("{other}", &other.to_string()));
            }
            None => return Err(note("trailing-backslash")),
        }
    }
    Ok(out)
}

/// The wording of a parse failure. The file is not itself a catalog: this
/// parser is what reads catalogs, including while `bn_catalog` is building.
fn note(key: &str) -> String {
    let notes = include_str!("../../../share/bn/text/catalog-parse.txt");
    for line in notes.lines() {
        let Some(value) = line
            .strip_prefix(key)
            .and_then(|rest| rest.strip_prefix('='))
        else {
            continue;
        };
        return value.to_owned();
    }
    key.to_owned()
}

/// The catalog line form of `value`: the inverse of [`unescape`].
#[must_use]
pub fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    let leading = out.len() - out.trim_start_matches(' ').len();
    let trailing = out.len() - out.trim_end_matches(' ').len();
    if leading == out.len() {
        return "\\s".repeat(leading);
    }
    let middle = &out[leading..out.len() - trailing];
    format!(
        "{}{middle}{}",
        "\\s".repeat(leading),
        "\\s".repeat(trailing)
    )
}

/// The name of the generated constant of `key`: upper case, with every
/// character that cannot be part of a Rust identifier replaced by `_`.
#[must_use]
pub fn const_name(key: &str) -> String {
    let name: String = key
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    if name.starts_with(|character: char| character.is_ascii_digit()) {
        format!("K_{name}")
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::{const_name, escape, parse, unescape};

    #[test]
    fn escape_and_unescape_are_inverse_at_the_edges_and_inside() {
        for value in [
            "plain",
            "  two leading",
            "trailing ",
            "a\nb\tc\\d\re",
            "CONFIG_INVALID: ",
            " ",
            "",
        ] {
            assert_eq!(unescape(&escape(value)).as_deref(), Ok(value), "{value:?}");
        }
        assert_eq!(escape("x "), "x\\s");
    }

    #[test]
    fn parse_skips_comments_and_rejects_unknown_escapes() {
        let entries = parse("# c\n\na.b = x\\sy\nc-d = \\\\{e}\n").expect("parse");
        assert_eq!(
            entries,
            [("a.b", "x y".to_owned()), ("c-d", "\\{e}".to_owned())]
        );
        assert!(parse("k = \\q\n").is_err());
        assert!(parse("no separator\n").is_err());
    }

    #[test]
    fn const_names_are_identifiers() {
        assert_eq!(const_name("cidr.host_bits"), "CIDR_HOST_BITS");
        assert_eq!(const_name("retain-overflow"), "RETAIN_OVERFLOW");
        assert_eq!(const_name("1x"), "K_1X");
    }
}
