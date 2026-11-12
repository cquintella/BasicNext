// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNWeb` client transfer methods, `ClientReply`, `Headers`, `Query`, `Fields`,
//! and `Form` members for the interpreter.

#![allow(clippy::wildcard_imports)]
use super::*;

fn alloc_obj(
    core: &mut dyn CoreContext,
    class_suffix: &str,
    span: Span,
) -> Result<(Handle, Value), Diagnostic> {
    let p = "BN";
    let m = "Web";
    let class_name = format!("{p}{m}.{class_suffix}");
    let obj = core.allocate_object(&class_name, span)?;
    let Value::Object { handle, .. } = obj else {
        unreachable!()
    };
    Ok((
        handle,
        Value::Object {
            handle,
            class: class_name.into(),
        },
    ))
}

pub(super) fn arg_err(expected: &str, span: Span) -> Diagnostic {
    type_mismatch(expected, "Invalid", "Argument", span)
}

pub(super) fn recv_err(expected: &str, span: Span) -> Diagnostic {
    type_mismatch(expected, "Invalid", "Receiver", span)
}

impl WebProvider {
    pub(super) fn web_client_reply_call(
        &mut self,
        _core: &mut dyn CoreContext,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = name.rsplit('.').next().unwrap_or_default();
        if method == "CONSTRUCTOR" {
            let Some(Value::Object { handle, .. }) = arguments.first() else {
                return Err(recv_err("ClientReply", span));
            };
            self.client_replies
                .insert(*handle, crate::web::ClientReply::default());
            return Ok(Value::Null);
        }

        let Some(Value::Object { handle, .. }) = arguments.first() else {
            return Err(recv_err("ClientReply", span));
        };
        let Some(reply) = self.client_replies.get(handle) else {
            return Ok(failed(
                name,
                &WebFailure::InvalidArgument("stale_reply".into()),
            ));
        };

        match method {
            "Status" => {
                require_arity(name, arguments, 1, span)?;
                Ok(Value::Integer(i128::from(reply.status), IntegerType::Int32))
            }
            "HttpVersion" => {
                require_arity(name, arguments, 1, span)?;
                Ok(Value::String(shared_string(reply.http_version())))
            }
            "BodySize" => {
                require_arity(name, arguments, 1, span)?;
                Ok(Value::Integer(
                    i128::try_from(reply.body_size).unwrap_or(i128::MAX),
                    IntegerType::Int32,
                ))
            }
            "TotalMs" => {
                require_arity(name, arguments, 1, span)?;
                Ok(Value::Integer(
                    i128::from(reply.total_ms),
                    IntegerType::Int32,
                ))
            }
            "ConnectMs" => {
                require_arity(name, arguments, 1, span)?;
                Ok(Value::Integer(
                    i128::from(reply.connect_ms),
                    IntegerType::Int32,
                ))
            }
            "Header" => {
                require_arity(name, arguments, 2, span)?;
                let Value::String(header_name) = &arguments[1] else {
                    return Err(arg_err("String", span));
                };
                match reply.header(header_name) {
                    Some(val) => Ok(Value::String(shared_string(val))),
                    None => Ok(failed(
                        name,
                        &WebFailure::HeaderNotFound(header_name.to_string()),
                    )),
                }
            }
            "HeaderCount" => {
                require_arity(name, arguments, 1, span)?;
                Ok(Value::Integer(
                    i128::try_from(reply.header_count()).unwrap_or(0),
                    IntegerType::Int32,
                ))
            }
            "HeaderName" => {
                require_arity(name, arguments, 2, span)?;
                let Value::Integer(idx, _) = arguments[1] else {
                    return Err(arg_err("Integer", span));
                };
                let index = usize::try_from(idx).unwrap_or(usize::MAX);
                match reply.header_name(index) {
                    Some(n) => Ok(Value::String(shared_string(n))),
                    None => Ok(failed(
                        name,
                        &WebFailure::IndexOutOfRange {
                            index: idx,
                            count: reply.header_count(),
                        },
                    )),
                }
            }
            "HeaderValue" => {
                require_arity(name, arguments, 2, span)?;
                let Value::Integer(idx, _) = arguments[1] else {
                    return Err(arg_err("Integer", span));
                };
                let index = usize::try_from(idx).unwrap_or(usize::MAX);
                match reply.header_value(index) {
                    Some(v) => Ok(Value::String(shared_string(v))),
                    None => Ok(failed(
                        name,
                        &WebFailure::IndexOutOfRange {
                            index: idx,
                            count: reply.header_count(),
                        },
                    )),
                }
            }
            "BodyText" => {
                require_arity(name, arguments, 2, span)?;
                let Value::Integer(max, _) = arguments[1] else {
                    return Err(arg_err("Integer", span));
                };
                let maximum = maximum_of(max);
                match reply.body_text(maximum) {
                    Ok(text) => Ok(Value::String(shared_string(text.as_str()))),
                    Err(err) => Ok(failed(name, &err)),
                }
            }
            "BodyBytes" => {
                require_arity(name, arguments, 2, span)?;
                let Value::Integer(max, _) = arguments[1] else {
                    return Err(arg_err("Integer", span));
                };
                let maximum = maximum_of(max);
                match reply.body_bytes(maximum) {
                    Ok(bytes) => {
                        let val = bn_lib_crypto::insert_crypto_bytes(bytes.to_vec());
                        Ok(val)
                    }
                    Err(err) => Ok(failed(name, &err)),
                }
            }
            "BodyDecodedText" => {
                require_arity(name, arguments, 2, span)?;
                let Value::Integer(max, _) = arguments[1] else {
                    return Err(arg_err("Integer", span));
                };
                let maximum = maximum_of(max);
                match reply.body_decoded_text(maximum) {
                    Ok(text) => Ok(Value::String(shared_string(text.as_str()))),
                    Err(err) => Ok(failed(name, &err)),
                }
            }
            "BodyDecodedBytes" => {
                require_arity(name, arguments, 2, span)?;
                let Value::Integer(max, _) = arguments[1] else {
                    return Err(arg_err("Integer", span));
                };
                let maximum = maximum_of(max);
                match reply.body_decoded_bytes(maximum) {
                    Ok(bytes) => {
                        let val = bn_lib_crypto::insert_crypto_bytes(bytes);
                        Ok(val)
                    }
                    Err(err) => Ok(failed(name, &err)),
                }
            }
            "PeerAddress" => {
                require_arity(name, arguments, 1, span)?;
                match reply.peer_address() {
                    Some(addr) => Ok(address_value(addr)),
                    None => Ok(failed(
                        name,
                        &WebFailure::NotFound("peer_address_not_found".into()),
                    )),
                }
            }
            _ => Ok(Value::Null),
        }
    }

    pub(super) fn web_headers_call(
        &mut self,
        core: &mut dyn CoreContext,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = name.rsplit('.').next().unwrap_or_default();
        if method == "New" {
            let (handle, obj) = alloc_obj(core, "Headers", span)?;
            self.headers.insert(handle, crate::web::Headers::default());
            return Ok(obj);
        }
        if method == "CONSTRUCTOR" {
            let Some(Value::Object { handle, .. }) = arguments.first() else {
                return Err(recv_err("Headers", span));
            };
            self.headers.insert(*handle, crate::web::Headers::default());
            return Ok(Value::Null);
        }

        let Some(Value::Object { handle, .. }) = arguments.first() else {
            return Err(recv_err("Headers", span));
        };
        let Some(headers) = self.headers.get_mut(handle) else {
            return Ok(failed(
                name,
                &WebFailure::InvalidArgument("stale_headers".into()),
            ));
        };

        match method {
            "Add" => {
                require_arity(name, arguments, 3, span)?;
                let (Value::String(k), Value::String(v)) = (&arguments[1], &arguments[2]) else {
                    return Err(arg_err("String", span));
                };
                headers.add(k.to_string(), v.to_string());
                Ok(Value::Null)
            }
            "Set" => {
                require_arity(name, arguments, 3, span)?;
                let (Value::String(k), Value::String(v)) = (&arguments[1], &arguments[2]) else {
                    return Err(arg_err("String", span));
                };
                headers.set(k.to_string(), v.to_string());
                Ok(Value::Null)
            }
            "Count" => {
                require_arity(name, arguments, 1, span)?;
                Ok(Value::Integer(
                    i128::try_from(headers.count()).unwrap_or(0),
                    IntegerType::Int32,
                ))
            }
            "Get" => {
                require_arity(name, arguments, 2, span)?;
                let Value::String(k) = &arguments[1] else {
                    return Err(arg_err("String", span));
                };
                match headers.get(k) {
                    Some(v) => Ok(Value::String(shared_string(v))),
                    None => Ok(failed(name, &WebFailure::HeaderNotFound(k.to_string()))),
                }
            }
            _ => Ok(Value::Null),
        }
    }

    pub(super) fn web_query_call(
        &mut self,
        core: &mut dyn CoreContext,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = name.rsplit('.').next().unwrap_or_default();
        if method == "New" {
            let (handle, obj) = alloc_obj(core, "Query", span)?;
            self.queries.insert(handle, crate::web::Query::default());
            return Ok(obj);
        }
        if method == "CONSTRUCTOR" {
            let Some(Value::Object { handle, .. }) = arguments.first() else {
                return Err(recv_err("Query", span));
            };
            self.queries.insert(*handle, crate::web::Query::default());
            return Ok(Value::Null);
        }

        let Some(Value::Object { handle, .. }) = arguments.first() else {
            return Err(recv_err("Query", span));
        };
        let Some(query) = self.queries.get_mut(handle) else {
            return Ok(failed(
                name,
                &WebFailure::InvalidArgument("stale_query".into()),
            ));
        };

        match method {
            "Add" => {
                require_arity(name, arguments, 3, span)?;
                let (Value::String(k), Value::String(v)) = (&arguments[1], &arguments[2]) else {
                    return Err(arg_err("String", span));
                };
                query.add(k.to_string(), v.to_string());
                Ok(Value::Null)
            }
            "Encode" => {
                require_arity(name, arguments, 1, span)?;
                let enc = query.encode();
                Ok(Value::String(shared_string(enc.as_str())))
            }
            _ => Ok(Value::Null),
        }
    }

    pub(super) fn web_fields_call(
        &mut self,
        core: &mut dyn CoreContext,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = name.rsplit('.').next().unwrap_or_default();
        if method == "New" {
            let (handle, obj) = alloc_obj(core, "Fields", span)?;
            self.fields.insert(handle, crate::web::Fields::default());
            return Ok(obj);
        }
        if method == "CONSTRUCTOR" {
            let Some(Value::Object { handle, .. }) = arguments.first() else {
                return Err(recv_err("Fields", span));
            };
            self.fields.insert(*handle, crate::web::Fields::default());
            return Ok(Value::Null);
        }

        let Some(Value::Object { handle, .. }) = arguments.first() else {
            return Err(recv_err("Fields", span));
        };
        let Some(fields) = self.fields.get_mut(handle) else {
            return Ok(failed(
                name,
                &WebFailure::InvalidArgument("stale_fields".into()),
            ));
        };

        match method {
            "Add" => {
                require_arity(name, arguments, 3, span)?;
                let (Value::String(k), Value::String(v)) = (&arguments[1], &arguments[2]) else {
                    return Err(arg_err("String", span));
                };
                fields.add(k.to_string(), v.to_string());
                Ok(Value::Null)
            }
            "Encode" => {
                require_arity(name, arguments, 1, span)?;
                let enc = fields.encode();
                Ok(Value::String(shared_string(enc.as_str())))
            }
            _ => Ok(Value::Null),
        }
    }

    pub(super) fn web_form_call(
        &mut self,
        core: &mut dyn CoreContext,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = name.rsplit('.').next().unwrap_or_default();
        if method == "New" {
            let (handle, obj) = alloc_obj(core, "Form", span)?;
            self.forms.insert(handle, crate::web::Form::default());
            return Ok(obj);
        }
        if method == "CONSTRUCTOR" {
            let Some(Value::Object { handle, .. }) = arguments.first() else {
                return Err(recv_err("Form", span));
            };
            self.forms.insert(*handle, crate::web::Form::default());
            return Ok(Value::Null);
        }

        let Some(Value::Object { handle, .. }) = arguments.first() else {
            return Err(recv_err("Form", span));
        };
        let Some(form) = self.forms.get_mut(handle) else {
            return Ok(failed(
                name,
                &WebFailure::InvalidArgument("stale_form".into()),
            ));
        };

        match method {
            "AddField" => {
                require_arity(name, arguments, 3, span)?;
                let (Value::String(k), Value::String(v)) = (&arguments[1], &arguments[2]) else {
                    return Err(arg_err("String", span));
                };
                form.add_field(k.to_string(), v.to_string());
                Ok(Value::Null)
            }
            "AddFile" => {
                require_arity(name, arguments, 4, span)?;
                let (Value::String(n), Value::String(p), Value::String(ct)) =
                    (&arguments[1], &arguments[2], &arguments[3])
                else {
                    return Err(arg_err("String", span));
                };
                form.add_file(n.to_string(), p.to_string(), ct.to_string());
                Ok(Value::Null)
            }
            _ => Ok(Value::Null),
        }
    }

    pub(super) fn execute_client_transfer(
        &mut self,
        core: &mut dyn CoreContext,
        name: &str,
        client_handle: Handle,
        method: &str,
        url: &str,
        body: &[u8],
        content_type: Option<&str>,
        headers_handle: Option<Handle>,
        maximum: i64,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let headers = headers_handle
            .and_then(|handle| self.headers.get(&handle))
            .map(|headers| headers.entries.clone())
            .unwrap_or_default();
        let outcome = crate::web::Transfer {
            method,
            url,
            body,
            content_type,
            headers,
            maximum,
            policy: None,
        }
        .run();
        if let Some(access) = &outcome.access {
            self.log_client_access(core, client_handle, method, url, access);
        }
        match outcome.result {
            Ok(reply) => {
                let (handle, obj) = alloc_obj(core, "ClientReply", span)?;
                self.client_replies.insert(handle, reply);
                Ok(obj)
            }
            Err(failure) => Ok(failed(name, &failure)),
        }
    }

    /// Writes one access record of a client transfer through the client's
    /// `BNLog` logger, when it has one.
    pub(super) fn log_client_access(
        &self,
        core: &mut dyn CoreContext,
        client_handle: Handle,
        method: &str,
        url: &str,
        access: &crate::web::ClientAccess,
    ) {
        let Some(logger) = self.clients.get(&client_handle).copied().and_then(|id| {
            with_log_provider(core, |log| log.core_logger(id))
                .ok()
                .flatten()
        }) else {
            return;
        };
        let Ok(logger) = logger.lock() else {
            return;
        };
        let sink = InterpLogSink {
            host: core.host().clone(),
        };
        let headers = access
            .headers
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        access_log::write_client_access(
            &logger,
            &sink,
            method,
            url,
            access.status,
            access.bytes,
            access.duration_ms,
            &headers,
        );
    }
}

/// A program's `maximum` as the provider core takes it: an `INTEGER` outside
/// `i64` keeps its sign, so the core still refuses a negative one.
pub(super) fn maximum_of(value: i128) -> i64 {
    i64::try_from(value).unwrap_or(if value < 0 { i64::MIN } else { i64::MAX })
}
