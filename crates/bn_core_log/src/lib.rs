// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNLog` core (R2): levels, record serialization, sensitive key redaction,
//! error representations and transport dispatch.

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::Path;

pub mod error;
pub use error::LogFailure;

const MAX_RECORD_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Level {
    Error = 0,
    Warn = 1,
    Info = 2,
    Http = 3,
    Verbose = 4,
    Debug = 5,
    Silly = 6,
}

impl Level {
    #[must_use]
    pub fn from_i64(value: i64) -> Option<Self> {
        match value {
            0 => Some(Self::Error),
            1 => Some(Self::Warn),
            2 => Some(Self::Info),
            3 => Some(Self::Http),
            4 => Some(Self::Verbose),
            5 => Some(Self::Debug),
            6 => Some(Self::Silly),
            _ => None,
        }
    }

    #[must_use]
    pub fn from_i128(value: i128) -> Option<Self> {
        i64::try_from(value).ok().and_then(Self::from_i64)
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Http => "HTTP",
            Self::Verbose => "VERBOSE",
            Self::Debug => "DEBUG",
            Self::Silly => "SILLY",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    pub timestamp: String,
    pub label: String,
    pub level: Level,
    pub message: String,
    pub fields: BTreeMap<String, String>,
}

impl Record {
    /// Builds a record from timestamp, label, level, message, context and provided fields.
    #[must_use]
    pub fn with_timestamp<'a>(
        timestamp: String,
        label: &str,
        level: Level,
        message: &str,
        context: &BTreeMap<String, String>,
        provided: impl IntoIterator<Item = (&'a String, &'a String)>,
    ) -> Self {
        let mut fields = context.clone();
        fields.extend(
            provided
                .into_iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        );
        Self {
            timestamp,
            label: label.to_owned(),
            level,
            message: message.to_owned(),
            fields,
        }
    }

    /// # Errors
    ///
    /// Returns an error when the bounded output exceeds the runtime limit.
    pub fn json_line(&self) -> Result<String, &'static str> {
        let fields = self
            .fields
            .iter()
            .filter(|(key, _)| !is_sensitive(key))
            .map(|(key, value)| format!("{}:{}", json_string(key), json_string(value)))
            .collect::<Vec<_>>()
            .join(",");
        bounded(format!(
            "{{\"fields\":{{{fields}}},\"label\":{},\"level\":{},\"message\":{},\"timestamp\":{}}}",
            json_string(&self.label),
            json_string(self.level.name()),
            json_string(&self.message),
            json_string(&self.timestamp),
        ))
    }

    /// # Errors
    ///
    /// Returns an error when the bounded output exceeds the runtime limit.
    pub fn text_line(&self) -> Result<String, &'static str> {
        let fields = self
            .fields
            .iter()
            .filter(|(key, _)| !is_sensitive(key))
            .map(|(key, value)| format!("{}={}", escape(key), escape(value)))
            .collect::<Vec<_>>()
            .join(" ");
        bounded(format!(
            "{} {} {} {}{}\n",
            self.timestamp,
            self.label,
            self.level.name(),
            escape(&self.message),
            if fields.is_empty() {
                String::new()
            } else {
                format!(" {fields}")
            }
        ))
    }

    /// # Errors
    ///
    /// Returns an error when the bounded output exceeds the runtime limit.
    pub fn apache_combined(&self) -> Result<String, &'static str> {
        let field = |name: &str| self.fields.get(name).map_or("-", String::as_str);
        bounded(format!(
            "{} - - [{}] \"{}\" {} {} \"{}\" \"{}\"\n",
            escape(field("remote")),
            escape(&self.timestamp),
            escape(&strip_query(field("request"))),
            escape(field("status")),
            escape(field("bytes_sent")),
            escape(&strip_query(field("referrer"))),
            escape(field("user_agent"))
        ))
    }
}

/// A configured file transport destination.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileTransport {
    pub path: String,
    pub minimum: Level,
}

/// Dispatches a record line across console and file transports.
///
/// Follows D2:
/// - Every transport is attempted.
/// - The first failure (if any) is returned as `Err(LogFailure)`.
/// - A permission denial when opening a file returns `Err(LogFailure::IoFailed(...))`.
///
/// # Errors
///
/// Returns `LogFailure` on write or file open error.
pub fn dispatch_log(
    line: &str,
    level: Level,
    console_transports: &[Level],
    file_transports: &[FileTransport],
    mut write_console: impl FnMut(&str) -> io::Result<()>,
    open_append: &dyn Fn(&Path) -> io::Result<Box<dyn Write>>,
) -> Result<(), LogFailure> {
    let mut first_error: Option<String> = None;

    for minimum in console_transports {
        if level <= *minimum
            && let Err(err) = write_console(line)
        {
            first_error.get_or_insert_with(|| err.to_string());
        }
    }

    for transport in file_transports {
        if level > transport.minimum {
            continue;
        }
        let write_res = open_append(Path::new(&transport.path)).and_then(|mut file| {
            file.write_all(line.as_bytes())?;
            file.write_all(b"\n")
        });
        if let Err(err) = write_res {
            first_error.get_or_insert_with(|| err.to_string());
        }
    }

    if let Some(err) = first_error {
        Err(LogFailure::IoFailed(err))
    } else {
        Ok(())
    }
}

fn json_string(value: &str) -> String {
    let mut result = String::with_capacity(value.len() + 2);
    result.push('"');
    for character in value.chars() {
        match character {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            character if character.is_control() => {
                use std::fmt::Write as _;
                let _ = write!(result, "\\u{:04x}", u32::from(character));
            }
            character => result.push(character),
        }
    }
    result.push('"');
    result
}

fn bounded(value: String) -> Result<String, &'static str> {
    (value.len() <= MAX_RECORD_BYTES)
        .then_some(value)
        .ok_or("serialized log record exceeds 64 KiB")
}

fn escape(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '\n' | '\r' | '\t' => ' ',
            character if character.is_control() => '?',
            character => character,
        })
        .collect()
}

#[must_use]
pub fn is_sensitive(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase().replace(['-', '.'], "_");
    matches!(
        normalized.as_str(),
        "authorization"
            | "proxy_authorization"
            | "cookie"
            | "set_cookie"
            | "session"
            | "session_id"
            | "query"
            | "body"
            | "password"
            | "passwd"
            | "secret"
            | "token"
            | "api_key"
            | "apikey"
            | "private_key"
            | "client_secret"
            | "access_key"
            | "tls_key"
            | "credential"
            | "bearer"
            | "refresh_token"
            | "jwt"
            | "signature"
    ) || [
        "authorization",
        "cookie",
        "session",
        "password",
        "secret",
        "token",
        "api_key",
        "apikey",
        "access_key",
        "private_key",
        "client_secret",
        "credential",
        "bearer",
        "refresh_token",
        "jwt",
        "signature",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn strip_query(value: &str) -> String {
    value
        .split_whitespace()
        .map(|part| part.split_once('?').map_or(part, |(path, _)| path))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_merging_and_json() {
        let context = BTreeMap::from([
            ("service".to_owned(), "api".to_owned()),
            ("route".to_owned(), "/".to_owned()),
        ]);
        let provided = BTreeMap::from([("route".to_owned(), "/users".to_owned())]);
        let record = Record::with_timestamp(
            "2026-10-08T12:00:00.000Z".into(),
            "web",
            Level::Info,
            "hit",
            &context,
            &provided,
        );
        assert_eq!(record.fields["service"], "api");
        assert_eq!(record.fields["route"], "/users");
        let json = record.json_line().unwrap();
        assert!(json.contains("\"route\":\"/users\""));
        assert!(json.contains("\"service\":\"api\""));
    }

    #[test]
    fn dispatch_log_handles_failures_deterministically() {
        let console_called = std::sync::atomic::AtomicBool::new(false);
        let res = dispatch_log(
            "test log line",
            Level::Error,
            &[Level::Error],
            &[FileTransport {
                path: "/nonexistent/test.log".into(),
                minimum: Level::Error,
            }],
            |_| {
                console_called.store(true, std::sync::atomic::Ordering::Relaxed);
                Ok(())
            },
            &|_| Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied")),
        );
        assert!(console_called.load(std::sync::atomic::Ordering::Relaxed));
        assert!(matches!(res, Err(LogFailure::IoFailed(msg)) if msg.contains("denied")));
    }

    /// A console that fails does not stop the file transport, and its
    /// failure is the one returned (every transport is attempted; the first
    /// failure wins).
    #[test]
    fn failed_console_still_writes_the_file_and_reports_first() {
        let written = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
        let res = dispatch_log(
            "line",
            Level::Error,
            &[Level::Error],
            &[FileTransport {
                path: "log.txt".into(),
                minimum: Level::Error,
            }],
            |_| Err(io::Error::other("console closed")),
            &|_| {
                struct Sink(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
                impl Write for Sink {
                    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                        self.0
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .extend_from_slice(bytes);
                        Ok(bytes.len())
                    }
                    fn flush(&mut self) -> io::Result<()> {
                        Ok(())
                    }
                }
                Ok(Box::new(Sink(std::sync::Arc::clone(&written))) as Box<dyn Write>)
            },
        );
        assert!(matches!(res, Err(LogFailure::IoFailed(msg)) if msg == "console closed"));
        assert_eq!(
            written
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_slice(),
            b"line\n"
        );
    }
}
