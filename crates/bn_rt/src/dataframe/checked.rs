use super::{DataFailure, DataFrameColumn, DataFrameResource, first_duplicate_column};

/// Constructs a rectangular string-valued CSV frame before publishing a handle.
///
/// # Errors
/// Rejects ragged rows and duplicate column labels.
pub fn frame_from_csv_rows<T>(
    mut rows: Vec<Vec<String>>,
    has_header: bool,
    value: impl Fn(String) -> T,
) -> Result<DataFrameResource<T>, DataFailure> {
    let headers = if has_header && !rows.is_empty() {
        rows.remove(0)
    } else {
        Vec::new()
    };
    let width = headers.len().max(rows.first().map_or(0, Vec::len));
    if has_header && headers.len() != width {
        return Err(DataFailure::RaggedRow {
            expected: width,
            got: headers.len(),
        });
    }
    if let Some(row) = rows.iter().find(|row| row.len() != width) {
        return Err(DataFailure::RaggedRow {
            expected: width,
            got: row.len(),
        });
    }
    let columns = (0..width)
        .map(|index| DataFrameColumn {
            name: headers
                .get(index)
                .cloned()
                .unwrap_or_else(|| format!("Column{}", index + 1)),
            values: rows.iter().map(|row| value(row[index].clone())).collect(),
        })
        .collect::<Vec<_>>();
    if let Some(name) = first_duplicate_column(&columns) {
        return Err(DataFailure::DuplicateColumn(name));
    }
    Ok(DataFrameResource { columns })
}

/// Copies a contiguous block after validating every bound without index arrays.
/// Empty ranges preserve the existing behavior: their start is not dereferenced.
///
/// # Errors
/// Returns an error for out-of-bounds nonempty ranges or reservation failure.
pub fn slice_dataframe<T: Clone>(
    frame: &DataFrameResource<T>,
    start_row: i64,
    row_count: i64,
    start_column: i64,
    column_count: i64,
) -> Result<DataFrameResource<T>, DataFailure> {
    // bndata.md: a negative slice bound is `INVALID_ARGUMENT`.
    let bound = |value: i64| {
        usize::try_from(value).map_err(|_| DataFailure::NegativeBound {
            bound: "slice bound",
            value,
        })
    };
    let (start_row, row_count) = (bound(start_row)?, bound(row_count)?);
    let (start_column, column_count) = (bound(start_column)?, bound(column_count)?);
    let rows = frame
        .columns
        .first()
        .map_or(0, |column| column.values.len());
    let check = |start: usize, len: usize, total: usize, dim: &'static str| {
        if len == 0 || (start < total && len <= total - start) {
            Ok(())
        } else {
            Err(DataFailure::SliceOutOfRange {
                start,
                len,
                total,
                dim,
            })
        }
    };
    check(start_row, row_count, rows, "row")?;
    check(start_column, column_count, frame.columns.len(), "column")?;
    let mut columns = Vec::new();
    columns
        .try_reserve_exact(column_count)
        .map_err(|_| DataFailure::InvalidFormat("DataFrame allocation failed".into()))?;
    if column_count != 0 {
        for source in &frame.columns[start_column..start_column + column_count] {
            let mut values = Vec::new();
            values
                .try_reserve_exact(row_count)
                .map_err(|_| DataFailure::InvalidFormat("DataFrame allocation failed".into()))?;
            if row_count != 0 {
                values.extend_from_slice(&source.values[start_row..start_row + row_count]);
            }
            columns.push(DataFrameColumn {
                name: source.name.clone(),
                values,
            });
        }
    }
    Ok(DataFrameResource { columns })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_extreme_counts_fail_before_allocating_and_valid_slices_own_values() {
        let mut frame = DataFrameResource {
            columns: vec![DataFrameColumn {
                name: "x".into(),
                values: vec![10, 20],
            }],
        };
        for (row, count, col, cols) in [
            (0, i64::MAX, 0, 1),
            (0, 1, 0, i64::MAX),
            (i64::MAX, 1, 0, 1),
            (0, 1, 1, 1),
        ] {
            assert!(slice_dataframe(&frame, row, count, col, cols).is_err());
        }
        assert!(matches!(
            slice_dataframe(&frame, -3, 1, 0, 1),
            Err(DataFailure::NegativeBound { value: -3, .. })
        ));
        let selected = slice_dataframe(&frame, 1, 1, 0, 1).unwrap();
        frame.columns[0].values[1] = 99;
        assert_eq!(selected.columns[0].values, [20]);
        assert!(
            slice_dataframe(&frame, i64::MAX, 0, i64::MAX, 0)
                .unwrap()
                .columns
                .is_empty()
        );
    }
}
