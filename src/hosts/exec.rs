// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.Exec` provider for the interpreter: checks the BN arguments, reads
//! the execution policy (allow flag, timeout, capture ceiling) from the host
//! environment and calls `bn_host_exec::run` — the one implementation shared
//! with the native runtime; projects the outcome as `HOST.Exec.Result` or `Error`.
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
fn exec_run(
    core: &mut dyn CoreContext,
    arguments: &[Value],
    span: Span,
) -> Result<Value, Diagnostic> {
    require_arity("HOST.Exec.Run", arguments, 2, span)?;
    let policy = core.host().policy().exec();
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
    if !policy.allowed {
        return Ok(Value::Error {
            code: bn_host_exec::EXEC_POLICY_DENIED,
            message: "HOST.Exec is denied by execution policy".into(),
        });
    }
    if program.is_empty() || program.as_bytes().contains(&0) {
        return Ok(Value::Error {
            code: bn_host_exec::EXEC_INVALID_ARGUMENT,
            message: "program must be non-empty and contain no NUL".into(),
        });
    }
    let mut values = Vec::with_capacity(args.len());
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
                code: bn_host_exec::EXEC_INVALID_ARGUMENT,
                message: "arguments must not contain NUL".into(),
            });
        }
        values.push(value.as_str());
    }
    match bn_host_exec::run(program, &values, &policy) {
        Ok(output) => Ok(Value::Record {
            type_name: "HOST.Exec.Result".into(),
            fields: HashMap::from([
                (
                    "ReturnCode".into(),
                    Value::Integer(i128::from(output.return_code), IntegerType::Int64),
                ),
                ("Stdout".into(), Value::String(output.stdout)),
                ("Stderr".into(), Value::String(output.stderr)),
            ]),
        }),
        Err(failure) => Ok(Value::Error {
            code: failure.code,
            message: failure.message,
        }),
    }
}
