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
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::TlsConnector;

use bn_core_net::TcpStream;

use crate::web::{RouteOutcome, ServerState};

pub type Handler = Arc<
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
    ) -> Result<Vec<bn_core_net::Address>, String>;
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
    ) -> Result<Vec<bn_core_net::Address>, String> {
        bn_core_net::resolve(host, port, maximum).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
/// Serves one already-accepted HOST.Net stream using Hyper's HTTP/1.1 and
/// HTTP/2 auto-detection. Handlers remain synchronous inside the bounded
/// `ServerState` dispatch callback.
pub(crate) fn serve_connection(
    stream: TcpStream,
    state: Arc<Mutex<ServerState>>,
) -> io::Result<()> {
    serve_connection_with_handler(stream, state, None)
}

#[cfg(test)]
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

pub fn serve_connection_with_runtime(
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

#[cfg(test)]
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

pub fn serve_tls_connection_with_runtime(
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
    // One instant bounds the connection and any handler it runs. A handler
    // deadline measured from the handler start would always fall after the
    // connection deadline, which then closed the connection with no response
    // instead of the `408` below. With the same instant, `Timeout` polls the
    // connection (and so the timed-out handler) before its own timer.
    let deadline = tokio::time::Instant::now() + connection_timeout;
    let header_timeout = std::time::Duration::from_millis(options.header_read_ms);
    let idle_timeout = std::time::Duration::from_millis(options.idle_keep_alive_ms);
    let serial_gate = state
        .lock()
        .map_err(|_| io::Error::other("server state unavailable"))?
        .serial_gate();
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
        let serial_gate = std::sync::Arc::clone(&serial_gate);
        let active_handler_tasks = std::sync::Arc::clone(&active_handler_tasks);
        async move {
            let request_id =
                crate::web_state::new_request_id(&crate::web_state::SystemEntropy).ok();
            let method = request.method().as_str().to_string();
            let path = request.uri().path().to_string();
            let mut response = route_response(
                request,
                peer,
                secure_transport,
                &state,
                handler,
                request_options,
                deadline,
                serial_gate,
                active_handler_tasks,
            )
            .await;
            if let Some(request_id) = &request_id {
                response.headers_mut().insert(
                    "x-request-id",
                    hyper::header::HeaderValue::from_str(request_id)
                        .expect("request IDs use only ASCII hex"),
                );
            }
            apply_default_security_headers(&mut response, secure_transport);
            let status = response.status().as_u16();
            if let Ok(server) = state.lock() {
                server.log_access(&method, &path, status, request_id.as_deref());
            }
            Ok::<_, Infallible>(response)
        }
    });
    tokio::time::timeout_at(deadline, builder.serve_connection(io, service))
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

#[cfg(test)]
/// Performs one bounded HTTP/1.1 request through the HOST.Net socket provider.
/// HTTPS uses Rustls with the host system CA bundle; no cleartext downgrade is
/// attempted.
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

pub fn client_request_with_policy(
    method: &str,
    url: &str,
    body: &str,
    policy: &crate::web::EgressPolicy,
) -> Result<crate::web::Response, String> {
    let resolver = SystemAddressResolver;
    client_request_with_resolver_and_policy(method, url, body, &resolver, policy)
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
    if body.len() > bn_limits::web_limits().max_body_bytes {
        return Err("request body exceeds 8 MiB".into());
    }
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| "URL requires an authority".to_string())?;
    if scheme != "http" && scheme != "https" {
        return Err("unsupported URL scheme".into());
    }
    let authority_end = rest.find(['/', '?']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let target = if authority_end == rest.len() {
        "/"
    } else {
        &rest[authority_end..]
    };
    let (host, port) =
        parse_authority_with_default(authority, if scheme == "https" { 443 } else { 80 })?;
    let addresses =
        resolve_validated_addresses_with_policy(host.as_str(), port, resolver, policy, scheme)?;
    if addresses.is_empty() {
        return Err("URL resolved to no addresses".to_string());
    }
    let mut connect_error = None;
    let mut stream = None;
    for address in &addresses {
        let endpoint = match bn_core_net::Address::parse(&address.to_string()) {
            Ok(addr) => bn_core_net::Endpoint::new(addr, port),
            Err(_) => continue,
        };
        match bn_core_net::TcpStream::connect(
            endpoint,
            std::time::Duration::from_millis(policy.total_deadline_ms()),
        ) {
            Ok(connected) => {
                stream = Some(connected);
                break;
            }
            Err(err) => {
                connect_error = Some(err.to_string());
            }
        }
    }
    let Some(stream) = stream else {
        return Err(
            connect_error.unwrap_or_else(|| "failed to connect to allowed destination".to_string())
        );
    };
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
        let stream =
            tokio::net::TcpStream::from_std(std_stream).map_err(|error| error.to_string())?;
        if scheme == "https" {
            let config = crate::tls::client_config()?;
            let connector = TlsConnector::from(Arc::new(config));
            let server_name = rustls::pki_types::ServerName::try_from(host.clone())
                .map_err(|_| "invalid TLS server name".to_string())?;
            let stream = connector
                .connect(server_name, stream)
                .await
                .map_err(|error| format!("TLS handshake failed: {error}"))?;
            perform_http_request(stream, method, authority, target, body, policy).await
        } else {
            perform_http_request(stream, method, authority, target, body, policy).await
        }
    })
}

async fn perform_http_request<I>(
    stream: I,
    method: &str,
    authority: &str,
    target: &str,
    body: &str,
    policy: &crate::web::EgressPolicy,
) -> Result<crate::web::Response, String>
where
    I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let io = TokioIo::new(stream);
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
    if bytes.len() > bn_limits::web_limits().max_response_body_bytes {
        return Err("response body exceeds 8 MiB".into());
    }
    let text =
        String::from_utf8(bytes.to_vec()).map_err(|_| "response body is not UTF-8".to_string())?;
    let mut result = crate::web::Response::new();
    result.status = status;
    result.headers = headers;
    result.body = text;
    result.commit().map_err(str::to_owned)?;
    Ok(result)
}

#[cfg(test)]
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
        .resolve(host, port, bn_limits::web_limits().resolved_addresses_max)?
        .into_iter()
        .map(bn_core_net::Address::as_std)
        .collect::<Vec<_>>();
    policy
        .filter_allowed_addresses(scheme, port, &addresses)
        .map_err(str::to_owned)
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

#[cfg(test)]
fn parse_authority(authority: &str) -> Result<(String, u16), String> {
    parse_authority_with_default(authority, 80)
}

fn parse_authority_with_default(
    authority: &str,
    default_port: u16,
) -> Result<(String, u16), String> {
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
            .map_or(Ok(default_port), |suffix| {
                suffix
                    .strip_prefix(':')
                    .ok_or("invalid IPv6 authority")?
                    .parse()
                    .map_err(|_| "invalid port")
            })?;
        return Ok((address.to_owned(), port));
    }
    let default_port = default_port.to_string();
    let (host, port) = authority
        .rsplit_once(':')
        .unwrap_or((authority, default_port.as_str()));
    if host.is_empty() {
        return Err("invalid URL authority".into());
    }
    Ok((host.to_owned(), port.parse().map_err(|_| "invalid port")?))
}

#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
async fn route_response(
    request: Request<hyper::body::Incoming>,
    peer: Option<std::net::IpAddr>,
    secure_transport: bool,
    state: &Arc<Mutex<ServerState>>,
    handler: Option<Handler>,
    options: crate::web::ServerOptions,
    deadline: tokio::time::Instant,
    serial_gate: std::sync::Arc<tokio::sync::Semaphore>,
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
    let Ok(mut request) =
        crate::web::Request::new(parts.method.as_str(), &target, headers, &body, peer)
    else {
        return response(StatusCode::BAD_REQUEST, "invalid request");
    };
    request.secure = secure_transport;
    let mut params = Vec::new();
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
            if let RouteOutcome::Matched(_, found) = &outcome {
                matched = true;
                params = crate::web::sorted_params(found);
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
    request.params = params;
    let (queue, sessions) = {
        let server = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (server.queue(), server.sessions())
    };
    if let Some(table) = &sessions {
        let cookie = request
            .cookie(crate::web::sessions::SESSION_COOKIE)
            .map(str::to_owned);
        request.session = Some(crate::web::sessions::RequestSession::begin(
            table,
            cookie.as_deref(),
        ));
    }
    if matched && let Some(handler) = handler {
        if let Some(queue) = queue {
            return run_on_queue(
                &queue,
                handler,
                request,
                state,
                deadline,
                &active_handler_tasks,
            )
            .await;
        }
        return run_serially(
            handler,
            request,
            state,
            deadline,
            &serial_gate,
            &active_handler_tasks,
        )
        .await;
    }
    if !matched {
        let static_mount_data = {
            let server = state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            server
                .match_static_mount(request.target.path.as_str())
                .map(|(mount, subpath)| (mount.clone(), subpath.to_owned(), server.static_opener()))
        };
        if let Some((mount, subpath, opener)) = static_mount_data {
            let if_none_match = request
                .header("If-None-Match")
                .ok()
                .and_then(|h| h.get(0).map(str::to_owned));
            let if_modified_since = request
                .header("If-Modified-Since")
                .ok()
                .and_then(|h| h.get(0).map(str::to_owned));
            let outcome = crate::web::serve_static_file(
                &mount,
                &target,
                &subpath,
                &request.method,
                if_none_match.as_deref(),
                if_modified_since.as_deref(),
                options.max_body_bytes,
                opener.as_ref(),
            );
            return static_outcome_to_response(outcome);
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

fn static_outcome_to_response(outcome: crate::web::StaticServeOutcome) -> Response<Full<Bytes>> {
    match outcome {
        crate::web::StaticServeOutcome::Ok {
            status,
            content_type,
            content_length,
            etag,
            last_modified,
            body,
        } => {
            let mut res = Response::builder()
                .status(StatusCode::from_u16(status).unwrap_or(StatusCode::OK))
                .body(Full::new(Bytes::from(body)))
                .unwrap_or_else(|_| Response::new(Full::new(Bytes::new())));
            let headers = res.headers_mut();
            if let Ok(v) = hyper::header::HeaderValue::from_str(content_type) {
                headers.insert(hyper::header::CONTENT_TYPE, v);
            }
            if let Ok(v) = hyper::header::HeaderValue::from_str(&content_length.to_string()) {
                headers.insert(hyper::header::CONTENT_LENGTH, v);
            }
            if let Ok(v) = hyper::header::HeaderValue::from_str(&etag) {
                headers.insert(hyper::header::ETAG, v);
            }
            if let Ok(v) = hyper::header::HeaderValue::from_str(&last_modified) {
                headers.insert(hyper::header::LAST_MODIFIED, v);
            }
            res
        }
        crate::web::StaticServeOutcome::NotModified {
            etag,
            last_modified,
        } => {
            let mut res = Response::builder()
                .status(StatusCode::NOT_MODIFIED)
                .body(Full::new(Bytes::new()))
                .unwrap_or_else(|_| Response::new(Full::new(Bytes::new())));
            let headers = res.headers_mut();
            if let Ok(v) = hyper::header::HeaderValue::from_str(&etag) {
                headers.insert(hyper::header::ETAG, v);
            }
            if let Ok(v) = hyper::header::HeaderValue::from_str(&last_modified) {
                headers.insert(hyper::header::LAST_MODIFIED, v);
            }
            res
        }
        crate::web::StaticServeOutcome::MethodNotAllowed => response_with_allow(
            StatusCode::METHOD_NOT_ALLOWED,
            "method not allowed",
            Some("GET, HEAD"),
        ),
        crate::web::StaticServeOutcome::BadRequest(msg) => response(StatusCode::BAD_REQUEST, msg),
        crate::web::StaticServeOutcome::PayloadTooLarge => response(
            StatusCode::PAYLOAD_TOO_LARGE,
            "static file exceeds configured body limit",
        ),
        crate::web::StaticServeOutcome::NotFound => response(StatusCode::NOT_FOUND, "not found"),
        crate::web::StaticServeOutcome::Forbidden => {
            response(StatusCode::FORBIDDEN, "forbidden")
        }
        crate::web::StaticServeOutcome::InternalError(_) => {
            response(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
        }
    }
}

/// The response a handler built, with the session cookie its session work
/// needs, committed for the request method.
fn finish_handler_response(
    request: &crate::web::Request,
    mut application_response: crate::web::Response,
) -> Response<Full<Bytes>> {
    if let Some(session) = &request.session
        && session.apply(&mut application_response).is_err()
    {
        return response(
            StatusCode::INTERNAL_SERVER_ERROR,
            bn_catalog::text("web", "response.session_cookie_failed"),
        );
    }
    if application_response
        .finish_for_method(&request.method)
        .is_err()
    {
        return response(StatusCode::INTERNAL_SERVER_ERROR, "response commit failed");
    }
    response_from_web(application_response)
}

/// Runs the handler of a server without a queue: one handler at a time
/// (`bnweb.md`, "Server lifecycle"), off the transport threads. A request
/// waits for its turn while `pendingWork` admits it; past the deadline it
/// answers `408`, and once `Stop` began, `503`.
async fn run_serially(
    handler: Handler,
    request: crate::web::Request,
    state: &Arc<Mutex<ServerState>>,
    deadline: tokio::time::Instant,
    serial_gate: &std::sync::Arc<tokio::sync::Semaphore>,
    active_handler_tasks: &std::sync::Arc<std::sync::atomic::AtomicUsize>,
) -> Response<Full<Bytes>> {
    let record = |timed_out: bool| {
        if let Ok(mut server) = state.lock() {
            server.record_request_failure(timed_out, false);
        }
    };
    let admitted = state
        .lock()
        .is_ok_and(|mut server| server.try_begin_handler().is_ok());
    if !admitted {
        record(false);
        return response(StatusCode::SERVICE_UNAVAILABLE, "handler queue is full");
    }
    let _handler_admission = HandlerAdmissionGuard(std::sync::Arc::clone(state));
    let turn =
        match tokio::time::timeout_at(deadline, std::sync::Arc::clone(serial_gate).acquire_owned())
            .await
        {
            Ok(Ok(turn)) => turn,
            Ok(Err(_)) => {
                record(false);
                return response(StatusCode::SERVICE_UNAVAILABLE, "server is stopping");
            }
            Err(_) => {
                record(true);
                return response(StatusCode::REQUEST_TIMEOUT, "handler timed out");
            }
        };
    active_handler_tasks.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    let task_guard = HandlerTaskGuard(std::sync::Arc::clone(active_handler_tasks));
    let task_request = request.clone();
    // The turn is held until the handler returns, even past the deadline.
    let handler_result = tokio::time::timeout_at(
        deadline,
        tokio::task::spawn_blocking(move || {
            let _task_guard = task_guard;
            let _turn = turn;
            let mut application_response = crate::web::Response::new();
            application_response.secure = task_request.secure;
            let result = handler(&task_request, &mut application_response);
            (result, application_response)
        }),
    )
    .await;
    match handler_result {
        Ok(Ok((Ok(()), application_response))) => {
            finish_handler_response(&request, application_response)
        }
        Ok(Ok((Err(_), _)) | Err(_)) => {
            record(false);
            response(StatusCode::INTERNAL_SERVER_ERROR, "handler failed")
        }
        Err(_) => {
            record(true);
            response(StatusCode::REQUEST_TIMEOUT, "handler timed out")
        }
    }
}
/// Runs the handler as one task of the program's `BNDispatch` queue
/// (`Server.UseQueue`): at most its workers at once, each in its own program
/// copy. A full or closed queue answers `503`; past the deadline, `408`.
async fn run_on_queue(
    queue: &bn_core_dispatch::Queue,
    handler: Handler,
    request: crate::web::Request,
    state: &Arc<Mutex<ServerState>>,
    deadline: tokio::time::Instant,
    active_handler_tasks: &std::sync::Arc<std::sync::atomic::AtomicUsize>,
) -> Response<Full<Bytes>> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let task_request = request.clone();
    // Counted until the task ends or is dropped unrun, so `Stop` waits for it.
    active_handler_tasks.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    let task_guard = HandlerTaskGuard(std::sync::Arc::clone(active_handler_tasks));
    let submitted = queue.submit_with("BNWeb handler".into(), move |_ticket| {
        let _task_guard = task_guard;
        let mut application_response = crate::web::Response::new();
        application_response.secure = task_request.secure;
        let result = handler(&task_request, &mut application_response);
        let _ = sender.send((result, application_response));
    });
    let record = |timed_out: bool| {
        if let Ok(mut server) = state.lock() {
            server.record_request_failure(timed_out, false);
        }
    };
    if submitted.is_err() {
        record(false);
        return response(StatusCode::SERVICE_UNAVAILABLE, "handler queue is full");
    }
    match tokio::time::timeout_at(deadline, receiver).await {
        Ok(Ok((Ok(()), application_response))) => {
            finish_handler_response(&request, application_response)
        }
        Ok(Ok((Err(_), _))) => {
            record(false);
            response(StatusCode::INTERNAL_SERVER_ERROR, "handler failed")
        }
        // The task was cancelled with the queue: it never ran.
        Ok(Err(_)) => {
            record(false);
            response(
                StatusCode::SERVICE_UNAVAILABLE,
                bn_catalog::text("web", "queue.closed"),
            )
        }
        Err(_) => {
            record(true);
            response(StatusCode::REQUEST_TIMEOUT, "handler timed out")
        }
    }
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

#[cfg(test)]
mod deadline_tests;
#[cfg(test)]
mod tests;
