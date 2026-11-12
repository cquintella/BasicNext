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

/// The one deadline of a client request: `totalDeadlineMs` from its start
/// bounds resolution, connection, TLS, the HTTP handshake, the request and
/// the body, across every redirect (bnweb.md, Egress policy).
#[derive(Clone, Copy, Debug)]
pub(super) struct Deadline(std::time::Instant);

impl Deadline {
    fn after(milliseconds: u64) -> Self {
        Self(std::time::Instant::now() + std::time::Duration::from_millis(milliseconds))
    }

    /// The time left, or `TIMEOUT` once it is gone.
    fn remaining(self) -> Result<std::time::Duration, WebFailure> {
        let left = self.0.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            Err(WebFailure::Timeout(
                bn_catalog::text("web", "client.deadline").into(),
            ))
        } else {
            Ok(left)
        }
    }

    /// `future` within the time left; `timed_out` names the step that ran
    /// past it.
    async fn within<T>(
        self,
        future: impl std::future::Future<Output = T>,
        timed_out: impl FnOnce() -> WebFailure,
    ) -> Result<T, WebFailure> {
        tokio::time::timeout(self.remaining()?, future)
            .await
            .map_err(|_| timed_out())
    }
}

fn timeout_of(key: &str) -> impl FnOnce() -> WebFailure + '_ {
    move || WebFailure::Timeout(bn_catalog::text("web", key).into())
}

pub(super) fn client_transfer_with_resolver_and_policy(
    method: &str,
    url: &str,
    body: &[u8],
    extra_headers: &[(String, String)],
    resolver: &dyn AddressResolver,
    policy: &crate::web::EgressPolicy,
) -> Result<crate::web::ClientReply, WebFailure> {
    let deadline = Deadline::after(policy.total_deadline_ms());
    let mut current = crate::web::url::ClientUrl::parse(url)?;
    for _ in 0..=policy.max_redirects() {
        let reply = client_transfer_once(
            method,
            &current,
            body,
            extra_headers,
            resolver,
            policy,
            deadline,
        )?;
        if !(300..=399).contains(&reply.status) {
            return Ok(reply);
        }
        let location = reply
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("location"))
            .map(|(_, value)| value.clone())
            .ok_or(WebFailure::MissingRedirectLocation)?;
        current = current.resolve(&location)?;
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

/// One exchange with the destination of `url`, under `deadline`.
pub(super) fn client_transfer_once(
    method: &str,
    url: &crate::web::url::ClientUrl,
    body: &[u8],
    extra_headers: &[(String, String)],
    resolver: &dyn AddressResolver,
    policy: &crate::web::EgressPolicy,
    deadline: Deadline,
) -> Result<crate::web::ClientReply, WebFailure> {
    let body_limit = bn_limits::web_limits().max_body_bytes;
    if body.len() > body_limit {
        return Err(WebFailure::TooLarge {
            what: crate::web_error::SizeLimit::RequestBody,
            limit: body_limit,
        });
    }
    let tls_config = match url.scheme() {
        crate::web::url::Scheme::Https => Some(Arc::new(crate::tls::client_config()?)),
        crate::web::url::Scheme::Http => None,
    };
    let addresses = resolve_allowed_addresses(
        url.host(),
        url.port(),
        resolver,
        policy,
        url.scheme().as_str(),
        deadline.remaining()?,
    )?;
    if addresses.is_empty() {
        return Err(WebFailure::NoResolvedAddresses);
    }
    let (stream, peer) = connect_first(&addresses, url.port(), deadline)?;
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
        let exchange = Exchange {
            method,
            url,
            body,
            extra_headers,
            deadline,
        };
        let Some(config) = tls_config else {
            return exchange.perform(stream).await;
        };
        let server_name =
            rustls::pki_types::ServerName::try_from(url.host().to_owned()).map_err(|_| {
                WebFailure::InvalidUrl(bn_catalog::text("web", "client.tls_server_name").into())
            })?;
        let stream = deadline
            .within(
                TlsConnector::from(config).connect(server_name, stream),
                timeout_of("client.tls_timeout"),
            )
            .await?
            .map_err(|error| {
                WebFailure::TlsHandshakeFailed(bn_catalog::fill(
                    "web",
                    "client.tls_handshake",
                    &[("error", &error.to_string())],
                ))
            })?;
        exchange.perform(stream).await
    })?;
    reply.peer = Some(peer);
    Ok(reply)
}

/// The first allowed address that accepts a connection before `deadline`.
fn connect_first(
    addresses: &[std::net::IpAddr],
    port: u16,
    deadline: Deadline,
) -> Result<(bn_core_net::TcpStream, std::net::IpAddr), WebFailure> {
    let mut connect_error = None;
    for address in addresses {
        let Ok(parsed) = bn_core_net::Address::parse(&address.to_string()) else {
            continue;
        };
        let endpoint = bn_core_net::Endpoint::new(parsed, port);
        match bn_core_net::TcpStream::connect(endpoint, deadline.remaining()?) {
            Ok(connected) => return Ok((connected, *address)),
            Err(err) => {
                let failure = connect_failure(&err);
                if failure.code() == bn_types::error_codes::web::LIMIT {
                    return Err(failure);
                }
                connect_error = Some(failure);
            }
        }
    }
    Err(connect_error.unwrap_or_else(|| {
        WebFailure::ConnectionFailed(bn_catalog::text("web", "client.connect_failed").into())
    }))
}

/// One HTTP/1.1 exchange on a connected stream.
struct Exchange<'a> {
    method: &'a str,
    url: &'a crate::web::url::ClientUrl,
    body: &'a [u8],
    extra_headers: &'a [(String, String)],
    deadline: Deadline,
}

impl Exchange<'_> {
    async fn perform<I>(self, stream: I) -> Result<crate::web::ClientReply, WebFailure>
    where
        I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let deadline = self.deadline;
        let (mut sender, connection) = deadline
            .within(
                hyper::client::conn::http1::handshake(TokioIo::new(stream)),
                || WebFailure::HandshakeTimeout,
            )
            .await?
            .map_err(|error| WebFailure::HttpFailed(error.to_string()))?;
        tokio::spawn(async move {
            let _ = connection.await;
        });
        let mut builder = Request::builder()
            .method(self.method)
            .uri(self.url.target())
            .header("host", self.url.authority());
        for (name, value) in self.extra_headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        if !self.body.is_empty() {
            builder = builder.header("content-length", self.body.len());
        }
        let request = builder
            .body(Full::new(Bytes::copy_from_slice(self.body)))
            .map_err(|error| WebFailure::InvalidHeader(error.to_string()))?;
        let response = deadline
            .within(
                sender.send_request(request),
                timeout_of("client.request_timeout"),
            )
            .await?
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
        let bytes = deadline
            .within(
                response.into_body().collect(),
                timeout_of("client.body_timeout"),
            )
            .await?
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
}

#[cfg(test)]
pub(super) fn resolve_validated_addresses(
    host: &str,
    port: u16,
    resolver: &dyn AddressResolver,
) -> Result<Vec<std::net::IpAddr>, WebFailure> {
    let policy = crate::web::EgressPolicy::default();
    let timeout = std::time::Duration::from_millis(policy.total_deadline_ms());
    resolve_allowed_addresses(host, port, resolver, &policy, "http", timeout)
}

#[cfg(test)]
pub(super) fn resolve_validated_addresses_with_policy(
    host: &str,
    port: u16,
    resolver: &dyn AddressResolver,
    policy: &crate::web::EgressPolicy,
    scheme: &str,
) -> Result<Vec<std::net::IpAddr>, WebFailure> {
    let timeout = std::time::Duration::from_millis(policy.total_deadline_ms());
    resolve_allowed_addresses(host, port, resolver, policy, scheme, timeout)
}

/// The resolved addresses of `host` the policy allows, resolved within `timeout`.
pub(super) fn resolve_allowed_addresses(
    host: &str,
    port: u16,
    resolver: &dyn AddressResolver,
    policy: &crate::web::EgressPolicy,
    scheme: &str,
    timeout: std::time::Duration,
) -> Result<Vec<std::net::IpAddr>, WebFailure> {
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

#[cfg(test)]
pub(super) fn resolve_redirect(base: &str, location: &str) -> Result<String, WebFailure> {
    Ok(crate::web::url::ClientUrl::parse(base)?
        .resolve(location)?
        .to_string())
}

#[cfg(test)]
pub(super) fn parse_authority(authority: &str) -> Result<(String, u16), WebFailure> {
    crate::web::url::parse_authority(authority, 80)
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
