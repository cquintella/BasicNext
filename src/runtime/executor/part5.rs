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
                    return Err(super::super::type_mismatch("STRING", "non-STRING value", "BNJson.Parse input", span));
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
                    return Err(super::super::type_mismatch("BNJson.Json", "non-BNJson.Json value", "BNJson.Stringify input", span));
                };
                let value = self.json_values.get(&id).ok_or_else(|| {
                    runtime_error("USE_AFTER_RELEASE", "BNJson.Json is invalid", span)
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
        request_id: Option<&str>,
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
        if let Some(request_id) = request_id {
            fields.insert("request_id".into(), request_id.into());
        }
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
            super::super::type_mismatch("BNLog.Fields", "missing receiver", "BNLog.Fields operation", span)
        })?
        else {
            return Err(super::super::type_mismatch("BNLog.Fields", "non-BNLog.Fields value", "BNLog.Fields operation", span));
        };
        let fields = self
            .log_fields
            .get_mut(id)
            .ok_or_else(|| runtime_error("USE_AFTER_RELEASE", "BNLog.Fields is invalid", span))?;
        match method {
            "CONSTRUCTOR" => Ok(Value::Null),
            "Count" => integer_from_i128_count(fields.len() as i128, span),
            "SetString" | "SetInteger" | "SetBoolean" => {
                require_arity(name, arguments, 3, span)?;
                let Value::String(key) = &arguments[1] else {
                    return Err(super::super::type_mismatch("STRING", "non-STRING value", "BNLog.Fields key", span));
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
                            return Err(super::super::type_mismatch("STRING", "non-STRING value", "BNLog.Fields.SetString value", span));
                        };
                        value.clone()
                    }
                    "SetInteger" => integer(&arguments[2], span)?.0.to_string(),
                    "SetBoolean" => match arguments[2] {
                        Value::Boolean(value) => value.to_string().to_uppercase(),
                        _ => {
                            return Err(super::super::type_mismatch("BOOLEAN", "non-BOOLEAN value", "BNLog.Fields.SetBoolean value", span));
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
                    return Err(super::super::type_mismatch("STRING", "non-STRING value", "BNLog.Fields.Get key", span));
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
            super::super::type_mismatch("BNLog.Entry", "missing receiver", "BNLog.Entry operation", span)
        })?
        else {
            return Err(super::super::type_mismatch("BNLog.Entry", "non-BNLog.Entry value", "BNLog.Entry operation", span));
        };
        let fields = self
            .log_entries
            .get(id)
            .ok_or_else(|| runtime_error("USE_AFTER_RELEASE", "BNLog.Entry is invalid", span))?;
        match method {
            "CONSTRUCTOR" => Ok(Value::Null),
            "WithField" => {
                require_arity(name, arguments, 3, span)?;
                let Value::String(key) = &arguments[1] else {
                    return Err(super::super::type_mismatch("STRING", "non-STRING value", "BNLog.Entry.WithField key", span));
                };
                let Value::String(value) = &arguments[2] else {
                    return Err(super::super::type_mismatch("STRING", "non-STRING value", "BNLog.Entry.WithField value", span));
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
