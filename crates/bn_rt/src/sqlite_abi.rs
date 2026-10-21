// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Stable C ABI and runtime adapter for `BNSqlite` database operations.
#![allow(unsafe_code)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::borrow_as_ptr)]

use std::ffi::c_char;

use bn_core_sqlite::{OpenMode, SqliteFailure, SqliteValue};

use super::dataframe::DataFrameColumn;
use super::dataframe_abi::{BNDataFrameHandle, StoredValue, register_frame_columns};
use super::sqlite_error::SqliteReport as _;

fn input_string(pointer: *const c_char) -> Option<String> {
    // Path and SQL arrive as STRING values. The header length keeps an
    // interior NUL, which `bn_core_sqlite` rejects (G7-D4).
    super::string::string_text(pointer)
}

fn open_connection(path_str: &str, mode: OpenMode) -> Result<u64, SqliteFailure> {
    bn_core_sqlite::open_connection(path_str, mode, |canonical_path, write_required| {
        if super::policy::allows_path(canonical_path, write_required) {
            Ok(())
        } else {
            let reason = super::policy::with_fs(|p| p.denial_reason(write_required));
            Err(reason.to_string())
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_sqlite_open(path: *const c_char, out_handle: *mut u64) -> i32 {
    let Some(path_str) = input_string(path) else {
        return SqliteFailure::Misuse("invalid null or non-UTF8 path string".into())
            .report("BNSqlite.Open");
    };
    if out_handle.is_null() {
        return SqliteFailure::Misuse("null out_handle pointer".into()).report("BNSqlite.Open");
    }

    match open_connection(&path_str, OpenMode::CreateOrReadWrite) {
        Ok(handle) => {
            // SAFETY: out_handle pointer checked non-null above.
            unsafe { *out_handle = handle };
            0
        }
        Err(failure) => failure.report("BNSqlite.Open"),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_sqlite_open_read_only(path: *const c_char, out_handle: *mut u64) -> i32 {
    let Some(path_str) = input_string(path) else {
        return SqliteFailure::Misuse("invalid null or non-UTF8 path string".into())
            .report("BNSqlite.OpenReadOnly");
    };
    if out_handle.is_null() {
        return SqliteFailure::Misuse("null out_handle pointer".into())
            .report("BNSqlite.OpenReadOnly");
    }

    match open_connection(&path_str, OpenMode::ReadOnly) {
        Ok(handle) => {
            // SAFETY: out_handle pointer checked non-null above.
            unsafe { *out_handle = handle };
            0
        }
        Err(failure) => failure.report("BNSqlite.OpenReadOnly"),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_sqlite_open_existing(path: *const c_char, out_handle: *mut u64) -> i32 {
    let Some(path_str) = input_string(path) else {
        return SqliteFailure::Misuse("invalid null or non-UTF8 path string".into())
            .report("BNSqlite.OpenExisting");
    };
    if out_handle.is_null() {
        return SqliteFailure::Misuse("null out_handle pointer".into())
            .report("BNSqlite.OpenExisting");
    }

    match open_connection(&path_str, OpenMode::ExistingReadWrite) {
        Ok(handle) => {
            // SAFETY: out_handle pointer checked non-null above.
            unsafe { *out_handle = handle };
            0
        }
        Err(failure) => failure.report("BNSqlite.OpenExisting"),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_sqlite_retain(handle: u64) -> i32 {
    if handle == 0 {
        return 0;
    }
    if bn_core_sqlite::retain(handle) {
        0
    } else {
        SqliteFailure::Closed.report(bn_catalog::text("sqlite", "msg.bnsqlite_connection_retain"))
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_sqlite_close(handle: u64) -> i32 {
    if handle == 0 {
        return 0;
    }
    match bn_core_sqlite::close(handle) {
        Ok(()) => 0,
        Err(failure) => failure.report("BNSqlite.Connection.Close"),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_sqlite_exec(handle: u64, sql: *const c_char) -> i32 {
    let Some(sql_str) = input_string(sql) else {
        return SqliteFailure::Misuse("invalid null or non-UTF8 SQL string".into())
            .report("BNSqlite.Connection.Exec");
    };

    match bn_core_sqlite::exec(handle, &sql_str) {
        Ok(()) => 0,
        Err(failure) => failure.report("BNSqlite.Connection.Exec"),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_sqlite_query(
    handle: u64,
    sql: *const c_char,
    out_frame: *mut BNDataFrameHandle,
) -> i32 {
    let Some(sql_str) = input_string(sql) else {
        return SqliteFailure::Misuse("invalid null or non-UTF8 SQL string".into())
            .report("BNSqlite.Connection.Query");
    };
    if out_frame.is_null() {
        return SqliteFailure::Misuse("null out_frame pointer".into())
            .report("BNSqlite.Connection.Query");
    }

    match bn_core_sqlite::query(handle, &sql_str) {
        Ok(query_result) => {
            let mut columns: Vec<DataFrameColumn<StoredValue>> =
                Vec::with_capacity(query_result.columns.len());
            for col in query_result.columns {
                let mut values = Vec::with_capacity(col.values.len());
                for val in col.values {
                    let stored = match val {
                        SqliteValue::Integer(i) => StoredValue::Integer(i),
                        SqliteValue::Float(f) => StoredValue::Float(f),
                        SqliteValue::Text(s) => StoredValue::String(s),
                        SqliteValue::Blob(b) => StoredValue::Bytes(b),
                        SqliteValue::Null => StoredValue::Null,
                        SqliteValue::NotAvailable => StoredValue::NotAvailable,
                    };
                    values.push(stored);
                }
                columns.push(DataFrameColumn {
                    name: col.name,
                    values,
                });
            }

            let frame_handle = register_frame_columns(columns);
            // SAFETY: out_frame checked non-null above.
            unsafe { *out_frame = frame_handle };
            0
        }
        Err(failure) => failure.report("BNSqlite.Connection.Query"),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_sqlite_begin(handle: u64) -> i32 {
    match bn_core_sqlite::begin(handle) {
        Ok(()) => 0,
        Err(failure) => failure.report("BNSqlite.Connection.Begin"),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_sqlite_commit(handle: u64) -> i32 {
    match bn_core_sqlite::commit(handle) {
        Ok(()) => 0,
        Err(failure) => failure.report("BNSqlite.Connection.Commit"),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_sqlite_rollback(handle: u64) -> i32 {
    match bn_core_sqlite::rollback(handle) {
        Ok(()) => 0,
        Err(failure) => failure.report("BNSqlite.Connection.Rollback"),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_sqlite_changes(handle: u64) -> i64 {
    bn_core_sqlite::changes(handle).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_sqlite_last_insert_rowid(handle: u64) -> i64 {
    bn_core_sqlite::last_insert_rowid(handle).unwrap_or(0)
}
