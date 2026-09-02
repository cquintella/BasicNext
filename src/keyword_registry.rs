// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Parses the versioned keyword registry and EBNF productions.
//!
//! `build.rs` includes this file by path so the generator and the U1 tests
//! share one parser. It has no `crate::` imports.

pub const RESERVED_BEGIN: &str = "<!-- reserved-words:start -->";
pub const RESERVED_END: &str = "<!-- reserved-words:end -->";
pub const SPECIAL_BEGIN: &str = "<!-- special-float-literals:start -->";
pub const SPECIAL_END: &str = "<!-- special-float-literals:end -->";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registry {
    pub reserved: Vec<String>,
    pub special_literals: Vec<String>,
}

/// Parses both marked lists from `keywords.md`.
///
/// # Errors
///
/// Returns a message when a marker pair is missing or duplicated, a list is
/// empty or unsorted, or an entry is not an uppercase identifier.
pub fn parse_keywords_md(text: &str) -> Result<Registry, String> {
    let reserved = parse_marked_list(text, RESERVED_BEGIN, RESERVED_END, "reserved word")?;
    let special_literals =
        parse_marked_list(text, SPECIAL_BEGIN, SPECIAL_END, "special float literal")?;
    if reserved.iter().any(|word| special_literals.contains(word)) {
        return Err("a spelling cannot be both a reserved word and a special literal".into());
    }
    Ok(Registry {
        reserved,
        special_literals,
    })
}

/// Collects quoted terminals from one EBNF production, sorted uniquely.
///
/// # Errors
///
/// Returns a message when the production is missing or a quote is unterminated.
pub fn parse_ebnf_quoted_production(ebnf: &str, name: &str) -> Result<Vec<String>, String> {
    let body = ebnf_production_body(ebnf, name)?;
    let mut words = Vec::new();
    let mut chars = body.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '"' {
            continue;
        }
        let mut word = String::new();
        loop {
            match chars.next() {
                Some('"') => break,
                Some(character) => word.push(character),
                None => return Err(format!("unterminated quoted terminal in {name}")),
            }
        }
        words.push(word);
    }
    if words.is_empty() {
        return Err(format!("EBNF production '{name}' has no quoted terminals"));
    }
    words.sort();
    words.dedup();
    Ok(words)
}

/// Parses one HTML-comment-delimited identifier list.
///
/// # Errors
///
/// Returns a message when markers are not a unique ordered pair or the list
/// violates identifier, uniqueness, or sort rules.
pub fn parse_marked_list(
    text: &str,
    begin: &str,
    end: &str,
    kind: &str,
) -> Result<Vec<String>, String> {
    let section = marked_section(text, begin, end)?;
    let mut words = Vec::new();
    for line in section.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("```") {
            continue;
        }
        validate_identifier(line, kind)?;
        words.push(line.to_string());
    }
    validate_sorted_unique(&words, kind)?;
    Ok(words)
}

fn marked_section<'a>(text: &'a str, begin: &str, end: &str) -> Result<&'a str, String> {
    let starts = text.matches(begin).count();
    let ends = text.matches(end).count();
    if starts != 1 || ends != 1 {
        return Err(format!(
            "markers '{begin}' and '{end}' must each appear exactly once (found {starts} and {ends})"
        ));
    }
    let Some((_, after)) = text.split_once(begin) else {
        return Err(format!("missing start marker {begin}"));
    };
    let Some((section, _)) = after.split_once(end) else {
        return Err(format!("start marker {begin} is not followed by {end}"));
    };
    Ok(section)
}

fn validate_identifier(word: &str, kind: &str) -> Result<(), String> {
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return Err(format!("{kind} list contains an empty entry"));
    };
    if !first.is_ascii_uppercase() {
        return Err(format!(
            "{kind} '{word}' must start with an uppercase ASCII letter"
        ));
    }
    if !chars.all(|character| character.is_ascii_uppercase() || character.is_ascii_digit()) {
        return Err(format!(
            "{kind} '{word}' may contain only uppercase ASCII letters and digits"
        ));
    }
    Ok(())
}

fn validate_sorted_unique(words: &[String], kind: &str) -> Result<(), String> {
    if words.is_empty() {
        return Err(format!("{kind} list cannot be empty"));
    }
    if !words.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err(format!("{kind} list must be unique and strictly sorted"));
    }
    Ok(())
}

fn ebnf_production_body(ebnf: &str, name: &str) -> Result<String, String> {
    let mut body = String::new();
    let mut recording = false;
    for line in ebnf.lines() {
        let trimmed = line.trim_start();
        if !recording {
            if let Some(rest) = trimmed.strip_prefix(name) {
                let rest = rest.trim_start();
                let Some(rest) = rest.strip_prefix('=') else {
                    continue;
                };
                recording = true;
                body.push_str(rest);
            }
            continue;
        }
        body.push(' ');
        body.push_str(trimmed);
        if trimmed.contains(';') {
            break;
        }
    }
    if !recording {
        return Err(format!("EBNF production '{name}' was not found"));
    }
    Ok(body
        .split_once(';')
        .map(|(before, _)| before.to_string())
        .unwrap_or(body))
}
