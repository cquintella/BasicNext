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
                let maximum = usize::try_from(max).unwrap_or(usize::MAX);
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
                let maximum = usize::try_from(max).unwrap_or(usize::MAX);
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
                let maximum = usize::try_from(max).unwrap_or(usize::MAX);
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
                let maximum = usize::try_from(max).unwrap_or(usize::MAX);
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
        maximum: usize,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let policy = crate::web::EgressPolicy::default();
        let headers_entries = if let Some(h) = headers_handle {
            self.headers
                .get(&h)
                .map(|hd| hd.entries.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let mut extra = headers_entries;
        if let Some(ct) = content_type {
            let has_ct = extra
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("content-type"));
            if !has_ct {
                extra.push(("content-type".into(), ct.into()));
            }
        }

        let started = std::time::Instant::now();
        let outcome = crate::http::client_transfer_with_policy(method, url, body, &extra, &policy);
        let duration_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);

        let (status, bytes, access_headers) = match &outcome {
            Ok(r) => {
                let hdrs = r
                    .headers
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect::<Vec<_>>();
                (
                    i64::from(r.status),
                    i64::try_from(r.body.len()).unwrap_or(i64::MAX),
                    hdrs,
                )
            }
            Err(_) => (0, 0, Vec::new()),
        };

        let maybe_logger = self.clients.get(&client_handle).copied().and_then(|id| {
            with_log_provider(core, |log| log.core_logger(id))
                .ok()
                .flatten()
        });
        #[allow(clippy::collapsible_if)]
        if let Some(logger) = maybe_logger {
            if let Ok(logger) = logger.lock() {
                let sink = InterpLogSink {
                    host: core.host().clone(),
                };
                access_log::write_client_access(
                    &logger,
                    &sink,
                    method,
                    url,
                    status,
                    bytes,
                    duration_ms,
                    &access_headers,
                );
            }
        }

        let reply = match outcome {
            Ok(r) => {
                if maximum > 0 && r.body.len() > maximum {
                    return Ok(failed(
                        name,
                        &WebFailure::BodyLimitExceeded(format!("{}/{}", r.body.len(), maximum)),
                    ));
                }
                r
            }
            Err(err_msg) => {
                return Ok(failed(name, &err_msg));
            }
        };

        let (handle, obj) = alloc_obj(core, "ClientReply", span)?;
        self.client_replies.insert(handle, reply);
        Ok(obj)
    }
}
