// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNJson` — an external library module served through the provider seam.
//! The provider owns the parsed-document table; the core only sees
//! `Value::Json(handle)`.

mod json;

use std::collections::HashMap;

use bn_diag::Diagnostic;
use bn_source::Span;
use bn_value::Value;

use bn_interp::provider::{CoreContext, Provider};
use bn_interp::{
    require_arity_pub as require_arity, runtime_error_pub as runtime_error, type_mismatch,
};

pub const NAME: &str = "BNJson";

pub struct JsonProvider {
    values: HashMap<u64, crate::json::Value>,
    next: u64,
}

impl Default for JsonProvider {
    fn default() -> Self {
        Self {
            values: HashMap::new(),
            next: 1,
        }
    }
}

impl JsonProvider {
    fn insert(&mut self, value: crate::json::Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.values.insert(id, value);
        Value::Json(id)
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
                let Value::String(text) = &arguments[0] else {
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "BNJson.Parse input",
                        span,
                    ));
                };
                let parsed = crate::json::parse(text).map_err(|message| {
                    runtime_error(bn_diag::DiagId::INVALID_JSON, message, span)
                })?;
                Ok(self.insert(parsed))
            }
            "Stringify" => {
                require_arity(member, &arguments, 1, span)?;
                let Value::Json(id) = arguments[0] else {
                    return Err(type_mismatch(
                        "BNJson.Json",
                        "non-BNJson.Json value",
                        "BNJson.Stringify input",
                        span,
                    ));
                };
                let value = self.values.get(&id).ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        "BNJson.Json is invalid",
                        span,
                    )
                })?;
                let text = crate::json::stringify(value).map_err(|message| {
                    runtime_error(bn_diag::DiagId::INVALID_JSON, message, span)
                })?;
                Ok(Value::String(text))
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
            .then(|| Ok(self.insert(crate::json::Value::Null)))
    }

    fn release(&mut self, value: &Value, span: Span) -> Option<Result<(), Diagnostic>> {
        let Value::Json(id) = value else {
            return None;
        };
        Some(if self.values.remove(id).is_some() {
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
