#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

impl Executor<'_, '_> {
    pub(crate) fn file_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let Value::File(id) = arguments
            .first()
            .ok_or_else(|| super::super::type_mismatch("FS.File", "missing receiver", "file operation", span))?
        else {
            return Err(super::super::type_mismatch(
                "FS.File",
                "non-FS.File value",
                "file operation receiver",
                span,
            ));
        };
        match name.rsplit('.').next().unwrap_or_default() {
            "ReadBytes" => return self.file_read_bytes(*id, arguments, span),
            "WriteBytes" => return self.file_write_bytes(*id, arguments, span),
            _ => {}
        }
        let resource = self
            .files
            .get_mut(id)
            .ok_or_else(|| runtime_error(crate::diagnostic::DiagId::USE_AFTER_RELEASE, "file handle is invalid", span))?;
        match name.rsplit('.').next().unwrap_or_default() {
            "Close" => {
                require_arity(name, arguments, 1, span)?;
                let Some(file) = resource.file.take() else {
                    return Ok(Value::Null);
                };
                resource.family = None;
                match file.sync_all() {
                    Ok(()) => Ok(Value::Null),
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: error.to_string(),
                    }),
                }
            }
            "ReadAll" => {
                require_arity(name, arguments, 1, span)?;
                let Some(file) = resource.file.as_mut() else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "file is closed".into(),
                    });
                };
                if resource.family == Some(false) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "file is in binary mode".into(),
                    });
                }
                let mut text = String::new();
                match file.read_to_string(&mut text) {
                    Ok(_) => {
                        resource.family = Some(true);
                        Ok(Value::String(text))
                    }
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: error.to_string(),
                    }),
                }
            }
            "ReadLine" => {
                require_arity(name, arguments, 1, span)?;
                if resource.family == Some(false) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "file is in binary mode".into(),
                    });
                }
                let Some(file) = resource.file.as_mut() else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "file is closed".into(),
                    });
                };
                let mut bytes = Vec::new();
                let mut read_any = false;
                let mut one = [0_u8; 1];
                loop {
                    match file.read(&mut one) {
                        Ok(0) => break,
                        Ok(_) if one[0] == b'\n' => {
                            read_any = true;
                            break;
                        }
                        Ok(_) => {
                            read_any = true;
                            bytes.push(one[0]);
                        }
                        Err(error) => {
                            return Ok(Value::Error {
                                code: 1,
                                message: error.to_string(),
                            });
                        }
                    }
                }
                if !read_any {
                    resource.family = Some(true);
                    return Ok(Value::EndOfFile);
                }
                if bytes.last() == Some(&b'\r') {
                    bytes.pop();
                }
                Ok(String::from_utf8(bytes).map_or_else(
                    |error| Value::Error {
                        code: 1,
                        message: format!("INVALID_UTF8: {error}"),
                    },
                    |text| {
                        resource.family = Some(true);
                        Value::String(text)
                    },
                ))
            }
            "Write" | "WriteLine" => {
                require_arity(name, arguments, 2, span)?;
                let Value::String(text) = &arguments[1] else {
                    return Err(super::super::type_mismatch("STRING", "non-STRING value", "FS.File.Write text", span));
                };
                let Some(file) = resource.file.as_mut() else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "file is closed".into(),
                    });
                };
                if resource.family == Some(false) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "file is in binary mode".into(),
                    });
                }
                let text = if name.ends_with("WriteLine") {
                    format!("{text}\n")
                } else {
                    text.clone()
                };
                match file.write_all(text.as_bytes()) {
                    Ok(()) => {
                        resource.family = Some(true);
                        Ok(Value::Null)
                    }
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: error.to_string(),
                    }),
                }
            }
            _ => Err(super::super::name_not_found(name, "FS.File method", span)),
        }
    }

}
