// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Structured error reporting integration for `BNSqlite` operations in `bn_rt`.

pub use bn_core_sqlite::SqliteFailure;

pub trait SqliteReport {
    fn report(&self, operation: &str) -> i32;
}

impl SqliteReport for SqliteFailure {
    fn report(&self, operation: &str) -> i32 {
        crate::set_error_report(self.code(), operation, self.message(), self.cause());
        self.code()
    }
}
