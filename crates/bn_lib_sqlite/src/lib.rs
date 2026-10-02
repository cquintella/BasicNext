// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNSqlite` — standard library module providing an embedded `SQLite3` driver.

#![allow(clippy::similar_names, clippy::borrow_as_ptr)]

use std::collections::HashMap;
use std::ffi::CString;

use bn_diag::Diagnostic;
use bn_interp::provider::{CoreContext, Provider};
use bn_interp::{name_not_found, require_arity_pub as require_arity, type_mismatch};
use bn_lib_data::DataProvider;
use bn_rt::sqlite_abi::{
    bn_rt_sqlite_begin, bn_rt_sqlite_changes, bn_rt_sqlite_close, bn_rt_sqlite_commit,
    bn_rt_sqlite_exec, bn_rt_sqlite_last_insert_rowid, bn_rt_sqlite_open,
    bn_rt_sqlite_open_existing, bn_rt_sqlite_open_read_only, bn_rt_sqlite_query,
    bn_rt_sqlite_rollback,
};
use bn_rt::{
    DataFrameColumn, DataFrameResource, StoredValue, get_frame_columns, take_error_report,
};
use bn_runtime::Handle;
use bn_source::Span;
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
            bn_rt_sqlite_close(db_handle);
        }
    }

    fn close_all(&mut self) {
        for db_handle in self.connections.values() {
            bn_rt_sqlite_close(*db_handle);
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
            "Open" => self.open_call(core, name, arguments, span, bn_rt_sqlite_open),
            "OpenReadOnly" => {
                self.open_call(core, name, arguments, span, bn_rt_sqlite_open_read_only)
            }
            "OpenExisting" => {
                self.open_call(core, name, arguments, span, bn_rt_sqlite_open_existing)
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
        open_fn: extern "C" fn(*const std::ffi::c_char, *mut u64) -> i32,
    ) -> Result<Value, Diagnostic> {
        require_arity(name, arguments, 1, span)?;
        let Value::String(path) = &arguments[0] else {
            return Err(type_mismatch("STRING", "non-STRING path", name, span));
        };
        let c_path = match CString::new(path.as_ref()) {
            Ok(s) => s,
            Err(e) => {
                return Ok(Value::error_report(
                    sqlite_err::MISUSE,
                    name,
                    "Database path contains interior NUL byte".to_string(),
                    e.to_string(),
                ));
            }
        };

        let mut db_handle = 0u64;
        let rc = open_fn(c_path.as_ptr(), &mut db_handle);
        if rc != 0 {
            let (code, op, msg, cause) = take_error_report();
            let effective_op = if op.is_empty() { name } else { &op };
            return Ok(Value::error_report(code, effective_op, msg, cause));
        }

        let object = core.allocate_object("BNSqlite.Connection", span)?;
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
            Ok(h) => h,
            Err(err) => return Ok(err),
        };
        let Value::String(sql) = &arguments[1] else {
            return Err(type_mismatch("STRING", "non-STRING SQL query", name, span));
        };
        let c_sql = match CString::new(sql.as_ref()) {
            Ok(s) => s,
            Err(e) => {
                return Ok(Value::error_report(
                    sqlite_err::MISUSE,
                    name,
                    "SQL statement contains interior NUL byte".to_string(),
                    e.to_string(),
                ));
            }
        };

        let rc = bn_rt_sqlite_exec(db_handle, c_sql.as_ptr());
        if rc != 0 {
            let (code, op, msg, cause) = take_error_report();
            let effective_op = if op.is_empty() { name } else { &op };
            return Ok(Value::error_report(code, effective_op, msg, cause));
        }
        Ok(Value::Null)
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
            Ok(h) => h,
            Err(err) => return Ok(err),
        };
        let Value::String(sql) = &arguments[1] else {
            return Err(type_mismatch("STRING", "non-STRING SQL query", name, span));
        };
        let c_sql = match CString::new(sql.as_ref()) {
            Ok(s) => s,
            Err(e) => {
                return Ok(Value::error_report(
                    sqlite_err::MISUSE,
                    name,
                    "SQL statement contains interior NUL byte".to_string(),
                    e.to_string(),
                ));
            }
        };

        let mut frame_handle = 0u64;
        let rc = bn_rt_sqlite_query(db_handle, c_sql.as_ptr(), &mut frame_handle);
        if rc != 0 {
            let (code, op, msg, cause) = take_error_report();
            let effective_op = if op.is_empty() { name } else { &op };
            return Ok(Value::error_report(code, effective_op, msg, cause));
        }

        let Some(columns) = get_frame_columns(frame_handle) else {
            return Ok(Value::error_report(
                sqlite_err::INTERNAL_ERROR,
                name,
                "Failed to retrieve query dataframe columns".to_string(),
                "internal frame not found".to_string(),
            ));
        };

        let mut value_columns = Vec::with_capacity(columns.len());
        for col in columns {
            let mut values = Vec::with_capacity(col.values.len());
            for v in col.values {
                let val = match v {
                    StoredValue::Integer(i) => Value::Integer(i.into(), IntegerType::Int64),
                    StoredValue::Float(f) => Value::Float(f, FloatType::Float64),
                    StoredValue::String(bytes) | StoredValue::Bytes(bytes) => {
                        Value::String(shared_string(String::from_utf8_lossy(&bytes)))
                    }
                    StoredValue::Boolean(b) => Value::Boolean(b),
                    StoredValue::Null => Value::Null,
                    StoredValue::NotAvailable | StoredValue::EndOfFile | StoredValue::Handle(_) => {
                        Value::NotAvailable
                    }
                };
                values.push(val);
            }
            value_columns.push(DataFrameColumn {
                name: col.name,
                values,
            });
        }

        let resource = DataFrameResource {
            columns: value_columns,
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
        require_arity(name, arguments, 1, span)?;
        let db_handle = match self.resolve_handle(name, &arguments[0], span)? {
            Ok(h) => h,
            Err(err) => return Ok(err),
        };
        let rc = bn_rt_sqlite_begin(db_handle);
        if rc != 0 {
            let (code, op, msg, cause) = take_error_report();
            let effective_op = if op.is_empty() { name } else { &op };
            return Ok(Value::error_report(code, effective_op, msg, cause));
        }
        Ok(Value::Null)
    }

    fn commit_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(name, arguments, 1, span)?;
        let db_handle = match self.resolve_handle(name, &arguments[0], span)? {
            Ok(h) => h,
            Err(err) => return Ok(err),
        };
        let rc = bn_rt_sqlite_commit(db_handle);
        if rc != 0 {
            let (code, op, msg, cause) = take_error_report();
            let effective_op = if op.is_empty() { name } else { &op };
            return Ok(Value::error_report(code, effective_op, msg, cause));
        }
        Ok(Value::Null)
    }

    fn rollback_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(name, arguments, 1, span)?;
        let db_handle = match self.resolve_handle(name, &arguments[0], span)? {
            Ok(h) => h,
            Err(err) => return Ok(err),
        };
        let rc = bn_rt_sqlite_rollback(db_handle);
        if rc != 0 {
            let (code, op, msg, cause) = take_error_report();
            let effective_op = if op.is_empty() { name } else { &op };
            return Ok(Value::error_report(code, effective_op, msg, cause));
        }
        Ok(Value::Null)
    }

    fn changes_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(name, arguments, 1, span)?;
        let db_handle = match self.resolve_handle(name, &arguments[0], span)? {
            Ok(h) => h,
            Err(err) => return Ok(err),
        };
        let count = bn_rt_sqlite_changes(db_handle);
        Ok(Value::Integer(count.into(), IntegerType::Int64))
    }

    fn last_insert_rowid_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(name, arguments, 1, span)?;
        let db_handle = match self.resolve_handle(name, &arguments[0], span)? {
            Ok(h) => h,
            Err(err) => return Ok(err),
        };
        let rowid = bn_rt_sqlite_last_insert_rowid(db_handle);
        Ok(Value::Integer(rowid.into(), IntegerType::Int64))
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
            let rc = bn_rt_sqlite_close(db_handle);
            if rc != 0 {
                let (code, op, msg, cause) = take_error_report();
                let effective_op = if op.is_empty() { name } else { &op };
                return Ok(Value::error_report(code, effective_op, msg, cause));
            }
            Ok(Value::Null)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_widths_are_int64() {
        assert_eq!(bn_types::integer_type_name(IntegerType::Int64), "INT64");
    }
}
