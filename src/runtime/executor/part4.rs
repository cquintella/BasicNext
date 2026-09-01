#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

impl Executor<'_, '_> {
    pub(crate) fn web_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = name.rsplit('.').next().unwrap_or_default();
        if name.contains(".SessionStore.") || name.contains(".Scraper.") || name.contains(".ACL.") || name.contains(".CookieJar.") || name.contains(".TLSConfig.") || name.contains(".HeaderValues.") || name.contains(".QueryValues.") {
            return self.web_state_call(name, arguments, span);
        }
        if name.contains(".Request.") {
            return self.web_request_call(name, arguments, span);
        }
        if name.contains(".Response.") {
            return self.web_response_call(name, arguments, span);
        }
        if name.contains(".Client.") {
            match method {
                "New" => {
                    require_arity(name, arguments, 1, span)?;
                    if !matches!(arguments.first(), Some(Value::Object { .. })) {
                        return Err(runtime_error(
                            "TYPE_MISMATCH",
                            "Client.New expects a BNLog.Logger",
                            span,
                        ));
                    }
                    self.allocate_object("BNWeb.Client", span)
                }
                "CONSTRUCTOR" | "$fields" => Ok(Value::Null),
                "Request" => {
                    require_arity(name, arguments, 4, span)?;
                    let (Value::String(method), Value::String(url), Value::String(body)) =
                        (&arguments[1], &arguments[2], &arguments[3])
                    else {
                        return Err(runtime_error(
                            "TYPE_MISMATCH",
                            "client request expects STRING method, URL, and body",
                            span,
                        ));
                    };
                    if !matches!(
                        method.as_str(),
                        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD"
                    ) {
                        return Ok(Value::Error {
                            code: 1,
                            message: "unsupported HTTP method".into(),
                        });
                    }
                    if let Err(message) = crate::web::validate_client_url(url) {
                        return Ok(Value::Error {
                            code: 1,
                            message: message.into(),
                        });
                    }
                    let authority = url
                        .split_once("://")
                        .and_then(|(_, rest)| rest.split(['/', '?']).next())
                        .unwrap_or_default();
                    if let Ok(address) = authority.parse::<std::net::IpAddr>()
                        && let Err(message) =
                            crate::web::validate_ssrf_destinations(&[address], false)
                    {
                        return Ok(Value::Error {
                            code: 1,
                            message: message.into(),
                        });
                    }
                    if let Err(message) = crate::web::bounded_body(body, 8 * 1024 * 1024) {
                        return Ok(Value::Error {
                            code: 1,
                            message: message.into(),
                        });
                    }
                    let response = match crate::http::client_request(method, url, body) {
                        Ok(response) => response,
                        Err(message) => {
                            return Ok(Value::Error { code: 1, message });
                        }
                    };
                    let object = self.allocate_object("BNWeb.Response", span)?;
                    let Value::Object { handle, .. } = object else {
                        unreachable!("allocate_object returns object")
                    };
                    self.web_responses.insert(handle, response);
                    Ok(Value::Object {
                        handle,
                        class: "BNWeb.Response".into(),
                    })
                }
                "Close" => {
                    require_arity(name, arguments, 1, span)?;
                    Ok(Value::Null)
                }
                _ => Ok(Value::Error {
                    code: 1,
                    message: "BNWeb provider unavailable".into(),
                }),
            }
        } else if name.contains(".Server.") {
            if method == "New" {
                require_arity(name, arguments, 1, span)?;
                if !matches!(
                    arguments.first(),
                    Some(Value::Object { .. } | Value::LogLogger(_))
                ) {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "Server.New expects a BNLog.Logger",
                        span,
                    ));
                }
                let object = self.allocate_object("BNWeb.Server", span)?;
                if let Value::Object { handle, .. } = object {
                    self.web_servers.insert(
                        handle,
                        std::sync::Arc::new(std::sync::Mutex::new(crate::web::ServerState::new())),
                    );
                    if let Some(Value::LogLogger(logger_id)) = arguments.first() {
                        self.web_loggers.insert(handle, *logger_id);
                    }
                    self.web_handlers.insert(handle, HashMap::new());
                    self.web_filters.insert(handle, Vec::new());
                }
                return Ok(object);
            }
            if method == "CONSTRUCTOR" {
                let Some(Value::Object { handle, .. }) = arguments.first() else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "BNWeb.Server receiver must be an object",
                        span,
                    ));
                };
                self.web_servers.insert(
                    *handle,
                    std::sync::Arc::new(std::sync::Mutex::new(crate::web::ServerState::new())),
                );
                self.web_loggers.remove(handle);
                self.web_handlers.insert(*handle, HashMap::new());
                self.web_filters.insert(*handle, Vec::new());
                return Ok(Value::Null);
            }
            let Some(Value::Object { handle, .. }) = arguments.first() else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "BNWeb.Server receiver must be an object",
                    span,
                ));
            };
            let state = self.web_servers.get(handle).cloned().ok_or_else(|| {
                runtime_error("STALE_HANDLE", "BNWeb.Server handle is not live", span)
            })?;
            match method {
                "AddFilter" => {
                    require_arity(name, arguments, 2, span)?;
                    let Value::Function(filter) = &arguments[1] else {
                        return Ok(Value::Error {
                            code: 1,
                            message: "filter must be a FUNCTION".into(),
                        });
                    };
                    let filters = self.web_filters.entry(*handle).or_default();
                    if filters.len() >= 64 {
                        return Ok(Value::Error {
                            code: 1,
                            message: "filter limit exceeded".into(),
                        });
                    }
                    filters.push(filter.clone());
                    Ok(Value::Null)
                }
                "Route" => {
                    require_arity(name, arguments, 4, span)?;
                    let (Value::String(method), Value::String(pattern)) =
                        (&arguments[1], &arguments[2])
                    else {
                        return Err(runtime_error(
                            "TYPE_MISMATCH",
                            "route method and pattern must be STRING",
                            span,
                        ));
                    };
                    if !matches!(arguments[3], Value::Function(_)) {
                        return Ok(Value::Error {
                            code: 1,
                            message: "route handler must be a FUNCTION".into(),
                        });
                    }
                    let mut state = state.lock().map_err(|_| {
                        runtime_error("SERVER_STATE", "server state unavailable", span)
                    })?;
                    let result = state.add_route(method.clone(), pattern.clone());
                    drop(state);
                    if let Err(message) = result {
                        return Ok(Value::Error {
                            code: 1,
                            message: message.into(),
                        });
                    }
                    let Value::Function(handler) = &arguments[3] else {
                        unreachable!("route handler was validated above")
                    };
                    self.web_handlers
                        .entry(*handle)
                        .or_default()
                        .insert(format!("{method}\n{pattern}"), handler.clone());
                    Ok(Value::Null)
                }
                "Start" => {
                    require_arity(name, arguments, 2, span)?;
                    let endpoint = net_endpoint(&arguments[1], span)?;
                    let listener = crate::net::TcpListener::bind(endpoint)
                        .map_err(|error| runtime_error("WEB_LISTEN", error.to_string(), span))?;
                    let mut state_guard = state.lock().map_err(|_| {
                        runtime_error("SERVER_STATE", "server state unavailable", span)
                    })?;
                    let started = state_guard.start();
                    drop(state_guard);
                    if let Err(message) = started {
                        return Ok(Value::Error {
                            code: 1,
                            message: message.into(),
                        });
                    }
                    let accept_state = state.clone();
                    std::thread::Builder::new()
                        .name("bnweb-listener".into())
                        .spawn(move || {
                            loop {
                                let stopped = accept_state
                                    .lock()
                                    .map_or(true, |state| state.is_stopping() || state.is_closed());
                                if stopped {
                                    break;
                                }
                                match listener.accept_timeout(std::time::Duration::from_millis(25))
                                {
                                    Ok(Some(stream)) => {
                                        let connection_state = accept_state.clone();
                                        let _ = std::thread::Builder::new()
                                            .name("bnweb-connection".into())
                                            .spawn(move || {
                                                let _ = crate::http::serve_connection(
                                                    stream,
                                                    connection_state,
                                                );
                                            });
                                    }
                                    Ok(None) => {}
                                    Err(_) => break,
                                }
                            }
                        })
                        .map_err(|error| runtime_error("WEB_LISTEN", error.to_string(), span))?;
                    Ok(Value::Null)
                }
                "StartTLS" => {
                    require_arity(name, arguments, 3, span)?;
                    let endpoint = net_endpoint(&arguments[1], span)?;
                    let Value::Object {
                        handle: config_handle,
                        ..
                    } = &arguments[2]
                    else {
                        return Err(runtime_error(
                            "TYPE_MISMATCH",
                            "StartTLS expects TLSConfig",
                            span,
                        ));
                    };
                    let config = self
                        .web_tls_configs
                        .get(config_handle)
                        .cloned()
                        .ok_or_else(|| {
                            runtime_error(
                                "STALE_HANDLE",
                                "BNWeb.TLSConfig handle is not live",
                                span,
                            )
                        })?;
                    let listener = crate::net::TcpListener::bind(endpoint)
                        .map_err(|error| runtime_error("WEB_LISTEN", error.to_string(), span))?;
                    let mut state_guard = state.lock().map_err(|_| {
                        runtime_error("SERVER_STATE", "server state unavailable", span)
                    })?;
                    let started = state_guard.start();
                    drop(state_guard);
                    if let Err(message) = started {
                        return Ok(Value::Error {
                            code: 1,
                            message: message.into(),
                        });
                    }
                    let accept_state = state.clone();
                    std::thread::Builder::new()
                        .name("bnweb-tls-listener".into())
                        .spawn(move || {
                            loop {
                                let stopped = accept_state.lock().map_or(true, |server| {
                                    server.is_stopping() || server.is_closed()
                                });
                                if stopped {
                                    break;
                                }
                                match listener.accept_timeout(std::time::Duration::from_millis(25))
                                {
                                    Ok(Some(stream)) => {
                                        let connection_state = accept_state.clone();
                                        let tls_config = config.clone();
                                        let _ = std::thread::Builder::new()
                                            .name("bnweb-tls-connection".into())
                                            .spawn(move || {
                                                let _ = crate::http::serve_tls_connection(
                                                    stream,
                                                    connection_state,
                                                    tls_config,
                                                );
                                            });
                                    }
                                    Ok(None) => {}
                                    Err(_) => break,
                                }
                            }
                        })
                        .map_err(|error| runtime_error("WEB_LISTEN", error.to_string(), span))?;
                    Ok(Value::Null)
                }
                "Stop" => {
                    require_arity(name, arguments, 2, span)?;
                    let timeout = integer(&arguments[1], span)?.0;
                    let mut state = state.lock().map_err(|_| {
                        runtime_error("SERVER_STATE", "server state unavailable", span)
                    })?;
                    Ok(state.stop(timeout).map_or_else(
                        |message| Value::Error {
                            code: 1,
                            message: message.into(),
                        },
                        |()| Value::Null,
                    ))
                }
                "Dispatch" => {
                    require_arity(name, arguments, 3, span)?;
                    let (
                        Value::Object {
                            handle: request_handle,
                            ..
                        },
                        Value::Object {
                            handle: response_handle,
                            ..
                        },
                    ) = (&arguments[1], &arguments[2])
                    else {
                        return Err(runtime_error(
                            "TYPE_MISMATCH",
                            "Dispatch expects Request and Response objects",
                            span,
                        ));
                    };
                    let request = self.web_requests.get(request_handle).ok_or_else(|| {
                        runtime_error("STALE_HANDLE", "BNWeb.Request handle is not live", span)
                    })?;
                    let method_name = request.method.clone();
                    let path = request.target.path.clone();
                    let selected = {
                        let mut state = state.lock().map_err(|_| {
                            runtime_error("SERVER_STATE", "server state unavailable", span)
                        })?;
                        let mut selected = None;
                        if state
                            .dispatch(&method_name, &path, |outcome| {
                                selected = Some(match outcome {
                                    crate::web::RouteOutcome::Matched(route, _) => {
                                        Ok(route.pattern().to_owned())
                                    }
                                    crate::web::RouteOutcome::MethodNotAllowed(_) => Err(405),
                                    crate::web::RouteOutcome::NotFound => Err(404),
                                });
                            })
                            .is_err()
                        {
                            return Ok(Value::Error {
                                code: 1,
                                message: "server is not accepting requests".into(),
                            });
                        }
                        selected.unwrap_or(Err(500))
                    };
                    match selected {
                        Ok(pattern) => {
                            let filters = self.web_filters.get(handle).cloned().unwrap_or_default();
                            for filter in filters {
                                let result = self.call_named(
                                    &filter,
                                    vec![arguments[1].clone(), arguments[2].clone()],
                                    span,
                                )?;
                                if let Value::Error { .. } = result {
                                    return Ok(result);
                                }
                            }
                            let handler = self
                                .web_handlers
                                .get(handle)
                                .and_then(|handlers| {
                                    handlers.get(&format!("{method_name}\n{pattern}"))
                                })
                                .cloned()
                                .ok_or_else(|| {
                                    runtime_error(
                                        "HANDLER_NOT_FOUND",
                                        "route handler is not live",
                                        span,
                                    )
                                })?;
                            let result = self.call_named(
                                &handler,
                                vec![arguments[1].clone(), arguments[2].clone()],
                                span,
                            );
                            let status = self
                                .web_responses
                                .get(response_handle)
                                .map_or(500, |response| i128::from(response.status));
                            self.log_web_dispatch(*handle, &method_name, &path, status, span)?;
                            result
                        }
                        Err(status) if status == 404 || status == 405 => {
                            if let Some(response) = self.web_responses.get_mut(response_handle) {
                                let _ = response.set_status(u16::try_from(status).unwrap_or(500));
                            }
                            self.log_web_dispatch(
                                *handle,
                                &method_name,
                                &path,
                                i128::from(status),
                                span,
                            )?;
                            Ok(Value::Null)
                        }
                        Err(_) => Ok(Value::Error {
                            code: 1,
                            message: "request dispatch failed".into(),
                        }),
                    }
                }
                "Close" => {
                    require_arity(name, arguments, 2, span)?;
                    let timeout = integer(&arguments[1], span)?.0;
                    let mut state = state.lock().map_err(|_| {
                        runtime_error("SERVER_STATE", "server state unavailable", span)
                    })?;
                    Ok(state.close(timeout).map_or_else(
                        |message| Value::Error {
                            code: 1,
                            message: message.into(),
                        },
                        |()| Value::Null,
                    ))
                }
                "$fields" => Ok(Value::Null),
                _ => Ok(Value::Error {
                    code: 1,
                    message: "BNWeb provider unavailable".into(),
                }),
            }
        } else if method == "CONSTRUCTOR" || method == "$fields" {
            Ok(Value::Null)
        } else {
            Ok(Value::Error {
                code: 1,
                message: "BNWeb provider unavailable".into(),
            })
        }
    }
}
