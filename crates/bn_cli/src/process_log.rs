//! Structured companion process logs shared by Basic Next command-line tools.

use std::{
    fs,
    io::{self, Write as _},
    path::{Path, PathBuf},
};

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

    /// Writes the filtered events to `target`: a path the user named (its
    /// directories are created) or the companion beside the product (they
    /// exist). Either way the file is opened without following a symlink
    /// and must be a regular file (known issue K2).
    ///
    /// # Errors
    ///
    /// Returns an I/O error when a directory cannot be created, the path is
    /// a symlink or not a regular file, or the file cannot be written.
    pub fn write_to(&self, target: &LogTarget) -> io::Result<()> {
        if let LogTarget::Explicit(path) = target
            && let Some(parent) = path.parent()
        {
            fs::create_dir_all(parent)?;
        }
        let mut file = open_regular_file(target.path())?;
        file.write_all(
            (self.lines.join("\n") + if self.lines.is_empty() { "" } else { "\n" }).as_bytes(),
        )
    }

    /// Writes the log to `path` when one is configured, reporting a failure
    /// on stderr. Returns `true` when the write failed.
    #[must_use]
    pub fn finish(&self, target: Option<&LogTarget>) -> bool {
        let Some(target) = target else {
            return false;
        };
        if let Err(error) = self.write_to(target) {
            eprintln!(
                "{}",
                crate::diagnostics::tool_diagnostic(
                    bn_diag::DiagId::PROCESS_LOG_WRITE,
                    format!(
                        "cannot write process log {}: {error}",
                        target.path().display()
                    ),
                    bn_diag::Catalog::embedded_global(),
                )
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

/// Where the process log goes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LogTarget {
    /// `--log-file` or `[logging] file`: the user named it.
    Explicit(PathBuf),
    /// `<product>.bnbuild.log`, beside the product the build writes.
    Companion(PathBuf),
}

impl LogTarget {
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            Self::Explicit(path) | Self::Companion(path) => path,
        }
    }
}

/// The companion log of `product`: its file name with `.bnbuild.log`
/// appended, so it never equals the product and keeps the product's own
/// extension. `None` when `product` exists and is not a regular file
/// (`/dev/null`, a device, a FIFO, a directory): there is no product to sit
/// beside. A product not written yet (a failed build) still gets its log.
#[must_use]
pub fn companion_log(product: &Path) -> Option<PathBuf> {
    if fs::symlink_metadata(product).is_ok_and(|metadata| !metadata.file_type().is_file()) {
        return None;
    }
    let mut name = product.file_name()?.to_os_string();
    name.push(".bnbuild.log");
    Some(product.with_file_name(name))
}

/// Opens `path` for writing without following a symlink and without
/// blocking on a FIFO, accepts only a regular file, then truncates it.
fn open_regular_file(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        // FILE_FLAG_OPEN_REPARSE_POINT: a symlink is opened itself, never
        // its target; the regular-file check below then refuses it.
        options.custom_flags(0x0020_0000);
    }
    let file = options.open(path)?;
    if !file.metadata()?.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the log path is not a regular file",
        ));
    }
    file.set_len(0)?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::{LogLevel, LogTarget, Path, ProcessLog, companion_log};

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
        log.write_to(&LogTarget::Explicit(path.clone()))
            .expect("write process log");
        let text = std::fs::read_to_string(&path).expect("read process log");
        assert!(text.contains("phase=pipeline"));
        assert!(text.ends_with('\n'));
        std::fs::remove_dir_all(directory).expect("remove process log directory");
    }

    /// The companion appends `.bnbuild.log`: it never equals the product,
    /// not even a product named `*.log`, and keeps the product's extension.
    #[test]
    fn companion_log_appends_its_suffix() {
        let directory = std::env::temp_dir().join(format!("bn-companion-{}", std::process::id()));
        assert_eq!(
            companion_log(&directory.join("hello")),
            Some(directory.join("hello.bnbuild.log"))
        );
        assert_eq!(
            companion_log(&directory.join("result.log")),
            Some(directory.join("result.log.bnbuild.log"))
        );
        assert_eq!(
            companion_log(&directory.join("app.exe")),
            Some(directory.join("app.exe.bnbuild.log"))
        );
    }

    /// A product that is not a regular file (`/dev/null`, a directory) has no
    /// companion log: nothing is written beside it.
    #[test]
    fn special_products_have_no_companion_log() {
        assert_eq!(companion_log(&std::env::temp_dir()), None);
        #[cfg(unix)]
        assert_eq!(companion_log(Path::new("/dev/null")), None);
    }

    /// The log never follows a symlink and never blocks on a FIFO: a
    /// planted link cannot redirect the write to another file.
    #[cfg(unix)]
    #[test]
    fn the_log_refuses_symlinks_and_fifos() {
        let directory = std::env::temp_dir().join(format!("bn-log-links-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("create directory");
        let victim = directory.join("victim.txt");
        std::fs::write(&victim, "keep").expect("write victim");
        let link = directory.join("planted.bnbuild.log");
        std::os::unix::fs::symlink(&victim, &link).expect("plant symlink");
        let mut log = ProcessLog::new(LogLevel::Debug);
        log.event(LogLevel::Info, "pipeline", "start", "x");
        assert!(log.write_to(&LogTarget::Companion(link)).is_err());
        assert_eq!(
            std::fs::read_to_string(&victim).expect("read victim"),
            "keep"
        );

        let fifo = directory.join("fifo.bnbuild.log");
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("run mkfifo");
        assert!(status.success());
        assert!(log.write_to(&LogTarget::Companion(fifo)).is_err());
        std::fs::remove_dir_all(directory).expect("remove directory");
    }
}
