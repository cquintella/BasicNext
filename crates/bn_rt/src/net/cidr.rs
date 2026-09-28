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
    /// Parses and canonicalizes an address/prefix pair.
    ///
    /// # Errors
    ///
    /// Returns an error when the separator, address, or prefix is invalid.
    pub fn parse(text: &str) -> Result<Self, &'static str> {
        let (address, prefix) = text.split_once('/').ok_or("CIDR requires '/'")?;
        let address = Address::parse(address)
            .map_err(|_| "invalid address")?
            .as_std();
        let prefix = prefix.parse::<u8>().map_err(|_| "invalid prefix")?;
        let maximum = match address {
            IpAddr::V4(_) => 32,
            IpAddr::V6(_) => 128,
        };
        if prefix > maximum {
            return Err("prefix is wider than address family");
        }
        Ok(Self {
            network: mask(address, prefix),
            prefix,
        })
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
        let cidr = Cidr::parse("192.168.1.9/24").expect("CIDR");
        assert_eq!(cidr.network().to_string(), "192.168.1.0");
        assert!(cidr.contains(Address::parse("192.168.1.200").expect("address")));
        assert!(!cidr.contains(Address::parse("192.168.2.1").expect("address")));
    }

    #[test]
    fn canonicalizes_ipv6_and_rejects_invalid_prefix() {
        let cidr = Cidr::parse("2001:db8::1/64").expect("CIDR");
        assert_eq!(cidr.network().to_string(), "2001:db8::");
        assert!(Cidr::parse("10.0.0.1/33").is_err());
    }
}
