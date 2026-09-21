#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::part10::indexed_value;
use super::*;

impl Executor<'_, '_> {
    pub fn instruction(
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
                                bn_diag::DiagId::UNINITIALIZED_VALUE,
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
                                bn_diag::DiagId::INVALID_VECTOR_DIMENSION
                            } else {
                                bn_diag::DiagId::NUMERIC_OVERFLOW
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
            Instruction::Phi { span, .. } => {
                return Err(runtime_error(
                    bn_diag::DiagId::INVALID_IR,
                    "Phi must be resolved by the executor control-flow loop",
                    *span,
                ));
            }
            Instruction::Load {
                destination,
                symbol,
                span,
                ..
            } => {
                if self
                    .ownership_frames
                    .last()
                    .is_some_and(|frame| frame.released_symbols.contains(symbol))
                {
                    let releasing_again = self
                        .ownership_frames
                        .last()
                        .is_some_and(|frame| frame.release_values.contains(destination));
                    return Err(runtime_error(
                        if releasing_again {
                            bn_diag::DiagId::DOUBLE_RELEASE
                        } else {
                            bn_diag::DiagId::USE_AFTER_RELEASE
                        },
                        if releasing_again {
                            "binding was already released"
                        } else {
                            "binding was released"
                        },
                        *span,
                    ));
                }
                let loaded = symbols.get(symbol).cloned().ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::UNINITIALIZED_VALUE,
                        "binding has no value",
                        *span,
                    )
                })?;
                set(values, *destination, loaded);
                self.ownership_frames
                    .last_mut()
                    .expect("instruction executes in an ownership frame")
                    .loaded_values
                    .insert(*destination, *symbol);
            }
            Instruction::Store {
                symbol,
                value: source,
                ty,
                span,
                ..
            } => {
                let stored = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                if let (Some(Value::Vector(previous)), Value::Vector(next)) =
                    (symbols.get(symbol), &stored)
                    && previous.len() != next.len()
                {
                    return Err(runtime_error(
                        bn_diag::DiagId::VECTOR_LENGTH_MISMATCH,
                        "assigned vector length differs from the declared length",
                        *span,
                    ));
                }
                let (weak, transferred) = {
                    let frame = self
                        .ownership_frames
                        .last_mut()
                        .expect("instruction executes in an ownership frame");
                    (
                        frame.weak_symbols.contains(symbol),
                        frame.owned_values.remove(source),
                    )
                };
                if !weak && !transferred {
                    self.retain_owned_value(&stored, *span)?;
                }
                if !weak && let Some(previous) = symbols.remove(symbol) {
                    self.release_owned_value(previous, *span)?;
                }
                symbols.insert(*symbol, stored);
                self.ownership_frames
                    .last_mut()
                    .expect("instruction executes in an ownership frame")
                    .released_symbols
                    .remove(symbol);
                self.refresh_weak_symbols(symbols);
            }
            Instruction::Copy {
                destination,
                source,
                ty,
                span,
            } => {
                let copied = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                self.retain_owned_value(&copied, *span)?;
                set(values, *destination, copied);
                self.ownership_frames
                    .last_mut()
                    .expect("instruction executes in an ownership frame")
                    .owned_values
                    .insert(*destination);
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
                    return Err(super::super::type_mismatch(
                        "FUNCTION",
                        "non-callable value",
                        "function call",
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
                if matches!(
                    values.get(destination),
                    Some(
                        Value::Object { .. }
                            | Value::Vector(_)
                            | Value::Record { .. }
                            | Value::Pointer { .. }
                    )
                ) {
                    self.ownership_frames
                        .last_mut()
                        .expect("instruction executes in an ownership frame")
                        .owned_values
                        .insert(*destination);
                }
                self.refresh_weak_symbols(symbols);
            }
            Instruction::DispatchSubmit {
                destination,
                callee,
                queue,
                task,
                arguments,
                ty,
                span,
            } => {
                let call = Instruction::Call {
                    destination: *destination,
                    callee: *callee,
                    arguments: std::iter::once(*queue)
                        .chain(std::iter::once(*task))
                        .chain(arguments.iter().copied())
                        .collect(),
                    ty: ty.clone(),
                    span: *span,
                };
                self.instruction(&call, symbols, values)?;
            }
            Instruction::DispatchAwait {
                destination,
                callee,
                ticket,
                timeout,
                ty,
                span,
            } => {
                let call = Instruction::Call {
                    destination: *destination,
                    callee: *callee,
                    arguments: vec![*ticket, *timeout],
                    ty: ty.clone(),
                    span: *span,
                };
                self.instruction(&call, symbols, values)?;
            }
            Instruction::Input {
                destination,
                prompt,
                span,
                ..
            } => {
                if let Some(prompt) = prompt {
                    write!(self.output, "{}", render(value(values, *prompt, *span)?)).map_err(
                        |error| {
                            runtime_error(
                                bn_diag::DiagId::OUTPUT_ERROR,
                                format!("cannot write output: {error}"),
                                *span,
                            )
                        },
                    )?;
                    self.output.flush().map_err(|error| {
                        runtime_error(
                            bn_diag::DiagId::OUTPUT_ERROR,
                            format!("cannot flush output: {error}"),
                            *span,
                        )
                    })?;
                }
                let mut line = String::new();
                let count = self.input.read_line(&mut line).map_err(|error| {
                    runtime_error(
                        bn_diag::DiagId::INPUT_ERROR,
                        format!("cannot read input: {error}"),
                        *span,
                    )
                })?;
                let result = if count == 0 {
                    Value::EndOfFile
                } else {
                    while matches!(line.as_bytes().last(), Some(b'\n' | b'\r')) {
                        line.pop();
                    }
                    Value::String(shared_string(line))
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
                        super::super::index_out_of_bounds("negative", "0", "index", *span)
                    })?;
                let element = self.index_value(value(values, *object, *span)?, index, *span)?;
                set(values, *destination, element);
            }
            Instruction::Member {
                destination,
                object,
                field,
                name,
                span,
                ..
            } => {
                let member =
                    self.member_of(value(values, *object, *span)?, name, field.as_ref(), *span)?;
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
                            |_| super::super::index_out_of_bounds("negative", "0", "index", *span),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let stored = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                let target_snapshot = symbols.get(symbol).cloned().ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::UNINITIALIZED_VALUE,
                        "binding has no value",
                        *span,
                    )
                })?;
                let previous = if matches!(target_snapshot, Value::Null) {
                    Value::Null
                } else if matches!(target_snapshot, Value::Pointer { .. }) && indices.len() == 1 {
                    self.index_value(&target_snapshot, indices[0], *span)?
                } else {
                    indexed_value(&target_snapshot, &indices, *span)?.clone()
                };
                let transferred = self
                    .ownership_frames
                    .last_mut()
                    .expect("instruction executes in an ownership frame")
                    .owned_values
                    .remove(source);
                if !transferred {
                    self.retain_owned_value(&stored, *span)?;
                }
                let target = symbols.get_mut(symbol).ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::UNINITIALIZED_VALUE,
                        "binding has no value",
                        *span,
                    )
                })?;
                self.set_index(target, &indices, stored, *span)?;
                self.release_owned_value(previous, *span)?;
            }
            Instruction::SetMemberIndex {
                object,
                field,
                name,
                indices,
                value: source,
                ty,
                span,
                ..
            } => {
                let indices = indices
                    .iter()
                    .map(|index| {
                        usize::try_from(integer(value(values, *index, *span)?, *span)?.0).map_err(
                            |_| super::super::index_out_of_bounds("negative", "0", "index", *span),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let source = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                self.set_member_index_value(
                    values,
                    *object,
                    name,
                    field.as_ref(),
                    &indices,
                    source,
                    *span,
                )?;
            }
            Instruction::SetFieldIndex {
                symbol,
                path,
                fields,
                indices,
                value: source,
                ty,
                span,
                ..
            } => {
                let indices = indices
                    .iter()
                    .map(|index| {
                        usize::try_from(integer(value(values, *index, *span)?, *span)?.0).map_err(
                            |_| super::super::index_out_of_bounds("negative", "0", "index", *span),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let source = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                let target = symbols.get_mut(symbol).ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::UNINITIALIZED_VALUE,
                        "binding has no value",
                        *span,
                    )
                })?;
                self.set_field_index_path(
                    target,
                    path,
                    fields.as_deref().ok_or_else(|| {
                        runtime_error(
                            bn_diag::DiagId::INVALID_IR,
                            "field path store lacks resolved fields",
                            *span,
                        )
                    })?,
                    &indices,
                    source,
                    *span,
                )?;
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
                            bn_diag::DiagId::NULL_POINTER_ACCESS,
                            "cannot read the length of a NULL pointer",
                            *span,
                        ));
                    }
                    Value::Integer(_, _) | Value::Float(_, _) => {
                        Value::Integer(1, IntegerType::Int32)
                    }
                    _ => {
                        return Err(super::super::type_mismatch(
                            "length-bearing value",
                            "value without length",
                            "LEN",
                            *span,
                        ));
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
                for (index, printed) in printed.iter().enumerate() {
                    if index > 0 {
                        write!(self.output, " ").map_err(|error| {
                            runtime_error(
                                bn_diag::DiagId::OUTPUT_ERROR,
                                format!("cannot write output: {error}"),
                                *span,
                            )
                        })?;
                    }
                    write!(self.output, "{}", render(value(values, *printed, *span)?)).map_err(
                        |error| {
                            runtime_error(
                                bn_diag::DiagId::OUTPUT_ERROR,
                                format!("cannot write output: {error}"),
                                *span,
                            )
                        },
                    )?;
                }
                writeln!(self.output).map_err(|error| {
                    runtime_error(
                        bn_diag::DiagId::OUTPUT_ERROR,
                        format!("cannot write output: {error}"),
                        *span,
                    )
                })?;
            }
            Instruction::ClearScreen { console, span } => {
                require_console(value(values, *console, *span)?, *span)?;
                write!(self.output, "\x1b[2J\x1b[H").map_err(|error| {
                    runtime_error(
                        bn_diag::DiagId::OUTPUT_ERROR,
                        format!("cannot write output: {error}"),
                        *span,
                    )
                })?;
            }
            Instruction::Beep { console, span } => {
                require_console(value(values, *console, *span)?, *span)?;
                write!(self.output, "\x07").map_err(|error| {
                    runtime_error(
                        bn_diag::DiagId::OUTPUT_ERROR,
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
                        self.host_allocate("FileSystem", "File", *span)?
                    }
                    _ => match self.library_allocate(type_name, *span) {
                        Some(allocated) => allocated?,
                        None => self.allocate_object(type_name, *span)?,
                    },
                };
                set(values, *destination, allocated);
                self.ownership_frames
                    .last_mut()
                    .expect("instruction executes in an ownership frame")
                    .owned_values
                    .insert(*destination);
            }
            Instruction::Release {
                value: deleted,
                destructor,
                span,
            } => {
                let target = value(values, *deleted, *span)?.clone();
                // An explicit RELEASE of an object whose destructor is running
                // (e.g. RELEASE SELF inside DESTRUCTOR) is a reentrant release.
                if let Value::Object { handle, .. } = &target
                    && self.objects.is_destroying(*handle)
                {
                    return Err(runtime_error(
                        bn_diag::DiagId::DOUBLE_RELEASE,
                        "allocation was already deleted",
                        *span,
                    ));
                }
                let symbol = self
                    .ownership_frames
                    .last()
                    .and_then(|frame| frame.loaded_values.get(deleted).copied());
                if let Some(symbol) = symbol {
                    let weak = self
                        .ownership_frames
                        .last()
                        .is_some_and(|frame| frame.weak_symbols.contains(&symbol));
                    let removed = symbols.remove(&symbol).unwrap_or(target);
                    if weak {
                        // A weak binding owns no reference; RELEASE only ends the binding.
                    } else if matches!(
                        &removed,
                        Value::Object { .. }
                            | Value::Vector(_)
                            | Value::Record { .. }
                            | Value::Pointer { .. }
                            | Value::Null
                    ) {
                        // ARC kinds: drop this binding's strong only. A NULL
                        // pointer binding holds no region; RELEASE ends the binding.
                        self.release_owned_value(removed, *span)?;
                    } else {
                        self.delete_value(removed, destructor.as_deref(), *span)?;
                    }
                    self.ownership_frames
                        .last_mut()
                        .expect("instruction executes in an ownership frame")
                        .released_symbols
                        .insert(symbol);
                    self.refresh_weak_symbols(symbols);
                } else {
                    self.delete_value(target, destructor.as_deref(), *span)?;
                }
            }
            Instruction::SetMember {
                object,
                field,
                name,
                value: source,
                ty,
                span,
                ..
            } => {
                let stored = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                let weak = field
                    .as_ref()
                    .is_some_and(|field| self.module.field_is_weak(field));
                let transferred = self
                    .ownership_frames
                    .last_mut()
                    .expect("instruction executes in an ownership frame")
                    .owned_values
                    .remove(source);
                if !weak && !transferred {
                    self.retain_owned_value(&stored, *span)?;
                }
                let previous = match values.get(object) {
                    Some(Value::Object { handle, .. }) => self
                        .objects
                        .get(*handle, 0, *span)?
                        .fields
                        .get(
                            field
                                .as_ref()
                                .map_or(usize::MAX, |field| field.slot.value() as usize),
                        )
                        .cloned(),
                    Some(Value::Record { record }) => record
                        .get(
                            field
                                .as_ref()
                                .map_or(usize::MAX, |field| field.slot.value() as usize),
                        )
                        .cloned(),
                    _ => None,
                };
                self.set_member_value(values, *object, name, field.as_ref(), stored, *span)?;
                if !weak && let Some(previous) = previous {
                    self.release_owned_value(previous, *span)?;
                }
            }
            Instruction::SetField {
                symbol,
                path,
                fields,
                value: source,
                ty,
                span,
                ..
            } => {
                let stored = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                let target = symbols.get_mut(symbol).ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::UNINITIALIZED_VALUE,
                        "binding has no value",
                        *span,
                    )
                })?;
                self.set_field_path(
                    target,
                    path,
                    fields.as_deref().ok_or_else(|| {
                        runtime_error(
                            bn_diag::DiagId::INVALID_IR,
                            "field path store lacks resolved fields",
                            *span,
                        )
                    })?,
                    stored,
                    *span,
                )?;
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
                        bn_diag::DiagId::UNINITIALIZED_VALUE,
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
            Instruction::SetStaticIndex {
                class,
                field,
                indices,
                value: source,
                ty,
                span,
            } => {
                let indices = indices
                    .iter()
                    .map(|index| {
                        usize::try_from(integer(value(values, *index, *span)?, *span)?.0).map_err(
                            |_| super::super::index_out_of_bounds("negative", "0", "index", *span),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let source = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                let key = (class.clone(), field.clone());
                let mut target = self.statics.get(&key).cloned().ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::UNINITIALIZED_VALUE,
                        format!("STATIC {class}.{field} has no value"),
                        *span,
                    )
                })?;
                self.set_index(&mut target, &indices, source, *span)?;
                self.statics.insert(key, target);
            }
        }
        Ok(())
    }
}
