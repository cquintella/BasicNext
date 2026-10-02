// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![allow(clippy::too_many_lines)] // Provider dispatch mirrors BNJson.bn member surface.
#![allow(clippy::cast_precision_loss)] // INTEGER→FLOAT for GetFloat follows BN numeric conversion.

//! `BNJson` — an external library module served through the provider seam.
//! This file marshals `Value`s only. The documents themselves live in
//! `bn_rt::json_abi`, the single table both backends share, so a handle means
//! the same thing interpreted and compiled (W3). Relocated from a
//! provider-owned table in bucket 0.6.1c.

use bn_diag::Diagnostic;
use bn_source::Span;
use bn_types::{FloatType, IntegerType};
use bn_value::{Value, shared_string};

use bn_interp::provider::{CoreContext, Provider};
use bn_interp::{
    require_arity_pub as require_arity, runtime_error_pub as runtime_error, type_mismatch,
};

use bn_rt::json_abi;
use bn_rt::json_error::JsonFailure;

pub const NAME: &str = "BNJson";

#[derive(Debug, Default)]
pub struct JsonProvider;

impl JsonProvider {
    /// The document id behind a `BNJson.Json` argument.
    fn handle(value: &Value, member: &str, span: Span) -> Result<u64, Diagnostic> {
        let Value::Json(id) = value else {
            return Err(type_mismatch(
                "BNJson.Json",
                "non-BNJson.Json value",
                member,
                span,
            ));
        };
        Ok(*id)
    }

    /// The STRING behind argument `index`.
    fn text<'a>(
        arguments: &'a [Value],
        index: usize,
        member: &str,
        span: Span,
    ) -> Result<&'a str, Diagnostic> {
        let Some(Value::String(text)) = arguments.get(index) else {
            return Err(type_mismatch("STRING", "non-STRING value", member, span));
        };
        Ok(text.as_ref())
    }

    fn integer(
        arguments: &[Value],
        index: usize,
        member: &str,
        span: Span,
    ) -> Result<i64, Diagnostic> {
        let Some(Value::Integer(number, _)) = arguments.get(index) else {
            return Err(type_mismatch("INTEGER", "non-INTEGER value", member, span));
        };
        i64::try_from(*number)
            .map_err(|_| type_mismatch("an INTEGER in range", "out-of-range value", member, span))
    }

    fn float(
        arguments: &[Value],
        index: usize,
        member: &str,
        span: Span,
    ) -> Result<f64, Diagnostic> {
        match arguments.get(index) {
            Some(Value::Float(number, _)) => Ok(*number),
            Some(Value::Integer(number, _)) => Ok(*number as f64),
            _ => Err(type_mismatch("FLOAT", "non-FLOAT value", member, span)),
        }
    }

    fn boolean(
        arguments: &[Value],
        index: usize,
        member: &str,
        span: Span,
    ) -> Result<bool, Diagnostic> {
        let Some(Value::Boolean(flag)) = arguments.get(index) else {
            return Err(type_mismatch("BOOLEAN", "non-BOOLEAN value", member, span));
        };
        Ok(*flag)
    }

    /// A `BNJson` result: `ok` of the value, an `Error` value of the
    /// failure (the report the native ABI records), or `USE_AFTER_RELEASE`
    /// for a released handle.
    fn answer<T>(
        method: &str,
        outcome: Result<T, JsonFailure>,
        span: Span,
        ok: impl FnOnce(T) -> Value,
    ) -> Result<Value, Diagnostic> {
        match outcome {
            Ok(value) => Ok(ok(value)),
            Err(JsonFailure::InvalidHandle) => Err(Self::invalid_handle(span)),
            Err(failure) => Ok(Value::error_report(
                failure.code(),
                &format!("BNJson.Json.{method}"),
                failure.message(),
                failure.cause(),
            )),
        }
    }

    /// A VOID member's result.
    fn done(
        method: &str,
        outcome: Result<(), JsonFailure>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        Self::answer(method, outcome, span, |()| Value::Null)
    }

    fn invalid_handle(span: Span) -> Diagnostic {
        runtime_error(
            bn_diag::DiagId::USE_AFTER_RELEASE,
            "BNJson.Json is invalid",
            span,
        )
    }

    /// The JSON value a `Set*` / `Append*` writes: argument `index` read as
    /// the member's type (`SetFloat` → FLOAT, …).
    fn scalar(
        method: &str,
        arguments: &[Value],
        index: usize,
        member: &str,
        span: Span,
    ) -> Result<Result<serde_json::Value, JsonFailure>, Diagnostic> {
        let kind = method
            .trim_start_matches("Set")
            .trim_start_matches("Append")
            .trim_end_matches("At");
        Ok(match kind {
            "String" => Ok(serde_json::Value::String(
                Self::text(arguments, index, member, span)?.to_owned(),
            )),
            "Integer" => Ok(serde_json::Value::from(Self::integer(
                arguments, index, member, span,
            )?)),
            "Float" => json_abi::number(Self::float(arguments, index, member, span)?),
            "Boolean" => Ok(serde_json::Value::Bool(Self::boolean(
                arguments, index, member, span,
            )?)),
            _ => Ok(serde_json::Value::Null),
        })
    }
}

impl Provider for JsonProvider {
    fn call(
        &mut self,
        _core: &mut dyn CoreContext,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = member.rsplit('.').next().unwrap_or_default();
        let string = |text: String| Value::String(shared_string(text.as_str()));
        match method {
            "Parse" => {
                require_arity(member, &arguments, 1, span)?;
                let text = Self::text(&arguments, 0, member, span)?;
                Self::answer(method, json_abi::parse_document(text), span, Value::Json)
            }
            "Stringify" => {
                require_arity(member, &arguments, 1, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                Self::answer(method, json_abi::stringify_document(handle), span, string)
            }
            "Object" => {
                require_arity(member, &arguments, 0, span)?;
                Ok(Value::Json(json_abi::bn_rt_json_object()))
            }
            "Array" => {
                require_arity(member, &arguments, 0, span)?;
                Ok(Value::Json(json_abi::bn_rt_json_array()))
            }
            "Kind" => {
                require_arity(member, &arguments, 1, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let kind = json_abi::kind(handle).ok_or_else(|| Self::invalid_handle(span))?;
                Ok(Value::String(shared_string(kind)))
            }
            "Has" => {
                require_arity(member, &arguments, 2, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let key = Self::text(&arguments, 1, member, span)?;
                Ok(Value::Boolean(json_abi::has(handle, key)))
            }
            "Length" => {
                require_arity(member, &arguments, 1, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                Self::answer(method, json_abi::length(handle), span, |count| {
                    Value::Integer(i128::from(count), IntegerType::Int32)
                })
            }
            "Clone" => {
                require_arity(member, &arguments, 1, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                match json_abi::clone_document(handle) {
                    Some(copy) => Ok(Value::Json(copy)),
                    None => Err(Self::invalid_handle(span)),
                }
            }
            "SetString" | "SetInteger" | "SetFloat" | "SetBoolean" | "SetNull" => {
                let arity = if method == "SetNull" { 2 } else { 3 };
                require_arity(member, &arguments, arity, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let key = Self::text(&arguments, 1, member, span)?;
                let value = Self::scalar(method, &arguments, 2, member, span)?;
                let outcome = value.and_then(|value| json_abi::set_value(handle, key, value));
                Self::done(method, outcome, span)
            }
            "SetStringAt" | "SetIntegerAt" | "SetFloatAt" | "SetBooleanAt" | "SetNullAt" => {
                let arity = if method == "SetNullAt" { 2 } else { 3 };
                require_arity(member, &arguments, arity, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let index = Self::integer(&arguments, 1, member, span)?;
                let value = Self::scalar(method, &arguments, 2, member, span)?;
                let outcome = value.and_then(|value| json_abi::set_at(handle, index, value));
                Self::done(method, outcome, span)
            }
            "AppendString" | "AppendInteger" | "AppendFloat" | "AppendBoolean" | "AppendNull" => {
                let arity = if method == "AppendNull" { 1 } else { 2 };
                require_arity(member, &arguments, arity, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let value = Self::scalar(method, &arguments, 1, member, span)?;
                let outcome = value.and_then(|value| json_abi::append_value(handle, value));
                Self::done(method, outcome, span)
            }
            "SetJson" => {
                require_arity(member, &arguments, 3, span)?;
                let parent = Self::handle(&arguments[0], member, span)?;
                let key = Self::text(&arguments, 1, member, span)?;
                let child = Self::handle(&arguments[2], member, span)?;
                // Move: child handle is consumed on success.
                Self::done(method, json_abi::move_into(parent, key, child), span)
            }
            "SetJsonAt" => {
                require_arity(member, &arguments, 3, span)?;
                let parent = Self::handle(&arguments[0], member, span)?;
                let index = Self::integer(&arguments, 1, member, span)?;
                let child = Self::handle(&arguments[2], member, span)?;
                Self::done(method, json_abi::move_into_at(parent, index, child), span)
            }
            "AppendJson" => {
                require_arity(member, &arguments, 2, span)?;
                let parent = Self::handle(&arguments[0], member, span)?;
                let child = Self::handle(&arguments[1], member, span)?;
                Self::done(method, json_abi::append_moved(parent, child), span)
            }
            "GetString" | "GetInteger" | "GetFloat" | "GetBoolean" => {
                require_arity(member, &arguments, 2, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let key = Self::text(&arguments, 1, member, span)?;
                let (expected, project) = read_as(method);
                let outcome = json_abi::get_value(handle, key, expected, project);
                Self::answer(method, outcome, span, std::convert::identity)
            }
            "GetStringAt" | "GetIntegerAt" | "GetFloatAt" | "GetBooleanAt" => {
                require_arity(member, &arguments, 2, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let index = Self::integer(&arguments, 1, member, span)?;
                let (expected, project) = read_as(method.trim_end_matches("At"));
                let outcome = json_abi::element(handle, index, expected, project);
                Self::answer(method, outcome, span, std::convert::identity)
            }
            "GetJson" => {
                require_arity(member, &arguments, 2, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let key = Self::text(&arguments, 1, member, span)?;
                Self::answer(method, json_abi::get_json(handle, key), span, Value::Json)
            }
            "GetJsonAt" => {
                require_arity(member, &arguments, 2, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let index = Self::integer(&arguments, 1, member, span)?;
                Self::answer(
                    method,
                    json_abi::get_json_at(handle, index),
                    span,
                    Value::Json,
                )
            }
            "CONSTRUCTOR" => Ok(Value::Null),
            _ => Self::answer::<Value>(method, Err(JsonFailure::Unavailable), span, |value| value),
        }
    }

    fn allocate(&mut self, class: &str, _span: Span) -> Option<Result<Value, Diagnostic>> {
        (class.rsplit('.').next() == Some("Json"))
            .then(|| Ok(Value::Json(json_abi::store(serde_json::Value::Null))))
    }

    fn release(&mut self, value: &Value, span: Span) -> Option<Result<(), Diagnostic>> {
        let Value::Json(id) = value else {
            return None;
        };
        Some(if json_abi::release(*id) {
            Ok(())
        } else {
            Err(runtime_error(
                bn_diag::DiagId::DOUBLE_RELEASE,
                "BNJson.Json was already deleted",
                span,
            ))
        })
    }
}

/// The type name and projection of a scalar `Get*` member: the BN value of a
/// JSON value of that type, or `None` for another kind. It runs under the
/// document table's lock, so it must not touch the table.
fn read_as(method: &str) -> (&'static str, fn(&serde_json::Value) -> Option<Value>) {
    match method {
        "GetString" => ("STRING", |value| {
            value
                .as_str()
                .map(|text| Value::String(shared_string(text)))
        }),
        "GetInteger" => ("INTEGER", |value| {
            value
                .as_i64()
                .map(|number| Value::Integer(i128::from(number), IntegerType::Int64))
        }),
        "GetFloat" => ("FLOAT", |value| {
            value
                .as_f64()
                .map(|number| Value::Float(number, FloatType::Float64))
        }),
        _ => ("BOOLEAN", |value| value.as_bool().map(Value::Boolean)),
    }
}

#[cfg(test)]
mod tests {
    /// `Json.*` codes in `modules/bn/BNJson.bn` are the ones both backends
    /// put in `Error.Code`.
    #[test]
    fn module_constants_match_the_runtime_codes() {
        let module = include_str!("../../../modules/bn/BNJson.bn");
        for (name, value) in bn_types::error_codes::json::ALL {
            let line = format!("EXPORT CONST {name} AS INTEGER = {value}");
            assert!(module.contains(&line), "BNJson.bn lacks `{line}`");
        }
    }
}
