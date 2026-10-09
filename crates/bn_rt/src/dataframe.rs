// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0.

//! Runtime-owned CSV parsing used by the `BNData` provider.

mod checked;
pub use checked::{frame_from_csv_rows, slice_dataframe};

use crate::data_error::DataFailure;

/// Provider boundary for standard-library data ingestion.
pub trait DataProvider: Send + Sync {
    /// Parses CSV text into rows.
    ///
    /// # Errors
    ///
    /// Returns an error when the input is not valid CSV.
    fn read_csv(&self, text: &str, separator: char) -> Result<Vec<Vec<String>>, DataFailure>;
}

/// Default statically linked CSV provider.
#[derive(Debug, Default)]
pub struct StandardDataProvider;

impl DataProvider for StandardDataProvider {
    fn read_csv(&self, text: &str, separator: char) -> Result<Vec<Vec<String>>, DataFailure> {
        parse_csv(text, separator)
    }
}

#[derive(Clone, Debug)]
pub struct DataFrameResource<T> {
    pub columns: Vec<DataFrameColumn<T>>,
}

#[derive(Clone, Debug)]
pub struct DataFrameColumn<T> {
    pub name: String,
    pub values: Vec<T>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DataFrameJoin {
    Inner,
    Left,
    Right,
    Full,
}

#[derive(Debug)]
pub struct DataFrameJoinConfig<'a, T> {
    pub left_label: &'a str,
    pub right_label: &'a str,
    pub kind: DataFrameJoin,
    pub equals: fn(&T, &T) -> bool,
    pub is_not_available: fn(&T) -> bool,
    pub not_available: &'a T,
}

#[allow(clippy::too_many_lines)] // Join modes share schema and output shaping.
/// Joins two typed `DataFrame` resources while preserving the selected outer
/// side and filling unmatched cells with the configured sentinel.
///
/// # Errors
///
/// Returns an error when a key column is missing or the output would contain
/// a duplicate non-key column label.
pub fn join_dataframes<T: Clone>(
    left: &DataFrameResource<T>,
    right: &DataFrameResource<T>,
    config: &DataFrameJoinConfig<'_, T>,
) -> Result<DataFrameResource<T>, DataFailure> {
    let Some(left_key) = left
        .columns
        .iter()
        .position(|column| column.name == config.left_label)
    else {
        return Err(DataFailure::KeyColumnNotFound {
            key: config.left_label.to_string(),
            side: "left",
        });
    };
    let Some(right_key) = right
        .columns
        .iter()
        .position(|column| column.name == config.right_label)
    else {
        return Err(DataFailure::KeyColumnNotFound {
            key: config.right_label.to_string(),
            side: "right",
        });
    };
    if let Some(dup) = right
        .columns
        .iter()
        .enumerate()
        .find_map(|(index, column)| {
            if index != right_key && left.columns.iter().any(|left| left.name == column.name) {
                Some(column.name.clone())
            } else {
                None
            }
        })
    {
        return Err(DataFailure::DuplicateColumn(dup));
    }
    let left_rows = left.columns.first().map_or(0, |column| column.values.len());
    let right_rows = right
        .columns
        .first()
        .map_or(0, |column| column.values.len());
    let matches = |left_row: usize, right_row: usize| {
        let left_value = &left.columns[left_key].values[left_row];
        let right_value = &right.columns[right_key].values[right_row];
        !(config.is_not_available)(left_value)
            && !(config.is_not_available)(right_value)
            && (config.equals)(left_value, right_value)
    };
    let mut pairs = Vec::new();
    match config.kind {
        DataFrameJoin::Right => {
            for right_row in 0..right_rows {
                let mut found = false;
                for left_row in 0..left_rows {
                    if matches(left_row, right_row) {
                        pairs.push((Some(left_row), Some(right_row)));
                        found = true;
                    }
                }
                if !found {
                    pairs.push((None, Some(right_row)));
                }
            }
        }
        DataFrameJoin::Inner | DataFrameJoin::Left | DataFrameJoin::Full => {
            let mut right_used = vec![false; right_rows];
            for left_row in 0..left_rows {
                let mut found = false;
                for (right_row, used) in right_used.iter_mut().enumerate() {
                    if matches(left_row, right_row) {
                        pairs.push((Some(left_row), Some(right_row)));
                        *used = true;
                        found = true;
                    }
                }
                if !found && !matches!(config.kind, DataFrameJoin::Inner) {
                    pairs.push((Some(left_row), None));
                }
            }
            if matches!(config.kind, DataFrameJoin::Full) {
                for (right_row, used) in right_used.into_iter().enumerate() {
                    if !used {
                        pairs.push((None, Some(right_row)));
                    }
                }
            }
        }
    }
    let mut columns = Vec::with_capacity(left.columns.len() + right.columns.len() - 1);
    for (index, column) in left.columns.iter().enumerate() {
        columns.push(DataFrameColumn {
            name: column.name.clone(),
            values: pairs
                .iter()
                .map(|(left_row, right_row)| {
                    left_row.map_or_else(
                        || {
                            if index == left_key {
                                right_row.map_or_else(
                                    || config.not_available.clone(),
                                    |row| right.columns[right_key].values[row].clone(),
                                )
                            } else {
                                config.not_available.clone()
                            }
                        },
                        |row| column.values[row].clone(),
                    )
                })
                .collect(),
        });
    }
    for (index, column) in right.columns.iter().enumerate() {
        if index != right_key {
            columns.push(DataFrameColumn {
                name: column.name.clone(),
                values: pairs
                    .iter()
                    .map(|(_, right_row)| {
                        right_row.map_or_else(
                            || config.not_available.clone(),
                            |row| column.values[row].clone(),
                        )
                    })
                    .collect(),
            });
        }
    }
    Ok(DataFrameResource { columns })
}

/// 0-based indices as positions below `count`, or the first index (negative or
/// too large) that is not one.
fn checked_indices(indices: &[i64], count: usize) -> Result<Vec<usize>, i64> {
    indices
        .iter()
        .map(|&index| {
            usize::try_from(index)
                .ok()
                .filter(|&at| at < count)
                .ok_or(index)
        })
        .collect()
}

/// The first column name that appears twice, for `DuplicateColumn`.
#[must_use]
pub fn first_duplicate_column<T>(columns: &[DataFrameColumn<T>]) -> Option<String> {
    let mut seen = std::collections::HashSet::with_capacity(columns.len());
    columns
        .iter()
        .find(|column| !seen.insert(column.name.as_str()))
        .map(|column| column.name.clone())
}

/// Appends rows after checking that both frames have the same column layout.
///
/// # Errors
///
/// Returns an error when layouts or non-missing column value types differ.
pub fn append_rows<T: Clone>(
    left: &DataFrameResource<T>,
    right: &DataFrameResource<T>,
    is_not_available: fn(&T) -> bool,
    same_type: fn(&T, &T) -> bool,
) -> Result<DataFrameResource<T>, DataFailure> {
    if left.columns.len() != right.columns.len()
        || left
            .columns
            .iter()
            .zip(&right.columns)
            .any(|(left, right)| left.name != right.name)
    {
        return Err(DataFailure::ColumnLayoutsDiffer);
    }
    let mut columns = left.columns.clone();
    for (column, other) in columns.iter_mut().zip(&right.columns) {
        let left_value = column.values.iter().find(|value| !is_not_available(value));
        let right_value = other.values.iter().find(|value| !is_not_available(value));
        if left_value.is_some_and(|left| right_value.is_some_and(|right| !same_type(left, right))) {
            return Err(DataFailure::ColumnTypesDiffer {
                column: column.name.clone(),
            });
        }
        column.values.extend(other.values.clone());
    }
    Ok(DataFrameResource { columns })
}

/// Appends columns after checking row counts and column-name uniqueness.
///
/// # Errors
///
/// Returns an error when row counts differ or a column label is duplicated.
pub fn append_columns<T: Clone>(
    left: &DataFrameResource<T>,
    right: &DataFrameResource<T>,
) -> Result<DataFrameResource<T>, DataFailure> {
    let rows = left.columns.first().map_or(0, |column| column.values.len());
    let right_rows = right
        .columns
        .first()
        .map_or(0, |column| column.values.len());
    if rows != right_rows {
        return Err(DataFailure::RowCountsDiffer {
            left: rows,
            right: right_rows,
        });
    }
    if let Some(dup) = left.columns.iter().find_map(|left_column| {
        if right
            .columns
            .iter()
            .any(|right_column| left_column.name == right_column.name)
        {
            Some(left_column.name.clone())
        } else {
            None
        }
    }) {
        return Err(DataFailure::DuplicateColumn(dup));
    }
    let mut columns = left.columns.clone();
    columns.extend(right.columns.clone());
    Ok(DataFrameResource { columns })
}

/// Selects rows and columns from a `DataFrame` resource.
///
/// # Errors
///
/// Returns an error when an index is out of bounds or the selection repeats a
/// column name.
pub fn select_dataframe<T: Clone>(
    frame: &DataFrameResource<T>,
    row_indices: &[i64],
    column_indices: &[i64],
) -> Result<DataFrameResource<T>, DataFailure> {
    let row_count = frame
        .columns
        .first()
        .map_or(0, |column| column.values.len());
    let rows =
        checked_indices(row_indices, row_count).map_err(|row| DataFailure::RowIndexOutOfRange {
            row,
            count: row_count,
        })?;
    let column_total = frame.columns.len();
    let columns = checked_indices(column_indices, column_total).map_err(|column| {
        DataFailure::ColumnIndexOutOfRange {
            column,
            count: column_total,
        }
    })?;
    let columns = columns
        .iter()
        .map(|column| {
            let source = &frame.columns[*column];
            DataFrameColumn {
                name: source.name.clone(),
                values: rows.iter().map(|row| source.values[*row].clone()).collect(),
            }
        })
        .collect::<Vec<_>>();
    if let Some(name) = first_duplicate_column(&columns) {
        return Err(DataFailure::DuplicateColumn(name));
    }
    Ok(DataFrameResource { columns })
}

/// Changes a column label in-place after verifying it is unique and non-empty.
///
/// # Errors
///
/// Returns an error when the new label is invalid, duplicate, or the old label
/// is not found.
pub fn set_column_label<T>(
    frame: &mut DataFrameResource<T>,
    old_label: &str,
    new_label: &str,
) -> Result<(), DataFailure> {
    if new_label.is_empty() {
        return Err(DataFailure::InvalidArgument(
            "a column label must not be empty".into(),
        ));
    }
    if frame
        .columns
        .iter()
        .any(|column| column.name == new_label && column.name != old_label)
    {
        return Err(DataFailure::DuplicateColumn(new_label.to_string()));
    }
    let Some(column) = frame
        .columns
        .iter_mut()
        .find(|column| column.name == old_label)
    else {
        return Err(DataFailure::ColumnNotFound(old_label.to_string()));
    };
    column.name = new_label.to_string();
    Ok(())
}

/// Transposes a `DataFrame` resource, shaping column names into a first column and
/// rows into numbered row columns formatted using the provided renderer.
pub fn transpose_dataframe<T: Clone>(
    frame: &DataFrameResource<T>,
    render: impl Fn(&T) -> String,
    make_string: impl Fn(String) -> T,
) -> DataFrameResource<T> {
    let source = &frame.columns;
    let rows = source.first().map_or(0, |column| column.values.len());
    let mut columns = Vec::with_capacity(rows + 1);
    columns.push(DataFrameColumn {
        name: "Column".into(),
        values: source
            .iter()
            .map(|column| make_string(column.name.clone()))
            .collect(),
    });
    for row in 0..rows {
        columns.push(DataFrameColumn {
            name: format!("Row{row}"),
            values: source
                .iter()
                .map(|column| make_string(render(&column.values[row])))
                .collect(),
        });
    }
    DataFrameResource { columns }
}

/// Adds a new column to a `DataFrame` resource, checking for uniqueness and length consistency.
///
/// # Errors
///
/// Returns an error if the column name already exists or if the values length doesn't match the existing row count.
pub fn add_dataframe_column<T>(
    frame: &mut DataFrameResource<T>,
    name: String,
    values: Vec<T>,
) -> Result<(), DataFailure> {
    if frame.columns.iter().any(|column| column.name == name) {
        return Err(DataFailure::DuplicateColumn(name));
    }
    if let Some(first) = frame.columns.first()
        && first.values.len() != values.len()
    {
        return Err(DataFailure::ColumnLengthMismatch {
            expected: first.values.len(),
            got: values.len(),
        });
    }
    frame.columns.push(DataFrameColumn { name, values });
    Ok(())
}

/// Returns the name of the column at the specified 0-based index.
///
/// # Errors
///
/// Returns an error if the index is out of bounds.
pub fn column_name<T>(frame: &DataFrameResource<T>, index: usize) -> Result<&str, DataFailure> {
    frame
        .columns
        .get(index)
        .map(|col| col.name.as_str())
        .ok_or_else(|| DataFailure::ColumnIndexOutOfRange {
            column: i64::try_from(index).unwrap_or(i64::MAX),
            count: frame.columns.len(),
        })
}

/// Retrieves a cell value from a column by name and 0-based row index.
///
/// # Errors
///
/// Returns an error if the column is not found or the row index is out of bounds.
pub fn get_dataframe_cell<'a, T>(
    frame: &'a DataFrameResource<T>,
    column_name: &str,
    row: usize,
) -> Result<&'a T, DataFailure> {
    let Some(column) = frame
        .columns
        .iter()
        .find(|column| column.name == column_name)
    else {
        return Err(DataFailure::ColumnNotFound(column_name.to_string()));
    };
    column
        .values
        .get(row)
        .ok_or_else(|| DataFailure::RowIndexOutOfRange {
            row: i64::try_from(row).unwrap_or(i64::MAX),
            count: column.values.len(),
        })
}

/// Converts the elements of a named column in place using a conversion closure.
///
/// # Errors
///
/// Returns an error if the column is not found or if the conversion closure fails on any element.
pub fn convert_dataframe_column<T>(
    frame: &mut DataFrameResource<T>,
    column_name: &str,
    mut converter: impl FnMut(&T) -> Result<T, DataFailure>,
) -> Result<(), DataFailure> {
    let Some(column) = frame
        .columns
        .iter_mut()
        .find(|column| column.name == column_name)
    else {
        return Err(DataFailure::ColumnNotFound(column_name.to_string()));
    };
    let new_values = column
        .values
        .iter()
        .map(&mut converter)
        .collect::<Result<Vec<_>, DataFailure>>()?;
    column.values = new_values;
    Ok(())
}

/// Computes the z-score for a numeric column, returning a new single-column `DataFrameResource`.
///
/// # Errors
///
/// Returns an error if the column is not found or is empty.
pub fn zscore_column<T: Clone>(
    frame: &DataFrameResource<T>,
    column_name: &str,
    to_f64: impl Fn(&T) -> Option<f64>,
    from_f64: impl Fn(f64) -> T,
    not_available: &T,
) -> Result<DataFrameResource<T>, DataFailure> {
    let Some(column) = frame
        .columns
        .iter()
        .find(|column| column.name == column_name)
    else {
        return Err(DataFailure::ColumnNotFound(column_name.to_string()));
    };
    let mut numeric = Vec::new();
    for cell in &column.values {
        if let Some(val) = to_f64(cell) {
            numeric.push(val);
        }
    }
    let (mean, stdev) = if numeric.is_empty() {
        (f64::NAN, f64::NAN)
    } else {
        use super::stats::{Reduction, reduce_f64};
        let m = match reduce_f64("MEAN", &numeric) {
            Reduction::Float(val) => val,
            Reduction::Na => f64::NAN,
        };
        let s = match reduce_f64("STDEV", &numeric) {
            Reduction::Float(val) => val,
            Reduction::Na => f64::NAN,
        };
        (m, s)
    };
    let zscore = |x: f64| {
        if !stdev.is_finite() || stdev == 0.0 {
            f64::NAN
        } else {
            (x - mean) / stdev
        }
    };
    let values = column
        .values
        .iter()
        .map(|cell| {
            if let Some(num) = to_f64(cell) {
                from_f64(zscore(num))
            } else {
                not_available.clone()
            }
        })
        .collect();
    Ok(DataFrameResource {
        columns: vec![DataFrameColumn {
            name: column.name.clone(),
            values,
        }],
    })
}

/// Computes a `BNMath` numeric reduction on a named column in a `DataFrameResource`.
///
/// # Errors
///
/// Returns an error if the column is not found or is not numeric.
pub fn dataframe_reduce_column<T>(
    frame: &DataFrameResource<T>,
    column_name: &str,
    method: &str,
    to_f64: impl Fn(&T) -> Result<Option<f64>, DataFailure>,
) -> Result<super::stats::Reduction, DataFailure> {
    use super::stats::{Reduction, reduce_f64};
    let Some(column) = frame
        .columns
        .iter()
        .find(|column| column.name == column_name)
    else {
        return Err(DataFailure::ColumnNotFound(column_name.to_string()));
    };
    let mut numeric = Vec::new();
    for cell in &column.values {
        if let Some(val) = to_f64(cell)? {
            numeric.push(val);
        }
    }
    if matches!(method, "Min" | "Max") && numeric.is_empty() {
        return Err(DataFailure::EmptyNumericColumn);
    }
    let math_name = match method {
        "Quartile1" => "QUARTILE1",
        "Quartile3" => "QUARTILE3",
        "Stdev" => "STDEV",
        "Variance" => "VARIANCE",
        "Mean" => "MEAN",
        "Median" => "MEDIAN",
        "Mode" => "MODE",
        "Range" => "RANGE",
        "Min" => "MIN",
        "Max" => "MAX",
        _ => {
            return Err(DataFailure::InvalidArgument(format!(
                "unknown reduction method {method}"
            )));
        }
    };
    if math_name == "MIN" {
        let min_val = numeric.iter().copied().fold(f64::INFINITY, f64::min);
        Ok(Reduction::Float(min_val))
    } else if math_name == "MAX" {
        let max_val = numeric.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        Ok(Reduction::Float(max_val))
    } else {
        Ok(reduce_f64(math_name, &numeric))
    }
}

/// Copies typed values from a named column into an output slice using an adapter closure.
///
/// # Errors
///
/// Returns an error if the column is not found, destination length differs, or an element cannot be converted.
pub fn copy_dataframe_column<T, U>(
    frame: &DataFrameResource<T>,
    column_name: &str,
    target_len: usize,
    mut adapter: impl FnMut(&T) -> Result<U, DataFailure>,
) -> Result<Vec<U>, DataFailure> {
    let Some(column) = frame
        .columns
        .iter()
        .find(|column| column.name == column_name)
    else {
        return Err(DataFailure::ColumnNotFound(column_name.to_string()));
    };
    if target_len != column.values.len() {
        return Err(DataFailure::DestinationLengthMismatch {
            expected: column.values.len(),
            got: target_len,
        });
    }
    column.values.iter().map(&mut adapter).collect()
}

/// One cell of `ConvertToInteger` / `ConvertToFloat`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConvertedCell {
    Integer(i32),
    Float(f64),
    NotAvailable,
}

/// Converts one string cell as bndata.md specifies: `BNMath.VAL`, then
/// `AS INTEGER` (range-checked) or `FLOAT`; a cell whose `LEN` is 0 after
/// skipping leading spaces is `NA`. Both backends call it.
///
/// # Errors
///
/// `ConversionFailed` naming `column` when an integer is out of range.
pub fn convert_cell(
    text: &str,
    to_integer: bool,
    column: &str,
) -> Result<ConvertedCell, DataFailure> {
    if text.trim_start().is_empty() {
        return Ok(ConvertedCell::NotAvailable);
    }
    let number = bn_core_text::parse_val(text);
    if !to_integer {
        return Ok(ConvertedCell::Float(number));
    }
    let truncated = number.trunc();
    if number.is_finite() && truncated >= f64::from(i32::MIN) && truncated <= f64::from(i32::MAX) {
        #[allow(clippy::cast_possible_truncation)] // range-checked just above
        Ok(ConvertedCell::Integer(truncated as i32))
    } else {
        Err(DataFailure::ConversionFailed {
            column: column.to_string(),
            reason: "failed to convert column".into(),
        })
    }
}

/// Parses CSV text without depending on the interpreter's `Value` model.
///
/// # Errors
///
/// Returns an error when the input ends inside a quoted field.
pub fn parse_csv(text: &str, separator: char) -> Result<Vec<Vec<String>>, DataFailure> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if quoted {
            match ch {
                '"' if chars.peek() == Some(&'"') => {
                    field.push('"');
                    chars.next();
                }
                '"' => quoted = false,
                _ => field.push(ch),
            }
        } else {
            match ch {
                '"' if field.is_empty() => quoted = true,
                c if c == separator => row.push(std::mem::take(&mut field)),
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                }
                '\r' => {
                    if chars.peek() != Some(&'\n') {
                        row.push(std::mem::take(&mut field));
                        rows.push(std::mem::take(&mut row));
                    }
                }
                _ => field.push(ch),
            }
        }
    }
    if quoted {
        return Err(DataFailure::UnterminatedQuotedField);
    }
    if !field.is_empty() || !row.is_empty() || text.ends_with(separator) {
        row.push(field);
    }
    if !row.is_empty() {
        rows.push(row);
    }
    if let [.., previous, last] = rows.as_slice()
        && last.len() == 1
        && last[0].is_empty()
        && previous.len() != 1
    {
        rows.pop();
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::{
        DataFailure, DataFrameColumn, DataFrameJoin, DataFrameJoinConfig, DataFrameResource,
        append_columns, append_rows, first_duplicate_column, join_dataframes, parse_csv,
        select_dataframe,
    };

    #[test]
    fn parses_quoted_fields_and_crlf() {
        assert_eq!(
            parse_csv("name,value\r\n\"Ana, A\",1\r\n", ',').expect("CSV"),
            vec![
                vec![String::from("name"), String::from("value")],
                vec![String::from("Ana, A"), String::from("1")]
            ]
        );
    }

    #[test]
    fn rejects_unterminated_fields() {
        assert!(matches!(
            parse_csv("\"unterminated", ','),
            Err(DataFailure::UnterminatedQuotedField)
        ));
    }

    #[test]
    fn joins_typed_columns_without_interpreter_value_dependencies() {
        let left = DataFrameResource {
            columns: vec![
                DataFrameColumn {
                    name: "id".into(),
                    values: vec![1, 2],
                },
                DataFrameColumn {
                    name: "left".into(),
                    values: vec![10, 20],
                },
            ],
        };
        let right = DataFrameResource {
            columns: vec![
                DataFrameColumn {
                    name: "id".into(),
                    values: vec![2, 3],
                },
                DataFrameColumn {
                    name: "right".into(),
                    values: vec![200, 300],
                },
            ],
        };
        let missing = -1;
        let joined = join_dataframes(
            &left,
            &right,
            &DataFrameJoinConfig {
                left_label: "id",
                right_label: "id",
                kind: DataFrameJoin::Inner,
                equals: |left, right| left == right,
                is_not_available: |value| *value == -1,
                not_available: &missing,
            },
        )
        .expect("inner join");
        assert_eq!(joined.columns[0].values, vec![2]);
        assert_eq!(joined.columns[1].values, vec![20]);
        assert_eq!(joined.columns[2].values, vec![200]);
    }

    #[test]
    fn appends_rows_and_columns_with_contract_checks() {
        let left = DataFrameResource {
            columns: vec![DataFrameColumn {
                name: "id".into(),
                values: vec![1, 2],
            }],
        };
        let right = DataFrameResource {
            columns: vec![DataFrameColumn {
                name: "id".into(),
                values: vec![3],
            }],
        };
        let rows =
            append_rows(&left, &right, |_| false, |_left, _right| true).expect("append rows");
        assert_eq!(rows.columns[0].values, vec![1, 2, 3]);
        let extra = DataFrameResource {
            columns: vec![DataFrameColumn {
                name: "name".into(),
                values: vec![4, 5],
            }],
        };
        let columns = append_columns(&left, &extra).expect("append columns");
        assert_eq!(columns.columns.len(), 2);
        assert!(append_columns(&left, &right).is_err());
    }

    #[test]
    fn selects_rows_and_columns_with_bounds_and_duplicate_checks() {
        let frame = DataFrameResource {
            columns: vec![
                DataFrameColumn {
                    name: "id".into(),
                    values: vec![1, 2],
                },
                DataFrameColumn {
                    name: "score".into(),
                    values: vec![10, 20],
                },
            ],
        };
        let selected = select_dataframe(&frame, &[1], &[1]).expect("select");
        assert_eq!(selected.columns[0].values, vec![20]);
        assert!(matches!(
            select_dataframe(&frame, &[2], &[0]),
            Err(DataFailure::RowIndexOutOfRange { row: 2, count: 2 })
        ));
        assert!(matches!(
            select_dataframe(&frame, &[0], &[3]),
            Err(DataFailure::ColumnIndexOutOfRange { column: 3, .. })
        ));
        assert!(matches!(
            select_dataframe(&frame, &[0], &[0, 0]),
            Err(DataFailure::DuplicateColumn(name)) if !name.is_empty()
        ));
    }

    #[test]
    fn detects_duplicate_column_names_in_runtime_schema() {
        let columns = vec![
            DataFrameColumn::<i32> {
                name: "id".into(),
                values: vec![1],
            },
            DataFrameColumn {
                name: "id".into(),
                values: vec![2],
            },
        ];
        assert_eq!(first_duplicate_column(&columns).as_deref(), Some("id"));
    }
}
