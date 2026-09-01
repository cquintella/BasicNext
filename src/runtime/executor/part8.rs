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
            .ok_or_else(|| runtime_error("TYPE_MISMATCH", "file receiver missing", span))?
        else {
            return Err(runtime_error(
                "TYPE_MISMATCH",
                "receiver is not FS.File",
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
            .ok_or_else(|| runtime_error("USE_AFTER_DELETE", "file handle is invalid", span))?;
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
                    return Err(runtime_error("TYPE_MISMATCH", "Write expects STRING", span));
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
            _ => Err(runtime_error(
                "NAME_NOT_FOUND",
                "unknown FS.File method",
                span,
            )),
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn data_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match name.rsplit('.').next().unwrap_or_default() {
            "ReadCSV" => {
                require_arity(name, arguments, 3, span)?;
                let Value::Boolean(has_header) = arguments[1] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "ReadCSV expects BOOLEAN header flag",
                        span,
                    ));
                };
                let Value::String(separator) = &arguments[2] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "ReadCSV expects STRING separator",
                        span,
                    ));
                };
                let separator = separator.chars().collect::<Vec<_>>();
                if separator.len() != 1
                    || separator[0] == '"'
                    || separator[0] == '\n'
                    || separator[0] == '\r'
                {
                    return Ok(Value::Error {
                        code: 1,
                        message: "invalid CSV separator".into(),
                    });
                }
                let text = match self.file_call("FS.File.ReadAll", &arguments[..1], span)? {
                    Value::String(text) => text,
                    Value::Error { message, .. } => return Ok(Value::Error { code: 1, message }),
                    _ => {
                        return Ok(Value::Error {
                            code: 1,
                            message: "CSV read failed".into(),
                        });
                    }
                };
                let mut rows = match parse_csv(&text, separator[0]) {
                    Ok(rows) => rows,
                    Err(message) => {
                        return Ok(Value::Error { code: 1, message });
                    }
                };
                let headers = if has_header && !rows.is_empty() {
                    rows.remove(0)
                } else {
                    Vec::new()
                };
                let width = headers.len().max(rows.first().map_or(0, Vec::len));
                if rows.iter().any(|row| row.len() != width)
                    || (has_header && headers.len() != width)
                {
                    return Ok(Value::Error {
                        code: 1,
                        message: "ragged CSV row".into(),
                    });
                }
                let columns: Vec<DataFrameColumn> = (0..width)
                    .map(|index| DataFrameColumn {
                        name: headers
                            .get(index)
                            .cloned()
                            .unwrap_or_else(|| format!("Column{}", index + 1)),
                        values: rows
                            .iter()
                            .map(|row| Value::String(row[index].clone()))
                            .collect(),
                    })
                    .collect();
                if duplicate_column_names(&columns) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "duplicate column name".into(),
                    });
                }
                let id = self.next_dataframe;
                self.next_dataframe += 1;
                self.dataframes.insert(id, DataFrameResource { columns });
                Ok(Value::DataFrame(id))
            }
            "WriteCSV" => {
                require_arity(name, arguments, 4, span)?;
                let Value::File(_) = arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "WriteCSV expects FS.File",
                        span,
                    ));
                };
                let Value::DataFrame(id) = arguments[1] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "WriteCSV expects DataFrame",
                        span,
                    ));
                };
                let Value::Boolean(write_header) = arguments[2] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "WriteCSV expects BOOLEAN header flag",
                        span,
                    ));
                };
                let Value::String(separator) = &arguments[3] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "WriteCSV expects STRING separator",
                        span,
                    ));
                };
                let separator = separator.chars().collect::<Vec<_>>();
                if separator.len() != 1
                    || separator[0] == '"'
                    || separator[0] == '\n'
                    || separator[0] == '\r'
                {
                    return Ok(Value::Error {
                        code: 1,
                        message: "invalid CSV separator".into(),
                    });
                }
                let quote = |value: &Value| {
                    let text = render(value);
                    if text.contains([separator[0], '"', '\n', '\r']) {
                        format!("\"{}\"", text.replace('"', "\"\""))
                    } else {
                        text
                    }
                };
                let lines = {
                    let frame = self.dataframes.get(&id).ok_or_else(|| {
                        runtime_error("USE_AFTER_DELETE", "DataFrame handle is invalid", span)
                    })?;
                    let mut lines = Vec::new();
                    if write_header {
                        lines.push(
                            frame
                                .columns
                                .iter()
                                .map(|column| quote(&Value::String(column.name.clone())))
                                .collect::<Vec<_>>()
                                .join(&separator[0].to_string()),
                        );
                    }
                    let rows = frame
                        .columns
                        .first()
                        .map_or(0, |column| column.values.len());
                    lines.extend((0..rows).map(|row| {
                        frame
                            .columns
                            .iter()
                            .map(|column| quote(&column.values[row]))
                            .collect::<Vec<_>>()
                            .join(&separator[0].to_string())
                    }));
                    lines
                };
                let body = if lines.is_empty() {
                    String::new()
                } else {
                    let mut body = lines.join("\n");
                    body.push('\n');
                    body
                };
                match self.file_call(
                    "FS.File.Write",
                    &[arguments[0].clone(), Value::String(body)],
                    span,
                )? {
                    Value::Error { code, message } => Ok(Value::Error { code, message }),
                    _ => Ok(Value::Null),
                }
            }
            _ => Err(runtime_error(
                "NAME_NOT_FOUND",
                "unknown BNData function",
                span,
            )),
        }
    }

}
