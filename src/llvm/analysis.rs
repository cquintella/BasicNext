#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn analyze_function<'a>(
    module: &Module,
    function: &'a Function,
) -> Result<LoweringAnalysis<'a>, String> {
    let mut values = HashMap::<ValueId, Type>::new();
    let mut symbols = HashMap::new();
    let mut functions = HashMap::new();
    let mut strings = Vec::new();
    let mut input_count = 0;
    let mut uses_random = false;
    let mut seeds_random = false;
    let mut intrinsics = BTreeSet::new();

    for block in &function.blocks {
        for instruction in &block.instructions {
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
                        functions.insert(*destination, name.as_str());
                    }
                    Constant::String(value) => {
                        values.insert(*destination, ty.clone());
                        strings.push((*destination, value.clone()));
                    }
                    Constant::Type(_) => {}
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
                    let ty = symbols.get(symbol).cloned().unwrap_or_else(|| ty.clone());
                    values.insert(*destination, ty.clone());
                    symbols.entry(*symbol).or_insert(ty);
                }
                Instruction::Store {
                    symbol, value, ty, ..
                } => {
                    let ty = values.get(value).cloned().unwrap_or_else(|| ty.clone());
                    symbols.entry(*symbol).or_insert(ty);
                }
                Instruction::Copy {
                    destination, ty, ..
                }
                | Instruction::Unary {
                    destination, ty, ..
                }
                | Instruction::Binary {
                    destination, ty, ..
                }
                | Instruction::Cast {
                    destination, ty, ..
                }
                | Instruction::Index {
                    destination, ty, ..
                } => {
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
                } if values.get(vector) == Some(&Type::HostArgs) => {
                    values.insert(*destination, Type::Integer(IntegerType::Int32));
                }
                Instruction::Length { .. }
                | Instruction::DispatchSubmit { .. }
                | Instruction::DispatchAwait { .. }
                | Instruction::Print { .. }
                | Instruction::Vector { .. }
                | Instruction::Member { .. }
                | Instruction::SetIndex { .. }
                | Instruction::SizeOf { .. }
                | Instruction::ClearScreen { .. }
                | Instruction::Beep { .. }
                | Instruction::Allocate { .. }
                | Instruction::Delete { .. }
                | Instruction::SetMember { .. }
                | Instruction::SetField { .. }
                | Instruction::EnsureClass { .. }
                | Instruction::LoadStatic { .. }
                | Instruction::StoreStatic { .. }
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
        intrinsics,
    })
}

#[allow(clippy::too_many_lines)]
fn validate_instruction(
    module: &Module,
    function: &Function,
    instruction: &Instruction,
    values: &HashMap<ValueId, Type>,
    symbols: &HashMap<SymbolId, Type>,
    functions: &HashMap<ValueId, &str>,
    intrinsics: &mut BTreeSet<&'static str>,
) -> Result<(), String> {
    let supported = match instruction {
        Instruction::Constant { value, ty, .. } => match value {
            Constant::Integer(_)
            | Constant::Float(_)
            | Constant::Boolean(_)
            | Constant::String(_) => llvm_type(ty).is_some(),
            Constant::Function(_) | Constant::Type(_) | Constant::HostArgs => true,
            Constant::HostConsole
            | Constant::Null
            | Constant::NotAvailable
            | Constant::EndOfFile => false,
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
            binary_supported(operator, left_ty, right_ty, ty)
        }
        Instruction::Cast { value, ty, .. } => cast_supported(values.get(value), ty),
        Instruction::Call {
            callee, arguments, ..
        } => match functions.get(callee).copied() {
            Some("HOST.Random.Seed") => arguments.len() == 1,
            Some("HOST.Random.Random") => arguments.is_empty(),
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
        Instruction::Length { vector, .. } => values.get(vector) == Some(&Type::HostArgs),
        Instruction::Index {
            object, index, ty, ..
        } => {
            values.get(object) == Some(&Type::HostArgs)
                && values.get(index).and_then(llvm_type).is_some()
                && llvm_type(ty).is_some()
        }
        Instruction::Print {
            values: printed, ..
        } => printed
            .iter()
            .all(|value| values.get(value).is_some_and(printable_type)),
        Instruction::DispatchSubmit { .. }
        | Instruction::DispatchAwait { .. }
        | Instruction::Vector { .. }
        | Instruction::ClearScreen { .. }
        | Instruction::Beep { .. }
        | Instruction::Member { .. }
        | Instruction::SetIndex { .. }
        | Instruction::SizeOf { .. }
        | Instruction::Allocate { .. }
        | Instruction::Delete { .. }
        | Instruction::SetMember { .. }
        | Instruction::SetField { .. }
        | Instruction::EnsureClass { .. }
        | Instruction::LoadStatic { .. }
        | Instruction::StoreStatic { .. } => false,
    };
    if supported {
        Ok(())
    } else {
        Err(unsupported_instruction(
            module,
            function,
            instruction,
            instruction_name(instruction),
        ))
    }
}
