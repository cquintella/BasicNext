// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNLog` — an external library module served through the provider seam.
//! Owns the fields / entry / logger tables; console transports write to the
//! program output through [`CoreContext::output`], file transports go through
//! the host filesystem policy.

mod log;

use std::collections::HashMap;

use bn_diag::Diagnostic;
use bn_rt::log_error::LogFailure;
use bn_source::Span;
use bn_value::{Value, shared_string};

use bn_interp::provider::{CoreContext, Provider};
use bn_interp::{
    integer_from_i128_count_pub, integer_pub as integer, require_arity_pub as require_arity,
    runtime_error_pub as runtime_error, type_mismatch,
};

pub const NAME: &str = "BNLog";

#[derive(Clone)]
struct LogLoggerResource {
    label: String,
    context: std::collections::BTreeMap<String, String>,
    null_transports: Vec<i128>,
    console_transports: Vec<i128>,
    file_transports: Vec<LogFileTransport>,
    closed: bool,
}

#[derive(Clone)]
struct LogFileTransport {
    path: String,
    minimum: i128,
}

pub struct LogProvider {
    fields: HashMap<u64, HashMap<String, String>>,
    next_fields: u64,
    entries: HashMap<u64, HashMap<String, String>>,
    next_entry: u64,
    loggers: HashMap<u64, LogLoggerResource>,
    next_logger: u64,
}

impl Default for LogProvider {
    fn default() -> Self {
        Self {
            fields: HashMap::new(),
            next_fields: 1,
            entries: HashMap::new(),
            next_entry: 1,
            loggers: HashMap::new(),
            next_logger: 1,
        }
    }
}

impl Provider for LogProvider {
    fn call(
        &mut self,
        core: &mut dyn CoreContext,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let name = format!("BNLog.{member}");
        if member.contains("Fields.") {
            return self.log_fields_call(core, &name, &arguments, span);
        }
        if member.contains("Entry.") {
            return self.log_entry_call(core, &name, &arguments, span);
        }
        if member.contains("Logger.") {
            return self.log_logger_call(core, &name, &arguments, span);
        }
        if matches!(member.rsplit('.').next(), Some("CONSTRUCTOR" | "$fields")) {
            return Ok(Value::Null);
        }
        Ok(failed(
            &name,
            &LogFailure::Unavailable("BNLog provider unavailable"),
        ))
    }

    fn allocate(&mut self, class: &str, _span: Span) -> Option<Result<Value, Diagnostic>> {
        match class.rsplit('.').next()? {
            "Fields" => {
                let id = self.next_fields;
                self.next_fields += 1;
                self.fields.insert(id, HashMap::new());
                Some(Ok(Value::LogFields(id)))
            }
            "Entry" => {
                let id = self.next_entry;
                self.next_entry += 1;
                self.entries.insert(id, HashMap::new());
                Some(Ok(Value::LogEntry(id)))
            }
            "Logger" => {
                let id = self.next_logger;
                self.next_logger += 1;
                self.loggers.insert(
                    id,
                    LogLoggerResource {
                        label: String::new(),
                        context: std::collections::BTreeMap::new(),
                        null_transports: Vec::new(),
                        console_transports: Vec::new(),
                        file_transports: Vec::new(),
                        closed: false,
                    },
                );
                Some(Ok(Value::LogLogger(id)))
            }
            _ => None,
        }
    }

    fn release(&mut self, value: &Value, span: Span) -> Option<Result<(), Diagnostic>> {
        let (removed, what) = match value {
            Value::LogFields(id) => (self.fields.remove(id).is_some(), "BNLog.Fields"),
            Value::LogEntry(id) => (self.entries.remove(id).is_some(), "BNLog.Entry"),
            Value::LogLogger(id) => (self.loggers.remove(id).is_some(), "BNLog.Logger"),
            _ => return None,
        };
        Some(if removed {
            Ok(())
        } else {
            Err(runtime_error(
                bn_diag::DiagId::DOUBLE_RELEASE,
                format!("{what} was already deleted"),
                span,
            ))
        })
    }
}

#[allow(clippy::too_many_lines)] // One arm per BNLog member, as the library contract lists them.
impl LogProvider {
    fn log_fields_call(
        &mut self,
        _core: &mut dyn CoreContext,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = name.rsplit('.').next().unwrap_or_default();
        let Value::LogFields(id) = arguments.first().ok_or_else(|| {
            type_mismatch(
                "BNLog.Fields",
                "missing receiver",
                "BNLog.Fields operation",
                span,
            )
        })?
        else {
            return Err(type_mismatch(
                "BNLog.Fields",
                "non-BNLog.Fields value",
                "BNLog.Fields operation",
                span,
            ));
        };
        let fields = self.fields.get_mut(id).ok_or_else(|| {
            runtime_error(
                bn_diag::DiagId::USE_AFTER_RELEASE,
                "BNLog.Fields is invalid",
                span,
            )
        })?;
        match method {
            "CONSTRUCTOR" => Ok(Value::Null),
            "Count" => integer_from_i128_count_pub(fields.len() as i128, span),
            "SetString" | "SetInteger" | "SetBoolean" => {
                require_arity(name, arguments, 3, span)?;
                let Value::String(key) = &arguments[1] else {
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "BNLog.Fields key",
                        span,
                    ));
                };
                if key.is_empty() || key.len() > 128 {
                    return Ok(failed(
                        name,
                        &LogFailure::InvalidString {
                            what: "field key",
                            len: key.len(),
                            max: 128,
                        },
                    ));
                }
                if fields.contains_key(key.as_ref()) {
                    return Ok(failed(name, &LogFailure::DuplicateKey(key.to_string())));
                }
                if fields.len() >= 64 {
                    return Ok(failed(
                        name,
                        &LogFailure::LimitExceeded {
                            what: "fields",
                            count: fields.len(),
                            max: 64,
                        },
                    ));
                }
                let value = match method {
                    "SetString" => {
                        let Value::String(value) = &arguments[2] else {
                            return Err(type_mismatch(
                                "STRING",
                                "non-STRING value",
                                "BNLog.Fields.SetString value",
                                span,
                            ));
                        };
                        value.to_string()
                    }
                    "SetInteger" => integer(&arguments[2], span)?.0.to_string(),
                    "SetBoolean" => match arguments[2] {
                        Value::Boolean(value) => value.to_string().to_uppercase(),
                        _ => {
                            return Err(type_mismatch(
                                "BOOLEAN",
                                "non-BOOLEAN value",
                                "BNLog.Fields.SetBoolean value",
                                span,
                            ));
                        }
                    },
                    _ => unreachable!(),
                };
                fields.insert(key.to_string(), value);
                Ok(Value::Null)
            }
            "Get" => {
                require_arity(name, arguments, 2, span)?;
                let Value::String(key) = &arguments[1] else {
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "BNLog.Fields.Get key",
                        span,
                    ));
                };
                fields
                    .get(key.as_ref())
                    .cloned()
                    .map(|value| Value::String(shared_string(value)))
                    .ok_or_else(|| {
                        runtime_error(bn_diag::DiagId::NOT_FOUND, "field key was not found", span)
                    })
            }
            _ => Ok(failed(
                name,
                &LogFailure::Unavailable("BNLog.Fields operation unavailable"),
            )),
        }
    }

    fn log_entry_call(
        &mut self,
        _core: &mut dyn CoreContext,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = name.rsplit('.').next().unwrap_or_default();
        let Value::LogEntry(id) = arguments.first().ok_or_else(|| {
            type_mismatch(
                "BNLog.Entry",
                "missing receiver",
                "BNLog.Entry operation",
                span,
            )
        })?
        else {
            return Err(type_mismatch(
                "BNLog.Entry",
                "non-BNLog.Entry value",
                "BNLog.Entry operation",
                span,
            ));
        };
        let fields = self.entries.get(id).ok_or_else(|| {
            runtime_error(
                bn_diag::DiagId::USE_AFTER_RELEASE,
                "BNLog.Entry is invalid",
                span,
            )
        })?;
        match method {
            "CONSTRUCTOR" => Ok(Value::Null),
            "WithField" => {
                require_arity(name, arguments, 3, span)?;
                let Value::String(key) = &arguments[1] else {
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "BNLog.Entry.WithField key",
                        span,
                    ));
                };
                let Value::String(value) = &arguments[2] else {
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "BNLog.Entry.WithField value",
                        span,
                    ));
                };
                if key.is_empty() || key.len() > 128 {
                    return Ok(failed(
                        name,
                        &LogFailure::InvalidString {
                            what: "entry field key",
                            len: key.len(),
                            max: 128,
                        },
                    ));
                }
                if value.len() > 4096 {
                    return Ok(failed(
                        name,
                        &LogFailure::InvalidString {
                            what: "entry field value",
                            len: value.len(),
                            max: 4096,
                        },
                    ));
                }
                if fields.contains_key(key.as_ref()) {
                    return Ok(failed(name, &LogFailure::DuplicateKey(key.to_string())));
                }
                let mut next = fields.clone();
                if next.len() >= 64 {
                    return Ok(failed(
                        name,
                        &LogFailure::LimitExceeded {
                            what: "entry fields",
                            count: next.len(),
                            max: 64,
                        },
                    ));
                }
                next.insert(key.to_string(), value.to_string());
                let next_id = self.next_entry;
                self.next_entry += 1;
                self.entries.insert(next_id, next);
                Ok(Value::LogEntry(next_id))
            }
            _ => Ok(failed(
                name,
                &LogFailure::Unavailable("BNLog.Entry provider unavailable"),
            )),
        }
    }

    fn log_logger_call(
        &mut self,
        core: &mut dyn CoreContext,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = name.rsplit('.').next().unwrap_or_default();
        if method == "New" {
            require_arity(name, arguments, 1, span)?;
            let Value::String(label) = &arguments[0] else {
                return Err(type_mismatch(
                    "STRING",
                    "non-STRING value",
                    "BNLog.Logger.New label",
                    span,
                ));
            };
            if label.is_empty() || label.len() > 128 {
                return Ok(failed(
                    name,
                    &LogFailure::InvalidString {
                        what: "logger label",
                        len: label.len(),
                        max: 128,
                    },
                ));
            }
            let id = self.next_logger;
            self.next_logger += 1;
            self.loggers.insert(
                id,
                LogLoggerResource {
                    label: label.to_string(),
                    context: std::collections::BTreeMap::new(),
                    null_transports: Vec::new(),
                    console_transports: Vec::new(),
                    file_transports: Vec::new(),
                    closed: false,
                },
            );
            return Ok(Value::LogLogger(id));
        }
        let Value::LogLogger(id) = arguments.first().ok_or_else(|| {
            type_mismatch(
                "BNLog.Logger",
                "missing receiver",
                "BNLog.Logger operation",
                span,
            )
        })?
        else {
            return Err(type_mismatch(
                "BNLog.Logger",
                "non-BNLog.Logger value",
                "BNLog.Logger operation",
                span,
            ));
        };
        if method == "Child" {
            require_arity(name, arguments, 2, span)?;
            let Value::LogFields(fields_id) = arguments[1] else {
                return Err(type_mismatch(
                    "BNLog.Fields",
                    "non-BNLog.Fields value",
                    "BNLog.Logger.Child fields",
                    span,
                ));
            };
            if !self.fields.contains_key(&fields_id) {
                return Err(runtime_error(
                    bn_diag::DiagId::USE_AFTER_RELEASE,
                    "BNLog.Fields is invalid",
                    span,
                ));
            }
            let parent = self
                .loggers
                .get(id)
                .ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        "BNLog.Logger is invalid",
                        span,
                    )
                })?
                .clone();
            let mut context = parent.context;
            context.extend(
                self.fields
                    .get(&fields_id)
                    .ok_or_else(|| {
                        runtime_error(
                            bn_diag::DiagId::USE_AFTER_RELEASE,
                            "BNLog.Fields is invalid",
                            span,
                        )
                    })?
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone())),
            );
            let child_id = self.next_logger;
            self.next_logger += 1;
            self.loggers.insert(
                child_id,
                LogLoggerResource {
                    label: parent.label,
                    context,
                    null_transports: parent.null_transports,
                    console_transports: parent.console_transports,
                    file_transports: parent.file_transports,
                    closed: false,
                },
            );
            return Ok(Value::LogLogger(child_id));
        }
        let logger = self.loggers.get(id).cloned().ok_or_else(|| {
            runtime_error(
                bn_diag::DiagId::USE_AFTER_RELEASE,
                "BNLog.Logger is invalid",
                span,
            )
        })?;
        if method == "CONSTRUCTOR" {
            return Ok(Value::Null);
        }
        if logger.closed {
            return Ok(failed(name, &LogFailure::Closed));
        }
        match method {
            "AddNull" => {
                require_arity(name, arguments, 2, span)?;
                let minimum = integer(&arguments[1], span)?.0;
                if !(0..=6).contains(&minimum) {
                    return Ok(failed(
                        name,
                        &LogFailure::OutOfRange {
                            what: "log level",
                            value: minimum,
                            min: 0,
                            max: 6,
                        },
                    ));
                }
                if logger.null_transports.len()
                    + logger.console_transports.len()
                    + logger.file_transports.len()
                    >= 8
                {
                    return Ok(failed(
                        name,
                        &LogFailure::LimitExceeded {
                            what: "transports",
                            count: 8,
                            max: 8,
                        },
                    ));
                }
                self.loggers
                    .get_mut(id)
                    .expect("logger was checked above")
                    .null_transports
                    .push(minimum);
                Ok(Value::Null)
            }
            "AddConsole" => {
                require_arity(name, arguments, 2, span)?;
                if core.module().console_import.is_none() {
                    return Ok(failed(
                        name,
                        &LogFailure::CapabilityRequired("HOST.Console"),
                    ));
                }
                let minimum = integer(&arguments[1], span)?.0;
                if !(0..=6).contains(&minimum) {
                    return Ok(failed(
                        name,
                        &LogFailure::OutOfRange {
                            what: "log level",
                            value: minimum,
                            min: 0,
                            max: 6,
                        },
                    ));
                }
                if logger.null_transports.len()
                    + logger.console_transports.len()
                    + logger.file_transports.len()
                    >= 8
                {
                    return Ok(failed(
                        name,
                        &LogFailure::LimitExceeded {
                            what: "transports",
                            count: 8,
                            max: 8,
                        },
                    ));
                }
                self.loggers
                    .get_mut(id)
                    .expect("logger was checked above")
                    .console_transports
                    .push(minimum);
                Ok(Value::Null)
            }
            "AddFile" => {
                require_arity(name, arguments, 3, span)?;
                if core.module().filesystem_import.is_none()
                    || !core.host().filesystem().allows_capability()
                {
                    return Ok(failed(
                        name,
                        &LogFailure::CapabilityRequired("HOST.FileSystem"),
                    ));
                }
                let Value::String(path) = &arguments[1] else {
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "BNLog.Logger.AddFile path",
                        span,
                    ));
                };
                if !core
                    .host()
                    .filesystem()
                    .allows_path(std::path::Path::new(path.as_ref()), true)
                {
                    return Err(runtime_error(
                        bn_diag::DiagId::EXECUTION_POLICY_DENIED,
                        "logger file path is outside the execution policy",
                        span,
                    ));
                }
                let minimum = integer(&arguments[2], span)?.0;
                if path.is_empty() || path.len() > 4096 {
                    return Ok(failed(
                        name,
                        &LogFailure::InvalidString {
                            what: "file path",
                            len: path.len(),
                            max: 4096,
                        },
                    ));
                }
                if !(0..=6).contains(&minimum) {
                    return Ok(failed(
                        name,
                        &LogFailure::OutOfRange {
                            what: "log level",
                            value: minimum,
                            min: 0,
                            max: 6,
                        },
                    ));
                }
                if logger.null_transports.len()
                    + logger.console_transports.len()
                    + logger.file_transports.len()
                    >= 8
                {
                    return Ok(failed(
                        name,
                        &LogFailure::LimitExceeded {
                            what: "transports",
                            count: 8,
                            max: 8,
                        },
                    ));
                }
                self.loggers
                    .get_mut(id)
                    .expect("logger was checked above")
                    .file_transports
                    .push(LogFileTransport {
                        path: path.to_string(),
                        minimum,
                    });
                Ok(Value::Null)
            }
            "Log" => {
                require_arity(name, arguments, 4, span)?;
                let level = integer(&arguments[1], span)?.0;
                if !(0..=6).contains(&level) {
                    return Ok(failed(
                        name,
                        &LogFailure::OutOfRange {
                            what: "log level",
                            value: level,
                            min: 0,
                            max: 6,
                        },
                    ));
                }
                let Value::String(message) = &arguments[2] else {
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "BNLog.Logger.Log message",
                        span,
                    ));
                };
                if message.len() > 16 * 1024 {
                    return Ok(failed(
                        name,
                        &LogFailure::InvalidString {
                            what: "log message",
                            len: message.len(),
                            max: 16 * 1024,
                        },
                    ));
                }
                if !matches!(arguments[3], Value::LogFields(_)) {
                    return Err(type_mismatch(
                        "BNLog.Fields",
                        "non-BNLog.Fields value",
                        "BNLog.Logger.Log fields",
                        span,
                    ));
                }
                let Value::LogFields(fields_id) = arguments[3] else {
                    unreachable!("fields type was validated above")
                };
                let provided = self.fields.get(&fields_id).ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        "BNLog.Fields is invalid",
                        span,
                    )
                })?;
                let Some(level) = crate::log::Level::from_i128(level) else {
                    unreachable!("level was validated above")
                };
                let record = crate::log::Record::now(
                    &logger.label,
                    level,
                    message,
                    &logger.context,
                    provided,
                );
                let json_line = match record.json_line() {
                    Ok(line) => line,
                    Err(error) => {
                        return Ok(failed(
                            name,
                            &LogFailure::RecordSerialization(error.to_string()),
                        ));
                    }
                };
                let mut first_error = None;
                for minimum in &logger.console_transports {
                    if level as i128 <= *minimum
                        && let Err(error) = writeln!(core.output(), "{json_line}")
                    {
                        first_error.get_or_insert_with(|| error.to_string());
                    }
                }
                for transport in &logger.file_transports {
                    if level as i128 > transport.minimum {
                        continue;
                    }
                    let result = core
                        .host()
                        .filesystem()
                        .open(
                            std::path::Path::new(&transport.path),
                            bn_rt::secure_fs::OpenMode::Append,
                        )
                        .and_then(|mut file| {
                            use std::io::Write as _;
                            file.write_all(json_line.as_bytes())?;
                            file.write_all(b"\n")
                        });
                    if result
                        .as_ref()
                        .is_err_and(|error| error.kind() == std::io::ErrorKind::PermissionDenied)
                    {
                        return Err(runtime_error(
                            bn_diag::DiagId::EXECUTION_POLICY_DENIED,
                            "logger file path is outside the execution policy",
                            span,
                        ));
                    }
                    if let Err(error) = result {
                        first_error.get_or_insert_with(|| error.to_string());
                    }
                }
                if let Some(error) = first_error {
                    return Ok(failed(name, &LogFailure::IoFailed(error)));
                }
                Ok(Value::Null)
            }
            "Flush" | "Close" => {
                require_arity(name, arguments, 2, span)?;
                let timeout = integer(&arguments[1], span)?.0;
                if !(1..=60_000).contains(&timeout) {
                    return Ok(failed(
                        name,
                        &LogFailure::OutOfRange {
                            what: "timeout",
                            value: timeout,
                            min: 1,
                            max: 60_000,
                        },
                    ));
                }
                let mut first_error = None;
                for transport in &logger.file_transports {
                    let result = core
                        .host()
                        .filesystem()
                        .open(
                            std::path::Path::new(&transport.path),
                            bn_rt::secure_fs::OpenMode::Append,
                        )
                        .and_then(|file| file.sync_all());
                    if result
                        .as_ref()
                        .is_err_and(|error| error.kind() == std::io::ErrorKind::PermissionDenied)
                    {
                        return Err(runtime_error(
                            bn_diag::DiagId::EXECUTION_POLICY_DENIED,
                            "logger file path is outside the execution policy",
                            span,
                        ));
                    }
                    if let Err(error) = result {
                        first_error.get_or_insert_with(|| error.to_string());
                    }
                }
                if !logger.console_transports.is_empty()
                    && let Err(error) = core.output().flush()
                {
                    first_error.get_or_insert_with(|| error.to_string());
                }
                if let Some(error) = first_error {
                    return Ok(failed(name, &LogFailure::IoFailed(error)));
                }
                if method == "Close" {
                    self.loggers
                        .get_mut(id)
                        .expect("logger was checked above")
                        .closed = true;
                }
                Ok(Value::Null)
            }
            _ => Ok(failed(
                name,
                &LogFailure::Unavailable("BNLog.Logger operation unavailable"),
            )),
        }
    }
}

/// The `Error` of a failed member `name` (`BNLog.Logger.Log`),
/// with the report the native ABI records for the same failure.
fn failed(name: &str, failure: &LogFailure) -> Value {
    let mut parts = name.rsplit('.');
    let method = parts.next().unwrap_or_default();
    let class = parts.next().unwrap_or_default();
    Value::error_report(
        failure.code(),
        &format!("BNLog.{class}.{method}"),
        failure.message(),
        failure.cause(),
    )
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
