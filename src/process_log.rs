// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::{fs, io, path::Path};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl LogLevel {
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "error" => Ok(Self::Error),
            "warn" | "warning" => Ok(Self::Warn),
            "info" => Ok(Self::Info),
            "debug" => Ok(Self::Debug),
            _ => Err(format!(
                "--log-level expects error, warn, info, or debug (got {value})"
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
        }
    }
}

#[derive(Debug)]
pub(crate) struct ProcessLog {
    level: LogLevel,
    lines: Vec<String>,
    warning_count: usize,
}

impl ProcessLog {
    pub(crate) fn new(level: LogLevel) -> Self {
        Self {
            level,
            lines: Vec::new(),
            warning_count: 0,
        }
    }

    pub(crate) fn record_warning(&mut self) {
        self.warning_count = self.warning_count.saturating_add(1);
    }

    pub(crate) fn warning_count(&self) -> usize {
        self.warning_count
    }

    pub(crate) fn event(
        &mut self,
        level: LogLevel,
        phase: &str,
        event: &str,
        detail: impl AsRef<str>,
    ) {
        if level > self.level {
            return;
        }
        self.lines.push(format!(
            "level={} phase={} event={} detail={}",
            level.as_str(),
            field(phase),
            field(event),
            field(&redact(detail.as_ref()))
        ));
    }

    pub(crate) fn write_to(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(
            path,
            self.lines.join("\n") + if self.lines.is_empty() { "" } else { "\n" },
        )
    }

    #[cfg(test)]
    fn lines(&self) -> &[String] {
        &self.lines
    }
}

fn field(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace(' ', "\\s")
}

fn redact(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if [
        "token=",
        "password=",
        "passwd=",
        "cookie=",
        "authorization=",
        "request_body=",
        "private_key=",
    ]
    .iter()
    .any(|key| lower.contains(key))
    {
        return "[REDACTED]".into();
    }
    value.into()
}

#[cfg(test)]
mod tests {
    use super::{LogLevel, ProcessLog};

    #[test]
    fn level_filtering_preserves_order_and_redacts_secrets() {
        let mut log = ProcessLog::new(LogLevel::Info);
        log.event(LogLevel::Debug, "compile", "argv", "token=secret");
        log.event(LogLevel::Info, "compile", "start", "line one\nline two");
        log.event(LogLevel::Error, "compile", "fail", "exit=1");
        assert_eq!(log.lines().len(), 2);
        assert!(log.lines()[0].contains("line\\sone\\nline\\stwo"));
        assert!(log.lines()[1].contains("event=fail"));
        assert!(!log.lines().iter().any(|line| line.contains("secret")));
    }

    #[test]
    fn writes_a_real_companion_file() {
        let directory = std::env::temp_dir().join(format!("bn-process-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        let path = directory.join("build.log");
        let mut log = ProcessLog::new(LogLevel::Debug);
        log.event(LogLevel::Info, "pipeline", "start", "target=native");
        log.write_to(&path).expect("write process log");
        let text = std::fs::read_to_string(&path).expect("read process log");
        assert!(text.contains("phase=pipeline"));
        assert!(text.ends_with('\n'));
        std::fs::remove_dir_all(directory).expect("remove process log directory");
    }
}
