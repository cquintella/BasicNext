// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.Exec` — runs a program under the execution policy (allow flag,
//! timeout, capture ceiling) the host environment carries. The native side
//! has its own copy in `bn_rt::exec`; unifying both is activity 3.1.
#![allow(
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)] // Moved verbatim from the core (bucket 0.5.1d 1.5).

use std::collections::HashMap;

use bn_diag::Diagnostic;
use bn_source::Span;
use bn_value::Value;

use crate::runtime::provider::{CoreContext, Provider};
use crate::runtime::{
    require_arity_pub as require_arity, runtime_error_pub as runtime_error, type_mismatch,
};
use crate::types::IntegerType;

pub const NAME: &str = "Exec";

pub struct ExecProvider;

impl Provider for ExecProvider {
    fn call(
        &mut self,
        core: &mut dyn CoreContext,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let name = format!("HOST.Exec.{member}");
        let name = name.as_str();
        match member {
            "Run" => exec_run(core, &arguments, span),
            _ => Err(runtime_error(
                crate::diagnostic::DiagId::HOST_CAPABILITY_UNAVAILABLE,
                format!("host function '{name}' is not available"),
                span,
            )),
        }
    }
}
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

fn exec_run(
    core: &mut dyn CoreContext,
    arguments: &[Value],
    span: Span,
) -> Result<Value, Diagnostic> {
    require_arity("HOST.Exec.Run", arguments, 2, span)?;
    if !core.host().exec_allowed() {
        return Ok(Value::Error {
            code: 11,
            message: "HOST.Exec is denied by execution policy".into(),
        });
    }
    let capture_limit = core.host().exec_capture_limit();
    let timeout = core.host().exec_timeout();
    let Value::String(program) = &arguments[0] else {
        return Err(type_mismatch(
            "STRING",
            "non-STRING value",
            "HOST.Exec.Run program",
            span,
        ));
    };
    let Value::Vector(args) = &arguments[1] else {
        return Err(type_mismatch(
            "STRING[]",
            "non-vector value",
            "HOST.Exec.Run args",
            span,
        ));
    };
    if program.is_empty() || program.as_bytes().contains(&0) {
        return Ok(Value::Error {
            code: 1,
            message: "program must be non-empty and contain no NUL".into(),
        });
    }
    let mut command = std::process::Command::new(program);
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    for arg in args {
        let Value::String(value) = arg else {
            return Err(type_mismatch(
                "STRING",
                "non-STRING vector element",
                "HOST.Exec.Run args",
                span,
            ));
        };
        if value.as_bytes().contains(&0) {
            return Ok(Value::Error {
                code: 1,
                message: "arguments must not contain NUL".into(),
            });
        }
        command.arg(value);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Value::Error {
                code: 2,
                message: error.to_string(),
            });
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return Ok(Value::Error {
                code: 3,
                message: error.to_string(),
            });
        }
        Err(error) => {
            return Ok(Value::Error {
                code: 4,
                message: error.to_string(),
            });
        }
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
                return Ok(Value::Error {
                    code: 9,
                    message: "process exceeded execution timeout".into(),
                });
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(5)),
            Err(error) => {
                return Ok(Value::Error {
                    code: 5,
                    message: error.to_string(),
                });
            }
        }
    };
    let stdout = out_thread.join().ok().flatten().unwrap_or_default();
    let stderr = err_thread.join().ok().flatten().unwrap_or_default();
    if stdout.len() > capture_limit || stderr.len() > capture_limit {
        return Ok(Value::Error {
            code: 8,
            message: "captured output exceeded per-stream capture limit".into(),
        });
    }
    let Ok(stdout) = String::from_utf8(stdout) else {
        return Ok(Value::Error {
            code: 7,
            message: "stdout is not valid UTF-8".into(),
        });
    };
    let Ok(stderr) = String::from_utf8(stderr) else {
        return Ok(Value::Error {
            code: 7,
            message: "stderr is not valid UTF-8".into(),
        });
    };
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
    Ok(Value::Record {
        type_name: "HOST.Exec.Result".into(),
        fields: HashMap::from([
            (
                "ReturnCode".into(),
                Value::Integer(return_code, IntegerType::Int64),
            ),
            ("Stdout".into(), Value::String(stdout)),
            ("Stderr".into(), Value::String(stderr)),
        ]),
    })
}
