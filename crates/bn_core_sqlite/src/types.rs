// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Type definitions for SQLite values, query columns, and open modes.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenMode {
    CreateOrReadWrite,
    ReadOnly,
    ExistingReadWrite,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SqliteValue {
    Integer(i64),
    Float(f64),
    Text(Vec<u8>),
    Blob(Vec<u8>),
    Null,
    NotAvailable,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SqliteColumn {
    pub name: String,
    pub values: Vec<SqliteValue>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SqliteQueryResult {
    pub columns: Vec<SqliteColumn>,
}
