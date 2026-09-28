// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.Console` — terminal control on the program's output stream, through
//! `bn_rt`'s console primitives (shared with native binaries).
#![allow(
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)] // Moved verbatim from the core (bucket 0.5.1d 1.5).

use bn_diag::Diagnostic;
use bn_source::Span;
use bn_value::Value;

use bn_interp::provider::{CoreContext, Provider};
use bn_interp::{
    integer_pub as integer, require_arity_pub as require_arity, runtime_error_pub as runtime_error,
    type_mismatch,
};
use bn_types::IntegerType;

pub const NAME: &str = "Console";

pub struct ConsoleProvider;

impl Provider for ConsoleProvider {
    fn call(
        &mut self,
        core: &mut dyn CoreContext,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let name = format!("HOST.Console.{member}");
        let arguments = &arguments;
        let name = name.as_str();
        match member {
            "Cls" => {
                require_arity(name, arguments, 0, span)?;
                bn_rt::cls(core.output()).map_err(|error| console_runtime_error(&error, span))?;
                Ok(Value::Null)
            }
            "Beep" => {
                require_arity(name, arguments, 0, span)?;
                bn_rt::beep(core.output()).map_err(|error| console_runtime_error(&error, span))?;
                Ok(Value::Null)
            }
            "PrintAt" => {
                require_arity(name, arguments, 3, span)?;
                let (column, _) = integer(&arguments[0], span)?;
                let (row, _) = integer(&arguments[1], span)?;
                let Value::String(text) = &arguments[2] else {
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "HOST.Console.PrintAt text",
                        span,
                    ));
                };
                bn_rt::print_at(core.output(), column, row, text)
                    .map_err(|error| console_runtime_error(&error, span))?;
                Ok(Value::Null)
            }
            "NumCols" => {
                require_arity(name, arguments, 0, span)?;
                match bn_rt::num_cols() {
                    Ok(value) => Ok(Value::Integer(i128::from(value), IntegerType::Int32)),
                    Err(error) => Err(console_runtime_error(&error, span)),
                }
            }
            "NumRows" => {
                require_arity(name, arguments, 0, span)?;
                match bn_rt::num_rows() {
                    Ok(value) => Ok(Value::Integer(i128::from(value), IntegerType::Int32)),
                    Err(error) => Err(console_runtime_error(&error, span)),
                }
            }
            _ => Err(runtime_error(
                bn_diag::DiagId::HOST_CAPABILITY_UNAVAILABLE,
                format!("host function '{name}' is not available"),
                span,
            )),
        }
    }
}

/// The diagnostic of a console failure, from the record the native runtime
/// prints too (`ConsoleError::failure`), so both backends show one text.
fn console_runtime_error(error: &bn_rt::ConsoleError, span: Span) -> Diagnostic {
    let failure = error.failure();
    let id = bn_diag::DiagId::from_code(failure.code).expect("console failure code is registered");
    Diagnostic::structured(
        id,
        failure
            .facts
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value.into()))
            .collect(),
        vec![bn_diag::Label {
            span,
            style: bn_diag::LabelStyle::Primary,
            text: None,
        }],
    )
    .expect("console failure facts match the identity schema")
}
