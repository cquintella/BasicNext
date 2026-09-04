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
            "HOST.Clock.Now" => {
                require_arity(name, arguments, 0, span)?;
                Ok(Value::Integer(
                    i128::from(self.host.timestamp_ms()),
                    IntegerType::Int64,
                ))
            }
            "HOST.Clock.Timer" => {
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
                let mut state = self
                    .host
                    .random_state
                    .load(std::sync::atomic::Ordering::Relaxed);
                state ^= state >> 12;
                state ^= state << 25;
                state ^= state >> 27;
                state = state.wrapping_mul(0x2545_F491_4F6C_DD1D);
                self.host
                    .random_state
                    .store(state, std::sync::atomic::Ordering::Relaxed);
                Ok(Value::Float(
                    (state >> 11) as f64 / 9_007_199_254_740_992.0,
                    FloatType::Float64,
                ))
            }
            "HOST.Random.Seed" => {
                require_arity(name, arguments, 1, span)?;
                let (seed, _) = integer(&arguments[0], span)?;
                self.host.random_state.store(
                    seed as u64 | u64::from(seed == 0),
                    std::sync::atomic::Ordering::Relaxed,
                );
                Ok(Value::Null)
            }
            "HOST.Console.Cls" => {
                require_arity(name, arguments, 0, span)?;
                bn_rt::cls(self.output).map_err(|error| console_runtime_error(&error, span))?;
                Ok(Value::Null)
            }
            "HOST.Console.Beep" => {
                require_arity(name, arguments, 0, span)?;
                bn_rt::beep(self.output).map_err(|error| console_runtime_error(&error, span))?;
                Ok(Value::Null)
            }
            "HOST.Console.PrintAt" => {
                require_arity(name, arguments, 3, span)?;
                let (column, _) = integer(&arguments[0], span)?;
                let (row, _) = integer(&arguments[1], span)?;
                let Value::String(text) = &arguments[2] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "PrintAt expects STRING",
                        span,
                    ));
                };
                bn_rt::print_at(self.output, column, row, text)
                    .map_err(|error| console_runtime_error(&error, span))?;
                Ok(Value::Null)
            }
            "HOST.Console.NumCols" => {
                require_arity(name, arguments, 0, span)?;
                match bn_rt::num_cols() {
                    Ok(value) => Ok(Value::Integer(i128::from(value), IntegerType::Int32)),
                    Err(error) => Err(console_runtime_error(&error, span)),
                }
            }
            "HOST.Console.NumRows" => {
                require_arity(name, arguments, 0, span)?;
                match bn_rt::num_rows() {
                    Ok(value) => Ok(Value::Integer(i128::from(value), IntegerType::Int32)),
                    Err(error) => Err(console_runtime_error(&error, span)),
                }
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
