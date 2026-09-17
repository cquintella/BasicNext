#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

impl Executor<'_, '_> {
    pub(crate) fn dispatch_name(&self, name: &str, arguments: &[Value]) -> String {
        if matches!(
            self.module.kind_of(name),
            Some(
                crate::ir::FunctionKind::FieldInit
                    | crate::ir::FunctionKind::Constructor
                    | crate::ir::FunctionKind::Destructor
            )
        ) {
            return name.to_string();
        }
        let Some(Value::Object { class, .. }) = arguments.first() else {
            return name.to_string();
        };
        let Some(method) = name.rsplit('.').next() else {
            return name.to_string();
        };
        let dispatched = arguments
            .first()
            .and_then(|value| match value {
                Value::Object { handle, .. } => self
                    .pinned_dispatch
                    .iter()
                    .rev()
                    .find(|(pinned, _)| pinned == handle)
                    .map(|(_, class)| format!("{class}.{method}")),
                _ => None,
            })
            .unwrap_or_else(|| format!("{class}.{method}"));
        if self
            .module
            .functions
            .iter()
            .any(|function| function.name == dispatched)
        {
            dispatched
        } else {
            name.to_string()
        }
    }

    pub(crate) fn allocate_object(&mut self, class: &str, span: Span) -> Result<Value, Diagnostic> {
        let handle = self.objects.allocate(
            class,
            1,
            Instance {
                class: class.to_string(),
                fields: HashMap::new(),
            },
            span,
        )?;
        Ok(Value::Object {
            handle,
            class: class.to_string(),
        })
    }

    pub(crate) fn allocate_region(
        &mut self,
        element: &Type,
        arguments: &[ValueId],
        values: &HashMap<ValueId, Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let count = if let Some(argument) = arguments.first() {
            let (count, _) = integer(value(values, *argument, span)?, span)?;
            if count < 0 {
                return Err(runtime_error(crate::diagnostic::DiagId::ALLOCATION_SIZE_INVALID,
                    "numeric NEW length cannot be negative",
                    span,
                ));
            }
            usize::try_from(count).map_err(|_| {
                runtime_error(crate::diagnostic::DiagId::ALLOCATION_SIZE_OVERFLOW,
                    "allocation length does not fit the host",
                    span,
                )
            })?
        } else {
            1
        };
        let element_size = pointer_element_size(element).ok_or_else(|| {
            super::type_mismatch("numeric pointer element", "non-numeric type", "pointer allocation", span)
        })?;
        let bytes = u64::try_from(count)
            .ok()
            .and_then(|count| count.checked_mul(element_size))
            .ok_or_else(|| {
                runtime_error(crate::diagnostic::DiagId::ALLOCATION_SIZE_OVERFLOW,
                    "allocation byte size overflowed",
                    span,
                )
            })?;
        if bytes > isize::MAX as u64 {
            return Err(runtime_error(crate::diagnostic::DiagId::ALLOCATION_TOO_LARGE,
                "allocation exceeds the host limit",
                span,
            ));
        }
        let initial = pointer_element_default(element, span)?;
        let handle = self
            .memory
            .allocate(display_element(element), count, initial, span)?;
        Ok(Value::Pointer { handle })
    }

    pub(crate) fn index_value(&self, object: &Value, index: usize, span: Span) -> Result<Value, Diagnostic> {
        match object {
            Value::Null => Err(runtime_error(crate::diagnostic::DiagId::NULL_POINTER_ACCESS,
                "cannot index a NULL pointer",
                span,
            )),
            Value::Vector(vector) => vector.get(index).cloned().ok_or_else(|| {
                super::index_out_of_bounds(index, vector.len(), "vector", span)
            }),
            Value::Pointer { handle } => self.memory.get(*handle, index, span).cloned(),
            Value::String(text) => text
                .chars()
                .nth(index)
                .map(|character| Value::String(character.into()))
                .ok_or_else(|| {
                    super::index_out_of_bounds(index, text.chars().count(), "string", span)
                }),
            Value::HostArgs => self
                .host
                .arguments
                .get(index)
                .cloned()
                .map(Value::String)
                .ok_or_else(|| {
                    super::index_out_of_bounds(index, self.host.arguments.len(), "HOST.Args", span)
                }),
            _ => Err(super::type_mismatch("indexable value", "non-indexable value", "index operation", span)),
        }
    }

    pub(crate) fn set_index(
        &mut self,
        target: &mut Value,
        indices: &[usize],
        stored: Value,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let Some((&index, remaining)) = indices.split_first() else {
            *target = stored;
            return Ok(());
        };
        match target {
            Value::Null => Err(runtime_error(crate::diagnostic::DiagId::NULL_POINTER_ACCESS,
                "cannot index a NULL pointer",
                span,
            )),
            Value::Pointer { handle } => {
                if !remaining.is_empty() {
                    return Err(super::index_out_of_bounds(
                        remaining.len() + 1,
                        1,
                        "pointer indexing",
                        span,
                    ));
                }
                *self.memory.get_mut(*handle, index, span)? = stored;
                Ok(())
            }
            Value::Vector(vector) => {
                let length = vector.len();
                let element = vector.get_mut(index).ok_or_else(|| {
                    super::index_out_of_bounds(index, length, "vector", span)
                })?;
                self.set_index(element, remaining, stored, span)
            }
            _ => Err(super::type_mismatch("indexable value", "non-indexable value", "assignment index operation", span)),
        }
    }

}

pub(crate) fn indexed_value<'a>(
    value: &'a Value,
    indices: &[usize],
    span: Span,
) -> Result<&'a Value, Diagnostic> {
    let Some((&index, remaining)) = indices.split_first() else {
        return Ok(value);
    };
    let Value::Vector(values) = value else {
        return Err(super::type_mismatch("indexable value", "non-indexable value", "nested index operation", span));
    };
    let element = values.get(index).ok_or_else(|| {
        super::index_out_of_bounds(index, values.len(), "vector", span)
    })?;
    indexed_value(element, remaining, span)
}
