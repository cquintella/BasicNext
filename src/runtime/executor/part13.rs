#![allow(clippy::wildcard_imports, clippy::too_many_lines, clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
use super::*;

impl Executor<'_, '_> {
    pub(crate) fn dataframe_add_column(&mut self, name: &str, method: &str, id: u64, arguments: &[Value], span: Span) -> Result<Value, Diagnostic> {
        let frame = self.dataframes.get_mut(&id).ok_or_else(|| runtime_error("USE_AFTER_DELETE", "DataFrame handle is invalid", span))?;

            require_arity(name, arguments, 3, span)?;
            let Value::String(column_name) = &arguments[1] else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "column name must be STRING",
                    span,
                ));
            };
            if frame
                .columns
                .iter()
                .any(|column| column.name == *column_name)
            {
                return Ok(Value::Error {
                    code: 1,
                    message: "duplicate column name".into(),
                });
            }
            let values = match &arguments[2] {
                Value::Vector(values) => values.clone(),
                Value::Pointer { handle } => (0..self.memory.len(*handle, span)?)
                    .map(|index| self.memory.get(*handle, index, span).cloned())
                    .collect::<Result<Vec<_>, _>>()?,
                _ => {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "column values must be a vector",
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
            if frame
                .columns
                .first()
                .is_some_and(|column| column.values.len() != values.len())
            {
                return Ok(Value::Error {
                    code: 1,
                    message: "column length mismatch".into(),
                });
            }
            frame.columns.push(DataFrameColumn {
                name: column_name.clone(),
                values,
            });
        Ok(Value::Null)
    }

    pub(crate) fn dataframe_count(&mut self, name: &str, method: &str, id: u64, arguments: &[Value], span: Span) -> Result<Value, Diagnostic> {
        require_arity(name, arguments, 1, span)?;
        let frame = self.dataframes.get(&id).ok_or_else(|| runtime_error("USE_AFTER_DELETE", "DataFrame handle is invalid", span))?;
        if method == "RowCount" {
            integer_from_count(frame.columns.first().map_or(0, |column| column.values.len()), span)
        } else {
            integer_from_count(frame.columns.len(), span)
        }
    }
}
