// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNWeb` — an external library module served through the provider seam.
//! Its classes are ordinary objects in the core heap; this provider keeps
//! their state in side tables keyed by object handle and drops an entry when
//! the core destroys the object. Requests are served on transport threads by
//! an isolated executor over a clone of the module (`execute_callback`).

#![allow(clippy::too_many_lines, clippy::too_many_arguments)] // Moved verbatim from the core; one arm per BNWeb member.
#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::string_slice,
    clippy::let_underscore_must_use,
    clippy::allow_attributes_without_reason,
    unreachable_pub,
    reason = "DEC-55 tier 3"
)]

pub use bn_web::web;
use bn_web::web::drain_server;
use bn_web::{http, tls, web_state};

/// Where the HTTPS client looks for `bn-cacert.pem` and `bn-ca.pem` (bnweb.md,
/// "HTTPS client trust roots"): `bni` names the entry file's directory.
pub use bn_web::tls_roots::{ProgramDirectory, set_program_directory};

mod access_log;
mod client_transfer;
mod request_response;
mod sessions;

use client_transfer::{arg_err, recv_err};

#[expect(clippy::verbose_file_reads)]
fn read_form_file(path: &str) -> Vec<u8> {
    use std::io::Read as _;
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let mut bytes = Vec::new();
    let _ = file.read_to_end(&mut bytes);
    bytes
}

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
use bn_web::WebFailure;

pub const NAME: &str = "BNWeb";

/// The `Error` of a failed member `name` (`BNWeb.Client.Request`),
/// with the report the native ABI records for the same failure.
fn failed(name: &str, failure: &WebFailure) -> Value {
    let mut parts = name.rsplit('.');
    let method = parts.next().unwrap_or_default();
    let class = parts.next().unwrap_or_default();
    let op_name = if class.is_empty() || class == "BNWeb" {
        format!("BNWeb.{method}")
    } else {
        format!("BNWeb.{class}.{method}")
    };
    Value::error_report(failure.code(), &op_name, failure.message(), failure.cause())
}

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
    /// `BNWeb.Session` objects: the request session each one reaches.
    request_sessions: HashMap<Handle, crate::web::sessions::RequestSession>,
    /// Logger id kept by each `BNWeb.Client`. Absent means the client does not log.
    clients: HashMap<Handle, u64>,
    client_replies: HashMap<Handle, crate::web::ClientReply>,
    headers: HashMap<Handle, crate::web::Headers>,
    queries: HashMap<Handle, crate::web::Query>,
    fields: HashMap<Handle, crate::web::Fields>,
    forms: HashMap<Handle, crate::web::Form>,
}

impl std::fmt::Debug for WebProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebProvider")
            .field("servers_count", &self.servers.len())
            .field("responses_count", &self.responses.len())
            .field("requests_count", &self.requests.len())
            .finish_non_exhaustive()
    }
}

impl WebProvider {
    fn record_client_access(
        &self,
        core: &mut dyn CoreContext,
        receiver: &Value,
        method: &str,
        url: &str,
        duration_ms: i64,
        outcome: &Result<crate::web::Response, String>,
    ) {
        let Value::Object { handle, .. } = receiver else {
            return;
        };
        let Some(logger_id) = self.clients.get(handle).copied() else {
            return;
        };
        let Ok(Some(logger)) = with_log_provider(core, |log| log.core_logger(logger_id)) else {
            return;
        };
        let Ok(logger) = logger.lock() else {
            return;
        };
        let sink = InterpLogSink {
            host: core.host().clone(),
        };
        let (status, bytes, headers) = match outcome {
            Ok(response) => {
                let headers = response
                    .headers
                    .iter()
                    .map(|(name, value)| (name.as_str(), value.as_str()))
                    .collect::<Vec<_>>();
                (
                    i64::from(response.status),
                    i64::try_from(response.body.len()).unwrap_or(i64::MAX),
                    headers,
                )
            }
            Err(_) => (0, 0, Vec::new()),
        };
        access_log::write_client_access(
            &logger,
            &sink,
            method,
            url,
            status,
            bytes,
            duration_ms,
            &headers,
        );
    }
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
        self.clients.remove(&handle);
        self.client_replies.remove(&handle);
        self.headers.remove(&handle);
        self.queries.remove(&handle);
        self.fields.remove(&handle);
        self.forms.remove(&handle);
        self.values.remove(&handle);
        self.request_sessions.remove(&handle);
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
    // Only a host without the provider (`--no-filesystem`, a notebook
    // kernel) fails before Start; a policy that denies every path still
    // binds the capability and each operation returns an Error.
    if !host.provides_filesystem()
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

use access_log::{InterpLogSink, with_log_provider};

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
        current.secure = request.secure;
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
        if name.contains(".Session.") {
            return self.web_session_call(name, arguments, span);
        }
        if name.contains(".Request.") {
            return self.web_request_call(core, name, arguments, span);
        }
        if name.contains(".Response.") {
            return self.web_response_call(core, name, arguments, span);
        }
        if name.contains(".ClientReply.") {
            return self.web_client_reply_call(core, name, arguments, span);
        }
        if name.contains(".Headers.") {
            return self.web_headers_call(core, name, arguments, span);
        }
        if name.contains(".Query.") {
            return self.web_query_call(core, name, arguments, span);
        }
        if name.contains(".Fields.") {
            return self.web_fields_call(core, name, arguments, span);
        }
        if name.contains(".Form.") {
            return self.web_form_call(core, name, arguments, span);
        }
        if name.contains(".Client.") {
            match method {
                "New" => {
                    require_arity(name, arguments, 1, span)?;
                    if !matches!(
                        arguments.first(),
                        Some(Value::Object { .. } | Value::LogLogger(_))
                    ) {
                        return Err(type_mismatch(
                            "BNLog.Logger",
                            "non-logger value",
                            "BNWeb.Client.New",
                            span,
                        ));
                    }
                    let object = core.allocate_object("BNWeb.Client", span)?;
                    if let (Value::Object { handle, .. }, Some(Value::LogLogger(logger_id))) =
                        (&object, arguments.first())
                    {
                        self.clients.insert(*handle, *logger_id);
                    }
                    Ok(object)
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
                        return Ok(failed(
                            name,
                            &WebFailure::UnsupportedMethod(method.to_string()),
                        ));
                    }
                    if let Err(message) = crate::web::validate_client_url(url) {
                        return Ok(failed(name, &WebFailure::from_message(message)));
                    }
                    if let Err(message) = crate::web::bounded_body(body, 8 * 1024 * 1024) {
                        return Ok(failed(name, &WebFailure::from_message(message)));
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
                    let started = std::time::Instant::now();
                    let outcome =
                        crate::http::client_request_with_policy(method, url, body, policy);
                    let duration_ms =
                        i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
                    self.record_client_access(
                        core,
                        &arguments[0],
                        method,
                        url,
                        duration_ms,
                        &outcome,
                    );
                    let response = match outcome {
                        Ok(response) => response,
                        Err(message) => {
                            return Ok(failed(name, &WebFailure::from_message(&message)));
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
                    if let Some(Value::Object { handle, .. }) = arguments.first() {
                        self.clients.remove(handle);
                    }
                    Ok(Value::Null)
                }
                "Get" | "GetDecoded" => {
                    require_arity(name, arguments, 3, span)?;
                    let (Value::String(url), Value::Integer(max, _)) =
                        (&arguments[1], &arguments[2])
                    else {
                        return Err(arg_err("Arguments", span));
                    };
                    let maximum = usize::try_from(*max).unwrap_or(usize::MAX);
                    let Value::Object { handle, .. } = &arguments[0] else {
                        return Err(recv_err("Client", span));
                    };
                    self.execute_client_transfer(
                        core,
                        name,
                        *handle,
                        "GET",
                        url,
                        &[],
                        None,
                        None,
                        maximum,
                        span,
                    )
                }
                "GetWithHeaders" | "GetDecodedWithHeaders" => {
                    require_arity(name, arguments, 4, span)?;
                    let (
                        Value::String(url),
                        Value::Object { handle: h_h, .. },
                        Value::Integer(max, _),
                    ) = (&arguments[1], &arguments[2], &arguments[3])
                    else {
                        return Err(arg_err("Arguments", span));
                    };
                    let maximum = usize::try_from(*max).unwrap_or(usize::MAX);
                    let Value::Object { handle, .. } = &arguments[0] else {
                        return Err(recv_err("Client", span));
                    };
                    self.execute_client_transfer(
                        core,
                        name,
                        *handle,
                        "GET",
                        url,
                        &[],
                        None,
                        Some(*h_h),
                        maximum,
                        span,
                    )
                }
                "GetQuery" => {
                    require_arity(name, arguments, 4, span)?;
                    let (
                        Value::String(url),
                        Value::Object { handle: q_h, .. },
                        Value::Integer(max, _),
                    ) = (&arguments[1], &arguments[2], &arguments[3])
                    else {
                        return Err(arg_err("Arguments", span));
                    };
                    let q_enc = self
                        .queries
                        .get(q_h)
                        .map(bn_web::Query::encode)
                        .unwrap_or_default();
                    let full_url = if url.contains('?') {
                        format!("{url}&{q_enc}")
                    } else {
                        format!("{url}?{q_enc}")
                    };
                    let maximum = usize::try_from(*max).unwrap_or(usize::MAX);
                    let Value::Object { handle, .. } = &arguments[0] else {
                        return Err(recv_err("Client", span));
                    };
                    self.execute_client_transfer(
                        core,
                        name,
                        *handle,
                        "GET",
                        &full_url,
                        &[],
                        None,
                        None,
                        maximum,
                        span,
                    )
                }
                "GetQueryWithHeaders" => {
                    require_arity(name, arguments, 5, span)?;
                    let (
                        Value::String(url),
                        Value::Object { handle: q_h, .. },
                        Value::Object { handle: h_h, .. },
                        Value::Integer(max, _),
                    ) = (&arguments[1], &arguments[2], &arguments[3], &arguments[4])
                    else {
                        return Err(arg_err("Arguments", span));
                    };
                    let q_enc = self
                        .queries
                        .get(q_h)
                        .map(bn_web::Query::encode)
                        .unwrap_or_default();
                    let full_url = if url.contains('?') {
                        format!("{url}&{q_enc}")
                    } else {
                        format!("{url}?{q_enc}")
                    };
                    let maximum = usize::try_from(*max).unwrap_or(usize::MAX);
                    let Value::Object { handle, .. } = &arguments[0] else {
                        return Err(recv_err("Client", span));
                    };
                    self.execute_client_transfer(
                        core,
                        name,
                        *handle,
                        "GET",
                        &full_url,
                        &[],
                        None,
                        Some(*h_h),
                        maximum,
                        span,
                    )
                }
                "Head" => {
                    require_arity(name, arguments, 3, span)?;
                    let (Value::String(url), Value::Integer(max, _)) =
                        (&arguments[1], &arguments[2])
                    else {
                        return Err(arg_err("Arguments", span));
                    };
                    let maximum = usize::try_from(*max).unwrap_or(usize::MAX);
                    let Value::Object { handle, .. } = &arguments[0] else {
                        return Err(recv_err("Client", span));
                    };
                    self.execute_client_transfer(
                        core,
                        name,
                        *handle,
                        "HEAD",
                        url,
                        &[],
                        None,
                        None,
                        maximum,
                        span,
                    )
                }
                "HeadWithHeaders" => {
                    require_arity(name, arguments, 4, span)?;
                    let (
                        Value::String(url),
                        Value::Object { handle: h_h, .. },
                        Value::Integer(max, _),
                    ) = (&arguments[1], &arguments[2], &arguments[3])
                    else {
                        return Err(arg_err("Arguments", span));
                    };
                    let maximum = usize::try_from(*max).unwrap_or(usize::MAX);
                    let Value::Object { handle, .. } = &arguments[0] else {
                        return Err(recv_err("Client", span));
                    };
                    self.execute_client_transfer(
                        core,
                        name,
                        *handle,
                        "HEAD",
                        url,
                        &[],
                        None,
                        Some(*h_h),
                        maximum,
                        span,
                    )
                }
                "Delete" => {
                    require_arity(name, arguments, 3, span)?;
                    let (Value::String(url), Value::Integer(max, _)) =
                        (&arguments[1], &arguments[2])
                    else {
                        return Err(arg_err("Arguments", span));
                    };
                    let maximum = usize::try_from(*max).unwrap_or(usize::MAX);
                    let Value::Object { handle, .. } = &arguments[0] else {
                        return Err(recv_err("Client", span));
                    };
                    self.execute_client_transfer(
                        core,
                        name,
                        *handle,
                        "DELETE",
                        url,
                        &[],
                        None,
                        None,
                        maximum,
                        span,
                    )
                }
                "DeleteWithHeaders" => {
                    require_arity(name, arguments, 4, span)?;
                    let (
                        Value::String(url),
                        Value::Object { handle: h_h, .. },
                        Value::Integer(max, _),
                    ) = (&arguments[1], &arguments[2], &arguments[3])
                    else {
                        return Err(arg_err("Arguments", span));
                    };
                    let maximum = usize::try_from(*max).unwrap_or(usize::MAX);
                    let Value::Object { handle, .. } = &arguments[0] else {
                        return Err(recv_err("Client", span));
                    };
                    self.execute_client_transfer(
                        core,
                        name,
                        *handle,
                        "DELETE",
                        url,
                        &[],
                        None,
                        Some(*h_h),
                        maximum,
                        span,
                    )
                }
                "Options" => {
                    require_arity(name, arguments, 3, span)?;
                    let (Value::String(url), Value::Integer(max, _)) =
                        (&arguments[1], &arguments[2])
                    else {
                        return Err(arg_err("Arguments", span));
                    };
                    let maximum = usize::try_from(*max).unwrap_or(usize::MAX);
                    let Value::Object { handle, .. } = &arguments[0] else {
                        return Err(recv_err("Client", span));
                    };
                    self.execute_client_transfer(
                        core,
                        name,
                        *handle,
                        "OPTIONS",
                        url,
                        &[],
                        None,
                        None,
                        maximum,
                        span,
                    )
                }
                "OptionsWithHeaders" => {
                    require_arity(name, arguments, 4, span)?;
                    let (
                        Value::String(url),
                        Value::Object { handle: h_h, .. },
                        Value::Integer(max, _),
                    ) = (&arguments[1], &arguments[2], &arguments[3])
                    else {
                        return Err(arg_err("Arguments", span));
                    };
                    let maximum = usize::try_from(*max).unwrap_or(usize::MAX);
                    let Value::Object { handle, .. } = &arguments[0] else {
                        return Err(recv_err("Client", span));
                    };
                    self.execute_client_transfer(
                        core,
                        name,
                        *handle,
                        "OPTIONS",
                        url,
                        &[],
                        None,
                        Some(*h_h),
                        maximum,
                        span,
                    )
                }
                "Post" | "Put" | "Patch" => {
                    require_arity(name, arguments, 5, span)?;
                    let (
                        Value::String(url),
                        Value::String(body),
                        Value::String(ct),
                        Value::Integer(max, _),
                    ) = (&arguments[1], &arguments[2], &arguments[3], &arguments[4])
                    else {
                        return Err(arg_err("Arguments", span));
                    };
                    let maximum = usize::try_from(*max).unwrap_or(usize::MAX);
                    let Value::Object { handle, .. } = &arguments[0] else {
                        return Err(recv_err("Client", span));
                    };
                    let verb = match method {
                        "Post" => "POST",
                        "Put" => "PUT",
                        _ => "PATCH",
                    };
                    self.execute_client_transfer(
                        core,
                        name,
                        *handle,
                        verb,
                        url,
                        body.as_bytes(),
                        Some(ct),
                        None,
                        maximum,
                        span,
                    )
                }
                "PostWithHeaders" | "PutWithHeaders" | "PatchWithHeaders" => {
                    require_arity(name, arguments, 6, span)?;
                    let (
                        Value::String(url),
                        Value::String(body),
                        Value::String(ct),
                        Value::Object { handle: h_h, .. },
                        Value::Integer(max, _),
                    ) = (
                        &arguments[1],
                        &arguments[2],
                        &arguments[3],
                        &arguments[4],
                        &arguments[5],
                    )
                    else {
                        return Err(arg_err("Arguments", span));
                    };
                    let maximum = usize::try_from(*max).unwrap_or(usize::MAX);
                    let Value::Object { handle, .. } = &arguments[0] else {
                        return Err(recv_err("Client", span));
                    };
                    let verb = match method {
                        "PostWithHeaders" => "POST",
                        "PutWithHeaders" => "PUT",
                        _ => "PATCH",
                    };
                    self.execute_client_transfer(
                        core,
                        name,
                        *handle,
                        verb,
                        url,
                        body.as_bytes(),
                        Some(ct),
                        Some(*h_h),
                        maximum,
                        span,
                    )
                }
                "PostBytes" | "PutBytes" | "PatchBytes" => {
                    require_arity(name, arguments, 5, span)?;
                    let (
                        Value::String(url),
                        Value::CryptoBytes(b_h),
                        Value::String(ct),
                        Value::Integer(max, _),
                    ) = (&arguments[1], &arguments[2], &arguments[3], &arguments[4])
                    else {
                        return Err(arg_err("Arguments", span));
                    };
                    let bytes =
                        bn_stdlib::crypto_abi::with_buffers(|b| b.get(b_h).map(|buf| buf.to_vec()))
                            .unwrap_or_default();
                    let maximum = usize::try_from(*max).unwrap_or(usize::MAX);
                    let Value::Object { handle, .. } = &arguments[0] else {
                        return Err(recv_err("Client", span));
                    };
                    let verb = match method {
                        "PostBytes" => "POST",
                        "PutBytes" => "PUT",
                        _ => "PATCH",
                    };
                    self.execute_client_transfer(
                        core,
                        name,
                        *handle,
                        verb,
                        url,
                        &bytes,
                        Some(ct),
                        None,
                        maximum,
                        span,
                    )
                }
                "PostBytesWithHeaders" | "PutBytesWithHeaders" | "PatchBytesWithHeaders" => {
                    require_arity(name, arguments, 6, span)?;
                    let (
                        Value::String(url),
                        Value::CryptoBytes(b_h),
                        Value::String(ct),
                        Value::Object { handle: h_h, .. },
                        Value::Integer(max, _),
                    ) = (
                        &arguments[1],
                        &arguments[2],
                        &arguments[3],
                        &arguments[4],
                        &arguments[5],
                    )
                    else {
                        return Err(arg_err("Arguments", span));
                    };
                    let bytes =
                        bn_stdlib::crypto_abi::with_buffers(|b| b.get(b_h).map(|buf| buf.to_vec()))
                            .unwrap_or_default();
                    let maximum = usize::try_from(*max).unwrap_or(usize::MAX);
                    let Value::Object { handle, .. } = &arguments[0] else {
                        return Err(recv_err("Client", span));
                    };
                    let verb = match method {
                        "PostBytesWithHeaders" => "POST",
                        "PutBytesWithHeaders" => "PUT",
                        _ => "PATCH",
                    };
                    self.execute_client_transfer(
                        core,
                        name,
                        *handle,
                        verb,
                        url,
                        &bytes,
                        Some(ct),
                        Some(*h_h),
                        maximum,
                        span,
                    )
                }
                "PostFields" => {
                    require_arity(name, arguments, 4, span)?;
                    let (
                        Value::String(url),
                        Value::Object { handle: f_h, .. },
                        Value::Integer(max, _),
                    ) = (&arguments[1], &arguments[2], &arguments[3])
                    else {
                        return Err(arg_err("Arguments", span));
                    };
                    let enc = self
                        .fields
                        .get(f_h)
                        .map(bn_web::Fields::encode)
                        .unwrap_or_default();
                    let maximum = usize::try_from(*max).unwrap_or(usize::MAX);
                    let Value::Object { handle, .. } = &arguments[0] else {
                        return Err(recv_err("Client", span));
                    };
                    self.execute_client_transfer(
                        core,
                        name,
                        *handle,
                        "POST",
                        url,
                        enc.as_bytes(),
                        Some("application/x-www-form-urlencoded"),
                        None,
                        maximum,
                        span,
                    )
                }
                "PostFieldsWithHeaders" => {
                    require_arity(name, arguments, 5, span)?;
                    let (
                        Value::String(url),
                        Value::Object { handle: f_h, .. },
                        Value::Object { handle: h_h, .. },
                        Value::Integer(max, _),
                    ) = (&arguments[1], &arguments[2], &arguments[3], &arguments[4])
                    else {
                        return Err(arg_err("Arguments", span));
                    };
                    let enc = self
                        .fields
                        .get(f_h)
                        .map(bn_web::Fields::encode)
                        .unwrap_or_default();
                    let maximum = usize::try_from(*max).unwrap_or(usize::MAX);
                    let Value::Object { handle, .. } = &arguments[0] else {
                        return Err(recv_err("Client", span));
                    };
                    self.execute_client_transfer(
                        core,
                        name,
                        *handle,
                        "POST",
                        url,
                        enc.as_bytes(),
                        Some("application/x-www-form-urlencoded"),
                        Some(*h_h),
                        maximum,
                        span,
                    )
                }
                "PostForm" => {
                    require_arity(name, arguments, 4, span)?;
                    let (
                        Value::String(url),
                        Value::Object { handle: f_h, .. },
                        Value::Integer(max, _),
                    ) = (&arguments[1], &arguments[2], &arguments[3])
                    else {
                        return Err(arg_err("Arguments", span));
                    };
                    let form_data = self.forms.get(f_h).cloned().unwrap_or_default();
                    let boundary = "----BNWebFormBoundary7MA4YWxkTrZu0gW";
                    let mut body = Vec::new();
                    for (k, v) in form_data.fields {
                        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n").as_bytes());
                    }
                    for (name_f, path, ct) in form_data.files {
                        let content = read_form_file(&path);
                        let filename = std::path::Path::new(&path)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("file");
                        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name_f}\"; filename=\"{filename}\"\r\nContent-Type: {ct}\r\n\r\n").as_bytes());
                        body.extend_from_slice(&content);
                        body.extend_from_slice(b"\r\n");
                    }
                    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
                    let ct = format!("multipart/form-data;boundary={boundary}");
                    let maximum = usize::try_from(*max).unwrap_or(usize::MAX);
                    let Value::Object { handle, .. } = &arguments[0] else {
                        return Err(recv_err("Client", span));
                    };
                    self.execute_client_transfer(
                        core,
                        name,
                        *handle,
                        "POST",
                        url,
                        &body,
                        Some(&ct),
                        None,
                        maximum,
                        span,
                    )
                }
                "PostFormWithHeaders" => {
                    require_arity(name, arguments, 5, span)?;
                    let (
                        Value::String(url),
                        Value::Object { handle: f_h, .. },
                        Value::Object { handle: h_h, .. },
                        Value::Integer(max, _),
                    ) = (&arguments[1], &arguments[2], &arguments[3], &arguments[4])
                    else {
                        return Err(arg_err("Arguments", span));
                    };
                    let form_data = self.forms.get(f_h).cloned().unwrap_or_default();
                    let boundary = "----BNWebFormBoundary7MA4YWxkTrZu0gW";
                    let mut body = Vec::new();
                    for (k, v) in form_data.fields {
                        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n").as_bytes());
                    }
                    for (name_f, path, ct) in form_data.files {
                        let content = read_form_file(&path);
                        let filename = std::path::Path::new(&path)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("file");
                        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name_f}\"; filename=\"{filename}\"\r\nContent-Type: {ct}\r\n\r\n").as_bytes());
                        body.extend_from_slice(&content);
                        body.extend_from_slice(b"\r\n");
                    }
                    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
                    let ct = format!("multipart/form-data;boundary={boundary}");
                    let maximum = usize::try_from(*max).unwrap_or(usize::MAX);
                    let Value::Object { handle, .. } = &arguments[0] else {
                        return Err(recv_err("Client", span));
                    };
                    self.execute_client_transfer(
                        core,
                        name,
                        *handle,
                        "POST",
                        url,
                        &body,
                        Some(&ct),
                        Some(*h_h),
                        maximum,
                        span,
                    )
                }
                "PostJson" => {
                    require_arity(name, arguments, 4, span)?;
                    let (Value::String(url), Value::String(json), Value::Integer(max, _)) =
                        (&arguments[1], &arguments[2], &arguments[3])
                    else {
                        return Err(arg_err("Arguments", span));
                    };
                    let maximum = usize::try_from(*max).unwrap_or(usize::MAX);
                    let Value::Object { handle, .. } = &arguments[0] else {
                        return Err(recv_err("Client", span));
                    };
                    self.execute_client_transfer(
                        core,
                        name,
                        *handle,
                        "POST",
                        url,
                        json.as_bytes(),
                        Some("application/json"),
                        None,
                        maximum,
                        span,
                    )
                }
                "PostJsonWithHeaders" => {
                    require_arity(name, arguments, 5, span)?;
                    let (
                        Value::String(url),
                        Value::String(json),
                        Value::Object { handle: h_h, .. },
                        Value::Integer(max, _),
                    ) = (&arguments[1], &arguments[2], &arguments[3], &arguments[4])
                    else {
                        return Err(arg_err("Arguments", span));
                    };
                    let maximum = usize::try_from(*max).unwrap_or(usize::MAX);
                    let Value::Object { handle, .. } = &arguments[0] else {
                        return Err(recv_err("Client", span));
                    };
                    self.execute_client_transfer(
                        core,
                        name,
                        *handle,
                        "POST",
                        url,
                        json.as_bytes(),
                        Some("application/json"),
                        Some(*h_h),
                        maximum,
                        span,
                    )
                }
                "RequestMethod" => {
                    require_arity(name, arguments, 6, span)?;
                    let (
                        Value::String(verb),
                        Value::String(url),
                        Value::String(body),
                        Value::String(ct),
                        Value::Integer(max, _),
                    ) = (
                        &arguments[1],
                        &arguments[2],
                        &arguments[3],
                        &arguments[4],
                        &arguments[5],
                    )
                    else {
                        return Err(arg_err("Arguments", span));
                    };
                    let maximum = usize::try_from(*max).unwrap_or(usize::MAX);
                    let Value::Object { handle, .. } = &arguments[0] else {
                        return Err(recv_err("Client", span));
                    };
                    self.execute_client_transfer(
                        core,
                        name,
                        *handle,
                        verb,
                        url,
                        body.as_bytes(),
                        Some(ct),
                        None,
                        maximum,
                        span,
                    )
                }
                "RequestMethodWithHeaders" => {
                    require_arity(name, arguments, 7, span)?;
                    let (
                        Value::String(verb),
                        Value::String(url),
                        Value::String(body),
                        Value::String(ct),
                        Value::Object { handle: h_h, .. },
                        Value::Integer(max, _),
                    ) = (
                        &arguments[1],
                        &arguments[2],
                        &arguments[3],
                        &arguments[4],
                        &arguments[5],
                        &arguments[6],
                    )
                    else {
                        return Err(arg_err("Arguments", span));
                    };
                    let maximum = usize::try_from(*max).unwrap_or(usize::MAX);
                    let Value::Object { handle, .. } = &arguments[0] else {
                        return Err(recv_err("Client", span));
                    };
                    self.execute_client_transfer(
                        core,
                        name,
                        *handle,
                        verb,
                        url,
                        body.as_bytes(),
                        Some(ct),
                        Some(*h_h),
                        maximum,
                        span,
                    )
                }
                _ => Ok(failed(name, &WebFailure::ProviderUnavailable("BNWeb"))),
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
                    let server_state =
                        std::sync::Arc::new(std::sync::Mutex::new(crate::web::ServerState::new()));
                    if let Some(Value::LogLogger(logger_id)) = arguments.first() {
                        self.loggers.insert(handle, *logger_id);
                        if let Ok(Some(logger)) =
                            with_log_provider(core, |log| log.core_logger(*logger_id))
                        {
                            let sink = std::sync::Arc::new(InterpLogSink {
                                host: core.host().clone(),
                            });
                            if let Ok(mut server) = server_state.lock() {
                                server.set_logger(logger, sink);
                            }
                        }
                    }
                    self.servers.insert(handle, server_state);
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
                "UseQueue" => Self::server_use_queue(core, name, arguments, &state, span),
                "UseSessions" => Self::server_use_sessions(name, arguments, &state, span),
                "AddFilter" => {
                    require_arity(name, arguments, 2, span)?;
                    let Value::Function(filter) = &arguments[1] else {
                        return Ok(failed(name, &WebFailure::FilterMustBeFunction));
                    };
                    let filters = self.filters.entry(*handle).or_default();
                    if filters.len() >= 64 {
                        return Ok(failed(name, &WebFailure::FilterLimitExceeded));
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
                        return Ok(failed(name, &WebFailure::RouteHandlerMustBeFunction));
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
                        return Ok(failed(name, &WebFailure::from_message(message)));
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
                "Static" => {
                    require_arity(name, arguments, 3, span)?;
                    let (Value::String(prefix), Value::String(directory)) =
                        (&arguments[1], &arguments[2])
                    else {
                        return Err(type_mismatch(
                            "STRING, STRING",
                            "non-STRING static argument",
                            "BNWeb.Server.Static",
                            span,
                        ));
                    };
                    if core.module().filesystem_import.is_none() {
                        return Ok(failed(
                            name,
                            &WebFailure::Unavailable(
                                "HOST.FileSystem is required for static file serving".to_string(),
                            ),
                        ));
                    }
                    if !core.host().provides_filesystem() {
                        return Ok(failed(
                            name,
                            &WebFailure::Unavailable(
                                "HOST.FileSystem is denied by host policy".to_string(),
                            ),
                        ));
                    }
                    let mut state = state.lock().map_err(|_| {
                        runtime_error(
                            bn_diag::DiagId::SERVER_STATE,
                            "server state unavailable",
                            span,
                        )
                    })?;
                    let policy = core.host().filesystem().clone();
                    state.set_static_opener(std::sync::Arc::new(
                        crate::access_log::InterpStaticFileOpener { policy },
                    ));
                    match state.add_static(prefix.as_ref(), directory.as_ref()) {
                        Ok(()) => Ok(Value::Null),
                        Err(message) => Ok(failed(name, &WebFailure::from_message(message))),
                    }
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
                    let listener = match bn_host_net::net::TcpListener::bind_with_backlog(
                        endpoint,
                        options.backlog,
                    ) {
                        Ok(listener) => listener,
                        Err(error) => {
                            return Ok(failed(name, &WebFailure::from_message(&error.to_string())));
                        }
                    };
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
                        return Ok(failed(name, &WebFailure::from_message(message)));
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
                        return Ok(failed(name, &WebFailure::from_message(message)));
                    }
                    let request_handler = bn_server_handler(
                        core.module().clone(),
                        core.host().clone(),
                        self.handlers.get(handle).cloned().unwrap_or_default(),
                        self.filters.get(handle).cloned().unwrap_or_default(),
                        std::sync::Arc::clone(&state),
                    );
                    crate::web::spawn_listener_thread(&state, listener, request_handler, None)
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
                    let listener = match bn_host_net::net::TcpListener::bind_with_backlog(
                        endpoint,
                        options.backlog,
                    ) {
                        Ok(listener) => listener,
                        Err(error) => {
                            return Ok(failed(name, &WebFailure::from_message(&error.to_string())));
                        }
                    };
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
                        return Ok(failed(name, &WebFailure::from_message(message)));
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
                        return Ok(failed(name, &WebFailure::from_message(message)));
                    }
                    let request_handler = bn_server_handler(
                        core.module().clone(),
                        core.host().clone(),
                        self.handlers.get(handle).cloned().unwrap_or_default(),
                        self.filters.get(handle).cloned().unwrap_or_default(),
                        std::sync::Arc::clone(&state),
                    );
                    crate::web::spawn_listener_thread(
                        &state,
                        listener,
                        request_handler,
                        Some(config),
                    )
                    .map_err(|message| runtime_error(bn_diag::DiagId::WEB_LISTEN, message, span))?;
                    Ok(Value::Null)
                }
                "Stop" => {
                    require_arity(name, arguments, 2, span)?;
                    let timeout = integer(&arguments[1], span)?.0;
                    let result = drain_server(&state, timeout, false);
                    Ok(result.map_or_else(
                        |message| failed(name, &WebFailure::from_message(message)),
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
                                    crate::web::RouteOutcome::Matched(route, found) => Ok((
                                        route.pattern().to_owned(),
                                        crate::web::sorted_params(&found),
                                    )),
                                    crate::web::RouteOutcome::MethodNotAllowed(_) => Err(405),
                                    crate::web::RouteOutcome::NotFound => Err(404),
                                });
                            })
                            .is_err()
                        {
                            return Ok(failed(name, &WebFailure::ServerNotAccepting));
                        }
                        selected.unwrap_or(Err(500))
                    };
                    match selected {
                        Ok((pattern, params)) => {
                            if let Some(stored) = self.requests.get_mut(request_handle) {
                                stored.params = params;
                            }
                            self.begin_request_session(*request_handle, &state);
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
                            self.apply_request_session(*request_handle, *response_handle);
                            let status = self
                                .responses
                                .get(response_handle)
                                .map_or(500, |response| response.status);
                            if let Ok(server) = state.lock() {
                                server.log_access(
                                    &method_name,
                                    &path,
                                    status,
                                    request_id.as_deref(),
                                );
                            }
                            result
                        }
                        Err(404) => {
                            let static_data = {
                                let server = state.lock().map_err(|_| {
                                    runtime_error(
                                        bn_diag::DiagId::SERVER_STATE,
                                        "server state unavailable",
                                        span,
                                    )
                                })?;
                                server.match_static_mount(&path).map(|(m, sub)| {
                                    (m.clone(), sub.to_owned(), server.static_opener())
                                })
                            };
                            if let Some((mount, subpath, opener)) = static_data {
                                let inm = request
                                    .header("If-None-Match")
                                    .ok()
                                    .and_then(|h| h.get(0).map(str::to_owned));
                                let ims = request
                                    .header("If-Modified-Since")
                                    .ok()
                                    .and_then(|h| h.get(0).map(str::to_owned));
                                let limits = bn_limits::web_limits();
                                let raw_target = &request.target.path;
                                let outcome = crate::web::serve_static_file(
                                    &mount,
                                    raw_target,
                                    &subpath,
                                    &method_name,
                                    inm.as_deref(),
                                    ims.as_deref(),
                                    limits.max_body_bytes,
                                    opener.as_ref(),
                                );
                                if let Some(response) = self.responses.get_mut(response_handle) {
                                    apply_static_outcome_to_response(outcome, response);
                                }
                                let status = self
                                    .responses
                                    .get(response_handle)
                                    .map_or(500, |r| r.status);
                                if let Ok(server) = state.lock() {
                                    server.log_access(&method_name, &path, status, None);
                                }
                                return Ok(Value::Null);
                            }
                            if let Some(response) = self.responses.get_mut(response_handle) {
                                let _ = response.set_status(404);
                            }
                            if let Ok(server) = state.lock() {
                                server.log_access(&method_name, &path, 404, None);
                            }
                            Ok(Value::Null)
                        }
                        Err(405) => {
                            if let Some(response) = self.responses.get_mut(response_handle) {
                                let _ = response.set_status(405);
                            }
                            if let Ok(server) = state.lock() {
                                server.log_access(&method_name, &path, 405, None);
                            }
                            Ok(Value::Null)
                        }
                        Err(_) => Ok(failed(name, &WebFailure::RequestDispatchFailed)),
                    }
                }
                "Close" => {
                    require_arity(name, arguments, 2, span)?;
                    let timeout = integer(&arguments[1], span)?.0;
                    let result = drain_server(&state, timeout, true);
                    Ok(result.map_or_else(
                        |message| failed(name, &WebFailure::from_message(message)),
                        |()| Value::Null,
                    ))
                }
                _ if is_lifecycle_stub(core.module(), name) => Ok(Value::Null),
                _ => Ok(failed(name, &WebFailure::ProviderUnavailable("BNWeb"))),
            }
        } else if is_lifecycle_stub(core.module(), name) {
            Ok(Value::Null)
        } else {
            Ok(failed(name, &WebFailure::ProviderUnavailable("BNWeb")))
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
                    return Ok(failed(name, &WebFailure::InvalidSessionIdleTimeout));
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
                    return Ok(failed(name, &WebFailure::InvalidSessionIdleTimeout));
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
                        |message| failed(name, &WebFailure::from_message(&message)),
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
                    Ok(store.get(id).map_or_else(
                        || failed(name, &WebFailure::SessionNotFound),
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
                        |message| failed(name, &WebFailure::from_message(&message)),
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
                        |message| failed(name, &WebFailure::from_message(&message)),
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
                        |message| failed(name, &WebFailure::from_message(&message)),
                        |value| Value::String(shared_string(value)),
                    ))
                }
                _ => Ok(failed(
                    name,
                    &WebFailure::ProviderUnavailable("SessionStore"),
                )),
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
                        return Ok(failed(name, &WebFailure::from_message(&message)));
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
                    |message| failed(name, &WebFailure::from_message(&message)),
                    |value| Value::String(shared_string(value)),
                ));
            }
            Ok(failed(name, &WebFailure::ProviderUnavailable("Scraper")))
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
                        |message| failed(name, &WebFailure::from_message(&message)),
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
                _ => Ok(failed(name, &WebFailure::ProviderUnavailable("ACL"))),
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
                        return Ok(failed(name, &WebFailure::NegativeCookieAge));
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
                            |message| failed(name, &WebFailure::from_message(&message)),
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
                    // One policy rule for jars and responses, on both backends.
                    let age =
                        match crate::web_state::cookie_max_age(integer(&arguments[5], span)?.0) {
                            Ok(age) => age,
                            Err(message) => {
                                return Ok(failed(name, &WebFailure::from_message(message)));
                            }
                        };
                    let options = match crate::web_state::CookieOptions::from_policy(
                        *secure, *http_only, same_site,
                    ) {
                        Ok(options) => options,
                        Err(message) => {
                            return Ok(failed(name, &WebFailure::from_message(message)));
                        }
                    };
                    Ok(jar.set_with_options(n, v, d, p, age, options).map_or_else(
                        |message| failed(name, &WebFailure::from_message(&message)),
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
                    Ok(jar.get(n, d, p).map_or_else(
                        || failed(name, &WebFailure::CookieNotFound),
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
                _ => Ok(failed(name, &WebFailure::ProviderUnavailable("CookieJar"))),
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
                // `EgressPolicy OR Error`: a setting outside its bounds is an
                // `INVALID_ARGUMENT` the program handles (bnweb.md, "Errors").
                let (Ok(max_redirects), Ok(deadline)) = (
                    usize::try_from(integer(&arguments[3], span)?.0),
                    u64::try_from(integer(&arguments[4], span)?.0),
                ) else {
                    return Ok(failed(
                        name,
                        &WebFailure::InvalidArgument(crate::web::EGRESS_POLICY_BOUNDS.into()),
                    ));
                };
                let policy = match crate::web::EgressPolicy::from_csv(
                    schemes,
                    cidrs,
                    ports,
                    max_redirects,
                    deadline,
                ) {
                    Ok(policy) => policy,
                    Err(message) => {
                        return Ok(failed(name, &WebFailure::InvalidArgument(message.into())));
                    }
                };
                let object = core.allocate_object("BNWeb.EgressPolicy", span)?;
                if let Value::Object { handle, .. } = object {
                    self.egress_policies.insert(handle, policy);
                }
                Ok(object)
            } else {
                Ok(failed(
                    name,
                    &WebFailure::ProviderUnavailable("EgressPolicy"),
                ))
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
                })
            };
            if method == "New" {
                require_arity(name, arguments, 18, span)?;
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
                    require_arity(name, arguments, 19, span)?;
                    let options = make_options(1)?;
                    options.validate().map_err(|message| {
                        runtime_error(bn_diag::DiagId::INVALID_OPTIONS, message, span)
                    })?;
                    self.server_options.insert(*handle, options);
                    Ok(Value::Null)
                } else {
                    Ok(failed(
                        name,
                        &WebFailure::ProviderUnavailable("ServerOptions"),
                    ))
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
                        return Ok(failed(name, &WebFailure::from_message(&message)));
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
                        return Ok(failed(name, &WebFailure::from_message(&message)));
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
                        return Ok(failed(name, &WebFailure::from_message(&message)));
                    }
                };
                self.tls_configs
                    .insert(*handle, std::sync::Arc::new(config));
                return Ok(Value::Object {
                    handle: *handle,
                    class: "BNWeb.TLSConfig".into(),
                });
            }
            Ok(failed(
                name,
                &WebFailure::ProviderUnavailable("BNWeb.TLSConfig"),
            ))
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
                        return Ok(failed(
                            name,
                            &WebFailure::IndexOutOfRange {
                                index,
                                count: values.len(),
                            },
                        ));
                    };
                    Ok(values.get(index).map_or_else(
                        || {
                            failed(
                                name,
                                &WebFailure::IndexOutOfRange {
                                    index: index as i128,
                                    count: values.len(),
                                },
                            )
                        },
                        |value| Value::String(shared_string(value.as_str())),
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
}

fn apply_static_outcome_to_response(
    outcome: crate::web::StaticServeOutcome,
    response: &mut crate::web::Response,
) {
    match outcome {
        crate::web::StaticServeOutcome::Ok {
            status,
            content_type,
            content_length,
            etag,
            last_modified,
            body,
        } => {
            let _ = response.set_status(status);
            let _ = response.set_header("Content-Type", content_type);
            let _ = response.set_header("Content-Length", &content_length.to_string());
            let _ = response.set_header("ETag", &etag);
            let _ = response.set_header("Last-Modified", &last_modified);
            let body_str = String::from_utf8(body)
                .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
            let _ = response.write(&body_str);
        }
        crate::web::StaticServeOutcome::NotModified {
            etag,
            last_modified,
        } => {
            let _ = response.set_status(304);
            let _ = response.set_header("ETag", &etag);
            let _ = response.set_header("Last-Modified", &last_modified);
        }
        crate::web::StaticServeOutcome::MethodNotAllowed => {
            let _ = response.set_status(405);
            let _ = response.set_header("Allow", "GET, HEAD");
        }
        crate::web::StaticServeOutcome::BadRequest(_) => {
            let _ = response.set_status(400);
        }
        crate::web::StaticServeOutcome::PayloadTooLarge => {
            let _ = response.set_status(413);
        }
        crate::web::StaticServeOutcome::NotFound => {
            let _ = response.set_status(404);
        }
        crate::web::StaticServeOutcome::Forbidden => {
            let _ = response.set_status(403);
        }
        crate::web::StaticServeOutcome::InternalError(_) => {
            let _ = response.set_status(500);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn module_constants_match_the_runtime_codes() {
        use bn_types::error_codes::web;
        let module = include_str!("../../../modules/bn/BNWeb.bn");
        let codes = [
            ("INVALID_ARGUMENT", web::INVALID_ARGUMENT),
            ("NOT_FOUND", web::NOT_FOUND),
            ("OUT_OF_RANGE", web::OUT_OF_RANGE),
            ("LIMIT", web::LIMIT),
            ("CLOSED", web::CLOSED),
            ("TIMEOUT", web::TIMEOUT),
            ("EGRESS_DENIED", web::EGRESS_DENIED),
            ("HTTP_FAILED", web::HTTP_FAILED),
            ("UNAVAILABLE", web::UNAVAILABLE),
            ("HTTP_STATUS", web::HTTP_STATUS),
            ("ALREADY_EXISTS", web::ALREADY_EXISTS),
            ("UNSUPPORTED_ENCODING", web::UNSUPPORTED_ENCODING),
        ];
        for (name, value) in codes {
            let line = format!("EXPORT CONST {name} AS INTEGER = {value}");
            assert!(module.contains(&line), "BNWeb.bn lacks `{line}`");
        }
    }
}
