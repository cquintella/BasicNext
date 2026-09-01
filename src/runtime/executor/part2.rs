#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

impl Executor<'_, '_> {
    pub(crate) fn instruction(
        &mut self,
        instruction: &Instruction,
        symbols: &mut HashMap<SymbolId, Value>,
        values: &mut HashMap<ValueId, Value>,
    ) -> Result<(), Diagnostic> {
        match instruction {
            Instruction::Constant {
                destination,
                value: constant,
                ty,
                span,
            } => set(values, *destination, constant_value(constant, ty, *span)?),
            Instruction::Default {
                destination,
                ty,
                dimensions,
                dynamic_dimensions,
                span,
            } => {
                let mut evaluated = dimensions.clone();
                for dimension in dynamic_dimensions {
                    let value = integer(
                        values.get(dimension).ok_or_else(|| {
                            runtime_error(
                                "UNINITIALIZED_VALUE",
                                "vector dimension is unavailable",
                                *span,
                            )
                        })?,
                        *span,
                    )?
                    .0;
                    let dimension = usize::try_from(value).map_err(|_| {
                        runtime_error(
                            if value < 0 {
                                "INVALID_VECTOR_DIMENSION"
                            } else {
                                "NUMERIC_OVERFLOW"
                            },
                            "vector dimension must be a non-negative size",
                            *span,
                        )
                    })?;
                    evaluated.push(dimension);
                }
                set(
                    values,
                    *destination,
                    self.default_value(ty, &evaluated, *span)?,
                );
            }
            Instruction::Load {
                destination,
                symbol,
                span,
                ..
            } => {
                let loaded = symbols.get(symbol).cloned().ok_or_else(|| {
                    runtime_error("UNINITIALIZED_VALUE", "binding has no value", *span)
                })?;
                set(values, *destination, loaded);
            }
            Instruction::Store {
                symbol,
                value: source,
                ty,
                span,
            } => {
                let stored = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                if let (Some(Value::Vector(previous)), Value::Vector(next)) =
                    (symbols.get(symbol), &stored)
                    && previous.len() != next.len()
                {
                    return Err(runtime_error(
                        "VECTOR_LENGTH_MISMATCH",
                        "assigned vector length differs from the declared length",
                        *span,
                    ));
                }
                symbols.insert(*symbol, stored);
            }
            Instruction::Copy {
                destination,
                source,
                ty,
                span,
            } => {
                let copied = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                set(values, *destination, copied);
            }
            Instruction::Unary {
                destination,
                operator,
                operand,
                ty,
                span,
            } => {
                let result = unary(operator, value(values, *operand, *span)?, ty, *span)?;
                set(values, *destination, result);
            }
            Instruction::Binary {
                destination,
                operator,
                left,
                right,
                ty,
                span,
            } => {
                let result = binary(
                    operator,
                    value(values, *left, *span)?,
                    value(values, *right, *span)?,
                    ty,
                    *span,
                )?;
                set(values, *destination, result);
            }
            Instruction::Cast {
                destination,
                value: source,
                ty,
                span,
            } => {
                let result = cast(value(values, *source, *span)?.clone(), ty, *span)?;
                set(values, *destination, result);
            }
            Instruction::Call {
                destination,
                callee,
                arguments,
                ty,
                span,
            } => {
                let Value::Function(name) = value(values, *callee, *span)? else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "value is not callable",
                        *span,
                    ));
                };
                let name = name.clone();
                let arguments = arguments
                    .iter()
                    .map(|argument| value(values, *argument, *span).cloned())
                    .collect::<Result<Vec<_>, _>>()?;
                let result = self.call_named(&name, arguments, *span)?;
                set(values, *destination, self.coerce_to(result, ty, *span)?);
            }
            Instruction::Input {
                destination, span, ..
            } => {
                let mut line = String::new();
                let count = self.input.read_line(&mut line).map_err(|error| {
                    runtime_error("INPUT_ERROR", format!("cannot read input: {error}"), *span)
                })?;
                let result = if count == 0 {
                    Value::EndOfFile
                } else {
                    while matches!(line.as_bytes().last(), Some(b'\n' | b'\r')) {
                        line.pop();
                    }
                    Value::String(line)
                };
                set(values, *destination, result);
            }
            Instruction::Vector {
                destination,
                values: elements,
                span,
                ..
            } => {
                let vector = elements
                    .iter()
                    .map(|element| value(values, *element, *span).cloned())
                    .collect::<Result<Vec<_>, _>>()?;
                set(values, *destination, Value::Vector(vector));
            }
            Instruction::Index {
                destination,
                object,
                index,
                span,
                ..
            } => {
                let index = usize::try_from(integer(value(values, *index, *span)?, *span)?.0)
                    .map_err(|_| {
                        runtime_error("INDEX_OUT_OF_BOUNDS", "index cannot be negative", *span)
                    })?;
                let element = self.index_value(value(values, *object, *span)?, index, *span)?;
                set(values, *destination, element);
            }
            Instruction::Member {
                destination,
                object,
                name,
                span,
                ..
            } => {
                let member = self.member_of(value(values, *object, *span)?, name, *span)?;
                set(values, *destination, member);
            }
            Instruction::SetIndex {
                symbol,
                indices,
                value: source,
                ty,
                span,
            } => {
                let indices = indices
                    .iter()
                    .map(|index| {
                        usize::try_from(integer(value(values, *index, *span)?, *span)?.0).map_err(
                            |_| {
                                runtime_error(
                                    "INDEX_OUT_OF_BOUNDS",
                                    "index cannot be negative",
                                    *span,
                                )
                            },
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let source = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                let target = symbols.get_mut(symbol).ok_or_else(|| {
                    runtime_error("UNINITIALIZED_VALUE", "binding has no value", *span)
                })?;
                self.set_index(target, &indices, source, *span)?;
            }
            Instruction::Length {
                destination,
                vector,
                span,
            } => {
                let length = match value(values, *vector, *span)? {
                    Value::Vector(elements) => integer_from_count(elements.len(), *span)?,
                    Value::String(text) => integer_from_count(text.chars().count(), *span)?,
                    Value::HostArgs => integer_from_count(self.host.arguments.len(), *span)?,
                    Value::Pointer { handle } => {
                        integer_from_count(self.memory.len(*handle, *span)?, *span)?
                    }
                    Value::Null => {
                        return Err(runtime_error(
                            "NULL_POINTER_ACCESS",
                            "cannot read the length of a NULL pointer",
                            *span,
                        ));
                    }
                    Value::Integer(_, _) | Value::Float(_, _) => {
                        Value::Integer(1, IntegerType::Int32)
                    }
                    _ => {
                        return Err(runtime_error("TYPE_MISMATCH", "value has no length", *span));
                    }
                };
                set(values, *destination, length);
            }
            Instruction::SizeOf {
                destination,
                value: source,
                span,
            } => {
                let size = self.size_of_value(value(values, *source, *span)?, *span)?;
                set(values, *destination, size);
            }
            Instruction::Print {
                values: printed,
                span,
            } => {
                for printed in printed {
                    write!(self.output, "{}", render(value(values, *printed, *span)?)).map_err(
                        |error| {
                            runtime_error(
                                "OUTPUT_ERROR",
                                format!("cannot write output: {error}"),
                                *span,
                            )
                        },
                    )?;
                }
                writeln!(self.output).map_err(|error| {
                    runtime_error(
                        "OUTPUT_ERROR",
                        format!("cannot write output: {error}"),
                        *span,
                    )
                })?;
            }
            Instruction::ClearScreen { console, span } => {
                require_console(value(values, *console, *span)?, *span)?;
                write!(self.output, "\x1b[2J\x1b[H").map_err(|error| {
                    runtime_error(
                        "OUTPUT_ERROR",
                        format!("cannot write output: {error}"),
                        *span,
                    )
                })?;
            }
            Instruction::Beep { console, span } => {
                require_console(value(values, *console, *span)?, *span)?;
                write!(self.output, "\x07").map_err(|error| {
                    runtime_error(
                        "OUTPUT_ERROR",
                        format!("cannot write output: {error}"),
                        *span,
                    )
                })?;
            }
            Instruction::Allocate {
                destination,
                type_name,
                arguments,
                ty,
                span,
                ..
            } => {
                let allocated = match ty {
                    Type::Pointer { element, .. } => {
                        self.allocate_region(element, arguments, values, *span)?
                    }
                    _ if is_host_file_type(type_name) => {
                        let id = self.next_file;
                        self.next_file += 1;
                        self.files.insert(
                            id,
                            FileResource {
                                file: None,
                                family: None,
                            },
                        );
                        Value::File(id)
                    }
                    _ if self.is_bndata_provider(type_name)
                        && type_name.rsplit('.').next() == Some("DataFrame") =>
                    {
                        let id = self.next_dataframe;
                        self.next_dataframe += 1;
                        self.dataframes.insert(
                            id,
                            DataFrameResource {
                                columns: Vec::new(),
                            },
                        );
                        Value::DataFrame(id)
                    }
                    _ if type_name.rsplit('.').next() == Some("Fields")
                        && (self.is_bnlog_provider(type_name)
                            || type_name.ends_with("BNLog.Fields")) =>
                    {
                        let id = self.next_log_fields;
                        self.next_log_fields += 1;
                        self.log_fields.insert(id, HashMap::new());
                        Value::LogFields(id)
                    }
                    _ if type_name.rsplit('.').next() == Some("Entry")
                        && (self.is_bnlog_provider(type_name)
                            || type_name.ends_with("BNLog.Entry")) =>
                    {
                        let id = self.next_log_entry;
                        self.next_log_entry += 1;
                        self.log_entries.insert(id, HashMap::new());
                        Value::LogEntry(id)
                    }
                    _ if type_name.rsplit('.').next() == Some("Logger")
                        && (self.is_bnlog_provider(type_name)
                            || type_name.ends_with("BNLog.Logger")) =>
                    {
                        let id = self.next_log_logger;
                        self.next_log_logger += 1;
                        self.log_loggers.insert(
                            id,
                            LogLoggerResource {
                                label: String::new(),
                                context: std::collections::BTreeMap::new(),
                                null_transports: Vec::new(),
                                console_transports: Vec::new(),
                                file_transports: Vec::new(),
                                closed: false,
                            },
                        );
                        Value::LogLogger(id)
                    }
                    _ if type_name.rsplit('.').next() == Some("Json")
                        && self.is_bnjson_provider(type_name) =>
                    {
                        let id = self.next_json_value;
                        self.next_json_value += 1;
                        self.json_values.insert(id, crate::json::Value::Null);
                        Value::Json(id)
                    }
                    _ => self.allocate_object(type_name, *span)?,
                };
                set(values, *destination, allocated);
            }
            Instruction::Delete {
                value: deleted,
                destructor,
                span,
            } => {
                let target = value(values, *deleted, *span)?.clone();
                self.delete_value(target, destructor.as_deref(), *span)?;
            }
            Instruction::SetMember {
                object,
                name,
                value: source,
                ty,
                span,
                ..
            } => {
                let stored = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                self.set_member_value(values, *object, name, stored, *span)?;
            }
            Instruction::SetField {
                symbol,
                path,
                value: source,
                ty,
                span,
            } => {
                let stored = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                let target = symbols.get_mut(symbol).ok_or_else(|| {
                    runtime_error("UNINITIALIZED_VALUE", "binding has no value", *span)
                })?;
                self.set_field_path(target, path, stored, *span)?;
            }
            Instruction::EnsureClass { class, span } => self.ensure_class(class, *span)?,
            Instruction::LoadStatic {
                destination,
                class,
                field,
                span,
                ..
            } => {
                let loaded = self.statics.get(&(class.clone(), field.clone())).cloned();
                let loaded = loaded.ok_or_else(|| {
                    runtime_error(
                        "UNINITIALIZED_VALUE",
                        format!("STATIC {class}.{field} has no value"),
                        *span,
                    )
                })?;
                set(values, *destination, loaded);
            }
            Instruction::StoreStatic {
                class,
                field,
                value: source,
                ty,
                span,
            } => {
                let stored = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                self.statics.insert((class.clone(), field.clone()), stored);
            }
        }
        Ok(())
    }

}
