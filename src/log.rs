#![allow(dead_code)] // ponytail: formatter core lands before host transports wire it.

use std::collections::BTreeMap;

use crate::json::Value;

const MAX_RECORD_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Level {
    Error = 0,
    Warn = 1,
    Info = 2,
    Http = 3,
    Verbose = 4,
    Debug = 5,
    Silly = 6,
}

impl Level {
    pub(crate) fn from_i128(value: i128) -> Option<Self> {
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

    const fn name(self) -> &'static str {
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
pub(crate) struct Record {
    pub(crate) timestamp: String,
    pub(crate) label: String,
    pub(crate) level: Level,
    pub(crate) message: String,
    pub(crate) fields: BTreeMap<String, String>,
}

impl Record {
    pub(crate) fn json_line(&self) -> Result<String, &'static str> {
        let mut object = BTreeMap::new();
        object.insert("timestamp".into(), Value::String(self.timestamp.clone()));
        object.insert("label".into(), Value::String(self.label.clone()));
        object.insert("level".into(), Value::String(self.level.name().into()));
        object.insert("message".into(), Value::String(self.message.clone()));
        object.insert(
            "fields".into(),
            Value::Object(
                self.fields
                    .iter()
                    .filter(|(k, _)| !is_sensitive(k))
                    .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                    .collect(),
            ),
        );
        bounded(crate::json::stringify(&Value::Object(object))?)
    }

    pub(crate) fn text_line(&self) -> Result<String, &'static str> {
        let fields = self
            .fields
            .iter()
            .filter(|(k, _)| !is_sensitive(k))
            .map(|(k, v)| format!("{}={}", escape(k), escape(v)))
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

    pub(crate) fn apache_combined(&self) -> Result<String, &'static str> {
        let field = |name: &str| self.fields.get(name).map_or("-", String::as_str);
        bounded(format!(
            "{} - - [{}] \"{}\" {} {} \"{}\" \"{}\"\n",
            escape(field("remote")),
            escape(&self.timestamp),
            escape(field("request")),
            escape(field("status")),
            escape(field("bytes_sent")),
            escape(field("referrer")),
            escape(field("user_agent"))
        ))
    }
}

fn bounded(value: String) -> Result<String, &'static str> {
    (value.len() <= MAX_RECORD_BYTES)
        .then_some(value)
        .ok_or("serialized log record exceeds 64 KiB")
}

fn escape(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            '\n' | '\r' | '\t' => ' ',
            c if c.is_control() => '?',
            c => c,
        })
        .collect()
}

fn is_sensitive(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "authorization" | "cookie" | "set-cookie" | "session" | "session_id" | "query" | "body"
    )
}

#[cfg(test)]
mod tests {
    use super::{Level, Record};
    fn record() -> Record {
        Record {
            timestamp: "2026-08-30T12:00:00Z".into(),
            label: "app".into(),
            level: Level::Info,
            message: "hello\nworld".into(),
            fields: [
                ("user".into(), "ana\t1".into()),
                ("authorization".into(), "secret".into()),
            ]
            .into_iter()
            .collect(),
        }
    }
    #[test]
    fn formats_escape_controls_and_exclude_sensitive_fields() {
        let record = record();
        let json = record.json_line().unwrap();
        assert!(json.contains("hello\\nworld"));
        assert!(!json.contains("secret"));
        let text = record.text_line().unwrap();
        assert!(text.contains("hello world"));
        assert!(text.contains("user=ana 1"));
    }
    #[test]
    fn apache_format_escapes_controls() {
        let mut record = record();
        record.fields.insert("remote".into(), "127.0.0.1".into());
        record
            .fields
            .insert("request".into(), "GET / HTTP/1.1".into());
        record.fields.insert("status".into(), "200".into());
        record.fields.insert("bytes_sent".into(), "5".into());
        let line = record.apache_combined().unwrap();
        assert!(line.contains("GET / HTTP/1.1"));
        assert!(!line.trim_end_matches('\n').contains('\n'));
    }

    #[test]
    fn apache_format_rejects_oversized_records() {
        let mut record = record();
        record.fields.insert("remote".into(), "127.0.0.1".into());
        record
            .fields
            .insert("request".into(), "GET / HTTP/1.1".into());
        record.fields.insert("status".into(), "200".into());
        record
            .fields
            .insert("user_agent".into(), "x".repeat(64 * 1024));
        assert!(record.apache_combined().is_err());
    }
    #[test]
    fn levels_are_fixed_and_bounded() {
        assert_eq!(Level::from_i128(0), Some(Level::Error));
        assert_eq!(Level::from_i128(7), None);
    }
}
