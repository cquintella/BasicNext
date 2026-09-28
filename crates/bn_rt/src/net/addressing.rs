// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.Net` addressing shared by both backends: `Address.Parse`, the
//! address predicates, and `CIDR.Parse`, with their `Error`s.

use std::net::IpAddr;

use super::error::{Failure, NetError};
use super::{Address, Cidr};

/// `HOST.Net.Address.Parse`.
///
/// # Errors
///
/// `Net.INVALID_ARGUMENT` when `text` is not an IPv4 or IPv6 address.
pub fn parse_address(text: &str) -> Result<Address, NetError> {
    Address::parse(text).map_err(|_| {
        NetError::new(
            "HOST.Net.Address.Parse",
            format!("parse \"{text}\" as an IP address"),
            Failure::InvalidArgument(
                "an address is IPv4 (four decimal octets, 192.0.2.1) or IPv6 \
                 (hexadecimal groups, 2001:db8::1), without a port or a name"
                    .into(),
            ),
        )
    })
}

/// `HOST.Net.CIDR.Parse`.
///
/// # Errors
///
/// `Net.INVALID_ARGUMENT` naming the part of `text` that is wrong.
pub fn parse_cidr(text: &str) -> Result<Cidr, NetError> {
    Cidr::parse(text).map_err(|reason| {
        let cause = match reason {
            "CIDR requires '/'" => {
                "a CIDR prefix is an address, '/', and a prefix length (10.0.0.0/8)".into()
            }
            "invalid address" => "the part before '/' is not an IPv4 or IPv6 address".into(),
            "invalid prefix" => "the part after '/' is not a prefix length (0..128)".into(),
            _ => {
                let maximum = if text.contains(':') { 128 } else { 32 };
                format!("the prefix length is wider than the {maximum} bits of the address")
            }
        };
        NetError::new(
            "HOST.Net.CIDR.Parse",
            format!("parse \"{text}\" as a CIDR prefix"),
            Failure::InvalidArgument(cause),
        )
    })
}

impl Address {
    /// `IsLoopback`: `127.0.0.0/8`, `::1`, and IPv4-mapped loopback.
    #[must_use]
    pub fn is_loopback(self) -> bool {
        match self.as_std() {
            IpAddr::V4(value) => value.is_loopback(),
            IpAddr::V6(value) => {
                value.is_loopback() || value.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
            }
        }
    }

    /// `IsPrivate`: RFC 1918, IPv6 unique local, and IPv4-mapped private.
    #[must_use]
    pub fn is_private(self) -> bool {
        match self.as_std() {
            IpAddr::V4(value) => value.is_private(),
            IpAddr::V6(value) => {
                value.is_unique_local() || value.to_ipv4_mapped().is_some_and(|v4| v4.is_private())
            }
        }
    }

    /// `IsLinkLocal`: `169.254.0.0/16` and `fe80::/10`.
    #[must_use]
    pub fn is_link_local(self) -> bool {
        match self.as_std() {
            IpAddr::V4(value) => value.is_link_local(),
            IpAddr::V6(value) => value.is_unicast_link_local(),
        }
    }

    /// `IsMulticast`.
    #[must_use]
    pub fn is_multicast(self) -> bool {
        self.as_std().is_multicast()
    }

    /// `IsIPv4`.
    #[must_use]
    pub fn is_ipv4(self) -> bool {
        self.as_std().is_ipv4()
    }

    /// `IsIPv6`.
    #[must_use]
    pub fn is_ipv6(self) -> bool {
        self.as_std().is_ipv6()
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_address, parse_cidr};
    use bn_types::error_codes::net;

    #[test]
    fn parse_errors_name_the_text_and_the_rule() {
        let error = parse_address("10.0.0.300").unwrap_err();
        assert_eq!(error.code(), net::INVALID_ARGUMENT);
        assert_eq!(error.operation(), "HOST.Net.Address.Parse");
        assert_eq!(
            error.message(),
            "cannot parse \"10.0.0.300\" as an IP address"
        );
        let error = parse_cidr("10.0.0.1/33").unwrap_err();
        assert_eq!(
            error.cause(),
            "the prefix length is wider than the 32 bits of the address"
        );
        assert!(parse_address("::ffff:127.0.0.1").unwrap().is_loopback());
        assert!(parse_address("fd00::1").unwrap().is_private());
    }
}
