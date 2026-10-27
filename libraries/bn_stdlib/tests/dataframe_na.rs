// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The missing-value rule both backends share (bndata.md, "Missing
//! values"): `CountNA`, `DropNA`, `FillNA` on a cell type of the test's own.

use bn_stdlib::data_error::DataFailure;
use bn_stdlib::{
    CellKind, DataFrameColumn, DataFrameResource, FillCells, count_not_available,
    drop_not_available, fill_not_available,
};

#[derive(Clone, Debug, PartialEq)]
enum Cell {
    Missing,
    Whole(i32),
    Real(f64),
    Text(&'static str),
}

const CELLS: FillCells<Cell> = FillCells {
    kind: |cell| match cell {
        Cell::Missing => CellKind::NotAvailable,
        Cell::Whole(_) => CellKind::Integer,
        Cell::Real(_) => CellKind::Float,
        Cell::Text(_) => CellKind::Other,
    },
    integer: Cell::Whole,
    float: Cell::Real,
};

fn missing(cell: &Cell) -> bool {
    *cell == Cell::Missing
}

fn frame() -> DataFrameResource<Cell> {
    let column = |name: &str, values| DataFrameColumn {
        name: name.to_string(),
        values,
    };
    DataFrameResource {
        columns: vec![
            column(
                "name",
                vec![Cell::Text("a"), Cell::Text("b"), Cell::Text("c")],
            ),
            column("qty", vec![Cell::Whole(4), Cell::Missing, Cell::Whole(6)]),
            column("blank", vec![Cell::Missing, Cell::Missing, Cell::Missing]),
        ],
    }
}

#[test]
fn count_and_drop_follow_the_na_cells() {
    let table = frame();
    assert_eq!(count_not_available(&table, "qty", missing), Ok(1));
    assert!(matches!(
        count_not_available(&table, "nothing", missing),
        Err(DataFailure::ColumnNotFound(_))
    ));
    // Every row has NA in "blank": none is kept, the columns are.
    let kept = drop_not_available(&table, missing);
    assert_eq!(kept.columns.len(), 3);
    assert!(kept.columns.iter().all(|column| column.values.is_empty()));
    let empty = DataFrameResource::<Cell> { columns: vec![] };
    assert!(drop_not_available(&empty, missing).columns.is_empty());
}

#[test]
fn fill_keeps_the_column_type_and_rejects_what_it_cannot_hold() {
    let mut table = frame();
    assert!(matches!(
        fill_not_available(&mut table, "qty", 1.5, &CELLS),
        Err(DataFailure::InvalidArgument(_))
    ));
    assert!(matches!(
        fill_not_available(&mut table, "qty", 3e9, &CELLS),
        Err(DataFailure::InvalidArgument(_))
    ));
    assert_eq!(
        table.columns[1].values[1],
        Cell::Missing,
        "unchanged on an error"
    );
    assert_eq!(fill_not_available(&mut table, "qty", -2.0, &CELLS), Ok(()));
    assert_eq!(table.columns[1].values[1], Cell::Whole(-2));
    assert!(matches!(
        fill_not_available(&mut table, "name", 0.0, &CELLS),
        Err(DataFailure::NonNumericColumn(_))
    ));
    assert_eq!(fill_not_available(&mut table, "blank", 2.5, &CELLS), Ok(()));
    assert_eq!(table.columns[2].values, vec![Cell::Real(2.5); 3]);
}
