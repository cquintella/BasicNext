// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.Random` — xorshift over the seed the host environment carries (a
//! forked dispatch worker derives its own seed).
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
use crate::runtime::{
    integer_pub as integer, require_arity_pub as require_arity, runtime_error_pub as runtime_error,
};
use crate::types::FloatType;

pub const NAME: &str = "Random";

pub struct RandomProvider;

impl Provider for RandomProvider {
    fn call(
        &mut self,
        core: &mut dyn CoreContext,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let name = format!("HOST.Random.{member}");
        let arguments = &arguments;
        let name = name.as_str();
        match member {
            "Random" => {
                require_arity(name, arguments, 0, span)?;
                let mut state = core
                    .host()
                    .random_state()
                    .load(std::sync::atomic::Ordering::Relaxed);
                state ^= state >> 12;
                state ^= state << 25;
                state ^= state >> 27;
                state = state.wrapping_mul(0x2545_F491_4F6C_DD1D);
                core.host()
                    .random_state()
                    .store(state, std::sync::atomic::Ordering::Relaxed);
                Ok(Value::Float(
                    (state >> 11) as f64 / 9_007_199_254_740_992.0,
                    FloatType::Float64,
                ))
            }
            "Seed" => {
                require_arity(name, arguments, 1, span)?;
                let (seed, _) = integer(&arguments[0], span)?;
                core.host().random_state().store(
                    seed as u64 | u64::from(seed == 0),
                    std::sync::atomic::Ordering::Relaxed,
                );
                Ok(Value::Null)
            }
            _ => Err(runtime_error(
                crate::diagnostic::DiagId::HOST_CAPABILITY_UNAVAILABLE,
                format!("host function '{name}' is not available"),
                span,
            )),
        }
    }
}
