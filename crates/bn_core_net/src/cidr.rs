// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.Net.CIDR`: prefix parsing, canonical network, and containment,
//! shared by both backends.

use std::net::IpAddr;

use super::Address;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cidr {
    network: IpAddr,
    prefix: u8,
}

impl Cidr {
    /// Parses an address/prefix pair under the strict rule of `host-net.md`,
    /// "CIDR text": a strict address, a prefix of decimal digits with no sign
    /// and no leading zero, and no bit set past the prefix.
    ///
    /// # Errors
    ///
    /// Returns an error when the separator, address, or prefix is invalid,
    /// or when the address is not the network address.
    pub fn parse(text: &str) -> Result<Self, &'static str> {
        let (address, prefix) = text.split_once('/').ok_or("CIDR requires '/'")?;
        let address = Address::parse(address)
            .map_err(|_| "invalid address")?
            .as_std();
        let canonical = !prefix.is_empty()
            && prefix.bytes().all(|byte| byte.is_ascii_digit())
            && (prefix == "0" || !prefix.starts_with('0'));
        let prefix = prefix
            .parse::<u8>()
            .ok()
            .filter(|_| canonical)
            .ok_or("invalid prefix")?;
        let maximum = match address {
            IpAddr::V4(_) => 32,
            IpAddr::V6(_) => 128,
        };
        if prefix > maximum {
            return Err("prefix is wider than address family");
        }
        let network = mask(address, prefix);
        if network != address {
            return Err(bn_catalog::text("net", "cidr.host_bits"));
        }
        Ok(Self { network, prefix })
    }

    #[must_use]
    pub const fn network(self) -> IpAddr {
        self.network
    }

    #[must_use]
    pub const fn prefix_length(self) -> u8 {
        self.prefix
    }

    #[must_use]
    pub fn contains(self, address: Address) -> bool {
        self.network == mask(address.as_std(), self.prefix)
    }
}

fn mask(address: IpAddr, prefix: u8) -> IpAddr {
    match address {
        IpAddr::V4(value) => IpAddr::V4(std::net::Ipv4Addr::from(
            u32::from(value)
                & if prefix == 0 {
                    0
                } else {
                    !0u32 << (32 - u32::from(prefix))
                },
        )),
        IpAddr::V6(value) => IpAddr::V6(std::net::Ipv6Addr::from(
            u128::from(value)
                & if prefix == 0 {
                    0
                } else {
                    !0u128 << (128 - u32::from(prefix))
                },
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{Address, Cidr};

    #[test]
    fn canonicalizes_and_contains_ipv4() {
        let cidr = Cidr::parse("192.168.1.0/24").expect("CIDR");
        assert_eq!(cidr.network().to_string(), "192.168.1.0");
        assert!(cidr.contains(Address::parse("192.168.1.200").expect("address")));
        assert!(!cidr.contains(Address::parse("192.168.2.1").expect("address")));
    }

    #[test]
    fn canonicalizes_ipv6_and_rejects_invalid_prefix() {
        let cidr = Cidr::parse("2001:db8::/64").expect("CIDR");
        assert_eq!(cidr.network().to_string(), "2001:db8::");
        assert!(Cidr::parse("10.0.0.1/33").is_err());
    }
}

/// `host-net.md`, "CIDR text" (Carlos, 2026-10-10; K39): a prefix of
/// decimal digits with no sign or leading zero, and the network address.
#[cfg(test)]
mod strict_text_tests {
    use super::Cidr;

    #[test]
    fn the_prefix_is_plain_decimal_and_the_address_is_the_network() {
        for accepted in [
            "10.0.0.0/8",
            "0.0.0.0/0",
            "192.168.1.0/24",
            "2001:db8::/32",
            "::/0",
        ] {
            assert!(Cidr::parse(accepted).is_ok(), "{accepted}");
        }
        for refused in [
            "10.0.0.0/08",
            "10.0.0.0/008",
            "10.0.0.0/+8",
            "10.0.0.0/ 8",
            "10.0.0.0/8 ",
            "10.0.0.0/",
            "10.0.0.0/00",
            "2001:db8::/032",
            "10.0.0.1/8",
            "192.168.1.9/24",
            "2001:db8::1/64",
            "10.0.0.0/33",
        ] {
            assert!(Cidr::parse(refused).is_err(), "{refused}");
        }
        assert_eq!(
            Cidr::parse("10.0.0.1/8"),
            Err(bn_catalog::text("net", "cidr.host_bits"))
        );
    }
}
