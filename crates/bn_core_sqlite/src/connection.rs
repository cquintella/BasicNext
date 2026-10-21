// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Thread-safe SQLite connection registry and synchronized operations.

use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};
use std::time::Duration;

use rusqlite::ffi as libsqlite3_sys;
use rusqlite::{Connection, OpenFlags};

use super::error::SqliteFailure;
use super::file_id::{FileIdentity, is_symlink_or_reparse_point};
use super::types::{OpenMode, SqliteColumn, SqliteQueryResult, SqliteValue};

/// Default busy timeout for database connection locks (5 seconds).
const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_millis(5000);
fn sqlite_text(key: &str) -> String {
    bn_catalog::text("sqlite", key).to_owned()
}

#[derive(Debug)]
pub(crate) struct ConnectionInner {
    connection: Option<Connection>,
}

fn is_in_transaction(conn: &Connection) -> bool {
    // SAFETY: Obtaining raw SQLite handle from active Connection to query autocommit state.
    let db_handle = unsafe { conn.handle() };
    unsafe { libsqlite3_sys::sqlite3_get_autocommit(db_handle) == 0 }
}

#[derive(Debug)]
pub struct ConnectionState {
    inner: Mutex<ConnectionInner>,
    pub path: String,
    pub read_only: bool,
}

impl ConnectionState {
    fn lock_inner(&self) -> MutexGuard<'_, ConnectionInner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The connections of the module copy that runs on this thread
/// (`bn_core_copy`): a task's close when the task ends, rolling back a
/// transaction it left open.
#[derive(Debug)]
pub(crate) struct ConnectionRegistry {
    next: AtomicU64,
    connections: RwLock<HashMap<u64, Arc<ConnectionState>>>,
    holders: RwLock<HashMap<u64, usize>>,
}

impl Default for ConnectionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionRegistry {
    fn new() -> Self {
        Self {
            next: AtomicU64::new(1),
            connections: RwLock::new(HashMap::new()),
            holders: RwLock::new(HashMap::new()),
        }
    }

    fn next_handle(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }

    fn get(&self, handle: u64) -> Option<Arc<ConnectionState>> {
        self.connections
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&handle)
            .cloned()
    }
}

fn registry() -> Arc<ConnectionRegistry> {
    bn_core_copy::table::<ConnectionRegistry>()
}

fn is_trailing_sql_empty(tail: &str) -> bool {
    let mut chars = tail.trim().chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            ';' | ' ' | '\t' | '\r' | '\n' => continue,
            '-' if chars.peek() == Some(&'-') => {
                chars.next();
                for next_ch in chars.by_ref() {
                    if next_ch == '\n' {
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
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

pub fn open_connection<F>(
    path_str: &str,
    mode: OpenMode,
    policy_check: F,
) -> Result<u64, SqliteFailure>
where
    F: Fn(&Path, bool) -> Result<(), String>,
{
    let path = Path::new(path_str);
    let write_required = !matches!(mode, OpenMode::ReadOnly);

    let is_in_memory = path_str == ":memory:";
    if !is_in_memory
        && (path_str.contains('\0')
            || path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir)))
    {
        return Err(SqliteFailure::PolicyDenied(sqlite_text(
            "error.path_traversal",
        )));
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

    let conn = if is_in_memory {
        Connection::open_in_memory_with_flags(flags).map_err(SqliteFailure::from)?
    } else {
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
            let file_name = path
                .file_name()
                .ok_or_else(|| SqliteFailure::Misuse(sqlite_text("error.missing_file_name")))?;
            canonical_parent.join(file_name)
        };

        policy_check(&canonical_path, write_required).map_err(SqliteFailure::PolicyDenied)?;

        for suffix in &["-wal", "-shm", "-journal"] {
            let mut sidecar_name = canonical_path.as_os_str().to_os_string();
            sidecar_name.push(suffix);
            let sidecar_path = PathBuf::from(sidecar_name);
            policy_check(&sidecar_path, write_required).map_err(SqliteFailure::PolicyDenied)?;
        }

        match mode {
            OpenMode::ExistingReadWrite | OpenMode::ReadOnly => {
                if !canonical_path.exists() {
                    return Err(SqliteFailure::FileNotFound(path_str.to_owned()));
                }
            }
            OpenMode::CreateOrReadWrite => {}
        }

        if is_symlink_or_reparse_point(&canonical_path) {
            return Err(SqliteFailure::PolicyDenied(sqlite_text(
                "error.concurrent_mutation",
            )));
        }

        let pre_meta = if canonical_path.exists() {
            FileIdentity::from_path(&canonical_path)
        } else {
            None
        };

        let c = Connection::open_with_flags(&canonical_path, flags).map_err(SqliteFailure::from)?;

        if let Some(expected_identity) = pre_meta {
            if let Some(post_identity) = FileIdentity::from_path(&canonical_path) {
                if expected_identity != post_identity {
                    return Err(SqliteFailure::PolicyDenied(sqlite_text(
                        "error.concurrent_mutation",
                    )));
                }
            } else {
                return Err(SqliteFailure::PolicyDenied(sqlite_text(
                    "error.concurrent_mutation",
                )));
            }
        }
        c
    };

    conn.busy_timeout(SQLITE_BUSY_TIMEOUT)
        .map_err(SqliteFailure::from)?;

    let handle = registry().next_handle();
    let state = Arc::new(ConnectionState {
        inner: Mutex::new(ConnectionInner {
            connection: Some(conn),
        }),
        path: path_str.to_owned(),
        read_only: matches!(mode, OpenMode::ReadOnly),
    });

    registry()
        .connections
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(handle, state);

    registry()
        .holders
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(handle, 1);

    Ok(handle)
}

/// One more binding holds the connection behind `handle` (a copy).
pub fn retain(handle: u64) -> bool {
    let reg = registry();
    let mut holders = reg.holders.write().unwrap_or_else(PoisonError::into_inner);
    if let Some(count) = holders.get_mut(&handle) {
        *count += 1;
        true
    } else {
        false
    }
}

pub fn close(handle: u64) -> Result<(), SqliteFailure> {
    let reg = registry();
    let mut holders = reg.holders.write().unwrap_or_else(PoisonError::into_inner);
    let Some(count) = holders.get_mut(&handle) else {
        return Err(SqliteFailure::Closed);
    };
    *count -= 1;
    if *count > 0 {
        return Ok(());
    }
    holders.remove(&handle);
    drop(holders);

    let state = reg
        .connections
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&handle);

    let Some(state) = state else {
        return Err(SqliteFailure::Closed);
    };

    let mut inner = state.lock_inner();
    if inner.connection.take().is_none() {
        return Err(SqliteFailure::Closed);
    }
    Ok(())
}

pub fn exec(handle: u64, sql_str: &str) -> Result<(), SqliteFailure> {
    let state = registry().get(handle).ok_or(SqliteFailure::Closed)?;
    let mut inner = state.lock_inner();
    let conn = inner.connection.as_mut().ok_or(SqliteFailure::Closed)?;

    let sql_c_str = CString::new(sql_str.as_bytes())
        .map_err(|_| SqliteFailure::SyntaxError(sqlite_text("error.sql_nul")))?;

    // SAFETY: Preparing statement on active, locked connection handle.
    let db_handle = unsafe { conn.handle() };
    let mut raw_stmt: *mut libsqlite3_sys::sqlite3_stmt = std::ptr::null_mut();
    let mut raw_tail: *const c_char = std::ptr::null();

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
        // SAFETY: sqlite3_errmsg is valid when sqlite3_prepare_v2 returns non-OK.
        let err_msg = unsafe { CStr::from_ptr(libsqlite3_sys::sqlite3_errmsg(db_handle)) }
            .to_string_lossy()
            .into_owned();
        return Err(SqliteFailure::from(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rc),
            Some(err_msg),
        )));
    }

    if raw_stmt.is_null() {
        return Ok(());
    }

    // SAFETY: Checking trailing statement tokens.
    if !raw_tail.is_null() {
        let tail_str = unsafe { CStr::from_ptr(raw_tail) }.to_string_lossy();
        if !is_trailing_sql_empty(&tail_str) {
            unsafe { libsqlite3_sys::sqlite3_finalize(raw_stmt) };
            return Err(SqliteFailure::Misuse(sqlite_text(
                "error.multiple_statements",
            )));
        }
    }

    // SAFETY: Verify statement is not row-producing.
    let col_count = unsafe { libsqlite3_sys::sqlite3_column_count(raw_stmt) };
    if col_count > 0 {
        unsafe { libsqlite3_sys::sqlite3_finalize(raw_stmt) };
        return Err(SqliteFailure::Misuse(sqlite_text(
            "error.exec_returns_rows",
        )));
    }

    // SAFETY: Verify read-only constraint.
    let is_stmt_readonly = unsafe { libsqlite3_sys::sqlite3_stmt_readonly(raw_stmt) } != 0;
    if state.read_only && !is_stmt_readonly {
        unsafe { libsqlite3_sys::sqlite3_finalize(raw_stmt) };
        return Err(SqliteFailure::ReadOnly(sqlite_text(
            "error.read_only_mutation",
        )));
    }

    // SAFETY: Stepping statement and finalizing.
    let step_rc = unsafe { libsqlite3_sys::sqlite3_step(raw_stmt) };
    unsafe { libsqlite3_sys::sqlite3_finalize(raw_stmt) };

    if step_rc != libsqlite3_sys::SQLITE_DONE && step_rc != libsqlite3_sys::SQLITE_ROW {
        let err_msg = unsafe { CStr::from_ptr(libsqlite3_sys::sqlite3_errmsg(db_handle)) }
            .to_string_lossy()
            .into_owned();
        return Err(SqliteFailure::from(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(step_rc),
            Some(err_msg),
        )));
    }

    Ok(())
}

pub fn query(handle: u64, sql_str: &str) -> Result<SqliteQueryResult, SqliteFailure> {
    let state = registry().get(handle).ok_or(SqliteFailure::Closed)?;
    let mut inner = state.lock_inner();
    let conn = inner.connection.as_mut().ok_or(SqliteFailure::Closed)?;

    let sql_c_str = CString::new(sql_str.as_bytes())
        .map_err(|_| SqliteFailure::SyntaxError(sqlite_text("error.sql_nul")))?;

    // SAFETY: Preparing query on active, locked connection handle.
    let db_handle = unsafe { conn.handle() };
    let mut raw_stmt: *mut libsqlite3_sys::sqlite3_stmt = std::ptr::null_mut();
    let mut raw_tail: *const c_char = std::ptr::null();

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
        // SAFETY: the pointer is non-null and NUL-terminated for this read.
        let err_msg = unsafe { CStr::from_ptr(libsqlite3_sys::sqlite3_errmsg(db_handle)) }
            .to_string_lossy()
            .into_owned();
        return Err(SqliteFailure::from(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rc),
            Some(err_msg),
        )));
    }

    if raw_stmt.is_null() {
        return Err(SqliteFailure::Misuse(sqlite_text("error.query_empty")));
    }

    // SAFETY: Checking trailing statement tokens.
    if !raw_tail.is_null() {
        let tail_str = unsafe { CStr::from_ptr(raw_tail) }.to_string_lossy();
        if !is_trailing_sql_empty(&tail_str) {
            unsafe { libsqlite3_sys::sqlite3_finalize(raw_stmt) };
            return Err(SqliteFailure::Misuse(sqlite_text(
                "error.multiple_statements",
            )));
        }
    }

    // SAFETY: Verify query produces rows.
    let col_count = unsafe { libsqlite3_sys::sqlite3_column_count(raw_stmt) };
    if col_count <= 0 {
        unsafe { libsqlite3_sys::sqlite3_finalize(raw_stmt) };
        return Err(SqliteFailure::Misuse(sqlite_text("error.query_not_rows")));
    }

    let col_count_usize = usize::try_from(col_count).unwrap_or(0);
    let mut columns: Vec<SqliteColumn> = Vec::with_capacity(col_count_usize);
    for i in 0..col_count {
        // SAFETY: Reading column name from prepared statement.
        let name_ptr = unsafe { libsqlite3_sys::sqlite3_column_name(raw_stmt, i) };
        let col_name = if name_ptr.is_null() {
            format!("col_{i}")
        } else {
            unsafe { CStr::from_ptr(name_ptr) }
                .to_string_lossy()
                .into_owned()
        };
        columns.push(SqliteColumn {
            name: col_name,
            values: Vec::new(),
        });
    }

    loop {
        // SAFETY: Stepping through statement rows.
        let step_rc = unsafe { libsqlite3_sys::sqlite3_step(raw_stmt) };
        if step_rc == libsqlite3_sys::SQLITE_DONE {
            break;
        }
        if step_rc == libsqlite3_sys::SQLITE_ROW {
            for (i, col) in columns.iter_mut().enumerate() {
                let col_idx = i as i32;
                // SAFETY: Extracting column values by column index.
                let val_type = unsafe { libsqlite3_sys::sqlite3_column_type(raw_stmt, col_idx) };
                let val = match val_type {
                    libsqlite3_sys::SQLITE_INTEGER => {
                        let num =
                            unsafe { libsqlite3_sys::sqlite3_column_int64(raw_stmt, col_idx) };
                        SqliteValue::Integer(num)
                    }
                    libsqlite3_sys::SQLITE_FLOAT => {
                        let num =
                            // SAFETY: `raw_stmt` is live until `sqlite3_finalize`, and `col_idx` is in range.
                            unsafe { libsqlite3_sys::sqlite3_column_double(raw_stmt, col_idx) };
                        SqliteValue::Float(num)
                    }
                    libsqlite3_sys::SQLITE_TEXT => {
                        let raw_bytes =
                            // SAFETY: `raw_stmt` is live until `sqlite3_finalize`, and `col_idx` is in range.
                            unsafe { libsqlite3_sys::sqlite3_column_bytes(raw_stmt, col_idx) };
                        let bytes_count = usize::try_from(raw_bytes).unwrap_or(0);
                        if bytes_count == 0 {
                            SqliteValue::Text(Vec::new())
                        } else {
                            let ptr =
                                // SAFETY: SQLite keeps this column pointer valid until the next step or finalize.
                                unsafe { libsqlite3_sys::sqlite3_column_text(raw_stmt, col_idx) };
                            if ptr.is_null() {
                                SqliteValue::NotAvailable
                            } else {
                                // SAFETY: SQLite keeps this column pointer valid until the next step or finalize.
                                let slice = unsafe { std::slice::from_raw_parts(ptr, bytes_count) };
                                SqliteValue::Text(slice.to_vec())
                            }
                        }
                    }
                    libsqlite3_sys::SQLITE_BLOB => {
                        let raw_bytes =
                            // SAFETY: `raw_stmt` is live until `sqlite3_finalize`, and `col_idx` is in range.
                            unsafe { libsqlite3_sys::sqlite3_column_bytes(raw_stmt, col_idx) };
                        let bytes_count = usize::try_from(raw_bytes).unwrap_or(0);
                        if bytes_count == 0 {
                            SqliteValue::Blob(Vec::new())
                        } else {
                            let ptr =
                                // SAFETY: SQLite keeps this column pointer valid until the next step or finalize.
                                unsafe { libsqlite3_sys::sqlite3_column_blob(raw_stmt, col_idx) };
                            if ptr.is_null() {
                                SqliteValue::NotAvailable
                            } else {
                                // SAFETY: SQLite keeps this column pointer valid until the next step or finalize.
                                let slice = unsafe {
                                    std::slice::from_raw_parts(ptr.cast::<u8>(), bytes_count)
                                };
                                SqliteValue::Blob(slice.to_vec())
                            }
                        }
                    }
                    libsqlite3_sys::SQLITE_NULL => SqliteValue::Null,
                    _ => SqliteValue::NotAvailable,
                };
                col.values.push(val);
            }
        } else {
            // SAFETY: the pointer is non-null and NUL-terminated for this read.
            let err_msg = unsafe { CStr::from_ptr(libsqlite3_sys::sqlite3_errmsg(db_handle)) }
                .to_string_lossy()
                .into_owned();
            // SAFETY: `raw_stmt` is the statement this function prepared and has not finalized yet.
            unsafe { libsqlite3_sys::sqlite3_finalize(raw_stmt) };
            return Err(SqliteFailure::from(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(step_rc),
                Some(err_msg),
            )));
        }
    }

    // SAFETY: Finalizing statement.
    unsafe { libsqlite3_sys::sqlite3_finalize(raw_stmt) };
    Ok(SqliteQueryResult { columns })
}

pub fn begin(handle: u64) -> Result<(), SqliteFailure> {
    let state = registry().get(handle).ok_or(SqliteFailure::Closed)?;
    let mut inner = state.lock_inner();
    let conn = inner.connection.as_mut().ok_or(SqliteFailure::Closed)?;

    if is_in_transaction(conn) {
        return Err(SqliteFailure::Misuse(sqlite_text("error.begin_active")));
    }

    conn.execute_batch(bn_catalog::text("sqlite", "sql.begin_immediate"))
        .map_err(SqliteFailure::from)
}

pub fn commit(handle: u64) -> Result<(), SqliteFailure> {
    let state = registry().get(handle).ok_or(SqliteFailure::Closed)?;
    let mut inner = state.lock_inner();
    let conn = inner.connection.as_mut().ok_or(SqliteFailure::Closed)?;

    if !is_in_transaction(conn) {
        return Err(SqliteFailure::Misuse(sqlite_text("error.commit_inactive")));
    }

    conn.execute_batch("COMMIT;").map_err(SqliteFailure::from)
}

pub fn rollback(handle: u64) -> Result<(), SqliteFailure> {
    let state = registry().get(handle).ok_or(SqliteFailure::Closed)?;
    let mut inner = state.lock_inner();
    let conn = inner.connection.as_mut().ok_or(SqliteFailure::Closed)?;

    if !is_in_transaction(conn) {
        return Err(SqliteFailure::Misuse(sqlite_text(
            "error.rollback_inactive",
        )));
    }

    conn.execute_batch("ROLLBACK;").map_err(SqliteFailure::from)
}

pub fn changes(handle: u64) -> Result<i64, SqliteFailure> {
    let state = registry().get(handle).ok_or(SqliteFailure::Closed)?;
    let mut inner = state.lock_inner();
    let conn = inner.connection.as_mut().ok_or(SqliteFailure::Closed)?;

    // SAFETY: Reading changes count from SQLite handle.
    let db_handle = unsafe { conn.handle() };
    let count = unsafe { libsqlite3_sys::sqlite3_changes64(db_handle) };
    Ok(count)
}

pub fn last_insert_rowid(handle: u64) -> Result<i64, SqliteFailure> {
    let state = registry().get(handle).ok_or(SqliteFailure::Closed)?;
    let mut inner = state.lock_inner();
    let conn = inner.connection.as_mut().ok_or(SqliteFailure::Closed)?;

    // SAFETY: Reading last insert rowid from SQLite handle.
    let db_handle = unsafe { conn.handle() };
    let rowid = unsafe { libsqlite3_sys::sqlite3_last_insert_rowid(db_handle) };
    Ok(rowid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::unnecessary_wraps)]
    fn no_policy(_: &Path, _: bool) -> Result<(), String> {
        Ok(())
    }

    /// A connection belongs to the module copy that opened it (`0.6.md`,
    /// "Isolation and conformance"): another thread's copy does not see it,
    /// and the end of a task's copy closes what it left open.
    #[test]
    fn a_connection_belongs_to_the_copy_that_opened_it() {
        let handle = open_connection(":memory:", OpenMode::CreateOrReadWrite, no_policy)
            .expect("open memory db");
        exec(handle, "CREATE TABLE items (id INTEGER PRIMARY KEY);").expect("create table");
        let elsewhere = std::thread::spawn(move || exec(handle, "SELECT 1;"))
            .join()
            .expect("thread join");
        assert_eq!(elsewhere.unwrap_err(), SqliteFailure::Closed);

        close(handle).expect("close");
        assert_eq!(
            exec(handle, "SELECT 1;").unwrap_err(),
            SqliteFailure::Closed
        );
    }

    /// A task that leaves a transaction open in a connection it keeps ends
    /// with its copy: the connection closes, the transaction rolls back, and
    /// the database is free for the program.
    #[test]
    fn the_end_of_a_task_closes_its_connections() {
        let path =
            std::env::temp_dir().join(format!("bn-core-sqlite-task-end-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let name = path.to_str().expect("temporary path is UTF-8").to_owned();
        let program = open_connection(&name, OpenMode::CreateOrReadWrite, no_policy)
            .expect("open in the program");
        exec(program, "CREATE TABLE items (id INTEGER);").expect("create table");
        bn_core_copy::run(|| {
            let task = open_connection(&name, OpenMode::CreateOrReadWrite, no_policy)
                .expect("open in the task");
            begin(task).expect("begin");
            exec(task, "INSERT INTO items VALUES (1);").expect("insert in the task");
        });
        exec(program, "INSERT INTO items VALUES (2);").expect("the database is free");
        let result = query(program, "SELECT COUNT(*) FROM items;").expect("count");
        assert_eq!(result.columns[0].values[0], SqliteValue::Integer(1));
        close(program).expect("close");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn concurrent_independent_connections_run_in_parallel() {
        let mut threads = Vec::new();
        for thread_idx in 0..4 {
            threads.push(std::thread::spawn(move || {
                let handle = open_connection(":memory:", OpenMode::CreateOrReadWrite, no_policy)
                    .expect("open memory db");

                let create_sql =
                    format!("CREATE TABLE logs_{thread_idx} (id INTEGER PRIMARY KEY, msg TEXT);");
                exec(handle, &create_sql).expect("create table");

                for i in 0..20 {
                    let sql = format!(
                        "INSERT INTO logs_{thread_idx} (msg) VALUES ('test-{thread_idx}-{i}');"
                    );
                    exec(handle, &sql).expect("insert");
                }

                let query_sql = format!("SELECT COUNT(*) FROM logs_{thread_idx};");
                let result = query(handle, &query_sql).expect("select count");
                assert_eq!(result.columns[0].values[0], SqliteValue::Integer(20));

                close(handle).expect("close");
            }));
        }

        for thread in threads {
            thread.join().expect("thread join");
        }
    }
}
