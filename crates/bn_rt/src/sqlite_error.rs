// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Structured error producer and mapping for `BNSqlite` operations.

use bn_types::error_codes::sqlite;
use rusqlite::ffi;

#[derive(Debug, Clone)]
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
            Self::FileNotFound(path) => format!("database file not found: {path}"),
            Self::AccessDenied(msg) => format!("database access denied: {msg}"),
            Self::PolicyDenied(msg) => format!("execution policy denied database operation: {msg}"),
            Self::Corrupt(msg) => format!("database corruption detected: {msg}"),
            Self::Busy(msg) => format!("database is busy: {msg}"),
            Self::Locked(msg) => format!("database table or lock conflict: {msg}"),
            Self::ReadOnly(msg) => format!("attempt to write to read-only database: {msg}"),
            Self::SyntaxError(msg) => format!("SQL syntax error: {msg}"),
            Self::SchemaError(msg) => format!("database schema error: {msg}"),
            Self::ConstraintViolation(msg) => format!("database constraint violation: {msg}"),
            Self::TypeMismatch(msg) => format!("data type mismatch: {msg}"),
            Self::Misuse(msg) => format!("API misuse: {msg}"),
            Self::Closed => "database connection is closed".into(),
            Self::LimitExceeded(msg) => format!("database limit exceeded: {msg}"),
            Self::IoFailed(msg) => format!("database I/O failure: {msg}"),
            Self::Internal(msg) => format!("internal database engine failure: {msg}"),
        }
    }

    #[must_use]
    pub fn cause(&self) -> String {
        match self {
            Self::FileNotFound(_) => "the specified path does not exist on disk".into(),
            Self::Busy(_) => "database lock contention exceeded timeout".into(),
            Self::Locked(_) => "conflicting transaction or statement held a lock".into(),
            Self::ReadOnly(_) => {
                "database was opened with OpenReadOnly or media is read-only".into()
            }
            Self::Closed => "method invoked on a closed Connection".into(),
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

    #[must_use]
    pub fn report(&self, operation: &str) -> i32 {
        crate::set_error_report(self.code(), operation, self.message(), self.cause());
        self.code()
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
                        if lower.contains("syntax error") || lower.contains("unrecognized token") {
                            Self::SyntaxError(text)
                        } else if lower.contains("no such table")
                            || lower.contains("no such column")
                            || lower.contains("no such index")
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
                Self::TypeMismatch("integer value out of 64-bit range".into())
            }
            rusqlite::Error::Utf8Error(detail) => {
                Self::TypeMismatch(format!("text is not valid UTF-8: {detail}"))
            }
            other => {
                let msg = other.to_string();
                let lower = msg.to_ascii_lowercase();
                if lower.contains("syntax error") || lower.contains("unrecognized token") {
                    Self::SyntaxError(msg)
                } else if lower.contains("no such table") || lower.contains("no such column") {
                    Self::SchemaError(msg)
                } else {
                    Self::Internal(msg)
                }
            }
        }
    }
}
