// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Missing values in a `DataFrame`: `CountNA`, `DropNA` and `FillNA`
//! (bndata.md, "Missing values"; BDFL decisions D-3 and G-5). One rule for
//! both backends; each passes how its cells read and are made.

use super::{DataFrameColumn, DataFrameResource};
use crate::data_error::DataFailure;

/// What a cell holds, as far as missing values are concerned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CellKind {
    NotAvailable,
    Integer,
    Float,
    /// A `STRING`, `BOOLEAN` or any other non-numeric value.
    Other,
}

/// How `FillNA` reads and makes the cells of one backend.
#[derive(Clone, Copy, Debug)]
pub struct FillCells<T> {
    pub kind: fn(&T) -> CellKind,
    pub integer: fn(i32) -> T,
    pub float: fn(f64) -> T,
}

fn find_column<'a, T>(
    frame: &'a DataFrameResource<T>,
    name: &str,
) -> Result<&'a DataFrameColumn<T>, DataFailure> {
    frame
        .columns
        .iter()
        .find(|column| column.name == name)
        .ok_or_else(|| DataFailure::ColumnNotFound(name.to_string()))
}

/// `CountNA`: the number of `NA` cells in column `name`.
///
/// # Errors
///
/// `ColumnNotFound` when the frame has no column `name`.
pub fn count_not_available<T>(
    frame: &DataFrameResource<T>,
    name: &str,
    is_not_available: fn(&T) -> bool,
) -> Result<usize, DataFailure> {
    Ok(find_column(frame, name)?
        .values
        .iter()
        .filter(|cell| is_not_available(cell))
        .count())
}

/// `DropNA`: a new frame with the same columns and every row that has no
/// `NA` cell, in order.
#[must_use]
pub fn drop_not_available<T: Clone>(
    frame: &DataFrameResource<T>,
    is_not_available: fn(&T) -> bool,
) -> DataFrameResource<T> {
    let rows = frame
        .columns
        .first()
        .map_or(0, |column| column.values.len());
    let complete = (0..rows)
        .filter(|row| {
            frame
                .columns
                .iter()
                .all(|column| !is_not_available(&column.values[*row]))
        })
        .collect::<Vec<_>>();
    DataFrameResource {
        columns: frame
            .columns
            .iter()
            .map(|column| DataFrameColumn {
                name: column.name.clone(),
                values: complete
                    .iter()
                    .map(|row| column.values[*row].clone())
                    .collect(),
            })
            .collect(),
    }
}

/// `FillNA`: every `NA` cell of the numeric column `name` becomes `value`,
/// of the column's type (an `INTEGER` column needs a whole number within
/// `INTEGER`; an all-`NA` column takes `FLOAT`).
///
/// # Errors
///
/// `ColumnNotFound`; `NonNumericColumn` for a `STRING` or `BOOLEAN` column;
/// `InvalidArgument` for a value an `INTEGER` column cannot hold. The column
/// is unchanged on an error.
pub fn fill_not_available<T: Clone>(
    frame: &mut DataFrameResource<T>,
    name: &str,
    value: f64,
    cells: &FillCells<T>,
) -> Result<(), DataFailure> {
    let column = find_column(frame, name)?;
    let kinds = column
        .values
        .iter()
        .map(cells.kind)
        .filter(|kind| *kind != CellKind::NotAvailable)
        .collect::<Vec<_>>();
    if kinds.contains(&CellKind::Other) {
        return Err(DataFailure::NonNumericColumn(name.to_string()));
    }
    let integer_column = !kinds.is_empty() && kinds.iter().all(|kind| *kind == CellKind::Integer);
    let fill = if integer_column {
        (cells.integer)(whole_integer(value).ok_or_else(|| not_integer(name))?)
    } else {
        (cells.float)(value)
    };
    let column = frame
        .columns
        .iter_mut()
        .find(|column| column.name == name)
        .expect("the column was found above");
    for cell in &mut column.values {
        if (cells.kind)(cell) == CellKind::NotAvailable {
            *cell = fill.clone();
        }
    }
    Ok(())
}

/// `value` as an `INTEGER`, when it is a whole number within `INTEGER`.
#[allow(clippy::cast_possible_truncation, clippy::float_cmp)] // range and wholeness checked
fn whole_integer(value: f64) -> Option<i32> {
    (value.trunc() == value && value >= f64::from(i32::MIN) && value <= f64::from(i32::MAX))
        .then_some(value as i32)
}

fn not_integer(name: &str) -> DataFailure {
    DataFailure::InvalidArgument(bn_catalog::fill(
        "data",
        "fill.not_integer.message",
        &[("column", &name)],
    ))
}
