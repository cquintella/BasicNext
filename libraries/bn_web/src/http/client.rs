// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The `BNWeb` HTTP client: resolution under the egress policy, one bounded
//! exchange per attempt, redirects, and the failure of each step as a
//! `WebFailure`.

use std::sync::Arc;

use http_body_util::{BodyExt, Full};
use hyper::{Request, body::Bytes};
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_rustls::TlsConnector;

use crate::web_error::WebFailure;

pub(super) trait AddressResolver {
    fn resolve(
        &self,
        host: &str,
        port: u16,
        maximum: usize,
        timeout: std::time::Duration,
    ) -> Result<Vec<bn_core_net::Address>, WebFailure>;
}

pub(super) struct SystemAddressResolver;

impl AddressResolver for SystemAddressResolver {
    fn resolve(
        &self,
        host: &str,
        port: u16,
        maximum: usize,
        timeout: std::time::Duration,
    ) -> Result<Vec<bn_core_net::Address>, WebFailure> {
        match bn_core_net::resolve_timeout(host, port, maximum, timeout) {
            Ok(Some(addresses)) => Ok(addresses),
            Ok(None) => Err(WebFailure::Timeout(
                bn_catalog::text("web", "dns.timeout.message").into(),
            )),
            Err(err) if err.kind() == std::io::ErrorKind::QuotaExceeded => Err(
                WebFailure::LimitExceeded(bn_catalog::text("web", "dns.pool_limit.message").into()),
            ),
            Err(err) => Err(WebFailure::HttpFailed(err.to_string())),
        }
    }
}

#[cfg(test)]
/// Performs one bounded HTTP/1.1 request through the HOST.Net socket provider.
/// HTTPS uses Rustls with the host system CA bundle; no cleartext downgrade is
/// attempted.
pub(super) fn client_request(
    method: &str,
    url: &str,
    body: &str,
) -> Result<crate::web::Response, WebFailure> {
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
) -> Result<crate::web::Response, WebFailure> {
    let resolver = SystemAddressResolver;
    client_request_with_resolver_and_policy(method, url, body, &resolver, policy)
}

pub fn client_transfer_with_policy(
    method: &str,
    url: &str,
    body: &[u8],
    extra_headers: &[(String, String)],
    policy: &crate::web::EgressPolicy,
) -> Result<crate::web::ClientReply, WebFailure> {
    let resolver = SystemAddressResolver;
    client_transfer_with_resolver_and_policy(method, url, body, extra_headers, &resolver, policy)
}

pub(super) fn client_transfer_with_resolver_and_policy(
    method: &str,
    url: &str,
    body: &[u8],
    extra_headers: &[(String, String)],
    resolver: &dyn AddressResolver,
    policy: &crate::web::EgressPolicy,
) -> Result<crate::web::ClientReply, WebFailure> {
    let mut current = url.to_owned();
    for _ in 0..=policy.max_redirects() {
        let reply = client_transfer_once(method, &current, body, extra_headers, resolver, policy)?;
        if !(300..=399).contains(&reply.status) {
            return Ok(reply);
        }
        let location = reply
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("location"))
            .map(|(_, value)| value.clone())
            .ok_or(WebFailure::MissingRedirectLocation)?;
        current = resolve_redirect(&current, &location)?;
    }
    Err(WebFailure::RedirectLimitExceeded)
}

pub(super) fn client_request_with_resolver_and_policy(
    method: &str,
    url: &str,
    body: &str,
    resolver: &dyn AddressResolver,
    policy: &crate::web::EgressPolicy,
) -> Result<crate::web::Response, WebFailure> {
    let reply = client_transfer_with_resolver_and_policy(
        method,
        url,
        body.as_bytes(),
        &[],
        resolver,
        policy,
    )?;
    let text = String::from_utf8(reply.body).map_err(|_| {
        WebFailure::InvalidArgument(bn_catalog::text("web", "client.body_not_utf8").into())
    })?;
    let mut result = crate::web::Response::new();
    result.status = reply.status;
    result.headers = reply.headers;
    result.body = text;
    result.commit()?;
    Ok(result)
}

#[allow(clippy::too_many_lines)] // Keep the bounded request policy in one auditable path.
pub(super) fn client_transfer_once(
    method: &str,
    url: &str,
    body: &[u8],
    extra_headers: &[(String, String)],
    resolver: &dyn AddressResolver,
    policy: &crate::web::EgressPolicy,
) -> Result<crate::web::ClientReply, WebFailure> {
    let body_limit = bn_limits::web_limits().max_body_bytes;
    if body.len() > body_limit {
        return Err(WebFailure::TooLarge {
            what: crate::web_error::SizeLimit::RequestBody,
            limit: body_limit,
        });
    }
    let (scheme, rest) = url.split_once("://").ok_or_else(|| {
        WebFailure::InvalidUrl(bn_catalog::text("web", "client.url_authority_missing").into())
    })?;
    if scheme != "http" && scheme != "https" {
        return Err(WebFailure::InvalidUrl(
            bn_catalog::text("web", "client.url_scheme").into(),
        ));
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
    let tls_config = if scheme == "https" {
        Some(Arc::new(crate::tls::client_config()?))
    } else {
        None
    };
    let deadline = std::time::Duration::from_millis(policy.total_deadline_ms());
    let addresses =
        resolve_validated_addresses_with_policy(host.as_str(), port, resolver, policy, scheme)?;
    if addresses.is_empty() {
        return Err(WebFailure::NoResolvedAddresses);
    }
    let mut connect_error = None;
    let mut stream = None;
    let mut peer_addr = None;
    for address in &addresses {
        let endpoint = match bn_core_net::Address::parse(&address.to_string()) {
            Ok(addr) => bn_core_net::Endpoint::new(addr, port),
            Err(_) => continue,
        };
        match bn_core_net::TcpStream::connect(endpoint, deadline) {
            Ok(connected) => {
                peer_addr = Some(*address);
                stream = Some(connected);
                break;
            }
            Err(err) => {
                let failure = connect_failure(&err);
                if failure.code() == bn_types::error_codes::web::LIMIT {
                    return Err(failure);
                }
                connect_error = Some(failure);
            }
        }
    }
    let Some(stream) = stream else {
        return Err(connect_error.unwrap_or_else(|| {
            WebFailure::ConnectionFailed(bn_catalog::text("web", "client.connect_failed").into())
        }));
    };
    let (std_stream, permit) = stream.into_std();
    std_stream
        .set_nonblocking(true)
        .map_err(|error| WebFailure::HttpFailed(error.to_string()))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(|error| WebFailure::HttpFailed(error.to_string()))?;
    let mut reply = runtime.block_on(async move {
        let _permit = permit;
        let stream = tokio::net::TcpStream::from_std(std_stream)
            .map_err(|error| WebFailure::HttpFailed(error.to_string()))?;
        if let Some(config) = tls_config {
            let connector = TlsConnector::from(config);
            let server_name =
                rustls::pki_types::ServerName::try_from(host.clone()).map_err(|_| {
                    WebFailure::InvalidUrl(bn_catalog::text("web", "client.tls_server_name").into())
                })?;
            let stream = connector
                .connect(server_name, stream)
                .await
                .map_err(|error| {
                    WebFailure::TlsHandshakeFailed(bn_catalog::fill(
                        "web",
                        "client.tls_handshake",
                        &[("error", &error.to_string())],
                    ))
                })?;
            perform_transfer(
                stream,
                method,
                authority,
                target,
                body,
                extra_headers,
                policy,
            )
            .await
        } else {
            perform_transfer(
                stream,
                method,
                authority,
                target,
                body,
                extra_headers,
                policy,
            )
            .await
        }
    })?;
    reply.peer = peer_addr;
    Ok(reply)
}

pub(super) async fn perform_transfer<I>(
    stream: I,
    method: &str,
    authority: &str,
    target: &str,
    body: &[u8],
    extra_headers: &[(String, String)],
    policy: &crate::web::EgressPolicy,
) -> Result<crate::web::ClientReply, WebFailure>
where
    I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let io = TokioIo::new(stream);
    let (mut sender, connection) = tokio::time::timeout(
        std::time::Duration::from_millis(policy.total_deadline_ms()),
        hyper::client::conn::http1::handshake(io),
    )
    .await
    .map_err(|_| WebFailure::HandshakeTimeout)?
    .map_err(|error| WebFailure::HttpFailed(error.to_string()))?;
    tokio::spawn(async move {
        let _ = connection.await;
    });
    let mut builder = Request::builder()
        .method(method)
        .uri(target)
        .header("host", authority);
    for (k, v) in extra_headers {
        builder = builder.header(k.as_str(), v.as_str());
    }
    if !body.is_empty() {
        builder = builder.header("content-length", body.len());
    }
    let request = builder
        .body(Full::new(Bytes::copy_from_slice(body)))
        .map_err(|error| WebFailure::InvalidHeader(error.to_string()))?;
    let response = tokio::time::timeout(
        std::time::Duration::from_millis(policy.total_deadline_ms()),
        sender.send_request(request),
    )
    .await
    .map_err(|_| WebFailure::Timeout(bn_catalog::text("web", "client.request_timeout").into()))?
    .map_err(|error| WebFailure::HttpFailed(error.to_string()))?;
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
    let bytes = tokio::time::timeout(
        std::time::Duration::from_millis(policy.total_deadline_ms()),
        response.into_body().collect(),
    )
    .await
    .map_err(|_| WebFailure::Timeout(bn_catalog::text("web", "client.body_timeout").into()))?
    .map_err(|error| WebFailure::HttpFailed(error.to_string()))?
    .to_bytes();
    let body_limit = bn_limits::web_limits().max_response_body_bytes;
    if bytes.len() > body_limit {
        return Err(WebFailure::TooLarge {
            what: crate::web_error::SizeLimit::ResponseBody,
            limit: body_limit,
        });
    }
    let body_bytes = bytes.to_vec();
    Ok(crate::web::ClientReply {
        status,
        http_version: "HTTP/1.1".into(),
        body_size: body_bytes.len(),
        total_ms: 0,
        connect_ms: 0,
        headers,
        body: body_bytes,
        peer: None,
        closed: false,
    })
}

#[cfg(test)]
pub(super) fn resolve_validated_addresses(
    host: &str,
    port: u16,
    resolver: &dyn AddressResolver,
) -> Result<Vec<std::net::IpAddr>, WebFailure> {
    resolve_validated_addresses_with_policy(
        host,
        port,
        resolver,
        &crate::web::EgressPolicy::default(),
        "http",
    )
}

pub(super) fn resolve_validated_addresses_with_policy(
    host: &str,
    port: u16,
    resolver: &dyn AddressResolver,
    policy: &crate::web::EgressPolicy,
    scheme: &str,
) -> Result<Vec<std::net::IpAddr>, WebFailure> {
    let timeout = std::time::Duration::from_millis(policy.total_deadline_ms());
    let addresses = resolver
        .resolve(
            host,
            port,
            bn_limits::web_limits().resolved_addresses_max,
            timeout,
        )?
        .into_iter()
        .map(bn_core_net::Address::as_std)
        .collect::<Vec<_>>();
    policy.filter_allowed_addresses(scheme, port, &addresses)
}

pub(super) fn resolve_redirect(base: &str, location: &str) -> Result<String, WebFailure> {
    if location.contains('#') || location.contains('\n') || location.contains('\r') {
        return Err(WebFailure::InvalidUrl(
            bn_catalog::text("web", "client.redirect_location").into(),
        ));
    }
    if location.starts_with("http://") || location.starts_with("https://") {
        crate::web::validate_client_url(location)?;
        return Ok(location.to_owned());
    }
    let (scheme, rest) = base.split_once("://").ok_or_else(|| {
        WebFailure::InvalidUrl(bn_catalog::text("web", "client.redirect_base").into())
    })?;
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
    crate::web::validate_client_url(&resolved)?;
    Ok(resolved)
}

#[cfg(test)]
pub(super) fn parse_authority(authority: &str) -> Result<(String, u16), WebFailure> {
    parse_authority_with_default(authority, 80)
}

pub(super) fn parse_authority_with_default(
    authority: &str,
    default_port: u16,
) -> Result<(String, u16), WebFailure> {
    let invalid = |reason: &str| WebFailure::InvalidUrl(reason.into());
    if authority.is_empty() || authority.contains('@') {
        return Err(invalid(bn_catalog::text("web", "client.url_authority")));
    }
    if let Some(host) = authority.strip_prefix('[') {
        let close = host
            .find(']')
            .ok_or_else(|| invalid(bn_catalog::text("web", "client.ipv6_authority")))?;
        let address = &host[..close];
        let port = match host.get(close + 1..).filter(|suffix| !suffix.is_empty()) {
            None => default_port,
            Some(suffix) => suffix
                .strip_prefix(':')
                .ok_or_else(|| invalid(bn_catalog::text("web", "client.ipv6_authority")))?
                .parse()
                .map_err(|_| invalid(bn_catalog::text("web", "client.port")))?,
        };
        return Ok((address.to_owned(), port));
    }
    let default_port = default_port.to_string();
    let (host, port) = authority
        .rsplit_once(':')
        .unwrap_or((authority, default_port.as_str()));
    if host.is_empty() {
        return Err(invalid(bn_catalog::text("web", "client.url_authority")));
    }
    Ok((
        host.to_owned(),
        port.parse()
            .map_err(|_| invalid(bn_catalog::text("web", "client.port")))?,
    ))
}

/// The failure of one outbound connection attempt: a process socket quota is
/// `LIMIT`; any other refusal is `HTTP_FAILED` with the system's reason.
pub(super) fn connect_failure(error: &std::io::Error) -> WebFailure {
    if bn_core_net::is_quota_error(error) {
        WebFailure::LimitExceeded(error.to_string())
    } else {
        WebFailure::ConnectionFailed(error.to_string())
    }
}

#[cfg(test)]
mod tests;
