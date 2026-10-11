// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNWeb.Request` and `BNWeb.Response` members for the interpreter (split
//! from `lib.rs` by domain).

#![allow(clippy::wildcard_imports)]
use super::*;

impl WebProvider {
    pub(super) fn web_request_call(
        &mut self,
        core: &mut dyn CoreContext,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = name.rsplit('.').next().unwrap_or_default();
        if name.contains(".Request.") {
            if method == "CONSTRUCTOR" {
                let Some(Value::Object { handle, .. }) = arguments.first() else {
                    return Err(type_mismatch(
                        "BNWeb.Request",
                        "non-object value",
                        "BNWeb.Request operation receiver",
                        span,
                    ));
                };
                let request = crate::web::Request::new(
                    "GET",
                    "/",
                    Vec::new(),
                    "",
                    std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                )
                .map_err(|message| {
                    runtime_error(bn_diag::DiagId::REQUEST_INVALID, message, span)
                })?;
                self.requests.insert(*handle, request);
                return Ok(Value::Null);
            }
            if method == "Session" {
                return self.request_session(core, name, arguments, span);
            }
            let Some(Value::Object { handle, .. }) = arguments.first() else {
                return Err(type_mismatch(
                    "BNWeb.Request",
                    "non-object value",
                    "BNWeb.Request operation receiver",
                    span,
                ));
            };
            let request = self.requests.get(handle).ok_or_else(|| {
                runtime_error(
                    bn_diag::DiagId::STALE_HANDLE,
                    "BNWeb.Request handle is not live",
                    span,
                )
            })?;
            match method {
                "Method" => {
                    require_arity(name, arguments, 1, span)?;
                    Ok(Value::String(request.method().into()))
                }
                "Target" => {
                    require_arity(name, arguments, 1, span)?;
                    Ok(Value::String(request.target().into()))
                }
                "Headers" | "Query" => {
                    require_arity(name, arguments, 2, span)?;
                    let Value::String(key) = &arguments[1] else {
                        return Err(type_mismatch(
                            "STRING",
                            "non-STRING value",
                            "BNWeb.Request collection name",
                            span,
                        ));
                    };
                    let values = if method == "Headers" {
                        request.header(key).map(|values| {
                            (0..values.count())
                                .filter_map(|index| values.get(index).map(str::to_owned))
                                .collect::<Vec<_>>()
                        })
                    } else {
                        request.query(key).map(|values| {
                            (0..values.count())
                                .filter_map(|index| values.get(index).map(str::to_owned))
                                .collect::<Vec<_>>()
                        })
                    };
                    let values = match values {
                        Ok(values) => values,
                        Err(message) => {
                            return Ok(failed(name, &WebFailure::from_message(message)));
                        }
                    };
                    let class = if method == "Headers" {
                        "BNWeb.HeaderValues"
                    } else {
                        "BNWeb.QueryValues"
                    };
                    let object = core.allocate_object(class, span)?;
                    let Value::Object { handle, .. } = object else {
                        unreachable!("allocate_object returns object")
                    };
                    self.values.insert(handle, values);
                    Ok(Value::Object {
                        handle,
                        class: class.into(),
                    })
                }
                "Body" => {
                    require_arity(name, arguments, 2, span)?;
                    let maximum = integer(&arguments[1], span)?.0;
                    Ok(request.body(maximum).map_or_else(
                        |message| failed(name, &WebFailure::from_message(message)),
                        |body| Value::String(body.into()),
                    ))
                }
                "PeerAddress" => {
                    require_arity(name, arguments, 1, span)?;
                    Ok(address_value(request.peer_address()))
                }
                "EffectiveClientAddress" => {
                    require_arity(name, arguments, 1, span)?;
                    Ok(address_value(request.effective_client_address(false)))
                }
                "Param" | "Cookie" => {
                    require_arity(name, arguments, 2, span)?;
                    let Value::String(key) = &arguments[1] else {
                        return Err(type_mismatch(
                            "STRING",
                            "non-STRING value",
                            bn_catalog::text("web", "context.request.member_name"),
                            span,
                        ));
                    };
                    let (found, missing) = if method == "Param" {
                        (request.param(key), WebFailure::RouteParameterNotFound)
                    } else {
                        (request.cookie(key), WebFailure::RequestCookieNotFound)
                    };
                    Ok(found.map_or_else(
                        || failed(name, &missing),
                        |value| Value::String(shared_string(value)),
                    ))
                }
                _ => Ok(failed(name, &WebFailure::ProviderUnavailable("BNWeb"))),
            }
        } else {
            Err(runtime_error(
                bn_diag::DiagId::HOST_CAPABILITY_UNAVAILABLE,
                format!("web function '{name}' is not available"),
                span,
            ))
        }
    }

    pub(super) fn web_response_call(
        &mut self,
        _core: &mut dyn CoreContext,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = name.rsplit('.').next().unwrap_or_default();
        if name.contains(".Response.") {
            if method == "CONSTRUCTOR" {
                let Some(Value::Object { handle, .. }) = arguments.first() else {
                    return Err(type_mismatch(
                        "BNWeb.Response",
                        "non-object value",
                        "BNWeb.Response constructor receiver",
                        span,
                    ));
                };
                self.responses.insert(*handle, crate::web::Response::new());
                return Ok(Value::Null);
            }
            let Some(Value::Object { handle, .. }) = arguments.first() else {
                return Err(type_mismatch(
                    "BNWeb.Response",
                    "non-object value",
                    "BNWeb.Response operation receiver",
                    span,
                ));
            };
            let response = self.responses.get_mut(handle).ok_or_else(|| {
                runtime_error(
                    bn_diag::DiagId::STALE_HANDLE,
                    "BNWeb.Response handle is not live",
                    span,
                )
            })?;
            match method {
                "Status" => {
                    require_arity(name, arguments, 1, span)?;
                    Ok(Value::Integer(
                        i128::from(response.status),
                        IntegerType::Int32,
                    ))
                }
                "SetStatus" => {
                    require_arity(name, arguments, 2, span)?;
                    let status = integer(&arguments[1], span)?.0;
                    let Ok(status) = u16::try_from(status) else {
                        return Ok(failed(name, &WebFailure::InvalidStatus(status)));
                    };
                    Ok(response.set_status(status).map_or_else(
                        |message| failed(name, &WebFailure::from_message(message)),
                        |()| Value::Null,
                    ))
                }
                "SetHeader" => {
                    require_arity(name, arguments, 3, span)?;
                    let (Value::String(key), Value::String(value)) = (&arguments[1], &arguments[2])
                    else {
                        return Err(type_mismatch(
                            "STRING, STRING",
                            "non-STRING header name/value",
                            "BNWeb.Response.SetHeader",
                            span,
                        ));
                    };
                    Ok(response.set_header(key, value).map_or_else(
                        |message| failed(name, &WebFailure::from_message(message)),
                        |()| Value::Null,
                    ))
                }
                "Header" => {
                    require_arity(name, arguments, 2, span)?;
                    let Value::String(key) = &arguments[1] else {
                        return Err(type_mismatch(
                            "STRING",
                            "non-STRING value",
                            "BNWeb.Response.Header name",
                            span,
                        ));
                    };
                    Ok(response
                        .headers
                        .iter()
                        .find(|(header, _)| header.eq_ignore_ascii_case(key))
                        .map_or_else(
                            || failed(name, &WebFailure::HeaderNotFound(key.to_string())),
                            |(_, value)| Value::String(shared_string(value.as_str())),
                        ))
                }
                "SetCookie" | "SetCookieWithPolicy" => {
                    let with_policy = method == "SetCookieWithPolicy";
                    require_arity(name, arguments, if with_policy { 9 } else { 6 }, span)?;
                    let (
                        Value::String(cookie),
                        Value::String(value),
                        Value::String(domain),
                        Value::String(path),
                    ) = (&arguments[1], &arguments[2], &arguments[3], &arguments[4])
                    else {
                        return Err(type_mismatch(
                            "STRING, STRING, STRING, STRING",
                            "non-STRING argument",
                            bn_catalog::text("web", "op.response.set_cookie"),
                            span,
                        ));
                    };
                    let options = if with_policy {
                        let (
                            Value::Boolean(secure),
                            Value::Boolean(http_only),
                            Value::String(same_site),
                        ) = (&arguments[6], &arguments[7], &arguments[8])
                        else {
                            return Err(type_mismatch(
                                bn_catalog::text("web", "context.cookie_policy"),
                                "incompatible argument",
                                bn_catalog::text("web", "op.response.set_cookie_with_policy"),
                                span,
                            ));
                        };
                        match crate::web_state::CookieOptions::from_policy(
                            *secure, *http_only, same_site,
                        ) {
                            Ok(options) => options,
                            Err(message) => {
                                return Ok(failed(name, &WebFailure::from_message(message)));
                            }
                        }
                    } else {
                        crate::web_state::CookieOptions::default()
                    };
                    let age =
                        match crate::web_state::cookie_max_age(integer(&arguments[5], span)?.0) {
                            Ok(age) => age,
                            Err(message) => {
                                return Ok(failed(name, &WebFailure::from_message(message)));
                            }
                        };
                    // `maxAgeMs` 0 is a session cookie; an empty domain is host-only.
                    let max_age = (!age.is_zero()).then_some(age);
                    let domain = (!domain.is_empty()).then_some(domain.as_ref());
                    Ok(response
                        .set_cookie(cookie, value, domain, path, max_age, options)
                        .map_or_else(
                            |message| failed(name, &WebFailure::from_message(message)),
                            |()| Value::Null,
                        ))
                }
                "ClearCookie" => {
                    require_arity(name, arguments, 4, span)?;
                    let (Value::String(cookie), Value::String(domain), Value::String(path)) =
                        (&arguments[1], &arguments[2], &arguments[3])
                    else {
                        return Err(type_mismatch(
                            "STRING, STRING, STRING",
                            "non-STRING argument",
                            bn_catalog::text("web", "op.response.clear_cookie"),
                            span,
                        ));
                    };
                    let domain = (!domain.is_empty()).then_some(domain.as_ref());
                    Ok(response.clear_cookie(cookie, domain, path).map_or_else(
                        |message| failed(name, &WebFailure::from_message(message)),
                        |()| Value::Null,
                    ))
                }
                "Write" => {
                    require_arity(name, arguments, 2, span)?;
                    let Value::String(body) = &arguments[1] else {
                        return Err(type_mismatch(
                            "STRING",
                            "non-STRING value",
                            "BNWeb.Response.Write body",
                            span,
                        ));
                    };
                    Ok(response.write(body).map_or_else(
                        |message| failed(name, &WebFailure::from_message(message)),
                        |()| Value::Null,
                    ))
                }
                "Commit" => {
                    require_arity(name, arguments, 1, span)?;
                    Ok(response.commit().map_or_else(
                        |message| failed(name, &WebFailure::from_message(message)),
                        |()| Value::Null,
                    ))
                }
                "IsCommitted" => {
                    require_arity(name, arguments, 1, span)?;
                    Ok(Value::Boolean(response.is_committed()))
                }
                "Close" => {
                    require_arity(name, arguments, 1, span)?;
                    response.close();
                    Ok(Value::Null)
                }
                _ => Ok(failed(name, &WebFailure::ProviderUnavailable("BNWeb"))),
            }
        } else {
            Err(runtime_error(
                bn_diag::DiagId::HOST_CAPABILITY_UNAVAILABLE,
                format!("web function '{name}' is not available"),
                span,
            ))
        }
    }
}
