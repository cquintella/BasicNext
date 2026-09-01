#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

impl Executor<'_, '_> {
    pub(crate) fn file_read_bytes(
        &mut self,
        id: u64,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity("FS.File.ReadBytes", arguments, 2, span)?;
        let Value::Pointer { handle } = arguments[1] else {
            return Err(runtime_error(
                "TYPE_MISMATCH",
                "ReadBytes expects BYTE buffer",
                span,
            ));
        };
        let (bytes, eof) = {
            let resource = self
                .files
                .get_mut(&id)
                .ok_or_else(|| runtime_error("USE_AFTER_DELETE", "file handle is invalid", span))?;
            if resource.family == Some(true) {
                return Ok(Value::Error {
                    code: 1,
                    message: "file is in text mode".into(),
                });
            }
            let Some(file) = resource.file.as_mut() else {
                return Ok(Value::Error {
                    code: 1,
                    message: "file is closed".into(),
                });
            };
            let len = self.memory.len(handle, span)?;
            let mut bytes = vec![0; len];
            let count = match file.read(&mut bytes) {
                Ok(count) => count,
                Err(error) => {
                    return Ok(Value::Error {
                        code: 1,
                        message: error.to_string(),
                    });
                }
            };
            resource.family = Some(false);
            (bytes, count)
        };
        if eof == 0 {
            return Ok(Value::EndOfFile);
        }
        for (index, byte) in bytes.into_iter().take(eof).enumerate() {
            *self.memory.get_mut(handle, index, span)? =
                Value::Integer(i128::from(byte), IntegerType::Byte);
        }
        Ok(Value::Integer(
            i128::try_from(eof).unwrap_or(i128::MAX),
            IntegerType::Int32,
        ))
    }

    pub(crate) fn file_write_bytes(
        &mut self,
        id: u64,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity("FS.File.WriteBytes", arguments, 3, span)?;
        let Value::Pointer { handle } = arguments[1] else {
            return Err(runtime_error(
                "TYPE_MISMATCH",
                "WriteBytes expects BYTE buffer",
                span,
            ));
        };
        let (count, _) = integer(&arguments[2], span)?;
        let len = self.memory.len(handle, span)?;
        let count = usize::try_from(count)
            .ok()
            .filter(|count| *count <= len)
            .ok_or_else(|| {
                runtime_error(
                    "INDEX_OUT_OF_BOUNDS",
                    "byte count exceeds buffer length",
                    span,
                )
            })?;
        let bytes = (0..count)
            .map(|index| {
                let value = self.memory.get(handle, index, span)?;
                let (value, _) = integer(value, span)?;
                u8::try_from(value)
                    .map_err(|_| runtime_error("TYPE_MISMATCH", "buffer is not BYTE", span))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let resource = self
            .files
            .get_mut(&id)
            .ok_or_else(|| runtime_error("USE_AFTER_DELETE", "file handle is invalid", span))?;
        if resource.family == Some(true) {
            return Ok(Value::Error {
                code: 1,
                message: "file is in text mode".into(),
            });
        }
        let Some(file) = resource.file.as_mut() else {
            return Ok(Value::Error {
                code: 1,
                message: "file is closed".into(),
            });
        };
        if let Err(error) = file.write_all(&bytes) {
            return Ok(Value::Error {
                code: 1,
                message: error.to_string(),
            });
        }
        resource.family = Some(false);
        Ok(Value::Null)
    }

    pub(crate) fn dispatch_name(&self, name: &str, arguments: &[Value]) -> String {
        if name.ends_with(".$fields")
            || name.ends_with(".CONSTRUCTOR")
            || name.ends_with(".DESTRUCTOR")
        {
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
                return Err(runtime_error(
                    "ALLOCATION_SIZE_INVALID",
                    "numeric NEW length cannot be negative",
                    span,
                ));
            }
            usize::try_from(count).map_err(|_| {
                runtime_error(
                    "ALLOCATION_SIZE_OVERFLOW",
                    "allocation length does not fit the host",
                    span,
                )
            })?
        } else {
            1
        };
        let element_size = pointer_element_size(element).ok_or_else(|| {
            runtime_error(
                "TYPE_MISMATCH",
                "pointer element is not a numeric type",
                span,
            )
        })?;
        let bytes = u64::try_from(count)
            .ok()
            .and_then(|count| count.checked_mul(element_size))
            .ok_or_else(|| {
                runtime_error(
                    "ALLOCATION_SIZE_OVERFLOW",
                    "allocation byte size overflowed",
                    span,
                )
            })?;
        if bytes > isize::MAX as u64 {
            return Err(runtime_error(
                "ALLOCATION_TOO_LARGE",
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
            Value::Null => Err(runtime_error(
                "NULL_POINTER_ACCESS",
                "cannot index a NULL pointer",
                span,
            )),
            Value::Vector(vector) => vector.get(index).cloned().ok_or_else(|| {
                runtime_error(
                    "INDEX_OUT_OF_BOUNDS",
                    format!("index {index} is outside vector length {}", vector.len()),
                    span,
                )
            }),
            Value::Pointer { handle } => self.memory.get(*handle, index, span).cloned(),
            Value::String(text) => text
                .chars()
                .nth(index)
                .map(|character| Value::String(character.into()))
                .ok_or_else(|| {
                    runtime_error(
                        "INDEX_OUT_OF_BOUNDS",
                        format!(
                            "index {index} is outside string length {}",
                            text.chars().count()
                        ),
                        span,
                    )
                }),
            Value::HostArgs => self
                .host
                .arguments
                .get(index)
                .cloned()
                .map(Value::String)
                .ok_or_else(|| {
                    runtime_error(
                        "INDEX_OUT_OF_BOUNDS",
                        format!(
                            "index {index} is outside argument count {}",
                            self.host.arguments.len()
                        ),
                        span,
                    )
                }),
            _ => Err(runtime_error(
                "TYPE_MISMATCH",
                "value is not indexable",
                span,
            )),
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
            Value::Null => Err(runtime_error(
                "NULL_POINTER_ACCESS",
                "cannot index a NULL pointer",
                span,
            )),
            Value::Pointer { handle } => {
                if !remaining.is_empty() {
                    return Err(runtime_error(
                        "INDEX_OUT_OF_BOUNDS",
                        "pointer indexing requires one index",
                        span,
                    ));
                }
                *self.memory.get_mut(*handle, index, span)? = stored;
                Ok(())
            }
            Value::Vector(vector) => {
                let length = vector.len();
                let element = vector.get_mut(index).ok_or_else(|| {
                    runtime_error(
                        "INDEX_OUT_OF_BOUNDS",
                        format!("index {index} is outside vector length {length}"),
                        span,
                    )
                })?;
                self.set_index(element, remaining, stored, span)
            }
            _ => Err(runtime_error(
                "TYPE_MISMATCH",
                "value is not indexable",
                span,
            )),
        }
    }

}
