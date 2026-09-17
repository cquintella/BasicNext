// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNLog` — an external library module served through the provider seam.
//! Owns the fields / entry / logger tables; console transports write to the
//! program output through [`CoreContext::output`], file transports go through
//! the host filesystem policy.

use std::collections::HashMap;

use bn_diag::Diagnostic;
use bn_source::Span;
use bn_value::Value;

use crate::runtime::provider::{CoreContext, Provider};
use crate::runtime::{
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
        Ok(Value::Error {
            code: 1,
            message: "BNLog provider unavailable".into(),
        })
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
                crate::diagnostic::DiagId::DOUBLE_RELEASE,
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
                crate::diagnostic::DiagId::USE_AFTER_RELEASE,
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
                    return Ok(Value::Error {
                        code: 1,
                        message: "field key exceeds 128 bytes".into(),
                    });
                }
                if fields.contains_key(key) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "field key already exists".into(),
                    });
                }
                if !fields.contains_key(key) && fields.len() >= 64 {
                    return Ok(Value::Error {
                        code: 1,
                        message: "field limit exceeded".into(),
                    });
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
                        value.clone()
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
                fields.insert(key.clone(), value);
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
                fields.get(key).cloned().map(Value::String).ok_or_else(|| {
                    runtime_error(
                        crate::diagnostic::DiagId::NOT_FOUND,
                        "field key was not found",
                        span,
                    )
                })
            }
            _ => Ok(Value::Error {
                code: 1,
                message: "BNLog.Fields operation unavailable".into(),
            }),
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
                crate::diagnostic::DiagId::USE_AFTER_RELEASE,
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
                if key.is_empty() || key.len() > 128 || value.len() > 4096 {
                    return Ok(Value::Error {
                        code: 1,
                        message: "entry field exceeds bounds".into(),
                    });
                }
                if fields.contains_key(key) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "entry field already exists".into(),
                    });
                }
                let mut next = fields.clone();
                if next.len() >= 64 {
                    return Ok(Value::Error {
                        code: 1,
                        message: "entry field limit exceeded".into(),
                    });
                }
                next.insert(key.clone(), value.clone());
                let next_id = self.next_entry;
                self.next_entry += 1;
                self.entries.insert(next_id, next);
                Ok(Value::LogEntry(next_id))
            }
            _ => Ok(Value::Error {
                code: 1,
                message: "BNLog.Entry provider unavailable".into(),
            }),
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
                return Ok(Value::Error {
                    code: 1,
                    message: "logger label exceeds bounds".into(),
                });
            }
            let id = self.next_logger;
            self.next_logger += 1;
            self.loggers.insert(
                id,
                LogLoggerResource {
                    label: label.clone(),
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
                    crate::diagnostic::DiagId::USE_AFTER_RELEASE,
                    "BNLog.Fields is invalid",
                    span,
                ));
            }
            let parent = self
                .loggers
                .get(id)
                .ok_or_else(|| {
                    runtime_error(
                        crate::diagnostic::DiagId::USE_AFTER_RELEASE,
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
                            crate::diagnostic::DiagId::USE_AFTER_RELEASE,
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
                crate::diagnostic::DiagId::USE_AFTER_RELEASE,
                "BNLog.Logger is invalid",
                span,
            )
        })?;
        if method == "CONSTRUCTOR" {
            return Ok(Value::Null);
        }
        if logger.closed {
            return Ok(Value::Error {
                code: 1,
                message: "logger is closed".into(),
            });
        }
        match method {
            "AddNull" => {
                require_arity(name, arguments, 2, span)?;
                let minimum = integer(&arguments[1], span)?.0;
                if !(0..=6).contains(&minimum) || logger.null_transports.len() >= 8 {
                    return Ok(Value::Error {
                        code: 1,
                        message: "invalid logger transport".into(),
                    });
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
                    return Ok(Value::Error {
                        code: 1,
                        message: "HOST.Console capability is required for AddConsole".into(),
                    });
                }
                let minimum = integer(&arguments[1], span)?.0;
                if !(0..=6).contains(&minimum)
                    || logger.null_transports.len()
                        + logger.console_transports.len()
                        + logger.file_transports.len()
                        >= 8
                {
                    return Ok(Value::Error {
                        code: 1,
                        message: "invalid logger console transport".into(),
                    });
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
                    return Ok(Value::Error {
                        code: 1,
                        message: "HOST.FileSystem capability is required for AddFile".into(),
                    });
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
                    .allows_path(std::path::Path::new(path), true)
                {
                    return Err(runtime_error(
                        crate::diagnostic::DiagId::EXECUTION_POLICY_DENIED,
                        "logger file path is outside the execution policy",
                        span,
                    ));
                }
                let minimum = integer(&arguments[2], span)?.0;
                if path.is_empty() || path.len() > 4096 || !(0..=6).contains(&minimum) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "invalid logger file transport".into(),
                    });
                }
                if logger.null_transports.len()
                    + logger.console_transports.len()
                    + logger.file_transports.len()
                    >= 8
                {
                    return Ok(Value::Error {
                        code: 1,
                        message: "logger transport limit exceeded".into(),
                    });
                }
                self.loggers
                    .get_mut(id)
                    .expect("logger was checked above")
                    .file_transports
                    .push(LogFileTransport {
                        path: path.clone(),
                        minimum,
                    });
                Ok(Value::Null)
            }
            "Log" => {
                require_arity(name, arguments, 4, span)?;
                let level = integer(&arguments[1], span)?.0;
                if !(0..=6).contains(&level) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "invalid log level".into(),
                    });
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
                    return Ok(Value::Error {
                        code: 1,
                        message: "log message exceeds bounds".into(),
                    });
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
                let mut fields = logger.context.clone();
                let provided = self.fields.get(&fields_id).ok_or_else(|| {
                    runtime_error(
                        crate::diagnostic::DiagId::USE_AFTER_RELEASE,
                        "BNLog.Fields is invalid",
                        span,
                    )
                })?;
                fields.extend(provided.clone());
                let Some(level) = crate::log::Level::from_i128(level) else {
                    unreachable!("level was validated above")
                };
                let record = crate::log::Record {
                    timestamp: format!("{:?}", std::time::SystemTime::now()),
                    label: logger.label.clone(),
                    level,
                    message: message.clone(),
                    fields,
                };
                let json_line = match record.json_line() {
                    Ok(line) => line,
                    Err(error) => {
                        return Ok(Value::Error {
                            code: 1,
                            message: error.into(),
                        });
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
                            crate::diagnostic::DiagId::EXECUTION_POLICY_DENIED,
                            "logger file path is outside the execution policy",
                            span,
                        ));
                    }
                    if let Err(error) = result {
                        first_error.get_or_insert_with(|| error.to_string());
                    }
                }
                if let Some(error) = first_error {
                    return Ok(Value::Error {
                        code: 1,
                        message: format!("log transport failed: {error}"),
                    });
                }
                Ok(Value::Null)
            }
            "Flush" | "Close" => {
                require_arity(name, arguments, 2, span)?;
                let timeout = integer(&arguments[1], span)?.0;
                if !(1..=60_000).contains(&timeout) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "timeout exceeds bounds".into(),
                    });
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
                            crate::diagnostic::DiagId::EXECUTION_POLICY_DENIED,
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
                    return Ok(Value::Error {
                        code: 1,
                        message: format!("log flush failed: {error}"),
                    });
                }
                if method == "Close" {
                    self.loggers
                        .get_mut(id)
                        .expect("logger was checked above")
                        .closed = true;
                }
                Ok(Value::Null)
            }
            _ => Ok(Value::Error {
                code: 1,
                message: "BNLog.Logger operation unavailable".into(),
            }),
        }
    }
}
