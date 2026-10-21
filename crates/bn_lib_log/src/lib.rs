// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNLog` — an external library module served through the provider seam
//! (bnlog.md, "API (0.6.5)"). Owns the logger table; the logger itself is
//! the shared core (`bn_core_log`). The console destination writes to the
//! program output through [`CoreContext::output`]; a file goes through the
//! host filesystem policy.

use std::collections::HashMap;

use bn_core_log::{Apache, Event, FieldMap, Imports, Level, LogFailure, Operation};
use bn_diag::Diagnostic;
use bn_source::Span;
use bn_value::{RecordValue, Value};

use bn_interp::provider::{CoreContext, Provider};
use bn_interp::{integer_pub as integer, runtime_error_pub as runtime_error, type_mismatch};

pub const NAME: &str = "BNLog";

/// The member the `BNWeb` provider writes its access records through: an
/// access record with fields beyond `Log.Apache` (`request_id`), as
/// alternating key and value texts. A `$` name cannot be written in BN.
pub(crate) const WEB_ACCESS_MEMBER: &str = "Logger.$access";

#[derive(Debug)]
pub struct LogProvider {
    loggers: HashMap<u64, std::sync::Arc<std::sync::Mutex<bn_core_log::Logger>>>,
    /// Bindings that hold each logger: the last one's end destroys it (K61).
    holders: HashMap<u64, usize>,
    next_logger: u64,
}

impl Default for LogProvider {
    fn default() -> Self {
        Self {
            loggers: HashMap::new(),
            holders: HashMap::new(),
            next_logger: 1,
        }
    }
}

impl Provider for LogProvider {
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn call(
        &mut self,
        core: &mut dyn CoreContext,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let operation = format!("{NAME}.{member}");
        if Operation::of_name(&operation) == Some(Operation::New) {
            return Ok(self.new_logger(core, &arguments));
        }
        let Some(Value::LogLogger(id)) = arguments.first() else {
            return Err(type_mismatch(
                "BNLog.Logger",
                "non-BNLog.Logger value",
                &operation,
                span,
            ));
        };
        let Some(logger) = self.loggers.get(id).cloned() else {
            return Err(invalid_logger(span));
        };
        let sink = CoreSink(std::cell::RefCell::new(core));
        let mut guard = logger.lock().map_err(|_| invalid_logger(span))?;
        let result = logger_call(&mut guard, &sink, &operation, member, &arguments, span)?;
        Ok(result.map_or_else(|failure| failed(&operation, &failure), |()| Value::Null))
    }

    fn allocate(&mut self, _class: &str, _span: Span) -> Option<Result<Value, Diagnostic>> {
        None
    }

    fn retain(&mut self, value: &Value, span: Span) -> Option<Result<(), Diagnostic>> {
        let Value::LogLogger(id) = value else {
            return None;
        };
        Some(match self.holders.get_mut(id) {
            Some(holders) => {
                *holders += 1;
                Ok(())
            }
            None => Err(invalid_logger(span)),
        })
    }

    /// One binding fewer holds the logger; the last one destroys it, which
    /// closes its destination (records are already written when received).
    fn release(&mut self, value: &Value, span: Span) -> Option<Result<(), Diagnostic>> {
        let Value::LogLogger(id) = value else {
            return None;
        };
        Some(if let Some(holders) = self.holders.get_mut(id) {
            *holders -= 1;
            if *holders == 0 {
                self.holders.remove(id);
                self.loggers.remove(id);
            }
            Ok(())
        } else {
            Err(runtime_error(
                bn_diag::DiagId::DOUBLE_RELEASE,
                bn_catalog::text("bnlog", "error.released.message"),
                span,
            ))
        })
    }
}

impl LogProvider {
    /// Hands the shared logger to another library (`BNWeb` access log).
    #[must_use]
    pub fn core_logger(
        &self,
        id: u64,
    ) -> Option<std::sync::Arc<std::sync::Mutex<bn_core_log::Logger>>> {
        self.loggers.get(&id).cloned()
    }

    /// `Logger.New(label, format, destination)`.
    fn new_logger(&mut self, core: &mut dyn CoreContext, arguments: &[Value]) -> Value {
        let operation = Operation::New.name();
        let text = |index: usize| match arguments.get(index) {
            Some(Value::String(text)) => text.to_string(),
            _ => String::new(),
        };
        let imports = Imports {
            console: core.module().console_import.is_some(),
            filesystem: core.module().filesystem_import.is_some(),
        };
        let sink = CoreSink(std::cell::RefCell::new(core));
        match bn_core_log::Logger::open(&text(0), &text(1), &text(2), imports, &sink) {
            Ok(logger) => {
                let id = self.next_logger;
                self.next_logger += 1;
                self.loggers
                    .insert(id, std::sync::Arc::new(std::sync::Mutex::new(logger)));
                self.holders.insert(id, 1);
                Value::LogLogger(id)
            }
            Err(failure) => failed(operation, &failure),
        }
    }
}

/// One call on an open logger; `Err` is a diagnostic that stops the program
/// (a wrong argument type the checker let through), `Ok(Err)` a BN `Error`.
fn logger_call(
    logger: &mut bn_core_log::Logger,
    sink: &CoreSink<'_>,
    operation: &str,
    member: &str,
    arguments: &[Value],
    span: Span,
) -> Result<Result<(), LogFailure>, Diagnostic> {
    if let Some(level) = Level::of_operation(operation) {
        let message = string_argument(arguments, 1, operation, span)?;
        let event = event_fields(record_argument(arguments, 2, operation, span)?);
        return Ok(logger.write_event(sink, level, &message, &event));
    }
    if member == WEB_ACCESS_MEMBER {
        let mut fields = FieldMap::new();
        for pair in arguments[1..].chunks(2) {
            if let [Value::String(key), Value::String(value)] = pair {
                fields.insert(key.to_string(), value.to_string());
            }
        }
        return Ok(logger.write_access(sink, &fields));
    }
    Ok(match Operation::of_name(operation) {
        Some(Operation::Access) => {
            let request = apache_fields(record_argument(arguments, 1, operation, span)?);
            logger.write_access(sink, &request)
        }
        Some(Operation::SetLevel) => logger.set_level(integer_argument(arguments, 1, span)?),
        Some(Operation::SetDefaults) => {
            let defaults = event_fields(record_argument(arguments, 1, operation, span)?);
            logger.set_event_defaults(defaults)
        }
        Some(Operation::SetAccessDefaults) => {
            let defaults = apache_fields(record_argument(arguments, 1, operation, span)?);
            logger.set_access_defaults(defaults)
        }
        Some(Operation::Flush) => logger.flush(sink, integer_argument(arguments, 1, span)?),
        Some(Operation::Close) => logger.close(sink, integer_argument(arguments, 1, span)?),
        Some(Operation::New) | None => Err(LogFailure::Unavailable(bn_catalog::text(
            "bnlog",
            "error.unavailable.cause",
        ))),
    })
}

fn string_argument(
    arguments: &[Value],
    index: usize,
    operation: &str,
    span: Span,
) -> Result<String, Diagnostic> {
    match arguments.get(index) {
        Some(Value::String(text)) => Ok(text.to_string()),
        _ => Err(type_mismatch("STRING", "non-STRING value", operation, span)),
    }
}

fn integer_argument(arguments: &[Value], index: usize, span: Span) -> Result<i128, Diagnostic> {
    let value = arguments.get(index).unwrap_or(&Value::Null);
    integer(value, span).map(|(value, _)| value)
}

fn record_argument<'a>(
    arguments: &'a [Value],
    index: usize,
    operation: &str,
    span: Span,
) -> Result<&'a RecordValue, Diagnostic> {
    match arguments.get(index) {
        Some(Value::Record { record }) => Ok(record),
        _ => Err(type_mismatch(
            "STRUCT",
            bn_catalog::text("bnlog", "context.non_struct"),
            operation,
            span,
        )),
    }
}

/// The text in `slot` of a record, or `""`.
fn slot_text(record: &RecordValue, slot: usize) -> String {
    match record.get(slot) {
        Some(Value::String(text)) => text.to_string(),
        _ => String::new(),
    }
}

/// The integer in `slot` of a record, or `0`.
fn slot_number(record: &RecordValue, slot: usize) -> i64 {
    match record.get(slot) {
        Some(Value::Integer(value, _)) => i64::try_from(*value).unwrap_or(0),
        _ => 0,
    }
}

/// The set fields of a `Log.Event` (slots in declaration order).
fn event_fields(record: &RecordValue) -> FieldMap {
    Event {
        service: slot_text(record, 0),
        request_id: slot_text(record, 1),
        route: slot_text(record, 2),
        user: slot_text(record, 3),
        detail: slot_text(record, 4),
        code: slot_number(record, 5),
    }
    .fields()
}

/// The set fields of a `Log.Apache` (slots in declaration order).
fn apache_fields(record: &RecordValue) -> FieldMap {
    Apache {
        remote: slot_text(record, 0),
        method: slot_text(record, 1),
        path: slot_text(record, 2),
        protocol: slot_text(record, 3),
        status: slot_number(record, 4),
        bytes: slot_number(record, 5),
        referrer: slot_text(record, 6),
        agent: slot_text(record, 7),
    }
    .fields()
}

/// `USE_AFTER_RELEASE` for a logger the program no longer holds.
fn invalid_logger(span: Span) -> Diagnostic {
    runtime_error(
        bn_diag::DiagId::USE_AFTER_RELEASE,
        "BNLog.Logger is invalid",
        span,
    )
}

/// The `Error` of a failed operation (`BNLog.Logger.Info`), with the report
/// the native ABI records for the same failure.
fn failed(operation: &str, failure: &LogFailure) -> Value {
    Value::error_report(
        failure.code(),
        operation,
        failure.message(),
        failure.cause(),
    )
}

/// Where a logger called by the program writes: the program output and the
/// host filesystem policy of the interpreter.
struct CoreSink<'a>(std::cell::RefCell<&'a mut dyn CoreContext>);

impl bn_core_log::LogSink for CoreSink<'_> {
    fn timestamp(&self) -> String {
        bn_rt::format_rfc3339(bn_rt::timestamp_ms())
    }
    fn console(&self, line: &str) -> std::io::Result<()> {
        writeln!(self.0.borrow_mut().output(), "{line}")
    }
    fn flush_console(&self) -> std::io::Result<()> {
        self.0.borrow_mut().output().flush()
    }
    fn open_append(&self, path: &std::path::Path) -> std::io::Result<Box<dyn std::io::Write>> {
        self.0
            .borrow()
            .host()
            .filesystem()
            .open(path, bn_rt::secure_fs::OpenMode::Append)
            .map(|file| Box::new(file) as Box<dyn std::io::Write>)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn module_constants_match_the_runtime_codes() {
        let module = include_str!("../../../modules/bn/BNLog.bn");
        for (name, value) in bn_types::error_codes::log::ALL {
            let line = format!("EXPORT CONST {name} AS INTEGER = {value}");
            assert!(module.contains(&line), "BNLog.bn lacks `{line}`");
        }
    }
}

#[cfg(test)]
mod lifetime_tests {
    use super::LogProvider;
    use bn_interp::provider::Provider;
    use bn_source::Span;
    use bn_value::Value;

    struct NoSink;

    impl bn_core_log::LogSink for NoSink {
        fn timestamp(&self) -> String {
            String::new()
        }
        fn console(&self, _: &str) -> std::io::Result<()> {
            Ok(())
        }
        fn flush_console(&self) -> std::io::Result<()> {
            Ok(())
        }
        fn open_append(&self, _: &std::path::Path) -> std::io::Result<Box<dyn std::io::Write>> {
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        }
    }

    /// A copy retains the logger, each binding's end releases one, and the
    /// last release destroys it (known issue K61).
    #[test]
    fn the_last_release_destroys_a_logger() {
        let mut provider = LogProvider::default();
        let logger = bn_core_log::Logger::open(
            "app",
            "JSON",
            "NULL",
            bn_core_log::Imports::default(),
            &NoSink,
        )
        .expect("null logger");
        provider
            .loggers
            .insert(1, std::sync::Arc::new(std::sync::Mutex::new(logger)));
        provider.holders.insert(1, 1);
        let value = Value::LogLogger(1);
        let position = bn_source::Position {
            source_id: bn_source::Position::UNKNOWN_SOURCE,
            revision: bn_source::Position::UNKNOWN_REVISION,
            offset: 0,
            line: 1,
            column: 1,
        };
        let span = Span {
            start: position,
            end: position,
        };
        assert!(
            provider
                .retain(&value, span)
                .is_some_and(|result| result.is_ok())
        );
        assert!(
            provider
                .release(&value, span)
                .is_some_and(|result| result.is_ok())
        );
        assert!(provider.loggers.contains_key(&1));
        assert!(
            provider
                .release(&value, span)
                .is_some_and(|result| result.is_ok())
        );
        assert!(!provider.loggers.contains_key(&1));
        assert!(
            provider
                .retain(&value, span)
                .is_some_and(|result| result.is_err())
        );
    }
}
