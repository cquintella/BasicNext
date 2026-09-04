#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn analyze_function<'a>(
    module: &Module,
    function: &'a Function,
    module_functions: &std::collections::HashSet<&str>,
) -> Result<LoweringAnalysis<'a>, String> {
    let mut values = HashMap::<ValueId, Type>::new();
    let mut symbols = HashMap::new();
    let mut functions = HashMap::new();
    let mut strings = Vec::new();
    let mut input_count = 0;
    let mut uses_random = false;
    let mut uses_string_concat = false;
    let mut uses_bn_rt = false;
    let mut uses_bn_rt_math = false;
    let mut uses_float_print = false;
    let mut uses_string_ops = false;
    let mut uses_temporal_print = false;
    let mut uses_heap = false;
    let mut seeds_random = false;
    let mut intrinsics = BTreeSet::new();
    let mut def_counts = HashMap::<ValueId, usize>::new();

    for block in &function.blocks {
        for instruction in &block.instructions {
            if let Some(destination) = instruction_destination(instruction) {
                *def_counts.entry(destination).or_insert(0) += 1;
            }
            match instruction {
                Instruction::Constant {
                    destination,
                    value,
                    ty,
                    ..
                } => match value {
                    Constant::Function(name) => {
                        if matches!(name.as_str(), "HOST.Random.Random" | "HOST.Random.Seed") {
                            uses_random = true;
                        }
                        if is_bn_rt_host_call(name) {
                            uses_bn_rt = true;
                        }
                        if bnmath_method(module, name).is_some() {
                            uses_bn_rt_math = true;
                        }
                        functions.insert(*destination, name.as_str());
                    }
                    Constant::String(value) => {
                        values.insert(*destination, ty.clone());
                        strings.push((*destination, value.clone()));
                    }
                    Constant::Type(_) => {
                        values.insert(*destination, ty.clone());
                    }
                    _ => {
                        values.insert(*destination, ty.clone());
                    }
                },
                Instruction::Default {
                    destination,
                    ty,
                    dimensions,
                    dynamic_dimensions,
                    ..
                } if dimensions.is_empty() && dynamic_dimensions.is_empty() => {
                    values.insert(*destination, ty.clone());
                }
                Instruction::Load {
                    destination,
                    symbol,
                    ty,
                    ..
                } => {
                    let load_ty = if llvm_type(ty).is_some() {
                        ty.clone()
                    } else {
                        symbols
                            .get(symbol)
                            .cloned()
                            .filter(|stored| llvm_type(stored).is_some())
                            .unwrap_or_else(|| ty.clone())
                    };
                    values.insert(*destination, load_ty.clone());
                    symbols.entry(*symbol).or_insert(load_ty);
                }
                Instruction::Store {
                    symbol, value, ty, ..
                } => {
                    let stored = if llvm_type(ty).is_some() {
                        ty.clone()
                    } else {
                        values
                            .get(value)
                            .cloned()
                            .filter(|value_ty| llvm_type(value_ty).is_some())
                            .unwrap_or_else(|| ty.clone())
                    };
                    symbols.insert(*symbol, stored);
                }
                Instruction::Copy {
                    destination, ty, ..
                }
                | Instruction::Unary {
                    destination, ty, ..
                }
                | Instruction::Cast {
                    destination, ty, ..
                } => {
                    values.insert(*destination, ty.clone());
                }
                Instruction::Index {
                    destination,
                    object,
                    ty,
                    ..
                } => {
                    if values.get(object) == Some(&Type::String) {
                        uses_string_ops = true;
                    }
                    values.insert(*destination, ty.clone());
                }
                Instruction::Binary {
                    destination,
                    operator,
                    left,
                    ty,
                    ..
                } => {
                    if operator == "Plus" && *ty == Type::String {
                        uses_string_concat = true;
                    }
                    if matches!(operator.as_str(), "Equal" | "Assign" | "NotEqual")
                        && values.get(left) == Some(&Type::String)
                    {
                        uses_string_ops = true;
                    }
                    values.insert(*destination, ty.clone());
                }
                Instruction::Call {
                    destination,
                    callee,
                    ty,
                    ..
                } => {
                    if functions.get(callee) == Some(&"HOST.Random.Seed") {
                        seeds_random = true;
                    }
                    if !matches!(ty, Type::Named(name) if name == "VOID") {
                        values.insert(*destination, ty.clone());
                    }
                }
                Instruction::Input { destination, .. } => {
                    input_count += 1;
                    values.insert(*destination, Type::String);
                }
                Instruction::Length {
                    destination,
                    vector,
                    ..
                } if values.get(vector) == Some(&Type::HostArgs)
                    || values.get(vector) == Some(&Type::String) =>
                {
                    if values.get(vector) == Some(&Type::String) {
                        uses_string_ops = true;
                    }
                    values.insert(*destination, Type::Integer(IntegerType::Int32));
                }
                Instruction::Vector {
                    destination, ty, ..
                } => {
                    values.insert(*destination, ty.clone());
                }
                Instruction::Print {
                    values: printed, ..
                } => {
                    if printed.iter().any(|value| {
                        matches!(
                            values.get(value),
                            Some(
                                Type::Float(_)
                                    | Type::FloatLiteral
                                    | Type::NotAvailable
                                    | Type::Alternative(_)
                            )
                        )
                    }) {
                        uses_float_print = true;
                    }
                    if printed.iter().any(|value| {
                        matches!(
                            values.get(value),
                            Some(Type::Named(name)) if name == "DATE" || name == "TIME"
                        )
                    }) {
                        uses_temporal_print = true;
                    }
                }
                Instruction::Length {
                    destination,
                    vector,
                    ..
                } if matches!(
                    values.get(vector),
                    Some(Type::Vector { .. } | Type::Pointer { .. })
                ) =>
                {
                    values.insert(*destination, Type::Integer(IntegerType::Int32));
                }
                Instruction::Allocate {
                    destination, ty, ..
                } => {
                    uses_heap = true;
                    values.insert(*destination, ty.clone());
                }
                Instruction::Delete { .. } => {
                    uses_heap = true;
                }
                Instruction::EnsureClass { .. } => {}
                Instruction::SetIndex { symbol, ty, .. } => {
                    symbols.entry(*symbol).or_insert_with(|| ty.clone());
                }
                Instruction::SetMember { .. } | Instruction::SetField { .. } => {}
                Instruction::Member {
                    destination, ty, ..
                } => {
                    values.insert(*destination, ty.clone());
                }
                Instruction::LoadStatic {
                    destination, ty, ..
                } => {
                    values.insert(*destination, ty.clone());
                }
                Instruction::StoreStatic { .. } => {}
                Instruction::Length { .. }
                | Instruction::DispatchSubmit { .. }
                | Instruction::DispatchAwait { .. }
                | Instruction::SizeOf { .. }
                | Instruction::ClearScreen { .. }
                | Instruction::Beep { .. }
                | Instruction::Default { .. } => {}
            }
        }
    }

    for block in &function.blocks {
        for instruction in &block.instructions {
            validate_instruction(
                module,
                function,
                instruction,
                &values,
                &symbols,
                &functions,
                module_functions,
                &mut intrinsics,
            )?;
        }
    }
    if uses_random && !seeds_random {
        let instruction = function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .find(|instruction| matches!(
                instruction,
                Instruction::Call { callee, .. } if functions.get(callee) == Some(&"HOST.Random.Random")
            ))
            .expect("random use without seed must come from a call");
        return Err(unsupported_instruction(
            module,
            function,
            instruction,
            "HOST.Random.Random without HOST.Random.Seed",
        ));
    }

    Ok(LoweringAnalysis {
        values,
        symbols,
        functions,
        strings,
        input_count,
        uses_random,
        uses_string_concat,
        uses_bn_rt,
        uses_bn_rt_math,
        uses_float_print,
        uses_string_ops,
        uses_temporal_print,
        uses_heap,
        multi_defs: def_counts
            .into_iter()
            .filter_map(|(value, count)| (count > 1).then_some(value))
            .collect(),
        intrinsics,
    })
}

fn instruction_destination(instruction: &Instruction) -> Option<ValueId> {
    match instruction {
        Instruction::Constant { destination, .. }
        | Instruction::Default { destination, .. }
        | Instruction::Load { destination, .. }
        | Instruction::Copy { destination, .. }
        | Instruction::Unary { destination, .. }
        | Instruction::Binary { destination, .. }
        | Instruction::Cast { destination, .. }
        | Instruction::Call { destination, .. }
        | Instruction::DispatchSubmit { destination, .. }
        | Instruction::DispatchAwait { destination, .. }
        | Instruction::Input { destination, .. }
        | Instruction::Vector { destination, .. }
        | Instruction::Index { destination, .. }
        | Instruction::Member { destination, .. }
        | Instruction::Length { destination, .. }
        | Instruction::SizeOf { destination, .. }
        | Instruction::Allocate { destination, .. }
        | Instruction::LoadStatic { destination, .. } => Some(*destination),
        Instruction::Store { .. }
        | Instruction::SetIndex { .. }
        | Instruction::SetMember { .. }
        | Instruction::SetField { .. }
        | Instruction::Print { .. }
        | Instruction::ClearScreen { .. }
        | Instruction::Beep { .. }
        | Instruction::Delete { .. }
        | Instruction::EnsureClass { .. }
        | Instruction::StoreStatic { .. } => None,
    }
}

#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
fn validate_instruction(
    module: &Module,
    function: &Function,
    instruction: &Instruction,
    values: &HashMap<ValueId, Type>,
    symbols: &HashMap<SymbolId, Type>,
    functions: &HashMap<ValueId, &str>,
    module_functions: &std::collections::HashSet<&str>,
    intrinsics: &mut BTreeSet<&'static str>,
) -> Result<(), String> {
    let supported = match instruction {
        Instruction::Constant { value, ty, .. } => match value {
            Constant::Integer(_)
            | Constant::Float(_)
            | Constant::Boolean(_)
            | Constant::String(_) => llvm_type(ty).is_some(),
            Constant::Function(_)
            | Constant::Type(_)
            | Constant::HostArgs
            | Constant::HostConsole => true,
            Constant::NotAvailable => llvm_type(ty).is_some(),
            Constant::Null => true,
            Constant::EndOfFile => false,
        },
        Instruction::Default {
            ty,
            dimensions,
            dynamic_dimensions,
            ..
        } => dimensions.is_empty() && dynamic_dimensions.is_empty() && llvm_type(ty).is_some(),
        Instruction::Load {
            destination,
            symbol,
            ..
        } => {
            values.get(destination).and_then(llvm_type).is_some()
                && symbols.get(symbol).and_then(llvm_type).is_some()
        }
        Instruction::Store { value, symbol, .. } => {
            values.get(value).and_then(llvm_type).is_some()
                && symbols.get(symbol).and_then(llvm_type).is_some()
        }
        Instruction::Copy { source, ty, .. } => {
            values.get(source).and_then(llvm_type).is_some() && llvm_type(ty).is_some()
        }
        Instruction::Unary {
            operator,
            operand,
            ty,
            ..
        } => unary_supported(operator, values.get(operand), ty),
        Instruction::Binary {
            operator,
            left,
            right,
            ty,
            ..
        } => {
            let Some(left_ty) = values.get(left) else {
                return Err(unsupported_instruction(
                    module,
                    function,
                    instruction,
                    "unknown value type",
                ));
            };
            let Some(right_ty) = values.get(right) else {
                return Err(unsupported_instruction(
                    module,
                    function,
                    instruction,
                    "unknown value type",
                ));
            };
            if let Some(intrinsic) = checked_intrinsic_declaration(left_ty, operator) {
                intrinsics.insert(intrinsic);
            }
            if operator == "Power"
                && let Some(intrinsic) = pow_intrinsic_declaration(ty)
            {
                intrinsics.insert(intrinsic);
            }
            if operator == "IS" {
                matches!(ty, Type::Boolean)
            } else {
                binary_supported(operator, left_ty, right_ty, ty)
            }
        }
        Instruction::Cast { value, ty, .. } => {
            cast_supported(values.get(value), ty)
                || values.get(value).is_some_and(|source| {
                    llvm_type(source) == Some("{ i1, double }")
                        && matches!(ty, Type::Float(_) | Type::FloatLiteral)
                })
        }
        Instruction::Call {
            callee, arguments, ..
        } => match functions.get(callee).copied() {
            Some("HOST.Random.Seed") => arguments.len() == 1,
            Some("HOST.Random.Random") => arguments.is_empty(),
            Some(name) if is_bn_rt_host_call(name) => bn_rt_call_supported(name, arguments, values),
            Some(name) if bnmath_method(module, name).is_some() => bnmath_call_supported(
                bnmath_method(module, name).unwrap_or(name),
                arguments,
                values,
            ),
            Some(name)
                if module_functions.contains(name.strip_prefix("@super:").unwrap_or(name)) =>
            {
                arguments.iter().all(|argument| {
                    values
                        .get(argument)
                        .is_some_and(|ty| llvm_type(ty).is_some() || is_void_type(ty))
                })
            }
            Some(name) => {
                return Err(unsupported_instruction(
                    module,
                    function,
                    instruction,
                    &unsupported_call_detail(module, name),
                ));
            }
            None => false,
        },
        Instruction::Input { .. } => true,
        Instruction::Length { vector, .. } => {
            matches!(
                values.get(vector),
                Some(Type::HostArgs | Type::String | Type::Vector { .. } | Type::Pointer { .. })
            )
        }
        Instruction::Index {
            object, index, ty, ..
        } => {
            (matches!(values.get(object), Some(Type::HostArgs | Type::String))
                || values.get(object).is_some_and(is_int_vector)
                || values.get(object).is_some_and(is_int_pointer))
                && values.get(index).and_then(llvm_type).is_some()
                && llvm_type(ty).is_some()
        }
        Instruction::Vector {
            values: elements,
            ty,
            ..
        } => {
            llvm_type(ty).is_some()
                && elements
                    .iter()
                    .all(|element| values.get(element).and_then(llvm_type).is_some())
        }
        Instruction::Print {
            values: printed, ..
        } => printed
            .iter()
            .all(|value| values.get(value).is_some_and(printable_type)),
        Instruction::Allocate { ty, arguments, .. } => {
            llvm_type(ty).is_some()
                && (matches!(ty, Type::Pointer { .. })
                    && arguments
                        .iter()
                        .all(|argument| values.get(argument).and_then(llvm_type).is_some())
                    || !matches!(ty, Type::Pointer { .. }))
        }
        Instruction::Delete { value, .. } => {
            values.get(value).is_some_and(is_int_pointer)
                || values.get(value).is_some_and(|ty| {
                    llvm_type(ty) == Some("{ ptr, i32 }") || llvm_type(ty) == Some("ptr")
                })
        }
        Instruction::SetIndex {
            symbol,
            indices,
            value,
            ty,
            ..
        } => {
            indices.len() == 1
                && symbols.get(symbol).is_some_and(is_int_pointer)
                && indices
                    .iter()
                    .all(|index| values.get(index).and_then(llvm_type).is_some())
                && values.get(value).and_then(llvm_type).is_some()
                && llvm_type(ty).is_some()
        }
        Instruction::EnsureClass { .. } => true,
        Instruction::Member { object, ty, .. } => {
            values.get(object).and_then(llvm_type) == Some("ptr") && llvm_type(ty).is_some()
        }
        Instruction::SetMember {
            object, value, ty, ..
        } => {
            values.get(object).and_then(llvm_type) == Some("ptr")
                && values.get(value).and_then(llvm_type).is_some()
                && llvm_type(ty).is_some()
        }
        Instruction::SetField {
            symbol,
            path,
            value,
            ty,
            ..
        } => {
            path.len() == 1
                && symbols.get(symbol).and_then(llvm_type) == Some("ptr")
                && values.get(value).and_then(llvm_type).is_some()
                && llvm_type(ty).is_some()
        }
        Instruction::LoadStatic { ty, .. } => llvm_type(ty).is_some(),
        Instruction::StoreStatic { value, ty, .. } => {
            llvm_type(ty).is_some() && values.get(value).and_then(llvm_type).is_some()
        }
        Instruction::DispatchSubmit { .. }
        | Instruction::DispatchAwait { .. }
        | Instruction::ClearScreen { .. }
        | Instruction::Beep { .. }
        | Instruction::SizeOf { .. } => false,
    };
    if supported {
        Ok(())
    } else {
        Err(unsupported_instruction(
            module,
            function,
            instruction,
            &unsupported_instruction_detail(instruction),
        ))
    }
}
