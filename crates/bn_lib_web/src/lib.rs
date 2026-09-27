// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNWeb` — an external library module served through the provider seam.
//! Its classes are ordinary objects in the core heap; this provider keeps
//! their state in side tables keyed by object handle and drops an entry when
//! the core destroys the object. Requests are served on transport threads by
//! an isolated executor over a clone of the module (`execute_callback`).

#![allow(clippy::too_many_lines, clippy::too_many_arguments)] // Moved verbatim from the core (bucket 0.5.1d); one arm per BNWeb member.

mod http;
#[cfg(test)]
mod test_support;
mod tls;
pub mod web;
mod web_state;

use std::collections::HashMap;

use bn_diag::Diagnostic;
use bn_ir::Module;
use bn_source::Span;
use bn_value::{Value, shared_string};

use bn_host_net::net_values::{address_value, net_endpoint, slots};
use bn_interp::provider::{CoreContext, Provider};
use bn_interp::{
    HostEnv, integer_from_count_pub as integer_from_count, integer_pub as integer,
    require_arity_pub as require_arity, run_isolated, runtime_error_pub as runtime_error,
    type_mismatch,
};
use bn_runtime::Handle;
use bn_types::IntegerType;

pub const NAME: &str = "BNWeb";

#[derive(Default)]
pub struct WebProvider {
    servers: HashMap<Handle, std::sync::Arc<std::sync::Mutex<crate::web::ServerState>>>,
    loggers: HashMap<Handle, u64>,
    tls_configs: HashMap<Handle, std::sync::Arc<rustls::ServerConfig>>,
    server_options: HashMap<Handle, crate::web::ServerOptions>,
    egress_policies: HashMap<Handle, crate::web::EgressPolicy>,
    cookie_jars: HashMap<Handle, crate::web_state::CookieJar>,
    session_stores: HashMap<Handle, crate::web_state::SessionStore>,
    acls: HashMap<Handle, crate::web_state::Acl>,
    scrapers: HashMap<Handle, crate::web_state::Scraper>,
    handlers: HashMap<Handle, HashMap<String, String>>,
    filters: HashMap<Handle, Vec<String>>,
    responses: HashMap<Handle, crate::web::Response>,
    requests: HashMap<Handle, crate::web::Request>,
    values: HashMap<Handle, Vec<String>>,
}

impl Provider for WebProvider {
    fn call(
        &mut self,
        core: &mut dyn CoreContext,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        // Idempotent; the core does not know TLS exists.
        crate::tls::install_ring_provider().map_err(|message| {
            runtime_error(bn_diag::DiagId::TLS_PROVIDER_UNAVAILABLE, message, span)
        })?;
        let name = format!("BNWeb.{member}");
        self.web_call(core, &name, &arguments, span)
    }

    fn object_destroyed(&mut self, handle: Handle) {
        self.servers.remove(&handle);
        self.loggers.remove(&handle);
        self.tls_configs.remove(&handle);
        self.server_options.remove(&handle);
        self.egress_policies.remove(&handle);
        self.cookie_jars.remove(&handle);
        self.session_stores.remove(&handle);
        self.acls.remove(&handle);
        self.scrapers.remove(&handle);
        self.handlers.remove(&handle);
        self.filters.remove(&handle);
        self.responses.remove(&handle);
        self.requests.remove(&handle);
        self.values.remove(&handle);
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

/// A provider-backed class's constructor / field initialiser. Standard
/// modules are not lowered, so such a callee has no `Function` (and no
/// `kind`) in the module; like an intrinsic it is identified by its
/// documented name shape through the contract's classifier.
fn is_lifecycle_stub(module: &Module, name: &str) -> bool {
    use bn_ir::names::{EmittedNameKind, classify};
    // `name` carries this library's prefix (`BNWeb.Client.CONSTRUCTOR`); the
    // classifier reads the member shape (`Client.CONSTRUCTOR`).
    let name = name.strip_prefix("BNWeb.").unwrap_or(name);
    match module.kind_of(name) {
        Some(kind) => matches!(
            kind,
            bn_ir::FunctionKind::Constructor | bn_ir::FunctionKind::FieldInit
        ),
        None => matches!(
            classify(name),
            Some(EmittedNameKind::Constructor | EmittedNameKind::FieldInit)
        ),
    }
}

/// Runs one `BNWeb` handler or filter for a request.
///
/// A network request must not borrow the executor that registered the
/// server: that executor may be running user code, and its heap is not
/// thread-safe. The callback receives copies of the request/response state
/// and returns the response projection to the transport layer.
///
/// # Errors
///
/// Returns the failure message the transport layer reports.
pub fn execute_callback(
    module: &Module,
    host: &HostEnv,
    function_name: &str,
    request: crate::web::Request,
    response: crate::web::Response,
) -> Result<crate::web::Response, String> {
    crate::tls::install_ring_provider().map_err(std::borrow::ToOwned::to_owned)?;
    if !host.filesystem().allows_capability()
        && let Some(span) = module.filesystem_import
    {
        return Err(runtime_error(
            bn_diag::DiagId::HOST_CAPABILITY_UNAVAILABLE,
            "HOST.FileSystem is not provided by this host",
            span,
        )
        .message
        .to_string());
    }
    run_isolated(module, host, |core| {
        let span = bn_interp::default_span();
        let request_value = core
            .allocate_object("BNWeb.Request", span)
            .map_err(|error| error.message.to_string())?;
        let response_value = core
            .allocate_object("BNWeb.Response", span)
            .map_err(|error| error.message.to_string())?;
        let request_handle = match &request_value {
            Value::Object { handle, .. } => *handle,
            _ => return Err("BNWeb callback object allocation failed".into()),
        };
        let response_handle = match &response_value {
            Value::Object { handle, .. } => *handle,
            _ => return Err("BNWeb callback object allocation failed".into()),
        };
        with_web_provider(core, |web| {
            web.requests.insert(request_handle, request);
            web.responses.insert(response_handle, response);
        })?;
        let result = core
            .call_function(function_name, vec![request_value, response_value], span)
            .map_err(|error| error.message.to_string())?;
        if let Value::Error { message, .. } = result {
            return Err(message.to_string());
        }
        with_web_provider(core, |web| web.responses.remove(&response_handle))?
            .ok_or_else(|| "BNWeb callback did not retain its response".into())
    })
}

/// Edits this library's state in `core`'s registry (take, edit, put back).
fn with_web_provider<R>(
    core: &mut dyn CoreContext,
    edit: impl FnOnce(&mut WebProvider) -> R,
) -> Result<R, String> {
    let mut provider = core
        .library_take(NAME)
        .ok_or_else(|| "BNWeb provider unavailable".to_string())?;
    let result = provider
        .as_any_mut()
        .and_then(|any| any.downcast_mut::<WebProvider>())
        .map(edit);
    core.library_insert(NAME, provider);
    result.ok_or_else(|| "BNWeb provider unavailable".to_string())
}

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
            current = execute_callback(&module, &host, filter, request.clone(), current)
                .map_err(|_| "BNWeb filter failed")?;
        }
        current = execute_callback(&module, &host, handler, request.clone(), current)
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
        let mut server = state.lock().map_err(|_| "server state unavailable")?;
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
            let mut server = state.lock().map_err(|_| "server state unavailable")?;
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
            let mut server = state.lock().map_err(|_| "server state unavailable")?;
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

impl WebProvider {
    /// Runs a BN function in the calling executor while this library stays
    /// reachable: the handler will call `Request.*`/`Response.*` on the same
    /// tables, so the state is parked in the core's registry for the duration
    /// (this provider was taken out of it to be called).
    fn call_reentrant(
        &mut self,
        core: &mut dyn CoreContext,
        function: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        core.library_insert(NAME, Box::new(std::mem::take(self)));
        let result = core.call_function(function, arguments, span);
        let mut parked = core.library_take(NAME).ok_or_else(|| {
            runtime_error(
                bn_diag::DiagId::LIBRARY_PROVIDER_UNAVAILABLE,
                "BNWeb state was lost during a handler",
                span,
            )
        })?;
        if let Some(web) = parked
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<WebProvider>())
        {
            *self = std::mem::take(web);
        }
        result
    }

    /// Access-log record for a served request, written through `BNLog`'s
    /// public contract (`Fields` + `Logger.Log`) exactly as a BN program would.
    fn log_web_dispatch(
        &mut self,
        core: &mut dyn CoreContext,
        server_handle: Handle,
        method: &str,
        path: &str,
        status: i128,
        request_id: Option<&str>,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let Some(&logger_id) = self.loggers.get(&server_handle) else {
            return Ok(());
        };
        let log = "BNLog";
        let Some(fields) = core.library_allocate(log, "Fields", span) else {
            return Ok(());
        };
        let fields = fields?;
        let mut entries = vec![
            ("http.method", method.to_string()),
            ("http.path", path.to_string()),
            ("http.status", status.to_string()),
        ];
        if let Some(request_id) = request_id {
            entries.push(("request_id", request_id.to_string()));
        }
        for (key, value) in entries {
            core.library_call(
                log,
                "Fields.SetString",
                vec![
                    fields.clone(),
                    Value::String(key.into()),
                    Value::String(shared_string(value)),
                ],
                span,
            )?;
        }
        let result = core.library_call(
            log,
            "Logger.Log",
            vec![
                Value::LogLogger(logger_id),
                Value::Integer(3, IntegerType::Int32),
                Value::String("web dispatch".into()),
                fields.clone(),
            ],
            span,
        );
        let _ = core.library_release(&fields, span);
        result.map(|_| ())
    }

    fn web_call(
        &mut self,
        core: &mut dyn CoreContext,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = name.rsplit('.').next().unwrap_or_default();
        if name.contains(".SessionStore.")
            || name.contains(".Scraper.")
            || name.contains(".ACL.")
            || name.contains(".CookieJar.")
            || name.contains(".TLSConfig.")
            || name.contains(".ServerOptions.")
            || name.contains(".EgressPolicy.")
            || name.contains(".HeaderValues.")
            || name.contains(".QueryValues.")
        {
            return self.web_state_call(core, name, arguments, span);
        }
        if name.contains(".Request.") {
            return self.web_request_call(core, name, arguments, span);
        }
        if name.contains(".Response.") {
            return self.web_response_call(core, name, arguments, span);
        }
        if name.contains(".Client.") {
            match method {
                "New" => {
                    require_arity(name, arguments, 1, span)?;
                    if !matches!(arguments.first(), Some(Value::Object { .. })) {
                        return Err(type_mismatch(
                            "BNLog.Logger",
                            "non-logger value",
                            "BNWeb.Client.New",
                            span,
                        ));
                    }
                    core.allocate_object("BNWeb.Client", span)
                }
                _ if is_lifecycle_stub(core.module(), name) => Ok(Value::Null),
                "Request" | "RequestWithPolicy" => {
                    let with_policy = method == "RequestWithPolicy";
                    require_arity(name, arguments, if with_policy { 5 } else { 4 }, span)?;
                    let (Value::String(method), Value::String(url), Value::String(body)) =
                        (&arguments[1], &arguments[2], &arguments[3])
                    else {
                        return Err(type_mismatch(
                            "STRING, STRING, STRING",
                            "non-STRING request argument",
                            "BNWeb.Client.Request",
                            span,
                        ));
                    };
                    if !matches!(
                        method.as_ref(),
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
                            return Err(type_mismatch(
                                "EgressPolicy",
                                "non-policy value",
                                "BNWeb.Client.RequestWithPolicy",
                                span,
                            ));
                        };
                        self.egress_policies.get(handle).ok_or_else(|| {
                            runtime_error(
                                bn_diag::DiagId::STALE_HANDLE,
                                "EgressPolicy handle is not live",
                                span,
                            )
                        })?
                    } else {
                        &crate::web::EgressPolicy::default()
                    };
                    let response =
                        match crate::http::client_request_with_policy(method, url, body, policy) {
                            Ok(response) => response,
                            Err(message) => {
                                return Ok(Value::Error {
                                    code: 1,
                                    message: shared_string(message),
                                });
                            }
                        };
                    let object = core.allocate_object("BNWeb.Response", span)?;
                    let Value::Object { handle, .. } = object else {
                        unreachable!("allocate_object returns object")
                    };
                    self.responses.insert(handle, response);
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
                    return Err(type_mismatch(
                        "BNLog.Logger",
                        "non-logger value",
                        "BNWeb.Server.New",
                        span,
                    ));
                }
                let object = core.allocate_object("BNWeb.Server", span)?;
                if let Value::Object { handle, .. } = object {
                    self.servers.insert(
                        handle,
                        std::sync::Arc::new(std::sync::Mutex::new(crate::web::ServerState::new())),
                    );
                    if let Some(Value::LogLogger(logger_id)) = arguments.first() {
                        self.loggers.insert(handle, *logger_id);
                    }
                    self.handlers.insert(handle, HashMap::new());
                    self.filters.insert(handle, Vec::new());
                }
                return Ok(object);
            }
            if method == "CONSTRUCTOR" {
                let Some(Value::Object { handle, .. }) = arguments.first() else {
                    return Err(type_mismatch(
                        "BNWeb.Server",
                        "non-object value",
                        "BNWeb.Server constructor receiver",
                        span,
                    ));
                };
                self.servers.insert(
                    *handle,
                    std::sync::Arc::new(std::sync::Mutex::new(crate::web::ServerState::new())),
                );
                self.loggers.remove(handle);
                self.handlers.insert(*handle, HashMap::new());
                self.filters.insert(*handle, Vec::new());
                return Ok(Value::Null);
            }
            let Some(Value::Object { handle, .. }) = arguments.first() else {
                return Err(type_mismatch(
                    "BNWeb.Server",
                    "non-object value",
                    "BNWeb.Server operation receiver",
                    span,
                ));
            };
            let state = self.servers.get(handle).cloned().ok_or_else(|| {
                runtime_error(
                    bn_diag::DiagId::STALE_HANDLE,
                    "BNWeb.Server handle is not live",
                    span,
                )
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
                    let filters = self.filters.entry(*handle).or_default();
                    if filters.len() >= 64 {
                        return Ok(Value::Error {
                            code: 1,
                            message: "filter limit exceeded".into(),
                        });
                    }
                    filters.push(filter.to_string());
                    Ok(Value::Null)
                }
                "Route" => {
                    require_arity(name, arguments, 4, span)?;
                    let (Value::String(method), Value::String(pattern)) =
                        (&arguments[1], &arguments[2])
                    else {
                        return Err(type_mismatch(
                            "STRING, STRING",
                            "non-STRING route argument",
                            "BNWeb.Server.Route",
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
                        runtime_error(
                            bn_diag::DiagId::SERVER_STATE,
                            "server state unavailable",
                            span,
                        )
                    })?;
                    let result = state.add_route(method.to_string(), pattern.to_string());
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
                    self.handlers
                        .entry(*handle)
                        .or_default()
                        .insert(format!("{method}\n{pattern}"), handler.to_string());
                    Ok(Value::Null)
                }
                "Status" => {
                    require_arity(name, arguments, 1, span)?;
                    let status = match state
                        .lock()
                        .map_err(|_| {
                            runtime_error(
                                bn_diag::DiagId::SERVER_STATE,
                                "server state unavailable",
                                span,
                            )
                        })?
                        .status()
                    {
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
                    Ok(Value::Boolean(
                        state
                            .lock()
                            .map_err(|_| {
                                runtime_error(
                                    bn_diag::DiagId::SERVER_STATE,
                                    "server state unavailable",
                                    span,
                                )
                            })?
                            .is_ready(),
                    ))
                }
                "ActiveConnections" => {
                    require_arity(name, arguments, 1, span)?;
                    Ok(Value::Integer(
                        state
                            .lock()
                            .map_err(|_| {
                                runtime_error(
                                    bn_diag::DiagId::SERVER_STATE,
                                    "server state unavailable",
                                    span,
                                )
                            })?
                            .active_connections() as i128,
                        IntegerType::Int32,
                    ))
                }
                "PendingRequests" => {
                    require_arity(name, arguments, 1, span)?;
                    Ok(Value::Integer(
                        state
                            .lock()
                            .map_err(|_| {
                                runtime_error(
                                    bn_diag::DiagId::SERVER_STATE,
                                    "server state unavailable",
                                    span,
                                )
                            })?
                            .pending_requests() as i128,
                        IntegerType::Int32,
                    ))
                }
                "AcceptedRequests"
                | "ActiveRequests"
                | "RejectedRequests"
                | "TimedOutRequests"
                | "CompletedRequests"
                | "FailedRequests"
                | "RateLimitedRequests"
                | "TotalRequestDurationMs"
                | "AverageRequestDurationMs"
                | "MaxRequestDurationMs" => {
                    require_arity(name, arguments, 1, span)?;
                    let snapshot = state
                        .lock()
                        .map_err(|_| {
                            runtime_error(
                                bn_diag::DiagId::SERVER_STATE,
                                "server state unavailable",
                                span,
                            )
                        })?
                        .stats();
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
                            return Err(type_mismatch(
                                "ServerOptions",
                                "non-options value",
                                "BNWeb.Server.StartWithOptions",
                                span,
                            ));
                        };
                        self.server_options.get(handle).cloned().ok_or_else(|| {
                            runtime_error(
                                bn_diag::DiagId::STALE_HANDLE,
                                "ServerOptions handle is not live",
                                span,
                            )
                        })?
                    } else {
                        require_arity(name, arguments, 2, span)?;
                        crate::web::ServerOptions::default()
                    };
                    options.validate().map_err(|message| {
                        runtime_error(bn_diag::DiagId::INVALID_OPTIONS, message, span)
                    })?;
                    let endpoint = net_endpoint(&arguments[1], span)?;
                    let listener =
                        bn_host_net::net::TcpListener::bind_with_backlog(endpoint, options.backlog)
                            .map_err(|error| {
                                runtime_error(bn_diag::DiagId::WEB_LISTEN, error.to_string(), span)
                            })?;
                    let mut state_guard = state.lock().map_err(|_| {
                        runtime_error(
                            bn_diag::DiagId::SERVER_STATE,
                            "server state unavailable",
                            span,
                        )
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
                        .map_err(|_| {
                            runtime_error(
                                bn_diag::DiagId::SERVER_STATE,
                                "server state unavailable",
                                span,
                            )
                        })?
                        .install_worker_pool()
                    {
                        return Ok(Value::Error {
                            code: 1,
                            message: message.into(),
                        });
                    }
                    let request_handler = bn_server_handler(
                        core.module().clone(),
                        core.host().clone(),
                        self.handlers.get(handle).cloned().unwrap_or_default(),
                        self.filters.get(handle).cloned().unwrap_or_default(),
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
                        .map_err(|error| runtime_error(bn_diag::DiagId::WEB_LISTEN, error.to_string(), span))?;
                    state
                        .lock()
                        .map_err(|_| {
                            runtime_error(
                                bn_diag::DiagId::SERVER_STATE,
                                "server state unavailable",
                                span,
                            )
                        })?
                        .install_listener(listener_handle)
                        .map_err(|message| {
                            runtime_error(bn_diag::DiagId::WEB_LISTEN, message, span)
                        })?;
                    Ok(Value::Null)
                }
                "StartTLS" | "StartTLSWithOptions" => {
                    let options = if method == "StartTLSWithOptions" {
                        require_arity(name, arguments, 4, span)?;
                        let Value::Object { handle, .. } = &arguments[3] else {
                            return Err(type_mismatch(
                                "ServerOptions",
                                "non-options value",
                                "BNWeb.Server.StartTLSWithOptions",
                                span,
                            ));
                        };
                        self.server_options.get(handle).cloned().ok_or_else(|| {
                            runtime_error(
                                bn_diag::DiagId::STALE_HANDLE,
                                "ServerOptions handle is not live",
                                span,
                            )
                        })?
                    } else {
                        require_arity(name, arguments, 3, span)?;
                        crate::web::ServerOptions::default()
                    };
                    options.validate().map_err(|message| {
                        runtime_error(bn_diag::DiagId::INVALID_OPTIONS, message, span)
                    })?;
                    let endpoint = net_endpoint(&arguments[1], span)?;
                    let Value::Object {
                        handle: config_handle,
                        ..
                    } = &arguments[2]
                    else {
                        return Err(type_mismatch(
                            "TLSConfig",
                            "non-TLSConfig value",
                            "BNWeb.Server.StartTLS",
                            span,
                        ));
                    };
                    let config = self
                        .tls_configs
                        .get(config_handle)
                        .cloned()
                        .ok_or_else(|| {
                            runtime_error(
                                bn_diag::DiagId::STALE_HANDLE,
                                "BNWeb.TLSConfig handle is not live",
                                span,
                            )
                        })?;
                    let listener =
                        bn_host_net::net::TcpListener::bind_with_backlog(endpoint, options.backlog)
                            .map_err(|error| {
                                runtime_error(bn_diag::DiagId::WEB_LISTEN, error.to_string(), span)
                            })?;
                    let mut state_guard = state.lock().map_err(|_| {
                        runtime_error(
                            bn_diag::DiagId::SERVER_STATE,
                            "server state unavailable",
                            span,
                        )
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
                        .map_err(|_| {
                            runtime_error(
                                bn_diag::DiagId::SERVER_STATE,
                                "server state unavailable",
                                span,
                            )
                        })?
                        .install_worker_pool()
                    {
                        return Ok(Value::Error {
                            code: 1,
                            message: message.into(),
                        });
                    }
                    let request_handler = bn_server_handler(
                        core.module().clone(),
                        core.host().clone(),
                        self.handlers.get(handle).cloned().unwrap_or_default(),
                        self.filters.get(handle).cloned().unwrap_or_default(),
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
                        .map_err(|error| runtime_error(bn_diag::DiagId::WEB_LISTEN, error.to_string(), span))?;
                    state
                        .lock()
                        .map_err(|_| {
                            runtime_error(
                                bn_diag::DiagId::SERVER_STATE,
                                "server state unavailable",
                                span,
                            )
                        })?
                        .install_listener(listener_handle)
                        .map_err(|message| {
                            runtime_error(bn_diag::DiagId::WEB_LISTEN, message, span)
                        })?;
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
                        return Err(type_mismatch(
                            "Request, Response",
                            "non-request/response values",
                            "BNWeb.Server.Dispatch",
                            span,
                        ));
                    };
                    let request = self.requests.get(request_handle).ok_or_else(|| {
                        runtime_error(
                            bn_diag::DiagId::STALE_HANDLE,
                            "BNWeb.Request handle is not live",
                            span,
                        )
                    })?;
                    let method_name = request.method.clone();
                    let path = request.target.path.clone();
                    let selected = {
                        let mut state = state.lock().map_err(|_| {
                            runtime_error(
                                bn_diag::DiagId::SERVER_STATE,
                                "server state unavailable",
                                span,
                            )
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
                            let filters = self.filters.get(handle).cloned().unwrap_or_default();
                            for filter in filters {
                                let result = self.call_reentrant(
                                    core,
                                    &filter,
                                    vec![arguments[1].clone(), arguments[2].clone()],
                                    span,
                                )?;
                                if let Value::Error { .. } = result {
                                    return Ok(result);
                                }
                            }
                            let handler = self
                                .handlers
                                .get(handle)
                                .and_then(|handlers| {
                                    handlers.get(&format!("{method_name}\n{pattern}"))
                                })
                                .cloned()
                                .ok_or_else(|| {
                                    runtime_error(
                                        bn_diag::DiagId::HANDLER_NOT_FOUND,
                                        "route handler is not live",
                                        span,
                                    )
                                })?;
                            let request_id =
                                crate::web_state::new_request_id(&crate::web_state::SystemEntropy)
                                    .ok();
                            if let Some(request_id) = request_id.as_deref()
                                && let Some(response) = self.responses.get_mut(response_handle)
                            {
                                let _ = response.set_header("X-Request-ID", request_id);
                            }
                            let result = self.call_reentrant(
                                core,
                                &handler,
                                vec![arguments[1].clone(), arguments[2].clone()],
                                span,
                            );
                            let status = self
                                .responses
                                .get(response_handle)
                                .map_or(500, |response| i128::from(response.status));
                            self.log_web_dispatch(
                                core,
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
                            if let Some(response) = self.responses.get_mut(response_handle) {
                                let _ = response.set_status(u16::try_from(status).unwrap_or(500));
                            }
                            self.log_web_dispatch(
                                core,
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
                _ if is_lifecycle_stub(core.module(), name) => Ok(Value::Null),
                _ => Ok(Value::Error {
                    code: 1,
                    message: "BNWeb provider unavailable".into(),
                }),
            }
        } else if is_lifecycle_stub(core.module(), name) {
            Ok(Value::Null)
        } else {
            Ok(Value::Error {
                code: 1,
                message: "BNWeb provider unavailable".into(),
            })
        }
    }

    fn web_state_call(
        &mut self,
        core: &mut dyn CoreContext,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = name.rsplit('.').next().unwrap_or_default();
        if name.contains(".SessionStore.") {
            if method == "New" {
                require_arity(name, arguments, 2, span)?;
                let capacity = integer(&arguments[0], span)?.0;
                let idle = integer(&arguments[1], span)?.0;
                if idle < 1 {
                    return Ok(Value::Error {
                        code: 1,
                        message: "invalid session idle timeout".into(),
                    });
                }
                let store = crate::web_state::SessionStore::new(
                    capacity,
                    std::time::Duration::from_millis(u64::try_from(idle).unwrap_or(0)),
                )
                .map_err(|message| runtime_error(bn_diag::DiagId::SESSION_CONFIG, message, span))?;
                let object = core.allocate_object("BNWeb.SessionStore", span)?;
                if let Value::Object { handle, .. } = object {
                    self.session_stores.insert(handle, store);
                }
                return Ok(object);
            }
            let Some(Value::Object { handle, .. }) = arguments.first() else {
                return Err(type_mismatch(
                    "SessionStore object",
                    "non-object value",
                    "SessionStore receiver",
                    span,
                ));
            };
            if method == "CONSTRUCTOR" {
                require_arity(name, arguments, 3, span)?;
                let capacity = integer(&arguments[1], span)?.0;
                let idle = integer(&arguments[2], span)?.0;
                if idle < 1 {
                    return Ok(Value::Error {
                        code: 1,
                        message: "invalid session idle timeout".into(),
                    });
                }
                let store = crate::web_state::SessionStore::new(
                    capacity,
                    std::time::Duration::from_millis(u64::try_from(idle).unwrap_or(0)),
                )
                .map_err(|message| runtime_error(bn_diag::DiagId::SESSION_CONFIG, message, span))?;
                self.session_stores.insert(*handle, store);
                return Ok(Value::Null);
            }
            let store = self.session_stores.get_mut(handle).ok_or_else(|| {
                runtime_error(
                    bn_diag::DiagId::STALE_HANDLE,
                    "SessionStore handle is not live",
                    span,
                )
            })?;
            match method {
                "Create" => {
                    require_arity(name, arguments, 2, span)?;
                    let Value::String(value) = &arguments[1] else {
                        return Err(type_mismatch(
                            "STRING",
                            "non-STRING value",
                            "SessionStore.Create",
                            span,
                        ));
                    };
                    Ok(store.create(value).map_or_else(
                        |message| Value::Error {
                            code: 1,
                            message: shared_string(message),
                        },
                        |value| Value::String(shared_string(value)),
                    ))
                }
                "Get" => {
                    require_arity(name, arguments, 2, span)?;
                    let Value::String(id) = &arguments[1] else {
                        return Err(type_mismatch(
                            "STRING",
                            "non-STRING value",
                            "SessionStore.Get",
                            span,
                        ));
                    };
                    Ok(store.get(id).map_or(
                        Value::Error {
                            code: 1,
                            message: "session not found".into(),
                        },
                        |value| Value::String(shared_string(value)),
                    ))
                }
                "Delete" => {
                    require_arity(name, arguments, 2, span)?;
                    let Value::String(id) = &arguments[1] else {
                        return Err(type_mismatch(
                            "STRING",
                            "non-STRING value",
                            "SessionStore.Delete",
                            span,
                        ));
                    };
                    Ok(store.delete(id).map_or_else(
                        |message| Value::Error {
                            code: 1,
                            message: shared_string(message),
                        },
                        |()| Value::Null,
                    ))
                }
                "Set" => {
                    require_arity(name, arguments, 3, span)?;
                    let (Value::String(id), Value::String(value)) = (&arguments[1], &arguments[2])
                    else {
                        return Err(type_mismatch(
                            "STRING, STRING",
                            "non-STRING argument",
                            "SessionStore.Set",
                            span,
                        ));
                    };
                    Ok(store.set(id, value).map_or_else(
                        |message| Value::Error {
                            code: 1,
                            message: shared_string(message),
                        },
                        |()| Value::Null,
                    ))
                }
                "Rotate" => {
                    require_arity(name, arguments, 3, span)?;
                    let (Value::String(id), Value::String(value)) = (&arguments[1], &arguments[2])
                    else {
                        return Err(type_mismatch(
                            "STRING, STRING",
                            "non-STRING argument",
                            "SessionStore.Rotate",
                            span,
                        ));
                    };
                    Ok(store.rotate(id, value).map_or_else(
                        |message| Value::Error {
                            code: 1,
                            message: shared_string(message),
                        },
                        |value| Value::String(shared_string(value)),
                    ))
                }
                _ => Ok(Value::Error {
                    code: 1,
                    message: "SessionStore provider unavailable".into(),
                }),
            }
        } else if name.contains(".Scraper.") {
            if method == "Parse" {
                require_arity(name, arguments, 1, span)?;
                let Value::String(html) = &arguments[0] else {
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "Scraper.Parse HTML",
                        span,
                    ));
                };
                let scraper = match crate::web_state::Scraper::parse(html) {
                    Ok(value) => value,
                    Err(message) => {
                        return Ok(Value::Error {
                            code: 1,
                            message: shared_string(message),
                        });
                    }
                };
                let object = core.allocate_object("BNWeb.Scraper", span)?;
                if let Value::Object { handle, .. } = object {
                    self.scrapers.insert(handle, scraper);
                }
                return Ok(object);
            }
            let Some(Value::Object { handle, .. }) = arguments.first() else {
                return Err(type_mismatch(
                    "Scraper object",
                    "non-object value",
                    "Scraper receiver",
                    span,
                ));
            };
            if method == "CONSTRUCTOR" {
                require_arity(name, arguments, 2, span)?;
                let Value::String(html) = &arguments[1] else {
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "Scraper constructor HTML",
                        span,
                    ));
                };
                let scraper = crate::web_state::Scraper::parse(html).map_err(|message| {
                    runtime_error(bn_diag::DiagId::SCRAPER_INPUT, message, span)
                })?;
                self.scrapers.insert(*handle, scraper);
                return Ok(Value::Null);
            }
            let scraper = self.scrapers.get(handle).ok_or_else(|| {
                runtime_error(
                    bn_diag::DiagId::STALE_HANDLE,
                    "Scraper handle is not live",
                    span,
                )
            })?;
            if method == "Text" {
                require_arity(name, arguments, 2, span)?;
                let Value::String(selector) = &arguments[1] else {
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "Scraper.Text selector",
                        span,
                    ));
                };
                return Ok(scraper.text(selector).map_or_else(
                    |message| Value::Error {
                        code: 1,
                        message: shared_string(message),
                    },
                    |value| Value::String(shared_string(value)),
                ));
            }
            Ok(Value::Error {
                code: 1,
                message: "Scraper provider unavailable".into(),
            })
        } else if name.contains(".ACL.") {
            let Some(Value::Object { handle, .. }) = arguments.first() else {
                return Err(type_mismatch(
                    "ACL object",
                    "non-object value",
                    "ACL receiver",
                    span,
                ));
            };
            if method == "CONSTRUCTOR" {
                self.acls.insert(*handle, crate::web_state::Acl::new());
                return Ok(Value::Null);
            }
            let acl = self.acls.get_mut(handle).ok_or_else(|| {
                runtime_error(
                    bn_diag::DiagId::STALE_HANDLE,
                    "ACL handle is not live",
                    span,
                )
            })?;
            match method {
                "Allow" | "Deny" => {
                    require_arity(name, arguments, 2, span)?;
                    let Value::Record { record } = &arguments[1] else {
                        return Err(type_mismatch(
                            "HOST.Net.CIDR",
                            "non-record value",
                            "ACL.Allow/Deny",
                            span,
                        ));
                    };
                    if record.type_name().as_ref() != "HOST.Net.CIDR"
                        || record.len() != slots::CIDR_FIELDS
                    {
                        return Err(type_mismatch(
                            "well-formed HOST.Net.CIDR",
                            "malformed record shape",
                            "ACL.Allow/Deny",
                            span,
                        ));
                    }
                    let (Value::String(network), Value::Integer(prefix, _)) = (
                        record.get(slots::CIDR_NETWORK).ok_or_else(|| {
                            type_mismatch("STRING", "missing network", "ACL CIDR", span)
                        })?,
                        record.get(slots::CIDR_PREFIX).ok_or_else(|| {
                            type_mismatch("INTEGER", "missing prefix", "ACL CIDR", span)
                        })?,
                    ) else {
                        return Err(type_mismatch(
                            "STRING, INTEGER",
                            "invalid CIDR fields",
                            "ACL CIDR",
                            span,
                        ));
                    };
                    let cidr = format!("{network}/{prefix}");
                    let result = if method == "Allow" {
                        acl.allow(&cidr)
                    } else {
                        acl.deny(&cidr)
                    };
                    Ok(result.map_or_else(
                        |message| Value::Error {
                            code: 1,
                            message: shared_string(message),
                        },
                        |()| Value::Null,
                    ))
                }
                "Check" => {
                    require_arity(name, arguments, 2, span)?;
                    let Value::Record { record } = &arguments[1] else {
                        return Err(type_mismatch(
                            "HOST.Net.Address",
                            "non-record value",
                            "ACL.Check",
                            span,
                        ));
                    };
                    if record.type_name().as_ref() != "HOST.Net.Address"
                        || record.len() != slots::ADDRESS_FIELDS
                    {
                        return Err(type_mismatch(
                            "well-formed HOST.Net.Address",
                            "malformed record shape",
                            "ACL.Check",
                            span,
                        ));
                    }
                    let Value::String(text) =
                        record.get(slots::ADDRESS_VALUE).ok_or_else(|| {
                            type_mismatch(
                                "value: STRING",
                                "missing field",
                                "ACL.Check address",
                                span,
                            )
                        })?
                    else {
                        return Err(type_mismatch(
                            "value: STRING",
                            "non-STRING field",
                            "ACL.Check address",
                            span,
                        ));
                    };
                    let address = text.parse().map_err(|_| {
                        type_mismatch(
                            "valid IP address",
                            "invalid address text",
                            "ACL.Check address",
                            span,
                        )
                    })?;
                    Ok(Value::Boolean(acl.check(address)))
                }
                _ => Ok(Value::Error {
                    code: 1,
                    message: "ACL provider unavailable".into(),
                }),
            }
        } else if name.contains(".CookieJar.") {
            let Some(Value::Object { handle, .. }) = arguments.first() else {
                return Err(type_mismatch(
                    "CookieJar object",
                    "non-object value",
                    "CookieJar receiver",
                    span,
                ));
            };
            if method == "CONSTRUCTOR" {
                self.cookie_jars
                    .insert(*handle, crate::web_state::CookieJar::new());
                return Ok(Value::Null);
            }
            let jar = self.cookie_jars.get_mut(handle).ok_or_else(|| {
                runtime_error(
                    bn_diag::DiagId::STALE_HANDLE,
                    "CookieJar handle is not live",
                    span,
                )
            })?;
            match method {
                "Set" => {
                    require_arity(name, arguments, 6, span)?;
                    let (Value::String(n), Value::String(v), Value::String(d), Value::String(p)) =
                        (&arguments[1], &arguments[2], &arguments[3], &arguments[4])
                    else {
                        return Err(type_mismatch(
                            "STRING, STRING, STRING, STRING",
                            "non-STRING argument",
                            "CookieJar.Set",
                            span,
                        ));
                    };
                    let age = integer(&arguments[5], span)?.0;
                    if age < 0 {
                        return Ok(Value::Error {
                            code: 1,
                            message: "negative cookie age".into(),
                        });
                    }
                    Ok(jar
                        .set(
                            n,
                            v,
                            d,
                            p,
                            std::time::Duration::from_millis(u64::try_from(age).unwrap_or(0)),
                        )
                        .map_or_else(
                            |message| Value::Error {
                                code: 1,
                                message: shared_string(message),
                            },
                            |()| Value::Null,
                        ))
                }
                "SetWithPolicy" => {
                    require_arity(name, arguments, 9, span)?;
                    let (
                        Value::String(n),
                        Value::String(v),
                        Value::String(d),
                        Value::String(p),
                        Value::Boolean(secure),
                        Value::Boolean(http_only),
                        Value::String(same_site),
                    ) = (
                        &arguments[1],
                        &arguments[2],
                        &arguments[3],
                        &arguments[4],
                        &arguments[6],
                        &arguments[7],
                        &arguments[8],
                    )
                    else {
                        return Err(type_mismatch(
                            "cookie policy types",
                            "incompatible argument",
                            "CookieJar.SetWithPolicy",
                            span,
                        ));
                    };
                    let age = integer(&arguments[5], span)?.0;
                    if age < 0 {
                        return Ok(Value::Error {
                            code: 1,
                            message: "negative cookie age".into(),
                        });
                    }
                    let same_site = match same_site.as_ref() {
                        "Strict" => crate::web_state::SameSite::Strict,
                        "Lax" => crate::web_state::SameSite::Lax,
                        "None" => crate::web_state::SameSite::None,
                        _ => {
                            return Ok(Value::Error {
                                code: 1,
                                message: "invalid SameSite policy".into(),
                            });
                        }
                    };
                    if same_site == crate::web_state::SameSite::None && !secure {
                        return Ok(Value::Error {
                            code: 1,
                            message: "SameSite=None requires Secure".into(),
                        });
                    }
                    Ok(jar
                        .set_with_options(
                            n,
                            v,
                            d,
                            p,
                            std::time::Duration::from_millis(u64::try_from(age).unwrap_or(0)),
                            crate::web_state::CookieOptions {
                                secure: *secure,
                                http_only: *http_only,
                                same_site,
                            },
                        )
                        .map_or_else(
                            |message| Value::Error {
                                code: 1,
                                message: shared_string(message),
                            },
                            |()| Value::Null,
                        ))
                }
                "Get" => {
                    require_arity(name, arguments, 4, span)?;
                    let (Value::String(n), Value::String(d), Value::String(p)) =
                        (&arguments[1], &arguments[2], &arguments[3])
                    else {
                        return Err(type_mismatch(
                            "STRING, STRING, STRING",
                            "non-STRING argument",
                            "CookieJar.Get",
                            span,
                        ));
                    };
                    Ok(jar.get(n, d, p).map_or(
                        Value::Error {
                            code: 1,
                            message: "cookie not found".into(),
                        },
                        |value| Value::String(shared_string(value)),
                    ))
                }
                "Delete" => {
                    require_arity(name, arguments, 4, span)?;
                    let (Value::String(n), Value::String(d), Value::String(p)) =
                        (&arguments[1], &arguments[2], &arguments[3])
                    else {
                        return Err(type_mismatch(
                            "STRING, STRING, STRING",
                            "non-STRING argument",
                            "CookieJar.Delete",
                            span,
                        ));
                    };
                    jar.delete(n, d, p);
                    Ok(Value::Null)
                }
                "Count" => Ok(Value::Integer(jar.len() as i128, IntegerType::Int32)),
                _ => Ok(Value::Error {
                    code: 1,
                    message: "CookieJar provider unavailable".into(),
                }),
            }
        } else if name.contains(".EgressPolicy.") {
            if method == "New" {
                require_arity(name, arguments, 5, span)?;
                let (Value::String(schemes), Value::String(cidrs), Value::String(ports)) =
                    (&arguments[0], &arguments[1], &arguments[2])
                else {
                    return Err(type_mismatch(
                        "STRING, STRING, STRING",
                        "non-STRING argument",
                        "EgressPolicy.New",
                        span,
                    ));
                };
                let max_redirects =
                    usize::try_from(integer(&arguments[3], span)?.0).map_err(|_| {
                        runtime_error(
                            bn_diag::DiagId::INVALID_EGRESS_POLICY,
                            "invalid redirect limit",
                            span,
                        )
                    })?;
                let deadline = u64::try_from(integer(&arguments[4], span)?.0).map_err(|_| {
                    runtime_error(
                        bn_diag::DiagId::INVALID_EGRESS_POLICY,
                        "invalid egress deadline",
                        span,
                    )
                })?;
                let policy = crate::web::EgressPolicy::from_csv(
                    schemes,
                    cidrs,
                    ports,
                    max_redirects,
                    deadline,
                )
                .map_err(|message| {
                    runtime_error(bn_diag::DiagId::INVALID_EGRESS_POLICY, message, span)
                })?;
                let object = core.allocate_object("BNWeb.EgressPolicy", span)?;
                if let Value::Object { handle, .. } = object {
                    self.egress_policies.insert(handle, policy);
                }
                Ok(object)
            } else {
                Ok(Value::Error {
                    code: 1,
                    message: "EgressPolicy provider unavailable".into(),
                })
            }
        } else if name.contains(".ServerOptions.") {
            let make_options = |offset: usize| -> Result<crate::web::ServerOptions, Diagnostic> {
                let value = |index: usize| {
                    usize::try_from(integer(&arguments[offset + index], span)?.0).map_err(|_| {
                        runtime_error(
                            bn_diag::DiagId::INVALID_OPTIONS,
                            "server option must be non-negative",
                            span,
                        )
                    })
                };
                let timeout = |index: usize| {
                    u64::try_from(integer(&arguments[offset + index], span)?.0).map_err(|_| {
                        runtime_error(
                            bn_diag::DiagId::INVALID_OPTIONS,
                            "server timeout must be non-negative",
                            span,
                        )
                    })
                };
                let trusted_proxy = match arguments.get(offset + 17) {
                    Some(Value::Boolean(value)) => *value,
                    _ => {
                        return Err(runtime_error(
                            bn_diag::DiagId::INVALID_OPTIONS,
                            "trustedProxy must be BOOLEAN",
                            span,
                        ));
                    }
                };
                let concurrent_handlers = match arguments.get(offset + 18) {
                    Some(Value::Boolean(value)) => *value,
                    _ => {
                        return Err(runtime_error(
                            bn_diag::DiagId::INVALID_OPTIONS,
                            "concurrentHandlers must be BOOLEAN",
                            span,
                        ));
                    }
                };
                Ok(crate::web::ServerOptions {
                    active_connections: value(0)?,
                    backlog: value(1)?,
                    pending_work: value(2)?,
                    worker_count: value(3)?,
                    max_header_bytes: value(4)?,
                    max_header_fields: value(5)?,
                    max_target_bytes: value(6)?,
                    max_body_bytes: value(7)?,
                    trusted_proxy,
                    tls_handshake_ms: timeout(8)?,
                    header_read_ms: timeout(9)?,
                    body_read_ms: timeout(10)?,
                    idle_keep_alive_ms: timeout(11)?,
                    connection_total_ms: timeout(12)?,
                    stop_drain_ms: timeout(13)?,
                    rate_limit_burst: value(14)?,
                    rate_limit_refill_per_second: value(15)?,
                    rate_limit_key_capacity: value(16)?,
                    concurrent_handlers,
                })
            };
            if method == "New" {
                require_arity(name, arguments, 19, span)?;
                let options = make_options(0)?;
                options.validate().map_err(|message| {
                    runtime_error(bn_diag::DiagId::INVALID_OPTIONS, message, span)
                })?;
                let object = core.allocate_object("BNWeb.ServerOptions", span)?;
                if let Value::Object { handle, .. } = object {
                    self.server_options.insert(handle, options);
                }
                Ok(object)
            } else {
                let Some(Value::Object { handle, .. }) = arguments.first() else {
                    return Err(type_mismatch(
                        "ServerOptions object",
                        "non-object value",
                        "ServerOptions receiver",
                        span,
                    ));
                };
                if method == "CONSTRUCTOR" {
                    require_arity(name, arguments, 20, span)?;
                    let options = make_options(1)?;
                    options.validate().map_err(|message| {
                        runtime_error(bn_diag::DiagId::INVALID_OPTIONS, message, span)
                    })?;
                    self.server_options.insert(*handle, options);
                    Ok(Value::Null)
                } else {
                    Ok(Value::Error {
                        code: 1,
                        message: "ServerOptions provider unavailable".into(),
                    })
                }
            }
        } else if name.contains(".TLSConfig.") {
            if method == "FromPEM" {
                require_arity(name, arguments, 2, span)?;
                let (Value::String(cert), Value::String(key)) = (&arguments[0], &arguments[1])
                else {
                    return Err(type_mismatch(
                        "STRING, STRING",
                        "non-STRING argument",
                        "TLSConfig.FromPEM",
                        span,
                    ));
                };
                let config = match crate::tls::server_config_from_pem(cert, key) {
                    Ok(config) => config,
                    Err(message) => {
                        return Ok(Value::Error {
                            code: 1,
                            message: shared_string(message),
                        });
                    }
                };
                let object = core.allocate_object("BNWeb.TLSConfig", span)?;
                if let Value::Object { handle, .. } = object {
                    self.tls_configs.insert(handle, std::sync::Arc::new(config));
                }
                return Ok(object);
            }
            let Some(Value::Object { handle, .. }) = arguments.first() else {
                return Err(type_mismatch(
                    "TLSConfig object",
                    "non-object value",
                    "TLSConfig receiver",
                    span,
                ));
            };
            if method == "CONSTRUCTOR" {
                require_arity(name, arguments, 3, span)?;
                let (Value::String(cert), Value::String(key)) = (&arguments[1], &arguments[2])
                else {
                    return Err(type_mismatch(
                        "STRING, STRING",
                        "non-STRING argument",
                        "TLSConfig constructor",
                        span,
                    ));
                };
                let config = match crate::tls::server_config_from_pem(cert, key) {
                    Ok(config) => config,
                    Err(message) => {
                        return Ok(Value::Error {
                            code: 1,
                            message: shared_string(message),
                        });
                    }
                };
                self.tls_configs
                    .insert(*handle, std::sync::Arc::new(config));
                return Ok(Value::Null);
            }
            if method == "FromPEM" {
                require_arity(name, arguments, 3, span)?;
                let (Value::String(cert), Value::String(key)) = (&arguments[1], &arguments[2])
                else {
                    return Err(type_mismatch(
                        "STRING, STRING",
                        "non-STRING argument",
                        "TLSConfig.FromPEM",
                        span,
                    ));
                };
                let config = match crate::tls::server_config_from_pem(cert, key) {
                    Ok(config) => config,
                    Err(message) => {
                        return Ok(Value::Error {
                            code: 1,
                            message: shared_string(message),
                        });
                    }
                };
                self.tls_configs
                    .insert(*handle, std::sync::Arc::new(config));
                return Ok(Value::Object {
                    handle: *handle,
                    class: "BNWeb.TLSConfig".into(),
                });
            }
            Ok(Value::Error {
                code: 1,
                message: "BNWeb.TLSConfig provider unavailable".into(),
            })
        } else if name.contains(".HeaderValues.") || name.contains(".QueryValues.") {
            if method == "CONSTRUCTOR" {
                let Some(Value::Object { handle, .. }) = arguments.first() else {
                    return Err(type_mismatch(
                        "BNWeb values object",
                        "non-object value",
                        "values constructor",
                        span,
                    ));
                };
                self.values.insert(*handle, Vec::new());
                return Ok(Value::Null);
            }
            let Some(Value::Object { handle, .. }) = arguments.first() else {
                return Err(type_mismatch(
                    "BNWeb values object",
                    "non-object value",
                    "values method",
                    span,
                ));
            };
            let values = self.values.get(handle).ok_or_else(|| {
                runtime_error(
                    bn_diag::DiagId::STALE_HANDLE,
                    "BNWeb values handle is not live",
                    span,
                )
            })?;
            match method {
                "Count" => {
                    require_arity(name, arguments, 1, span)?;
                    integer_from_count(values.len(), span)
                }
                "Get" => {
                    require_arity(name, arguments, 2, span)?;
                    let index = integer(&arguments[1], span)?.0;
                    let Ok(index) = usize::try_from(index) else {
                        return Ok(Value::Error {
                            code: 1,
                            message: "index is outside collection".into(),
                        });
                    };
                    Ok(values.get(index).map_or_else(
                        || Value::Error {
                            code: 1,
                            message: "index is outside collection".into(),
                        },
                        |value| Value::String(shared_string(value.as_str())),
                    ))
                }
                _ => Ok(Value::Error {
                    code: 1,
                    message: "BNWeb provider unavailable".into(),
                }),
            }
        } else {
            Err(runtime_error(
                bn_diag::DiagId::HOST_CAPABILITY_UNAVAILABLE,
                format!("web function '{name}' is not available"),
                span,
            ))
        }
    }

    fn web_request_call(
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
                bn_diag::DiagId::HOST_CAPABILITY_UNAVAILABLE,
                format!("web function '{name}' is not available"),
                span,
            ))
        }
    }

    fn web_response_call(
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
                        return Err(type_mismatch(
                            "STRING, STRING",
                            "non-STRING header name/value",
                            "BNWeb.Response.SetHeader",
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
                        return Err(type_mismatch(
                            "STRING",
                            "non-STRING value",
                            "BNWeb.Response.Header name",
                            span,
                        ));
                    };
                    response
                        .headers
                        .iter()
                        .find(|(name, _)| name.eq_ignore_ascii_case(key))
                        .map(|(_, value)| Value::String(shared_string(value.as_str())))
                        .ok_or_else(|| {
                            runtime_error(
                                bn_diag::DiagId::HEADER_NOT_FOUND,
                                "response header is not present",
                                span,
                            )
                        })
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
        } else {
            Err(runtime_error(
                bn_diag::DiagId::HOST_CAPABILITY_UNAVAILABLE,
                format!("web function '{name}' is not available"),
                span,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::drain_server;
    use crate::web::ServerState;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
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
