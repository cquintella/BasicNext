#![allow(dead_code)] // ponytail: callback/response projection is staged after transport wiring.

use std::{
    convert::Infallible,
    io,
    net::ToSocketAddrs,
    sync::{Arc, Mutex},
};

use http_body_util::{BodyExt, Full};
use hyper::{Request, Response, StatusCode, body::Bytes, service::service_fn};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto,
};
use tokio_rustls::TlsAcceptor;

use crate::{
    net::TcpStream,
    web::{RouteOutcome, ServerState},
};

const MAX_HTTP_BODY: usize = 8 * 1024 * 1024;
const MAX_HTTP_RESPONSE_BODY: usize = 8 * 1024 * 1024;
const REQUEST_BODY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const CONNECT_HANDSHAKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

pub(crate) type Handler = Arc<
    dyn Fn(&crate::web::Request, &mut crate::web::Response) -> Result<(), &'static str>
        + Send
        + Sync,
>;

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
    let peer = stream
        .remote_endpoint()
        .ok()
        .map(|endpoint| endpoint.address().as_std());
    let std_stream = stream.into_std();
    std_stream.set_nonblocking(true)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(io::Error::other)?;
    runtime.block_on(async move {
        let io = TokioIo::new(tokio::net::TcpStream::from_std(std_stream)?);
        serve_hyper_connection(io, peer, state, handler).await
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
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(io::Error::other)?;
    runtime.block_on(async move {
        let stream = tokio::net::TcpStream::from_std(std_stream)?;
        let stream = TlsAcceptor::from(config)
            .accept(stream)
            .await
            .map_err(io::Error::other)?;
        serve_hyper_connection(TokioIo::new(stream), peer, state, handler).await
    })
}

async fn serve_hyper_connection<I>(
    io: I,
    peer: Option<std::net::IpAddr>,
    state: Arc<Mutex<ServerState>>,
    handler: Option<Handler>,
) -> io::Result<()>
where
    I: hyper::rt::Read + hyper::rt::Write + Unpin + Send + 'static,
{
    let builder = auto::Builder::new(TokioExecutor::new());
    let service = service_fn(move |request: Request<hyper::body::Incoming>| {
        let state = Arc::clone(&state);
        let handler = handler.clone();
        async move { Ok::<_, Infallible>(route_response(request, peer, &state, handler).await) }
    });
    builder
        .serve_connection(io, service)
        .await
        .map_err(io::Error::other)
}

/// Performs one bounded cleartext HTTP/1.1 request through the HOST.Net
/// socket provider. HTTPS is rejected by the caller until the TLS adapter is
/// installed; no cleartext downgrade is attempted.
pub(crate) fn client_request(
    method: &str,
    url: &str,
    body: &str,
) -> Result<crate::web::Response, String> {
    let mut current = url.to_owned();
    for _ in 0..=10 {
        let response = client_request_once(method, &current, body)?;
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
) -> Result<crate::web::Response, String> {
    if body.len() > MAX_HTTP_BODY {
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
    let addresses = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|error| error.to_string())?
        .map(|address| address.ip())
        .collect::<Vec<_>>();
    crate::web::validate_ssrf_destinations(&addresses, false).map_err(str::to_owned)?;
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
        std::time::Duration::from_secs(5),
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
            CONNECT_HANDSHAKE_TIMEOUT,
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
            std::time::Duration::from_secs(10),
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
        let bytes = tokio::time::timeout(REQUEST_BODY_TIMEOUT, response.into_body().collect())
            .await
            .map_err(|_| "HTTP response body timed out".to_string())?
            .map_err(|error| error.to_string())?
            .to_bytes();
        if bytes.len() > MAX_HTTP_RESPONSE_BODY {
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

async fn route_response(
    request: Request<hyper::body::Incoming>,
    peer: Option<std::net::IpAddr>,
    state: &Arc<Mutex<ServerState>>,
    handler: Option<Handler>,
) -> Response<Full<Bytes>> {
    let (parts, body) = request.into_parts();
    let body = match tokio::time::timeout(REQUEST_BODY_TIMEOUT, body.collect()).await {
        Ok(Ok(body)) => body.to_bytes(),
        Ok(Err(_)) => return response(StatusCode::BAD_REQUEST, "invalid request body"),
        Err(_) => return response(StatusCode::REQUEST_TIMEOUT, "request body timed out"),
    };
    if body.len() > MAX_HTTP_BODY {
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
    let (status, allow, application_response) = {
        let Ok(mut state) = state.lock() else {
            return response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server state unavailable",
            );
        };
        let mut status = StatusCode::NOT_FOUND;
        let mut allow = None;
        let mut application_response = None;
        let path = request.target.path.as_str();
        let method = request.method.as_str();
        if state
            .dispatch(method, path, |outcome| {
                (status, allow) = route_status(method, &outcome);
                if let RouteOutcome::Matched(_, _) = outcome
                    && let Some(handler) = handler.as_ref()
                {
                    let mut response = crate::web::Response::new();
                    if handler(&request, &mut response).is_ok() {
                        application_response = Some(response);
                    } else {
                        status = StatusCode::INTERNAL_SERVER_ERROR;
                    }
                }
            })
            .is_err()
        {
            status = StatusCode::SERVICE_UNAVAILABLE;
        }
        (status, allow, application_response)
    };
    if let Some(mut application_response) = application_response {
        if application_response
            .finish_for_method(&request.method)
            .is_err()
        {
            return response(StatusCode::INTERNAL_SERVER_ERROR, "response commit failed");
        }
        return response_from_web(application_response);
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

#[allow(dead_code)]
const _: usize = MAX_HTTP_BODY;

#[cfg(test)]
mod tests;
