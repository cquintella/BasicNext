#![allow(clippy::wildcard_imports, clippy::too_many_lines, clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
use super::*;

impl Executor<'_, '_> {
    pub(crate) fn dataframe_join(&mut self, name: &str, id: u64, arguments: &[Value], span: Span, kind: DataFrameJoin) -> Result<Value, Diagnostic> {
            require_arity(name, arguments, 4, span)?;
            let Value::DataFrame(other_id) = arguments[1] else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "join expects DataFrame",
                    span,
                ));
            };
            let Value::String(left_label) = &arguments[2] else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "left key must be STRING",
                    span,
                ));
            };
            let Value::String(right_label) = &arguments[3] else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "right key must be STRING",
                    span,
                ));
            };
            let left = self.dataframes.get(&id).ok_or_else(|| {
                runtime_error("USE_AFTER_DELETE", "DataFrame handle is invalid", span)
            })?;
            let right = self.dataframes.get(&other_id).ok_or_else(|| {
                runtime_error("USE_AFTER_DELETE", "DataFrame handle is invalid", span)
            })?;
            let frame = match join_dataframes(left, right, left_label, right_label, kind, equals) {
                Ok(frame) => frame,
                Err(message) => return Ok(Value::Error { code: 1, message }),
            };
            let new_id = self.next_dataframe;
            self.next_dataframe += 1;
            self.dataframes.insert(new_id, frame);
            Ok(Value::DataFrame(new_id))
    }

    pub(crate) fn dataframe_append(&mut self, name: &str, method: &str, id: u64, arguments: &[Value], span: Span) -> Result<Value, Diagnostic> {
            require_arity(name, arguments, 2, span)?;
            let Value::DataFrame(other_id) = arguments[1] else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "AppendRows/AppendColumns expects DataFrame",
                    span,
                ));
            };
            let left = self.dataframes.get(&id).ok_or_else(|| {
                runtime_error("USE_AFTER_DELETE", "DataFrame handle is invalid", span)
            })?;
            let right = self.dataframes.get(&other_id).ok_or_else(|| {
                runtime_error("USE_AFTER_DELETE", "DataFrame handle is invalid", span)
            })?;
            if method == "AppendRows" {
                if left.columns.len() != right.columns.len()
                    || left
                        .columns
                        .iter()
                        .zip(&right.columns)
                        .any(|(left, right)| left.name != right.name)
                {
                    return Ok(Value::Error {
                        code: 1,
                        message: "column layouts differ".into(),
                    });
                }
                let mut columns = left.columns.clone();
                for (column, other) in columns.iter_mut().zip(&right.columns) {
                    let left_type = column
                        .values
                        .iter()
                        .find(|value| !matches!(value, Value::NotAvailable))
                        .map(std::mem::discriminant);
                    let right_type = other
                        .values
                        .iter()
                        .find(|value| !matches!(value, Value::NotAvailable))
                        .map(std::mem::discriminant);
                    if left_type.is_some() && right_type.is_some() && left_type != right_type {
                        return Ok(Value::Error {
                            code: 1,
                            message: "column types differ".into(),
                        });
                    }
                    column.values.extend(other.values.clone());
                }
                let new_id = self.next_dataframe;
                self.next_dataframe += 1;
                self.dataframes
                    .insert(new_id, DataFrameResource { columns });
                return Ok(Value::DataFrame(new_id));
            }
            let rows = left.columns.first().map_or(0, |column| column.values.len());
            if rows
                != right
                    .columns
                    .first()
                    .map_or(0, |column| column.values.len())
            {
                return Ok(Value::Error {
                    code: 1,
                    message: "row counts differ".into(),
                });
            }
            if left.columns.iter().any(|left_column| {
                right
                    .columns
                    .iter()
                    .any(|right_column| left_column.name == right_column.name)
            }) {
                return Ok(Value::Error {
                    code: 1,
                    message: "duplicate column label".into(),
                });
            }
            let mut columns = left.columns.clone();
            columns.extend(right.columns.clone());
            let new_id = self.next_dataframe;
            self.next_dataframe += 1;
            self.dataframes
                .insert(new_id, DataFrameResource { columns });
            Ok(Value::DataFrame(new_id))
    }
}
