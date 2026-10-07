// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Native `BNSqlite` lowering onto the `bn_rt_sqlite_*` C ABI.

#![allow(clippy::wildcard_imports)]
use super::bndata_columns::emit_handle_operand;
use super::*;
use crate::ir::{CastOp, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};

pub(crate) fn sqlite_call_supported(
    method: &str,
    arguments: &[ValueId],
    values: &HashMap<ValueId, Type>,
    module: &Module,
) -> bool {
    let string = |argument: &ValueId| values.get(argument) == Some(&Type::String);
    let conn = |argument: &ValueId| {
        values
            .get(argument)
            .is_some_and(|ty| carries_bnsqlite_connection(module, ty))
    };
    match method {
        "Open" | "OpenReadOnly" | "OpenExisting" => arguments.len() == 1 && string(&arguments[0]),
        "Exec" | "Query" => arguments.len() == 2 && conn(&arguments[0]) && string(&arguments[1]),
        "Begin" | "Commit" | "Rollback" | "Changes" | "LastInsertRowId" | "Close" => {
            arguments.len() == 1 && conn(&arguments[0])
        }
        _ => false,
    }
}

pub(crate) fn lower_sqlite_call(
    text: &mut String,
    destination: ValueId,
    method: &str,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("sqlite{name}{dest}"));
    let v = |index: usize| O::reg(format!("v{}", arguments[index].0));
    let handle = |text: &mut String| {
        emit_handle_operand(text, analysis, format!("sqliteh{dest}"), arguments[0]);
        (T::I64, r("h"))
    };
    // A call whose status is the `VOID OR Error` result.
    let status = |text: &mut String, symbol: &str, args| {
        text.assign(format!("sqliterc{dest}"), I::call(T::I32, symbol, args));
        emit_void_result(text, destination, format!("%sqliterc{dest}"));
    };
    // A call that writes a new handle through its last argument.
    let new_handle = |text: &mut String, symbol: &str, mut args: Vec<(T, O)>, value: &str| {
        text.assign(format!("sqliteout{dest}"), I::alloca(T::I64));
        args.push((T::Ptr, r("out")));
        text.assign(format!("sqliterc{dest}"), I::call(T::I32, symbol, args));
        text.assign(format!("sqlite{value}{dest}"), I::load(T::I64, r("out")));
        emit_handle_result(
            text,
            destination,
            format!("%sqliterc{dest}"),
            format!("%sqlite{value}{dest}"),
        );
    };
    match method {
        "Open" | "OpenReadOnly" | "OpenExisting" => {
            let symbol = match method {
                "OpenReadOnly" => "bn_rt_sqlite_open_read_only",
                "OpenExisting" => "bn_rt_sqlite_open_existing",
                _ => "bn_rt_sqlite_open",
            };
            new_handle(text, symbol, vec![(T::Ptr, v(0))], "handle");
        }
        "Exec" => {
            let args = vec![handle(text), (T::Ptr, v(1))];
            status(text, "bn_rt_sqlite_exec", args);
        }
        "Query" => {
            let args = vec![handle(text), (T::Ptr, v(1))];
            new_handle(text, "bn_rt_sqlite_query", args, "frame");
        }
        "Begin" | "Commit" | "Rollback" | "Close" => {
            let symbol = format!("bn_rt_sqlite_{}", method.to_ascii_lowercase());
            let args = vec![handle(text)];
            status(text, &symbol, args);
        }
        "Changes" => {
            let args = vec![handle(text)];
            text.assign(
                format!("v{dest}"),
                I::call(T::I32, "bn_rt_sqlite_changes", args),
            );
        }
        "LastInsertRowId" => {
            let args = vec![handle(text)];
            let rowid = I::call(T::I64, "bn_rt_sqlite_last_insert_rowid", args);
            text.assign(format!("sqliterowid{dest}"), rowid);
            text.assign(
                format!("v{dest}"),
                I::cast(CastOp::Trunc, T::I64, r("rowid"), T::I32),
            );
        }
        _ => panic!("unhandled BNSqlite member in emission: {method}"),
    }
}
