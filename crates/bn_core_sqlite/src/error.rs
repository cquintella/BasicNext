// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Structured error producer and mapping for `BNSqlite` operations.

use bn_types::error_codes::sqlite;
use rusqlite::ffi;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqliteFailure {
    FileNotFound(String),
    AccessDenied(String),
    PolicyDenied(String),
    Corrupt(String),
    Busy(String),
    Locked(String),
    ReadOnly(String),
    SyntaxError(String),
    SchemaError(String),
    ConstraintViolation(String),
    TypeMismatch(String),
    Misuse(String),
    Closed,
    LimitExceeded(String),
    IoFailed(String),
    Internal(String),
}

impl SqliteFailure {
    #[must_use]
    pub fn code(&self) -> i32 {
        match self {
            Self::FileNotFound(_) => sqlite::FILE_NOT_FOUND,
            Self::AccessDenied(_) => sqlite::ACCESS_DENIED,
            Self::PolicyDenied(_) => sqlite::POLICY_DENIED,
            Self::Corrupt(_) => sqlite::CORRUPT,
            Self::Busy(_) => sqlite::BUSY,
            Self::Locked(_) => sqlite::LOCKED,
            Self::ReadOnly(_) => sqlite::READ_ONLY,
            Self::SyntaxError(_) => sqlite::SYNTAX_ERROR,
            Self::SchemaError(_) => sqlite::SCHEMA_ERROR,
            Self::ConstraintViolation(_) => sqlite::CONSTRAINT_VIOLATION,
            Self::TypeMismatch(_) => sqlite::TYPE_MISMATCH,
            Self::Misuse(_) => sqlite::MISUSE,
            Self::Closed => sqlite::CLOSED,
            Self::LimitExceeded(_) => sqlite::LIMIT_EXCEEDED,
            Self::IoFailed(_) => sqlite::IO_FAILED,
            Self::Internal(_) => sqlite::INTERNAL_ERROR,
        }
    }

    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::FileNotFound(path) => {
                bn_catalog::fill("sqlite", "error.file_not_found.message", &[("path", path)])
            }
            Self::AccessDenied(msg) => {
                bn_catalog::fill("sqlite", "error.access_denied.message", &[("msg", msg)])
            }
            Self::PolicyDenied(msg) => {
                bn_catalog::fill("sqlite", "error.policy_denied.message", &[("msg", msg)])
            }
            Self::Corrupt(msg) => {
                bn_catalog::fill("sqlite", "error.corrupt.message", &[("msg", msg)])
            }
            Self::Busy(msg) => bn_catalog::fill("sqlite", "error.busy.message", &[("msg", msg)]),
            Self::Locked(msg) => {
                bn_catalog::fill("sqlite", "error.locked.message", &[("msg", msg)])
            }
            Self::ReadOnly(msg) => {
                bn_catalog::fill("sqlite", "error.read_only.message", &[("msg", msg)])
            }
            Self::SyntaxError(msg) => {
                bn_catalog::fill("sqlite", "error.syntax.message", &[("msg", msg)])
            }
            Self::SchemaError(msg) => {
                bn_catalog::fill("sqlite", "error.schema.message", &[("msg", msg)])
            }
            Self::ConstraintViolation(msg) => {
                bn_catalog::fill("sqlite", "error.constraint.message", &[("msg", msg)])
            }
            Self::TypeMismatch(msg) => {
                bn_catalog::fill("sqlite", "error.type_mismatch.message", &[("msg", msg)])
            }
            Self::Misuse(msg) => {
                bn_catalog::fill("sqlite", "error.misuse.message", &[("msg", msg)])
            }
            Self::Closed => bn_catalog::text("sqlite", "error.closed.message").to_owned(),
            Self::LimitExceeded(msg) => {
                bn_catalog::fill("sqlite", "error.limit.message", &[("msg", msg)])
            }
            Self::IoFailed(msg) => bn_catalog::fill("sqlite", "error.io.message", &[("msg", msg)]),
            Self::Internal(msg) => {
                bn_catalog::fill("sqlite", "error.internal.message", &[("msg", msg)])
            }
        }
    }

    #[must_use]
    pub fn cause(&self) -> String {
        let cause = |key: &str| bn_catalog::text("sqlite", key).to_owned();
        match self {
            Self::FileNotFound(_) => cause("error.file_not_found.cause"),
            Self::Busy(_) => cause("error.busy.cause"),
            Self::Locked(_) => cause("error.locked.cause"),
            Self::ReadOnly(_) => cause("error.read_only.cause"),
            Self::Closed => cause("error.closed.cause"),
            Self::AccessDenied(c)
            | Self::PolicyDenied(c)
            | Self::Corrupt(c)
            | Self::SyntaxError(c)
            | Self::SchemaError(c)
            | Self::ConstraintViolation(c)
            | Self::TypeMismatch(c)
            | Self::Misuse(c)
            | Self::LimitExceeded(c)
            | Self::IoFailed(c)
            | Self::Internal(c) => c.clone(),
        }
    }
}

impl From<rusqlite::Error> for SqliteFailure {
    fn from(err: rusqlite::Error) -> Self {
        match err {
            rusqlite::Error::SqliteFailure(ffi_err, detail) => {
                let text = detail.unwrap_or_else(|| ffi_err.to_string());
                let primary = ffi_err.extended_code & 0xFF;
                match primary {
                    ffi::SQLITE_BUSY => Self::Busy(text),
                    ffi::SQLITE_LOCKED => Self::Locked(text),
                    ffi::SQLITE_READONLY => Self::ReadOnly(text),
                    ffi::SQLITE_CANTOPEN => Self::AccessDenied(text),
                    ffi::SQLITE_CORRUPT | ffi::SQLITE_NOTADB => Self::Corrupt(text),
                    ffi::SQLITE_CONSTRAINT => Self::ConstraintViolation(text),
                    ffi::SQLITE_SCHEMA => Self::SchemaError(text),
                    ffi::SQLITE_MISMATCH => Self::TypeMismatch(text),
                    ffi::SQLITE_MISUSE => Self::Misuse(text),
                    ffi::SQLITE_ABORT | ffi::SQLITE_IOERR => Self::IoFailed(text),
                    _ => {
                        let lower = text.to_ascii_lowercase();
                        if lower.contains(bn_catalog::text("sqlite", "needle.syntax_error"))
                            || lower
                                .contains(bn_catalog::text("sqlite", "needle.unrecognized_token"))
                        {
                            Self::SyntaxError(text)
                        } else if lower.contains(bn_catalog::text("sqlite", "needle.no_such_table"))
                            || lower.contains(bn_catalog::text("sqlite", "needle.no_such_column"))
                            || lower.contains(bn_catalog::text("sqlite", "needle.no_such_index"))
                        {
                            Self::SchemaError(text)
                        } else {
                            Self::Internal(text)
                        }
                    }
                }
            }
            rusqlite::Error::FromSqlConversionFailure(_, _, detail) => {
                Self::TypeMismatch(detail.to_string())
            }
            rusqlite::Error::IntegralValueOutOfRange(_, _) => {
                Self::TypeMismatch(bn_catalog::text("sqlite", "error.integer_range").to_owned())
            }
            rusqlite::Error::Utf8Error(detail) => Self::TypeMismatch(bn_catalog::fill(
                "sqlite",
                "error.utf8",
                &[("detail", &detail)],
            )),
            other => {
                let msg = other.to_string();
                let lower = msg.to_ascii_lowercase();
                if lower.contains(bn_catalog::text("sqlite", "needle.syntax_error"))
                    || lower.contains(bn_catalog::text("sqlite", "needle.unrecognized_token"))
                {
                    Self::SyntaxError(msg)
                } else if lower.contains(bn_catalog::text("sqlite", "needle.no_such_table"))
                    || lower.contains(bn_catalog::text("sqlite", "needle.no_such_column"))
                {
                    Self::SchemaError(msg)
                } else {
                    Self::Internal(msg)
                }
            }
        }
    }
}
