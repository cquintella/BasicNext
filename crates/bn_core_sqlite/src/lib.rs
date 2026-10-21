// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `bn_core_sqlite` — Basic Next SQLite domain core.
//!
//! Provides thread-safe, synchronized SQLite database connection management,
//! query/statement execution, parameter extraction, and structured error mapping.

#![allow(unsafe_code)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::borrow_as_ptr)]
#![allow(clippy::needless_continue)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]

pub mod connection;
pub mod error;
pub(crate) mod file_id;
pub(crate) mod types;

pub use connection::{
    begin, changes, close, commit, exec, last_insert_rowid, open_connection, query, retain,
    rollback,
};
pub use error::SqliteFailure;
pub use types::{OpenMode, SqliteColumn, SqliteQueryResult, SqliteValue};
