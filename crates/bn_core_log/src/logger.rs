// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The `BNLog.Logger` state and its operations (bnlog.md, "API (0.6.5)"),
//! shared by `bni` (`bn_lib_log`), `bnc` (`bn_rt`) and the `BNWeb` access
//! log. What differs per caller (clock, console, how a file is opened under
//! the host policy) comes through a [`LogSink`].

use std::io::{self, Write};
use std::path::Path;

use crate::catalog;
use crate::fields::{FieldMap, check_values};
use crate::{Format, Level, LogFailure, Record};

/// Bytes of a logger label: at least 1, at most this many (UTF-8).
pub const LABEL_MAX_BYTES: usize = 128;
/// The bounds of a `Flush` or `Close` timeout, in milliseconds.
pub const TIMEOUT_MIN_MS: i128 = 1;
pub const TIMEOUT_MAX_MS: i128 = 60_000;

/// What a logger needs from its host to write a record.
pub trait LogSink {
    /// The record's RFC 3339 timestamp.
    fn timestamp(&self) -> String;
    /// Writes one line (without its newline) to the console.
    ///
    /// # Errors
    ///
    /// The console's write error.
    fn console(&self, line: &str) -> io::Result<()>;
    /// Writes what the console buffers.
    ///
    /// # Errors
    ///
    /// The console's flush error.
    fn flush_console(&self) -> io::Result<()>;
    /// Opens `path` for appending, under the host's filesystem policy.
    ///
    /// # Errors
    ///
    /// The policy denial or the open error.
    fn open_append(&self, path: &Path) -> io::Result<Box<dyn Write>>;
}

/// The capabilities the program imported (programs import every capability
/// they use; bnlog.md, "Errors").
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Imports {
    pub console: bool,
    pub filesystem: bool,
}

/// Where a logger writes (bnlog.md, "Destinations").
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Destination {
    Console,
    Null,
    File(String),
}

impl Destination {
    /// The destination `text` names: `"CONSOLE"`, `"NULL"`, or a file path.
    ///
    /// # Errors
    ///
    /// `InvalidString` for an empty text.
    pub fn from_text(text: &str) -> Result<Self, LogFailure> {
        if text.is_empty() {
            return Err(LogFailure::InvalidString {
                what: catalog::text("what.destination"),
                len: 0,
                max: usize::MAX,
            });
        }
        Ok(if text == catalog::text("destination.console") {
            Self::Console
        } else if text == catalog::text("destination.null") {
            Self::Null
        } else {
            Self::File(text.to_owned())
        })
    }
}

/// A `Logger` operation other than the level methods (`Level::operation`):
/// its contract name (`Error.Operation`), the one both adapters report.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operation {
    New,
    Access,
    SetLevel,
    SetDefaults,
    SetAccessDefaults,
    Flush,
    Close,
}

impl Operation {
    /// Every operation, for an adapter that dispatches by name.
    pub const ALL: [Self; 7] = [
        Self::New,
        Self::Access,
        Self::SetLevel,
        Self::SetDefaults,
        Self::SetAccessDefaults,
        Self::Flush,
        Self::Close,
    ];

    /// The contract name (`BNLog.Logger.Close`).
    #[must_use]
    pub fn name(self) -> &'static str {
        catalog::text(match self {
            Self::New => "op.logger.new",
            Self::Access => "op.logger.access",
            Self::SetLevel => "op.logger.set_level",
            Self::SetDefaults => "op.logger.set_defaults",
            Self::SetAccessDefaults => "op.logger.set_access_defaults",
            Self::Flush => "op.logger.flush",
            Self::Close => "op.logger.close",
        })
    }

    /// The operation named `name`, if any.
    #[must_use]
    pub fn of_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|operation| operation.name() == name)
    }
}

/// One `BNLog.Logger`.
#[derive(Clone, Debug)]
pub struct Logger {
    pub label: String,
    pub format: Format,
    pub destination: Destination,
    pub minimum: Level,
    pub event_defaults: FieldMap,
    pub access_defaults: FieldMap,
    pub closed: bool,
}

impl Logger {
    /// `Logger.New(label, format, destination)`: a file destination is
    /// opened once, so a path that cannot be written fails here.
    ///
    /// # Errors
    ///
    /// `InvalidString` for a label out of bounds or an empty destination,
    /// `InvalidFormat` for an unknown format, `CapabilityRequired` for a
    /// destination whose capability the program did not import, `IoFailed`
    /// for a file that cannot be opened.
    pub fn open(
        label: &str,
        format: &str,
        destination: &str,
        imports: Imports,
        sink: &dyn LogSink,
    ) -> Result<Self, LogFailure> {
        if label.is_empty() || label.len() > LABEL_MAX_BYTES {
            return Err(LogFailure::InvalidString {
                what: catalog::text("what.label"),
                len: label.len(),
                max: LABEL_MAX_BYTES,
            });
        }
        let format = Format::from_name(format)
            .ok_or_else(|| LogFailure::InvalidFormat(format.to_owned()))?;
        let destination = Destination::from_text(destination)?;
        match &destination {
            Destination::Console if !imports.console => {
                return Err(LogFailure::CapabilityRequired(catalog::text(
                    "capability.console",
                )));
            }
            Destination::File(_) if !imports.filesystem => {
                return Err(LogFailure::CapabilityRequired(catalog::text(
                    "capability.filesystem",
                )));
            }
            Destination::File(path) => {
                sink.open_append(Path::new(path))
                    .map_err(|error| LogFailure::IoFailed(error.to_string()))?;
            }
            _ => {}
        }
        Ok(Self {
            label: label.to_owned(),
            format,
            destination,
            // An APACHE logger writes access records (`HTTP`) only.
            minimum: if format == Format::Apache {
                Level::Http
            } else {
                Level::Info
            },
            event_defaults: FieldMap::new(),
            access_defaults: FieldMap::new(),
            closed: false,
        })
    }

    /// `Error` … `Silly`: one record at `level` with the event's set fields
    /// over the event defaults.
    ///
    /// # Errors
    ///
    /// `Closed`; `InvalidArgument` on an APACHE logger or for a field value
    /// past its bound; `RecordSerialization` past the line bound; `IoFailed`.
    pub fn write_event(
        &self,
        sink: &dyn LogSink,
        level: Level,
        message: &str,
        event: &FieldMap,
    ) -> Result<(), LogFailure> {
        self.check_open()?;
        if self.format == Format::Apache {
            return Err(LogFailure::ApacheLevel);
        }
        self.write(sink, level, message, &self.event_defaults, event)
    }

    /// `Access`: one record at `HTTP`, with no message, of the request's set
    /// fields over the access defaults.
    ///
    /// # Errors
    ///
    /// As `write_event`, except that every format accepts it.
    pub fn write_access(&self, sink: &dyn LogSink, request: &FieldMap) -> Result<(), LogFailure> {
        self.check_open()?;
        self.write(sink, Level::Http, "", &self.access_defaults, request)
    }

    /// `SetLevel`: the minimum severity written.
    ///
    /// # Errors
    ///
    /// `Closed`; `OutOfRange` for a value outside the levels.
    pub fn set_level(&mut self, minimum: i128) -> Result<(), LogFailure> {
        self.check_open()?;
        self.minimum = Level::from_i128(minimum).ok_or(LogFailure::OutOfRange {
            what: catalog::text("what.level"),
            value: minimum,
            min: Level::Error as i128,
            max: Level::Silly as i128,
        })?;
        Ok(())
    }

    /// `SetDefaults`: the event defaults become the set fields of `defaults`.
    ///
    /// # Errors
    ///
    /// `Closed`; `InvalidString` for a value past its bound.
    pub fn set_event_defaults(&mut self, defaults: FieldMap) -> Result<(), LogFailure> {
        self.check_open()?;
        check_values(&defaults)?;
        self.event_defaults = defaults;
        Ok(())
    }

    /// `SetAccessDefaults`: the access defaults become the set fields of
    /// `defaults`.
    ///
    /// # Errors
    ///
    /// As `set_event_defaults`.
    pub fn set_access_defaults(&mut self, defaults: FieldMap) -> Result<(), LogFailure> {
        self.check_open()?;
        check_values(&defaults)?;
        self.access_defaults = defaults;
        Ok(())
    }

    /// `Flush`: writes what the destination buffers.
    ///
    /// # Errors
    ///
    /// `OutOfRange` for the timeout, `Closed`, `IoFailed`.
    pub fn flush(&self, sink: &dyn LogSink, timeout_ms: i128) -> Result<(), LogFailure> {
        check_timeout(timeout_ms)?;
        self.check_open()?;
        self.flush_destination(sink)
    }

    /// `Close`: flushes, then refuses every later call but `Close`, which is
    /// idempotent.
    ///
    /// # Errors
    ///
    /// `OutOfRange` for the timeout, `IoFailed` from the flush.
    pub fn close(&mut self, sink: &dyn LogSink, timeout_ms: i128) -> Result<(), LogFailure> {
        check_timeout(timeout_ms)?;
        if self.closed {
            return Ok(());
        }
        let flushed = self.flush_destination(sink);
        self.closed = true;
        flushed
    }

    fn check_open(&self) -> Result<(), LogFailure> {
        if self.closed {
            Err(LogFailure::Closed)
        } else {
            Ok(())
        }
    }

    fn write(
        &self,
        sink: &dyn LogSink,
        level: Level,
        message: &str,
        defaults: &FieldMap,
        provided: &FieldMap,
    ) -> Result<(), LogFailure> {
        check_values(provided)?;
        if level > self.minimum || self.destination == Destination::Null {
            return Ok(());
        }
        let record = Record::with_timestamp(
            sink.timestamp(),
            &self.label,
            level,
            message,
            defaults,
            provided,
        );
        let line = record
            .line(self.format)
            .map_err(|error| LogFailure::RecordSerialization(error.to_string()))?;
        let written = match &self.destination {
            Destination::Console => sink.console(&line),
            Destination::File(path) => sink.open_append(Path::new(path)).and_then(|mut file| {
                file.write_all(line.as_bytes())?;
                file.write_all(b"\n")
            }),
            Destination::Null => Ok(()),
        };
        written.map_err(|error| LogFailure::IoFailed(error.to_string()))
    }

    fn flush_destination(&self, sink: &dyn LogSink) -> Result<(), LogFailure> {
        let flushed = match &self.destination {
            Destination::Console => sink.flush_console(),
            Destination::File(path) => sink
                .open_append(Path::new(path))
                .and_then(|mut file| file.flush()),
            Destination::Null => Ok(()),
        };
        flushed.map_err(|error| LogFailure::IoFailed(error.to_string()))
    }
}

fn check_timeout(timeout_ms: i128) -> Result<(), LogFailure> {
    if (TIMEOUT_MIN_MS..=TIMEOUT_MAX_MS).contains(&timeout_ms) {
        Ok(())
    } else {
        Err(LogFailure::OutOfRange {
            what: catalog::text("what.timeout"),
            value: timeout_ms,
            min: TIMEOUT_MIN_MS,
            max: TIMEOUT_MAX_MS,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::{Apache, Event};
    use bn_types::error_codes::log;
    use std::cell::RefCell;

    struct Capture {
        lines: RefCell<Vec<String>>,
    }

    impl LogSink for Capture {
        fn timestamp(&self) -> String {
            "2026-10-10T12:00:00.000Z".into()
        }
        fn console(&self, line: &str) -> io::Result<()> {
            self.lines.borrow_mut().push(line.to_owned());
            Ok(())
        }
        fn flush_console(&self) -> io::Result<()> {
            Ok(())
        }
        fn open_append(&self, _: &Path) -> io::Result<Box<dyn Write>> {
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        }
    }

    fn capture() -> Capture {
        Capture {
            lines: RefCell::new(Vec::new()),
        }
    }

    const BOTH: Imports = Imports {
        console: true,
        filesystem: true,
    };

    #[test]
    fn new_checks_label_format_destination_and_imports() {
        let sink = capture();
        let code = |result: Result<Logger, LogFailure>| result.unwrap_err().code();
        assert_eq!(
            code(Logger::open("", "JSON", "CONSOLE", BOTH, &sink)),
            log::INVALID_ARGUMENT
        );
        assert_eq!(
            code(Logger::open("a", "XML", "CONSOLE", BOTH, &sink)),
            log::INVALID_ARGUMENT
        );
        assert_eq!(
            code(Logger::open("a", "JSON", "", BOTH, &sink)),
            log::INVALID_ARGUMENT
        );
        let none = Imports::default();
        assert_eq!(
            code(Logger::open("a", "JSON", "CONSOLE", none, &sink)),
            log::CAPABILITY_REQUIRED
        );
        assert_eq!(
            code(Logger::open("a", "JSON", "app.log", none, &sink)),
            log::CAPABILITY_REQUIRED
        );
        assert_eq!(
            code(Logger::open("a", "JSON", "app.log", BOTH, &sink)),
            log::IO_FAILED
        );
        let null = Logger::open("a", "SYSLOG", "NULL", none, &sink).unwrap();
        assert_eq!(null.destination, Destination::Null);
        assert_eq!(null.minimum, Level::Info);
    }

    #[test]
    fn events_take_defaults_and_follow_the_minimum_level() {
        let sink = capture();
        let mut logger = Logger::open("app", "JSON", "CONSOLE", BOTH, &sink).unwrap();
        let service = Event {
            service: "users".into(),
            ..Event::default()
        };
        logger.set_event_defaults(service.fields()).unwrap();
        let event = Event {
            user: "ana".into(),
            ..Event::default()
        };
        logger
            .write_event(&sink, Level::Info, "in", &event.fields())
            .unwrap();
        logger
            .write_event(&sink, Level::Debug, "hidden", &event.fields())
            .unwrap();
        logger.set_level(Level::Debug as i128).unwrap();
        logger
            .write_event(&sink, Level::Debug, "shown", &event.fields())
            .unwrap();
        let lines = sink.lines.borrow();
        assert_eq!(lines.len(), 2);
        assert!(
            lines[0].contains("\"service\":\"users\"") && lines[0].contains("\"user\":\"ana\"")
        );
        assert!(lines[1].contains("shown"));
        assert_eq!(
            logger.set_level(7).unwrap_err().code(),
            log::INVALID_ARGUMENT
        );
    }

    #[test]
    fn an_apache_logger_writes_access_records_only() {
        let sink = capture();
        let logger = Logger::open("web", "APACHE", "CONSOLE", BOTH, &sink).unwrap();
        let refused = logger.write_event(&sink, Level::Info, "x", &FieldMap::new());
        assert_eq!(refused.unwrap_err().code(), log::INVALID_ARGUMENT);
        let request = Apache {
            method: "GET".into(),
            path: "/items".into(),
            status: 200,
            ..Apache::default()
        };
        logger.write_access(&sink, &request.fields()).unwrap();
        assert!(sink.lines.borrow()[0].contains("\"GET /items\" 200"));
    }

    #[test]
    fn close_is_idempotent_and_refuses_later_writes() {
        let sink = capture();
        let mut logger = Logger::open("app", "JSON", "CONSOLE", BOTH, &sink).unwrap();
        assert_eq!(
            logger.flush(&sink, 0).unwrap_err().code(),
            log::INVALID_ARGUMENT
        );
        logger.close(&sink, 100).unwrap();
        logger.close(&sink, 100).unwrap();
        let refused = logger.write_event(&sink, Level::Info, "x", &FieldMap::new());
        assert_eq!(refused.unwrap_err().code(), log::CLOSED);
        assert_eq!(logger.flush(&sink, 100).unwrap_err().code(), log::CLOSED);
    }
}
