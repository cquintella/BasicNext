#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

impl Executor<'_, '_> {
    pub(crate) fn json_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = name.rsplit('.').next().unwrap_or_default();
        match method {
            "Parse" => {
                require_arity(name, arguments, 1, span)?;
                let Value::String(text) = &arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "JSON input must be STRING",
                        span,
                    ));
                };
                let parsed = crate::json::parse(text)
                    .map_err(|message| runtime_error("INVALID_JSON", message, span))?;
                let id = self.next_json_value;
                self.next_json_value += 1;
                self.json_values.insert(id, parsed);
                Ok(Value::Json(id))
            }
            "Stringify" => {
                require_arity(name, arguments, 1, span)?;
                let Value::Json(id) = arguments[0] else {
                    return Err(runtime_error("TYPE_MISMATCH", "expected BNJson.Json", span));
                };
                let value = self.json_values.get(&id).ok_or_else(|| {
                    runtime_error("USE_AFTER_DELETE", "BNJson.Json is invalid", span)
                })?;
                let text = crate::json::stringify(value)
                    .map_err(|message| runtime_error("INVALID_JSON", message, span))?;
                Ok(Value::String(text))
            }
            "CONSTRUCTOR" => Ok(Value::Null),
            _ => Ok(Value::Error {
                code: 1,
                message: "BNJson operation unavailable".into(),
            }),
        }
    }

    pub(crate) fn log_web_dispatch(
        &mut self,
        server_handle: Handle,
        method: &str,
        path: &str,
        status: i128,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let Some(&logger_id) = self.web_loggers.get(&server_handle) else {
            return Ok(());
        };
        let fields_id = self.next_log_fields;
        self.next_log_fields += 1;
        let mut fields = HashMap::new();
        fields.insert("http.method".into(), method.into());
        fields.insert("http.path".into(), path.into());
        fields.insert("http.status".into(), status.to_string());
        self.log_fields.insert(fields_id, fields);
        let result = self.log_logger_call(
            "BNLog.Logger.Log",
            &[
                Value::LogLogger(logger_id),
                Value::Integer(3, IntegerType::Int32),
                Value::String("web dispatch".into()),
                Value::LogFields(fields_id),
            ],
            span,
        );
        self.log_fields.remove(&fields_id);
        result.map(|_| ())
    }

    pub(crate) fn log_fields_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = name.rsplit('.').next().unwrap_or_default();
        let Value::LogFields(id) = arguments.first().ok_or_else(|| {
            runtime_error("TYPE_MISMATCH", "BNLog.Fields receiver is missing", span)
        })?
        else {
            return Err(runtime_error(
                "TYPE_MISMATCH",
                "expected BNLog.Fields",
                span,
            ));
        };
        let fields = self
            .log_fields
            .get_mut(id)
            .ok_or_else(|| runtime_error("USE_AFTER_DELETE", "BNLog.Fields is invalid", span))?;
        match method {
            "CONSTRUCTOR" => Ok(Value::Null),
            "Count" => integer_from_i128_count(fields.len() as i128, span),
            "SetString" | "SetInteger" | "SetBoolean" => {
                require_arity(name, arguments, 3, span)?;
                let Value::String(key) = &arguments[1] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "field key must be STRING",
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
                            return Err(runtime_error(
                                "TYPE_MISMATCH",
                                "field value must be STRING",
                                span,
                            ));
                        };
                        value.clone()
                    }
                    "SetInteger" => integer(&arguments[2], span)?.0.to_string(),
                    "SetBoolean" => match arguments[2] {
                        Value::Boolean(value) => value.to_string().to_uppercase(),
                        _ => {
                            return Err(runtime_error(
                                "TYPE_MISMATCH",
                                "field value must be BOOLEAN",
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
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "field key must be STRING",
                        span,
                    ));
                };
                fields
                    .get(key)
                    .cloned()
                    .map(Value::String)
                    .ok_or_else(|| runtime_error("NOT_FOUND", "field key was not found", span))
            }
            _ => Ok(Value::Error {
                code: 1,
                message: "BNLog.Fields operation unavailable".into(),
            }),
        }
    }

    pub(crate) fn log_entry_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = name.rsplit('.').next().unwrap_or_default();
        let Value::LogEntry(id) = arguments.first().ok_or_else(|| {
            runtime_error("TYPE_MISMATCH", "BNLog.Entry receiver is missing", span)
        })?
        else {
            return Err(runtime_error("TYPE_MISMATCH", "expected BNLog.Entry", span));
        };
        let fields = self
            .log_entries
            .get(id)
            .ok_or_else(|| runtime_error("USE_AFTER_DELETE", "BNLog.Entry is invalid", span))?;
        match method {
            "CONSTRUCTOR" => Ok(Value::Null),
            "WithField" => {
                require_arity(name, arguments, 3, span)?;
                let Value::String(key) = &arguments[1] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "field key must be STRING",
                        span,
                    ));
                };
                let Value::String(value) = &arguments[2] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "field value must be STRING",
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
                let next_id = self.next_log_entry;
                self.next_log_entry += 1;
                self.log_entries.insert(next_id, next);
                Ok(Value::LogEntry(next_id))
            }
            _ => Ok(Value::Error {
                code: 1,
                message: "BNLog.Entry provider unavailable".into(),
            }),
        }
    }

}
