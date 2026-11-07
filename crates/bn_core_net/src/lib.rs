// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.Net` core shared by the interpreter (`bn_host_net`) and compiled
//! binaries (`bn_rt` `net_abi`): address/endpoint types, bounded
//! resolve/reverse, ICMP ping, trusted-path ARP/NDP neighbor lookup, sockets.
//! Moved out of `bn_rt` so the `BNWeb` core can use it.

mod addressing;
mod cidr;
pub mod error;
mod icmp;
mod lookup;
mod lookup_pool;
mod neighbor;
mod reverse;
mod socket;
mod tcp;
mod udp;

pub use socket::{TcpListener, TcpStream, UdpPacket, UdpSocket};
pub use tcp::{
    listener_endpoint, quota_exceeded, tcp_accept, tcp_connect, tcp_endpoint, tcp_listen, tcp_read,
    tcp_set_timeouts, tcp_shutdown, tcp_write,
};
pub use udp::{datagram_max, udp_bind, udp_local_endpoint, udp_receive, udp_send_to};

use std::{net::IpAddr, str::FromStr};

pub use addressing::{parse_address, parse_cidr};
pub use cidr::Cidr;
pub use icmp::{PingError, PingReply, ping};
pub use lookup::{address_index, neighbor_of, ping_address, resolve_host, reverse_lookup};
pub use lookup_pool::{LOOKUP_WORKERS, LOOKUPS_IN_FLIGHT_MAX, LookupPool, Refusal};
pub use neighbor::{NeighborError, neighbor};
pub use reverse::{ReverseError, reverse_timeout};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Endpoint {
    address: Address,
    port: u16,
}

impl std::fmt::Display for Endpoint {
    /// `127.0.0.1:80`, `[::1]:80`.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::net::SocketAddr::new(self.address.as_std(), self.port).fmt(formatter)
    }
}

impl Endpoint {
    #[must_use]
    pub const fn new(address: Address, port: u16) -> Self {
        Self { address, port }
    }

    #[must_use]
    pub const fn address(self) -> Address {
        self.address
    }

    #[must_use]
    pub const fn port(self) -> u16 {
        self.port
    }
}

/// Opaque compiled-runtime result for HOST.Net.Resolve.
///
/// The LLVM ABI owns this allocation until `bn_rt_net_addresses_free` is called.
#[derive(Debug)]
pub struct AddressesHandle {
    values: Vec<Address>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Address(IpAddr);

impl Address {
    /// Parses a strict IPv4 or IPv6 address.
    ///
    /// # Errors
    ///
    /// Returns the parser error when `text` is not an address.
    pub fn parse(text: &str) -> Result<Self, std::net::AddrParseError> {
        text.parse().map(Self)
    }

    #[must_use]
    pub(crate) const fn from_ip(address: IpAddr) -> Self {
        Self(address)
    }

    #[must_use]
    pub const fn as_std(self) -> IpAddr {
        self.0
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for Address {
    type Err = std::net::AddrParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

/// Resolves a host name to unique addresses, in resolver order. The port is
/// part of the `BNWeb` resolver contract and does not change the addresses.
///
/// # Errors
///
/// `InvalidInput` for an empty name or one with a space or control character,
/// `NotFound` when the name has no address, or the resolver's failure.
pub fn resolve(host: &str, _port: u16, maximum: usize) -> std::io::Result<Vec<Address>> {
    if host.is_empty()
        || host.len() > 253
        || host
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            bn_catalog::text("net", "resolve.host_name.cause"),
        ));
    }
    if maximum == 0 {
        return Ok(Vec::new());
    }
    // `getaddrinfo` through `dns_lookup` keeps the resolver's error kind: a
    // name without records is `NotFound`, not a generic I/O failure.
    let found =
        dns_lookup::getaddrinfo(Some(host), None, None).map_err(|error| match error.kind() {
            dns_lookup::LookupErrorKind::NoName | dns_lookup::LookupErrorKind::NoData => {
                std::io::Error::new(std::io::ErrorKind::NotFound, std::io::Error::from(error))
            }
            _ => std::io::Error::from(error),
        })?;
    let mut seen = std::collections::HashSet::new();
    let mut addresses = Vec::new();
    for entry in found {
        let address = Address(entry?.sockaddr.ip());
        if seen.insert(address) {
            addresses.push(address);
            if addresses.len() == maximum {
                break;
            }
        }
    }
    Ok(addresses)
}

/// Resolves on the shared lookup workers with a timeout; `Ok(None)` means the
/// timeout elapsed (the lookup is cancelled if it has not started, and its
/// late answer is dropped if it has).
///
/// # Errors
///
/// The OS resolution error; `QuotaExceeded` when [`LOOKUPS_IN_FLIGHT_MAX`]
/// lookups are already in flight, `Other` when the lookup died.
pub fn resolve_timeout(
    host: &str,
    port: u16,
    maximum: usize,
    timeout: std::time::Duration,
) -> std::io::Result<Option<Vec<Address>>> {
    let pool = LookupPool::global().map_err(|refusal| refusal_error(&refusal))?;
    resolve_timeout_in(pool, host, port, maximum, timeout)
}

/// [`resolve_timeout`] on a given pool.
fn resolve_timeout_in(
    pool: &LookupPool,
    host: &str,
    port: u16,
    maximum: usize,
    timeout: std::time::Duration,
) -> std::io::Result<Option<Vec<Address>>> {
    let host = host.to_owned();
    match pool.run(timeout, move || resolve(&host, port, maximum)) {
        Ok(Some(result)) => result.map(Some),
        Ok(None) => Ok(None),
        Err(Refusal::Full { .. }) => Err(std::io::Error::from(std::io::ErrorKind::QuotaExceeded)),
        Err(refusal @ (Refusal::Lost | Refusal::Unavailable)) => Err(refusal_error(&refusal)),
    }
}

/// The I/O error a refused lookup reports.
fn refusal_error(refusal: &Refusal) -> std::io::Error {
    match refusal {
        Refusal::Full { .. } => std::io::Error::from(std::io::ErrorKind::QuotaExceeded),
        Refusal::Lost => {
            std::io::Error::other(bn_catalog::text("net", "resolve.worker_lost.cause"))
        }
        Refusal::Unavailable => {
            std::io::Error::other(bn_catalog::text("net", "resolve.workers_unavailable.cause"))
        }
    }
}

impl AddressesHandle {
    /// Resolves at most `maximum` unique addresses.
    ///
    /// # Errors
    ///
    /// Returns the operating-system resolver error.
    pub fn resolve(host: &str, port: u16, maximum: usize) -> std::io::Result<Self> {
        Ok(Self {
            values: resolve(host, port, maximum)?,
        })
    }

    /// Resolves at most `maximum` addresses before the timeout expires.
    ///
    /// # Errors
    ///
    /// Returns the resolver error; `Ok(None)` indicates timeout.
    pub fn resolve_timeout(
        host: &str,
        port: u16,
        maximum: usize,
        timeout: std::time::Duration,
    ) -> std::io::Result<Option<Self>> {
        Ok(resolve_timeout(host, port, maximum, timeout)?.map(|values| Self { values }))
    }

    /// A collection of already resolved addresses.
    #[must_use]
    pub const fn from_addresses(values: Vec<Address>) -> Self {
        Self { values }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    #[must_use]
    pub fn get(&self, index: usize) -> Option<Address> {
        self.values.get(index).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::{AddressesHandle, LookupPool, resolve_timeout_in};
    use std::time::Duration;

    #[test]
    fn resolve_handle_honors_zero_bound_without_network_access() {
        let handle = AddressesHandle::resolve("invalid.invalid", 80, 0)
            .expect("zero bound must not resolve");
        assert!(handle.is_empty());
        assert_eq!(handle.len(), 0);
        assert_eq!(handle.get(0), None);
    }

    #[test]
    fn a_full_pool_is_a_quota_error_not_a_thread() {
        let pool = LookupPool::new(1, 0).expect("start the pool");
        let error = resolve_timeout_in(&pool, "localhost", 0, 1, Duration::from_millis(1))
            .expect_err("no place in the pool");
        assert_eq!(error.kind(), std::io::ErrorKind::QuotaExceeded);
    }
}
