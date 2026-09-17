#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

impl Executor<'_, '_> {
    pub(crate) fn retain_owned_value(
        &mut self,
        value: &Value,
        span: Span,
    ) -> Result<(), Diagnostic> {
        match value {
            Value::Object { handle, .. } => self.objects.retain(*handle, span),
            Value::Pointer { handle } => self.memory.retain(*handle, span),
            Value::Vector(values) => {
                for value in values {
                    self.retain_owned_value(value, span)?;
                }
                Ok(())
            }
            Value::Record { fields, .. } => {
                for value in fields.values() {
                    self.retain_owned_value(value, span)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub(crate) fn release_owned_value(
        &mut self,
        value: Value,
        span: Span,
    ) -> Result<(), Diagnostic> {
        match value {
            Value::Object { handle, class } => {
                if !self.objects.release(handle, span)? {
                    return Ok(());
                }
                self.objects.begin_delete(handle, span)?;
                let target = Value::Object {
                    handle,
                    class: class.clone(),
                };
                let destructor = self
                    .module
                    .function_of_kind(crate::ir::FunctionKind::Destructor, &class)
                    .map(|function| function.name.clone());
                let result = if let Some(destructor) = destructor {
                    self.call_named(&destructor, vec![target], span).map(|_| ())
                } else {
                    Ok(())
                };
                let instance = self.objects.get(handle, 0, span)?.clone();
                self.objects.for_each_live_mut(|candidate| {
                    for (name, field) in &mut candidate.fields {
                        if self
                            .module
                            .weak_fields
                            .contains(&(candidate.class.clone(), name.clone()))
                            && matches!(field, Value::Object { handle: other, .. } if *other == handle)
                        {
                            *field = Value::Null;
                        }
                    }
                });
                self.web_servers.remove(&handle);
                self.web_loggers.remove(&handle);
                self.web_tls_configs.remove(&handle);
                self.web_server_options.remove(&handle);
                self.web_egress_policies.remove(&handle);
                self.web_cookie_jars.remove(&handle);
                self.web_session_stores.remove(&handle);
                self.web_acls.remove(&handle);
                self.web_scrapers.remove(&handle);
                self.web_handlers.remove(&handle);
                self.web_filters.remove(&handle);
                self.web_responses.remove(&handle);
                self.web_requests.remove(&handle);
                self.web_values.remove(&handle);
                self.objects.finish_delete(handle, span)?;
                for (name, field) in instance.fields {
                    if !self.module.weak_fields.contains(&(class.clone(), name)) {
                        self.release_owned_value(field, span)?;
                    }
                }
                result
            }
            Value::Vector(values) => {
                for value in values {
                    self.release_owned_value(value, span)?;
                }
                Ok(())
            }
            Value::Record { fields, .. } => {
                for value in fields.into_values() {
                    self.release_owned_value(value, span)?;
                }
                Ok(())
            }
            Value::Pointer { handle } => {
                if self.memory.release(handle, span)? {
                    self.memory.delete(handle, span)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub(crate) fn refresh_weak_symbols(
        &self,
        symbols: &mut HashMap<SymbolId, Value>,
    ) {
        let Some(frame) = self.ownership_frames.last() else {
            return;
        };
        for symbol in &frame.weak_symbols {
            if matches!(symbols.get(symbol), Some(Value::Object { handle, .. }) if !self.objects.is_live(*handle))
            {
                symbols.insert(*symbol, Value::Null);
            }
        }
    }

    pub(crate) fn finish_ownership_frame(
        &mut self,
        symbols: &mut HashMap<SymbolId, Value>,
        returned: Option<ValueId>,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let transferred_symbol = returned.and_then(|value| {
            self.ownership_frames
                .last()
                .and_then(|frame| frame.loaded_values.get(&value).copied())
        });
        let (locals, weak) = {
            let frame = self
                .ownership_frames
                .last()
                .expect("ownership frame exists while executing a function");
            (frame.local_symbols.clone(), frame.weak_symbols.clone())
        };
        if let Some(symbol) = transferred_symbol {
            symbols.remove(&symbol);
        }
        for symbol in locals {
            if Some(symbol) == transferred_symbol || weak.contains(&symbol) {
                continue;
            }
            if let Some(value) = symbols.remove(&symbol) {
                self.release_owned_value(value, span)?;
            }
        }
        self.ownership_frames.pop();
        Ok(())
    }

    pub(crate) fn delete_value(
        &mut self,
        target: Value,
        _destructor: Option<&str>,
        span: Span,
    ) -> Result<(), Diagnostic> {
        match target {
            Value::Null => Err(runtime_error(crate::diagnostic::DiagId::NULL_POINTER_ACCESS,
                "cannot RELEASE NULL",
                span,
            )),
            Value::Pointer { handle } => self.memory.delete(handle, span),
            Value::Object { .. } | Value::Vector(_) | Value::Record { .. } => {
                self.release_owned_value(target, span)
            }
            Value::File(id) => {
                if self.files.remove(&id).is_some() {
                    Ok(())
                } else {
                    Err(runtime_error(crate::diagnostic::DiagId::DOUBLE_RELEASE,
                        "file handle was already deleted",
                        span,
                    ))
                }
            }
            Value::DataFrame(id) => {
                if self.dataframes.remove(&id).is_some() {
                    Ok(())
                } else {
                    Err(runtime_error(crate::diagnostic::DiagId::DOUBLE_RELEASE,
                        "DataFrame handle was already deleted",
                        span,
                    ))
                }
            }
            Value::LogFields(id) => {
                if self.log_fields.remove(&id).is_some() {
                    Ok(())
                } else {
                    Err(runtime_error(crate::diagnostic::DiagId::DOUBLE_RELEASE,
                        "BNLog.Fields was already deleted",
                        span,
                    ))
                }
            }
            Value::LogEntry(id) => {
                if self.log_entries.remove(&id).is_some() {
                    Ok(())
                } else {
                    Err(runtime_error(crate::diagnostic::DiagId::DOUBLE_RELEASE,
                        "BNLog.Entry was already deleted",
                        span,
                    ))
                }
            }
            Value::LogLogger(id) => {
                if self.log_loggers.remove(&id).is_some() {
                    Ok(())
                } else {
                    Err(runtime_error(crate::diagnostic::DiagId::DOUBLE_RELEASE,
                        "BNLog.Logger was already deleted",
                        span,
                    ))
                }
            }
            Value::Json(id) => {
                if self.json_values.remove(&id).is_some() {
                    Ok(())
                } else {
                    Err(runtime_error(crate::diagnostic::DiagId::DOUBLE_RELEASE,
                        "BNJson.Json was already deleted",
                        span,
                    ))
                }
            }
            _ => Ok(()),
        }
    }

    pub(crate) fn coerce_to(&self, value: Value, ty: &Type, span: Span) -> Result<Value, Diagnostic> {
        if matches!(ty, Type::Unknown) {
            return Ok(value);
        }
        if let (
            Value::Pointer { handle },
            Type::Pointer {
                length: PointerLength::Fixed(expected),
                ..
            },
        ) = (&value, ty)
        {
            let actual = u64::try_from(self.memory.len(*handle, span)?).map_err(|_| {
                runtime_error(crate::diagnostic::DiagId::POINTER_LENGTH_MISMATCH,
                    "pointer length does not fit INTEGER",
                    span,
                )
            })?;
            if actual != *expected {
                return Err(runtime_error(crate::diagnostic::DiagId::POINTER_LENGTH_MISMATCH,
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
                super::name_not_found(name, "record member", span)
            }),
            (Value::Object { handle, .. }, _) => {
                let instance = self.objects.get(*handle, 0, span)?;
                instance.fields.get(name).cloned().ok_or_else(|| {
                    super::name_not_found(name, "object member", span)
                })
            }
            _ => Err(super::name_not_found(name, "member lookup", span)),
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
                    return Err(runtime_error(crate::diagnostic::DiagId::INVALID_IR,
                        "record value disappeared",
                        span,
                    ));
                };
                fields.insert(name.to_string(), stored);
                Ok(())
            }
            _ => Err(super::name_not_found(name, "member assignment", span)),
        }
    }

    pub(crate) fn set_member_index_value(
        &mut self,
        values: &mut HashMap<ValueId, Value>,
        object: ValueId,
        name: &str,
        indices: &[usize],
        stored: Value,
        span: Span,
    ) -> Result<(), Diagnostic> {
        match values.get(&object) {
            Some(Value::Object { handle, .. }) => {
                let mut target = self
                    .objects
                    .get(*handle, 0, span)?
                    .fields
                    .get(name)
                    .cloned()
                    .ok_or_else(|| super::name_not_found(name, "object member", span))?;
                self.set_index(&mut target, indices, stored, span)?;
                self.objects
                    .get_mut(*handle, 0, span)?
                    .fields
                    .insert(name.to_string(), target);
                Ok(())
            }
            Some(Value::Record { .. }) => Err(runtime_error(crate::diagnostic::DiagId::INVALID_IR,
                "indexed value-type fields require a binding-rooted field store",
                span,
            )),
            _ => Err(super::name_not_found(name, "indexed member assignment", span)),
        }
    }

    pub(crate) fn set_field_index_path(
        &mut self,
        target: &mut Value,
        path: &[String],
        indices: &[usize],
        stored: Value,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let Some((name, rest)) = path.split_first() else {
            return self.set_index(target, indices, stored, span);
        };
        match target {
            Value::Record { fields, .. } => {
                let mut nested = fields.get(name).cloned().ok_or_else(|| {
                    super::name_not_found(name, "nested record member", span)
                })?;
                self.set_field_index_path(&mut nested, rest, indices, stored, span)?;
                fields.insert(name.clone(), nested);
                Ok(())
            }
            Value::Object { handle, .. } => {
                let handle = *handle;
                let mut nested = self
                    .objects
                    .get(handle, 0, span)?
                    .fields
                    .get(name)
                    .cloned()
                    .ok_or_else(|| {
                        super::name_not_found(name, "nested object member", span)
                    })?;
                self.set_field_index_path(&mut nested, rest, indices, stored, span)?;
                self.objects
                    .get_mut(handle, 0, span)?
                    .fields
                    .insert(name.clone(), nested);
                Ok(())
            }
            _ => Err(super::type_mismatch("record or object", "value without fields", "field index assignment", span)),
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
                    super::name_not_found(name, "nested record member", span)
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
                        super::name_not_found(name, "nested object member", span)
                    })?;
                self.set_field_path(&mut nested, rest, stored, span)?;
                self.objects
                    .get_mut(handle, 0, span)?
                    .fields
                    .insert(name.clone(), nested);
                Ok(())
            }
            _ => Err(super::type_mismatch("record or object", "value without fields", "field assignment", span)),
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
                            runtime_error(crate::diagnostic::DiagId::INVALID_IR, "vector dimension is too large", span)
                        })?;
                    &owned_dimensions
                } else {
                    dimensions
                };
                let Some(_) = dimensions.first() else {
                    return Err(runtime_error(crate::diagnostic::DiagId::INVALID_IR,
                        "vector default is missing its dimension",
                        span,
                    ));
                };
                let element_size = static_size_of(element).unwrap_or(1);
                let total = dimensions.iter().try_fold(element_size, |size, length| {
                    size.checked_mul(u64::try_from(*length).ok()?)
                });
                if total.is_none_or(|size| size > isize::MAX as u64) {
                    return Err(runtime_error(crate::diagnostic::DiagId::NUMERIC_OVERFLOW,
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
                    .ok_or_else(|| runtime_error(crate::diagnostic::DiagId::INVALID_IR, "empty alternative type", span))?,
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
            Type::HostExec => Ok(Value::Type("HOST.Exec".into())),
            _ => Err(runtime_error(crate::diagnostic::DiagId::UNINITIALIZED_VALUE,
                "type has no default value",
                span,
            )),
        }
    }

    pub(crate) fn default_named(&mut self, ir_name: &str, span: Span) -> Result<Value, Diagnostic> {
        if let Some(default) = self
            .module
            .function_of_kind(crate::ir::FunctionKind::Default, ir_name)
            .map(|function| function.name.clone())
        {
            self.call_named(&default, Vec::new(), span)
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
                return Err(super::type_mismatch("sized value", "unsized value", "SIZE operation", span));
            }
        };
        integer_from_u64(size, span)
    }
}
