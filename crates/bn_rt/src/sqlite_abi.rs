// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Stable C ABI and runtime backend for `BNSqlite` database operations.
#![allow(unsafe_code)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::borrow_as_ptr)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::needless_continue)]

use std::collections::HashMap;
use std::ffi::{CStr, c_char};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use rusqlite::ffi as libsqlite3_sys;
use rusqlite::{Connection, OpenFlags};

use super::dataframe::DataFrameColumn;
use super::dataframe_abi::{BNDataFrameHandle, StoredValue, register_frame_columns};
use super::sqlite_error::SqliteFailure;

struct ConnectionState {
    connection: Option<Connection>,
    _path: String,
    read_only: bool,
}

fn is_in_transaction(conn: &Connection) -> bool {
    let db_handle = unsafe { conn.handle() };
    unsafe { libsqlite3_sys::sqlite3_get_autocommit(db_handle) == 0 }
}

struct ConnectionRegistry {
    next: AtomicU64,
    connections: Mutex<HashMap<u64, ConnectionState>>,
}

impl ConnectionRegistry {
    fn lock(&self) -> MutexGuard<'_, HashMap<u64, ConnectionState>> {
        self.connections
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

fn registry() -> &'static ConnectionRegistry {
    static REGISTRY: OnceLock<ConnectionRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| ConnectionRegistry {
        next: AtomicU64::new(1),
        connections: Mutex::new(HashMap::new()),
    })
}

fn next_handle() -> u64 {
    registry().next.fetch_add(1, Ordering::Relaxed)
}

fn input_string(pointer: *const c_char) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .ok()
        .map(str::to_owned)
}

fn is_trailing_sql_empty(tail: &str) -> bool {
    let mut chars = tail.trim().chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            ';' | ' ' | '\t' | '\r' | '\n' => continue,
            '-' if chars.peek() == Some(&'-') => {
                chars.next();
                // Line comment: skip until newline
                for next_ch in chars.by_ref() {
                    if next_ch == '\n' {
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                // Block comment: skip until */
                let mut prev = ' ';
                for next_ch in chars.by_ref() {
                    if prev == '*' && next_ch == '/' {
                        break;
                    }
                    prev = next_ch;
                }
            }
            _ => return false,
        }
    }
    true
}

#[derive(Clone, Copy)]
enum OpenMode {
    CreateOrReadWrite,
    ReadOnly,
    ExistingReadWrite,
}

fn open_connection(path_str: &str, mode: OpenMode) -> Result<u64, SqliteFailure> {
    let is_memory = path_str == ":memory:";

    if is_memory {
        let conn = Connection::open_in_memory().map_err(SqliteFailure::from)?;
        conn.busy_timeout(std::time::Duration::from_millis(5000))
            .map_err(SqliteFailure::from)?;

        let handle = next_handle();
        let state = ConnectionState {
            connection: Some(conn),
            _path: path_str.to_owned(),
            read_only: false,
        };
        registry().lock().insert(handle, state);
        return Ok(handle);
    }

    let path = Path::new(path_str);
    if path_str.contains('\0')
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(SqliteFailure::PolicyDenied(
            "directory traversal (..) and null bytes are not permitted".to_owned(),
        ));
    }

    let write_required = !matches!(mode, OpenMode::ReadOnly);

    // Resolve canonical path
    let canonical_path = if path.exists() {
        path.canonicalize()
            .map_err(|e| SqliteFailure::IoFailed(e.to_string()))?
    } else {
        let parent = match path.parent() {
            Some(p) if !p.as_os_str().is_empty() => p,
            _ => Path::new("."),
        };
        let canonical_parent = parent.canonicalize().map_err(|e| {
            if parent.exists() {
                SqliteFailure::IoFailed(e.to_string())
            } else {
                SqliteFailure::FileNotFound(path_str.to_owned())
            }
        })?;
        let file_name = path.file_name().ok_or_else(|| {
            SqliteFailure::Misuse("invalid database path (missing file name)".into())
        })?;
        canonical_parent.join(file_name)
    };

    // Sandbox and execution policy enforcement on the target path
    let policy_allowed = super::policy::allows_path(&canonical_path, write_required);
    if !policy_allowed {
        let reason = super::policy::with_fs(|p| p.denial_reason(write_required));
        return Err(SqliteFailure::PolicyDenied(reason.to_string()));
    }

    // Sidecar check (-wal, -shm, -journal) to ensure derived files remain inside policy
    for suffix in &["-wal", "-shm", "-journal"] {
        let mut sidecar_name = canonical_path.as_os_str().to_os_string();
        sidecar_name.push(suffix);
        let sidecar_path = PathBuf::from(sidecar_name);
        if !super::policy::allows_path(&sidecar_path, write_required) {
            let reason = super::policy::with_fs(|p| p.denial_reason(write_required));
            return Err(SqliteFailure::PolicyDenied(reason.to_string()));
        }
    }

    // Check file existence when required
    match mode {
        OpenMode::ExistingReadWrite | OpenMode::ReadOnly => {
            if !canonical_path.exists() {
                return Err(SqliteFailure::FileNotFound(path_str.to_owned()));
            }
        }
        OpenMode::CreateOrReadWrite => {}
    }

    let flags = match mode {
        OpenMode::ReadOnly => OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        OpenMode::ExistingReadWrite => {
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX
        }
        OpenMode::CreateOrReadWrite => {
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
        }
    };

    let conn = Connection::open_with_flags(&canonical_path, flags).map_err(SqliteFailure::from)?;

    conn.busy_timeout(std::time::Duration::from_millis(5000))
        .map_err(SqliteFailure::from)?;

    let handle = next_handle();
    let state = ConnectionState {
        connection: Some(conn),
        _path: path_str.to_owned(),
        read_only: matches!(mode, OpenMode::ReadOnly),
    };
    registry().lock().insert(handle, state);
    Ok(handle)
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
            unsafe { *out_handle = handle };
            0
        }
        Err(failure) => failure.report("BNSqlite.OpenExisting"),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_sqlite_close(handle: u64) -> i32 {
    let mut table = registry().lock();
    let Some(state) = table.get_mut(&handle) else {
        return SqliteFailure::Closed.report("BNSqlite.Connection.Close");
    };
    if state.connection.take().is_none() {
        return SqliteFailure::Closed.report("BNSqlite.Connection.Close");
    }
    table.remove(&handle);
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_sqlite_exec(handle: u64, sql: *const c_char) -> i32 {
    let Some(sql_str) = input_string(sql) else {
        return SqliteFailure::Misuse("invalid null or non-UTF8 SQL string".into())
            .report("BNSqlite.Connection.Exec");
    };

    let mut table = registry().lock();
    let Some(state) = table.get_mut(&handle) else {
        return SqliteFailure::Closed.report("BNSqlite.Connection.Exec");
    };
    let Some(conn) = &mut state.connection else {
        return SqliteFailure::Closed.report("BNSqlite.Connection.Exec");
    };

    // Check single statement enforcement
    let db_handle = unsafe { conn.handle() };
    let mut raw_stmt: *mut libsqlite3_sys::sqlite3_stmt = std::ptr::null_mut();
    let mut raw_tail: *const c_char = std::ptr::null();
    let sql_c_str = match std::ffi::CString::new(sql_str.as_bytes()) {
        Ok(s) => s,
        Err(_) => {
            return SqliteFailure::SyntaxError("SQL string contains null byte".into())
                .report("BNSqlite.Connection.Exec");
        }
    };

    let rc = unsafe {
        libsqlite3_sys::sqlite3_prepare_v2(
            db_handle,
            sql_c_str.as_ptr(),
            -1,
            &mut raw_stmt,
            &mut raw_tail,
        )
    };

    if rc != libsqlite3_sys::SQLITE_OK {
        let err_msg = unsafe { CStr::from_ptr(libsqlite3_sys::sqlite3_errmsg(db_handle)) }
            .to_string_lossy()
            .into_owned();
        return SqliteFailure::from(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rc),
            Some(err_msg),
        ))
        .report("BNSqlite.Connection.Exec");
    }

    if raw_stmt.is_null() {
        // Empty statement string
        return 0;
    }

    // Verify trailing SQL tokens
    if !raw_tail.is_null() {
        let tail_str = unsafe { CStr::from_ptr(raw_tail) }.to_string_lossy();
        if !is_trailing_sql_empty(&tail_str) {
            unsafe { libsqlite3_sys::sqlite3_finalize(raw_stmt) };
            return SqliteFailure::Misuse(
                "multiple SQL statements in single call are prohibited".into(),
            )
            .report("BNSqlite.Connection.Exec");
        }
    }

    // Exec must not be row-producing
    let col_count = unsafe { libsqlite3_sys::sqlite3_column_count(raw_stmt) };
    if col_count > 0 {
        unsafe { libsqlite3_sys::sqlite3_finalize(raw_stmt) };
        return SqliteFailure::Misuse(
            "Exec cannot be called with row-producing query (SELECT); use Query".into(),
        )
        .report("BNSqlite.Connection.Exec");
    }

    // Check read-only constraint
    let is_stmt_readonly = unsafe { libsqlite3_sys::sqlite3_stmt_readonly(raw_stmt) } != 0;
    if state.read_only && !is_stmt_readonly {
        unsafe { libsqlite3_sys::sqlite3_finalize(raw_stmt) };
        return SqliteFailure::ReadOnly(
            "cannot execute mutative statement on read-only connection".into(),
        )
        .report("BNSqlite.Connection.Exec");
    }

    // Step statement to completion
    let step_rc = unsafe { libsqlite3_sys::sqlite3_step(raw_stmt) };
    unsafe { libsqlite3_sys::sqlite3_finalize(raw_stmt) };

    if step_rc != libsqlite3_sys::SQLITE_DONE && step_rc != libsqlite3_sys::SQLITE_ROW {
        let err_msg = unsafe { CStr::from_ptr(libsqlite3_sys::sqlite3_errmsg(db_handle)) }
            .to_string_lossy()
            .into_owned();
        return SqliteFailure::from(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(step_rc),
            Some(err_msg),
        ))
        .report("BNSqlite.Connection.Exec");
    }

    0
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

    let mut table = registry().lock();
    let Some(state) = table.get_mut(&handle) else {
        return SqliteFailure::Closed.report("BNSqlite.Connection.Query");
    };
    let Some(conn) = &mut state.connection else {
        return SqliteFailure::Closed.report("BNSqlite.Connection.Query");
    };

    // Check single statement enforcement
    let db_handle = unsafe { conn.handle() };
    let mut raw_stmt: *mut libsqlite3_sys::sqlite3_stmt = std::ptr::null_mut();
    let mut raw_tail: *const c_char = std::ptr::null();
    let sql_c_str = match std::ffi::CString::new(sql_str.as_bytes()) {
        Ok(s) => s,
        Err(_) => {
            return SqliteFailure::SyntaxError("SQL string contains null byte".into())
                .report("BNSqlite.Connection.Query");
        }
    };

    let rc = unsafe {
        libsqlite3_sys::sqlite3_prepare_v2(
            db_handle,
            sql_c_str.as_ptr(),
            -1,
            &mut raw_stmt,
            &mut raw_tail,
        )
    };

    if rc != libsqlite3_sys::SQLITE_OK {
        let err_msg = unsafe { CStr::from_ptr(libsqlite3_sys::sqlite3_errmsg(db_handle)) }
            .to_string_lossy()
            .into_owned();
        return SqliteFailure::from(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rc),
            Some(err_msg),
        ))
        .report("BNSqlite.Connection.Query");
    }

    if raw_stmt.is_null() {
        return SqliteFailure::Misuse("query string contains no executable SQL statement".into())
            .report("BNSqlite.Connection.Query");
    }

    // Verify trailing SQL tokens
    if !raw_tail.is_null() {
        let tail_str = unsafe { CStr::from_ptr(raw_tail) }.to_string_lossy();
        if !is_trailing_sql_empty(&tail_str) {
            unsafe { libsqlite3_sys::sqlite3_finalize(raw_stmt) };
            return SqliteFailure::Misuse(
                "multiple SQL statements in single call are prohibited".into(),
            )
            .report("BNSqlite.Connection.Query");
        }
    }

    // Query must be row-producing
    let col_count = unsafe { libsqlite3_sys::sqlite3_column_count(raw_stmt) };
    if col_count <= 0 {
        unsafe { libsqlite3_sys::sqlite3_finalize(raw_stmt) };
        return SqliteFailure::Misuse(
            "Query cannot be called with non-row statement (DDL/DML); use Exec".into(),
        )
        .report("BNSqlite.Connection.Query");
    }

    // Construct columns
    let col_count_usize = usize::try_from(col_count).unwrap_or(0);
    let mut columns: Vec<DataFrameColumn<StoredValue>> = Vec::with_capacity(col_count_usize);
    for i in 0..col_count {
        let name_ptr = unsafe { libsqlite3_sys::sqlite3_column_name(raw_stmt, i) };
        let col_name = if name_ptr.is_null() {
            format!("col_{i}")
        } else {
            unsafe { CStr::from_ptr(name_ptr) }
                .to_string_lossy()
                .into_owned()
        };
        columns.push(DataFrameColumn {
            name: col_name,
            values: Vec::new(),
        });
    }

    // Step through result rows
    loop {
        let step_rc = unsafe { libsqlite3_sys::sqlite3_step(raw_stmt) };
        if step_rc == libsqlite3_sys::SQLITE_DONE {
            break;
        }
        if step_rc == libsqlite3_sys::SQLITE_ROW {
            for (i, col) in columns.iter_mut().enumerate() {
                let col_idx = i as i32;
                let val_type = unsafe { libsqlite3_sys::sqlite3_column_type(raw_stmt, col_idx) };
                let val = match val_type {
                    libsqlite3_sys::SQLITE_INTEGER => {
                        let num =
                            unsafe { libsqlite3_sys::sqlite3_column_int64(raw_stmt, col_idx) };
                        StoredValue::Integer(num)
                    }
                    libsqlite3_sys::SQLITE_FLOAT => {
                        let num =
                            unsafe { libsqlite3_sys::sqlite3_column_double(raw_stmt, col_idx) };
                        StoredValue::Float(num)
                    }
                    libsqlite3_sys::SQLITE_TEXT => {
                        let ptr = unsafe { libsqlite3_sys::sqlite3_column_text(raw_stmt, col_idx) };
                        let raw_bytes =
                            unsafe { libsqlite3_sys::sqlite3_column_bytes(raw_stmt, col_idx) };
                        let bytes_count = usize::try_from(raw_bytes).unwrap_or(0);
                        if ptr.is_null() {
                            StoredValue::NotAvailable
                        } else {
                            let slice = unsafe { std::slice::from_raw_parts(ptr, bytes_count) };
                            StoredValue::String(slice.to_vec())
                        }
                    }
                    libsqlite3_sys::SQLITE_BLOB => {
                        let ptr = unsafe { libsqlite3_sys::sqlite3_column_blob(raw_stmt, col_idx) };
                        let raw_bytes =
                            unsafe { libsqlite3_sys::sqlite3_column_bytes(raw_stmt, col_idx) };
                        let bytes_count = usize::try_from(raw_bytes).unwrap_or(0);
                        if ptr.is_null() {
                            StoredValue::NotAvailable
                        } else {
                            let slice = unsafe {
                                std::slice::from_raw_parts(ptr.cast::<u8>(), bytes_count)
                            };
                            StoredValue::Bytes(slice.to_vec())
                        }
                    }
                    _ => StoredValue::NotAvailable,
                };
                col.values.push(val);
            }
        } else {
            let err_msg = unsafe { CStr::from_ptr(libsqlite3_sys::sqlite3_errmsg(db_handle)) }
                .to_string_lossy()
                .into_owned();
            unsafe { libsqlite3_sys::sqlite3_finalize(raw_stmt) };
            return SqliteFailure::from(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(step_rc),
                Some(err_msg),
            ))
            .report("BNSqlite.Connection.Query");
        }
    }

    unsafe { libsqlite3_sys::sqlite3_finalize(raw_stmt) };

    let frame_handle = register_frame_columns(columns);
    unsafe { *out_frame = frame_handle };
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_sqlite_begin(handle: u64) -> i32 {
    let mut table = registry().lock();
    let Some(state) = table.get_mut(&handle) else {
        return SqliteFailure::Closed.report("BNSqlite.Connection.Begin");
    };
    let Some(conn) = &mut state.connection else {
        return SqliteFailure::Closed.report("BNSqlite.Connection.Begin");
    };

    if is_in_transaction(conn) {
        return SqliteFailure::Misuse(
            "cannot begin transaction: transaction is already active".into(),
        )
        .report("BNSqlite.Connection.Begin");
    }

    if let Err(e) = conn.execute_batch("BEGIN IMMEDIATE;") {
        return SqliteFailure::from(e).report("BNSqlite.Connection.Begin");
    }

    0
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_sqlite_commit(handle: u64) -> i32 {
    let mut table = registry().lock();
    let Some(state) = table.get_mut(&handle) else {
        return SqliteFailure::Closed.report("BNSqlite.Connection.Commit");
    };
    let Some(conn) = &mut state.connection else {
        return SqliteFailure::Closed.report("BNSqlite.Connection.Commit");
    };

    if !is_in_transaction(conn) {
        return SqliteFailure::Misuse("cannot commit: no active transaction".into())
            .report("BNSqlite.Connection.Commit");
    }

    if let Err(e) = conn.execute_batch("COMMIT;") {
        return SqliteFailure::from(e).report("BNSqlite.Connection.Commit");
    }

    0
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_sqlite_rollback(handle: u64) -> i32 {
    let mut table = registry().lock();
    let Some(state) = table.get_mut(&handle) else {
        return SqliteFailure::Closed.report("BNSqlite.Connection.Rollback");
    };
    let Some(conn) = &mut state.connection else {
        return SqliteFailure::Closed.report("BNSqlite.Connection.Rollback");
    };

    if !is_in_transaction(conn) {
        return SqliteFailure::Misuse("cannot rollback: no active transaction".into())
            .report("BNSqlite.Connection.Rollback");
    }

    if let Err(e) = conn.execute_batch("ROLLBACK;") {
        return SqliteFailure::from(e).report("BNSqlite.Connection.Rollback");
    }

    0
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_sqlite_changes(handle: u64) -> i64 {
    let table = registry().lock();
    let Some(state) = table.get(&handle) else {
        return 0;
    };
    let Some(conn) = &state.connection else {
        return 0;
    };
    conn.changes().cast_signed()
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_sqlite_last_insert_rowid(handle: u64) -> i64 {
    let table = registry().lock();
    let Some(state) = table.get(&handle) else {
        return 0;
    };
    let Some(conn) = &state.connection else {
        return 0;
    };
    conn.last_insert_rowid()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataframe_abi::{
        bn_rt_dataframe_close, bn_rt_dataframe_column_count, bn_rt_dataframe_row_count,
    };
    use bn_types::error_codes::sqlite;
    use std::ffi::CString;

    #[test]
    fn in_memory_lifecycle_and_dataframe_query() {
        let mem = CString::new(":memory:").unwrap();
        let mut handle = 0u64;
        let rc = bn_rt_sqlite_open(mem.as_ptr(), &mut handle);
        assert_eq!(rc, 0);
        assert!(handle > 0);

        let ddl =
            CString::new("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, score REAL);")
                .unwrap();
        assert_eq!(bn_rt_sqlite_exec(handle, ddl.as_ptr()), 0);

        let insert1 =
            CString::new("INSERT INTO users (name, score) VALUES ('Alice', 95.5);").unwrap();
        assert_eq!(bn_rt_sqlite_exec(handle, insert1.as_ptr()), 0);
        assert_eq!(bn_rt_sqlite_changes(handle), 1);
        assert_eq!(bn_rt_sqlite_last_insert_rowid(handle), 1);

        let insert2 =
            CString::new("INSERT INTO users (name, score) VALUES ('Bob', 88.0);").unwrap();
        assert_eq!(bn_rt_sqlite_exec(handle, insert2.as_ptr()), 0);
        assert_eq!(bn_rt_sqlite_last_insert_rowid(handle), 2);

        let query = CString::new("SELECT id, name, score FROM users ORDER BY id;").unwrap();
        let mut frame = 0u64;
        assert_eq!(bn_rt_sqlite_query(handle, query.as_ptr(), &mut frame), 0);
        assert!(frame > 0);

        let mut row_count = 0u32;
        assert_eq!(bn_rt_dataframe_row_count(frame, &mut row_count), 0);
        assert_eq!(row_count, 2);

        let mut col_count = 0u32;
        assert_eq!(bn_rt_dataframe_column_count(frame, &mut col_count), 0);
        assert_eq!(col_count, 3);

        assert_eq!(bn_rt_dataframe_close(frame), 0);
        assert_eq!(bn_rt_sqlite_close(handle), 0);
    }

    #[test]
    fn transactions_atomic_commit_and_rollback() {
        let mem = CString::new(":memory:").unwrap();
        let mut handle = 0u64;
        assert_eq!(bn_rt_sqlite_open(mem.as_ptr(), &mut handle), 0);

        let ddl = CString::new("CREATE TABLE items (v INT);").unwrap();
        assert_eq!(bn_rt_sqlite_exec(handle, ddl.as_ptr()), 0);

        // Rollback test
        assert_eq!(bn_rt_sqlite_begin(handle), 0);
        let ins1 = CString::new("INSERT INTO items VALUES (10);").unwrap();
        assert_eq!(bn_rt_sqlite_exec(handle, ins1.as_ptr()), 0);
        assert_eq!(bn_rt_sqlite_rollback(handle), 0);

        let q = CString::new("SELECT v FROM items;").unwrap();
        let mut frame = 0u64;
        assert_eq!(bn_rt_sqlite_query(handle, q.as_ptr(), &mut frame), 0);
        let mut row_count = 0u32;
        assert_eq!(bn_rt_dataframe_row_count(frame, &mut row_count), 0);
        assert_eq!(row_count, 0);
        assert_eq!(bn_rt_dataframe_close(frame), 0);

        // Commit test
        assert_eq!(bn_rt_sqlite_begin(handle), 0);
        let ins2 = CString::new("INSERT INTO items VALUES (20);").unwrap();
        assert_eq!(bn_rt_sqlite_exec(handle, ins2.as_ptr()), 0);
        assert_eq!(bn_rt_sqlite_commit(handle), 0);

        assert_eq!(bn_rt_sqlite_query(handle, q.as_ptr(), &mut frame), 0);
        assert_eq!(bn_rt_dataframe_row_count(frame, &mut row_count), 0);
        assert_eq!(row_count, 1);
        assert_eq!(bn_rt_dataframe_close(frame), 0);

        assert_eq!(bn_rt_sqlite_close(handle), 0);
    }

    #[test]
    fn misuse_and_closed_guards() {
        let mem = CString::new(":memory:").unwrap();
        let mut handle = 0u64;
        assert_eq!(bn_rt_sqlite_open(mem.as_ptr(), &mut handle), 0);

        // Exec with SELECT must fail with MISUSE
        let select_stmt = CString::new("SELECT 1;").unwrap();
        assert_eq!(
            bn_rt_sqlite_exec(handle, select_stmt.as_ptr()),
            sqlite::MISUSE
        );

        // Query with DDL/DML must fail with MISUSE
        let mut frame = 0u64;
        let ddl_stmt = CString::new("CREATE TABLE t (id INT);").unwrap();
        assert_eq!(
            bn_rt_sqlite_query(handle, ddl_stmt.as_ptr(), &mut frame),
            sqlite::MISUSE
        );

        // Chained multi-statements must fail with MISUSE
        let chained = CString::new("CREATE TABLE t1 (id INT); CREATE TABLE t2 (id INT);").unwrap();
        assert_eq!(bn_rt_sqlite_exec(handle, chained.as_ptr()), sqlite::MISUSE);

        // Close connection
        assert_eq!(bn_rt_sqlite_close(handle), 0);

        // Post-close operations must fail with CLOSED
        assert_eq!(bn_rt_sqlite_exec(handle, ddl_stmt.as_ptr()), sqlite::CLOSED);
        assert_eq!(
            bn_rt_sqlite_query(handle, select_stmt.as_ptr(), &mut frame),
            sqlite::CLOSED
        );
        assert_eq!(bn_rt_sqlite_close(handle), sqlite::CLOSED);
    }

    #[test]
    fn open_existing_non_existent_file_fails_with_file_not_found() {
        let missing = CString::new("/tmp/non_existent_basicnext_db_12345.db").unwrap();
        let mut handle = 0u64;
        let rc = bn_rt_sqlite_open_existing(missing.as_ptr(), &mut handle);
        assert_eq!(rc, sqlite::FILE_NOT_FOUND);
    }

    #[test]
    fn large_integers_and_blob_storage() {
        use crate::dataframe_abi::{StoredValue, get_frame_columns};

        let mem = CString::new(":memory:").unwrap();
        let mut handle = 0u64;
        assert_eq!(bn_rt_sqlite_open(mem.as_ptr(), &mut handle), 0);

        let ddl = CString::new("CREATE TABLE items (id INTEGER, data BLOB);").unwrap();
        assert_eq!(bn_rt_sqlite_exec(handle, ddl.as_ptr()), 0);

        // 9_000_000_000 is > i32::MAX (2_147_483_647)
        let ins = CString::new("INSERT INTO items VALUES (9000000000, X'DEAD00BEEF');").unwrap();
        assert_eq!(bn_rt_sqlite_exec(handle, ins.as_ptr()), 0);

        let q = CString::new("SELECT id, data FROM items;").unwrap();
        let mut frame = 0u64;
        assert_eq!(bn_rt_sqlite_query(handle, q.as_ptr(), &mut frame), 0);

        let cols = get_frame_columns(frame).expect("frame columns");
        assert_eq!(cols.len(), 2);
        assert_eq!(cols[0].values, vec![StoredValue::Integer(9_000_000_000)]);
        assert_eq!(
            cols[1].values,
            vec![StoredValue::Bytes(vec![0xDE, 0xAD, 0x00, 0xBE, 0xEF])]
        );

        assert_eq!(bn_rt_dataframe_close(frame), 0);
        assert_eq!(bn_rt_sqlite_close(handle), 0);
    }

    #[test]
    fn transaction_autocommit_sync_with_manual_exec() {
        let mem = CString::new(":memory:").unwrap();
        let mut handle = 0u64;
        assert_eq!(bn_rt_sqlite_open(mem.as_ptr(), &mut handle), 0);

        let ddl = CString::new("CREATE TABLE kv (k INT, v TEXT);").unwrap();
        assert_eq!(bn_rt_sqlite_exec(handle, ddl.as_ptr()), 0);

        // Start transaction manually via Exec("BEGIN;")
        let begin_manual = CString::new("BEGIN;").unwrap();
        assert_eq!(bn_rt_sqlite_exec(handle, begin_manual.as_ptr()), 0);

        // Calling Begin() now should detect that transaction is already active and return MISUSE
        assert_eq!(bn_rt_sqlite_begin(handle), sqlite::MISUSE);

        // Commit() should recognize the active transaction and succeed
        assert_eq!(bn_rt_sqlite_commit(handle), 0);

        // Now commit again should fail because transaction is closed
        assert_eq!(bn_rt_sqlite_commit(handle), sqlite::MISUSE);

        // Begin() succeeds now
        assert_eq!(bn_rt_sqlite_begin(handle), 0);

        // Rollback manually via Exec("ROLLBACK;")
        let rollback_manual = CString::new("ROLLBACK;").unwrap();
        assert_eq!(bn_rt_sqlite_exec(handle, rollback_manual.as_ptr()), 0);

        // Rollback() should now fail because no transaction is active
        assert_eq!(bn_rt_sqlite_rollback(handle), sqlite::MISUSE);

        assert_eq!(bn_rt_sqlite_close(handle), 0);
    }

    #[test]
    fn sqlite_sandbox_symlink_escape_is_rejected() {
        #[cfg(unix)]
        {
            let _policy = crate::policy::reset_for_tests();
            let base =
                std::env::temp_dir().join(format!("bn-sqlite-escape-{}", std::process::id()));
            let root = base.join("sandbox_root");
            let outside = base.join("outside_dir");
            std::fs::create_dir_all(&root).unwrap();
            std::fs::create_dir_all(&outside).unwrap();

            assert_eq!(crate::policy::bn_rt_policy_filesystem_sandboxed(), 0);
            let root_name = CString::new(root.to_string_lossy().as_bytes()).unwrap();
            assert_eq!(
                crate::policy::bn_rt_policy_filesystem_root(1, root_name.as_ptr()),
                0
            );

            // Create a symlink inside root pointing outside
            let symlink_path = root.join("escape_link");
            std::os::unix::fs::symlink(&outside, &symlink_path).unwrap();

            // Attempt to open SQLite DB through the symlink to outside
            let db_escape_path = symlink_path.join("target.db");
            let c_path = CString::new(db_escape_path.to_string_lossy().as_bytes()).unwrap();
            let mut handle = 0u64;
            let rc = bn_rt_sqlite_open(c_path.as_ptr(), &mut handle);
            assert_eq!(rc, sqlite::POLICY_DENIED);

            // Clean up
            let _ = std::fs::remove_dir_all(base);
        }
    }

    #[test]
    fn mutex_poison_recovery_in_connection_registry() {
        // Deliberately poison the registry mutex in a worker thread
        let _ = std::thread::spawn(|| {
            let _guard = registry().connections.lock().unwrap();
            panic!("intentional panic to poison mutex");
        })
        .join();

        assert!(registry().connections.is_poisoned());

        // Calling registry().lock() and opening connection should recover cleanly
        let mem = CString::new(":memory:").unwrap();
        let mut handle = 0u64;
        assert_eq!(bn_rt_sqlite_open(mem.as_ptr(), &mut handle), 0);
        assert_eq!(bn_rt_sqlite_close(handle), 0);
    }

    #[test]
    fn query_rejects_non_row_statement_safely() {
        let mem = CString::new(":memory:").unwrap();
        let mut handle = 0u64;
        assert_eq!(bn_rt_sqlite_open(mem.as_ptr(), &mut handle), 0);

        let ddl = CString::new("CREATE TABLE t (x INT)").unwrap();
        let mut df_handle = 0u64;
        let rc = bn_rt_sqlite_query(handle, ddl.as_ptr(), &mut df_handle);
        assert_ne!(rc, 0, "Query on non-row statement must fail safely");
        assert_eq!(df_handle, 0);

        assert_eq!(bn_rt_sqlite_close(handle), 0);
    }
}
