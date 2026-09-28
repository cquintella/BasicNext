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
