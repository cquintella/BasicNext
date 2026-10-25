// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Interpreter adapter for `BNSqlite`.
//!
//! Calls `bn_core_sqlite` directly. The compiled program uses the same crate
//! through `bn_rt::sqlite_abi`; this adapter does not.

#![allow(clippy::similar_names)]

use std::collections::HashMap;
use std::ffi::CString;

use bn_core_sqlite::{
    OpenMode, SqliteFailure, SqliteValue, begin, changes, close, commit, exec, last_insert_rowid,
    open_connection, query, rollback,
};
use bn_diag::Diagnostic;
use bn_interp::provider::{CoreContext, Provider};
use bn_interp::{name_not_found, require_arity_pub as require_arity, type_mismatch};
use bn_lib_data::DataProvider;
use bn_runtime::Handle;
use bn_source::Span;
use bn_stdlib::{DataFrameColumn, DataFrameResource};
use bn_types::error_codes::sqlite as sqlite_err;
use bn_types::{FloatType, IntegerType};
use bn_value::{Value, shared_string};

pub const NAME: &str = "BNSqlite";

#[derive(Debug, Default)]
pub struct SqliteProvider {
    connections: HashMap<Handle, u64>,
}

impl Provider for SqliteProvider {
    fn call(
        &mut self,
        core: &mut dyn CoreContext,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let name = format!("{NAME}.{member}");
        if matches!(member.rsplit('.').next(), Some("CONSTRUCTOR" | "$fields")) {
            return Ok(Value::Null);
        }
        self.dispatch_call(core, &name, &arguments, span)
    }

    fn object_destroyed(&mut self, handle: Handle) {
        if let Some(db_handle) = self.connections.remove(&handle) {
            let _ = close(db_handle);
        }
    }

    fn close_all(&mut self) {
        for db_handle in self.connections.values() {
            let _ = close(*db_handle);
        }
        self.connections.clear();
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

impl SqliteProvider {
    fn dispatch_call(
        &mut self,
        core: &mut dyn CoreContext,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let op = name.rsplit('.').next().unwrap_or_default();
        match op {
            "Open" => self.open_call(core, name, arguments, span, OpenMode::CreateOrReadWrite),
            "OpenReadOnly" => self.open_call(core, name, arguments, span, OpenMode::ReadOnly),
            "OpenExisting" => {
                self.open_call(core, name, arguments, span, OpenMode::ExistingReadWrite)
            }
            "Exec" => self.exec_call(name, arguments, span),
            "Query" => self.query_call(core, name, arguments, span),
            "Begin" => self.begin_call(name, arguments, span),
            "Commit" => self.commit_call(name, arguments, span),
            "Rollback" => self.rollback_call(name, arguments, span),
            "Changes" => self.changes_call(name, arguments, span),
            "LastInsertRowId" => self.last_insert_rowid_call(name, arguments, span),
            "Close" => self.close_call(name, arguments, span),
            _ => Err(name_not_found(name, "BNSqlite member", span)),
        }
    }

    fn open_call(
        &mut self,
        core: &mut dyn CoreContext,
        name: &str,
        arguments: &[Value],
        span: Span,
        mode: OpenMode,
    ) -> Result<Value, Diagnostic> {
        require_arity(name, arguments, 1, span)?;
        let Value::String(path) = &arguments[0] else {
            return Err(type_mismatch("STRING", "non-STRING path", name, span));
        };
        if let Some(value) = interior_nul(
            name,
            path.as_ref(),
            "Database path contains interior NUL byte",
        ) {
            return Ok(value);
        }

        let opened = open_with_policy(core, path.as_ref(), mode);
        let db_handle = match opened {
            Ok(handle) => handle,
            Err(failure) => return Ok(failure_value(name, &failure)),
        };

        let object = core
            .allocate_object("BNSqlite.Connection", span)
            .inspect_err(|_| {
                let _ = close(db_handle);
            })?;
        let Value::Object { handle, .. } = object else {
            unreachable!("allocate_object returns object");
        };
        self.connections.insert(handle, db_handle);
        Ok(object)
    }

    fn resolve_handle(
        &self,
        name: &str,
        receiver: &Value,
        span: Span,
    ) -> Result<Result<u64, Value>, Diagnostic> {
        let Value::Object { handle, .. } = receiver else {
            return Err(type_mismatch(
                "BNSqlite.Connection",
                "non-object receiver",
                name,
                span,
            ));
        };
        if let Some(&db_handle) = self.connections.get(handle) {
            Ok(Ok(db_handle))
        } else {
            Ok(Err(Value::error_report(
                sqlite_err::CLOSED,
                name,
                "Connection is closed or invalid".to_string(),
                "handle not found in connection table".to_string(),
            )))
        }
    }

    fn exec_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(name, arguments, 2, span)?;
        let db_handle = match self.resolve_handle(name, &arguments[0], span)? {
            Ok(handle) => handle,
            Err(err) => return Ok(err),
        };
        let Value::String(sql) = &arguments[1] else {
            return Err(type_mismatch("STRING", "non-STRING SQL query", name, span));
        };
        if let Some(value) = interior_nul(
            name,
            sql.as_ref(),
            "SQL statement contains interior NUL byte",
        ) {
            return Ok(value);
        }
        match exec(db_handle, sql.as_ref()) {
            Ok(()) => Ok(Value::Null),
            Err(failure) => Ok(failure_value(name, &failure)),
        }
    }

    fn query_call(
        &mut self,
        core: &mut dyn CoreContext,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(name, arguments, 2, span)?;
        let db_handle = match self.resolve_handle(name, &arguments[0], span)? {
            Ok(handle) => handle,
            Err(err) => return Ok(err),
        };
        let Value::String(sql) = &arguments[1] else {
            return Err(type_mismatch("STRING", "non-STRING SQL query", name, span));
        };
        if let Some(value) = interior_nul(
            name,
            sql.as_ref(),
            "SQL statement contains interior NUL byte",
        ) {
            return Ok(value);
        }

        let columns = match query(db_handle, sql.as_ref()) {
            Ok(result) => result.columns,
            Err(failure) => return Ok(failure_value(name, &failure)),
        };
        let resource = DataFrameResource {
            columns: columns
                .into_iter()
                .map(|column| DataFrameColumn {
                    name: column.name,
                    values: column.values.into_iter().map(sqlite_cell).collect(),
                })
                .collect(),
        };

        let mut data_box = core.library_take("BNData");
        let Some(mut data_provider) = data_box.take() else {
            return Ok(Value::error_report(
                sqlite_err::INTERNAL_ERROR,
                name,
                "BNData provider is unavailable".to_string(),
                "missing BNData provider".to_string(),
            ));
        };

        let Some(prov) = data_provider
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<DataProvider>())
        else {
            core.library_insert("BNData", data_provider);
            return Ok(Value::error_report(
                sqlite_err::INTERNAL_ERROR,
                name,
                "Failed to downcast BNData provider".to_string(),
                "downcast error".to_string(),
            ));
        };

        let frame_id = prov.insert_frame(resource);
        core.library_insert("BNData", data_provider);
        Ok(Value::DataFrame(frame_id))
    }

    fn begin_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        self.status_call(name, arguments, span, begin)
    }

    fn commit_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        self.status_call(name, arguments, span, commit)
    }

    fn rollback_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        self.status_call(name, arguments, span, rollback)
    }

    fn status_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
        operation: fn(u64) -> Result<(), SqliteFailure>,
    ) -> Result<Value, Diagnostic> {
        require_arity(name, arguments, 1, span)?;
        let db_handle = match self.resolve_handle(name, &arguments[0], span)? {
            Ok(handle) => handle,
            Err(err) => return Ok(err),
        };
        match operation(db_handle) {
            Ok(()) => Ok(Value::Null),
            Err(failure) => Ok(failure_value(name, &failure)),
        }
    }

    fn changes_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        self.integer_call(name, arguments, span, changes)
    }

    fn last_insert_rowid_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        self.integer_call(name, arguments, span, last_insert_rowid)
    }

    fn integer_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
        operation: fn(u64) -> Result<i64, SqliteFailure>,
    ) -> Result<Value, Diagnostic> {
        require_arity(name, arguments, 1, span)?;
        let db_handle = match self.resolve_handle(name, &arguments[0], span)? {
            Ok(handle) => handle,
            Err(err) => return Ok(err),
        };
        // A closed handle reports 0, matching the compiled `bn_rt_sqlite_*` ABI.
        let value = operation(db_handle).unwrap_or(0);
        Ok(Value::Integer(value.into(), IntegerType::Int64))
    }

    fn close_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(name, arguments, 1, span)?;
        let Value::Object { handle, .. } = arguments[0] else {
            return Err(type_mismatch(
                "BNSqlite.Connection",
                "non-object receiver",
                name,
                span,
            ));
        };
        if let Some(db_handle) = self.connections.remove(&handle) {
            match close(db_handle) {
                Ok(()) => Ok(Value::Null),
                Err(failure) => Ok(failure_value(name, &failure)),
            }
        } else {
            Ok(Value::error_report(
                sqlite_err::CLOSED,
                name,
                "Connection is already closed".to_string(),
                "closed handle".to_string(),
            ))
        }
    }
}

/// Filesystem policy stays in the host. The core only receives the answer.
fn open_with_policy(
    core: &dyn CoreContext,
    path: &str,
    mode: OpenMode,
) -> Result<u64, SqliteFailure> {
    let filesystem = core.host().filesystem();
    open_connection(path, mode, |canonical, write| {
        if filesystem.allows_path(canonical, write) {
            Ok(())
        } else {
            Err(filesystem.denial_reason(write).to_string())
        }
    })
}

fn failure_value(name: &str, failure: &SqliteFailure) -> Value {
    Value::error_report(failure.code(), name, failure.message(), failure.cause())
}

/// Keeps the adapter's `MISUSE` text. The core reports the same byte as `PolicyDenied` or `SyntaxError`.
fn interior_nul(name: &str, text: &str, message: &str) -> Option<Value> {
    CString::new(text).err().map(|error| {
        Value::error_report(
            sqlite_err::MISUSE,
            name,
            message.to_string(),
            error.to_string(),
        )
    })
}

fn sqlite_cell(value: SqliteValue) -> Value {
    match value {
        SqliteValue::Integer(integer) => Value::Integer(integer.into(), IntegerType::Int64),
        SqliteValue::Float(float) => Value::Float(float, FloatType::Float64),
        SqliteValue::Text(bytes) | SqliteValue::Blob(bytes) => {
            Value::String(shared_string(String::from_utf8_lossy(&bytes)))
        }
        SqliteValue::Null | SqliteValue::NotAvailable => Value::NotAvailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_widths_are_int64() {
        assert_eq!(bn_types::integer_type_name(IntegerType::Int64), "INT64");
    }
}
