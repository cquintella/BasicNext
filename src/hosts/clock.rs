// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.Clock` — wall clock and monotonic timer, read from the host policy
//! (`HostEnv` fixes them for tests).
#![allow(
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)] // Moved verbatim from the core (bucket 0.5.1d 1.5).

use bn_diag::Diagnostic;
use bn_source::Span;
use bn_value::Value;

use crate::runtime::provider::{CoreContext, Provider};
use crate::runtime::{require_arity_pub as require_arity, runtime_error_pub as runtime_error};
use crate::types::IntegerType;

pub const NAME: &str = "Clock";

pub struct ClockProvider;

impl Provider for ClockProvider {
    fn call(
        &mut self,
        core: &mut dyn CoreContext,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let name = format!("HOST.Clock.{member}");
        let arguments = &arguments;
        let name = name.as_str();
        match member {
            "Now" => {
                require_arity(name, arguments, 0, span)?;
                Ok(Value::Integer(
                    i128::from(core.host().timestamp_ms()),
                    IntegerType::Int64,
                ))
            }
            "Timer" => {
                require_arity(name, arguments, 0, span)?;
                Ok(Value::Integer(
                    i128::from(core.host().monotonic_ns()),
                    IntegerType::Int64,
                ))
            }
            _ => Err(runtime_error(
                crate::diagnostic::DiagId::HOST_CAPABILITY_UNAVAILABLE,
                format!("host function '{name}' is not available"),
                span,
            )),
        }
    }
}
