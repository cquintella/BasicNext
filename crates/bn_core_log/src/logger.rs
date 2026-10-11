// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The `BNLog.Logger` state and the write of one record, shared by `bni`
//! (`bn_lib_log`), `bnc` (`bn_rt`) and the `BNWeb` access log (`bn_core_web`).
//! What differs per caller (clock, console, how a file is opened under the
//! host policy) comes through a [`LogSink`].

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::Path;

use crate::{FileTransport, Format, Level, LogFailure, Record, dispatch_log};

/// At most this many transports per logger (`bnlog.md`).
pub const MAX_TRANSPORTS: usize = 8;

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
    /// Opens `path` for appending, under the host's filesystem policy.
    ///
    /// # Errors
    ///
    /// The policy denial or the open error.
    fn open_append(&self, path: &Path) -> io::Result<Box<dyn Write>>;
}

/// One `BNLog.Logger`: label, context, transports and format.
#[derive(Clone, Debug, Default)]
pub struct Logger {
    pub label: String,
    pub context: BTreeMap<String, String>,
    pub null_transports: Vec<Level>,
    pub console_transports: Vec<Level>,
    pub file_transports: Vec<FileTransport>,
    pub format: Format,
    pub closed: bool,
}

impl Logger {
    /// A logger labelled `label`, with no transport.
    #[must_use]
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_owned(),
            ..Self::default()
        }
    }

    /// A child sharing this logger's transports and format, with `context`
    /// added to (and overriding) the parent's.
    #[must_use]
    pub fn child(&self, context: impl IntoIterator<Item = (String, String)>) -> Self {
        let mut child = self.clone();
        child.context.extend(context);
        child.closed = false;
        child
    }

    /// The number of transports of every kind.
    #[must_use]
    pub fn transport_count(&self) -> usize {
        self.null_transports.len() + self.console_transports.len() + self.file_transports.len()
    }

    /// Writes one record to every transport whose minimum level admits
    /// `level`; every transport is attempted and the first failure returned.
    ///
    /// # Errors
    ///
    /// `Closed` after `Close`, `RecordSerialization` for a record past the
    /// size bound, `IoFailed` for a console or file failure.
    pub fn write<'a>(
        &self,
        sink: &dyn LogSink,
        level: Level,
        message: &str,
        provided: impl IntoIterator<Item = (&'a String, &'a String)>,
    ) -> Result<(), LogFailure> {
        if self.closed {
            return Err(LogFailure::Closed);
        }
        let record = Record::with_timestamp(
            sink.timestamp(),
            &self.label,
            level,
            message,
            &self.context,
            provided,
        );
        let line = record
            .line(self.format)
            .map_err(|error| LogFailure::RecordSerialization(error.to_string()))?;
        dispatch_log(
            &line,
            level,
            &self.console_transports,
            &self.file_transports,
            |text| sink.console(text),
            &|path| sink.open_append(path),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        fn open_append(&self, _: &Path) -> io::Result<Box<dyn Write>> {
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        }
    }

    #[test]
    fn a_record_reaches_the_transports_its_level_admits() {
        let sink = Capture {
            lines: RefCell::new(Vec::new()),
        };
        let mut logger = Logger::new("web");
        logger.console_transports.push(Level::Http);
        let fields = BTreeMap::from([("http.status".to_owned(), "200".to_owned())]);
        logger
            .write(&sink, Level::Http, "web dispatch", &fields)
            .unwrap();
        logger
            .write(&sink, Level::Debug, "too verbose", &fields)
            .unwrap();
        {
            let lines = sink.lines.borrow();
            assert_eq!(lines.len(), 1);
            assert!(lines[0].contains("web dispatch") && lines[0].contains("200"));
        }

        logger.file_transports.push(FileTransport {
            path: "denied.log".into(),
            minimum: Level::Silly,
        });
        assert!(matches!(
            logger.write(&sink, Level::Info, "x", &fields),
            Err(LogFailure::IoFailed(_))
        ));
        logger.closed = true;
        assert_eq!(
            logger.write(&sink, Level::Info, "x", &fields),
            Err(LogFailure::Closed)
        );
    }

    #[test]
    fn a_child_adds_context_and_keeps_the_transports() {
        let mut parent = Logger::new("app");
        parent.console_transports.push(Level::Info);
        parent.context.insert("service".into(), "users".into());
        parent.closed = true;
        let child = parent.child([("request_id".to_owned(), "r1".to_owned())]);
        assert_eq!(child.label, "app");
        assert_eq!(child.console_transports, [Level::Info]);
        assert_eq!(child.context.len(), 2);
        assert!(!child.closed);
        assert_eq!(child.transport_count(), 1);
    }
}
