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
                let parsed = parse_float_constant(value).ok_or_else(|| {
                    unsupported_instruction(module, function, instruction, "invalid float constant")
                })?;
                block_state
                    .constants
                    .insert(*destination, ConstantValue::Float(parsed));
                emit_constant_assignment(text, *destination, ty, &render_float(parsed, ty));
            }
            Constant::Boolean(value) => {
                block_state
                    .constants
                    .insert(*destination, ConstantValue::Boolean(*value));
                define_boolean(text, analysis, *destination, *value);
            }
            Constant::String(value) => {
                block_state
                    .constants
                    .insert(*destination, ConstantValue::String(value.clone()));
                let _ = writeln!(
                    text,
                    "  %v{} = getelementptr i8, ptr {}, i64 0",
                    destination.0,
                    string_global(&function.name, destination.0)
                );
            }
            Constant::HostArgs
            | Constant::Type(_)
            | Constant::Function(_)
            | Constant::HostConsole => {}
            Constant::NotAvailable => {
                let dest = destination.0;
                let _ = writeln!(
                    text,
                    "  %na{dest} = insertvalue {{ i1, double }} undef, i1 true, 0"
                );
                let _ = writeln!(
                    text,
                    "  %v{dest} = insertvalue {{ i1, double }} %na{dest}, double 0.0, 1"
                );
            }
            Constant::Null => {
                let _ = writeln!(text, "  %v{} = inttoptr i64 0 to ptr", destination.0);
            }
            Constant::EndOfFile => {
                return Err(unsupported_instruction(
                    module,
                    function,
                    instruction,
                    instruction_name(instruction),
                ));
            }
        },
        Instruction::Default {
            destination,
            ty,
            dimensions,
            ..
        } if !dimensions.is_empty() => {
            let Type::Vector { element, .. } = ty else {
                unreachable!("validated multidimensional default type");
            };
            let element_llvm = llvm_type(element).expect("validated vector element type");
            let len = dimensions[0];
            let dest = destination.0;
            let _ = writeln!(
                text,
                "  %vecdefault{dest} = alloca [{len} x {element_llvm}]"
            );
            for index in 0..len {
                let _ = writeln!(
                    text,
                    "  %vecdefaultslot{dest}_{index} = getelementptr [{len} x {element_llvm}], ptr %vecdefault{dest}, i32 0, i32 {index}"
                );
                let zero = match element_llvm {
                    "i1" | "i8" | "i16" | "i32" | "i64" => format!("{element_llvm} 0"),
                    "float" => "float 0.0".into(),
                    "double" => "double 0.0".into(),
                    _ => {
                        return Err(unsupported_instruction(
                            module,
                            function,
                            instruction,
                            "vector element default",
                        ));
                    }
                };
                let _ = writeln!(text, "  store {zero}, ptr %vecdefaultslot{dest}_{index}");
            }
            let _ = writeln!(
                text,
                "  %vecdefaultptr{dest} = getelementptr [{len} x {element_llvm}], ptr %vecdefault{dest}, i32 0, i32 0"
            );
            let _ = writeln!(
                text,
                "  %vecdefaultfat{dest} = insertvalue {{ ptr, i32 }} undef, ptr %vecdefaultptr{dest}, 0"
            );
            let _ = writeln!(
                text,
                "  %v{dest} = insertvalue {{ ptr, i32 }} %vecdefaultfat{dest}, i32 {len}, 1"
            );
        }
        Instruction::Default {
            destination, ty, ..
        } => match llvm_type(ty).expect("validated default type") {
            "i1" => define_boolean(text, analysis, *destination, false),
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
                let _ = writeln!(
                    text,
                    "  %v{} = getelementptr i8, ptr @.bn_empty, i64 0",
                    destination.0
                );
            }
            "{ i1, double }" => emit_optional_float_default(text, *destination),
            "{ ptr, i32 }" => {
                let dest = destination.0;
                let _ = writeln!(
                    text,
                    "  %vec{dest} = insertvalue {{ ptr, i32 }} undef, ptr null, 0"
                );
                let _ = writeln!(
                    text,
                    "  %v{dest} = insertvalue {{ ptr, i32 }} %vec{dest}, i32 0, 1"
                );
            }
            _ => unreachable!("validated scalar default type"),
        },
        Instruction::Store { symbol, value, .. } => {
            let value_ty = analysis
                .values
                .get(value)
                .expect("validated stored value type");
            let slot_ty = analysis.symbols.get(symbol).unwrap_or(value_ty);
            let value_llvm = llvm_type(value_ty).expect("validated store LLVM type");
            let slot_llvm = llvm_type(slot_ty).expect("validated slot LLVM type");
            let operand = if slot_llvm == "i1" {
                i1_operand(text, analysis, state, *value)
            } else if value_llvm != slot_llvm
                && (matches!(value_llvm, "i8" | "i16" | "i32" | "i64")
                    && matches!(slot_llvm, "i8" | "i16" | "i32" | "i64")
                    || matches!(
                        (value_llvm, slot_llvm),
                        ("float", "double") | ("double", "float")
                    ))
            {
                coerce_to_type(text, *value, value_ty, slot_ty)
            } else {
                format!("%v{}", value.0)
            };
            let _ = writeln!(
                text,
                "  store {slot_llvm} {operand}, ptr %s{}",
                symbols[symbol]
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
            let dest_ty = analysis
                .values
                .get(destination)
                .expect("validated loaded type");
            let slot_ty = analysis.symbols.get(symbol).unwrap_or(dest_ty);
            let dest_llvm = llvm_type(dest_ty).expect("validated load LLVM type");
            let slot_llvm = llvm_type(slot_ty).expect("validated slot LLVM type");
            if slot_llvm == "{ i1, double }" && matches!(dest_llvm, "float" | "double") {
                let _ = writeln!(
                    text,
                    "  %optload{} = load {{ i1, double }}, ptr %s{}",
                    destination.0, symbols[symbol]
                );
                if dest_llvm == "float" {
                    let _ = writeln!(
                        text,
                        "  %optdbl{} = extractvalue {{ i1, double }} %optload{}, 1",
                        destination.0, destination.0
                    );
                    let _ = writeln!(
                        text,
                        "  %v{} = fptrunc double %optdbl{} to float",
                        destination.0, destination.0
                    );
                } else {
                    let _ = writeln!(
                        text,
                        "  %v{} = extractvalue {{ i1, double }} %optload{}, 1",
                        destination.0, destination.0
                    );
                }
            } else if slot_llvm != dest_llvm
                && slot_llvm == "{ i1, ptr, i32 }"
                && dest_llvm == "{ ptr, i32 }"
            {
                let dest = destination.0;
                let _ = writeln!(
                    text,
                    "  %netload{dest} = load {{ i1, ptr, i32 }}, ptr %s{}",
                    symbols[symbol]
                );
                let _ = writeln!(
                    text,
                    "  %netloadp{dest} = extractvalue {{ i1, ptr, i32 }} %netload{dest}, 1"
                );
                let _ = writeln!(
                    text,
                    "  %netloadport{dest} = extractvalue {{ i1, ptr, i32 }} %netload{dest}, 2"
                );
                let _ = writeln!(
                    text,
                    "  %netloadagg{dest} = insertvalue {{ ptr, i32 }} undef, ptr %netloadp{dest}, 0"
                );
                let _ = writeln!(
                    text,
                    "  %v{dest} = insertvalue {{ ptr, i32 }} %netloadagg{dest}, i32 %netloadport{dest}, 1"
                );
            } else if slot_llvm != dest_llvm
                && matches!(slot_llvm, "i8" | "i16" | "i32" | "i64")
                && matches!(dest_llvm, "i8" | "i16" | "i32" | "i64")
            {
                let _ = writeln!(
                    text,
                    "  %slotload{} = load {slot_llvm}, ptr %s{}",
                    destination.0, symbols[symbol]
                );
                let slot_w = match slot_llvm {
                    "i8" => 8u8,
                    "i16" => 16,
                    "i32" => 32,
                    _ => 64,
                };
                let dest_w = match dest_llvm {
                    "i8" => 8u8,
                    "i16" => 16,
                    "i32" => 32,
                    _ => 64,
                };
                let opcode = if slot_w < dest_w {
                    if is_unsigned(slot_ty) { "zext" } else { "sext" }
                } else {
                    "trunc"
                };
                let _ = writeln!(
                    text,
                    "  %v{} = {opcode} {slot_llvm} %slotload{} to {dest_llvm}",
                    destination.0, destination.0
                );
            } else {
                let _ = writeln!(
                    text,
                    "  %v{} = load {dest_llvm}, ptr %s{}",
                    destination.0, symbols[symbol]
                );
            }
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
                emit_constant_value_analyzed(text, analysis, *destination, ty, &value);
            } else {
                block_state.constants.remove(destination);
                match llvm_type(ty).expect("validated copy type") {
                    "i1" => define_boolean_from(text, analysis, state, *destination, *source),
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
                    "{ i1, ptr, i64 }" => {
                        let dest = destination.0;
                        let src = source.0;
                        let _ = writeln!(
                            text,
                            "  %netc0{dest} = extractvalue {{ i1, ptr, i64 }} %v{src}, 0"
                        );
                        let _ = writeln!(
                            text,
                            "  %netc1{dest} = extractvalue {{ i1, ptr, i64 }} %v{src}, 1"
                        );
                        let _ = writeln!(
                            text,
                            "  %netc2{dest} = extractvalue {{ i1, ptr, i64 }} %v{src}, 2"
                        );
                        let _ = writeln!(
                            text,
                            "  %netca{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %netc0{dest}, 0"
                        );
                        let _ = writeln!(
                            text,
                            "  %netcb{dest} = insertvalue {{ i1, ptr, i64 }} %netca{dest}, ptr %netc1{dest}, 1"
                        );
                        let _ = writeln!(
                            text,
                            "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %netcb{dest}, i64 %netc2{dest}, 2"
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
                    let operand_ty = analysis.values.get(operand).unwrap_or(ty);
                    let operand_op = coerce_to_type(text, *operand, operand_ty, ty);
                    let _ = writeln!(
                        text,
                        "  %v{} = add {} 0, {operand_op}",
                        destination.0,
                        llvm_type(ty).expect("validated integer unary type")
                    );
                }
                ("Minus", "i8" | "i16" | "i32" | "i64") => {
                    let operand_ty = analysis.values.get(operand).unwrap_or(ty);
                    emit_checked_integer_op(
                        text,
                        block_id,
                        *destination,
                        "Minus",
                        *operand,
                        None,
                        operand_ty,
                        ty,
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
                ("NOT", "i8" | "i16" | "i32" | "i64") => {
                    emit_integer_not(text, *destination, *operand, ty, state);
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
            let right_ty = analysis.values.get(right).expect("validated right type");
            if operator == "IS" {
                emit_is(text, *destination, *left, left_ty, right_ty);
            } else {
                emit_runtime_binary(
                    text,
                    block_id,
                    *destination,
                    operator,
                    *left,
                    *right,
                    left_ty,
                    right_ty,
                    ty,
                    state,
                );
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
            if llvm_type(source_ty) == Some("{ i1, double }")
                && matches!(ty, Type::Float(_) | Type::FloatLiteral)
            {
                extract_optional_float(text, *destination, *value);
            } else {
                lower_cast(text, block_id, *destination, *value, source_ty, ty, state);
            }
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
