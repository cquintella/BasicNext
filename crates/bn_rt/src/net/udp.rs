// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.Net` UDP operations shared by both backends: argument checks, the
//! socket call, and the `Error`. A socket that was closed is `None` here and
//! becomes `Net.CLOSED`.

use super::error::{Failure, NetError, checked_timeout};
use super::{Endpoint, UdpPacket, UdpSocket};

/// The largest datagram one `SendTo` or `Receive` moves (`bn_limits`).
#[must_use]
pub fn datagram_max() -> usize {
    bn_limits::web_limits().datagram_max_bytes
}

fn closed(operation: &'static str) -> NetError {
    NetError::new(operation, "use the UDP socket", Failure::Closed)
}

fn datagram_length(operation: &'static str, action: &str, length: i128) -> Result<usize, NetError> {
    let maximum = datagram_max();
    usize::try_from(length)
        .ok()
        .filter(|length| *length <= maximum)
        .ok_or_else(|| {
            NetError::new(
                operation,
                action,
                Failure::InvalidArgument(format!(
                    "a datagram holds 0..{maximum} bytes; got {length}"
                )),
            )
        })
}

/// `HOST.Net.UDPBind(endpoint)`.
///
/// # Errors
///
/// The endpoint is in use, the operating system refuses it, or another
/// operating-system failure.
pub fn udp_bind(endpoint: Endpoint) -> Result<UdpSocket, NetError> {
    UdpSocket::bind(endpoint).map_err(|error| {
        NetError::new(
            "HOST.Net.UDPBind",
            format!("bind {endpoint}"),
            Failure::Io(error),
        )
    })
}

/// `HOST.Net.UDPSocket.SendTo(endpoint, buffer, count)`: the bytes sent.
///
/// # Errors
///
/// A closed socket, a datagram over the bound, a broadcast or multicast
/// destination, or an operating-system failure.
pub fn udp_send_to(
    socket: Option<&UdpSocket>,
    endpoint: Endpoint,
    bytes: &[u8],
) -> Result<usize, NetError> {
    const OPERATION: &str = "HOST.Net.UDPSocket.SendTo";
    let action = format!("send a datagram to {endpoint}");
    let socket = socket.ok_or_else(|| closed(OPERATION))?;
    datagram_length(
        OPERATION,
        &action,
        i128::try_from(bytes.len()).unwrap_or(i128::MAX),
    )?;
    socket
        .send_to(endpoint, bytes)
        .map_err(|error| NetError::new(OPERATION, action, Failure::Io(error)))
}

/// `HOST.Net.UDPSocket.Receive(maximum, timeout)`: one datagram, cut to
/// `maximum` bytes and flagged when it was longer.
///
/// # Errors
///
/// A closed socket, `maximum` outside `1..datagram_max()`, a timeout
/// outside `1..60000` ms, no datagram in time, or an operating-system
/// failure.
pub fn udp_receive(
    socket: Option<&UdpSocket>,
    maximum: i128,
    timeout_ms: i128,
) -> Result<UdpPacket, NetError> {
    const OPERATION: &str = "HOST.Net.UDPSocket.Receive";
    let action = "receive a datagram";
    let socket = socket.ok_or_else(|| closed(OPERATION))?;
    let maximum = datagram_length(OPERATION, action, maximum).and_then(|maximum| {
        if maximum == 0 {
            Err(NetError::new(
                OPERATION,
                action,
                Failure::InvalidArgument("a receive takes at least 1 byte; got 0".into()),
            ))
        } else {
            Ok(maximum)
        }
    })?;
    let timeout = checked_timeout(OPERATION, action, timeout_ms)?;
    socket
        .set_read_timeout(Some(timeout))
        .and_then(|()| socket.receive(maximum))
        .map_err(|error| {
            let failure = if matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) {
                Failure::Timeout(u64::try_from(timeout_ms).unwrap_or(0))
            } else {
                Failure::Io(error)
            };
            NetError::new(OPERATION, action, failure)
        })
}

/// `HOST.Net.UDPSocket.LocalEndpoint`.
///
/// # Errors
///
/// A closed socket or an operating-system failure.
pub fn udp_local_endpoint(socket: Option<&UdpSocket>) -> Result<Endpoint, NetError> {
    const OPERATION: &str = "HOST.Net.UDPSocket.LocalEndpoint";
    socket
        .ok_or_else(|| closed(OPERATION))?
        .local_endpoint()
        .map_err(|error| NetError::new(OPERATION, "read the socket's endpoint", Failure::Io(error)))
}

#[cfg(test)]
mod tests {
    use super::{datagram_max, udp_bind, udp_receive, udp_send_to};
    use crate::net::{Address, Endpoint};
    use bn_types::error_codes::net;

    #[test]
    fn udp_errors_carry_portable_codes() {
        let loopback = Address::parse("127.0.0.1").unwrap();
        let socket = udp_bind(Endpoint::new(loopback, 0)).unwrap();
        let taken = socket.local_endpoint().unwrap();
        assert_eq!(udp_bind(taken).unwrap_err().code(), net::ADDRESS_IN_USE);
        let error = udp_receive(Some(&socket), 16, 1).unwrap_err();
        assert_eq!(error.code(), net::TIMEOUT);
        assert_eq!(error.operation(), "HOST.Net.UDPSocket.Receive");
        let error = udp_receive(Some(&socket), 0, 100).unwrap_err();
        assert_eq!(error.code(), net::INVALID_ARGUMENT);
        assert_eq!(error.cause(), "a receive takes at least 1 byte; got 0");
        let error = udp_receive(Some(&socket), 16, 0).unwrap_err();
        assert_eq!(error.code(), net::INVALID_ARGUMENT);
        let oversized = vec![0; datagram_max() + 1];
        let error = udp_send_to(Some(&socket), taken, &oversized).unwrap_err();
        assert_eq!(error.code(), net::INVALID_ARGUMENT);
        let broadcast = Endpoint::new(Address::parse("255.255.255.255").unwrap(), 9);
        let error = udp_send_to(Some(&socket), broadcast, b"x").unwrap_err();
        assert_eq!(error.code(), net::PERMISSION_DENIED);
        let error = udp_receive(None, 16, 100).unwrap_err();
        assert_eq!(error.code(), net::CLOSED);
        assert_eq!(error.cause(), "it was closed by Close");
    }
}
