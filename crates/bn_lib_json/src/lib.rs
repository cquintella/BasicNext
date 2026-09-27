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

use bn_rt::json_abi::{
    self, BN_JSON_INVALID_HANDLE, BN_JSON_NOT_FOUND, BN_JSON_OK, BN_JSON_TOO_LARGE,
};

pub const NAME: &str = "BNJson";

#[derive(Default)]
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

    /// One place where a write is turned into `VOID OR Error`, so the depth
    /// rejection reads the same for every scalar setter.
    fn write(
        handle: u64,
        key: &str,
        value: serde_json::Value,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let code = match value {
            serde_json::Value::String(ref text) => json_abi::set_string(handle, key, text),
            _ => json_abi::set_value_public(handle, key, value),
        };
        Self::status_void(code, span, "object")
    }

    fn append(handle: u64, value: serde_json::Value, span: Span) -> Result<Value, Diagnostic> {
        Self::status_void(json_abi::append_value_public(handle, value), span, "array")
    }

    fn write_at(
        handle: u64,
        index: i64,
        value: serde_json::Value,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        Self::status_void(json_abi::set_at_public(handle, index, value), span, "array")
    }

    fn status_void(code: i32, span: Span, expected: &str) -> Result<Value, Diagnostic> {
        match code {
            BN_JSON_OK => Ok(Value::Null),
            BN_JSON_TOO_LARGE => Ok(Value::Error {
                code: 1,
                message: "BNJson write would exceed the depth limit".into(),
            }),
            BN_JSON_INVALID_HANDLE => Err(Self::invalid_handle(span)),
            BN_JSON_NOT_FOUND => Ok(Value::Error {
                code: 1,
                message: "BNJson: index out of range".into(),
            }),
            _ => Ok(Value::Error {
                code: 1,
                message: format!("BNJson write target is not an {expected}").into(),
            }),
        }
    }

    /// A missing key, or a key holding another kind. Never a default value.
    fn not_found(expected: &str) -> Value {
        Value::Error {
            code: 1,
            message: format!("BNJson: missing key or value is not a {expected}").into(),
        }
    }

    fn invalid_handle(span: Span) -> Diagnostic {
        runtime_error(
            bn_diag::DiagId::USE_AFTER_RELEASE,
            "BNJson.Json is invalid",
            span,
        )
    }

    fn float_number(value: f64) -> Result<serde_json::Value, Value> {
        serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .ok_or(Value::Error {
                code: 1,
                message: "BNJson: non-finite FLOAT is not allowed".into(),
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
        match method {
            "Parse" => {
                require_arity(member, &arguments, 1, span)?;
                let text = Self::text(&arguments, 0, member, span)?;
                match bn_rt::json::parse(text) {
                    Ok(value) => Ok(Value::Json(json_abi::store(value))),
                    Err(message) => {
                        Err(runtime_error(bn_diag::DiagId::INVALID_JSON, &message, span))
                    }
                }
            }
            "Stringify" => {
                require_arity(member, &arguments, 1, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let document =
                    json_abi::document(handle).ok_or_else(|| Self::invalid_handle(span))?;
                match bn_rt::json::stringify(&document) {
                    Ok(text) => Ok(Value::String(shared_string(text.as_str()))),
                    Err(message) => {
                        Err(runtime_error(bn_diag::DiagId::INVALID_JSON, &message, span))
                    }
                }
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
                match json_abi::length(handle) {
                    -1 => Ok(Value::Error {
                        code: 1,
                        message: "BNJson.Length: document is not an object or array".into(),
                    }),
                    count => Ok(Value::Integer(i128::from(count), IntegerType::Int32)),
                }
            }
            "Clone" => {
                require_arity(member, &arguments, 1, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                match json_abi::clone_document(handle) {
                    Some(copy) => Ok(Value::Json(copy)),
                    None => Err(Self::invalid_handle(span)),
                }
            }
            "SetString" => {
                require_arity(member, &arguments, 3, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let key = Self::text(&arguments, 1, member, span)?;
                let value = Self::text(&arguments, 2, member, span)?;
                Self::write(
                    handle,
                    key,
                    serde_json::Value::String(value.to_owned()),
                    span,
                )
            }
            "SetInteger" => {
                require_arity(member, &arguments, 3, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let key = Self::text(&arguments, 1, member, span)?;
                let value = Self::integer(&arguments, 2, member, span)?;
                Self::write(handle, key, serde_json::Value::from(value), span)
            }
            "SetFloat" => {
                require_arity(member, &arguments, 3, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let key = Self::text(&arguments, 1, member, span)?;
                let value = Self::float(&arguments, 2, member, span)?;
                match Self::float_number(value) {
                    Ok(number) => Self::write(handle, key, number, span),
                    Err(error) => Ok(error),
                }
            }
            "SetBoolean" => {
                require_arity(member, &arguments, 3, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let key = Self::text(&arguments, 1, member, span)?;
                let value = Self::boolean(&arguments, 2, member, span)?;
                Self::write(handle, key, serde_json::Value::Bool(value), span)
            }
            "SetNull" => {
                require_arity(member, &arguments, 2, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let key = Self::text(&arguments, 1, member, span)?;
                Self::write(handle, key, serde_json::Value::Null, span)
            }
            "SetJson" => {
                require_arity(member, &arguments, 3, span)?;
                let parent = Self::handle(&arguments[0], member, span)?;
                let key = Self::text(&arguments, 1, member, span)?;
                let child = Self::handle(&arguments[2], member, span)?;
                // Move: child handle is consumed on success.
                Self::status_void(json_abi::move_into(parent, key, child), span, "object")
            }
            "GetString" => {
                require_arity(member, &arguments, 2, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let key = Self::text(&arguments, 1, member, span)?;
                match json_abi::get_string(handle, key) {
                    Ok(text) => Ok(Value::String(shared_string(text.as_str()))),
                    Err(BN_JSON_INVALID_HANDLE) => Err(Self::invalid_handle(span)),
                    Err(_) => Ok(Self::not_found("STRING")),
                }
            }
            "GetInteger" => {
                require_arity(member, &arguments, 2, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let key = Self::text(&arguments, 1, member, span)?;
                let document =
                    json_abi::document(handle).ok_or_else(|| Self::invalid_handle(span))?;
                match document.get(key).and_then(serde_json::Value::as_i64) {
                    Some(number) => Ok(Value::Integer(i128::from(number), IntegerType::Int64)),
                    None => Ok(Self::not_found("INTEGER")),
                }
            }
            "GetFloat" => {
                require_arity(member, &arguments, 2, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let key = Self::text(&arguments, 1, member, span)?;
                let document =
                    json_abi::document(handle).ok_or_else(|| Self::invalid_handle(span))?;
                match document.get(key).and_then(serde_json::Value::as_f64) {
                    Some(number) => Ok(Value::Float(number, FloatType::Float64)),
                    None => Ok(Self::not_found("FLOAT")),
                }
            }
            "GetBoolean" => {
                require_arity(member, &arguments, 2, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let key = Self::text(&arguments, 1, member, span)?;
                let document =
                    json_abi::document(handle).ok_or_else(|| Self::invalid_handle(span))?;
                match document.get(key).and_then(serde_json::Value::as_bool) {
                    Some(flag) => Ok(Value::Boolean(flag)),
                    None => Ok(Self::not_found("BOOLEAN")),
                }
            }
            "GetJson" => {
                require_arity(member, &arguments, 2, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let key = Self::text(&arguments, 1, member, span)?;
                match json_abi::get_json(handle, key) {
                    Ok(copy) => Ok(Value::Json(copy)),
                    Err(BN_JSON_INVALID_HANDLE) => Err(Self::invalid_handle(span)),
                    Err(_) => Ok(Self::not_found("Json")),
                }
            }
            "AppendString" => {
                require_arity(member, &arguments, 2, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let value = Self::text(&arguments, 1, member, span)?;
                Self::append(handle, serde_json::Value::String(value.to_owned()), span)
            }
            "AppendInteger" => {
                require_arity(member, &arguments, 2, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let value = Self::integer(&arguments, 1, member, span)?;
                Self::append(handle, serde_json::Value::from(value), span)
            }
            "AppendFloat" => {
                require_arity(member, &arguments, 2, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let value = Self::float(&arguments, 1, member, span)?;
                match Self::float_number(value) {
                    Ok(number) => Self::append(handle, number, span),
                    Err(error) => Ok(error),
                }
            }
            "AppendBoolean" => {
                require_arity(member, &arguments, 2, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let value = Self::boolean(&arguments, 1, member, span)?;
                Self::append(handle, serde_json::Value::Bool(value), span)
            }
            "AppendNull" => {
                require_arity(member, &arguments, 1, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                Self::append(handle, serde_json::Value::Null, span)
            }
            "AppendJson" => {
                require_arity(member, &arguments, 2, span)?;
                let parent = Self::handle(&arguments[0], member, span)?;
                let child = Self::handle(&arguments[1], member, span)?;
                Self::status_void(json_abi::append_moved(parent, child), span, "array")
            }
            "GetStringAt" => {
                require_arity(member, &arguments, 2, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let index = Self::integer(&arguments, 1, member, span)?;
                match json_abi::element_at(handle, index)
                    .as_ref()
                    .and_then(serde_json::Value::as_str)
                {
                    Some(text) => Ok(Value::String(shared_string(text))),
                    None => {
                        // Distinguish invalid handle from OOB / wrong kind.
                        if json_abi::document(handle).is_none() {
                            Err(Self::invalid_handle(span))
                        } else {
                            Ok(Self::not_found("STRING"))
                        }
                    }
                }
            }
            "GetIntegerAt" => {
                require_arity(member, &arguments, 2, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let index = Self::integer(&arguments, 1, member, span)?;
                match json_abi::element_at(handle, index)
                    .as_ref()
                    .and_then(serde_json::Value::as_i64)
                {
                    Some(number) => Ok(Value::Integer(i128::from(number), IntegerType::Int64)),
                    None => {
                        if json_abi::document(handle).is_none() {
                            Err(Self::invalid_handle(span))
                        } else {
                            Ok(Self::not_found("INTEGER"))
                        }
                    }
                }
            }
            "GetFloatAt" => {
                require_arity(member, &arguments, 2, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let index = Self::integer(&arguments, 1, member, span)?;
                match json_abi::element_at(handle, index)
                    .as_ref()
                    .and_then(serde_json::Value::as_f64)
                {
                    Some(number) => Ok(Value::Float(number, FloatType::Float64)),
                    None => {
                        if json_abi::document(handle).is_none() {
                            Err(Self::invalid_handle(span))
                        } else {
                            Ok(Self::not_found("FLOAT"))
                        }
                    }
                }
            }
            "GetBooleanAt" => {
                require_arity(member, &arguments, 2, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let index = Self::integer(&arguments, 1, member, span)?;
                match json_abi::element_at(handle, index)
                    .as_ref()
                    .and_then(serde_json::Value::as_bool)
                {
                    Some(flag) => Ok(Value::Boolean(flag)),
                    None => {
                        if json_abi::document(handle).is_none() {
                            Err(Self::invalid_handle(span))
                        } else {
                            Ok(Self::not_found("BOOLEAN"))
                        }
                    }
                }
            }
            "GetJsonAt" => {
                require_arity(member, &arguments, 2, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let index = Self::integer(&arguments, 1, member, span)?;
                match json_abi::get_json_at(handle, index) {
                    Ok(copy) => Ok(Value::Json(copy)),
                    Err(BN_JSON_INVALID_HANDLE) => Err(Self::invalid_handle(span)),
                    Err(_) => Ok(Self::not_found("Json")),
                }
            }
            "SetStringAt" => {
                require_arity(member, &arguments, 3, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let index = Self::integer(&arguments, 1, member, span)?;
                let value = Self::text(&arguments, 2, member, span)?;
                Self::write_at(
                    handle,
                    index,
                    serde_json::Value::String(value.to_owned()),
                    span,
                )
            }
            "SetIntegerAt" => {
                require_arity(member, &arguments, 3, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let index = Self::integer(&arguments, 1, member, span)?;
                let value = Self::integer(&arguments, 2, member, span)?;
                Self::write_at(handle, index, serde_json::Value::from(value), span)
            }
            "SetFloatAt" => {
                require_arity(member, &arguments, 3, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let index = Self::integer(&arguments, 1, member, span)?;
                let value = Self::float(&arguments, 2, member, span)?;
                match Self::float_number(value) {
                    Ok(number) => Self::write_at(handle, index, number, span),
                    Err(error) => Ok(error),
                }
            }
            "SetBooleanAt" => {
                require_arity(member, &arguments, 3, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let index = Self::integer(&arguments, 1, member, span)?;
                let value = Self::boolean(&arguments, 2, member, span)?;
                Self::write_at(handle, index, serde_json::Value::Bool(value), span)
            }
            "SetNullAt" => {
                require_arity(member, &arguments, 2, span)?;
                let handle = Self::handle(&arguments[0], member, span)?;
                let index = Self::integer(&arguments, 1, member, span)?;
                Self::write_at(handle, index, serde_json::Value::Null, span)
            }
            "SetJsonAt" => {
                require_arity(member, &arguments, 3, span)?;
                let parent = Self::handle(&arguments[0], member, span)?;
                let index = Self::integer(&arguments, 1, member, span)?;
                let child = Self::handle(&arguments[2], member, span)?;
                Self::status_void(json_abi::move_into_at(parent, index, child), span, "array")
            }
            "CONSTRUCTOR" => Ok(Value::Null),
            _ => Ok(Value::Error {
                code: 1,
                message: "BNJson operation unavailable".into(),
            }),
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
