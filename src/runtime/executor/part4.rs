#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

fn bn_server_handler(
    module: Module,
    host: HostEnv,
    handlers: HashMap<String, String>,
    filters: Vec<String>,
    state: std::sync::Arc<std::sync::Mutex<crate::web::ServerState>>,
) -> crate::http::Handler {
    std::sync::Arc::new(move |request, response| {
        let pattern = state
            .lock()
            .ok()
            .and_then(|server| server.matched_route_pattern(&request.method, &request.target.path))
            .ok_or("BNWeb route disappeared")?;
        let key = format!("{}\n{pattern}", request.method);
        let handler = handlers.get(&key).ok_or("BNWeb handler is not live")?;
        let mut current = crate::web::Response::new();
        for filter in &filters {
            current = crate::runtime::execute_web_callback(
                &module,
                &host,
                filter,
                request.clone(),
                current,
            )
            .map_err(|_| "BNWeb filter failed")?;
        }
        current = crate::runtime::execute_web_callback(
            &module,
            &host,
            handler,
            request.clone(),
            current,
        )
        .map_err(|_| "BNWeb handler failed")?;
        *response = current;
        Ok(())
    })
}

fn drain_server(
    state: &std::sync::Arc<std::sync::Mutex<crate::web::ServerState>>,
    timeout_ms: i128,
    close: bool,
) -> Result<(), &'static str> {
    let timeout = std::time::Duration::from_millis(
        u64::try_from(timeout_ms).map_err(|_| "stop timeout is outside 1..60000 ms")?,
    );
    let deadline = std::time::Instant::now() + timeout;
    let mut listener = {
        let mut server = state
            .lock()
            .map_err(|_| "server state unavailable")?;
        server.begin_stop(timeout_ms)?;
        server.take_listener()
    };
    while let Some(handle) = listener {
        if handle.is_finished() {
            if handle.join().is_err() {
                if let Ok(mut server) = state.lock() {
                    server.mark_failed();
                }
                return Err("server listener join failed");
            }
            break;
        }
        if std::time::Instant::now() >= deadline {
            let mut server = state
                .lock()
                .map_err(|_| "server state unavailable")?;
            server
                .install_listener(handle)
                .map_err(|_| "server listener is already installed")?;
            return Err("server listener join timed out");
        }
        std::thread::yield_now();
        listener = Some(handle);
    }
    loop {
        let result = {
            let mut server = state
                .lock()
                .map_err(|_| "server state unavailable")?;
            server.finish_stop()
        };
        match result {
            Ok(()) => {
                let workers_finished = state
                    .lock()
                    .map_err(|_| "server state unavailable")?
                    .workers_finished();
                if !workers_finished {
                    if std::time::Instant::now() >= deadline {
                        return Err("server worker drain timed out");
                    }
                    std::thread::yield_now();
                    continue;
                }
                if close {
                    state
                        .lock()
                        .map_err(|_| "server state unavailable")?
                        .mark_closed();
                }
                return Ok(());
            }
            Err("server drain timed out with active connections") => {
                if std::time::Instant::now() >= deadline {
                    return Err("server drain timed out with active connections");
                }
            }
            Err(message) => return Err(message),
        }
        std::thread::yield_now();
    }
}

impl Executor<'_, '_> {
    pub(crate) fn web_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = name.rsplit('.').next().unwrap_or_default();
        if name.contains(".SessionStore.") || name.contains(".Scraper.") || name.contains(".ACL.") || name.contains(".CookieJar.") || name.contains(".TLSConfig.") || name.contains(".ServerOptions.") || name.contains(".EgressPolicy.") || name.contains(".HeaderValues.") || name.contains(".QueryValues.") {
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
                "Request" | "RequestWithPolicy" => {
                    let with_policy = method == "RequestWithPolicy";
                    require_arity(name, arguments, if with_policy { 5 } else { 4 }, span)?;
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
                    let policy = if with_policy {
                        let Value::Object { handle, .. } = &arguments[4] else {
                            return Err(runtime_error(
                                "TYPE_MISMATCH",
                                "RequestWithPolicy expects EgressPolicy",
                                span,
                            ));
                        };
                        self.web_egress_policies.get(handle).ok_or_else(|| {
                            runtime_error("STALE_HANDLE", "EgressPolicy handle is not live", span)
                        })?
                    } else {
                        &crate::web::EgressPolicy::default()
                    };
                    let response = match crate::http::client_request_with_policy(
                        method, url, body, policy,
                    ) {
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
                "Status" => {
                    require_arity(name, arguments, 1, span)?;
                    let status = match state.lock().map_err(|_| runtime_error("SERVER_STATE", "server state unavailable", span))?.status() {
                        crate::web::ServerStatus::Starting => "Starting",
                        crate::web::ServerStatus::Accepting => "Accepting",
                        crate::web::ServerStatus::Draining => "Draining",
                        crate::web::ServerStatus::Stopped => "Stopped",
                        crate::web::ServerStatus::Failed => "Failed",
                    };
                    Ok(Value::String(status.into()))
                }
                "IsReady" => {
                    require_arity(name, arguments, 1, span)?;
                    Ok(Value::Boolean(state.lock().map_err(|_| runtime_error("SERVER_STATE", "server state unavailable", span))?.is_ready()))
                }
                "ActiveConnections" => {
                    require_arity(name, arguments, 1, span)?;
                    Ok(Value::Integer(state.lock().map_err(|_| runtime_error("SERVER_STATE", "server state unavailable", span))?.active_connections() as i128, IntegerType::Int32))
                }
                "PendingRequests" => {
                    require_arity(name, arguments, 1, span)?;
                    Ok(Value::Integer(state.lock().map_err(|_| runtime_error("SERVER_STATE", "server state unavailable", span))?.pending_requests() as i128, IntegerType::Int32))
                }
                "AcceptedRequests" | "ActiveRequests" | "RejectedRequests" | "TimedOutRequests" | "CompletedRequests" | "FailedRequests" | "RateLimitedRequests" | "TotalRequestDurationMs" | "AverageRequestDurationMs" | "MaxRequestDurationMs" => {
                    require_arity(name, arguments, 1, span)?;
                    let snapshot = state.lock().map_err(|_| runtime_error("SERVER_STATE", "server state unavailable", span))?.stats();
                    let value = match method {
                        "AcceptedRequests" => snapshot.accepted,
                        "ActiveRequests" => snapshot.active,
                        "RejectedRequests" => snapshot.rejected,
                        "TimedOutRequests" => snapshot.timed_out,
                        "CompletedRequests" => snapshot.completed,
                        "FailedRequests" => snapshot.failed,
                        "RateLimitedRequests" => snapshot.rate_limited,
                        "TotalRequestDurationMs" => snapshot.duration_total_ms,
                        "AverageRequestDurationMs" => snapshot
                            .duration_total_ms
                            .checked_div(snapshot.completed.max(1))
                            .unwrap_or(0),
                        "MaxRequestDurationMs" => snapshot.duration_max_ms,
                        _ => unreachable!(),
                    };
                    Ok(Value::Integer(i128::from(value), IntegerType::Int32))
                }
                "Start" | "StartWithOptions" => {
                    let options = if method == "StartWithOptions" {
                        require_arity(name, arguments, 3, span)?;
                        let Value::Object { handle, .. } = &arguments[2] else {
                            return Err(runtime_error("TYPE_MISMATCH", "StartWithOptions expects ServerOptions", span));
                        };
                        self.web_server_options.get(handle).cloned().ok_or_else(|| {
                            runtime_error("STALE_HANDLE", "ServerOptions handle is not live", span)
                        })?
                    } else {
                        require_arity(name, arguments, 2, span)?;
                        crate::web::ServerOptions::default()
                    };
                    options.validate().map_err(|message| runtime_error("INVALID_OPTIONS", message, span))?;
                    let endpoint = net_endpoint(&arguments[1], span)?;
                    let listener = crate::net::TcpListener::bind_with_backlog(endpoint, options.backlog)
                        .map_err(|error| runtime_error("WEB_LISTEN", error.to_string(), span))?;
                    let mut state_guard = state.lock().map_err(|_| {
                        runtime_error("SERVER_STATE", "server state unavailable", span)
                    })?;
                    let started = state_guard.start_with_options(options);
                    drop(state_guard);
                    if let Err(message) = started {
                        return Ok(Value::Error {
                            code: 1,
                            message: message.into(),
                        });
                    }
                    if let Err(message) = state
                        .lock()
                        .map_err(|_| runtime_error("SERVER_STATE", "server state unavailable", span))?
                        .install_worker_pool()
                    {
                        return Ok(Value::Error {
                            code: 1,
                            message: message.into(),
                        });
                    }
                    let request_handler = bn_server_handler(
                        self.module.clone(),
                        self.host.clone(),
                        self.web_handlers.get(handle).cloned().unwrap_or_default(),
                        self.web_filters.get(handle).cloned().unwrap_or_default(),
                        state.clone(),
                    );
                    let accept_state = state.clone();
                    let listener_handle = std::thread::Builder::new()
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
                                        let admitted = accept_state.lock().is_ok_and(|mut server| {
                                            server.admit_connection().is_ok()
                                                && server.track_connection_socket(&stream)
                                        });
                                        if !admitted {
                                            continue;
                                        }
                                        let Some(http_runtime) = accept_state
                                            .lock()
                                            .ok()
                                            .and_then(|server| server.http_runtime())
                                        else {
                                            if let Ok(mut server) = accept_state.lock() {
                                                server.release_connection();
                                            }
                                            continue;
                                        };
                                        let connection_state = accept_state.clone();
                                        let connection_handler = request_handler.clone();
                                        let work: crate::web::ConnectionWork = Box::new(move || {
                                            crate::web::ServerState::run_connection_worker(
                                                &connection_state,
                                                || {
                                                    if let Err(error) = crate::http::serve_connection_with_runtime(
                                                        stream,
                                                        connection_state.clone(),
                                                        Some(connection_handler),
                                                        &http_runtime,
                                                    ) && let Ok(mut server) = connection_state.lock() {
                                                        server.record_connection_error(
                                                            error.kind() == std::io::ErrorKind::TimedOut,
                                                        );
                                                    }
                                                },
                                            );
                                        });
                                        if !accept_state.lock().is_ok_and(|server| {
                                            server.submit_connection_work(work).is_ok()
                                        }) && let Ok(mut server) = accept_state.lock() {
                                            server.release_connection();
                                        }
                                    }
                                    Ok(None) => {}
                                    Err(_) => break,
                                }
                            }
                        })
                        .map_err(|error| runtime_error("WEB_LISTEN", error.to_string(), span))?;
                    state
                        .lock()
                        .map_err(|_| runtime_error("SERVER_STATE", "server state unavailable", span))?
                        .install_listener(listener_handle)
                        .map_err(|message| runtime_error("WEB_LISTEN", message, span))?;
                    Ok(Value::Null)
                }
                "StartTLS" | "StartTLSWithOptions" => {
                    let options = if method == "StartTLSWithOptions" {
                        require_arity(name, arguments, 4, span)?;
                        let Value::Object { handle, .. } = &arguments[3] else {
                            return Err(runtime_error("TYPE_MISMATCH", "StartTLSWithOptions expects ServerOptions", span));
                        };
                        self.web_server_options.get(handle).cloned().ok_or_else(|| {
                            runtime_error("STALE_HANDLE", "ServerOptions handle is not live", span)
                        })?
                    } else {
                        require_arity(name, arguments, 3, span)?;
                        crate::web::ServerOptions::default()
                    };
                    options.validate().map_err(|message| runtime_error("INVALID_OPTIONS", message, span))?;
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
                    let listener = crate::net::TcpListener::bind_with_backlog(endpoint, options.backlog)
                        .map_err(|error| runtime_error("WEB_LISTEN", error.to_string(), span))?;
                    let mut state_guard = state.lock().map_err(|_| {
                        runtime_error("SERVER_STATE", "server state unavailable", span)
                    })?;
                    let started = state_guard.start_with_options(options);
                    drop(state_guard);
                    if let Err(message) = started {
                        return Ok(Value::Error {
                            code: 1,
                            message: message.into(),
                        });
                    }
                    if let Err(message) = state
                        .lock()
                        .map_err(|_| runtime_error("SERVER_STATE", "server state unavailable", span))?
                        .install_worker_pool()
                    {
                        return Ok(Value::Error {
                            code: 1,
                            message: message.into(),
                        });
                    }
                    let request_handler = bn_server_handler(
                        self.module.clone(),
                        self.host.clone(),
                        self.web_handlers.get(handle).cloned().unwrap_or_default(),
                        self.web_filters.get(handle).cloned().unwrap_or_default(),
                        state.clone(),
                    );
                    let accept_state = state.clone();
                    let listener_handle = std::thread::Builder::new()
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
                                        let admitted = accept_state.lock().is_ok_and(|mut server| {
                                            server.admit_connection().is_ok()
                                                && server.track_connection_socket(&stream)
                                        });
                                        if !admitted {
                                            continue;
                                        }
                                        let Some(http_runtime) = accept_state
                                            .lock()
                                            .ok()
                                            .and_then(|server| server.http_runtime())
                                        else {
                                            if let Ok(mut server) = accept_state.lock() {
                                                server.release_connection();
                                            }
                                            continue;
                                        };
                                        let connection_state = accept_state.clone();
                                        let tls_config = config.clone();
                                        let connection_handler = request_handler.clone();
                                        let work: crate::web::ConnectionWork = Box::new(move || {
                                            crate::web::ServerState::run_connection_worker(
                                                &connection_state,
                                                || {
                                                    if let Err(error) = crate::http::serve_tls_connection_with_runtime(
                                                        stream,
                                                        connection_state.clone(),
                                                        tls_config,
                                                        Some(connection_handler),
                                                        &http_runtime,
                                                    ) && let Ok(mut server) = connection_state.lock() {
                                                        server.record_connection_error(
                                                            error.kind() == std::io::ErrorKind::TimedOut,
                                                        );
                                                    }
                                                },
                                            );
                                        });
                                        if !accept_state.lock().is_ok_and(|server| {
                                            server.submit_connection_work(work).is_ok()
                                        }) && let Ok(mut server) = accept_state.lock() {
                                            server.release_connection();
                                        }
                                    }
                                    Ok(None) => {}
                                    Err(_) => break,
                                }
                            }
                        })
                        .map_err(|error| runtime_error("WEB_LISTEN", error.to_string(), span))?;
                    state
                        .lock()
                        .map_err(|_| runtime_error("SERVER_STATE", "server state unavailable", span))?
                        .install_listener(listener_handle)
                        .map_err(|message| runtime_error("WEB_LISTEN", message, span))?;
                    Ok(Value::Null)
                }
                "Stop" => {
                    require_arity(name, arguments, 2, span)?;
                    let timeout = integer(&arguments[1], span)?.0;
                    let result = drain_server(&state, timeout, false);
                    Ok(result.map_or_else(
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
                            let request_id = crate::web_state::new_request_id(
                                &crate::web_state::SystemEntropy,
                            )
                            .ok();
                            if let Some(request_id) = request_id.as_deref()
                                && let Some(response) = self.web_responses.get_mut(response_handle)
                            {
                                let _ = response.set_header("X-Request-ID", request_id);
                            }
                            let result = self.call_named(
                                &handler,
                                vec![arguments[1].clone(), arguments[2].clone()],
                                span,
                            );
                            let status = self
                                .web_responses
                                .get(response_handle)
                                .map_or(500, |response| i128::from(response.status));
                            self.log_web_dispatch(
                                *handle,
                                &method_name,
                                &path,
                                status,
                                request_id.as_deref(),
                                span,
                            )?;
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
                                None,
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
                    let result = drain_server(&state, timeout, true);
                    Ok(result.map_or_else(
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

#[cfg(test)]
mod tests {
    use super::drain_server;
    use crate::web::ServerState;
    use std::sync::{Arc, Mutex};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;

    #[test]
    fn drain_server_times_out_without_holding_the_state_lock() {
        let state = Arc::new(Mutex::new(ServerState::new()));
        let release = Arc::new(AtomicBool::new(false));
        {
            let mut server = state.lock().unwrap();
            server.start().unwrap();
            server.admit_connection().unwrap();
        }
        let worker_release = release.clone();
        let worker = std::thread::spawn(move || {
            while !worker_release.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
        });
        {
            let mut server = state.lock().unwrap();
            server.track_connection_worker(worker);
        }
        let started = Instant::now();
        assert_eq!(
            drain_server(&state, 1, false),
            Err("server drain timed out with active connections")
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(1));

        release.store(true, Ordering::Release);
        state.lock().unwrap().release_connection();
        loop {
            let finished = {
                let mut server = state.lock().unwrap();
                server.reap_finished_workers();
                server.tracked_worker_count() == 0
            };
            if finished {
                break;
            }
            std::thread::yield_now();
        }
    }

    #[test]
    fn drain_server_bounds_listener_join_at_the_minimum_timeout() {
        let state = Arc::new(Mutex::new(ServerState::new()));
        let release = Arc::new(AtomicBool::new(false));
        let listener_release = release.clone();
        let listener = std::thread::spawn(move || {
            while !listener_release.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
        });
        state.lock().unwrap().install_listener(listener).unwrap();

        assert_eq!(
            drain_server(&state, 1, false),
            Err("server listener join timed out")
        );
        release.store(true, Ordering::Release);
        drain_server(&state, 1000, false).unwrap();
    }
}
