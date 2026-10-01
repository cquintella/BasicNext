// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNLog` `Error`s (`language/0.6/bnlog.md` "Errors"), one producer for both
//! backends: the interpreter turns a [`LogFailure`] into an `Error` value, the
//! C ABI records it for the emitted code.

use bn_types::error_codes::log;

/// Why a `BNLog` operation failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LogFailure {
    /// Invalid handle (released or unknown).
    InvalidHandle,
    /// An unknown level or timeout outside bounds; `what` names it.
    OutOfRange {
        what: &'static str,
        value: i128,
        min: i128,
        max: i128,
    },
    /// A string argument past its length bound, or empty when not allowed.
    InvalidString {
        what: &'static str,
        len: usize,
        max: usize,
    },
    /// Invalid transport type or target.
    InvalidTransport(&'static str),
    /// A field or entry key that already exists.
    DuplicateKey(String),
    /// Too many transports, fields, or entry fields.
    LimitExceeded {
        what: &'static str,
        count: usize,
        max: usize,
    },
    /// Record serialization error (e.g. JSON line serialization failed or exceeded buffer).
    RecordSerialization(String),
    /// An operation on a closed logger.
    Closed,
    /// A transport could not write or flush.
    IoFailed(String),
    /// `AddFile` without `IMPORT HOST.FileSystem`, `AddConsole` without `IMPORT HOST.Console`.
    CapabilityRequired(&'static str),
    /// Flush or Close ran past its timeout.
    Timeout { ms: i128 },
    /// The operation or provider is not available.
    Unavailable(&'static str),
}

impl LogFailure {
    /// `Error.Code`.
    #[must_use]
    pub const fn code(&self) -> i32 {
        match self {
            Self::InvalidHandle
            | Self::OutOfRange { .. }
            | Self::InvalidString { .. }
            | Self::InvalidTransport(_) => log::INVALID_ARGUMENT,
            Self::DuplicateKey(_) => log::DUPLICATE,
            Self::LimitExceeded { .. } | Self::RecordSerialization(_) => log::LIMIT,
            Self::Closed => log::CLOSED,
            Self::IoFailed(_) => log::IO_FAILED,
            Self::CapabilityRequired(_) => log::CAPABILITY_REQUIRED,
            Self::Timeout { .. } => log::TIMEOUT,
            Self::Unavailable(_) => log::UNAVAILABLE,
        }
    }

    /// `Error.Message`: what failed.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::InvalidHandle => "the logger or fields handle is invalid".into(),
            Self::OutOfRange { what, value, .. } => format!("cannot use {value} as the {what}"),
            Self::InvalidString { what, len: 0, .. } => {
                format!("the {what} must not be empty")
            }
            Self::InvalidString { what, len, max } => {
                format!("the {what} of {len} bytes exceeds the maximum of {max} bytes")
            }
            Self::InvalidTransport(kind) => format!("invalid transport {kind}"),
            Self::DuplicateKey(key) => format!("key \"{key}\" already exists"),
            Self::LimitExceeded { what, count, max } => {
                format!("cannot add {what}: {count} reaches the maximum of {max}")
            }
            Self::RecordSerialization(reason) => {
                format!("cannot serialize log record: {reason}")
            }
            Self::Closed => "logger is closed".into(),
            Self::IoFailed(reason) => format!("log transport failed: {reason}"),
            Self::CapabilityRequired(cap) => format!("{cap} capability is required"),
            Self::Timeout { ms } => format!("timed out after {ms} ms"),
            Self::Unavailable(_) => "the operation is not available".into(),
        }
    }

    /// `Error.Cause`: the violated rule.
    #[must_use]
    pub fn cause(&self) -> String {
        match self {
            Self::InvalidHandle => "the handle was released or was not created by BNLog".into(),
            Self::OutOfRange { what, min, max, .. } => {
                format!("the {what} must be from {min} through {max}")
            }
            Self::InvalidString { what, len: 0, .. } => {
                format!("the {what} must have at least 1 byte")
            }
            Self::InvalidString { what, max, .. } => {
                format!("the {what} must not exceed {max} bytes")
            }
            Self::InvalidTransport(kind) => format!("transport \"{kind}\" is not supported"),
            Self::DuplicateKey(key) => format!("the key \"{key}\" is already present"),
            Self::LimitExceeded { what, max, .. } => {
                format!("a logger allows at most {max} {what}")
            }
            Self::RecordSerialization(reason) | Self::IoFailed(reason) => reason.clone(),
            Self::Closed => "a closed logger accepts no further operations".into(),
            Self::CapabilityRequired(cap) => {
                format!("importing {cap} is required to use this transport")
            }
            Self::Timeout { .. } => "the operation did not complete within its timeout".into(),
            Self::Unavailable(reason) => (*reason).into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LogFailure;
    use bn_types::error_codes::log;

    #[test]
    fn failures_carry_code_message_and_cause() {
        let level = LogFailure::OutOfRange {
            what: "log level",
            value: 9,
            min: 0,
            max: 6,
        };
        assert_eq!(level.code(), log::INVALID_ARGUMENT);
        assert_eq!(level.message(), "cannot use 9 as the log level");
        assert_eq!(level.cause(), "the log level must be from 0 through 6");

        let dup = LogFailure::DuplicateKey("foo".into());
        assert_eq!(dup.code(), log::DUPLICATE);
        assert_eq!(dup.message(), "key \"foo\" already exists");
        assert_eq!(dup.cause(), "the key \"foo\" is already present");

        let limit = LogFailure::LimitExceeded {
            what: "transports",
            count: 8,
            max: 8,
        };
        assert_eq!(limit.code(), log::LIMIT);
        assert_eq!(LogFailure::Closed.code(), log::CLOSED);
        assert_eq!(
            LogFailure::IoFailed("disk full".into()).code(),
            log::IO_FAILED
        );
        assert_eq!(
            LogFailure::CapabilityRequired("HOST.Console").code(),
            log::CAPABILITY_REQUIRED
        );
        assert_eq!(LogFailure::Timeout { ms: 100 }.code(), log::TIMEOUT);
        assert_eq!(
            LogFailure::Unavailable("no provider").code(),
            log::UNAVAILABLE
        );
    }
}
