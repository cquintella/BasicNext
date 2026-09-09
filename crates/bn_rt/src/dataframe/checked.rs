use super::{DataFrameColumn, DataFrameResource, duplicate_column_names};

/// Constructs a rectangular string-valued CSV frame before publishing a handle.
///
/// # Errors
/// Rejects ragged rows and duplicate column labels.
pub fn frame_from_csv_rows<T>(
    mut rows: Vec<Vec<String>>,
    has_header: bool,
    value: impl Fn(String) -> T,
) -> Result<DataFrameResource<T>, String> {
    let headers = if has_header && !rows.is_empty() {
        rows.remove(0)
    } else {
        Vec::new()
    };
    let width = headers.len().max(rows.first().map_or(0, Vec::len));
    if rows.iter().any(|row| row.len() != width) || (has_header && headers.len() != width) {
        return Err("ragged CSV row".into());
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
    if duplicate_column_names(&columns) {
        return Err("duplicate column name".into());
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
    start_row: usize,
    row_count: usize,
    start_column: usize,
    column_count: usize,
) -> Result<DataFrameResource<T>, String> {
    let rows = frame
        .columns
        .first()
        .map_or(0, |column| column.values.len());
    let valid = |start: usize, count: usize, size: usize| {
        count == 0 || (start < size && count <= size - start)
    };
    if !valid(start_row, row_count, rows) || !valid(start_column, column_count, frame.columns.len())
    {
        return Err("DataFrame index out of bounds".into());
    }
    let mut columns = Vec::new();
    columns
        .try_reserve_exact(column_count)
        .map_err(|_| "DataFrame allocation failed")?;
    if column_count != 0 {
        for source in &frame.columns[start_column..start_column + column_count] {
            let mut values = Vec::new();
            values
                .try_reserve_exact(row_count)
                .map_err(|_| "DataFrame allocation failed")?;
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
            (0, usize::MAX, 0, 1),
            (0, 1, 0, usize::MAX),
            (usize::MAX, 1, 0, 1),
            (0, 1, 1, 1),
        ] {
            assert!(slice_dataframe(&frame, row, count, col, cols).is_err());
        }
        let selected = slice_dataframe(&frame, 1, 1, 0, 1).unwrap();
        frame.columns[0].values[1] = 99;
        assert_eq!(selected.columns[0].values, [20]);
        assert!(
            slice_dataframe(&frame, usize::MAX, 0, usize::MAX, 0)
                .unwrap()
                .columns
                .is_empty()
        );
    }
}
