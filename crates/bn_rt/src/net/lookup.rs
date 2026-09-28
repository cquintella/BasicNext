// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.Net` lookups shared by both backends — `Ping`, `Neighbor`,
//! `Reverse`, `Resolve` — with timeout validation and their `Error`s.

use std::time::Duration;

use super::error::{Failure, NetError, checked_timeout};
use super::{Address, NeighborError, PingError, PingReply, ReverseError};

/// `HOST.Net.Ping(address, timeout)`.
///
/// # Errors
///
/// An invalid timeout, no reply in time, an unreachable destination, missing
/// ICMP permission, or an operating-system failure.
pub fn ping_address(address: Address, timeout_ms: i128) -> Result<PingReply, NetError> {
    const OPERATION: &str = "HOST.Net.Ping";
    let action = format!("ping {address}");
    let timeout = checked_timeout(OPERATION, &action, timeout_ms)?;
    super::ping(address, timeout).map_err(|error| {
        let failure = match error {
            PingError::Timeout => Failure::Timeout(millis(timeout)),
            PingError::Unreachable => {
                Failure::Unreachable(format!("{address} reported the destination unreachable"))
            }
            PingError::PermissionDenied => Failure::PermissionDenied(
                "the host does not permit ICMP sockets for this process".into(),
            ),
            PingError::Unavailable => {
                Failure::Unavailable("this host does not provide ICMP Echo".into())
            }
            PingError::Io(error) => Failure::Io(error),
        };
        NetError::new(OPERATION, action, failure)
    })
}

/// `HOST.Net.Neighbor(address)`.
///
/// # Errors
///
/// No complete entry, or a host without a readable neighbor table.
pub fn neighbor_of(address: Address) -> Result<Address, NetError> {
    super::neighbor(address).map_err(|error| {
        let failure = match error {
            NeighborError::NotFound => Failure::NotFound(format!(
                "the neighbor table has no complete entry for {address}"
            )),
            NeighborError::Unsupported => {
                Failure::Unavailable("this host does not expose its neighbor table".into())
            }
        };
        NetError::new(
            "HOST.Net.Neighbor",
            format!("look up the neighbor {address}"),
            failure,
        )
    })
}

/// `HOST.Net.Reverse(address, timeout)`.
///
/// # Errors
///
/// An invalid timeout, no answer in time, no name, or a resolver failure.
pub fn reverse_lookup(address: Address, timeout_ms: i128) -> Result<String, NetError> {
    const OPERATION: &str = "HOST.Net.Reverse";
    let action = format!("find the name of {address}");
    let timeout = checked_timeout(OPERATION, &action, timeout_ms)?;
    super::reverse_timeout(address, timeout).map_err(|error| {
        let failure = match error {
            ReverseError::Timeout => Failure::Timeout(millis(timeout)),
            ReverseError::NotFound => {
                Failure::NotFound(format!("no name is registered for {address}"))
            }
            ReverseError::Io(error) => Failure::Io(error),
        };
        NetError::new(OPERATION, action, failure)
    })
}

/// `HOST.Net.Resolve(host, timeout)`: at most `maximum` addresses, in
/// provider order, without duplicates; none is an empty result.
///
/// # Errors
///
/// An invalid timeout, no answer in time, or a resolver failure.
pub fn resolve_host(
    host: &str,
    timeout_ms: i128,
    maximum: usize,
) -> Result<Vec<Address>, NetError> {
    const OPERATION: &str = "HOST.Net.Resolve";
    let action = format!("resolve \"{host}\"");
    let timeout = checked_timeout(OPERATION, &action, timeout_ms)?;
    match super::resolve_timeout(host, 0, maximum, timeout) {
        Ok(Some(addresses)) => Ok(addresses),
        Ok(None) => Err(NetError::new(
            OPERATION,
            action,
            Failure::Timeout(millis(timeout)),
        )),
        Err(error) => Err(NetError::new(OPERATION, action, Failure::Io(error))),
    }
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::{ping_address, resolve_host};
    use crate::net::Address;
    use bn_types::error_codes::net;

    #[test]
    fn timeouts_outside_the_bound_are_invalid_arguments() {
        let address = Address::parse("127.0.0.1").unwrap();
        let error = ping_address(address, 0).unwrap_err();
        assert_eq!(error.code(), net::INVALID_ARGUMENT);
        assert_eq!(error.message(), "cannot ping 127.0.0.1");
        assert_eq!(
            error.cause(),
            "the timeout must be within 1..60000 ms; got 0"
        );
        let error = resolve_host("localhost", 60_001, 4).unwrap_err();
        assert_eq!(error.operation(), "HOST.Net.Resolve");
        assert_eq!(error.message(), "cannot resolve \"localhost\"");
    }
}
