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
