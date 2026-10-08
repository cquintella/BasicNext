// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Portable `Error.Code` constants (language/0.6/error.md), one table per
//! capability. The frontend exposes them as members (`FS.NOT_FOUND`) and the
//! shared runtime cores produce them, so a value is written once.

/// `HOST.FileSystem` codes (language/0.6/host.md, "File system errors").
pub mod fs {
    pub const INVALID_ARGUMENT: i32 = 1;
    pub const NOT_FOUND: i32 = 2;
    pub const PERMISSION_DENIED: i32 = 3;
    pub const IS_DIRECTORY: i32 = 4;
    pub const CLOSED: i32 = 5;
    pub const WRONG_FAMILY: i32 = 6;
    pub const INVALID_UTF8: i32 = 7;
    pub const IO_FAILED: i32 = 8;
    pub const POLICY_DENIED: i32 = 9;

    /// Member name and value of every code.
    pub const ALL: [(&str, i32); 9] = [
        ("INVALID_ARGUMENT", INVALID_ARGUMENT),
        ("NOT_FOUND", NOT_FOUND),
        ("PERMISSION_DENIED", PERMISSION_DENIED),
        ("IS_DIRECTORY", IS_DIRECTORY),
        ("CLOSED", CLOSED),
        ("WRONG_FAMILY", WRONG_FAMILY),
        ("INVALID_UTF8", INVALID_UTF8),
        ("IO_FAILED", IO_FAILED),
        ("POLICY_DENIED", POLICY_DENIED),
    ];
}

/// `HOST.Net` codes (language/0.6/host-net.md, "Errors").
pub mod net {
    pub const INVALID_ARGUMENT: i32 = 1;
    pub const TIMEOUT: i32 = 2;
    pub const UNREACHABLE: i32 = 3;
    pub const CONNECTION_REFUSED: i32 = 4;
    pub const CONNECTION_CLOSED: i32 = 5;
    pub const ADDRESS_IN_USE: i32 = 6;
    pub const PERMISSION_DENIED: i32 = 7;
    pub const NOT_FOUND: i32 = 8;
    pub const UNAVAILABLE: i32 = 9;
    pub const LIMIT: i32 = 10;
    pub const CLOSED: i32 = 11;
    pub const IO_FAILED: i32 = 12;
    pub const POLICY_DENIED: i32 = 13;

    /// Member name and value of every code.
    pub const ALL: [(&str, i32); 13] = [
        ("INVALID_ARGUMENT", INVALID_ARGUMENT),
        ("TIMEOUT", TIMEOUT),
        ("UNREACHABLE", UNREACHABLE),
        ("CONNECTION_REFUSED", CONNECTION_REFUSED),
        ("CONNECTION_CLOSED", CONNECTION_CLOSED),
        ("ADDRESS_IN_USE", ADDRESS_IN_USE),
        ("PERMISSION_DENIED", PERMISSION_DENIED),
        ("NOT_FOUND", NOT_FOUND),
        ("UNAVAILABLE", UNAVAILABLE),
        ("LIMIT", LIMIT),
        ("CLOSED", CLOSED),
        ("IO_FAILED", IO_FAILED),
        ("POLICY_DENIED", POLICY_DENIED),
    ];
}

/// `HOST.Exec` codes (language/0.6/0.6.md, "HOST.Exec Errors").
pub mod exec {
    pub const INVALID_ARGUMENT: i32 = 1;
    pub const PROGRAM_NOT_FOUND: i32 = 2;
    pub const PERMISSION_DENIED: i32 = 3;
    pub const SPAWN_FAILED: i32 = 4;
    pub const WAIT_FAILED: i32 = 5;
    pub const CAPTURE_FAILED: i32 = 6;
    pub const INVALID_UTF8: i32 = 7;
    pub const CAPTURE_LIMIT: i32 = 8;
    pub const TIMEOUT: i32 = 9;
    pub const TERMINATION_FAILED: i32 = 10;
    pub const POLICY_DENIED: i32 = 11;

    /// Member name and value of every code.
    pub const ALL: [(&str, i32); 11] = [
        ("INVALID_ARGUMENT", INVALID_ARGUMENT),
        ("PROGRAM_NOT_FOUND", PROGRAM_NOT_FOUND),
        ("PERMISSION_DENIED", PERMISSION_DENIED),
        ("SPAWN_FAILED", SPAWN_FAILED),
        ("WAIT_FAILED", WAIT_FAILED),
        ("CAPTURE_FAILED", CAPTURE_FAILED),
        ("INVALID_UTF8", INVALID_UTF8),
        ("CAPTURE_LIMIT", CAPTURE_LIMIT),
        ("TIMEOUT", TIMEOUT),
        ("TERMINATION_FAILED", TERMINATION_FAILED),
        ("POLICY_DENIED", POLICY_DENIED),
    ];
}

/// `HOST.Env` codes (language/0.6/host-env.md, "Errors").
pub mod env {
    pub const NOT_SET: i32 = 1;
    pub const INVALID_NAME: i32 = 2;
    pub const INVALID_UTF8: i32 = 3;
    pub const POLICY_DENIED: i32 = 4;

    /// Member name and value of every code.
    pub const ALL: [(&str, i32); 4] = [
        ("NOT_SET", NOT_SET),
        ("INVALID_NAME", INVALID_NAME),
        ("INVALID_UTF8", INVALID_UTF8),
        ("POLICY_DENIED", POLICY_DENIED),
    ];
}

/// `BNCrypto` `Error.Code` (`language/0.6/bncrypto.md` "Errors").
pub mod crypto {
    pub const INVALID_ARGUMENT: i32 = 1;
    pub const AUTHENTICATION_FAILED: i32 = 2;
    pub const UNAVAILABLE: i32 = 3;

    /// Name and value of each constant, as `modules/bn/BNCrypto.bn` exports them.
    pub const ALL: &[(&str, i32)] = &[
        ("INVALID_ARGUMENT", INVALID_ARGUMENT),
        ("AUTHENTICATION_FAILED", AUTHENTICATION_FAILED),
        ("UNAVAILABLE", UNAVAILABLE),
    ];
}

/// `BNJson` `Error.Code` (`language/0.6/bnjson.md` "Errors").
pub mod json {
    pub const INVALID_ARGUMENT: i32 = 1;
    pub const NOT_FOUND: i32 = 2;
    pub const TYPE_MISMATCH: i32 = 3;
    pub const OUT_OF_RANGE: i32 = 4;
    pub const LIMIT: i32 = 5;
    pub const UNAVAILABLE: i32 = 6;
    pub const PARSE_FAILED: i32 = 7;

    /// Name and value of each constant, as `modules/bn/BNJson.bn` exports them.
    pub const ALL: &[(&str, i32)] = &[
        ("INVALID_ARGUMENT", INVALID_ARGUMENT),
        ("NOT_FOUND", NOT_FOUND),
        ("TYPE_MISMATCH", TYPE_MISMATCH),
        ("OUT_OF_RANGE", OUT_OF_RANGE),
        ("LIMIT", LIMIT),
        ("UNAVAILABLE", UNAVAILABLE),
        ("PARSE_FAILED", PARSE_FAILED),
    ];
}

/// `BNDispatch` `Error.Code` (`language/0.6/bndispatch.md` "Errors").
pub mod dispatch {
    pub const INVALID_ARGUMENT: i32 = 1;
    pub const TIMEOUT: i32 = 2;
    pub const CLOSED: i32 = 3;
    pub const SATURATED: i32 = 4;
    pub const CANCELLED: i32 = 5;
    pub const TASK_FAILED: i32 = 6;
    pub const INVALID_STATE: i32 = 7;
    pub const UNAVAILABLE: i32 = 8;

    /// Name and value of each constant, as `modules/bn/BNDispatch.bn` exports them.
    pub const ALL: &[(&str, i32)] = &[
        ("INVALID_ARGUMENT", INVALID_ARGUMENT),
        ("TIMEOUT", TIMEOUT),
        ("CLOSED", CLOSED),
        ("SATURATED", SATURATED),
        ("CANCELLED", CANCELLED),
        ("TASK_FAILED", TASK_FAILED),
        ("INVALID_STATE", INVALID_STATE),
        ("UNAVAILABLE", UNAVAILABLE),
    ];
}

/// `BNLog` `Error.Code` (`language/0.6/bnlog.md` "Errors").
pub mod log {
    pub const INVALID_ARGUMENT: i32 = 1;
    pub const DUPLICATE: i32 = 2;
    pub const LIMIT: i32 = 3;
    pub const CLOSED: i32 = 4;
    pub const IO_FAILED: i32 = 5;
    pub const CAPABILITY_REQUIRED: i32 = 6;
    pub const TIMEOUT: i32 = 7;
    pub const UNAVAILABLE: i32 = 8;

    /// Name and value of each constant, as `modules/bn/BNLog.bn` exports them.
    pub const ALL: &[(&str, i32)] = &[
        ("INVALID_ARGUMENT", INVALID_ARGUMENT),
        ("DUPLICATE", DUPLICATE),
        ("LIMIT", LIMIT),
        ("CLOSED", CLOSED),
        ("IO_FAILED", IO_FAILED),
        ("CAPABILITY_REQUIRED", CAPABILITY_REQUIRED),
        ("TIMEOUT", TIMEOUT),
        ("UNAVAILABLE", UNAVAILABLE),
    ];
}

/// `BNData` `Error.Code` (`language/0.6/bndata.md` "Errors").
pub mod data {
    pub const INVALID_ARGUMENT: i32 = 1;
    pub const NOT_FOUND: i32 = 2;
    pub const TYPE_MISMATCH: i32 = 3;
    pub const OUT_OF_RANGE: i32 = 4;
    pub const IO_FAILED: i32 = 5;
    pub const INVALID_FORMAT: i32 = 6;

    /// Name and value of each constant, as `modules/bn/BNData.bn` exports them.
    pub const ALL: &[(&str, i32)] = &[
        ("INVALID_ARGUMENT", INVALID_ARGUMENT),
        ("NOT_FOUND", NOT_FOUND),
        ("TYPE_MISMATCH", TYPE_MISMATCH),
        ("OUT_OF_RANGE", OUT_OF_RANGE),
        ("IO_FAILED", IO_FAILED),
        ("INVALID_FORMAT", INVALID_FORMAT),
    ];
}

/// `BNWeb` `Error.Code` (`language/0.6/bnweb.md` "Errors").
pub mod web {
    pub const INVALID_ARGUMENT: i32 = 1;
    pub const NOT_FOUND: i32 = 2;
    pub const OUT_OF_RANGE: i32 = 3;
    pub const LIMIT: i32 = 4;
    pub const CLOSED: i32 = 5;
    pub const TIMEOUT: i32 = 6;
    pub const EGRESS_DENIED: i32 = 7;
    pub const HTTP_FAILED: i32 = 8;
    pub const UNAVAILABLE: i32 = 9;

    /// Name and value of each constant, as `modules/bn/BNWeb.bn` exports them.
    pub const ALL: &[(&str, i32)] = &[
        ("INVALID_ARGUMENT", INVALID_ARGUMENT),
        ("NOT_FOUND", NOT_FOUND),
        ("OUT_OF_RANGE", OUT_OF_RANGE),
        ("LIMIT", LIMIT),
        ("CLOSED", CLOSED),
        ("TIMEOUT", TIMEOUT),
        ("EGRESS_DENIED", EGRESS_DENIED),
        ("HTTP_FAILED", HTTP_FAILED),
        ("UNAVAILABLE", UNAVAILABLE),
    ];
}

/// `BNSqlite` `Error.Code` (`language/0.6/bnsqlite.md` "Errors").
pub mod sqlite {
    pub const FILE_NOT_FOUND: i32 = 1;
    pub const ACCESS_DENIED: i32 = 2;
    pub const POLICY_DENIED: i32 = 3;
    pub const CORRUPT: i32 = 4;
    pub const BUSY: i32 = 5;
    pub const LOCKED: i32 = 6;
    pub const READ_ONLY: i32 = 7;
    pub const SYNTAX_ERROR: i32 = 8;
    pub const SCHEMA_ERROR: i32 = 9;
    pub const CONSTRAINT_VIOLATION: i32 = 10;
    pub const TYPE_MISMATCH: i32 = 11;
    pub const MISUSE: i32 = 12;
    pub const CLOSED: i32 = 13;
    pub const LIMIT_EXCEEDED: i32 = 14;
    pub const IO_FAILED: i32 = 15;
    pub const INTERNAL_ERROR: i32 = 16;

    /// Name and value of each constant, as `modules/bn/BNSqlite.bn` exports them.
    pub const ALL: &[(&str, i32)] = &[
        ("FILE_NOT_FOUND", FILE_NOT_FOUND),
        ("ACCESS_DENIED", ACCESS_DENIED),
        ("POLICY_DENIED", POLICY_DENIED),
        ("CORRUPT", CORRUPT),
        ("BUSY", BUSY),
        ("LOCKED", LOCKED),
        ("READ_ONLY", READ_ONLY),
        ("SYNTAX_ERROR", SYNTAX_ERROR),
        ("SCHEMA_ERROR", SCHEMA_ERROR),
        ("CONSTRAINT_VIOLATION", CONSTRAINT_VIOLATION),
        ("TYPE_MISMATCH", TYPE_MISMATCH),
        ("MISUSE", MISUSE),
        ("CLOSED", CLOSED),
        ("LIMIT_EXCEEDED", LIMIT_EXCEEDED),
        ("IO_FAILED", IO_FAILED),
        ("INTERNAL_ERROR", INTERNAL_ERROR),
    ];
}
