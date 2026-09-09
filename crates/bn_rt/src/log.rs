use std::collections::BTreeMap;

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
pub struct Record {
    pub timestamp: String,
    pub label: String,
    pub level: Level,
    pub message: String,
    pub fields: BTreeMap<String, String>,
}

impl Record {
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

fn is_sensitive(key: &str) -> bool {
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
    use super::{Level, Record};
    use std::collections::BTreeMap;

    #[test]
    fn json_redacts_sensitive_fields_and_escapes_controls() {
        let record = Record {
            timestamp: "now".into(),
            label: "app".into(),
            level: Level::Info,
            message: "hello\nworld".into(),
            fields: BTreeMap::from([
                ("user".into(), "ana".into()),
                ("authorization".into(), "secret".into()),
            ]),
        };
        let json = record.json_line().unwrap();
        assert!(json.contains("hello\\nworld"));
        assert!(json.contains("\"user\":\"ana\""));
        assert!(!json.contains("secret"));
    }
}
