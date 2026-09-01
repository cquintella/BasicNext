#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

impl Executor<'_, '_> {
pub(crate) fn web_request_call(&mut self, name: &str, arguments: &[Value], span: Span) -> Result<Value, Diagnostic> {
        let method = name.rsplit('.').next().unwrap_or_default();
        if name.contains(".Request.") {
            if method == "CONSTRUCTOR" {
                let Some(Value::Object { handle, .. }) = arguments.first() else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "BNWeb.Request receiver must be an object",
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
                .map_err(|message| runtime_error("REQUEST_INVALID", message, span))?;
                self.web_requests.insert(*handle, request);
                return Ok(Value::Null);
            }
            let Some(Value::Object { handle, .. }) = arguments.first() else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "BNWeb.Request receiver must be an object",
                    span,
                ));
            };
            let request = self.web_requests.get(handle).ok_or_else(|| {
                runtime_error("STALE_HANDLE", "BNWeb.Request handle is not live", span)
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
                        return Err(runtime_error(
                            "TYPE_MISMATCH",
                            "collection name must be STRING",
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
                            return Ok(Value::Error {
                                code: 1,
                                message: message.into(),
                            });
                        }
                    };
                    let class = if method == "Headers" {
                        "BNWeb.HeaderValues"
                    } else {
                        "BNWeb.QueryValues"
                    };
                    let object = self.allocate_object(class, span)?;
                    let Value::Object { handle, .. } = object else {
                        unreachable!("allocate_object returns object")
                    };
                    self.web_values.insert(handle, values);
                    Ok(Value::Object {
                        handle,
                        class: class.into(),
                    })
                }
                "Body" => {
                    require_arity(name, arguments, 2, span)?;
                    let maximum = integer(&arguments[1], span)?.0;
                    Ok(request.body(maximum).map_or_else(
                        |message| Value::Error {
                            code: 1,
                            message: message.into(),
                        },
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
                _ => Ok(Value::Error {
                    code: 1,
                    message: "BNWeb provider unavailable".into(),
                }),
            }
        } else {
            Err(runtime_error(
                "HOST_CAPABILITY_UNAVAILABLE",
                format!("web function '{name}' is not available"),
                span,
            ))
        }

    }
}
