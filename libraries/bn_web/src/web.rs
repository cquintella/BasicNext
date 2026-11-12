use std::{collections::HashMap, net::IpAddr};

use crate::web_error::WebFailure;

/// Why `EgressPolicy::new` refused a redirect limit or deadline (both
/// backends report it as `INVALID_ARGUMENT`).
pub const EGRESS_POLICY_BOUNDS: &str = "egress policy value is outside the configured bounds";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EgressPolicy {
    schemes: Option<Vec<String>>,
    cidrs: Option<Vec<bn_core_net::Cidr>>,
    ports: Option<Vec<u16>>,
    max_redirects: usize,
    total_deadline_ms: u64,
}

impl Default for EgressPolicy {
    fn default() -> Self {
        let limits = bn_limits::web_limits();
        Self {
            schemes: None,
            cidrs: None,
            ports: None,
            max_redirects: limits.redirects,
            total_deadline_ms: limits.connection_total_ms,
        }
    }
}

impl EgressPolicy {
    pub fn from_csv(
        schemes: &str,
        cidrs: &str,
        ports: &str,
        max_redirects: usize,
        total_deadline_ms: u64,
    ) -> Result<Self, &'static str> {
        let schemes = Some(parse_csv(schemes, |value| match value {
            "http" | "https" => Ok(value.to_owned()),
            _ => Err("egress policy contains an unsupported scheme"),
        })?);
        let limits = bn_limits::web_limits();
        let cidrs = Some(parse_csv(cidrs, bn_core_net::Cidr::parse)?);
        let ports = Some(parse_csv(ports, |value| {
            value
                .parse::<u16>()
                .map_err(|_| "egress policy contains an invalid port")
        })?);
        if schemes
            .as_ref()
            .is_some_and(|values| values.len() > limits.egress_list_max)
            || cidrs
                .as_ref()
                .is_some_and(|values| values.len() > limits.egress_list_max)
            || ports
                .as_ref()
                .is_some_and(|values| values.len() > limits.egress_list_max)
        {
            return Err("egress policy list exceeds configured capacity");
        }
        Self::new(schemes, cidrs, ports, max_redirects, total_deadline_ms)
    }

    pub(crate) fn new(
        schemes: Option<Vec<String>>,
        cidrs: Option<Vec<bn_core_net::Cidr>>,
        ports: Option<Vec<u16>>,
        max_redirects: usize,
        total_deadline_ms: u64,
    ) -> Result<Self, &'static str> {
        let limits = bn_limits::web_limits();
        if !(0..=limits.redirects_max).contains(&max_redirects)
            || !(1..=limits.connection_total_max_ms).contains(&total_deadline_ms)
        {
            return Err(EGRESS_POLICY_BOUNDS);
        }
        if schemes.as_ref().is_some_and(|values| {
            values.is_empty()
                || values
                    .iter()
                    .any(|scheme| !matches!(scheme.as_str(), "http" | "https"))
        }) {
            return Err("egress policy contains an unsupported scheme");
        }
        if ports.as_ref().is_some_and(|values| values.contains(&0)) {
            return Err("egress policy contains an invalid port");
        }
        Ok(Self {
            schemes,
            cidrs,
            ports,
            max_redirects,
            total_deadline_ms,
        })
    }

    pub(crate) fn filter_allowed_addresses(
        &self,
        scheme: &str,
        port: u16,
        addresses: &[IpAddr],
    ) -> Result<Vec<IpAddr>, WebFailure> {
        if self
            .schemes
            .as_ref()
            .is_some_and(|values| !values.iter().any(|value| value == scheme))
            || self
                .ports
                .as_ref()
                .is_some_and(|values| !values.contains(&port))
        {
            return Err(WebFailure::EgressDenied(
                "destination is outside the egress policy".into(),
            ));
        }
        if addresses.is_empty() {
            return Err(WebFailure::NoResolvedAddresses);
        }
        let mut allowed = Vec::new();
        for &address in addresses {
            if self.allows_candidate(address) {
                allowed.push(address);
            }
        }
        if allowed.is_empty() {
            return Err(WebFailure::EgressDenied(
                "destination is outside the egress policy".into(),
            ));
        }
        Ok(allowed)
    }

    fn allows_candidate(&self, address: IpAddr) -> bool {
        if let Some(cidrs) = &self.cidrs {
            if cidrs.is_empty() {
                return false;
            }
            let normalized = match address {
                IpAddr::V4(addr) => IpAddr::V4(addr),
                IpAddr::V6(addr) => addr.to_ipv4().map_or(IpAddr::V6(addr), IpAddr::V4),
            };
            if let Ok(net_addr) = bn_core_net::Address::parse(&normalized.to_string()) {
                if !cidrs.iter().any(|cidr| cidr.contains(net_addr)) {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }

    pub(crate) const fn max_redirects(&self) -> usize {
        self.max_redirects
    }
    pub(crate) const fn total_deadline_ms(&self) -> u64 {
        self.total_deadline_ms
    }
}

fn parse_csv<T, F>(input: &str, parse: F) -> Result<Vec<T>, &'static str>
where
    F: Fn(&str) -> Result<T, &'static str>,
{
    if input.trim().is_empty() {
        return Ok(Vec::new());
    }
    input.split(',').map(str::trim).map(parse).collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Target {
    pub path: String,
    pub query: String,
}

#[derive(Clone, Debug)]
pub struct Request {
    pub method: String,
    pub target: Target,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub peer: IpAddr,
    /// The matched route's `:name` parameters, sorted by name.
    pub params: Vec<(String, String)>,
    /// Whether the request arrived over TLS (a `Secure` cookie needs it).
    pub secure: bool,
    /// The server session, when the server keeps sessions.
    pub session: Option<sessions::RequestSession>,
}

impl Request {
    /// # Errors
    ///
    /// Returns the validation message for a malformed target or header set.
    pub fn new(
        method: &str,
        raw_target: &str,
        headers: Vec<(String, String)>,
        body: &str,
        peer: IpAddr,
    ) -> Result<Self, WebFailure> {
        if method.is_empty()
            || method.bytes().any(|byte| {
                !byte.is_ascii_uppercase() && !byte.is_ascii_digit() && !b"-".contains(&byte)
            })
        {
            return Err(WebFailure::InvalidArgument("invalid request method".into()));
        }
        let target = canonical_target(raw_target)
            .map_err(|reason| WebFailure::InvalidArgument(reason.into()))?;
        validate_headers(&headers)?;
        bounded_body(body, bn_limits::web_limits().max_body_bytes as i128)?;
        Ok(Self {
            method: method.into(),
            target,
            headers,
            body: body.into(),
            peer,
            params: Vec::new(),
            secure: false,
            session: None,
        })
    }
    pub fn query(&self, name: &str) -> Result<QueryValues, WebFailure> {
        QueryValues::from_query(&self.target.query, name)
    }
    pub fn header(&self, name: &str) -> Result<HeaderValues, WebFailure> {
        HeaderValues::from_headers(&self.headers, name)
    }
    /// The value of the route parameter `name` (`:name` in the matched
    /// pattern; `/users/:id` gives `id`).
    #[must_use]
    pub fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
    /// The value of the cookie `name` the client sent (RFC 6265 §5.4: `name=value`
    /// pairs separated by `;` in every `Cookie` header; a quoted value loses its
    /// quotes; the first pair with that name wins).
    #[must_use]
    pub fn cookie(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .filter(|(header, _)| header.eq_ignore_ascii_case("cookie"))
            .flat_map(|(_, value)| value.split(';'))
            .filter_map(|pair| pair.trim().split_once('='))
            .find(|(key, _)| *key == name)
            .map(|(_, value)| {
                value
                    .strip_prefix('"')
                    .and_then(|quoted| quoted.strip_suffix('"'))
                    .unwrap_or(value)
            })
    }
    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target.path
    }
    pub fn body(&self, maximum: i128) -> Result<&str, WebFailure> {
        bounded_body(&self.body, maximum)
    }
    #[must_use]
    pub fn peer_address(&self) -> IpAddr {
        self.peer
    }
    #[must_use]
    pub fn effective_client_address(&self, trusted_proxy: bool) -> IpAddr {
        let forwarded = self
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("x-forwarded-for"))
            .map(|(_, value)| value.as_str());
        effective_client_address(self.peer, forwarded, trusted_proxy)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub committed: bool,
    /// Whether the response travels over TLS (copied from the request); a
    /// `Secure` cookie needs it.
    pub secure: bool,
    closed: bool,
}

impl Default for Request {
    fn default() -> Self {
        Self {
            method: "GET".into(),
            target: Target {
                path: "/".into(),
                query: String::new(),
            },
            headers: Vec::new(),
            body: String::new(),
            peer: std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            params: Vec::new(),
            secure: false,
            session: None,
        }
    }
}

impl Response {
    #[must_use]
    pub fn new() -> Self {
        Self {
            status: 200,
            ..Self::default()
        }
    }
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .rev()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
    pub fn set_status(&mut self, status: u16) -> Result<(), WebFailure> {
        if self.committed || self.closed {
            return Err(WebFailure::ResponseCommittedOrClosed);
        }
        if !(100..=599).contains(&status) {
            return Err(WebFailure::InvalidStatus(i128::from(status)));
        }
        self.status = status;
        Ok(())
    }
    pub fn set_header(&mut self, name: &str, value: &str) -> Result<(), WebFailure> {
        if self.committed || self.closed {
            return Err(WebFailure::ResponseCommittedOrClosed);
        }
        if name.is_empty()
            || name
                .bytes()
                .any(|byte| byte <= 0x20 || byte == 0x7f || byte == b':')
            || value.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
        {
            return Err(WebFailure::InvalidHeader("invalid response header".into()));
        }
        if self
            .headers
            .iter()
            .map(|(key, value)| key.len() + value.len())
            .sum::<usize>()
            + name.len()
            + value.len()
            > bn_limits::web_limits().max_header_bytes
        {
            return Err(WebFailure::TooLarge {
                what: crate::web_error::SizeLimit::ResponseHeaders,
                limit: bn_limits::web_limits().max_header_bytes,
            });
        }
        self.headers.push((name.to_ascii_lowercase(), value.into()));
        Ok(())
    }
    /// Adds a `Set-Cookie` header (RFC 6265): no `domain` is a host-only
    /// cookie, no `max_age` a session cookie. A `Secure` cookie needs a response
    /// that travels over TLS (`secure`).
    pub fn set_cookie(
        &mut self,
        name: &str,
        value: &str,
        domain: Option<&str>,
        path: &str,
        max_age: Option<std::time::Duration>,
        options: crate::web_state::CookieOptions,
    ) -> Result<(), WebFailure> {
        if options.secure && !self.secure {
            return Err(WebFailure::SecureCookieOverCleartext);
        }
        let cookie_header = options.format_set_cookie(name, value, domain, path, max_age)?;
        self.set_header("set-cookie", &cookie_header)
    }
    /// Expires the cookie `name` (`Max-Age=0`); `Secure` only when the
    /// response travels over TLS, so a cleartext response can clear it too.
    pub fn clear_cookie(
        &mut self,
        name: &str,
        domain: Option<&str>,
        path: &str,
    ) -> Result<(), WebFailure> {
        let options = crate::web_state::CookieOptions {
            secure: self.secure,
            ..crate::web_state::CookieOptions::default()
        };
        self.set_cookie(
            name,
            "",
            domain,
            path,
            Some(std::time::Duration::ZERO),
            options,
        )
    }
    pub fn write(&mut self, body: &str) -> Result<(), WebFailure> {
        if self.committed || self.closed {
            return Err(WebFailure::ResponseCommittedOrClosed);
        }
        let limit = bn_limits::web_limits().max_response_body_bytes;
        if self.body.len() + body.len() > limit {
            return Err(WebFailure::TooLarge {
                what: crate::web_error::SizeLimit::ResponseBody,
                limit,
            });
        }
        self.body.push_str(body);
        Ok(())
    }
    pub fn commit(&mut self) -> Result<(), WebFailure> {
        if self.closed {
            return Err(WebFailure::ResponseClosed);
        }
        self.committed = true;
        Ok(())
    }
    #[must_use]
    pub fn is_committed(&self) -> bool {
        self.committed
    }
    pub fn close(&mut self) {
        self.closed = true;
    }
    pub(crate) fn finish_for_method(&mut self, method: &str) -> Result<(), WebFailure> {
        if method == "HEAD" {
            self.body.clear();
        }
        self.commit()
    }
}

pub(crate) fn canonical_target(raw: &str) -> Result<Target, &'static str> {
    if raw.is_empty()
        || raw.len() > bn_limits::web_limits().max_target_bytes
        || raw
            .bytes()
            .any(|byte| byte == b'\\' || byte < 0x20 || byte == 0x7f)
    {
        return Err("invalid request target");
    }
    let (raw_path, query) = raw.split_once('?').unwrap_or((raw, ""));
    let mut path = String::new();
    for (index, segment) in raw_path.split('/').enumerate() {
        if index > 0 {
            path.push('/');
        }
        let decoded = decode_component(segment)?;
        if decoded.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
            || decoded == "."
            || decoded == ".."
            || decoded.contains('/')
            || decoded.contains('\\')
        {
            return Err("ambiguous path segment");
        }
        path.push_str(&decoded);
    }
    if !path.starts_with('/') {
        return Err("request target must be an origin-form path");
    }
    Ok(Target {
        path,
        query: query.to_string(),
    })
}

pub(crate) fn query_values(query: &str, name: &str) -> Result<Vec<String>, WebFailure> {
    let limit = bn_limits::web_limits().max_target_bytes;
    if query.len() > limit {
        return Err(WebFailure::TooLarge {
            what: crate::web_error::SizeLimit::Query,
            limit,
        });
    }
    let mut values = Vec::new();
    let mut field_count = 0;
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        field_count += 1;
        if pair.len() > bn_limits::web_limits().max_target_bytes
            || field_count > bn_limits::web_limits().max_header_fields
        {
            return Err(WebFailure::LimitExceeded(
                "query fields exceed bounds".into(),
            ));
        }
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let key = decode_component(key).map_err(|reason| WebFailure::InvalidUrl(reason.into()))?;
        let value =
            decode_component(value).map_err(|reason| WebFailure::InvalidUrl(reason.into()))?;
        if key.len() > bn_limits::web_limits().max_target_bytes
            || value.len() > bn_limits::web_limits().max_target_bytes
        {
            return Err(WebFailure::LimitExceeded(
                "query key or value exceeds bounds".into(),
            ));
        }
        if key == name {
            values.push(value);
        }
    }
    Ok(values)
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct QueryValues(Vec<String>);

impl QueryValues {
    pub(crate) fn from_query(query: &str, name: &str) -> Result<Self, WebFailure> {
        Ok(Self(query_values(query, name)?))
    }
    #[must_use]
    pub fn count(&self) -> usize {
        self.0.len()
    }
    pub fn get(&self, index: usize) -> Option<&str> {
        self.0.get(index).map(String::as_str)
    }
}

pub(crate) fn header_values(
    headers: &[(String, String)],
    name: &str,
) -> Result<Vec<String>, WebFailure> {
    if name.is_empty()
        || name.len() > 128
        || name
            .bytes()
            .any(|byte| byte <= 0x20 || byte == 0x7f || byte == b':')
    {
        return Err(WebFailure::InvalidHeader("invalid header name".into()));
    }
    let wanted = name.to_ascii_lowercase();
    Ok(headers
        .iter()
        .filter(|(key, _)| key.eq_ignore_ascii_case(&wanted))
        .map(|(_, value)| value.clone())
        .collect())
}

fn validate_headers(headers: &[(String, String)]) -> Result<(), WebFailure> {
    if headers.len() > bn_limits::web_limits().max_header_fields
        || headers
            .iter()
            .map(|(name, value)| name.len() + value.len())
            .sum::<usize>()
            > bn_limits::web_limits().max_header_bytes
    {
        return Err(WebFailure::LimitExceeded(
            "request headers exceed bounds".into(),
        ));
    }
    if headers.iter().any(|(name, value)| {
        name.is_empty()
            || name
                .bytes()
                .any(|byte| byte <= 0x20 || byte == 0x7f || byte == b':')
            || value.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
    }) {
        return Err(WebFailure::InvalidHeader("invalid request header".into()));
    }
    Ok(())
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HeaderValues(Vec<String>);

impl HeaderValues {
    pub(crate) fn from_headers(
        headers: &[(String, String)],
        name: &str,
    ) -> Result<Self, WebFailure> {
        Ok(Self(header_values(headers, name)?))
    }
    #[must_use]
    pub fn count(&self) -> usize {
        self.0.len()
    }
    pub fn get(&self, index: usize) -> Option<&str> {
        self.0.get(index).map(String::as_str)
    }
}

pub fn validate_client_url(url: &str) -> Result<(), WebFailure> {
    let invalid = |reason: &str| WebFailure::InvalidUrl(reason.into());
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| invalid("URL requires an authority"))?;
    if !matches!(scheme, "http" | "https") || rest.is_empty() || rest.contains('#') {
        return Err(invalid("unsupported or ambiguous URL"));
    }
    let authority = rest.split(['/', '?']).next().unwrap_or_default();
    if authority.is_empty()
        || authority.contains('@')
        || authority.bytes().any(|byte| byte < 0x21 || byte == 0x7f)
    {
        return Err(invalid("invalid URL authority"));
    }
    Ok(())
}

pub fn bounded_body(body: &str, maximum: i128) -> Result<&str, WebFailure> {
    let limit = bn_limits::web_limits().max_response_body_bytes;
    let too_large = || WebFailure::TooLarge {
        what: crate::web_error::SizeLimit::BodyMaximum,
        limit,
    };
    let maximum = usize::try_from(maximum).map_err(|_| too_large())?;
    if maximum > limit {
        return Err(too_large());
    }
    if body.len() > maximum {
        return Err(WebFailure::BodyLimitExceeded(
            "request body exceeds declared limit".into(),
        ));
    }
    Ok(body)
}

pub(crate) fn effective_client_address(
    peer: IpAddr,
    forwarded_for: Option<&str>,
    trusted_proxy: bool,
) -> IpAddr {
    if !trusted_proxy {
        return peer;
    }
    forwarded_for
        .and_then(|value| value.split(',').next())
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(peer)
}

fn decode_component(input: &str) -> Result<String, &'static str> {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err("malformed percent escape");
            }
            let high = hex(bytes[index + 1]).ok_or("malformed percent escape")?;
            let low = hex(bytes[index + 2]).ok_or("malformed percent escape")?;
            output.push((high << 4) | low);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).map_err(|_| "invalid UTF-8 in target")
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// A matched route's parameters as `Request::params`: name and value pairs
/// sorted by name (the router's map has no order).
#[must_use]
pub fn sorted_params<S: std::hash::BuildHasher>(
    found: &HashMap<String, String, S>,
) -> Vec<(String, String)> {
    let mut params = found
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect::<Vec<_>>();
    params.sort();
    params
}

mod routing;
pub use routing::{Route, RouteOutcome};
#[cfg(test)]
pub(crate) use routing::{allowed_methods, select_route};
pub(crate) use routing::{dispatch_route, route_for_request, valid_method, valid_route_pattern};
mod drain;
pub use drain::{DrainNotice, DrainSignal, HandlerTasks, drain_server};
pub(crate) mod policy_opener;
mod server;
pub mod sessions;
pub(crate) mod static_files;
pub use server::{
    ConnectionWork, ServerOptions, ServerState, ServerStats, ServerStatus, spawn_listener_thread,
};
pub use static_files::{
    StaticFileContent, StaticFileError, StaticFileOpener, StaticMount, StaticServeOutcome,
    serve_static_file,
};

#[cfg(test)]
mod request_tests;
#[cfg(test)]
mod sessions_tests;
#[cfg(test)]
mod tests;

// ---- Client Transfer Types (ClientReply, Headers, Query) -----------------

#[derive(Clone, Debug, Default)]
pub struct ClientReply {
    pub status: u16,
    pub http_version: String,
    pub body_size: usize,
    pub total_ms: i64,
    pub connect_ms: i64,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub peer: Option<IpAddr>,
    pub closed: bool,
}

impl ClientReply {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn status(&self) -> u16 {
        self.status
    }

    #[must_use]
    pub fn http_version(&self) -> &str {
        if self.http_version.is_empty() {
            "HTTP/1.1"
        } else {
            &self.http_version
        }
    }

    #[must_use]
    pub fn body_size(&self) -> usize {
        self.body_size
    }

    #[must_use]
    pub fn total_ms(&self) -> i64 {
        self.total_ms
    }

    #[must_use]
    pub fn connect_ms(&self) -> i64 {
        self.connect_ms
    }

    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    #[must_use]
    pub fn header_count(&self) -> usize {
        self.headers.len()
    }

    #[must_use]
    pub fn header_name(&self, index: usize) -> Option<&str> {
        self.headers.get(index).map(|(k, _)| k.as_str())
    }

    #[must_use]
    pub fn header_value(&self, index: usize) -> Option<&str> {
        self.headers.get(index).map(|(_, v)| v.as_str())
    }

    pub fn body_text(&self, maximum: usize) -> Result<String, WebFailure> {
        if self.body.len() > maximum {
            return Err(WebFailure::BodyLimitExceeded(format!(
                "{}/{}",
                self.body.len(),
                maximum
            )));
        }
        String::from_utf8(self.body.clone())
            .map_err(|_| WebFailure::InvalidArgument("invalid_utf8".into()))
    }

    pub fn body_bytes(&self, maximum: usize) -> Result<&[u8], WebFailure> {
        if self.body.len() > maximum {
            return Err(WebFailure::BodyLimitExceeded(format!(
                "{}/{}",
                self.body.len(),
                maximum
            )));
        }
        Ok(&self.body)
    }

    pub fn body_decoded_bytes(&self, maximum: usize) -> Result<Vec<u8>, WebFailure> {
        let encoding = self.header("content-encoding").unwrap_or("").trim();
        let decoded = if encoding.is_empty() || encoding.eq_ignore_ascii_case("identity") {
            self.body.clone()
        } else if encoding.eq_ignore_ascii_case("gzip") {
            bn_core_compress::gunzip_bounded(&self.body, maximum as u64)
                .map_err(|_| WebFailure::InvalidArgument("gunzip_failed".into()))?
        } else if encoding.eq_ignore_ascii_case("deflate") {
            bn_core_compress::inflate_bounded(&self.body, maximum as u64)
                .map_err(|_| WebFailure::InvalidArgument("inflate_failed".into()))?
        } else {
            return Err(WebFailure::UnsupportedEncoding(encoding.to_owned()));
        };
        if decoded.len() > maximum {
            return Err(WebFailure::BodyLimitExceeded(format!(
                "{}/{}",
                decoded.len(),
                maximum
            )));
        }
        Ok(decoded)
    }

    pub fn body_decoded_text(&self, maximum: usize) -> Result<String, WebFailure> {
        let bytes = self.body_decoded_bytes(maximum)?;
        String::from_utf8(bytes).map_err(|_| WebFailure::InvalidArgument("invalid_utf8".into()))
    }

    #[must_use]
    pub fn peer_address(&self) -> Option<IpAddr> {
        self.peer
    }
}

#[derive(Clone, Debug, Default)]
pub struct Headers {
    pub entries: Vec<(String, String)>,
}

impl Headers {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, name: String, value: String) {
        self.entries.push((name, value));
    }

    pub fn set(&mut self, name: String, value: String) {
        if let Some((_, v)) = self
            .entries
            .iter_mut()
            .find(|(k, _)| k.eq_ignore_ascii_case(&name))
        {
            *v = value;
        } else {
            self.entries.push((name, value));
        }
    }

    #[must_use]
    pub fn count(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Clone, Debug, Default)]
pub struct Query {
    pub entries: Vec<(String, String)>,
}

impl Query {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, name: String, value: String) {
        self.entries.push((name, value));
    }

    #[must_use]
    pub fn encode(&self) -> String {
        let mut out = String::new();
        for (i, (k, v)) in self.entries.iter().enumerate() {
            if i > 0 {
                out.push('&');
            }
            out.push_str(&percent_encode(k));
            out.push('=');
            out.push_str(&percent_encode(v));
        }
        out
    }
}

#[derive(Clone, Debug, Default)]
pub struct Fields {
    pub entries: Vec<(String, String)>,
}

impl Fields {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, name: String, value: String) {
        self.entries.push((name, value));
    }

    #[must_use]
    pub fn encode(&self) -> String {
        let mut out = String::new();
        for (i, (k, v)) in self.entries.iter().enumerate() {
            if i > 0 {
                out.push('&');
            }
            out.push_str(&percent_encode_mode(k, true));
            out.push('=');
            out.push_str(&percent_encode_mode(v, true));
        }
        out
    }
}

#[derive(Clone, Debug, Default)]
pub struct Form {
    pub fields: Vec<(String, String)>,
    pub files: Vec<(String, String, String)>,
}

impl Form {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_field(&mut self, name: String, value: String) {
        self.fields.push((name, value));
    }

    pub fn add_file(&mut self, name: String, path: String, content_type: String) {
        self.files.push((name, path, content_type));
    }
}

fn percent_encode(input: &str) -> String {
    percent_encode_mode(input, false)
}

fn percent_encode_mode(input: &str, space_as_plus: bool) -> String {
    let mut out = String::new();
    for b in input.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' if space_as_plus => {
                out.push('+');
            }
            _ => {
                use std::fmt::Write;
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}
