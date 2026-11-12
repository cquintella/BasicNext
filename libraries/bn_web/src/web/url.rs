// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The URL of a client request, parsed once. [`ClientUrl::parse`] is the one
//! place a client URL is split into scheme, authority, host, port and target;
//! the client, the policy check and the redirect follow read its parts.

use crate::web_error::WebFailure;

/// The separator between a URL's scheme and its authority.
const SCHEME_SEPARATOR: &str = "://";
/// The default ports of the two client schemes.
const HTTP_PORT: u16 = 80;
const HTTPS_PORT: u16 = 443;

fn invalid(key: &str) -> WebFailure {
    WebFailure::InvalidUrl(bn_catalog::text("web", key).into())
}

/// `http` or `https`: the only schemes a client sends to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scheme {
    Http,
    Https,
}

impl Scheme {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }

    const fn default_port(self) -> u16 {
        match self {
            Self::Http => HTTP_PORT,
            Self::Https => HTTPS_PORT,
        }
    }
}

/// A client URL: `scheme://authority` and an origin-form target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientUrl {
    scheme: Scheme,
    authority: String,
    host: String,
    port: u16,
    target: String,
}

impl ClientUrl {
    /// Parses an absolute `http` or `https` URL with no fragment and no
    /// userinfo.
    ///
    /// # Errors
    ///
    /// `INVALID_ARGUMENT` (`WebFailure::InvalidUrl`) naming what is wrong.
    pub fn parse(url: &str) -> Result<Self, WebFailure> {
        let (scheme, rest) = url
            .split_once(SCHEME_SEPARATOR)
            .ok_or_else(|| invalid("client.url_authority_missing"))?;
        let scheme = match scheme {
            "http" => Scheme::Http,
            "https" => Scheme::Https,
            _ => return Err(invalid("client.url_scheme")),
        };
        if rest.is_empty() || rest.contains('#') {
            return Err(invalid("client.url_ambiguous"));
        }
        let authority_end = rest.find(['/', '?']).unwrap_or(rest.len());
        let (authority, target) = rest.split_at(authority_end);
        if authority.is_empty()
            || authority.contains('@')
            || authority.bytes().any(|byte| byte < 0x21 || byte == 0x7f)
        {
            return Err(invalid("client.url_authority"));
        }
        let (host, port) = parse_authority(authority, scheme.default_port())?;
        let target = match target.chars().next() {
            None => "/".to_owned(),
            Some('?') => format!("/{target}"),
            Some(_) => target.to_owned(),
        };
        Ok(Self {
            scheme,
            authority: authority.to_owned(),
            host,
            port,
            target,
        })
    }

    /// The URL a redirect's `Location` names, relative to this one.
    ///
    /// # Errors
    ///
    /// `INVALID_ARGUMENT` for a `Location` with a fragment or a line break, or
    /// one that resolves to an invalid URL.
    pub fn resolve(&self, location: &str) -> Result<Self, WebFailure> {
        if location.contains(['#', '\n', '\r']) {
            return Err(invalid("client.redirect_location"));
        }
        if location.starts_with("http://") || location.starts_with("https://") {
            return Self::parse(location);
        }
        let base_path = self.target.split('?').next().unwrap_or("/");
        let target = if location.starts_with('/') {
            location.to_owned()
        } else if location.starts_with('?') {
            format!("{base_path}{location}")
        } else {
            let directory = base_path
                .rsplit_once('/')
                .map_or("", |(directory, _)| directory);
            format!("{directory}/{location}")
        };
        Self::parse(&format!(
            "{}{SCHEME_SEPARATOR}{}{target}",
            self.scheme.as_str(),
            self.authority
        ))
    }

    #[must_use]
    pub const fn scheme(&self) -> Scheme {
        self.scheme
    }
    #[must_use]
    pub fn authority(&self) -> &str {
        &self.authority
    }
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }
    /// The origin-form target: a path, with its query.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }
}

impl std::fmt::Display for ClientUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{SCHEME_SEPARATOR}{}{}",
            self.scheme.as_str(),
            self.authority,
            self.target
        )
    }
}

/// `host[:port]` or `[ipv6][:port]`, with `default_port` when none is given.
///
/// # Errors
///
/// `INVALID_ARGUMENT` for an empty host, userinfo, a bad IPv6 literal or a
/// port outside `u16`.
pub fn parse_authority(authority: &str, default_port: u16) -> Result<(String, u16), WebFailure> {
    if authority.is_empty() || authority.contains('@') {
        return Err(invalid("client.url_authority"));
    }
    let port_of = |text: &str| text.parse::<u16>().map_err(|_| invalid("client.port"));
    if let Some(host) = authority.strip_prefix('[') {
        let close = host
            .find(']')
            .ok_or_else(|| invalid("client.ipv6_authority"))?;
        let (address, suffix) = host.split_at(close);
        let port = match suffix.get(1..).filter(|suffix| !suffix.is_empty()) {
            None => default_port,
            Some(suffix) => port_of(
                suffix
                    .strip_prefix(':')
                    .ok_or_else(|| invalid("client.ipv6_authority"))?,
            )?,
        };
        return Ok((address.to_owned(), port));
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, port_of(port)?),
        None => (authority, default_port),
    };
    if host.is_empty() {
        return Err(invalid("client.url_authority"));
    }
    Ok((host.to_owned(), port))
}
