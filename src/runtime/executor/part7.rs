#![allow(clippy::wildcard_imports, clippy::too_many_lines, clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
use super::*;

fn drain_exec_output<R: std::io::Read>(pipe: Option<R>, capture_limit: usize) -> Option<Vec<u8>> {
    pipe.map(|mut pipe| {
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 8192];
        // Keep draining past the ceiling so the child cannot block on a full pipe;
        // discard excess bytes because Error has no stream fields (D-H1-02).
        let mut total = 0_usize;
        let mut exceeded = false;
        loop {
            match std::io::Read::read(&mut pipe, &mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    total = total.saturating_add(count);
                    if !exceeded && bytes.len() <= capture_limit {
                        bytes.extend_from_slice(&chunk[..count]);
                        if bytes.len() > capture_limit {
                            exceeded = true;
                            bytes.clear();
                        }
                    } else {
                        exceeded = true;
                        bytes.clear();
                    }
                }
            }
        }
        if exceeded || total > capture_limit {
            // Signal overflow to the waiter via a sentinel length above the limit.
            bytes.clear();
            bytes.resize(capture_limit.saturating_add(1), 0);
        }
        bytes
    })
}

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
            "HOST.Exec.Run" => self.exec_run(arguments, span),
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
                    return Err(super::super::type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "HOST.Console.PrintAt text",
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
                    return Err(super::super::type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "HOST.FileSystem.Exists path",
                        span,
                    ));
                };
                match self.host.filesystem.open(
                    std::path::Path::new(path),
                    bn_rt::secure_fs::OpenMode::Read,
                ) {
                    Ok(file) => Ok(Value::Boolean(file.metadata().is_ok_and(|meta| meta.is_file()))),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        Ok(Value::Boolean(false))
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                        Err(runtime_error(
                            "EXECUTION_POLICY_DENIED",
                            "filesystem read is outside the execution policy",
                            span,
                        ))
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
                    return Err(super::super::type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "HOST.FileSystem.Open path",
                        span,
                    ));
                };
                let (mode, _) = integer(&arguments[1], span)?;
                let open_mode = match mode {
                    0 => bn_rt::secure_fs::OpenMode::Read,
                    1 => bn_rt::secure_fs::OpenMode::Write,
                    2 => bn_rt::secure_fs::OpenMode::Append,
                    _ => {
                        return Ok(Value::Error {
                            code: 1,
                            message: "unknown file mode".into(),
                        });
                    }
                };
                let result = self
                    .host
                    .filesystem
                    .open(std::path::Path::new(path), open_mode);
                match result {
                    Ok(file) => {
                        if file.metadata().is_ok_and(|meta| meta.is_dir()) {
                            return Ok(Value::Error {
                                code: 1,
                                message: "path is a directory".into(),
                            });
                        }
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
                    Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                        Err(runtime_error(
                            "EXECUTION_POLICY_DENIED",
                            "filesystem path is outside the execution policy",
                            span,
                        ))
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
                    return Err(super::super::type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "HOST.FileSystem.DeleteFile path",
                        span,
                    ));
                };
                match self
                    .host
                    .filesystem
                    .remove_file(std::path::Path::new(path))
                {
                    Ok(()) => Ok(Value::Null),
                    Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                        Err(runtime_error(
                            "EXECUTION_POLICY_DENIED",
                            "filesystem deletion is outside the execution policy",
                            span,
                        ))
                    }
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

    fn exec_run(&mut self, arguments: &[Value], span: Span) -> Result<Value, Diagnostic> {
        require_arity("HOST.Exec.Run", arguments, 2, span)?;
        if !self.host.exec_allowed {
            return Ok(Value::Error { code: 11, message: "HOST.Exec is denied by execution policy".into() });
        }
        let capture_limit = self.host.exec_capture_limit;
        let timeout = self.host.exec_timeout;
        let Value::String(program) = &arguments[0] else {
            return Err(super::super::type_mismatch("STRING", "non-STRING value", "HOST.Exec.Run program", span));
        };
        let Value::Vector(args) = &arguments[1] else {
            return Err(super::super::type_mismatch("STRING[]", "non-vector value", "HOST.Exec.Run args", span));
        };
        if program.is_empty() || program.as_bytes().contains(&0) {
            return Ok(Value::Error { code: 1, message: "program must be non-empty and contain no NUL".into() });
        }
        let mut command = std::process::Command::new(program);
        command.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        for arg in args {
            let Value::String(value) = arg else {
                return Err(super::super::type_mismatch("STRING", "non-STRING vector element", "HOST.Exec.Run args", span));
            };
            if value.as_bytes().contains(&0) {
                return Ok(Value::Error { code: 1, message: "arguments must not contain NUL".into() });
            }
            command.arg(value);
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Value::Error { code: 2, message: error.to_string() }),
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return Ok(Value::Error { code: 3, message: error.to_string() }),
            Err(error) => return Ok(Value::Error { code: 4, message: error.to_string() }),
        };
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let out_thread = std::thread::spawn(move || drain_exec_output(stdout, capture_limit));
        let err_thread = std::thread::spawn(move || drain_exec_output(stderr, capture_limit));
        let deadline = std::time::Instant::now() + timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if std::time::Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = out_thread.join();
                    let _ = err_thread.join();
                    return Ok(Value::Error { code: 9, message: "process exceeded execution timeout".into() });
                }
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(5)),
                Err(error) => return Ok(Value::Error { code: 5, message: error.to_string() }),
            }
        };
        let stdout = out_thread.join().ok().flatten().unwrap_or_default();
        let stderr = err_thread.join().ok().flatten().unwrap_or_default();
        if stdout.len() > capture_limit || stderr.len() > capture_limit {
            return Ok(Value::Error { code: 8, message: "captured output exceeded per-stream capture limit".into() });
        }
        let Ok(stdout) = String::from_utf8(stdout) else { return Ok(Value::Error { code: 7, message: "stdout is not valid UTF-8".into() }) };
        let Ok(stderr) = String::from_utf8(stderr) else { return Ok(Value::Error { code: 7, message: "stderr is not valid UTF-8".into() }) };
        #[cfg(unix)]
        let return_code = status.code().map_or_else(
            || {
                use std::os::unix::process::ExitStatusExt;
                -i128::from(status.signal().unwrap_or(1))
            },
            i128::from,
        );
        #[cfg(not(unix))]
        let return_code = status.code().map_or(-1_i128, i128::from);
        Ok(Value::Record { type_name: "HOST.Exec.Result".into(), fields: HashMap::from([
            ("ReturnCode".into(), Value::Integer(return_code, IntegerType::Int64)),
            ("Stdout".into(), Value::String(stdout)),
            ("Stderr".into(), Value::String(stderr)),
        ]) })
    }
}
