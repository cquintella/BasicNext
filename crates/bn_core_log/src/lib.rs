// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNLog` core (R2): levels, record serialization, sensitive key redaction,
//! error representations, records and the logger.

use std::collections::BTreeMap;

mod catalog;
pub mod error;
pub use error::LogFailure;
mod fields;
pub use fields::FieldMap;
mod logger;
pub use logger::{Destination, Imports, LogSink, Logger, Operation, TIMEOUT_MAX_MS};
mod records;
pub use records::{Apache, Event, http_client_access};

const MAX_RECORD_BYTES: usize = 64 * 1024;
const RECORD_TOO_LONG: &str = "serialized log record exceeds 64 KiB";

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
    pub(crate) fn from_i128(value: i128) -> Option<Self> {
        i64::try_from(value).ok().and_then(Self::from_i64)
    }

    /// The RFC 5424 severity: ERROR 3, WARN 4, INFO and HTTP 6, VERBOSE,
    /// DEBUG and SILLY 7.
    #[must_use]
    pub(crate) const fn syslog_severity(self) -> u8 {
        match self {
            Self::Error => 3,
            Self::Warn => 4,
            Self::Info | Self::Http => 6,
            Self::Verbose | Self::Debug | Self::Silly => 7,
        }
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

    /// Every level, most severe first.
    pub const ALL: [Self; 7] = [
        Self::Error,
        Self::Warn,
        Self::Info,
        Self::Http,
        Self::Verbose,
        Self::Debug,
        Self::Silly,
    ];

    /// The contract name of the `Logger` method that writes at this level
    /// (`BNLog.Logger.Info`).
    #[must_use]
    pub fn operation(self) -> &'static str {
        catalog::text(match self {
            Self::Error => "op.logger.error",
            Self::Warn => "op.logger.warn",
            Self::Info => "op.logger.info",
            Self::Http => "op.logger.http",
            Self::Verbose => "op.logger.verbose",
            Self::Debug => "op.logger.debug",
            Self::Silly => "op.logger.silly",
        })
    }

    /// The level whose `Logger` method has the contract name `operation`.
    #[must_use]
    pub fn of_operation(operation: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|level| level.operation() == operation)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    pub timestamp: String,
    pub label: String,
    pub level: Level,
    pub message: String,
    pub fields: BTreeMap<String, String>,
    /// The machine that wrote the record (SYSLOG `HOSTNAME`); empty when
    /// the platform does not tell.
    pub host: String,
    /// The writing process (SYSLOG `PROCID`).
    pub process: u32,
}

impl Record {
    /// Builds a record from timestamp, label, level, message, context and provided fields.
    #[must_use]
    pub(crate) fn with_timestamp<'a>(
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
            host: bn_host_env::host_name().unwrap_or_default(),
            process: std::process::id(),
        }
    }

    /// The record as one line in `format`, without a line terminator: the
    /// transport writes exactly one line feed after it (bnlog.md, "Formats").
    ///
    /// # Errors
    ///
    /// Returns an error as soon as the line would exceed the runtime limit;
    /// the limit is checked while building, so an oversized record is never
    /// built whole.
    pub fn line(&self, format: Format) -> Result<String, &'static str> {
        let record = self.without_queries();
        let mut line = Line::default();
        match format {
            Format::Json => record.json(&mut line)?,
            Format::Syslog => record.syslog(&mut line)?,
            Format::Apache => record.apache(&mut line)?,
        }
        Ok(line.text)
    }

    /// The record with the query of `http.path` and `http.referrer` removed: a
    /// URL query may carry a token or a session id, so no format writes it
    /// (bnlog.md, "Formats"; bnweb.md never logs query values).
    fn without_queries(&self) -> Self {
        let mut record = self.clone();
        for key in ["apache.path", "apache.referrer"] {
            if let Some(value) = record.fields.get_mut(catalog::text(key)) {
                *value = without_query(value);
            }
        }
        record
    }

    fn written_fields(&self) -> impl Iterator<Item = (&String, &String)> {
        self.fields.iter().filter(|(key, _)| !is_sensitive(key))
    }

    fn json(&self, line: &mut Line) -> Result<(), &'static str> {
        line.push("{\"fields\":{")?;
        for (index, (key, value)) in self.written_fields().enumerate() {
            if index > 0 {
                line.push(",")?;
            }
            line.json_string(key)?;
            line.push(":")?;
            line.json_string(value)?;
        }
        line.push("},\"label\":")?;
        line.json_string(&self.label)?;
        line.push(",\"level\":")?;
        line.json_string(self.level.name())?;
        line.push(",\"message\":")?;
        line.json_string(&self.message)?;
        line.push(",\"timestamp\":")?;
        line.json_string(&self.timestamp)?;
        line.push("}")
    }

    /// RFC 5424: `<PRI>1 TIMESTAMP HOSTNAME APP-NAME PROCID MSGID SD MSG`,
    /// facility `user`, no `MSGID` and no structured data; the fields
    /// follow the message as `key=value`.
    fn syslog(&self, line: &mut Line) -> Result<(), &'static str> {
        let nil = catalog::text("syslog.nil");
        let priority = SYSLOG_FACILITY_USER * 8 + self.level.syslog_severity();
        line.push(&format!("<{priority}>1 "))?;
        let valid_time = bn_core_text::temporal::parse_rfc3339(&self.timestamp).is_some();
        line.push(if valid_time { &self.timestamp } else { nil })?;
        line.push(" ")?;
        line.push(syslog_header(&self.host, 255).unwrap_or(nil))?;
        line.push(" ")?;
        line.push(syslog_header(&self.label, 48).unwrap_or(nil))?;
        line.push(" ")?;
        line.push(&self.process.to_string())?;
        line.push(" ")?;
        line.push(nil)?;
        line.push(" ")?;
        line.push(nil)?;
        line.push(" ")?;
        line.plain(&self.message)?;
        for (key, value) in self.written_fields() {
            line.push(" ")?;
            line.plain(key)?;
            line.push("=")?;
            line.plain(value)?;
        }
        Ok(())
    }

    fn apache(&self, line: &mut Line) -> Result<(), &'static str> {
        let absent = catalog::text("apache.absent");
        let field = |key: &str| {
            self.fields
                .get(catalog::text(key))
                .map(String::as_str)
                .filter(|value| !value.is_empty())
        };
        let request = [
            field("apache.method").map(str::to_owned),
            field("apache.path").map(str::to_owned),
            field("apache.protocol").map(str::to_owned),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
        line.plain(field("apache.remote").unwrap_or(absent))?;
        line.push(" ")?;
        line.push(absent)?;
        line.push(" ")?;
        line.push(absent)?;
        line.push(" [")?;
        line.push(&apache_time(&self.timestamp).unwrap_or_else(|| absent.to_owned()))?;
        line.push("] \"")?;
        line.quoted(if request.is_empty() { absent } else { &request })?;
        line.push("\" ")?;
        line.plain(field("apache.status").unwrap_or(absent))?;
        line.push(" ")?;
        line.plain(field("apache.bytes").unwrap_or(absent))?;
        line.push(" \"")?;
        line.quoted(field("apache.referrer").unwrap_or(absent))?;
        line.push("\" \"")?;
        line.quoted(field("apache.user_agent").unwrap_or(absent))?;
        line.push("\"")
    }
}

/// The format of every record a logger writes, named in `Logger.New`
/// (bnlog.md, "Formats").
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Format {
    #[default]
    Json,
    Syslog,
    Apache,
}

impl Format {
    /// The format whose name is exactly `name` (`"JSON"`, `"SYSLOG"`,
    /// `"APACHE"`).
    #[must_use]
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        [
            (Self::Json, "format.json"),
            (Self::Syslog, "format.syslog"),
            (Self::Apache, "format.apache"),
        ]
        .into_iter()
        .find(|(_, key)| catalog::text(key) == name)
        .map(|(format, _)| format)
    }
}

/// RFC 5424 facility 1, `user`.
const SYSLOG_FACILITY_USER: u8 = 1;

/// A SYSLOG header field: printable US-ASCII without spaces, at most `max`
/// bytes (RFC 5424 §6.2); otherwise the field is NILVALUE.
fn syslog_header(value: &str, max: usize) -> Option<&str> {
    (!value.is_empty()
        && value.len() <= max
        && value.bytes().all(|byte| (33..=126).contains(&byte)))
    .then_some(value)
}

/// A record line under construction: every append first checks that the
/// line stays within `MAX_RECORD_BYTES`.
#[derive(Default)]
struct Line {
    text: String,
}

impl Line {
    fn push(&mut self, part: &str) -> Result<(), &'static str> {
        if self.text.len() + part.len() > MAX_RECORD_BYTES {
            return Err(RECORD_TOO_LONG);
        }
        self.text.push_str(part);
        Ok(())
    }

    fn push_char(&mut self, character: char) -> Result<(), &'static str> {
        self.push(character.encode_utf8(&mut [0; 4]))
    }

    /// A JSON string (RFC 8259).
    fn json_string(&mut self, value: &str) -> Result<(), &'static str> {
        self.push("\"")?;
        for character in value.chars() {
            match character {
                '"' => self.push("\\\"")?,
                '\\' => self.push("\\\\")?,
                '\n' => self.push("\\n")?,
                '\r' => self.push("\\r")?,
                '\t' => self.push("\\t")?,
                character if character.is_control() => {
                    self.push(&format!("\\u{:04x}", u32::from(character)))?;
                }
                character => self.push_char(character)?,
            }
        }
        self.push("\"")
    }

    /// Text outside quotes: line breaks and tabs become a space, any other
    /// control character `?`, so a record stays one line.
    fn plain(&mut self, value: &str) -> Result<(), &'static str> {
        for character in value.chars() {
            match character {
                '\n' | '\r' | '\t' => self.push(" ")?,
                character if character.is_control() => self.push("?")?,
                character => self.push_char(character)?,
            }
        }
        Ok(())
    }

    /// Text inside an Apache quoted field: `"` and `\` escaped with `\`, a
    /// control character as `\xhh` per byte (Apache `mod_log_config`).
    fn quoted(&mut self, value: &str) -> Result<(), &'static str> {
        for character in value.chars() {
            match character {
                '"' => self.push("\\\"")?,
                '\\' => self.push("\\\\")?,
                character if character.is_control() => {
                    for byte in character.encode_utf8(&mut [0; 4]).bytes() {
                        self.push(&format!("\\x{byte:02x}"))?;
                    }
                }
                character => self.push_char(character)?,
            }
        }
        Ok(())
    }
}

/// `dd/Mon/yyyy:HH:MM:SS +0000` of an RFC 3339 record time.
fn apache_time(timestamp: &str) -> Option<String> {
    let (days, millis) =
        bn_core_text::civil::split(bn_core_text::temporal::parse_rfc3339(timestamp)?)?;
    let date = bn_core_text::civil::civil_from_days(days);
    let month = catalog::words("apache.month").nth(usize::try_from(date.month).ok()? - 1)?;
    let seconds = millis / 1000;
    Some(format!(
        "{:02}/{month}/{:04}:{:02}:{:02}:{:02} {}",
        date.day,
        date.year,
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60,
        catalog::text("apache.zone"),
    ))
}

/// Whether a field key is never written (bnlog.md, "Formats"): lower-cased
/// with `-` and `.` read as `_`, an exact sensitive name or one containing a
/// sensitive marker.
#[must_use]
pub(crate) fn is_sensitive(key: &str) -> bool {
    let normalized = key
        .chars()
        .map(|character| match character.to_ascii_lowercase() {
            '-' | '.' => '_',
            character => character,
        })
        .collect::<String>();
    catalog::words("sensitive.marker").any(|marker| normalized.contains(marker))
        || sensitive_segments(&normalized)
}

/// One segment, or several consecutive segments joined by `_`, is an exact
/// sensitive name (`request_body`, `passwd_hash`).
fn sensitive_segments(normalized: &str) -> bool {
    let segments = normalized
        .split('_')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    let names = catalog::words("sensitive.exact").collect::<Vec<_>>();
    for start in 0..segments.len() {
        let mut joined = String::new();
        for (offset, segment) in segments[start..].iter().enumerate() {
            if offset != 0 {
                joined.push('_');
            }
            joined.push_str(segment);
            if names.iter().any(|name| *name == joined) {
                return true;
            }
        }
    }
    false
}

/// Each space-separated part of `value` without its query (`?…`).
fn without_query(value: &str) -> String {
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
        assert_eq!(
            record.line(Format::Json).unwrap(),
            "{\"fields\":{\"route\":\"/users\",\"service\":\"api\"},\"label\":\"web\",\"level\":\"INFO\",\"message\":\"hit\",\"timestamp\":\"2026-10-08T12:00:00.000Z\"}"
        );
    }

    fn record(label: &str, message: &str, fields: &[(&str, &str)]) -> Record {
        Record {
            timestamp: "2026-10-08T09:05:03.250Z".into(),
            label: label.into(),
            level: Level::Http,
            message: message.into(),
            fields: fields
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
            host: "web01".into(),
            process: 4242,
        }
    }

    #[test]
    fn no_format_writes_a_url_query() {
        let record = record(
            "web",
            "m",
            &[
                ("http.path", "/a?token=1"),
                ("http.referrer", "https://x/?q=2"),
            ],
        );
        for format in [Format::Json, Format::Syslog, Format::Apache] {
            let line = record.line(format).unwrap();
            assert!(!line.contains("token") && !line.contains("q=2"), "{line}");
        }
    }

    #[test]
    fn format_names_are_exact() {
        assert_eq!(Format::from_name("JSON"), Some(Format::Json));
        assert_eq!(Format::from_name("SYSLOG"), Some(Format::Syslog));
        assert_eq!(Format::from_name("TEXT"), None);
        assert_eq!(Format::from_name("APACHE"), Some(Format::Apache));
        assert_eq!(Format::from_name("json"), None);
        assert_eq!(Format::from_name(""), None);
    }

    /// No format ends with a line terminator, and a value cannot start a
    /// second line: the transport writes the one line feed.
    #[test]
    fn every_format_is_one_line_without_terminator() {
        let record = record("a\nb", "x\ry", &[("k\n", "v\u{1b}[31m")]);
        for format in [Format::Json, Format::Syslog, Format::Apache] {
            let line = record.line(format).unwrap();
            assert!(
                !line.contains('\n') && !line.contains('\r'),
                "{format:?}: {line}"
            );
            assert!(!line.contains('\u{1b}'), "{format:?}: {line}");
        }
        assert_eq!(
            record.line(Format::Syslog).unwrap(),
            "<14>1 2026-10-08T09:05:03.250Z web01 - 4242 - - x y k =v?[31m"
        );
    }

    /// RFC 5424 header: PRI from facility `user` and the level's severity,
    /// version 1, NILVALUE for a header field that is not printable ASCII.
    #[test]
    fn syslog_follows_rfc_5424() {
        let mut error = record("app", "disk full", &[("disk", "/dev/sda")]);
        error.level = Level::Error;
        assert_eq!(
            error.line(Format::Syslog).unwrap(),
            "<11>1 2026-10-08T09:05:03.250Z web01 app 4242 - - disk full disk=/dev/sda"
        );
        for (level, priority) in [
            (Level::Warn, 12),
            (Level::Info, 14),
            (Level::Http, 14),
            (Level::Verbose, 15),
            (Level::Debug, 15),
            (Level::Silly, 15),
        ] {
            let mut record = record("app", "m", &[]);
            record.level = level;
            let line = record.line(Format::Syslog).unwrap();
            assert!(line.starts_with(&format!("<{priority}>1 ")), "{line}");
        }
        let mut odd = record("my app", "m", &[]);
        odd.host = String::new();
        odd.timestamp = "now".into();
        assert_eq!(odd.line(Format::Syslog).unwrap(), "<14>1 - - - 4242 - - m");
    }

    #[test]
    fn json_escapes_strings() {
        let line = record("l", "say \"hi\" \\ \u{1}", &[])
            .line(Format::Json)
            .unwrap();
        assert!(
            line.contains(r#""message":"say \"hi\" \\ \u0001""#),
            "{line}"
        );
    }

    #[test]
    fn apache_reads_the_http_fields_and_escapes_quoted_text() {
        let line = record(
            "web",
            "ignored",
            &[
                ("http.remote", "203.0.113.7"),
                ("http.method", "GET"),
                ("http.path", "/a?token=1"),
                ("http.protocol", "HTTP/1.1"),
                ("http.status", "200"),
                ("http.bytes_sent", "512"),
                ("http.referrer", "https://x/?q=2"),
                ("http.user_agent", "agent \"7\" \\ \u{7}"),
            ],
        )
        .line(Format::Apache)
        .unwrap();
        assert_eq!(
            line,
            r#"203.0.113.7 - - [08/Oct/2026:09:05:03 +0000] "GET /a HTTP/1.1" 200 512 "https://x/" "agent \"7\" \\ \x07""#
        );
        let empty = record("web", "m", &[]).line(Format::Apache).unwrap();
        assert_eq!(
            empty,
            r#"- - - [08/Oct/2026:09:05:03 +0000] "-" - - "-" "-""#
        );
    }

    #[test]
    fn sensitive_keys_are_never_written() {
        for key in [
            "Authorization",
            "set-cookie",
            "refresh.token",
            "query",
            "tls_key",
            "request_body",
            "passwd_hash",
            "token_count",
        ] {
            assert!(is_sensitive(key), "{key}");
        }
        for key in ["route", "http.status"] {
            assert!(!is_sensitive(key), "{key}");
        }
        let line = record("l", "m", &[("api-key", "s3cret"), ("route", "/")])
            .line(Format::Syslog)
            .unwrap();
        assert!(!line.contains("s3cret"), "{line}");
    }

    /// The limit is checked while building: a record past it fails, one at
    /// it succeeds, and escapes count.
    #[test]
    fn the_limit_counts_the_line_as_built() {
        let fixed = record("l", "", &[]).line(Format::Syslog).unwrap().len();
        let fits = record("l", &"m".repeat(MAX_RECORD_BYTES - fixed), &[]);
        assert_eq!(fits.line(Format::Syslog).unwrap().len(), MAX_RECORD_BYTES);
        let over = record("l", &"m".repeat(MAX_RECORD_BYTES - fixed + 1), &[]);
        assert_eq!(over.line(Format::Syslog), Err(RECORD_TOO_LONG));
        let quotes = record("l", &"\"".repeat(MAX_RECORD_BYTES / 2), &[]);
        assert_eq!(quotes.line(Format::Json), Err(RECORD_TOO_LONG));
    }
}
