// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNData` — an external library module served through the provider seam.
//! Owns the `DataFrame` table; CSV parsing goes through the host's
//! `DataProvider`, files through the public `HOST.FileSystem` members, and
//! copy-out writes into caller regions through [`CoreContext::memory_mut`].

#![allow(
    clippy::too_many_lines, // One arm per DataFrame member, as the library contract lists them.
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss // Numeric column conversions follow the BNData contract (was executor/part9,12,13).
)]

use std::collections::HashMap;

use bn_diag::Diagnostic;
use bn_source::Span;
use bn_value::Value;

use crate::dataframe::{
    DataFrameJoin, DataFrameJoinConfig, add_dataframe_column, append_columns, append_rows,
    column_name, convert_dataframe_column, copy_dataframe_column, dataframe_reduce_column,
    get_dataframe_cell, join_dataframes, select_dataframe, set_column_label, transpose_dataframe,
    zscore_column,
};
use crate::runtime::provider::{CoreContext, Provider};
use crate::runtime::{
    collect_indices_pub as collect_indices, dataframe_index_error_pub as dataframe_index_error,
    equals_pub as equals, integer_from_count_pub as integer_from_count, integer_pub as integer,
    is_not_available_pub as is_not_available, name_not_found, parse_val_pub as parse_val,
    render_pub as render, require_arity_pub as require_arity, runtime_error_pub as runtime_error,
    type_mismatch, unsigned_indices_pub as unsigned_indices,
};
use crate::types::{FloatType, IntegerType};
use bn_rt::Reduction;

type DataFrameResource = crate::dataframe::DataFrameResource<Value>;

pub const NAME: &str = "BNData";

pub struct DataProvider {
    frames: HashMap<u64, DataFrameResource>,
    next: u64,
}

impl Default for DataProvider {
    fn default() -> Self {
        Self {
            frames: HashMap::new(),
            next: 1,
        }
    }
}

impl Provider for DataProvider {
    fn call(
        &mut self,
        core: &mut dyn CoreContext,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let name = format!("BNData.{member}");
        if matches!(member.rsplit('.').next(), Some("CONSTRUCTOR" | "$fields")) {
            return Ok(Value::Null);
        }
        if member.contains("DataFrame.") {
            return self.dataframe_call(core, &name, &arguments, span);
        }
        if member.ends_with("ReadCSV") || member.ends_with("WriteCSV") {
            return self.data_call(core, &name, &arguments, span);
        }
        Err(name_not_found(&name, "BNData member", span))
    }

    fn allocate(&mut self, class: &str, _span: Span) -> Option<Result<Value, Diagnostic>> {
        (class.rsplit('.').next() == Some("DataFrame")).then(|| {
            let id = self.next;
            self.next += 1;
            self.frames.insert(
                id,
                DataFrameResource {
                    columns: Vec::new(),
                },
            );
            Ok(Value::DataFrame(id))
        })
    }

    fn release(&mut self, value: &Value, span: Span) -> Option<Result<(), Diagnostic>> {
        let Value::DataFrame(id) = value else {
            return None;
        };
        Some(if self.frames.remove(id).is_some() {
            Ok(())
        } else {
            Err(runtime_error(
                crate::diagnostic::DiagId::DOUBLE_RELEASE,
                "DataFrame handle was already deleted",
                span,
            ))
        })
    }
}

impl DataProvider {
    fn data_call(
        &mut self,
        core: &mut dyn CoreContext,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match name.rsplit('.').next().unwrap_or_default() {
            "ReadCSV" => {
                require_arity(name, arguments, 3, span)?;
                let Value::Boolean(has_header) = arguments[1] else {
                    return Err(type_mismatch(
                        "BOOLEAN",
                        "non-BOOLEAN value",
                        "FS.File.ReadCSV header flag",
                        span,
                    ));
                };
                let Value::String(separator) = &arguments[2] else {
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "FS.File.ReadCSV separator",
                        span,
                    ));
                };
                let separator = separator.chars().collect::<Vec<_>>();
                if separator.len() != 1
                    || separator[0] == '"'
                    || separator[0] == '\n'
                    || separator[0] == '\r'
                {
                    return Ok(Value::Error {
                        code: 1,
                        message: "invalid CSV separator".into(),
                    });
                }
                let text =
                    match core.call_function("FS.File.ReadAll", arguments[..1].to_vec(), span)? {
                        Value::String(text) => text,
                        Value::Error { message, .. } => {
                            return Ok(Value::Error { code: 1, message });
                        }
                        _ => {
                            return Ok(Value::Error {
                                code: 1,
                                message: "CSV read failed".into(),
                            });
                        }
                    };
                let rows = match core.host().data_provider().read_csv(&text, separator[0]) {
                    Ok(rows) => rows,
                    Err(message) => {
                        return Ok(Value::Error { code: 1, message });
                    }
                };
                let frame = match bn_rt::frame_from_csv_rows(rows, has_header, Value::String) {
                    Ok(frame) => frame,
                    Err(message) => return Ok(Value::Error { code: 1, message }),
                };
                let id = self.next;
                self.next += 1;
                self.frames.insert(id, frame);
                Ok(Value::DataFrame(id))
            }
            "WriteCSV" => {
                require_arity(name, arguments, 4, span)?;
                let Value::File(_) = arguments[0] else {
                    return Err(type_mismatch(
                        "FS.File",
                        "non-FS.File value",
                        "FS.File.WriteCSV file",
                        span,
                    ));
                };
                let Value::DataFrame(id) = arguments[1] else {
                    return Err(type_mismatch(
                        "DataFrame",
                        "non-DataFrame value",
                        "FS.File.WriteCSV data",
                        span,
                    ));
                };
                let Value::Boolean(write_header) = arguments[2] else {
                    return Err(type_mismatch(
                        "BOOLEAN",
                        "non-BOOLEAN value",
                        "FS.File.WriteCSV header flag",
                        span,
                    ));
                };
                let Value::String(separator) = &arguments[3] else {
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "FS.File.WriteCSV separator",
                        span,
                    ));
                };
                let separator = separator.chars().collect::<Vec<_>>();
                if separator.len() != 1
                    || separator[0] == '"'
                    || separator[0] == '\n'
                    || separator[0] == '\r'
                {
                    return Ok(Value::Error {
                        code: 1,
                        message: "invalid CSV separator".into(),
                    });
                }
                let quote = |value: &Value| {
                    let text = render(value);
                    if text.contains([separator[0], '"', '\n', '\r']) {
                        format!("\"{}\"", text.replace('"', "\"\""))
                    } else {
                        text
                    }
                };
                let lines = {
                    let frame = self.frames.get(&id).ok_or_else(|| {
                        runtime_error(
                            crate::diagnostic::DiagId::USE_AFTER_RELEASE,
                            "DataFrame handle is invalid",
                            span,
                        )
                    })?;
                    let mut lines = Vec::new();
                    if write_header {
                        lines.push(
                            frame
                                .columns
                                .iter()
                                .map(|column| quote(&Value::String(column.name.clone())))
                                .collect::<Vec<_>>()
                                .join(&separator[0].to_string()),
                        );
                    }
                    let rows = frame
                        .columns
                        .first()
                        .map_or(0, |column| column.values.len());
                    lines.extend((0..rows).map(|row| {
                        frame
                            .columns
                            .iter()
                            .map(|column| quote(&column.values[row]))
                            .collect::<Vec<_>>()
                            .join(&separator[0].to_string())
                    }));
                    lines
                };
                let body = if lines.is_empty() {
                    String::new()
                } else {
                    let mut body = lines.join("\n");
                    body.push('\n');
                    body
                };
                match core.call_function(
                    "FS.File.Write",
                    vec![arguments[0].clone(), Value::String(body)],
                    span,
                )? {
                    Value::Error { code, message } => Ok(Value::Error { code, message }),
                    _ => Ok(Value::Null),
                }
            }
            _ => Err(name_not_found(name, "BNData function", span)),
        }
    }

    fn dataframe_call(
        &mut self,
        core: &mut dyn CoreContext,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let Value::DataFrame(id) = arguments.first().cloned().ok_or_else(|| {
            type_mismatch("DataFrame", "missing receiver", "DataFrame method", span)
        })?
        else {
            return Err(type_mismatch(
                "DataFrame",
                "non-DataFrame value",
                "DataFrame method",
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
            return self.dataframe_join(core, name, id, arguments, span, kind);
        }
        if method == "AppendRows" || method == "AppendColumns" {
            return self.dataframe_append(core, name, method, id, arguments, span);
        }
        if matches!(
            method,
            "AddIntegerColumn" | "AddFloatColumn" | "AddStringColumn" | "AddBooleanColumn"
        ) {
            return self.dataframe_add_column(core, name, method, id, arguments, span);
        }
        if matches!(method, "RowCount" | "ColumnCount") {
            return self.dataframe_count(core, name, method, id, arguments, span);
        }
        let frame = self.frames.get_mut(&id).ok_or_else(|| {
            runtime_error(
                crate::diagnostic::DiagId::USE_AFTER_RELEASE,
                "DataFrame handle is invalid",
                span,
            )
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
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "DataFrame.SetLabel old label",
                        span,
                    ));
                };
                let Value::String(new_label) = &arguments[2] else {
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "DataFrame.SetLabel new label",
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
                let new_id = self.next;
                self.next += 1;
                self.frames.insert(new_id, transposed);
                Ok(Value::DataFrame(new_id))
            }
            "GetString" | "GetInteger" | "GetFloat" | "GetBoolean" => {
                require_arity(name, arguments, 3, span)?;
                let (row, _) = integer(&arguments[1], span)?;
                let Value::String(column_name) = &arguments[2] else {
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "DataFrame column name",
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
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "DataFrame column name",
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
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "DataFrame column name",
                        span,
                    ));
                };
                let frame = self.frames.get(&id).ok_or_else(|| {
                    runtime_error(
                        crate::diagnostic::DiagId::USE_AFTER_RELEASE,
                        "DataFrame handle is invalid",
                        span,
                    )
                })?;
                let to_f64 = |val: &Value| match val {
                    Value::Integer(number, _) => Some(*number as f64),
                    Value::Float(number, _) => Some(*number),
                    _ => None,
                };
                let from_f64 = |val: f64| Value::Float(val, FloatType::Float64);
                let zscored =
                    match zscore_column(frame, column_name, to_f64, from_f64, &Value::NotAvailable)
                    {
                        Ok(frame) => frame,
                        Err(message) => return Ok(Value::Error { code: 1, message }),
                    };
                let new_id = self.next;
                self.next += 1;
                self.frames.insert(new_id, zscored);
                Ok(Value::DataFrame(new_id))
            }
            "Mean" | "Median" | "Quartile1" | "Quartile3" | "Mode" | "Stdev" | "Variance"
            | "Range" | "Min" | "Max" => {
                require_arity(name, arguments, 2, span)?;
                let Value::String(column_name) = &arguments[1] else {
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "DataFrame column name",
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
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "DataFrame column name",
                        span,
                    ));
                };
                let Value::Pointer { handle } = arguments[2] else {
                    return Err(type_mismatch(
                        "pointer",
                        "non-pointer value",
                        "DataFrame column copy",
                        span,
                    ));
                };
                let target_len = core.memory().len(handle, span)?;
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
                    *core.memory_mut().get_mut(handle, index, span)? = stored;
                }
                Ok(Value::Null)
            }
            "Select" | "Slice" => self.dataframe_select_slice(core, method, id, arguments, span),
            _ => Err(name_not_found(method, "DataFrame method", span)),
        }
    }

    fn dataframe_select_slice(
        &mut self,
        core: &mut dyn CoreContext,
        method: &str,
        id: u64,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let frame = self.frames.get(&id).ok_or_else(|| {
            runtime_error(
                crate::diagnostic::DiagId::USE_AFTER_RELEASE,
                "DataFrame handle is invalid",
                span,
            )
        })?;
        let selected = if method == "Select" {
            require_arity(method, arguments, 3, span)?;
            let Some(row_indices) =
                unsigned_indices(collect_indices(&arguments[1], core.memory(), span)?)
            else {
                return Ok(dataframe_index_error());
            };
            let Some(column_indices) =
                unsigned_indices(collect_indices(&arguments[2], core.memory(), span)?)
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
        let new_id = self.next;
        self.next += 1;
        self.frames.insert(new_id, selected);
        Ok(Value::DataFrame(new_id))
    }

    fn dataframe_join(
        &mut self,
        _core: &mut dyn CoreContext,
        name: &str,
        id: u64,
        arguments: &[Value],
        span: Span,
        kind: DataFrameJoin,
    ) -> Result<Value, Diagnostic> {
        require_arity(name, arguments, 4, span)?;
        let Value::DataFrame(other_id) = arguments[1] else {
            return Err(type_mismatch(
                "DataFrame",
                "non-DataFrame value",
                "join",
                span,
            ));
        };
        let Value::String(left_label) = &arguments[2] else {
            return Err(type_mismatch(
                "STRING",
                "non-STRING value",
                "join left key",
                span,
            ));
        };
        let Value::String(right_label) = &arguments[3] else {
            return Err(type_mismatch(
                "STRING",
                "non-STRING value",
                "join right key",
                span,
            ));
        };
        let left = self.frames.get(&id).ok_or_else(|| {
            runtime_error(
                crate::diagnostic::DiagId::USE_AFTER_RELEASE,
                "DataFrame handle is invalid",
                span,
            )
        })?;
        let right = self.frames.get(&other_id).ok_or_else(|| {
            runtime_error(
                crate::diagnostic::DiagId::USE_AFTER_RELEASE,
                "DataFrame handle is invalid",
                span,
            )
        })?;
        let not_available = Value::NotAvailable;
        let frame = match join_dataframes(
            left,
            right,
            &DataFrameJoinConfig {
                left_label,
                right_label,
                kind,
                equals,
                is_not_available,
                not_available: &not_available,
            },
        ) {
            Ok(frame) => frame,
            Err(message) => return Ok(Value::Error { code: 1, message }),
        };
        let new_id = self.next;
        self.next += 1;
        self.frames.insert(new_id, frame);
        Ok(Value::DataFrame(new_id))
    }

    fn dataframe_append(
        &mut self,
        _core: &mut dyn CoreContext,
        name: &str,
        method: &str,
        id: u64,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(name, arguments, 2, span)?;
        let Value::DataFrame(other_id) = arguments[1] else {
            return Err(type_mismatch(
                "DataFrame",
                "non-DataFrame value",
                method,
                span,
            ));
        };
        let left = self.frames.get(&id).ok_or_else(|| {
            runtime_error(
                crate::diagnostic::DiagId::USE_AFTER_RELEASE,
                "DataFrame handle is invalid",
                span,
            )
        })?;
        let right = self.frames.get(&other_id).ok_or_else(|| {
            runtime_error(
                crate::diagnostic::DiagId::USE_AFTER_RELEASE,
                "DataFrame handle is invalid",
                span,
            )
        })?;
        if method == "AppendRows" {
            let columns = match append_rows(left, right, is_not_available, |left, right| {
                std::mem::discriminant(left) == std::mem::discriminant(right)
            }) {
                Ok(frame) => frame,
                Err(message) => return Ok(Value::Error { code: 1, message }),
            };
            let new_id = self.next;
            self.next += 1;
            self.frames.insert(new_id, columns);
            return Ok(Value::DataFrame(new_id));
        }
        let columns = match append_columns(left, right) {
            Ok(frame) => frame,
            Err(message) => return Ok(Value::Error { code: 1, message }),
        };
        let new_id = self.next;
        self.next += 1;
        self.frames.insert(new_id, columns);
        Ok(Value::DataFrame(new_id))
    }

    fn dataframe_add_column(
        &mut self,
        core: &mut dyn CoreContext,
        name: &str,
        method: &str,
        id: u64,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let frame = self.frames.get_mut(&id).ok_or_else(|| {
            runtime_error(
                crate::diagnostic::DiagId::USE_AFTER_RELEASE,
                "DataFrame handle is invalid",
                span,
            )
        })?;

        require_arity(name, arguments, 3, span)?;
        let Value::String(column_name) = &arguments[1] else {
            return Err(type_mismatch(
                "STRING",
                "non-STRING value",
                "DataFrame.AddColumn name",
                span,
            ));
        };
        let values = match &arguments[2] {
            Value::Vector(values) => values.clone(),
            Value::Pointer { handle } => (0..core.memory().len(*handle, span)?)
                .map(|index| core.memory().get(*handle, index, span).cloned())
                .collect::<Result<Vec<_>, _>>()?,
            _ => {
                return Err(type_mismatch(
                    "vector",
                    "non-vector value",
                    "DataFrame.AddColumn values",
                    span,
                ));
            }
        };
        let type_ok = values.iter().all(|value| match method {
            "AddIntegerColumn" => matches!(value, Value::Integer(_, _)),
            "AddFloatColumn" => matches!(value, Value::Float(_, _)),
            "AddStringColumn" => matches!(value, Value::String(_)),
            "AddBooleanColumn" => matches!(value, Value::Boolean(_)),
            _ => false,
        });
        if !type_ok {
            return Ok(Value::Error {
                code: 1,
                message: "column type mismatch".into(),
            });
        }
        match add_dataframe_column(frame, column_name.clone(), values) {
            Ok(()) => Ok(Value::Null),
            Err(message) => Ok(Value::Error { code: 1, message }),
        }
    }

    fn dataframe_count(
        &mut self,
        _core: &mut dyn CoreContext,
        name: &str,
        method: &str,
        id: u64,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(name, arguments, 1, span)?;
        let frame = self.frames.get(&id).ok_or_else(|| {
            runtime_error(
                crate::diagnostic::DiagId::USE_AFTER_RELEASE,
                "DataFrame handle is invalid",
                span,
            )
        })?;
        if method == "RowCount" {
            integer_from_count(
                frame
                    .columns
                    .first()
                    .map_or(0, |column| column.values.len()),
                span,
            )
        } else {
            integer_from_count(frame.columns.len(), span)
        }
    }
}
