#![allow(dead_code)] // ponytail: parser/route core lands before HTTP provider wiring.

use std::{collections::HashMap, net::IpAddr};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Target {
    pub path: String,
    pub query: String,
}

#[derive(Clone, Debug)]
pub(crate) struct Request {
    pub method: String,
    pub target: Target,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub peer: IpAddr,
}

impl Request {
    pub(crate) fn new(
        method: &str,
        raw_target: &str,
        headers: Vec<(String, String)>,
        body: &str,
        peer: IpAddr,
    ) -> Result<Self, &'static str> {
        if method.is_empty()
            || method.bytes().any(|byte| {
                !byte.is_ascii_uppercase() && !byte.is_ascii_digit() && !b"-".contains(&byte)
            })
        {
            return Err("invalid request method");
        }
        let target = canonical_target(raw_target)?;
        validate_headers(&headers)?;
        bounded_body(body, MAX_RESPONSE_BODY as i128)?;
        Ok(Self {
            method: method.into(),
            target,
            headers,
            body: body.into(),
            peer,
        })
    }
    pub(crate) fn query(&self, name: &str) -> Result<QueryValues, &'static str> {
        QueryValues::from_query(&self.target.query, name)
    }
    pub(crate) fn header(&self, name: &str) -> Result<HeaderValues, &'static str> {
        HeaderValues::from_headers(&self.headers, name)
    }
    pub(crate) fn method(&self) -> &str {
        &self.method
    }
    pub(crate) fn target(&self) -> &str {
        &self.target.path
    }
    pub(crate) fn body(&self, maximum: i128) -> Result<&str, &'static str> {
        bounded_body(&self.body, maximum)
    }
    pub(crate) fn peer_address(&self) -> IpAddr {
        self.peer
    }
    pub(crate) fn effective_client_address(&self, trusted_proxy: bool) -> IpAddr {
        let forwarded = self
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("x-forwarded-for"))
            .map(|(_, value)| value.as_str());
        effective_client_address(self.peer, forwarded, trusted_proxy)
    }
}

const MAX_RESPONSE_BODY: usize = 8 * 1024 * 1024;
const MAX_HEADER_BYTES: usize = 64 * 1024;
const MAX_HEADER_FIELDS: usize = 100;
const MAX_TARGET_BYTES: usize = 8 * 1024;

#[derive(Debug, Default)]
pub(crate) struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub committed: bool,
    closed: bool,
}

impl Response {
    pub(crate) fn new() -> Self {
        Self {
            status: 200,
            ..Self::default()
        }
    }
    pub(crate) fn set_status(&mut self, status: u16) -> Result<(), &'static str> {
        if self.committed || self.closed {
            return Err("response is already committed or closed");
        }
        if !(100..=599).contains(&status) {
            return Err("invalid response status");
        }
        self.status = status;
        Ok(())
    }
    pub(crate) fn set_header(&mut self, name: &str, value: &str) -> Result<(), &'static str> {
        if self.committed || self.closed {
            return Err("response is already committed or closed");
        }
        if name.is_empty()
            || name
                .bytes()
                .any(|byte| byte <= 0x20 || byte == 0x7f || byte == b':')
            || value.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
        {
            return Err("invalid response header");
        }
        if self
            .headers
            .iter()
            .map(|(key, value)| key.len() + value.len())
            .sum::<usize>()
            + name.len()
            + value.len()
            > MAX_HEADER_BYTES
        {
            return Err("response headers exceed 64 KiB");
        }
        self.headers.push((name.to_ascii_lowercase(), value.into()));
        Ok(())
    }
    pub(crate) fn write(&mut self, body: &str) -> Result<(), &'static str> {
        if self.committed || self.closed {
            return Err("response is already committed or closed");
        }
        if self.body.len() + body.len() > MAX_RESPONSE_BODY {
            return Err("response body exceeds 8 MiB");
        }
        self.body.push_str(body);
        Ok(())
    }
    pub(crate) fn commit(&mut self) -> Result<(), &'static str> {
        if self.closed {
            return Err("response is closed");
        }
        self.committed = true;
        Ok(())
    }
    pub(crate) fn is_committed(&self) -> bool {
        self.committed
    }
    pub(crate) fn close(&mut self) {
        self.closed = true;
    }
    pub(crate) fn finish_for_method(&mut self, method: &str) -> Result<(), &'static str> {
        if method == "HEAD" {
            self.body.clear();
        }
        self.commit()
    }
}

pub(crate) fn canonical_target(raw: &str) -> Result<Target, &'static str> {
    if raw.is_empty()
        || raw.len() > MAX_TARGET_BYTES
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

pub(crate) fn query_values(query: &str, name: &str) -> Result<Vec<String>, &'static str> {
    if query.len() > MAX_TARGET_BYTES {
        return Err("query exceeds 8 KiB");
    }
    let mut values = Vec::new();
    let mut field_count = 0;
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        field_count += 1;
        if pair.len() > 8 * 1024 || field_count > 100 {
            return Err("query fields exceed bounds");
        }
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let key = decode_component(key)?;
        let value = decode_component(value)?;
        if key.len() > 1024 || value.len() > 8 * 1024 {
            return Err("query key or value exceeds bounds");
        }
        if key == name {
            values.push(value);
        }
    }
    Ok(values)
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct QueryValues(Vec<String>);

impl QueryValues {
    pub(crate) fn from_query(query: &str, name: &str) -> Result<Self, &'static str> {
        Ok(Self(query_values(query, name)?))
    }
    pub(crate) fn count(&self) -> usize {
        self.0.len()
    }
    pub(crate) fn get(&self, index: usize) -> Option<&str> {
        self.0.get(index).map(String::as_str)
    }
}

pub(crate) fn header_values(
    headers: &[(String, String)],
    name: &str,
) -> Result<Vec<String>, &'static str> {
    if name.is_empty()
        || name.len() > 128
        || name
            .bytes()
            .any(|byte| byte <= 0x20 || byte == 0x7f || byte == b':')
    {
        return Err("invalid header name");
    }
    let wanted = name.to_ascii_lowercase();
    Ok(headers
        .iter()
        .filter(|(key, _)| key.eq_ignore_ascii_case(&wanted))
        .map(|(_, value)| value.clone())
        .collect())
}

fn validate_headers(headers: &[(String, String)]) -> Result<(), &'static str> {
    if headers.len() > MAX_HEADER_FIELDS
        || headers
            .iter()
            .map(|(name, value)| name.len() + value.len())
            .sum::<usize>()
            > MAX_HEADER_BYTES
    {
        return Err("request headers exceed bounds");
    }
    if headers.iter().any(|(name, value)| {
        name.is_empty()
            || name
                .bytes()
                .any(|byte| byte <= 0x20 || byte == 0x7f || byte == b':')
            || value.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
    }) {
        return Err("invalid request header");
    }
    Ok(())
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct HeaderValues(Vec<String>);

impl HeaderValues {
    pub(crate) fn from_headers(
        headers: &[(String, String)],
        name: &str,
    ) -> Result<Self, &'static str> {
        Ok(Self(header_values(headers, name)?))
    }
    pub(crate) fn count(&self) -> usize {
        self.0.len()
    }
    pub(crate) fn get(&self, index: usize) -> Option<&str> {
        self.0.get(index).map(String::as_str)
    }
}

pub(crate) fn validate_client_url(url: &str) -> Result<(), &'static str> {
    let (scheme, rest) = url.split_once("://").ok_or("URL requires an authority")?;
    if !matches!(scheme, "http" | "https") || rest.is_empty() || rest.contains('#') {
        return Err("unsupported or ambiguous URL");
    }
    let authority = rest.split(['/', '?']).next().unwrap_or_default();
    if authority.is_empty()
        || authority.contains('@')
        || authority.bytes().any(|byte| byte < 0x21 || byte == 0x7f)
    {
        return Err("invalid URL authority");
    }
    Ok(())
}

pub(crate) fn bounded_body(body: &str, maximum: i128) -> Result<&str, &'static str> {
    if maximum < 0 || maximum > MAX_RESPONSE_BODY as i128 {
        return Err("body limit exceeds 8 MiB");
    }
    let maximum = usize::try_from(maximum).map_err(|_| "body limit is invalid")?;
    if body.len() > maximum {
        return Err("request body exceeds declared limit");
    }
    Ok(body)
}

pub(crate) fn validate_ssrf_destinations(
    addresses: &[IpAddr],
    allow_private: bool,
) -> Result<(), &'static str> {
    if allow_private {
        return Ok(());
    }
    if addresses.is_empty() {
        return Err("URL resolved to no addresses");
    }
    if addresses.iter().any(|address| match address {
        IpAddr::V4(address) => {
            address.is_private()
                || address.is_loopback()
                || address.is_link_local()
                || address.is_multicast()
                || address.is_unspecified()
        }
        IpAddr::V6(address) => {
            address.is_loopback()
                || address.is_unicast_link_local()
                || address.is_multicast()
                || address.is_unspecified()
                || address.segments()[0] & 0xfe00 == 0xfc00
        }
    }) {
        return Err("private or local destination requires explicit opt-in");
    }
    Ok(())
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

mod routing;
pub(crate) use routing::{Route, RouteOutcome, dispatch_route, valid_method, valid_route_pattern};
#[cfg(test)]
pub(crate) use routing::{allowed_methods, route_for_request, select_route};
mod server;
pub(crate) use server::ServerState;

#[cfg(test)]
mod tests;
