#![allow(clippy::wildcard_imports, clippy::too_many_lines, clippy::cast_possible_truncation, clippy::cast_precision_loss)]
use super::*;
use bn_rt::Reduction;
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
                let Ok(index) = usize::try_from(index) else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "column index out of bounds".into(),
                    });
                };
                match column_name(frame, index) {
                    Ok(name) => Ok(Value::String(name.to_string())),
                    Err(message) => Ok(Value::Error { code: 1, message }),
                }
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
                match set_column_label(frame, old_label, new_label) {
                    Ok(()) => Ok(Value::Null),
                    Err(message) => Ok(Value::Error { code: 1, message }),
                }
            }
            "Transpose" => {
                require_arity(name, arguments, 1, span)?;
                let transposed = transpose_dataframe(frame, render, Value::String);
                let new_id = self.next_dataframe;
                self.next_dataframe += 1;
                self.dataframes.insert(new_id, transposed);
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
                let Ok(row) = usize::try_from(row) else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "row index out of bounds".into(),
                    });
                };
                let value = match get_dataframe_cell(frame, column_name, row) {
                    Ok(value) => value.clone(),
                    Err(message) => return Ok(Value::Error { code: 1, message }),
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
                let converter = |value: &Value| {
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
                };
                match convert_dataframe_column(frame, column_name, converter) {
                    Ok(()) => Ok(Value::Null),
                    Err("column not found") => Ok(Value::Error {
                        code: 1,
                        message: "column not found".into(),
                    }),
                    Err(_) => Ok(Value::Error {
                        code: 1,
                        message: "column conversion failed".into(),
                    }),
                }
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
                let to_f64 = |val: &Value| match val {
                    Value::Integer(number, _) => Some(*number as f64),
                    Value::Float(number, _) => Some(*number),
                    _ => None,
                };
                let from_f64 = |val: f64| Value::Float(val, FloatType::Float64);
                let zscored = match zscore_column(frame, column_name, to_f64, from_f64, &Value::NotAvailable) {
                    Ok(frame) => frame,
                    Err(message) => return Ok(Value::Error { code: 1, message }),
                };
                let new_id = self.next_dataframe;
                self.next_dataframe += 1;
                self.dataframes.insert(new_id, zscored);
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
                let to_f64 = |val: &Value| match val {
                    Value::Integer(number, _) => Ok(Some(*number as f64)),
                    Value::Float(number, _) => Ok(Some(*number)),
                    Value::NotAvailable => Ok(None),
                    _ => Err("column is not numeric"),
                };
                match dataframe_reduce_column(frame, column_name, method, to_f64) {
                    Ok(Reduction::Float(val)) => Ok(Value::Float(val, FloatType::Float64)),
                    Ok(Reduction::Na) => Ok(Value::NotAvailable),
                    Err(message) => Ok(Value::Error {
                        code: 1,
                        message: message.into(),
                    }),
                }
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
                let target_len = self.memory.len(handle, span)?;
                let adapter = |value: &Value| match (method, value) {
                    ("CopyIntegerColumn", Value::Integer(number, _)) => {
                        Ok(Value::Integer(*number, IntegerType::Int32))
                    }
                    ("CopyFloatColumn", Value::Float(number, _)) => {
                        Ok(Value::Float(*number, FloatType::Float64))
                    }
                    _ => Err("column type or NA mismatch"),
                };
                let values = match copy_dataframe_column(frame, column_name, target_len, adapter) {
                    Ok(values) => values,
                    Err("column not found") => {
                        return Ok(Value::Error {
                            code: 1,
                            message: "column not found".into(),
                        });
                    }
                    Err("destination length mismatch") => {
                        return Ok(Value::Error {
                            code: 1,
                            message: "destination length mismatch".into(),
                        });
                    }
                    Err(_) => {
                        return Ok(Value::Error {
                            code: 1,
                            message: "column type or NA mismatch".into(),
                        });
                    }
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
        let selected = if method == "Select" {
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
                select_dataframe(frame, &row_indices, &column_indices)
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
                bn_rt::slice_dataframe(frame, values[0], values[1], values[2], values[3])
            };
            let selected = match selected {
                Ok(frame) => frame,
                Err(message) if message == "DataFrame index out of bounds" => {
                    return Ok(dataframe_index_error());
                }
                Err(message) => return Ok(Value::Error { code: 1, message }),
            };
            let new_id = self.next_dataframe;
            self.next_dataframe += 1;
            self.dataframes.insert(new_id, selected);
            Ok(Value::DataFrame(new_id))
    }
}
