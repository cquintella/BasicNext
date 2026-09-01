#![allow(clippy::wildcard_imports)]
use super::*;
#[path = "emission_tail.rs"]
mod emission_tail;
use emission_tail::lower_scalar_instruction_tail;

pub(crate) fn lower_scalar_instruction(
    text: &mut String,
    module: &Module,
    function: &Function,
    block_id: BlockId,
    instruction: &Instruction,
    analysis: &LoweringAnalysis<'_>,
    symbols: &HashMap<SymbolId, usize>,
    block_state: &mut BlockState,
    state: &mut EmissionState,
) -> Result<(), String> {
    match instruction {
        Instruction::Constant {
            destination,
            value,
            ty,
            ..
        } => match value {
            Constant::Integer(value) => {
                let parsed = parse_integer(value).ok_or_else(|| {
                    unsupported_instruction(
                        module,
                        function,
                        instruction,
                        "invalid integer constant",
                    )
                })?;
                block_state.constants.insert(
                    *destination,
                    ConstantValue::Integer(parsed, integer_kind(ty)),
                );
                emit_constant_assignment(text, *destination, ty, value);
            }
            Constant::Float(value) => {
                let parsed = value.parse::<f64>().map_err(|_| {
                    unsupported_instruction(module, function, instruction, "invalid float constant")
                })?;
                block_state
                    .constants
                    .insert(*destination, ConstantValue::Float(parsed));
                emit_constant_assignment(text, *destination, ty, value);
            }
            Constant::Boolean(value) => {
                block_state
                    .constants
                    .insert(*destination, ConstantValue::Boolean(*value));
                emit_boolean_assignment(text, *destination, *value);
            }
            Constant::String(value) => {
                block_state
                    .constants
                    .insert(*destination, ConstantValue::String(value.clone()));
                let _ = writeln!(
                    text,
                    "  %v{} = getelementptr i8, ptr @.bn_str{}, i64 0",
                    destination.0, destination.0
                );
            }
            Constant::HostArgs | Constant::Type(_) | Constant::Function(_) => {}
            Constant::HostConsole
            | Constant::Null
            | Constant::NotAvailable
            | Constant::EndOfFile => {
                return Err(unsupported_instruction(
                    module,
                    function,
                    instruction,
                    instruction_name(instruction),
                ));
            }
        },
        Instruction::Default {
            destination, ty, ..
        } => match llvm_type(ty).expect("validated default type") {
            "i1" => emit_boolean_assignment(text, *destination, false),
            "i8" | "i16" | "i32" | "i64" => {
                let _ = writeln!(
                    text,
                    "  %v{} = add {} 0, 0",
                    destination.0,
                    llvm_type(ty).expect("validated integer default type")
                );
            }
            "float" => {
                let _ = writeln!(text, "  %v{} = fadd float 0.0, 0.0", destination.0);
            }
            "double" => {
                let _ = writeln!(text, "  %v{} = fadd double 0.0, 0.0", destination.0);
            }
            "ptr" => {
                let _ = writeln!(text, "  %v{} = inttoptr i64 0 to ptr", destination.0);
            }
            _ => unreachable!("validated scalar default type"),
        },
        Instruction::Store { symbol, value, .. } => {
            let ty = llvm_type(
                analysis
                    .values
                    .get(value)
                    .expect("validated stored value type"),
            )
            .expect("validated store LLVM type");
            let _ = writeln!(
                text,
                "  store {ty} %v{}, ptr %s{}",
                value.0, symbols[symbol]
            );
            if let Some(value) = block_state.constants.get(value).cloned() {
                block_state.bindings.insert(*symbol, value);
            } else {
                block_state.bindings.remove(symbol);
            }
        }
        Instruction::Load {
            destination,
            symbol,
            ..
        } => {
            let ty = llvm_type(
                analysis
                    .values
                    .get(destination)
                    .expect("validated loaded type"),
            )
            .expect("validated load LLVM type");
            let _ = writeln!(
                text,
                "  %v{} = load {ty}, ptr %s{}",
                destination.0, symbols[symbol]
            );
            if let Some(value) = block_state.bindings.get(symbol).cloned() {
                block_state.constants.insert(*destination, value);
            } else {
                block_state.constants.remove(destination);
            }
        }
        Instruction::Copy {
            destination,
            source,
            ty,
            ..
        } => {
            if let Some(value) = block_state.constants.get(source).cloned() {
                block_state.constants.insert(*destination, value.clone());
                emit_constant_value(text, *destination, ty, &value);
            } else {
                block_state.constants.remove(destination);
                match llvm_type(ty).expect("validated copy type") {
                    "i1" => {
                        let _ =
                            writeln!(text, "  %v{} = or i1 false, %v{}", destination.0, source.0);
                    }
                    "i8" | "i16" | "i32" | "i64" => {
                        let _ = writeln!(
                            text,
                            "  %v{} = add {} 0, %v{}",
                            destination.0,
                            llvm_type(ty).expect("validated integer copy type"),
                            source.0
                        );
                    }
                    "float" => {
                        let _ = writeln!(
                            text,
                            "  %v{} = fadd float 0.0, %v{}",
                            destination.0, source.0
                        );
                    }
                    "double" => {
                        let _ = writeln!(
                            text,
                            "  %v{} = fadd double 0.0, %v{}",
                            destination.0, source.0
                        );
                    }
                    "ptr" => {
                        let _ = writeln!(
                            text,
                            "  %v{} = getelementptr i8, ptr %v{}, i64 0",
                            destination.0, source.0
                        );
                    }
                    _ => unreachable!("validated copy type"),
                }
            }
        }
        Instruction::Unary {
            destination,
            operator,
            operand,
            ty,
            ..
        } => {
            if let Some(result) = fold_unary(operator, block_state.constants.get(operand), ty) {
                block_state.constants.insert(*destination, result.clone());
                emit_constant_value(text, *destination, ty, &result);
                return Ok(());
            }
            block_state.constants.remove(destination);
            match (
                operator.as_str(),
                llvm_type(ty).expect("validated unary LLVM type"),
            ) {
                ("Plus", "i8" | "i16" | "i32" | "i64") => {
                    let _ = writeln!(
                        text,
                        "  %v{} = add {} 0, %v{}",
                        destination.0,
                        llvm_type(ty).expect("validated integer unary type"),
                        operand.0
                    );
                }
                ("Minus", "i8" | "i16" | "i32" | "i64") => {
                    emit_checked_integer_op(
                        text,
                        block_id,
                        *destination,
                        "Minus",
                        *operand,
                        None,
                        ty,
                        state,
                    );
                }
                ("Plus", "float") => {
                    let _ = writeln!(
                        text,
                        "  %v{} = fadd float 0.0, %v{}",
                        destination.0, operand.0
                    );
                }
                ("Minus", "float") => {
                    let _ = writeln!(
                        text,
                        "  %v{} = fsub float 0.0, %v{}",
                        destination.0, operand.0
                    );
                }
                ("Plus", "double") => {
                    let _ = writeln!(
                        text,
                        "  %v{} = fadd double 0.0, %v{}",
                        destination.0, operand.0
                    );
                }
                ("Minus", "double") => {
                    let _ = writeln!(
                        text,
                        "  %v{} = fsub double 0.0, %v{}",
                        destination.0, operand.0
                    );
                }
                ("NOT", "i1") => {
                    let _ = writeln!(text, "  %v{} = xor i1 1, %v{}", destination.0, operand.0);
                }
                _ => unreachable!("validated unary operator"),
            }
        }
        Instruction::Binary {
            destination,
            operator,
            left,
            right,
            ty,
            ..
        } => {
            if let Some(result) = fold_binary(
                operator,
                block_state.constants.get(left),
                block_state.constants.get(right),
                ty,
            ) {
                block_state.constants.insert(*destination, result.clone());
                emit_constant_value(text, *destination, ty, &result);
                return Ok(());
            }
            block_state.constants.remove(destination);
            let left_ty = analysis.values.get(left).expect("validated left type");
            match (
                operator.as_str(),
                llvm_type(left_ty).expect("validated binary LLVM type"),
            ) {
                ("Plus" | "Minus" | "Star" | "Multiply", "i8" | "i16" | "i32" | "i64") => {
                    emit_checked_integer_op(
                        text,
                        block_id,
                        *destination,
                        operator,
                        *left,
                        Some(*right),
                        ty,
                        state,
                    );
                }
                ("AND" | "OR" | "XOR", "i8" | "i16" | "i32" | "i64") => {
                    let op = match operator.as_str() {
                        "AND" => "and",
                        "OR" => "or",
                        "XOR" => "xor",
                        _ => unreachable!(),
                    };
                    let _ = writeln!(
                        text,
                        "  %v{} = {op} {} %v{}, %v{}",
                        destination.0,
                        llvm_type(left_ty).expect("validated integer op type"),
                        left.0,
                        right.0
                    );
                }
                ("Plus", "float") => {
                    let _ = writeln!(
                        text,
                        "  %v{} = fadd float %v{}, %v{}",
                        destination.0, left.0, right.0
                    );
                }
                ("Minus", "float") => {
                    let _ = writeln!(
                        text,
                        "  %v{} = fsub float %v{}, %v{}",
                        destination.0, left.0, right.0
                    );
                }
                ("Star" | "Multiply", "float") => {
                    let _ = writeln!(
                        text,
                        "  %v{} = fmul float %v{}, %v{}",
                        destination.0, left.0, right.0
                    );
                }
                ("Slash" | "Divide", "float") => {
                    let _ = writeln!(
                        text,
                        "  %v{} = fdiv float %v{}, %v{}",
                        destination.0, left.0, right.0
                    );
                }
                ("Plus", "double") => {
                    let _ = writeln!(
                        text,
                        "  %v{} = fadd double %v{}, %v{}",
                        destination.0, left.0, right.0
                    );
                }
                ("Minus", "double") => {
                    let _ = writeln!(
                        text,
                        "  %v{} = fsub double %v{}, %v{}",
                        destination.0, left.0, right.0
                    );
                }
                ("Star" | "Multiply", "double") => {
                    let _ = writeln!(
                        text,
                        "  %v{} = fmul double %v{}, %v{}",
                        destination.0, left.0, right.0
                    );
                }
                ("Slash" | "Divide", "double") => {
                    let _ = writeln!(
                        text,
                        "  %v{} = fdiv double %v{}, %v{}",
                        destination.0, left.0, right.0
                    );
                }
                ("AND", "i1") => {
                    let _ = writeln!(
                        text,
                        "  %v{} = and i1 %v{}, %v{}",
                        destination.0, left.0, right.0
                    );
                }
                ("OR", "i1") => {
                    let _ = writeln!(
                        text,
                        "  %v{} = or i1 %v{}, %v{}",
                        destination.0, left.0, right.0
                    );
                }
                ("XOR", "i1") => {
                    let _ = writeln!(
                        text,
                        "  %v{} = xor i1 %v{}, %v{}",
                        destination.0, left.0, right.0
                    );
                }
                (
                    "Less" | "LessEqual" | "Greater" | "GreaterEqual" | "Equal" | "Assign"
                    | "NotEqual",
                    "i8" | "i16" | "i32" | "i64",
                ) => {
                    let _ = writeln!(
                        text,
                        "  %v{} = {} {} %v{}, %v{}",
                        destination.0,
                        integer_compare_opcode(operator, left_ty),
                        llvm_type(left_ty).expect("validated compare type"),
                        left.0,
                        right.0
                    );
                }
                (
                    "Less" | "LessEqual" | "Greater" | "GreaterEqual" | "Equal" | "Assign"
                    | "NotEqual",
                    "float" | "double",
                ) => {
                    let _ = writeln!(
                        text,
                        "  %v{} = {} {} %v{}, %v{}",
                        destination.0,
                        float_compare_opcode(operator),
                        llvm_type(left_ty).expect("validated float compare type"),
                        left.0,
                        right.0
                    );
                }
                ("Equal" | "Assign", "i1") => {
                    let _ = writeln!(
                        text,
                        "  %v{} = icmp eq i1 %v{}, %v{}",
                        destination.0, left.0, right.0
                    );
                }
                ("NotEqual", "i1") => {
                    let _ = writeln!(
                        text,
                        "  %v{} = icmp ne i1 %v{}, %v{}",
                        destination.0, left.0, right.0
                    );
                }
                _ => unreachable!("validated binary operator"),
            }
        }
        Instruction::Cast {
            destination,
            value,
            ty,
            ..
        } => {
            if let Some(result) = fold_cast(block_state.constants.get(value), ty) {
                block_state.constants.insert(*destination, result.clone());
                emit_constant_value(text, *destination, ty, &result);
                return Ok(());
            }
            block_state.constants.remove(destination);
            let source_ty = analysis
                .values
                .get(value)
                .expect("validated cast source type");
            lower_cast(text, *destination, *value, source_ty, ty);
        }
        _ => {
            return lower_scalar_instruction_tail(
                text,
                module,
                function,
                block_id,
                instruction,
                analysis,
                symbols,
                block_state,
                state,
            );
        }
    }
    Ok(())
}
