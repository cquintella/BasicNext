#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

impl Executor<'_, '_> {
    pub fn instruction(
        &mut self,
        instruction: &Instruction,
        symbols: &mut HashMap<SymbolId, Value>,
        values: &mut HashMap<ValueId, Value>,
    ) -> Result<(), Diagnostic> {
        match instruction {
            Instruction::Retain {
                destination,
                value: source,
                span,
                ..
            } => {
                let retained = value(values, *source, *span)?.clone();
                self.retain_owned_value(&retained, *span)?;
                set(values, *destination, retained);
            }
            Instruction::Take {
                destination,
                symbol,
                ..
            } => {
                // An emptied binding (taken by `RELEASE` earlier on this
                // path) yields NULL, which holds no reference.
                let taken = symbols.remove(symbol).unwrap_or(Value::Null);
                set(values, *destination, taken);
            }
            Instruction::TakeMember {
                destination,
                object,
                field,
                name,
                span,
                ..
            } => {
                let Value::Object { handle, .. } = value(values, *object, *span)? else {
                    return Err(runtime_error(
                        bn_diag::DiagId::INVALID_IR,
                        "TakeMember needs an object",
                        *span,
                    ));
                };
                let handle = *handle;
                let slot = super::part11::field_slot(field.as_ref(), *span)?;
                let taken = self
                    .objects
                    .get_mut(handle, 0, *span)?
                    .fields
                    .get_mut(slot)
                    .map(|field| std::mem::replace(field, Value::Null))
                    .ok_or_else(|| super::super::name_not_found(name, "object member", *span))?;
                set(values, *destination, taken);
            }
            Instruction::EndBinding { symbol, span } => {
                let frame = self
                    .ownership_frames
                    .last_mut()
                    .expect("instruction executes in an ownership frame");
                if !frame.released_symbols.insert(*symbol) {
                    return Err(runtime_error(
                        bn_diag::DiagId::DOUBLE_RELEASE,
                        "binding was already released",
                        *span,
                    ));
                }
            }
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
                let (released, weak) =
                    self.ownership_frames
                        .last()
                        .map_or((false, false), |frame| {
                            (
                                frame.released_symbols.contains(symbol),
                                frame.weak_symbols.contains(symbol),
                            )
                        });
                if released {
                    return Err(runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        "binding was released",
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
                let loaded = if weak { self.weak_read(loaded) } else { loaded };
                set(values, *destination, loaded);
            }
            Instruction::Store {
                symbol,
                value: source,
                previous,
                ty,
                span,
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
                let replaced = symbols.insert(*symbol, stored).unwrap_or(Value::Null);
                if let Some(previous) = previous {
                    set(values, *previous, replaced);
                }
                self.ownership_frames
                    .last_mut()
                    .expect("instruction executes in an ownership frame")
                    .released_symbols
                    .remove(symbol);
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
                // A call that stopped returns nothing; only the STOP path of
                // the IR sees its result (0.6.md, "`STOP`").
                let result = if self.stop_code.is_some() {
                    result
                } else {
                    self.coerce_to(result, ty, *span)?
                };
                set(values, *destination, result);
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
                let index = integer(value(values, *index, *span)?, *span)?.0;
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
                previous,
                symbol,
                indices,
                value: source,
                ty,
                span,
            } => {
                let indices = indices
                    .iter()
                    .map(|index| Ok(integer(value(values, *index, *span)?, *span)?.0))
                    .collect::<Result<Vec<_>, _>>()?;
                let stored = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                let target = symbols.get(symbol).ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::UNINITIALIZED_VALUE,
                        "binding has no value",
                        *span,
                    )
                })?;
                let replaced = match previous {
                    Some(_) => Some(self.element_at(target, &indices, *span)?),
                    None => None,
                };
                let target = symbols.get_mut(symbol).expect("binding was just read");
                self.set_index(target, &indices, stored, *span)?;
                if let (Some(previous), Some(replaced)) = (previous, replaced) {
                    set(values, *previous, replaced);
                }
            }
            Instruction::SetMemberIndex {
                previous,
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
                    .map(|index| Ok(integer(value(values, *index, *span)?, *span)?.0))
                    .collect::<Result<Vec<_>, _>>()?;
                let source = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                let replaced = match previous {
                    Some(_) => {
                        let member = self.member_of(
                            value(values, *object, *span)?,
                            name,
                            field.as_ref(),
                            *span,
                        )?;
                        Some(self.element_at(&member, &indices, *span)?)
                    }
                    None => None,
                };
                self.set_member_index_value(
                    values,
                    *object,
                    name,
                    field.as_ref(),
                    &indices,
                    source,
                    *span,
                )?;
                if let (Some(previous), Some(replaced)) = (previous, replaced) {
                    set(values, *previous, replaced);
                }
            }
            Instruction::SetFieldIndex {
                previous,
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
                    .map(|index| Ok(integer(value(values, *index, *span)?, *span)?.0))
                    .collect::<Result<Vec<_>, _>>()?;
                let source = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                let fields = fields.as_deref().ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::INVALID_IR,
                        "field path store lacks resolved fields",
                        *span,
                    )
                })?;
                let root = symbols.get(symbol).ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::UNINITIALIZED_VALUE,
                        "binding has no value",
                        *span,
                    )
                })?;
                let replaced = match previous {
                    Some(_) => {
                        let field = self.field_at(root, fields, *span)?;
                        Some(self.element_at(&field, &indices, *span)?)
                    }
                    None => None,
                };
                let target = symbols.get_mut(symbol).expect("binding was just read");
                self.set_field_index_path(target, path, fields, &indices, source, *span)?;
                if let (Some(previous), Some(replaced)) = (previous, replaced) {
                    set(values, *previous, replaced);
                }
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
                    Value::Pointer { handle, .. } => {
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
            }
            Instruction::Release {
                value: released,
                span,
                ..
            } => {
                let target = value(values, *released, *span)?.clone();
                self.release_value(target, *span)?;
            }
            Instruction::SetMember {
                previous,
                object,
                field,
                name,
                value: source,
                ty,
                span,
                ..
            } => {
                let stored = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                let replaced = match previous {
                    Some(_) => Some(self.member_of(
                        value(values, *object, *span)?,
                        name,
                        field.as_ref(),
                        *span,
                    )?),
                    None => None,
                };
                self.set_member_value(values, *object, name, field.as_ref(), stored, *span)?;
                if let (Some(previous), Some(replaced)) = (previous, replaced) {
                    set(values, *previous, replaced);
                }
            }
            Instruction::SetField {
                previous,
                symbol,
                path,
                fields,
                value: source,
                ty,
                span,
                ..
            } => {
                let stored = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                let fields = fields.as_deref().ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::INVALID_IR,
                        "field path store lacks resolved fields",
                        *span,
                    )
                })?;
                let root = symbols.get(symbol).ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::UNINITIALIZED_VALUE,
                        "binding has no value",
                        *span,
                    )
                })?;
                let replaced = match previous {
                    Some(_) => Some(self.field_at(root, fields, *span)?),
                    None => None,
                };
                let target = symbols.get_mut(symbol).expect("binding was just read");
                self.set_field_path(target, path, fields, stored, *span)?;
                if let (Some(previous), Some(replaced)) = (previous, replaced) {
                    set(values, *previous, replaced);
                }
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
                previous,
                class,
                field,
                value: source,
                ty,
                span,
            } => {
                let stored = self.coerce_to(value(values, *source, *span)?.clone(), ty, *span)?;
                let replaced = self
                    .statics
                    .insert((class.clone(), field.clone()), stored)
                    .unwrap_or(Value::Null);
                if let Some(previous) = previous {
                    set(values, *previous, replaced);
                }
            }
            Instruction::SetStaticIndex {
                previous,
                class,
                field,
                indices,
                value: source,
                ty,
                span,
            } => {
                let indices = indices
                    .iter()
                    .map(|index| Ok(integer(value(values, *index, *span)?, *span)?.0))
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
                if let Some(previous) = previous {
                    let replaced = self.element_at(&target, &indices, *span)?;
                    set(values, *previous, replaced);
                }
                self.set_index(&mut target, &indices, source, *span)?;
                self.statics.insert(key, target);
            }
        }
        Ok(())
    }
}
