#![allow(dead_code)] // ponytail: callback/response projection is staged after transport wiring.

use std::{
    convert::Infallible,
    io,
    sync::{Arc, Mutex},
};

use http_body_util::{BodyExt, Full};
use hyper::{Request, Response, StatusCode, body::Bytes, service::service_fn};
use hyper_util::{
    rt::{TokioExecutor, TokioIo, TokioTimer},
    server::conn::auto,
};
use tokio_rustls::TlsAcceptor;

use crate::{
    net::TcpStream,
    web::{RouteOutcome, ServerState},
};

pub(crate) type Handler = Arc<
    dyn Fn(&crate::web::Request, &mut crate::web::Response) -> Result<(), &'static str>
        + Send
        + Sync,
>;

trait AddressResolver {
    fn resolve(
        &self,
        host: &str,
        port: u16,
        maximum: usize,
    ) -> Result<Vec<crate::net::Address>, String>;
}

struct SystemAddressResolver;

struct HandlerTaskGuard(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl Drop for HandlerTaskGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

struct HandlerAdmissionGuard(std::sync::Arc<std::sync::Mutex<ServerState>>);

impl Drop for HandlerAdmissionGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.lock() {
            state.finish_handler();
        }
    }
}

impl AddressResolver for SystemAddressResolver {
    fn resolve(
        &self,
        host: &str,
        port: u16,
        maximum: usize,
    ) -> Result<Vec<crate::net::Address>, String> {
        crate::net::resolve(host, port, maximum).map_err(|error| error.to_string())
    }
}

/// Serves one already-accepted HOST.Net stream using Hyper's HTTP/1.1 and
/// HTTP/2 auto-detection. Handlers remain synchronous inside the bounded
/// `ServerState` dispatch callback.
pub(crate) fn serve_connection(
    stream: TcpStream,
    state: Arc<Mutex<ServerState>>,
) -> io::Result<()> {
    serve_connection_with_handler(stream, state, None)
}

pub(crate) fn serve_connection_with_handler(
    stream: TcpStream,
    state: Arc<Mutex<ServerState>>,
    handler: Option<Handler>,
) -> io::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(io::Error::other)?;
    serve_connection_with_runtime(stream, state, handler, &runtime)
}

pub(crate) fn serve_connection_with_runtime(
    stream: TcpStream,
    state: Arc<Mutex<ServerState>>,
    handler: Option<Handler>,
    runtime: &tokio::runtime::Runtime,
) -> io::Result<()> {
    let options = state
        .lock()
        .map_err(|_| io::Error::other("server state unavailable"))?
        .options();
    let peer = stream
        .remote_endpoint()
        .ok()
        .map(|endpoint| endpoint.address().as_std());
    let std_stream = stream.into_std();
    std_stream.set_nonblocking(true)?;
    runtime.block_on(async move {
        let io = TokioIo::new(tokio::net::TcpStream::from_std(std_stream)?);
        serve_hyper_connection(io, peer, state, handler, false, options).await
    })
}

/// Serves one accepted stream after a Rustls handshake. The caller supplies a
/// preconfigured certificate policy; this function never falls back to
/// cleartext when the handshake fails.
pub(crate) fn serve_tls_connection(
    stream: TcpStream,
    state: Arc<Mutex<ServerState>>,
    config: Arc<rustls::ServerConfig>,
) -> io::Result<()> {
    serve_tls_connection_with_handler(stream, state, config, None)
}

pub(crate) fn serve_tls_connection_with_handler(
    stream: TcpStream,
    state: Arc<Mutex<ServerState>>,
    config: Arc<rustls::ServerConfig>,
    handler: Option<Handler>,
) -> io::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(io::Error::other)?;
    serve_tls_connection_with_runtime(stream, state, config, handler, &runtime)
}

pub(crate) fn serve_tls_connection_with_runtime(
    stream: TcpStream,
    state: Arc<Mutex<ServerState>>,
    config: Arc<rustls::ServerConfig>,
    handler: Option<Handler>,
    runtime: &tokio::runtime::Runtime,
) -> io::Result<()> {
    let options = state
        .lock()
        .map_err(|_| io::Error::other("server state unavailable"))?
        .options();
    if !crate::tls::supports_http_alpn(&config.alpn_protocols) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "TLS configuration must advertise h2 or http/1.1",
        ));
    }
    let peer = stream
        .remote_endpoint()
        .ok()
        .map(|endpoint| endpoint.address().as_std());
    let std_stream = stream.into_std();
    std_stream.set_nonblocking(true)?;
    runtime.block_on(async move {
        let stream = tokio::net::TcpStream::from_std(std_stream)?;
        let stream = tokio::time::timeout(
            std::time::Duration::from_millis(options.tls_handshake_ms),
            TlsAcceptor::from(config).accept(stream),
        )
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "TLS handshake timed out"))?
        .map_err(io::Error::other)?;
        serve_hyper_connection(TokioIo::new(stream), peer, state, handler, true, options).await
    })
}

async fn serve_hyper_connection<I>(
    io: I,
    peer: Option<std::net::IpAddr>,
    state: Arc<Mutex<ServerState>>,
    handler: Option<Handler>,
    secure_transport: bool,
    options: crate::web::ServerOptions,
) -> io::Result<()>
where
    I: hyper::rt::Read + hyper::rt::Write + Unpin + Send + 'static,
{
    let mut builder = auto::Builder::new(TokioExecutor::new());
    let connection_timeout = std::time::Duration::from_millis(options.connection_total_ms);
    let header_timeout = std::time::Duration::from_millis(options.header_read_ms);
    let idle_timeout = std::time::Duration::from_millis(options.idle_keep_alive_ms);
    let handler_slots = state
        .lock()
        .map_err(|_| io::Error::other("server state unavailable"))?
        .handler_slots()
        .unwrap_or_else(|| std::sync::Arc::new(tokio::sync::Semaphore::new(options.worker_count)));
    let active_handler_tasks = state
        .lock()
        .map_err(|_| io::Error::other("server state unavailable"))?
        .active_handler_tasks();
    builder
        .http1()
        .timer(TokioTimer::new())
        .header_read_timeout(header_timeout);
    builder
        .http2()
        .timer(TokioTimer::new())
        .keep_alive_interval(idle_timeout)
        .keep_alive_timeout(idle_timeout);
    let service = service_fn(move |request: Request<hyper::body::Incoming>| {
        let state = Arc::clone(&state);
        let handler = handler.clone();
        let request_options = options.clone();
        let handler_slots = std::sync::Arc::clone(&handler_slots);
        let active_handler_tasks = std::sync::Arc::clone(&active_handler_tasks);
        async move {
            let request_id =
                crate::web_state::new_request_id(&crate::web_state::SystemEntropy).ok();
            let mut response = route_response(
                request,
                peer,
                &state,
                handler,
                request_options,
                handler_slots,
                active_handler_tasks,
            )
            .await;
            if let Some(request_id) = request_id {
                response.headers_mut().insert(
                    "x-request-id",
                    hyper::header::HeaderValue::from_str(&request_id)
                        .expect("request IDs use only ASCII hex"),
                );
            }
            apply_default_security_headers(&mut response, secure_transport);
            Ok::<_, Infallible>(response)
        }
    });
    tokio::time::timeout(connection_timeout, builder.serve_connection(io, service))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "connection deadline exceeded"))?
        .map_err(io::Error::other)
}

fn apply_default_security_headers(response: &mut Response<Full<Bytes>>, secure_transport: bool) {
    response.headers_mut().insert(
        "x-content-type-options",
        hyper::header::HeaderValue::from_static("nosniff"),
    );
    if secure_transport {
        response.headers_mut().insert(
            "strict-transport-security",
            hyper::header::HeaderValue::from_static("max-age=31536000"),
        );
    }
}

/// Performs one bounded cleartext HTTP/1.1 request through the HOST.Net
/// socket provider. HTTPS is rejected by the caller until the TLS adapter is
/// installed; no cleartext downgrade is attempted.
pub(crate) fn client_request(
    method: &str,
    url: &str,
    body: &str,
) -> Result<crate::web::Response, String> {
    let resolver = SystemAddressResolver;
    client_request_with_resolver_and_policy(
        method,
        url,
        body,
        &resolver,
        &crate::web::EgressPolicy::default(),
    )
}

pub(crate) fn client_request_with_policy(
    method: &str,
    url: &str,
    body: &str,
    policy: &crate::web::EgressPolicy,
) -> Result<crate::web::Response, String> {
    let resolver = SystemAddressResolver;
    client_request_with_resolver_and_policy(method, url, body, &resolver, policy)
}

fn client_request_with_resolver(
    method: &str,
    url: &str,
    body: &str,
    resolver: &dyn AddressResolver,
) -> Result<crate::web::Response, String> {
    client_request_with_resolver_and_policy(
        method,
        url,
        body,
        resolver,
        &crate::web::EgressPolicy::default(),
    )
}

fn client_request_with_resolver_and_policy(
    method: &str,
    url: &str,
    body: &str,
    resolver: &dyn AddressResolver,
    policy: &crate::web::EgressPolicy,
) -> Result<crate::web::Response, String> {
    let mut current = url.to_owned();
    for _ in 0..=policy.max_redirects() {
        let response = client_request_once(method, &current, body, resolver, policy)?;
        if !(300..=399).contains(&response.status) {
            return Ok(response);
        }
        let location = response
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("location"))
            .map(|(_, value)| value.clone())
            .ok_or_else(|| "redirect response missing Location".to_string())?;
        current = resolve_redirect(&current, &location)?;
    }
    Err("redirect limit exceeded".into())
}

#[allow(clippy::too_many_lines)] // Keep the bounded request policy in one auditable path.
fn client_request_once(
    method: &str,
    url: &str,
    body: &str,
    resolver: &dyn AddressResolver,
    policy: &crate::web::EgressPolicy,
) -> Result<crate::web::Response, String> {
    if body.len() > crate::config::web_limits().max_body_bytes {
        return Err("request body exceeds 8 MiB".into());
    }
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| "URL requires an authority".to_string())?;
    if scheme != "http" {
        return Err("HTTPS client transport is not available".into());
    }
    let authority_end = rest.find(['/', '?']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let target = if authority_end == rest.len() {
        "/"
    } else {
        &rest[authority_end..]
    };
    let (host, port) = parse_authority(authority)?;
    let addresses =
        resolve_validated_addresses_with_policy(host.as_str(), port, resolver, policy, scheme)?;
    let address = addresses
        .first()
        .copied()
        .ok_or_else(|| "URL resolved to no addresses".to_string())?;
    let stream = crate::net::TcpStream::connect(
        crate::net::Endpoint::new(
            crate::net::Address::parse(&address.to_string())
                .map_err(|_| "invalid resolved address")?,
            port,
        ),
        std::time::Duration::from_millis(policy.total_deadline_ms()),
    )
    .map_err(|error| error.to_string())?;
    let std_stream = stream.into_std();
    std_stream
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(|error| error.to_string())?;
    runtime.block_on(async move {
        let io = TokioIo::new(
            tokio::net::TcpStream::from_std(std_stream).map_err(|error| error.to_string())?,
        );
        let (mut sender, connection) = tokio::time::timeout(
            std::time::Duration::from_millis(policy.total_deadline_ms()),
            hyper::client::conn::http1::handshake(io),
        )
        .await
        .map_err(|_| "HTTP handshake timed out".to_string())?
        .map_err(|error| error.to_string())?;
        tokio::spawn(async move {
            let _ = connection.await;
        });
        let request = Request::builder()
            .method(method)
            .uri(target)
            .header("host", authority)
            .header("content-length", body.len())
            .body(Full::new(Bytes::copy_from_slice(body.as_bytes())))
            .map_err(|error| error.to_string())?;
        let response = tokio::time::timeout(
            std::time::Duration::from_millis(policy.total_deadline_ms()),
            sender.send_request(request),
        )
        .await
        .map_err(|_| "HTTP request timed out".to_string())?
        .map_err(|error| error.to_string())?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|value| (name.as_str().to_owned(), value.to_owned()))
            })
            .collect::<Vec<_>>();
        if has_unsupported_encoding(&headers) {
            return Err("compressed responses are unavailable without a bounded decoder".into());
        }
        let bytes = tokio::time::timeout(
            std::time::Duration::from_millis(policy.total_deadline_ms()),
            response.into_body().collect(),
        )
        .await
        .map_err(|_| "HTTP response body timed out".to_string())?
        .map_err(|error| error.to_string())?
        .to_bytes();
        if bytes.len() > crate::config::web_limits().max_response_body_bytes {
            return Err("response body exceeds 8 MiB".into());
        }
        let text = String::from_utf8(bytes.to_vec())
            .map_err(|_| "response body is not UTF-8".to_string())?;
        let mut result = crate::web::Response::new();
        result.status = status;
        result.headers = headers;
        result.body = text;
        result.commit().map_err(str::to_owned)?;
        Ok(result)
    })
}

fn resolve_validated_addresses(
    host: &str,
    port: u16,
    resolver: &dyn AddressResolver,
) -> Result<Vec<std::net::IpAddr>, String> {
    resolve_validated_addresses_with_policy(
        host,
        port,
        resolver,
        &crate::web::EgressPolicy::default(),
        "http",
    )
}

fn resolve_validated_addresses_with_policy(
    host: &str,
    port: u16,
    resolver: &dyn AddressResolver,
    policy: &crate::web::EgressPolicy,
    scheme: &str,
) -> Result<Vec<std::net::IpAddr>, String> {
    let addresses = resolver
        .resolve(
            host,
            port,
            crate::config::web_limits().resolved_addresses_max,
        )?
        .into_iter()
        .map(crate::net::Address::as_std)
        .collect::<Vec<_>>();
    policy
        .validate(scheme, port, &addresses)
        .map_err(str::to_owned)?;
    Ok(addresses)
}

fn has_unsupported_encoding(headers: &[(String, String)]) -> bool {
    headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("content-encoding"))
}

fn resolve_redirect(base: &str, location: &str) -> Result<String, String> {
    if location.contains('#') || location.contains('\n') || location.contains('\r') {
        return Err("invalid redirect Location".into());
    }
    if location.starts_with("http://") || location.starts_with("https://") {
        crate::web::validate_client_url(location).map_err(str::to_owned)?;
        return Ok(location.to_owned());
    }
    let (scheme, rest) = base.split_once("://").ok_or("invalid redirect base")?;
    let authority_end = rest.find(['/', '?']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let path = if location.starts_with('/') {
        location.to_owned()
    } else if location.starts_with('?') {
        let base_path = rest[authority_end..].split('?').next().unwrap_or("/");
        format!("{base_path}{location}")
    } else {
        let base_path = rest[authority_end..].split('?').next().unwrap_or("/");
        let directory = base_path
            .rsplit_once('/')
            .map_or("/", |(directory, _)| directory);
        format!("{directory}/{location}")
    };
    let resolved = format!("{scheme}://{authority}{path}");
    crate::web::validate_client_url(&resolved).map_err(str::to_owned)?;
    Ok(resolved)
}

fn parse_authority(authority: &str) -> Result<(String, u16), String> {
    if authority.is_empty() || authority.contains('@') {
        return Err("invalid URL authority".into());
    }
    if let Some(host) = authority.strip_prefix('[') {
        let close = host
            .find(']')
            .ok_or_else(|| "invalid IPv6 authority".to_string())?;
        let address = &host[..close];
        let port = host
            .get(close + 1..)
            .filter(|suffix| !suffix.is_empty())
            .map_or(Ok(80), |suffix| {
                suffix
                    .strip_prefix(':')
                    .ok_or("invalid IPv6 authority")?
                    .parse()
                    .map_err(|_| "invalid port")
            })?;
        return Ok((address.to_owned(), port));
    }
    let (host, port) = authority.rsplit_once(':').unwrap_or((authority, "80"));
    if host.is_empty() {
        return Err("invalid URL authority".into());
    }
    Ok((host.to_owned(), port.parse().map_err(|_| "invalid port")?))
}

#[allow(clippy::too_many_lines)]
async fn route_response(
    request: Request<hyper::body::Incoming>,
    peer: Option<std::net::IpAddr>,
    state: &Arc<Mutex<ServerState>>,
    handler: Option<Handler>,
    options: crate::web::ServerOptions,
    handler_slots: std::sync::Arc<tokio::sync::Semaphore>,
    active_handler_tasks: std::sync::Arc<std::sync::atomic::AtomicUsize>,
) -> Response<Full<Bytes>> {
    let (parts, body) = request.into_parts();
    let header_bytes = parts
        .headers
        .iter()
        .map(|(name, value)| name.as_str().len().saturating_add(value.as_bytes().len()))
        .sum::<usize>();
    if parts.headers.len() > options.max_header_fields
        || header_bytes > options.max_header_bytes
        || parts.uri.to_string().len() > options.max_target_bytes
    {
        return response(
            StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
            "request headers or target exceed configured bounds",
        );
    }
    let body = match tokio::time::timeout(
        std::time::Duration::from_millis(options.body_read_ms),
        body.collect(),
    )
    .await
    {
        Ok(Ok(body)) => body.to_bytes(),
        Ok(Err(_)) => return response(StatusCode::BAD_REQUEST, "invalid request body"),
        Err(_) => return response(StatusCode::REQUEST_TIMEOUT, "request body timed out"),
    };
    if body.len() > options.max_body_bytes {
        return response(StatusCode::PAYLOAD_TOO_LARGE, "request body exceeds 8 MiB");
    }
    let Ok(body) = String::from_utf8(body.to_vec()) else {
        return response(StatusCode::BAD_REQUEST, "request body is not UTF-8");
    };
    let target = parts
        .uri
        .path_and_query()
        .map_or_else(|| parts.uri.path().to_owned(), ToString::to_string);
    let Ok(headers) = parts
        .headers
        .iter()
        .map(|(name, value)| {
            value
                .to_str()
                .map(|value| (name.as_str().to_owned(), value.to_owned()))
                .map_err(|_| ())
        })
        .collect::<Result<Vec<_>, _>>()
    else {
        return response(StatusCode::BAD_REQUEST, "invalid request header");
    };
    let Some(peer) = peer else {
        return response(StatusCode::BAD_REQUEST, "peer address unavailable");
    };
    let Ok(request) =
        crate::web::Request::new(parts.method.as_str(), &target, headers, &body, peer)
    else {
        return response(StatusCode::BAD_REQUEST, "invalid request");
    };
    let (status, allow, matched, rate_limited) = {
        let Ok(mut state) = state.lock() else {
            return response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server state unavailable",
            );
        };
        let mut status = StatusCode::NOT_FOUND;
        let mut allow = None;
        let mut matched = false;
        let mut rate_limited = false;
        let path = request.target.path.as_str();
        let method = request.method.as_str();
        let key = format!(
            "{path}|{}",
            request.effective_client_address(options.trusted_proxy)
        );
        let dispatch_result = state.dispatch_with_key(method, path, &key, |outcome| {
            (status, allow) = route_status(method, &outcome);
            if matches!(outcome, RouteOutcome::Matched(_, _)) {
                matched = true;
            }
        });
        if let Err(message) = dispatch_result {
            if message == "rate limit exceeded" {
                rate_limited = true;
                status = StatusCode::TOO_MANY_REQUESTS;
            } else {
                status = StatusCode::SERVICE_UNAVAILABLE;
            }
        }
        if rate_limited {
            state.record_request_failure(false, true);
        } else if status == StatusCode::SERVICE_UNAVAILABLE {
            state.record_request_failure(false, false);
        }
        (status, allow, matched, rate_limited)
    };
    if matched && let Some(handler) = handler {
        if !options.concurrent_handlers {
            let admitted = state
                .lock()
                .is_ok_and(|mut server| server.try_begin_handler().is_ok());
            if !admitted {
                if let Ok(mut server) = state.lock() {
                    server.record_request_failure(false, false);
                }
                return response(StatusCode::SERVICE_UNAVAILABLE, "handler queue is full");
            }
            let _handler_admission = HandlerAdmissionGuard(std::sync::Arc::clone(state));
            let mut application_response = crate::web::Response::new();
            if handler(&request, &mut application_response).is_err() {
                return response(StatusCode::INTERNAL_SERVER_ERROR, "handler failed");
            }
            if application_response
                .finish_for_method(&request.method)
                .is_err()
            {
                return response(StatusCode::INTERNAL_SERVER_ERROR, "response commit failed");
            }
            return response_from_web(application_response);
        }
        let handler_request = request.clone();
        let Ok(permit) = handler_slots.try_acquire_owned() else {
            if let Ok(mut state) = state.lock() {
                state.record_request_failure(false, false);
            }
            return response(StatusCode::SERVICE_UNAVAILABLE, "handler queue is full");
        };
        active_handler_tasks.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let task_counter = std::sync::Arc::clone(&active_handler_tasks);
        let handler_result = tokio::time::timeout(
            std::time::Duration::from_millis(options.connection_total_ms),
            tokio::task::spawn_blocking(move || {
                let _task_guard = HandlerTaskGuard(task_counter);
                let mut application_response = crate::web::Response::new();
                let result = handler(&handler_request, &mut application_response);
                (result, application_response, permit)
            }),
        )
        .await;
        match handler_result {
            Ok(Ok((Ok(()), mut application_response, _permit))) => {
                if application_response
                    .finish_for_method(&request.method)
                    .is_err()
                {
                    return response(StatusCode::INTERNAL_SERVER_ERROR, "response commit failed");
                }
                return response_from_web(application_response);
            }
            Ok(Ok((Err(_), _, _)) | Err(_)) => {
                if let Ok(mut state) = state.lock() {
                    state.record_request_failure(false, false);
                }
                return response(StatusCode::INTERNAL_SERVER_ERROR, "handler failed");
            }
            Err(_) => {
                if let Ok(mut state) = state.lock() {
                    state.record_request_failure(true, false);
                }
                return response(StatusCode::REQUEST_TIMEOUT, "handler timed out");
            }
        }
    }
    if rate_limited {
        return response_with_retry_after(StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded", 1);
    }
    let body = match status {
        StatusCode::NOT_FOUND => "not found",
        StatusCode::METHOD_NOT_ALLOWED => "method not allowed",
        _ => "",
    };
    response_with_allow(status, body, allow.as_deref())
}

fn response_from_web(web_response: crate::web::Response) -> Response<Full<Bytes>> {
    let mut builder = Response::builder().status(web_response.status);
    for (name, value) in web_response.headers {
        builder = builder.header(name, value);
    }
    builder
        .body(Full::new(Bytes::from(web_response.body)))
        .unwrap_or_else(|_| response(StatusCode::INTERNAL_SERVER_ERROR, "invalid response"))
}

fn route_status(method: &str, outcome: &RouteOutcome<'_>) -> (StatusCode, Option<String>) {
    match outcome {
        RouteOutcome::Matched(_, _) => (StatusCode::OK, None),
        RouteOutcome::MethodNotAllowed(methods) if method == "OPTIONS" => {
            (StatusCode::NO_CONTENT, Some(methods.join(", ")))
        }
        RouteOutcome::MethodNotAllowed(methods) => {
            (StatusCode::METHOD_NOT_ALLOWED, Some(methods.join(", ")))
        }
        RouteOutcome::NotFound => (StatusCode::NOT_FOUND, None),
    }
}

fn response(status: StatusCode, body: &str) -> Response<Full<Bytes>> {
    response_with_allow(status, body, None)
}

fn response_with_allow(
    status: StatusCode,
    body: &str,
    allow: Option<&str>,
) -> Response<Full<Bytes>> {
    let mut builder = Response::builder()
        .status(status)
        .header("content-length", body.len());
    if let Some(allow) = allow {
        builder = builder.header("allow", allow);
    }
    builder
        .body(Full::new(Bytes::copy_from_slice(body.as_bytes())))
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::new())))
}

fn response_with_retry_after(
    status: StatusCode,
    body: &str,
    retry_after: u64,
) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header("content-length", body.len())
        .header("retry-after", retry_after)
        .body(Full::new(Bytes::copy_from_slice(body.as_bytes())))
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::new())))
}

#[allow(dead_code)]
#[cfg(test)]
mod tests;
