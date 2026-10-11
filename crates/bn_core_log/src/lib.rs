// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNLog` core (R2): levels, record serialization, sensitive key redaction,
//! error representations and transport dispatch.

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::Path;

mod catalog;
pub mod error;
pub use error::LogFailure;
mod logger;
pub use logger::{LogSink, Logger, MAX_TRANSPORTS};

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
    pub fn from_i128(value: i128) -> Option<Self> {
        i64::try_from(value).ok().and_then(Self::from_i64)
    }

    /// The RFC 5424 severity: ERROR 3, WARN 4, INFO and HTTP 6, VERBOSE,
    /// DEBUG and SILLY 7.
    #[must_use]
    pub const fn syslog_severity(self) -> u8 {
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
        let mut line = Line::default();
        match format {
            Format::Json => self.json(&mut line)?,
            Format::Syslog => self.syslog(&mut line)?,
            Format::Apache => self.apache(&mut line)?,
        }
        Ok(line.text)
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
            field("apache.path").map(without_query),
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
        line.quoted(&field("apache.referrer").map_or_else(|| absent.to_owned(), without_query))?;
        line.push("\" \"")?;
        line.quoted(field("apache.user_agent").unwrap_or(absent))?;
        line.push("\"")
    }
}

/// The format of every record a logger writes, named by
/// `Logger.SetFormat` (bnlog.md, "Formats").
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
    pub fn from_name(name: &str) -> Option<Self> {
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
pub fn is_sensitive(key: &str) -> bool {
    let normalized = key
        .chars()
        .map(|character| match character.to_ascii_lowercase() {
            '-' | '.' => '_',
            character => character,
        })
        .collect::<String>();
    catalog::words("sensitive.exact").any(|name| normalized == name)
        || catalog::words("sensitive.marker").any(|marker| normalized.contains(marker))
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
        ] {
            assert!(is_sensitive(key), "{key}");
        }
        for key in ["route", "request_body", "passwd_hash", "http.status"] {
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
