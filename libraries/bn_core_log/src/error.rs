// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNLog` `Error` descriptions (`language/0.6/bnlog.md` "Errors").

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
    /// A `Logger.New` format that is not `"JSON"`, `"SYSLOG"` or `"APACHE"`.
    InvalidFormat(String),
    /// A level method on an APACHE logger, which writes access records only.
    ApacheLevel,
    /// Record serialization error (e.g. JSON line serialization failed or exceeded buffer).
    RecordSerialization(String),
    /// An operation on a closed logger.
    Closed,
    /// A transport could not write or flush.
    IoFailed(String),
    /// A destination whose capability the program did not import.
    CapabilityRequired(&'static str),
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
            | Self::InvalidFormat(_)
            | Self::ApacheLevel => log::INVALID_ARGUMENT,
            Self::RecordSerialization(_) => log::LIMIT,
            Self::Closed => log::CLOSED,
            Self::IoFailed(_) => log::IO_FAILED,
            Self::CapabilityRequired(_) => log::CAPABILITY_REQUIRED,
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
            Self::InvalidFormat(format) => {
                crate::catalog::text("error.invalid_format.message").replace("{format}", format)
            }
            Self::ApacheLevel => crate::catalog::text("error.apache_level.message").into(),
            Self::RecordSerialization(reason) => {
                format!("cannot serialize log record: {reason}")
            }
            Self::Closed => "logger is closed".into(),
            Self::IoFailed(reason) => format!("log transport failed: {reason}"),
            Self::CapabilityRequired(cap) => format!("{cap} capability is required"),
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
            Self::InvalidFormat(_) => crate::catalog::text("error.invalid_format.cause").into(),
            Self::ApacheLevel => crate::catalog::text("error.apache_level.cause").into(),
            Self::RecordSerialization(reason) | Self::IoFailed(reason) => reason.clone(),
            Self::Closed => "a closed logger accepts no further operations".into(),
            Self::CapabilityRequired(cap) => {
                crate::catalog::fill("error.capability.cause", &[("capability", cap)])
            }
            Self::Unavailable(reason) => (*reason).into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LogFailure;
    use bn_types::error_codes::log;

    #[test]
    fn an_unknown_format_names_it_from_the_catalog() {
        let failure = LogFailure::InvalidFormat("xml".into());
        assert_eq!(failure.code(), log::INVALID_ARGUMENT);
        assert_eq!(failure.message(), "unknown log format \"xml\"");
        assert_eq!(
            failure.cause(),
            "the format is \"JSON\", \"SYSLOG\" or \"APACHE\""
        );
    }

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

        let apache = LogFailure::ApacheLevel;
        assert_eq!(apache.code(), log::INVALID_ARGUMENT);
        assert_eq!(
            apache.message(),
            "cannot write a level record to an APACHE logger"
        );
        assert_eq!(
            LogFailure::RecordSerialization("long".into()).code(),
            log::LIMIT
        );
        assert_eq!(
            LogFailure::CapabilityRequired("HOST.Console").cause(),
            "the destination needs IMPORT HOST.Console"
        );
        assert_eq!(LogFailure::Closed.code(), log::CLOSED);
        assert_eq!(
            LogFailure::IoFailed("disk full".into()).code(),
            log::IO_FAILED
        );
        assert_eq!(
            LogFailure::CapabilityRequired("HOST.Console").code(),
            log::CAPABILITY_REQUIRED
        );
        assert_eq!(
            LogFailure::Unavailable("no provider").code(),
            log::UNAVAILABLE
        );
    }
}
