// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Native `BNSqlite` lowering onto the `bn_rt_sqlite_*` C ABI.

#![allow(clippy::wildcard_imports)]
use super::*;

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

fn extract_connection_handle(
    text: &mut String,
    receiver: ValueId,
    dest: u32,
    analysis: &LoweringAnalysis<'_>,
) -> String {
    if llvm_type(
        analysis
            .values
            .get(&receiver)
            .expect("validated connection receiver"),
    ) == Some("{ i1, ptr, i64 }")
    {
        let _ = writeln!(
            text,
            "  %sqliteh{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
            receiver.0
        );
    } else {
        let _ = writeln!(
            text,
            "  %sqliteh{dest} = ptrtoint ptr %v{} to i64",
            receiver.0
        );
    }
    format!("%sqliteh{dest}")
}

pub(crate) fn lower_sqlite_call(
    text: &mut String,
    destination: ValueId,
    method: &str,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    let dest = destination.0;
    match method {
        "Open" | "OpenReadOnly" | "OpenExisting" => {
            let path_val = arguments[0].0;
            let symbol = match method {
                "OpenReadOnly" => "bn_rt_sqlite_open_read_only",
                "OpenExisting" => "bn_rt_sqlite_open_existing",
                _ => "bn_rt_sqlite_open",
            };
            let _ = writeln!(text, "  %sqliteout{dest} = alloca i64");
            let _ = writeln!(
                text,
                "  %sqliterc{dest} = call i32 @{symbol}(ptr %v{path_val}, ptr %sqliteout{dest})"
            );
            let _ = writeln!(
                text,
                "  %sqlitehandle{dest} = load i64, ptr %sqliteout{dest}"
            );
            emit_handle_result(
                text,
                destination,
                format!("%sqliterc{dest}"),
                format!("%sqlitehandle{dest}"),
            );
        }
        "Exec" => {
            let handle = extract_connection_handle(text, arguments[0], dest, analysis);
            let sql_val = arguments[1].0;
            let _ = writeln!(
                text,
                "  %sqliterc{dest} = call i32 @bn_rt_sqlite_exec(i64 {handle}, ptr %v{sql_val})"
            );
            emit_void_result(text, destination, format!("%sqliterc{dest}"));
        }
        "Query" => {
            let handle = extract_connection_handle(text, arguments[0], dest, analysis);
            let sql_val = arguments[1].0;
            let _ = writeln!(text, "  %sqliteout{dest} = alloca i64");
            let _ = writeln!(
                text,
                "  %sqliterc{dest} = call i32 @bn_rt_sqlite_query(i64 {handle}, ptr %v{sql_val}, ptr %sqliteout{dest})"
            );
            let _ = writeln!(
                text,
                "  %sqliteframe{dest} = load i64, ptr %sqliteout{dest}"
            );
            emit_handle_result(
                text,
                destination,
                format!("%sqliterc{dest}"),
                format!("%sqliteframe{dest}"),
            );
        }
        "Begin" | "Commit" | "Rollback" | "Close" => {
            let handle = extract_connection_handle(text, arguments[0], dest, analysis);
            let symbol = match method {
                "Begin" => "bn_rt_sqlite_begin",
                "Commit" => "bn_rt_sqlite_commit",
                "Rollback" => "bn_rt_sqlite_rollback",
                _ => "bn_rt_sqlite_close",
            };
            let _ = writeln!(text, "  %sqliterc{dest} = call i32 @{symbol}(i64 {handle})");
            emit_void_result(text, destination, format!("%sqliterc{dest}"));
        }
        "Changes" => {
            let handle = extract_connection_handle(text, arguments[0], dest, analysis);
            let _ = writeln!(
                text,
                "  %v{dest} = call i32 @bn_rt_sqlite_changes(i64 {handle})"
            );
        }
        "LastInsertRowId" => {
            let handle = extract_connection_handle(text, arguments[0], dest, analysis);
            let _ = writeln!(
                text,
                "  %sqliterowid{dest} = call i64 @bn_rt_sqlite_last_insert_rowid(i64 {handle})"
            );
            let _ = writeln!(text, "  %v{dest} = trunc i64 %sqliterowid{dest} to i32");
        }
        _ => panic!("unhandled BNSqlite member in emission: {method}"),
    }
}
