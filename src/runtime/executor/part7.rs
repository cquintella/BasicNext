#![allow(clippy::wildcard_imports, clippy::too_many_lines, clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
use super::*;

impl Executor<'_, '_> {
    pub(crate) fn host_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        if name.starts_with("HOST.Net.") {
            return self.host_net_call(name, arguments, span);
        }
        match name {
            "HOST.Clock.Timestamp" => {
                require_arity(name, arguments, 0, span)?;
                Ok(Value::Integer(
                    i128::from(self.host.timestamp_ms()),
                    IntegerType::Int64,
                ))
            }
            "HOST.Clock.Monotonic" => {
                require_arity(name, arguments, 0, span)?;
                Ok(Value::Integer(
                    i128::from(self.host.monotonic_ns()),
                    IntegerType::Int64,
                ))
            }
            "HOST.NumProcs" => {
                require_arity(name, arguments, 0, span)?;
                match std::thread::available_parallelism() {
                    Ok(count) => match i32::try_from(count.get()) {
                        Ok(count) => Ok(Value::Integer(i128::from(count), IntegerType::Int32)),
                        Err(_) => Ok(Value::Error {
                            code: 1,
                            message: "available processor count exceeds INTEGER range".into(),
                        }),
                    },
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: format!("available processor count is unavailable: {error}"),
                    }),
                }
            }
            "HOST.Random.Random" => {
                require_arity(name, arguments, 0, span)?;
                let mut state = self.host.random_state.get();
                state ^= state >> 12;
                state ^= state << 25;
                state ^= state >> 27;
                state = state.wrapping_mul(0x2545_F491_4F6C_DD1D);
                self.host.random_state.set(state);
                Ok(Value::Float(
                    (state >> 11) as f64 / 9_007_199_254_740_992.0,
                    FloatType::Float64,
                ))
            }
            "HOST.Random.Seed" => {
                require_arity(name, arguments, 1, span)?;
                let (seed, _) = integer(&arguments[0], span)?;
                self.host
                    .random_state
                    .set(seed as u64 | u64::from(seed == 0));
                Ok(Value::Null)
            }
            "HOST.Console.Cls" => {
                require_arity(name, arguments, 0, span)?;
                write!(self.output, "\x1b[2J\x1b[H")
                    .map_err(|e| runtime_error("OUTPUT_ERROR", e.to_string(), span))?;
                Ok(Value::Null)
            }
            "HOST.Console.Beep" => {
                require_arity(name, arguments, 0, span)?;
                write!(self.output, "\x07")
                    .map_err(|e| runtime_error("OUTPUT_ERROR", e.to_string(), span))?;
                Ok(Value::Null)
            }
            "HOST.Console.PrintAt" => {
                require_arity(name, arguments, 3, span)?;
                if !std::io::stdout().is_terminal() {
                    return Err(runtime_error(
                        "HOST_CAPABILITY_UNAVAILABLE",
                        "PrintAt requires a TTY",
                        span,
                    ));
                }
                let (column, _) = integer(&arguments[0], span)?;
                let (row, _) = integer(&arguments[1], span)?;
                let Value::String(text) = &arguments[2] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "PrintAt expects STRING",
                        span,
                    ));
                };
                let (cols, rows) = terminal_dimensions().ok_or_else(|| {
                    runtime_error(
                        "HOST_CAPABILITY_UNAVAILABLE",
                        "terminal dimensions are unavailable",
                        span,
                    )
                })?;
                let width = i128::try_from(text.chars().count()).unwrap_or(i128::MAX);
                if column < 1 || row < 1 || row > rows || column > cols || width > cols - column + 1
                {
                    return Err(runtime_error(
                        "INDEX_OUT_OF_BOUNDS",
                        "console coordinate is outside the window",
                        span,
                    ));
                }
                write!(self.output, "\x1b[{row};{column}H{text}")
                    .map_err(|e| runtime_error("OUTPUT_ERROR", e.to_string(), span))?;
                Ok(Value::Null)
            }
            "HOST.Console.NumCols" | "HOST.Console.NumRows" => {
                require_arity(name, arguments, 0, span)?;
                if !std::io::stdout().is_terminal() {
                    return Err(runtime_error(
                        "HOST_CAPABILITY_UNAVAILABLE",
                        "window size requires a TTY",
                        span,
                    ));
                }
                let (cols, rows) = terminal_dimensions().ok_or_else(|| {
                    runtime_error(
                        "HOST_CAPABILITY_UNAVAILABLE",
                        "terminal dimensions are unavailable",
                        span,
                    )
                })?;
                let value = if name.ends_with("NumCols") {
                    cols
                } else {
                    rows
                };
                integer_from_i128_count(value, span)
            }
            "HOST.FileSystem.Exists" => {
                require_arity(name, arguments, 1, span)?;
                let Value::String(path) = &arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "Exists expects STRING",
                        span,
                    ));
                };
                match std::fs::metadata(path) {
                    Ok(meta) => Ok(Value::Boolean(meta.is_file())),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        Ok(Value::Boolean(false))
                    }
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: error.to_string(),
                    }),
                }
            }
            "HOST.FileSystem.Open" => {
                require_arity(name, arguments, 2, span)?;
                let Value::String(path) = &arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "Open expects STRING path",
                        span,
                    ));
                };
                let (mode, _) = integer(&arguments[1], span)?;
                if std::fs::metadata(path).is_ok_and(|meta| meta.is_dir()) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "path is a directory".into(),
                    });
                }
                let result = match mode {
                    0 => std::fs::OpenOptions::new().read(true).open(path),
                    1 => std::fs::OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(path),
                    2 => std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path),
                    _ => {
                        return Ok(Value::Error {
                            code: 1,
                            message: "unknown file mode".into(),
                        });
                    }
                };
                match result {
                    Ok(file) => {
                        let id = self.next_file;
                        self.next_file += 1;
                        self.files.insert(
                            id,
                            FileResource {
                                file: Some(file),
                                family: None,
                            },
                        );
                        Ok(Value::File(id))
                    }
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: error.to_string(),
                    }),
                }
            }
            "HOST.FileSystem.DeleteFile" => {
                require_arity(name, arguments, 1, span)?;
                let Value::String(path) = &arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "DeleteFile expects STRING",
                        span,
                    ));
                };
                match std::fs::remove_file(path) {
                    Ok(()) => Ok(Value::Null),
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: error.to_string(),
                    }),
                }
            }
            _ => Err(runtime_error(
                "HOST_CAPABILITY_UNAVAILABLE",
                format!("host function '{name}' is not available"),
                span,
            )),
        }
    }
}
