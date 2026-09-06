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
            let not_available = Value::NotAvailable;
            let frame = match join_dataframes(
                left,
                right,
                &DataFrameJoinConfig {
                    left_label,
                    right_label,
                    kind,
                    equals,
                    is_not_available: super::super::is_not_available,
                    not_available: &not_available,
                },
            ) {
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
                let columns = match append_rows(
                    left,
                    right,
                    super::super::is_not_available,
                    |left, right| std::mem::discriminant(left) == std::mem::discriminant(right),
                ) {
                    Ok(frame) => frame,
                    Err(message) => return Ok(Value::Error { code: 1, message }),
                };
                let new_id = self.next_dataframe;
                self.next_dataframe += 1;
                self.dataframes
                    .insert(new_id, columns);
                return Ok(Value::DataFrame(new_id));
            }
            let columns = match append_columns(left, right) {
                Ok(frame) => frame,
                Err(message) => return Ok(Value::Error { code: 1, message }),
            };
            let new_id = self.next_dataframe;
            self.next_dataframe += 1;
            self.dataframes.insert(new_id, columns);
            Ok(Value::DataFrame(new_id))
    }
}
