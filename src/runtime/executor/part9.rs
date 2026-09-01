#![allow(clippy::wildcard_imports, clippy::too_many_lines, clippy::cast_possible_truncation, clippy::cast_precision_loss)]
use super::*;
impl Executor<'_, '_> {
    pub(crate) fn dataframe_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let Value::DataFrame(id) = arguments
            .first()
            .cloned()
            .ok_or_else(|| runtime_error("TYPE_MISMATCH", "DataFrame receiver missing", span))?
        else {
            return Err(runtime_error(
                "TYPE_MISMATCH",
                "receiver is not DataFrame",
                span,
            ));
        };
        let method = name.rsplit('.').next().unwrap_or_default();
        if let Some(kind) = match method {
            "Join" => Some(DataFrameJoin::Inner),
            "LeftJoin" => Some(DataFrameJoin::Left),
            "RightJoin" => Some(DataFrameJoin::Right),
            "FullJoin" => Some(DataFrameJoin::Full),
            _ => None,
        } {
            return self.dataframe_join(name, id, arguments, span, kind);
        }
        if method == "AppendRows" || method == "AppendColumns" {
            return self.dataframe_append(name, method, id, arguments, span);
        }
        if matches!(method, "AddIntegerColumn" | "AddFloatColumn" | "AddStringColumn" | "AddBooleanColumn") {
            return self.dataframe_add_column(name, method, id, arguments, span);
        }
        if matches!(method, "RowCount" | "ColumnCount") {
            return self.dataframe_count(name, method, id, arguments, span);
        }
        let frame = self.dataframes.get_mut(&id).ok_or_else(|| {
            runtime_error("USE_AFTER_DELETE", "DataFrame handle is invalid", span)
        })?;
        match method {
            "ColumnName" => {
                require_arity(name, arguments, 2, span)?;
                let (index, _) = integer(&arguments[1], span)?;
                usize::try_from(index)
                    .ok()
                    .and_then(|index| frame.columns.get(index))
                    .map_or_else(
                        || {
                            Ok(Value::Error {
                                code: 1,
                                message: "column index out of bounds".into(),
                            })
                        },
                        |column| Ok(Value::String(column.name.clone())),
                    )
            }
            "SetLabel" => {
                require_arity(method, arguments, 3, span)?;
                let Value::String(old_label) = &arguments[1] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "old label must be STRING",
                        span,
                    ));
                };
                let Value::String(new_label) = &arguments[2] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "new label must be STRING",
                        span,
                    ));
                };
                if new_label.is_empty()
                    || frame
                        .columns
                        .iter()
                        .any(|column| column.name == *new_label && column.name != *old_label)
                {
                    return Ok(Value::Error {
                        code: 1,
                        message: "invalid or duplicate column label".into(),
                    });
                }
                let Some(column) = frame
                    .columns
                    .iter_mut()
                    .find(|column| column.name == *old_label)
                else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "column not found".into(),
                    });
                };
                column.name.clone_from(new_label);
                Ok(Value::Null)
            }
            "Transpose" => {
                require_arity(name, arguments, 1, span)?;
                let source = frame.columns.clone();
                let rows = source.first().map_or(0, |column| column.values.len());
                let mut columns = Vec::with_capacity(rows + 1);
                columns.push(DataFrameColumn {
                    name: "Column".into(),
                    values: source
                        .iter()
                        .map(|column| Value::String(column.name.clone()))
                        .collect(),
                });
                for row in 0..rows {
                    columns.push(DataFrameColumn {
                        name: format!("Row{row}"),
                        values: source
                            .iter()
                            .map(|column| Value::String(render(&column.values[row])))
                            .collect(),
                    });
                }
                let new_id = self.next_dataframe;
                self.next_dataframe += 1;
                self.dataframes
                    .insert(new_id, DataFrameResource { columns });
                Ok(Value::DataFrame(new_id))
            }
            "GetString" | "GetInteger" | "GetFloat" | "GetBoolean" => {
                require_arity(name, arguments, 3, span)?;
                let (row, _) = integer(&arguments[1], span)?;
                let Value::String(column_name) = &arguments[2] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "column name must be STRING",
                        span,
                    ));
                };
                let Some(column) = frame
                    .columns
                    .iter()
                    .find(|column| column.name == *column_name)
                else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "column not found".into(),
                    });
                };
                let Some(value) = usize::try_from(row)
                    .ok()
                    .and_then(|row| column.values.get(row))
                    .cloned()
                else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "row index out of bounds".into(),
                    });
                };
                match method {
                    "GetString" if matches!(value, Value::String(_) | Value::NotAvailable) => {
                        Ok(value)
                    }
                    "GetInteger" if matches!(value, Value::Integer(_, _) | Value::NotAvailable) => {
                        Ok(value)
                    }
                    "GetFloat" if matches!(value, Value::Float(_, _) | Value::NotAvailable) => {
                        Ok(value)
                    }
                    "GetBoolean" if matches!(value, Value::Boolean(_) | Value::NotAvailable) => {
                        Ok(value)
                    }
                    _ => Ok(Value::Error {
                        code: 1,
                        message: "column type mismatch".into(),
                    }),
                }
            }
            "ConvertToInteger" | "ConvertToFloat" => {
                require_arity(name, arguments, 2, span)?;
                let Value::String(column_name) = &arguments[1] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "column name must be STRING",
                        span,
                    ));
                };
                let Some(index) = frame
                    .columns
                    .iter()
                    .position(|column| column.name == *column_name)
                else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "column not found".into(),
                    });
                };
                let source = &frame.columns[index].values;
                let converted = source
                    .iter()
                    .map(|value| {
                        let Value::String(text) = value else {
                            return Err("column is not STRING");
                        };
                        if text.trim().is_empty() {
                            return Ok(Value::NotAvailable);
                        }
                        let number = parse_val(text);
                        if method == "ConvertToFloat" {
                            Ok(Value::Float(number, FloatType::Float64))
                        } else if number.is_finite()
                            && number.trunc() >= f64::from(i32::MIN)
                            && number.trunc() <= f64::from(i32::MAX)
                        {
                            Ok(Value::Integer(number.trunc() as i128, IntegerType::Int32))
                        } else {
                            Err("integer conversion overflow")
                        }
                    })
                    .collect::<Result<Vec<_>, &str>>();
                let Ok(converted) = converted else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "column conversion failed".into(),
                    });
                };
                frame.columns[index].values = converted;
                Ok(Value::Null)
            }
            "ZScore" => {
                require_arity(name, arguments, 2, span)?;
                let Value::String(column_name) = &arguments[1] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "column name must be STRING",
                        span,
                    ));
                };
                let frame = self.dataframes.get(&id).ok_or_else(|| {
                    runtime_error("USE_AFTER_DELETE", "DataFrame handle is invalid", span)
                })?;
                let Some(column) = frame
                    .columns
                    .iter()
                    .find(|column| column.name == *column_name)
                else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "column not found".into(),
                    });
                };
                let label = column.name.clone();
                let cells = column.values.clone();
                let numeric = match dataframe_numeric_values(column) {
                    Ok(values) => values,
                    Err(message) => {
                        return Ok(Value::Error {
                            code: 1,
                            message: message.into(),
                        });
                    }
                };
                let mean = match builtin(
                    "BNMath.MEAN",
                    &[Value::Vector(numeric.clone())],
                    span,
                    &self.memory,
                )? {
                    Value::Float(value, _) => value,
                    _ => f64::NAN,
                };
                let stdev = match builtin(
                    "BNMath.STDEV",
                    &[Value::Vector(numeric)],
                    span,
                    &self.memory,
                )? {
                    Value::Float(value, _) => value,
                    _ => f64::NAN,
                };
                let zscore = |x: f64| {
                    if !stdev.is_finite() || stdev == 0.0 {
                        f64::NAN
                    } else {
                        (x - mean) / stdev
                    }
                };
                let values = cells
                    .into_iter()
                    .map(|value| match value {
                        Value::Integer(number, _) => {
                            Value::Float(zscore(number as f64), FloatType::Float64)
                        }
                        Value::Float(number, _) => Value::Float(zscore(number), FloatType::Float64),
                        _ => Value::NotAvailable,
                    })
                    .collect();
                let new_id = self.next_dataframe;
                self.next_dataframe += 1;
                self.dataframes.insert(
                    new_id,
                    DataFrameResource {
                        columns: vec![DataFrameColumn {
                            name: label,
                            values,
                        }],
                    },
                );
                Ok(Value::DataFrame(new_id))
            }
            "Mean" | "Median" | "Quartile1" | "Quartile3" | "Mode" | "Stdev" | "Variance"
            | "Range" | "Min" | "Max" => {
                require_arity(name, arguments, 2, span)?;
                let Value::String(column_name) = &arguments[1] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "column name must be STRING",
                        span,
                    ));
                };
                let Some(column) = frame
                    .columns
                    .iter()
                    .find(|column| column.name == *column_name)
                else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "column not found".into(),
                    });
                };
                let values = match dataframe_numeric_values(column) {
                    Ok(values) => values,
                    Err(message) => {
                        return Ok(Value::Error {
                            code: 1,
                            message: message.into(),
                        });
                    }
                };
                if matches!(method, "Min" | "Max") && values.is_empty() {
                    return Ok(Value::Error {
                        code: 1,
                        message: "empty numeric column".into(),
                    });
                }
                let math_name = match method {
                    "Quartile1" => "QUARTILE1".to_string(),
                    "Quartile3" => "QUARTILE3".to_string(),
                    "Stdev" => "STDEV".to_string(),
                    "Variance" => "VARIANCE".to_string(),
                    other => other.to_ascii_uppercase(),
                };
                builtin(
                    &format!("BNMath.{math_name}"),
                    &[Value::Vector(values)],
                    span,
                    &self.memory,
                )
            }
            "CopyIntegerColumn" | "CopyFloatColumn" => {
                require_arity(name, arguments, 3, span)?;
                let Value::String(column_name) = &arguments[1] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "column name must be STRING",
                        span,
                    ));
                };
                let Value::Pointer { handle } = arguments[2] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "destination must be a pointer",
                        span,
                    ));
                };
                let Some(column) = frame
                    .columns
                    .iter()
                    .find(|column| column.name == *column_name)
                else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "column not found".into(),
                    });
                };
                if self.memory.len(handle, span)? != column.values.len() {
                    return Ok(Value::Error {
                        code: 1,
                        message: "destination length mismatch".into(),
                    });
                }
                let values = column
                    .values
                    .iter()
                    .cloned()
                    .map(|value| match (method, value) {
                        ("CopyIntegerColumn", Value::Integer(number, _)) => {
                            Ok(Value::Integer(number, IntegerType::Int32))
                        }
                        ("CopyFloatColumn", Value::Float(number, _)) => {
                            Ok(Value::Float(number, FloatType::Float64))
                        }
                        _ => Err(()),
                    })
                    .collect::<Result<Vec<_>, _>>();
                let Ok(values) = values else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "column type or NA mismatch".into(),
                    });
                };
                for (index, stored) in values.into_iter().enumerate() {
                    *self.memory.get_mut(handle, index, span)? = stored;
                }
                Ok(Value::Null)
            }
            "Select" | "Slice" => self.dataframe_select_slice(method, id, arguments, span),
            _ => Err(runtime_error(
                "NAME_NOT_FOUND",
                "unknown DataFrame method",
                span,
            )),
        }
        }

    pub(crate) fn dataframe_select_slice(&mut self, method: &str, id: u64, arguments: &[Value], span: Span) -> Result<Value, Diagnostic> {
        let frame = self.dataframes.get(&id).ok_or_else(|| runtime_error("USE_AFTER_DELETE", "DataFrame handle is invalid", span))?;
        let (row_indices, column_indices) = if method == "Select" {
                require_arity(method, arguments, 3, span)?;
                let Some(row_indices) =
                    unsigned_indices(collect_indices(&arguments[1], &self.memory, span)?)
                else {
                    return Ok(dataframe_index_error());
                };
                let Some(column_indices) =
                    unsigned_indices(collect_indices(&arguments[2], &self.memory, span)?)
                else {
                    return Ok(dataframe_index_error());
                };
                (row_indices, column_indices)
            } else {
                require_arity(method, arguments, 5, span)?;
                let (start_row, _) = integer(&arguments[1], span)?;
                let (row_count, _) = integer(&arguments[2], span)?;
                let (start_col, _) = integer(&arguments[3], span)?;
                let (col_count, _) = integer(&arguments[4], span)?;
                let values = [start_row, row_count, start_col, col_count]
                    .into_iter()
                    .map(|value| usize::try_from(value).ok())
                    .collect::<Option<Vec<_>>>();
                let Some(values) = values else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "negative slice bound".into(),
                    });
                };
                (
                    (values[0]..values[0].saturating_add(values[1])).collect(),
                    (values[2]..values[2].saturating_add(values[3])).collect(),
                )
            };
            let source = frame.columns.clone();
            let row_count = source.first().map_or(0, |column| column.values.len());
            if row_indices.iter().any(|row| *row >= row_count)
                || column_indices.iter().any(|column| *column >= source.len())
            {
                return Ok(dataframe_index_error());
        }
            let columns: Vec<DataFrameColumn> = column_indices
                .into_iter()
                .map(|column| {
                    let source = &source[column];
                    DataFrameColumn {
                        name: source.name.clone(),
                        values: row_indices
                            .iter()
                            .map(|row| source.values[*row].clone())
                            .collect(),
                    }
                })
                .collect();
            if duplicate_column_names(&columns) {
                return Ok(Value::Error {
                    code: 1,
                    message: "duplicate column name".into(),
                });
            }
            let new_id = self.next_dataframe;
            self.next_dataframe += 1;
            self.dataframes
                .insert(new_id, DataFrameResource { columns });
            Ok(Value::DataFrame(new_id))
    }
}
