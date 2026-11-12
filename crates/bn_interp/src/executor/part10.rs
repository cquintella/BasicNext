#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

impl Executor<'_, '_> {
    /// The module's callees, resolved on the first call (`bn_ir::callee`).
    pub(crate) fn callees(&self) -> &std::rc::Rc<bn_ir::callee::CalleeTable> {
        self.callees
            .get_or_init(|| std::rc::Rc::new(bn_ir::callee::CalleeTable::new(self.module)))
    }

    /// The function a call of `id` runs: for a method, the one the
    /// receiver's runtime class (or the class a lifecycle call pinned) has.
    pub(crate) fn dispatch_index(
        &self,
        callees: &bn_ir::callee::CalleeTable,
        id: bn_ir::callee::FunctionId,
        method: Option<&str>,
        arguments: &[Value],
    ) -> usize {
        method
            .and_then(|method| self.receiver_method(callees, method, arguments))
            .map_or(id.index(), bn_ir::callee::FunctionId::index)
    }

    /// The function `method` names for the object the call receives.
    pub(crate) fn receiver_method(
        &self,
        callees: &bn_ir::callee::CalleeTable,
        method: &str,
        arguments: &[Value],
    ) -> Option<bn_ir::callee::FunctionId> {
        let Some(Value::Object { handle, class }) = arguments.first() else {
            return None;
        };
        let runtime_class = self
            .pinned_dispatch
            .iter()
            .rev()
            .find(|(pinned, _)| pinned == handle)
            .map_or(&**class, |(_, pinned_class)| pinned_class.as_str());
        callees.method(self.module, runtime_class, method)
    }

    /// Splits a library type into (library name, class): `#3.Json` →
    /// (`"BNJson"`, `"Json"`) when the IR says module 3 provides `BNJson`.
    pub(crate) fn library_type<'n>(&self, name: &'n str) -> Option<(&'static str, &'n str)> {
        let rest = name.strip_prefix(bn_ir::names::MODULE_PREFIX)?;
        let (module, member) = rest.split_once('.')?;
        let library = self
            .module
            .standard_library_of(ModuleId(module.parse().ok()?))?;
        Some((library, member))
    }

    pub fn allocate_heap_object(&mut self, class: &str, span: Span) -> Result<Value, Diagnostic> {
        let field_count = self
            .module
            .field_layouts
            .get(class)
            .map_or(0, |layout| layout.fields.len());
        let handle = self.register_allocation(class, span);
        self.objects.insert(
            handle,
            1,
            Instance {
                fields: vec![Value::Null; field_count].into_boxed_slice(),
            },
            span,
        )?;
        Ok(Value::Object {
            handle,
            class: shared_string(class),
        })
    }

    pub(crate) fn allocate_region(
        &mut self,
        element: &Type,
        arguments: &[ValueId],
        values: &ValueSlots,
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
        let handle = self.register_allocation(&display_element(element), span);
        self.memory.insert(handle, count, initial, span)?;
        let code = bn_types::alternatives::member_code(element).unwrap_or(0);
        Ok(Value::Pointer {
            handle,
            element: code,
        })
    }

    pub(crate) fn index_value(
        &self,
        object: &Value,
        index: i128,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match object {
            Value::Null => Err(runtime_error(
                bn_diag::DiagId::NULL_POINTER_ACCESS,
                bn_diag::trap_texts::NULL_POINTER_INDEX,
                span,
            )),
            Value::Vector(vector) => {
                let index = checked_index(index, vector.len(), "vector", span)?;
                Ok(vector[index].clone())
            }
            Value::Pointer { handle, .. } => {
                let length = self.memory.len(*handle, span)?;
                let index = checked_index(index, length, "region", span)?;
                self.memory.get(*handle, index, span).cloned()
            }
            Value::String(text) => string_character(text, index, span),
            Value::HostArgs => {
                let arguments = &self.host.arguments;
                let context = bn_diag::trap_texts::HOST_ARGS_CONTEXT;
                let index = checked_index(index, arguments.len(), context, span)?;
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

    pub(crate) fn set_index(
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
                bn_diag::trap_texts::NULL_POINTER_INDEX,
                span,
            )),
            Value::Pointer { handle, .. } => {
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
                let vector = std::sync::Arc::make_mut(vector);
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
/// What an index error on a `STRING` names as its context.
const STRING_INDEX_CONTEXT: &str = "string";

/// `text[index]`, one Unicode scalar value: by byte when the text is ASCII,
/// otherwise in one pass; the scalar count is computed only for the error.
fn string_character(text: &str, index: i128, span: Span) -> Result<Value, Diagnostic> {
    if text.is_ascii() {
        let index = checked_index(index, text.len(), STRING_INDEX_CONTEXT, span)?;
        return Ok(Value::String(shared_string(&text[index..=index])));
    }
    usize::try_from(index)
        .ok()
        .and_then(|position| text.chars().nth(position))
        .map(|character| Value::String(shared_string(character.to_string())))
        .ok_or_else(|| {
            super::index_out_of_bounds(index, text.chars().count(), STRING_INDEX_CONTEXT, span)
        })
}

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

pub(crate) fn indexed_value<'a>(
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

#[cfg(test)]
mod tests {
    use super::checked_index;

    /// An index outside `0..length` reports the index as written, negative
    /// ones included, and the length (the native trap's facts).
    #[test]
    fn checked_index_reports_the_index_and_length() {
        let span = crate::default_span();
        assert_eq!(checked_index(2, 3, "vector", span).ok(), Some(2));
        for (index, text) in [
            (3, "Index 3 is outside vector (bound: 3)."),
            (-1, "Index -1 is outside vector (bound: 3)."),
        ] {
            let diagnostic = checked_index(index, 3, "vector", span).expect_err("outside");
            let rendered = bn_diag::Catalog::embedded_global()
                .render(diagnostic.structured.as_ref().expect("structured"))
                .expect("renders");
            assert_eq!(rendered.message, text);
        }
    }
}
