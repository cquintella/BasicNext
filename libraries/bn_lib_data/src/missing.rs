// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `DataFrame.CountNA`, `DropNA` and `FillNA` for the interpreter
//! (bndata.md, "Missing values"); the rule is `bn_stdlib::dataframe::na`,
//! shared with the native runtime.

use bn_diag::Diagnostic;
use bn_source::Span;
use bn_stdlib::{CellKind, FillCells, count_not_available, drop_not_available, fill_not_available};
use bn_types::{FloatType, IntegerType};
use bn_value::Value;

use bn_interp::{
    integer_from_count_pub as integer_from_count, is_not_available_pub as is_not_available,
    number_as_float_pub as number_as_float, require_arity_pub as require_arity, type_mismatch,
};

use super::{DataFrameResource, failed};

/// How the interpreter's cells read and are made for `FillNA`.
const CELLS: FillCells<Value> = FillCells {
    kind: cell_kind,
    integer: |number| Value::Integer(i128::from(number), IntegerType::Int32),
    float: |number| Value::Float(number, FloatType::Float64),
};

fn cell_kind(cell: &Value) -> CellKind {
    match cell {
        Value::NotAvailable => CellKind::NotAvailable,
        Value::Integer(..) => CellKind::Integer,
        Value::Float(..) => CellKind::Float,
        _ => CellKind::Other,
    }
}

fn column_argument(value: &Value, span: Span) -> Result<&str, Diagnostic> {
    match value {
        Value::String(name) => Ok(name),
        _ => Err(type_mismatch(
            "STRING",
            "non-STRING value",
            "DataFrame column name",
            span,
        )),
    }
}

/// `CountNA(name)`: an `INTEGER`, or the `Error` of a missing column.
pub(super) fn count(
    name: &str,
    frame: &DataFrameResource,
    arguments: &[Value],
    span: Span,
) -> Result<Value, Diagnostic> {
    require_arity(name, arguments, 2, span)?;
    let column = column_argument(&arguments[1], span)?;
    match count_not_available(frame, column, is_not_available) {
        Ok(count) => integer_from_count(count, span),
        Err(failure) => Ok(failed(name, &failure)),
    }
}

/// `DropNA()`: the frame without its rows that hold `NA`.
pub(super) fn drop(
    name: &str,
    frame: &DataFrameResource,
    arguments: &[Value],
    span: Span,
) -> Result<DataFrameResource, Diagnostic> {
    require_arity(name, arguments, 1, span)?;
    Ok(drop_not_available(frame, is_not_available))
}

/// `FillNA(name, value)`: `NULL` (the `VOID` result), or an `Error`.
pub(super) fn fill(
    name: &str,
    frame: &mut DataFrameResource,
    arguments: &[Value],
    span: Span,
) -> Result<Value, Diagnostic> {
    require_arity(name, arguments, 3, span)?;
    let column = column_argument(&arguments[1], span)?;
    let value = number_as_float(&arguments[2], span)?;
    match fill_not_available(frame, column, value, &CELLS) {
        Ok(()) => Ok(Value::Null),
        Err(failure) => Ok(failed(name, &failure)),
    }
}
