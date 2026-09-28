#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

impl Executor<'_, '_> {
    pub fn dispatch_name(&self, name: &str, arguments: &[Value]) -> String {
        if matches!(
            self.module.kind_of(name),
            Some(
                bn_ir::FunctionKind::FieldInit
                    | bn_ir::FunctionKind::Constructor
                    | bn_ir::FunctionKind::Destructor
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

    pub fn allocate_object(&mut self, class: &str, span: Span) -> Result<Value, Diagnostic> {
        let field_count = self
            .module
            .field_layouts
            .get(class)
            .map_or(0, |layout| layout.fields.len());
        let handle = self.objects.allocate(
            class,
            1,
            Instance {
                class: class.to_string(),
                fields: vec![Value::Null; field_count].into_boxed_slice(),
            },
            span,
        )?;
        Ok(Value::Object {
            handle,
            class: shared_string(class),
        })
    }

    pub fn allocate_region(
        &mut self,
        element: &Type,
        arguments: &[ValueId],
        values: &HashMap<ValueId, Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let count = if let Some(argument) = arguments.first() {
            let (count, _) = integer(value(values, *argument, span)?, span)?;
            if count < 0 {
                return Err(runtime_error(
                    bn_diag::DiagId::ALLOCATION_SIZE_INVALID,
                    "numeric NEW length cannot be negative",
                    span,
                ));
            }
            usize::try_from(count).map_err(|_| {
                runtime_error(
                    bn_diag::DiagId::ALLOCATION_SIZE_OVERFLOW,
                    "allocation length does not fit the host",
                    span,
                )
            })?
        } else {
            1
        };
        let element_size = pointer_element_size(element).ok_or_else(|| {
            super::type_mismatch(
                "numeric pointer element",
                "non-numeric type",
                "pointer allocation",
                span,
            )
        })?;
        let bytes = u64::try_from(count)
            .ok()
            .and_then(|count| count.checked_mul(element_size))
            .ok_or_else(|| {
                runtime_error(
                    bn_diag::DiagId::ALLOCATION_SIZE_OVERFLOW,
                    "allocation byte size overflowed",
                    span,
                )
            })?;
        if bytes > isize::MAX as u64 {
            return Err(runtime_error(
                bn_diag::DiagId::ALLOCATION_TOO_LARGE,
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

    pub fn index_value(
        &self,
        object: &Value,
        index: i128,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match object {
            Value::Null => Err(runtime_error(
                bn_diag::DiagId::NULL_POINTER_ACCESS,
                "cannot index a NULL pointer",
                span,
            )),
            Value::Vector(vector) => {
                let index = checked_index(index, vector.len(), "vector", span)?;
                Ok(vector[index].clone())
            }
            Value::Pointer { handle } => {
                let length = self.memory.len(*handle, span)?;
                let index = checked_index(index, length, "region", span)?;
                self.memory.get(*handle, index, span).cloned()
            }
            Value::String(text) => {
                let index = checked_index(index, text.chars().count(), "string", span)?;
                Ok(text
                    .chars()
                    .nth(index)
                    .map(|character| Value::String(shared_string(character.to_string())))
                    .expect("checked string index"))
            }
            Value::HostArgs => {
                let arguments = &self.host.arguments;
                let index = checked_index(index, arguments.len(), "HOST.Args", span)?;
                Ok(Value::String(shared_string(arguments[index].as_str())))
            }
            _ => Err(super::type_mismatch(
                "indexable value",
                "non-indexable value",
                "index operation",
                span,
            )),
        }
    }

    pub fn set_index(
        &mut self,
        target: &mut Value,
        indices: &[i128],
        stored: Value,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let Some((&index, remaining)) = indices.split_first() else {
            *target = stored;
            return Ok(());
        };
        match target {
            Value::Null => Err(runtime_error(
                bn_diag::DiagId::NULL_POINTER_ACCESS,
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
                let length = self.memory.len(*handle, span)?;
                let index = checked_index(index, length, "region", span)?;
                *self.memory.get_mut(*handle, index, span)? = stored;
                Ok(())
            }
            Value::Vector(vector) => {
                let index = checked_index(index, vector.len(), "vector", span)?;
                self.set_index(&mut vector[index], remaining, stored, span)
            }
            _ => Err(super::type_mismatch(
                "indexable value",
                "non-indexable value",
                "assignment index operation",
                span,
            )),
        }
    }
}

/// `index` as a position in a sequence of `length` elements, or
/// `INDEX_OUT_OF_BOUNDS` naming the index as written (negative ones too) and
/// the length; the native trap reports the same facts.
fn checked_index(
    index: i128,
    length: usize,
    context: &str,
    span: Span,
) -> Result<usize, Diagnostic> {
    usize::try_from(index)
        .ok()
        .filter(|index| *index < length)
        .ok_or_else(|| super::index_out_of_bounds(index, length, context, span))
}

pub fn indexed_value<'a>(
    value: &'a Value,
    indices: &[i128],
    span: Span,
) -> Result<&'a Value, Diagnostic> {
    let Some((&index, remaining)) = indices.split_first() else {
        return Ok(value);
    };
    let Value::Vector(values) = value else {
        return Err(super::type_mismatch(
            "indexable value",
            "non-indexable value",
            "nested index operation",
            span,
        ));
    };
    let index = checked_index(index, values.len(), "vector", span)?;
    indexed_value(&values[index], remaining, span)
}
