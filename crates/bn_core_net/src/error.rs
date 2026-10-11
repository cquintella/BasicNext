// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A failed `HOST.Net` operation and the BN `Error` it becomes
//! (language/0.6/error.md; codes in host-net.md, "Errors"): the portable
//! code, the canonical operation, a message naming the address, endpoint, or
//! host, and the cause. Both backends build their `Error` from this one type.

use std::fmt;
use std::io;

use bn_types::error_codes::net;

/// Why an operation failed.
#[derive(Debug)]
pub enum Failure {
    /// An argument outside its domain; the text is the violated rule.
    InvalidArgument(String),
    /// A bounded wait expired after this many milliseconds.
    Timeout(u64),
    /// A bounded resource is exhausted; the text names the limit.
    Limit(String),
    /// An operation on a closed stream, listener, or socket.
    Closed,
    /// Resolution or neighbor lookup found no record.
    NotFound(String),
    /// The operation is not available on this host; the text says why.
    Unavailable(String),
    /// The destination cannot be reached; the text says why.
    Unreachable(String),
    /// The operating system refuses the operation; the text says why.
    PermissionDenied(String),
    /// The execution policy denies `HOST.Net`.
    PolicyDenied,
    /// An operating-system failure, classified by its kind.
    Io(io::Error),
}

impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// A failed `HOST.Net` operation.
#[derive(Debug)]
pub struct NetError {
    operation: &'static str,
    action: String,
    failure: Failure,
}

impl NetError {
    /// A failure of `operation` (its canonical name, `HOST.Net.TCPConnect`);
    /// `action` completes "cannot …" and names the target
    /// (`connect to 127.0.0.1:9`).
    #[must_use]
    pub fn new(operation: &'static str, action: impl Into<String>, failure: Failure) -> Self {
        Self {
            operation,
            action: action.into(),
            failure,
        }
    }

    #[must_use]
    pub const fn failure(&self) -> &Failure {
        &self.failure
    }

    /// `Error.Code`.
    #[must_use]
    pub fn code(&self) -> i32 {
        match &self.failure {
            Failure::InvalidArgument(_) => net::INVALID_ARGUMENT,
            Failure::Timeout(_) => net::TIMEOUT,
            Failure::Limit(_) => net::LIMIT,
            Failure::Closed => net::CLOSED,
            Failure::NotFound(_) => net::NOT_FOUND,
            Failure::Unavailable(_) => net::UNAVAILABLE,
            Failure::Unreachable(_) => net::UNREACHABLE,
            Failure::PermissionDenied(_) => net::PERMISSION_DENIED,
            Failure::PolicyDenied => net::POLICY_DENIED,
            Failure::Io(error) => io_code(error),
        }
    }

    /// `Error.Operation`.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    /// `Error.Message`.
    #[must_use]
    pub fn message(&self) -> String {
        format!("cannot {}", self.action)
    }

    /// `Error.Cause`.
    #[must_use]
    pub fn cause(&self) -> String {
        match &self.failure {
            Failure::InvalidArgument(rule)
            | Failure::Limit(rule)
            | Failure::NotFound(rule)
            | Failure::Unavailable(rule)
            | Failure::Unreachable(rule)
            | Failure::PermissionDenied(rule) => rule.clone(),
            Failure::Timeout(milliseconds) => format!("no answer within {milliseconds} ms"),
            Failure::Closed => "it was closed by Close".into(),
            Failure::PolicyDenied => "the execution policy denies HOST.Net".into(),
            Failure::Io(error) => error.to_string(),
        }
    }
}

/// A bounded wait of `timeout_ms` for `operation`, which `host-net.md`
/// bounds to 1..60000 ms.
///
/// # Errors
///
/// `Net.INVALID_ARGUMENT` outside that range.
pub fn checked_timeout(
    operation: &'static str,
    action: &str,
    timeout_ms: i128,
) -> Result<std::time::Duration, NetError> {
    u64::try_from(timeout_ms)
        .ok()
        .filter(|milliseconds| (1..=60_000).contains(milliseconds))
        .map(std::time::Duration::from_millis)
        .ok_or_else(|| {
            NetError::new(
                operation,
                action,
                Failure::InvalidArgument(format!(
                    "the timeout must be within 1..60000 ms; got {timeout_ms}"
                )),
            )
        })
}

/// The portable code of an operating-system failure.
fn io_code(error: &io::Error) -> i32 {
    use io::ErrorKind;
    match error.kind() {
        ErrorKind::TimedOut | ErrorKind::WouldBlock => net::TIMEOUT,
        ErrorKind::HostUnreachable | ErrorKind::NetworkUnreachable => net::UNREACHABLE,
        ErrorKind::ConnectionRefused => net::CONNECTION_REFUSED,
        ErrorKind::ConnectionReset
        | ErrorKind::ConnectionAborted
        | ErrorKind::BrokenPipe
        | ErrorKind::UnexpectedEof
        | ErrorKind::NotConnected => net::CONNECTION_CLOSED,
        ErrorKind::AddrInUse => net::ADDRESS_IN_USE,
        ErrorKind::PermissionDenied => net::PERMISSION_DENIED,
        ErrorKind::AddrNotAvailable | ErrorKind::Unsupported => net::UNAVAILABLE,
        _ => net::IO_FAILED,
    }
}

impl fmt::Display for NetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.message(), self.cause())
    }
}

#[cfg(test)]
mod tests {
    use super::{Failure, NetError};
    use bn_types::error_codes::net;
    use std::io;

    #[test]
    fn io_kinds_map_to_portable_codes() {
        let connect = |kind| {
            NetError::new(
                "HOST.Net.TCPConnect",
                "connect to 127.0.0.1:9",
                Failure::Io(io::Error::from(kind)),
            )
        };
        assert_eq!(
            connect(io::ErrorKind::ConnectionRefused).code(),
            net::CONNECTION_REFUSED
        );
        assert_eq!(connect(io::ErrorKind::TimedOut).code(), net::TIMEOUT);
        assert_eq!(
            connect(io::ErrorKind::AddrInUse).code(),
            net::ADDRESS_IN_USE
        );
        assert_eq!(
            connect(io::ErrorKind::ConnectionReset).code(),
            net::CONNECTION_CLOSED
        );
        assert_eq!(connect(io::ErrorKind::Other).code(), net::IO_FAILED);
        let error = NetError::new(
            "HOST.Net.TCPConnect",
            "connect to 127.0.0.1:9",
            Failure::InvalidArgument("the timeout must be within 1..60000 ms; got 0".into()),
        );
        assert_eq!(error.code(), net::INVALID_ARGUMENT);
        assert_eq!(error.message(), "cannot connect to 127.0.0.1:9");
        assert_eq!(
            error.cause(),
            "the timeout must be within 1..60000 ms; got 0"
        );
    }
}
