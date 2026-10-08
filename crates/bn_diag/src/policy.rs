// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Policy and configuration parsing for compiler and runtime warning control.

use crate::{DiagId, Level};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Default)]
pub struct WarningPolicy {
    config_default: Option<Level>,
    config_levels: HashMap<DiagId, Level>,
    cli_levels: HashMap<DiagId, Level>,
    warnings_as_errors: bool,
}

impl WarningPolicy {
    /// Parses the warning subset defined by the 0.4.5 `config.toml` contract.
    /// Unknown keys outside the warning tables are ignored by design.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed entries, unknown diagnostic codes,
    /// invalid levels or an invalid global default.
    pub fn from_config(text: &str) -> Result<Self, String> {
        let mut policy = Self::default();
        let mut section = "";
        let mut seen = HashSet::new();
        for raw in text.lines() {
            let line = strip_config_comment(raw)?.trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = &line[1..line.len() - 1];
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(format!("malformed warning configuration: {line}"));
            };
            let key = key.trim();
            let full_key = format!("{section}.{}", key.trim());
            if matches!(section, "warnings" | "warnings.levels") && !seen.insert(full_key) {
                return Err(format!(
                    "duplicate warning configuration key: {}",
                    key.trim()
                ));
            }
            match section {
                "warnings" if key == "default" => {
                    let value = parse_config_string(value.trim())?;
                    let level = parse_level(&value)?;
                    if level == Level::Allow {
                        return Err("warning default cannot be allow".into());
                    }
                    policy.config_default = Some(level);
                }
                "warnings.levels" => {
                    let value = parse_config_string(value.trim())?;
                    let id = DiagId::from_code(key)
                        .ok_or_else(|| format!("unknown diagnostic code in warnings: {key}"))?;
                    policy.set_config(id, parse_level(&value)?)?;
                }
                "warnings" => return Err(format!("unknown warnings key: {key}")),
                _ => {}
            }
        }
        Ok(policy)
    }

    /// # Errors
    ///
    /// Returns an error when `allow` is requested for a hard diagnostic.
    pub fn set_config(&mut self, id: DiagId, level: Level) -> Result<(), String> {
        validate_level(id, level)?;
        self.config_levels.insert(id, level);
        Ok(())
    }

    /// # Errors
    ///
    /// Returns an error when `allow` is requested for a hard diagnostic.
    pub fn set_cli(&mut self, id: DiagId, level: Level) -> Result<(), String> {
        validate_level(id, level)?;
        self.cli_levels.insert(id, level);
        Ok(())
    }

    pub fn set_warnings_as_errors(&mut self, enabled: bool) {
        self.warnings_as_errors = enabled;
    }

    #[must_use]
    pub fn level(&self, id: DiagId) -> Level {
        if !id.warnings_allowed() {
            return Level::Error;
        }
        if let Some(level) = self.cli_levels.get(&id) {
            return *level;
        }
        if self.warnings_as_errors {
            return Level::Error;
        }
        if let Some(level) = self.config_levels.get(&id) {
            return *level;
        }
        if let Some(level) = self.config_default {
            return level;
        }
        Level::Warn
    }
}

pub(crate) fn strip_config_comment(line: &str) -> Result<&str, String> {
    let mut quoted = false;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '#' if !quoted => return Ok(&line[..index]),
            _ => {}
        }
    }
    if quoted || escaped {
        return Err("unterminated quoted config value".into());
    }
    Ok(line)
}

pub(crate) fn parse_config_string(value: &str) -> Result<String, String> {
    if !(value.starts_with('"') && value.ends_with('"') && value.len() >= 2) {
        return Err(format!(
            "configuration value must be a quoted string: {value}"
        ));
    }
    let mut output = String::new();
    let mut characters = value[1..value.len() - 1].chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        output.push(match characters.next() {
            Some('"') => '"',
            Some('\\') => '\\',
            Some('n') => '\n',
            Some('t') => '\t',
            Some(other) => return Err(format!("unsupported config escape: \\{other}")),
            None => return Err("unterminated config escape".into()),
        });
    }
    Ok(output)
}

fn parse_level(value: &str) -> Result<Level, String> {
    match value {
        "allow" => Ok(Level::Allow),
        "warn" => Ok(Level::Warn),
        "error" => Ok(Level::Error),
        _ => Err(format!("invalid warning level: {value}")),
    }
}

fn validate_level(id: DiagId, level: Level) -> Result<(), String> {
    if level == Level::Allow && !id.warnings_allowed() {
        return Err(format!("cannot allow hard diagnostic {}", id.code()));
    }
    Ok(())
}
