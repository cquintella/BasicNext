#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_value_emission(
    text: &mut String,
    module: &Module,
    _function: &Function,
    block_id: BlockId,
    instruction: &Instruction,
    analysis: &LoweringAnalysis<'_>,
    symbols: &HashMap<SymbolId, usize>,
    block_state: &mut BlockState,
    state: &mut EmissionState,
) -> bool {
    match instruction {
        Instruction::Input {
            destination,
            prompt,
            ..
        } => {
            block_state.constants.remove(destination);
            if let Some(prompt) = prompt {
                let _ = writeln!(
                    text,
                    "  call i32 (ptr, ...) @printf(ptr @.bn_fmt_str, ptr %v{})",
                    prompt.0
                );
            }
            let symbol = analysis
                .input_targets
                .get(destination)
                .expect("validated INPUT owner");
            let slot = symbols[symbol];
            let dest = destination.0;
            let _ = writeln!(text, "  %inputold{dest} = load ptr, ptr %s{slot}");
            let _ = writeln!(
                text,
                "  %inputwasowned{dest} = load i1, ptr %inputowned{slot}"
            );
            let _ = writeln!(
                text,
                "  %inputreuse{dest} = select i1 %inputwasowned{dest}, ptr %inputold{dest}, ptr null"
            );
            let _ = writeln!(
                text,
                "  %v{dest} = call ptr @bn_input(ptr %inputreuse{dest})"
            );
        }
        Instruction::Length {
            destination,
            vector,
            ..
        } if analysis.values.get(vector) == Some(&Type::HostArgs) => {
            block_state.constants.remove(destination);
            let _ = writeln!(text, "  %v{} = add i32 0, %argc", destination.0);
        }
        Instruction::Length {
            destination,
            vector,
            ..
        } if analysis.values.get(vector) == Some(&Type::String) => {
            block_state.constants.remove(destination);
            let _ = writeln!(
                text,
                "  %v{} = call i32 @bn_rt_str_len(ptr %v{})",
                destination.0, vector.0
            );
        }
        Instruction::SizeOf {
            destination, value, ..
        } => {
            block_state.constants.remove(destination);
            let continuation = take_continuation(block_id, state);
            let dest = destination.0;
            let _ = writeln!(
                text,
                "  %sizeofbytes{dest} = call i64 @bn_string_byte_length(ptr %v{})",
                value.0
            );
            let _ = writeln!(
                text,
                "  %sizeofok{dest} = icmp ule i64 %sizeofbytes{dest}, 2147483647"
            );
            let _ = writeln!(
                text,
                "  br i1 %sizeofok{dest}, label %{continuation}, label %trap_numeric_overflow"
            );
            state.control_flow.label(text, continuation);
            let _ = writeln!(text, "  %v{dest} = trunc i64 %sizeofbytes{dest} to i32");
            state.needs_numeric_overflow_trap = true;
        }
        Instruction::Length {
            destination,
            vector,
            ..
        } if matches!(
            analysis.values.get(vector),
            Some(Type::Vector { .. } | Type::Pointer { .. })
        ) =>
        {
            block_state.constants.remove(destination);
            emit_vector_length(text, *destination, *vector);
        }
        Instruction::Vector {
            destination,
            values: elements,
            ty,
            ..
        } => {
            block_state.constants.remove(destination);
            emit_vector(text, module, *destination, elements, ty, analysis);
        }
        Instruction::Allocate {
            destination,
            type_name,
            arguments,
            ty,
            ..
        } => {
            block_state.constants.remove(destination);
            let object_bytes = class_instance_bytes(module, type_name);
            emit_allocate(
                text,
                module,
                *destination,
                arguments,
                ty,
                analysis,
                object_bytes,
            );
            if !matches!(ty, Type::Pointer { .. })
                && !is_bndata_dataframe_type(module, ty)
                && bnlog_resource_kind(module, ty).is_none()
            {
                let class_global = format!("@.bn_cls_{}", sanitize_symbol(type_name));
                emit_store_object_class(text, *destination, &class_global);
            }
            if analysis.owned_object_results.contains_key(destination) {
                if is_region_type(ty) {
                    let _ = writeln!(
                        text,
                        "  store ptr %allocbase{}, ptr %objectowned{}",
                        destination.0, destination.0
                    );
                } else {
                    let _ = writeln!(
                        text,
                        "  store ptr %v{}, ptr %objectowned{}",
                        destination.0, destination.0
                    );
                }
            }
        }
        _ => return false,
    }
    true
}
