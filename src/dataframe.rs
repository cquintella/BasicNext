// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::runtime::Value;

#[derive(Clone)]
pub(crate) struct DataFrameResource {
    pub columns: Vec<DataFrameColumn>,
}

#[derive(Clone)]
pub(crate) struct DataFrameColumn {
    pub name: String,
    pub values: Vec<Value>,
}

#[derive(Clone, Copy)]
pub(crate) enum DataFrameJoin {
    Inner,
    Left,
    Right,
    Full,
}

#[allow(clippy::too_many_lines)] // The four join modes share schema validation and output shaping.
pub(crate) fn join_dataframes(
    left: &DataFrameResource,
    right: &DataFrameResource,
    left_label: &str,
    right_label: &str,
    kind: DataFrameJoin,
    equals: fn(&Value, &Value) -> bool,
) -> Result<DataFrameResource, String> {
    let Some(left_key) = left
        .columns
        .iter()
        .position(|column| column.name == left_label)
    else {
        return Err("left key column not found".into());
    };
    let Some(right_key) = right
        .columns
        .iter()
        .position(|column| column.name == right_label)
    else {
        return Err("right key column not found".into());
    };
    if right.columns.iter().enumerate().any(|(index, column)| {
        index != right_key && left.columns.iter().any(|left| left.name == column.name)
    }) {
        return Err("duplicate column label".into());
    }
    let left_rows = left.columns.first().map_or(0, |column| column.values.len());
    let right_rows = right
        .columns
        .first()
        .map_or(0, |column| column.values.len());
    let matches = |left_row: usize, right_row: usize| {
        let left_value = &left.columns[left_key].values[left_row];
        let right_value = &right.columns[right_key].values[right_row];
        !matches!(left_value, Value::NotAvailable)
            && !matches!(right_value, Value::NotAvailable)
            && equals(left_value, right_value)
    };
    let mut pairs = Vec::new();
    match kind {
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
                if !found && !matches!(kind, DataFrameJoin::Inner) {
                    pairs.push((Some(left_row), None));
                }
            }
            if matches!(kind, DataFrameJoin::Full) {
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
                                right_row.map_or(Value::NotAvailable, |row| {
                                    right.columns[right_key].values[row].clone()
                                })
                            } else {
                                Value::NotAvailable
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
                        right_row.map_or(Value::NotAvailable, |row| column.values[row].clone())
                    })
                    .collect(),
            });
        }
    }
    Ok(DataFrameResource { columns })
}

pub(crate) fn parse_csv(text: &str, separator: char) -> Result<Vec<Vec<String>>, String> {
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
        return Err("unterminated quoted field".into());
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

pub(crate) fn duplicate_column_names(columns: &[DataFrameColumn]) -> bool {
    let mut seen = std::collections::HashSet::with_capacity(columns.len());
    columns
        .iter()
        .any(|column| !seen.insert(column.name.as_str()))
}
