#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

impl Executor<'_, '_> {
pub(crate) fn web_response_call(&mut self, name: &str, arguments: &[Value], span: Span) -> Result<Value, Diagnostic> {
        let method = name.rsplit('.').next().unwrap_or_default();
        if name.contains(".Response.") {
            if method == "CONSTRUCTOR" {
                let Some(Value::Object { handle, .. }) = arguments.first() else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "BNWeb.Response receiver must be an object",
                        span,
                    ));
                };
                self.web_responses
                    .insert(*handle, crate::web::Response::new());
                return Ok(Value::Null);
            }
            let Some(Value::Object { handle, .. }) = arguments.first() else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "BNWeb.Response receiver must be an object",
                    span,
                ));
            };
            let response = self.web_responses.get_mut(handle).ok_or_else(|| {
                runtime_error("STALE_HANDLE", "BNWeb.Response handle is not live", span)
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
                        return Ok(Value::Error {
                            code: 1,
                            message: "status must be 100..599".into(),
                        });
                    };
                    Ok(response.set_status(status).map_or_else(
                        |message| Value::Error {
                            code: 1,
                            message: message.into(),
                        },
                        |()| Value::Null,
                    ))
                }
                "SetHeader" => {
                    require_arity(name, arguments, 3, span)?;
                    let (Value::String(key), Value::String(value)) = (&arguments[1], &arguments[2])
                    else {
                        return Err(runtime_error(
                            "TYPE_MISMATCH",
                            "response header name and value must be STRING",
                            span,
                        ));
                    };
                    Ok(response.set_header(key, value).map_or_else(
                        |message| Value::Error {
                            code: 1,
                            message: message.into(),
                        },
                        |()| Value::Null,
                    ))
                }
                "Header" => {
                    require_arity(name, arguments, 2, span)?;
                    let Value::String(key) = &arguments[1] else {
                        return Err(runtime_error(
                            "TYPE_MISMATCH",
                            "response header name must be STRING",
                            span,
                        ));
                    };
                    response
                        .headers
                        .iter()
                        .find(|(name, _)| name.eq_ignore_ascii_case(key))
                        .map(|(_, value)| Value::String(value.clone()))
                        .ok_or_else(|| runtime_error("HEADER_NOT_FOUND", "response header is not present", span))
                }
                "Write" => {
                    require_arity(name, arguments, 2, span)?;
                    let Value::String(body) = &arguments[1] else {
                        return Err(runtime_error(
                            "TYPE_MISMATCH",
                            "response body must be STRING",
                            span,
                        ));
                    };
                    Ok(response.write(body).map_or_else(
                        |message| Value::Error {
                            code: 1,
                            message: message.into(),
                        },
                        |()| Value::Null,
                    ))
                }
                "Commit" => {
                    require_arity(name, arguments, 1, span)?;
                    Ok(response.commit().map_or_else(
                        |message| Value::Error {
                            code: 1,
                            message: message.into(),
                        },
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
                _ => Ok(Value::Error {
                    code: 1,
                    message: "BNWeb provider unavailable".into(),
                }),
            }

        }
        else {
            Err(runtime_error("HOST_CAPABILITY_UNAVAILABLE", format!("web function '{name}' is not available"), span))
        }
    }
}
