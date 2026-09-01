#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

impl Executor<'_, '_> {
    pub(crate) fn log_logger_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = name.rsplit('.').next().unwrap_or_default();
        if method == "New" {
            require_arity(name, arguments, 1, span)?;
            let Value::String(label) = &arguments[0] else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "logger label must be STRING",
                    span,
                ));
            };
            if label.is_empty() || label.len() > 128 {
                return Ok(Value::Error {
                    code: 1,
                    message: "logger label exceeds bounds".into(),
                });
            }
            let id = self.next_log_logger;
            self.next_log_logger += 1;
            self.log_loggers.insert(
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
            runtime_error("TYPE_MISMATCH", "BNLog.Logger receiver is missing", span)
        })?
        else {
            return Err(runtime_error(
                "TYPE_MISMATCH",
                "expected BNLog.Logger",
                span,
            ));
        };
        if method == "Child" {
            require_arity(name, arguments, 2, span)?;
            let Value::LogFields(fields_id) = arguments[1] else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "child fields must be BNLog.Fields",
                    span,
                ));
            };
            if !self.log_fields.contains_key(&fields_id) {
                return Err(runtime_error(
                    "USE_AFTER_DELETE",
                    "BNLog.Fields is invalid",
                    span,
                ));
            }
            let parent = self
                .log_loggers
                .get(id)
                .ok_or_else(|| runtime_error("USE_AFTER_DELETE", "BNLog.Logger is invalid", span))?
                .clone();
            let mut context = parent.context;
            context.extend(
                self.log_fields
                    .get(&fields_id)
                    .ok_or_else(|| {
                        runtime_error("USE_AFTER_DELETE", "BNLog.Fields is invalid", span)
                    })?
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone())),
            );
            let child_id = self.next_log_logger;
            self.next_log_logger += 1;
            self.log_loggers.insert(
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
        let logger =
            self.log_loggers.get(id).cloned().ok_or_else(|| {
                runtime_error("USE_AFTER_DELETE", "BNLog.Logger is invalid", span)
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
                self.log_loggers
                    .get_mut(id)
                    .expect("logger was checked above")
                    .null_transports
                    .push(minimum);
                Ok(Value::Null)
            }
            "AddConsole" => {
                require_arity(name, arguments, 2, span)?;
                if self.module.console_import.is_none() {
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
                self.log_loggers
                    .get_mut(id)
                    .expect("logger was checked above")
                    .console_transports
                    .push(minimum);
                Ok(Value::Null)
            }
            "AddFile" => {
                require_arity(name, arguments, 3, span)?;
                if self.module.filesystem_import.is_none() || !self.host.filesystem {
                    return Ok(Value::Error {
                        code: 1,
                        message: "HOST.FileSystem capability is required for AddFile".into(),
                    });
                }
                let Value::String(path) = &arguments[1] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "file path must be STRING",
                        span,
                    ));
                };
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
                self.log_loggers
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
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "log message must be STRING",
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
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "log fields must be BNLog.Fields",
                        span,
                    ));
                }
                let Value::LogFields(fields_id) = arguments[3] else {
                    unreachable!("fields type was validated above")
                };
                let mut fields = logger.context.clone();
                let provided = self.log_fields.get(&fields_id).ok_or_else(|| {
                    runtime_error("USE_AFTER_DELETE", "BNLog.Fields is invalid", span)
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
                        && let Err(error) = writeln!(self.output, "{json_line}")
                    {
                        first_error.get_or_insert_with(|| error.to_string());
                    }
                }
                for transport in &logger.file_transports {
                    if level as i128 > transport.minimum {
                        continue;
                    }
                    let result = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&transport.path)
                        .and_then(|mut file| {
                            use std::io::Write as _;
                            file.write_all(json_line.as_bytes())?;
                            file.write_all(b"\n")
                        });
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
                    let result = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&transport.path)
                        .and_then(|file| file.sync_all());
                    if let Err(error) = result {
                        first_error.get_or_insert_with(|| error.to_string());
                    }
                }
                if !logger.console_transports.is_empty()
                    && let Err(error) = self.output.flush()
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
                    self.log_loggers
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

    pub(crate) fn standard_provider(name: &str, providers: &HashSet<ModuleId>) -> bool {
        let Some(name) = name.strip_prefix('#') else {
            return false;
        };
        let Some((module, _)) = name.split_once('.') else {
            return false;
        };
        let Ok(id) = module.parse() else {
            return false;
        };
        providers.contains(&ModuleId(id))
    }

}
