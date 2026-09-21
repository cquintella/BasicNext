//! Structured companion process logs shared by Basic Next command-line tools.

use std::{fs, io, path::Path};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl LogLevel {
    /// Parses a configured process-log level.
    ///
    /// # Errors
    ///
    /// Returns an error when `value` is not an accepted level spelling.
    pub fn parse(value: &str) -> Result<Self, String> {
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

    const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
        }
    }
}

#[derive(Debug)]
pub struct ProcessLog {
    level: LogLevel,
    lines: Vec<String>,
    warning_count: usize,
}

impl ProcessLog {
    #[must_use]
    pub const fn new(level: LogLevel) -> Self {
        Self {
            level,
            lines: Vec::new(),
            warning_count: 0,
        }
    }

    pub fn record_warning(&mut self) {
        self.warning_count = self.warning_count.saturating_add(1);
    }

    #[must_use]
    pub const fn warning_count(&self) -> usize {
        self.warning_count
    }

    pub fn event(&mut self, level: LogLevel, phase: &str, event: &str, detail: impl AsRef<str>) {
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

    /// Writes the filtered events to a companion log file.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the parent directory cannot be created or
    /// the log file cannot be written.
    pub fn write_to(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(
            path,
            self.lines.join("\n") + if self.lines.is_empty() { "" } else { "\n" },
        )
    }

    /// Writes the log to `path` when one is configured, reporting a failure
    /// on stderr. Returns `true` when the write failed.
    #[must_use]
    pub fn finish(&self, path: Option<&Path>) -> bool {
        let Some(path) = path else {
            return false;
        };
        if let Err(error) = self.write_to(path) {
            eprintln!(
                "error[PROCESS_LOG_WRITE]: cannot write process log {}: {error}",
                path.display()
            );
            return true;
        }
        false
    }

    /// Mirrors every frontend warning as a `diagnostic emit` event.
    pub fn mirror_frontend_diagnostics(&mut self, frontend: &bn_frontend::prepare::Prepared) {
        for diagnostic in &frontend.warnings {
            self.record_warning();
            self.event(
                LogLevel::Warn,
                "diagnostic",
                "emit",
                format!(
                    "code={} source={} line={} column={}",
                    diagnostic.diagnostic.code,
                    diagnostic.module.0,
                    diagnostic.diagnostic.span.start.line,
                    diagnostic.diagnostic.span.start.column
                ),
            );
        }
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
