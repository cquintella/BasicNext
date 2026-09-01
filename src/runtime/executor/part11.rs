#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

impl Executor<'_, '_> {
    pub(crate) fn delete_value(
        &mut self,
        target: Value,
        destructor: Option<&str>,
        span: Span,
    ) -> Result<(), Diagnostic> {
        match target {
            Value::Null => Err(runtime_error(
                "NULL_POINTER_ACCESS",
                "cannot DELETE NULL",
                span,
            )),
            Value::Pointer { handle } => self.memory.delete(handle, span),
            Value::Object { handle, .. } => {
                if let Some(destructor) = destructor {
                    self.objects.begin_delete(handle, span)?;
                    let result = self.call_named(destructor, vec![target], span);
                    self.objects.finish_delete(handle, span)?;
                    result.map(|_| ())
                } else {
                    self.web_servers.remove(&handle);
                    self.web_loggers.remove(&handle);
                    self.web_tls_configs.remove(&handle);
                    self.web_cookie_jars.remove(&handle);
                    self.web_session_stores.remove(&handle);
                    self.web_acls.remove(&handle);
                    self.web_scrapers.remove(&handle);
                    self.web_handlers.remove(&handle);
                    self.web_filters.remove(&handle);
                    self.web_responses.remove(&handle);
                    self.web_requests.remove(&handle);
                    self.web_values.remove(&handle);
                    self.objects.delete(handle, span)
                }
            }
            Value::File(id) => {
                if self.files.remove(&id).is_some() {
                    Ok(())
                } else {
                    Err(runtime_error(
                        "DOUBLE_DELETE",
                        "file handle was already deleted",
                        span,
                    ))
                }
            }
            Value::DataFrame(id) => {
                if self.dataframes.remove(&id).is_some() {
                    Ok(())
                } else {
                    Err(runtime_error(
                        "DOUBLE_DELETE",
                        "DataFrame handle was already deleted",
                        span,
                    ))
                }
            }
            Value::LogFields(id) => {
                if self.log_fields.remove(&id).is_some() {
                    Ok(())
                } else {
                    Err(runtime_error(
                        "DOUBLE_DELETE",
                        "BNLog.Fields was already deleted",
                        span,
                    ))
                }
            }
            Value::LogEntry(id) => {
                if self.log_entries.remove(&id).is_some() {
                    Ok(())
                } else {
                    Err(runtime_error(
                        "DOUBLE_DELETE",
                        "BNLog.Entry was already deleted",
                        span,
                    ))
                }
            }
            Value::LogLogger(id) => {
                if self.log_loggers.remove(&id).is_some() {
                    Ok(())
                } else {
                    Err(runtime_error(
                        "DOUBLE_DELETE",
                        "BNLog.Logger was already deleted",
                        span,
                    ))
                }
            }
            Value::Json(id) => {
                if self.json_values.remove(&id).is_some() {
                    Ok(())
                } else {
                    Err(runtime_error(
                        "DOUBLE_DELETE",
                        "BNJson.Json was already deleted",
                        span,
                    ))
                }
            }
            _ => Err(runtime_error(
                "TYPE_MISMATCH",
                "DELETE requires a pointer or CLASS reference",
                span,
            )),
        }
    }

    pub(crate) fn coerce_to(&self, value: Value, ty: &Type, span: Span) -> Result<Value, Diagnostic> {
        if let (
            Value::Pointer { handle },
            Type::Pointer {
                length: PointerLength::Fixed(expected),
                ..
            },
        ) = (&value, ty)
        {
            let actual = u64::try_from(self.memory.len(*handle, span)?).map_err(|_| {
                runtime_error(
                    "POINTER_LENGTH_MISMATCH",
                    "pointer length does not fit INTEGER",
                    span,
                )
            })?;
            if actual != *expected {
                return Err(runtime_error(
                    "POINTER_LENGTH_MISMATCH",
                    format!("pointer length {actual} does not match {expected}"),
                    span,
                ));
            }
        }
        coerce(value, ty, span)
    }

    pub(crate) fn member_of(&self, object: &Value, name: &str, span: Span) -> Result<Value, Diagnostic> {
        match (object, name) {
            (Value::Error { code, .. }, "Code") => {
                Ok(Value::Integer(i128::from(*code), IntegerType::Int32))
            }
            (Value::Error { message, .. }, "Message") => Ok(Value::String(message.clone())),
            (Value::Record { fields, .. }, _) => fields.get(name).cloned().ok_or_else(|| {
                runtime_error(
                    "NAME_NOT_FOUND",
                    format!("runtime value has no member '{name}'"),
                    span,
                )
            }),
            (Value::Object { handle, .. }, _) => {
                let instance = self.objects.get(*handle, 0, span)?;
                instance.fields.get(name).cloned().ok_or_else(|| {
                    runtime_error(
                        "NAME_NOT_FOUND",
                        format!("runtime value has no member '{name}'"),
                        span,
                    )
                })
            }
            _ => Err(runtime_error(
                "NAME_NOT_FOUND",
                format!("runtime value has no member '{name}'"),
                span,
            )),
        }
    }

    pub(crate) fn set_member_value(
        &mut self,
        values: &mut HashMap<ValueId, Value>,
        object: ValueId,
        name: &str,
        stored: Value,
        span: Span,
    ) -> Result<(), Diagnostic> {
        match values.get(&object) {
            Some(Value::Object { handle, .. }) => {
                let handle = *handle;
                self.objects
                    .get_mut(handle, 0, span)?
                    .fields
                    .insert(name.to_string(), stored);
                Ok(())
            }
            Some(Value::Record { .. }) => {
                let Some(Value::Record { fields, .. }) = values.get_mut(&object) else {
                    return Err(runtime_error(
                        "INVALID_IR",
                        "record value disappeared",
                        span,
                    ));
                };
                fields.insert(name.to_string(), stored);
                Ok(())
            }
            _ => Err(runtime_error(
                "NAME_NOT_FOUND",
                format!("runtime value has no member '{name}'"),
                span,
            )),
        }
    }

    pub(crate) fn set_field_path(
        &mut self,
        target: &mut Value,
        path: &[String],
        stored: Value,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let Some((name, rest)) = path.split_first() else {
            *target = stored;
            return Ok(());
        };
        match target {
            Value::Record { fields, .. } => {
                if rest.is_empty() {
                    fields.insert(name.clone(), stored);
                    return Ok(());
                }
                let mut nested = fields.get(name).cloned().ok_or_else(|| {
                    runtime_error(
                        "NAME_NOT_FOUND",
                        format!("runtime value has no member '{name}'"),
                        span,
                    )
                })?;
                self.set_field_path(&mut nested, rest, stored, span)?;
                fields.insert(name.clone(), nested);
                Ok(())
            }
            Value::Object { handle, .. } => {
                let handle = *handle;
                if rest.is_empty() {
                    self.objects
                        .get_mut(handle, 0, span)?
                        .fields
                        .insert(name.clone(), stored);
                    return Ok(());
                }
                let mut nested = self
                    .objects
                    .get(handle, 0, span)?
                    .fields
                    .get(name)
                    .cloned()
                    .ok_or_else(|| {
                        runtime_error(
                            "NAME_NOT_FOUND",
                            format!("runtime value has no member '{name}'"),
                            span,
                        )
                    })?;
                self.set_field_path(&mut nested, rest, stored, span)?;
                self.objects
                    .get_mut(handle, 0, span)?
                    .fields
                    .insert(name.clone(), nested);
                Ok(())
            }
            _ => Err(runtime_error("TYPE_MISMATCH", "value has no fields", span)),
        }
    }

    pub(crate) fn default_value(
        &mut self,
        ty: &Type,
        dimensions: &[usize],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match ty {
            Type::Boolean => Ok(Value::Boolean(false)),
            Type::Integer(kind) => Ok(Value::Integer(0, *kind)),
            Type::Float(kind) => Ok(Value::Float(0.0, *kind)),
            Type::String => Ok(Value::String(String::new())),
            Type::Vector {
                element,
                dimensions: declared_dimensions,
            } => {
                let owned_dimensions;
                let dimensions = if dimensions.is_empty() {
                    owned_dimensions = declared_dimensions
                        .iter()
                        .map(|dimension| usize::try_from(*dimension))
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|_| {
                            runtime_error("INVALID_IR", "vector dimension is too large", span)
                        })?;
                    &owned_dimensions
                } else {
                    dimensions
                };
                let Some(_) = dimensions.first() else {
                    return Err(runtime_error(
                        "INVALID_IR",
                        "vector default is missing its dimension",
                        span,
                    ));
                };
                let element_size = static_size_of(element).unwrap_or(1);
                let total = dimensions.iter().try_fold(element_size, |size, length| {
                    size.checked_mul(u64::try_from(*length).ok()?)
                });
                if total.is_none_or(|size| size > isize::MAX as u64) {
                    return Err(runtime_error(
                        "NUMERIC_OVERFLOW",
                        "vector allocation size overflowed",
                        span,
                    ));
                }
                let mut value = if let Some(name) = default_function_owner(element) {
                    self.default_named(&name, span)?
                } else {
                    self.default_value(element, &[], span)?
                };
                for length in dimensions.iter().rev() {
                    value = Value::Vector(vec![value; *length]);
                }
                Ok(value)
            }
            Type::Alternative(types) => self.default_value(
                types
                    .first()
                    .ok_or_else(|| runtime_error("INVALID_IR", "empty alternative type", span))?,
                dimensions,
                span,
            ),
            Type::Named(name) | Type::TypeName(name) => Ok(empty_named(name)),
            Type::ImportedNamed { module, name } | Type::ImportedTypeName { module, name } => {
                Ok(empty_named(&format!("#{}.{name}", module.0)))
            }
            Type::System => Ok(Value::Type("SYSTEM".into())),
            Type::HostClock => Ok(Value::Type("HOST.Clock".into())),
            Type::HostRandom => Ok(Value::Type("HOST.Random".into())),
            Type::HostFileSystem => Ok(Value::Type("HOST.FileSystem".into())),
            Type::HostNet => Ok(Value::Type("HOST.Net".into())),
            _ => Err(runtime_error(
                "UNINITIALIZED_VALUE",
                "type has no default value",
                span,
            )),
        }
    }

    pub(crate) fn default_named(&mut self, ir_name: &str, span: Span) -> Result<Value, Diagnostic> {
        let function_name = format!("{ir_name}.$default");
        if self
            .module
            .functions
            .iter()
            .any(|function| function.name == function_name)
        {
            self.call_named(&function_name, Vec::new(), span)
        } else {
            Ok(empty_named(ir_name))
        }
    }

    pub(crate) fn size_of_value(&self, value: &Value, span: Span) -> Result<Value, Diagnostic> {
        let size = match value {
            Value::Integer(_, kind) => integer_byte_size(*kind),
            Value::Float(_, FloatType::Float32) | Value::Date(_) | Value::Time(_) => 4,
            Value::Float(_, FloatType::Float64) => 8,
            Value::Boolean(_) => 1,
            Value::String(text) => u64::try_from(text.len()).map_err(|_| integer_overflow(span))?,
            Value::Vector(elements) => {
                let mut total = 0u64;
                for element in elements {
                    total = add_sizes(total, &self.size_of_value(element, span)?, span)?;
                }
                total
            }
            Value::Record { fields, .. } => {
                let mut total = 0u64;
                for field in fields.values() {
                    total = add_sizes(total, &self.size_of_value(field, span)?, span)?;
                }
                total
            }
            Value::Object { handle, .. } => {
                let instance = self.objects.get(*handle, 0, span)?;
                let mut total = 0u64;
                for field in instance.fields.values() {
                    total = add_sizes(total, &self.size_of_value(field, span)?, span)?;
                }
                total
            }
            _ => {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "value has no byte size",
                    span,
                ));
            }
        };
        integer_from_u64(size, span)
    }
}
