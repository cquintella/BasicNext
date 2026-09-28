// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.Net` TCP operations shared by both backends: argument checks, the
//! socket call, and the `Error`. A stream or listener that was closed is
//! `None` here and becomes `Net.CLOSED`.

use std::net::Shutdown;

use super::error::{Failure, NetError, checked_timeout};
use super::{Endpoint, TcpListener, TcpStream};

/// The most bytes one `Read` or `Write` moves (1 MiB).
pub const TRANSFER_MAX: usize = 1_048_576;

/// `Net.LIMIT` for a new socket past the process quota (`bn_limits`).
#[must_use]
pub fn quota_exceeded(operation: &'static str, action: impl Into<String>) -> NetError {
    NetError::new(
        operation,
        action,
        Failure::Limit(format!(
            "the process already holds {} sockets, the quota",
            bn_limits::web_limits().socket_handles_max
        )),
    )
}

fn closed(operation: &'static str, action: &str) -> NetError {
    NetError::new(operation, action, Failure::Closed)
}

/// `HOST.Net.TCPConnect(endpoint, timeout)`.
///
/// # Errors
///
/// An invalid timeout, no connection in time, refusal, unreachable host, or
/// another operating-system failure.
pub fn tcp_connect(endpoint: Endpoint, timeout_ms: i128) -> Result<TcpStream, NetError> {
    const OPERATION: &str = "HOST.Net.TCPConnect";
    let action = format!("connect to {endpoint}");
    let timeout = checked_timeout(OPERATION, &action, timeout_ms)?;
    TcpStream::connect(endpoint, timeout).map_err(|error| {
        let failure = if error.kind() == std::io::ErrorKind::TimedOut {
            Failure::Timeout(u64::try_from(timeout_ms).unwrap_or(0))
        } else {
            Failure::Io(error)
        };
        NetError::new(OPERATION, action, failure)
    })
}

/// `HOST.Net.TCPListen(endpoints, backlog)`: all listeners or none.
///
/// # Errors
///
/// A backlog outside 1..128, a set outside 1..16 endpoints, or a failed
/// bind (an address in use).
pub fn tcp_listen(endpoints: &[Endpoint], backlog: i128) -> Result<Vec<TcpListener>, NetError> {
    const OPERATION: &str = "HOST.Net.TCPListen";
    let names: Vec<String> = endpoints.iter().map(ToString::to_string).collect();
    let action = format!("listen on {}", names.join(", "));
    let invalid =
        |rule: String| NetError::new(OPERATION, action.clone(), Failure::InvalidArgument(rule));
    let backlog = usize::try_from(backlog)
        .ok()
        .filter(|backlog| (1..=128).contains(backlog))
        .ok_or_else(|| invalid(format!("the backlog must be within 1..128; got {backlog}")))?;
    if endpoints.is_empty() || endpoints.len() > 16 {
        return Err(invalid(format!(
            "a listener set has 1..16 endpoints; got {}",
            endpoints.len()
        )));
    }
    endpoints
        .iter()
        .map(|endpoint| {
            TcpListener::bind_with_backlog(*endpoint, backlog).map_err(|error| {
                NetError::new(
                    OPERATION,
                    format!("listen on {endpoint}"),
                    Failure::Io(error),
                )
            })
        })
        .collect()
}

/// `HOST.Net.TCPListener.Accept(timeout)` on a listener set.
///
/// # Errors
///
/// A closed listener, an invalid timeout, no connection in time, or another
/// operating-system failure.
pub fn tcp_accept(
    listeners: Option<&[TcpListener]>,
    timeout_ms: i128,
) -> Result<TcpStream, NetError> {
    const OPERATION: &str = "HOST.Net.TCPListener.Accept";
    let action = "accept a connection";
    let listeners = listeners.ok_or_else(|| closed(OPERATION, action))?;
    let timeout = checked_timeout(OPERATION, action, timeout_ms)?;
    for listener in listeners {
        let accepted = listener
            .accept_timeout(timeout)
            .map_err(|error| NetError::new(OPERATION, action, Failure::Io(error)))?;
        if let Some(stream) = accepted {
            // An accepted stream waits as long as the accept did.
            let _ = stream.set_timeouts(Some(timeout), Some(timeout));
            return Ok(stream);
        }
    }
    Err(NetError::new(
        OPERATION,
        action,
        Failure::Timeout(u64::try_from(timeout_ms).unwrap_or(0)),
    ))
}

/// `HOST.Net.TCPListener.LocalEndpoint`: the first listener's endpoint.
///
/// # Errors
///
/// A closed listener or an operating-system failure.
pub fn listener_endpoint(listeners: Option<&[TcpListener]>) -> Result<Endpoint, NetError> {
    const OPERATION: &str = "HOST.Net.TCPListener.LocalEndpoint";
    let action = "read the listener's endpoint";
    let listener = listeners
        .and_then(<[TcpListener]>::first)
        .ok_or_else(|| closed(OPERATION, action))?;
    listener
        .local_endpoint()
        .map_err(|error| NetError::new(OPERATION, action, Failure::Io(error)))
}

/// The peer of `stream`, for messages (`read from 127.0.0.1:8080`).
fn peer(stream: &TcpStream) -> String {
    stream
        .remote_endpoint()
        .map_or_else(|_| "the TCP stream".into(), |endpoint| endpoint.to_string())
}

/// `HOST.Net.TCPStream.Read`: `Ok(0)` is `EOF`.
///
/// # Errors
///
/// A closed stream, no data in time, a reset connection, or another
/// operating-system failure.
pub fn tcp_read(stream: Option<&mut TcpStream>, buffer: &mut [u8]) -> Result<usize, NetError> {
    const OPERATION: &str = "HOST.Net.TCPStream.Read";
    let stream = stream.ok_or_else(|| closed(OPERATION, "read from the TCP stream"))?;
    let action = format!("read from {}", peer(stream));
    stream
        .read_bounded(buffer)
        .map_err(|error| NetError::new(OPERATION, action, Failure::Io(error)))
}

/// `HOST.Net.TCPStream.Write`: the bytes written.
///
/// # Errors
///
/// A closed stream, no progress in time, a reset connection, or another
/// operating-system failure.
pub fn tcp_write(stream: Option<&mut TcpStream>, bytes: &[u8]) -> Result<usize, NetError> {
    const OPERATION: &str = "HOST.Net.TCPStream.Write";
    let stream = stream.ok_or_else(|| closed(OPERATION, "write to the TCP stream"))?;
    let action = format!("write to {}", peer(stream));
    stream
        .write_bounded(bytes)
        .map_err(|error| NetError::new(OPERATION, action, Failure::Io(error)))
}

/// `HOST.Net.TCPStream.SetTimeouts(read, write)`.
///
/// # Errors
///
/// A closed stream, a timeout outside 1..60000 ms, or an operating-system
/// failure.
pub fn tcp_set_timeouts(
    stream: Option<&TcpStream>,
    read_ms: i128,
    write_ms: i128,
) -> Result<(), NetError> {
    const OPERATION: &str = "HOST.Net.TCPStream.SetTimeouts";
    let action = "set the timeouts of the TCP stream";
    let stream = stream.ok_or_else(|| closed(OPERATION, action))?;
    let read = checked_timeout(OPERATION, action, read_ms)?;
    let write = checked_timeout(OPERATION, action, write_ms)?;
    stream
        .set_timeouts(Some(read), Some(write))
        .map_err(|error| NetError::new(OPERATION, action, Failure::Io(error)))
}

/// `HOST.Net.TCPStream.ShutdownRead` / `ShutdownWrite`.
///
/// # Errors
///
/// A closed stream or an operating-system failure.
pub fn tcp_shutdown(stream: Option<&TcpStream>, reading: bool) -> Result<(), NetError> {
    let (operation, action, direction) = if reading {
        (
            "HOST.Net.TCPStream.ShutdownRead",
            "shut down reading on the TCP stream",
            Shutdown::Read,
        )
    } else {
        (
            "HOST.Net.TCPStream.ShutdownWrite",
            "shut down writing on the TCP stream",
            Shutdown::Write,
        )
    };
    let stream = stream.ok_or_else(|| closed(operation, action))?;
    stream
        .shutdown(direction)
        .map_err(|error| NetError::new(operation, action, Failure::Io(error)))
}

/// `HOST.Net.TCPStream.LocalEndpoint` / `RemoteEndpoint`.
///
/// # Errors
///
/// A closed stream or an operating-system failure.
pub fn tcp_endpoint(stream: Option<&TcpStream>, remote: bool) -> Result<Endpoint, NetError> {
    let (operation, action) = if remote {
        (
            "HOST.Net.TCPStream.RemoteEndpoint",
            "read the TCP stream's remote endpoint",
        )
    } else {
        (
            "HOST.Net.TCPStream.LocalEndpoint",
            "read the TCP stream's local endpoint",
        )
    };
    let stream = stream.ok_or_else(|| closed(operation, action))?;
    if remote {
        stream.remote_endpoint()
    } else {
        stream.local_endpoint()
    }
    .map_err(|error| NetError::new(operation, action, Failure::Io(error)))
}

#[cfg(test)]
mod tests {
    use super::{tcp_connect, tcp_listen, tcp_read};
    use crate::net::{Address, Endpoint};
    use bn_types::error_codes::net;

    #[test]
    fn tcp_errors_carry_portable_codes() {
        let loopback = Address::parse("127.0.0.1").unwrap();
        let listeners = tcp_listen(&[Endpoint::new(loopback, 0)], 8).unwrap();
        let taken = listeners[0].local_endpoint().unwrap();
        let error = tcp_listen(&[taken], 8).unwrap_err();
        assert_eq!(error.code(), net::ADDRESS_IN_USE);
        assert_eq!(error.message(), format!("cannot listen on {taken}"));
        let error = tcp_listen(&[taken], 0).unwrap_err();
        assert_eq!(error.cause(), "the backlog must be within 1..128; got 0");
        drop(listeners);
        // Windows reports a loopback refusal only after ~2 s of SYN retries.
        let error = tcp_connect(taken, 10_000).unwrap_err();
        assert_eq!(error.code(), net::CONNECTION_REFUSED);
        assert_eq!(error.operation(), "HOST.Net.TCPConnect");
        let error = tcp_read(None, &mut [0; 4]).unwrap_err();
        assert_eq!(error.code(), net::CLOSED);
        assert_eq!(error.cause(), "it was closed by Close");
    }
}
