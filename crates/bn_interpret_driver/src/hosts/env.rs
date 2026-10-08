// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.Env` provider for the interpreter: validates parameters, reads policy,
//! and delegates to `bn_host_env::get` and `bn_host_env::has`.

use bn_diag::Diagnostic;
use bn_source::Span;
use bn_value::{Value, shared_string};

use bn_interp::provider::{CoreContext, Provider};
use bn_interp::{
    require_arity_pub as require_arity, runtime_error_pub as runtime_error, type_mismatch,
};

pub const NAME: &str = "Env";

#[derive(Debug, Default)]
pub struct EnvProvider;

impl Provider for EnvProvider {
    fn call(
        &mut self,
        core: &mut dyn CoreContext,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let name = format!("HOST.Env.{member}");
        let name = name.as_str();
        match member {
            "Get" => env_get(core, &arguments, span),
            "Has" => env_has(core, &arguments, span),
            _ => Err(runtime_error(
                bn_diag::DiagId::HOST_CAPABILITY_UNAVAILABLE,
                format!("host function '{name}' is not available"),
                span,
            )),
        }
    }
}

fn env_get(
    core: &mut dyn CoreContext,
    arguments: &[Value],
    span: Span,
) -> Result<Value, Diagnostic> {
    require_arity("HOST.Env.Get", arguments, 1, span)?;
    let policy = core.host().policy().env();
    let Value::String(name) = &arguments[0] else {
        return Err(type_mismatch(
            "STRING",
            "non-STRING value",
            "HOST.Env.Get name",
            span,
        ));
    };

    match bn_host_env::get(name, &policy) {
        Ok(val) => Ok(Value::String(shared_string(val))),
        Err(failure) => Ok(Value::error_report(
            failure.code,
            failure.operation,
            failure.message,
            failure.cause,
        )),
    }
}

fn env_has(
    core: &mut dyn CoreContext,
    arguments: &[Value],
    span: Span,
) -> Result<Value, Diagnostic> {
    require_arity("HOST.Env.Has", arguments, 1, span)?;
    let policy = core.host().policy().env();
    let Value::String(name) = &arguments[0] else {
        return Err(type_mismatch(
            "STRING",
            "non-STRING value",
            "HOST.Env.Has name",
            span,
        ));
    };

    match bn_host_env::has(name, &policy) {
        Ok(present) => Ok(Value::Boolean(present)),
        Err(failure) => Ok(Value::error_report(
            failure.code,
            failure.operation,
            failure.message,
            failure.cause,
        )),
    }
}
