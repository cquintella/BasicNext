#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_access_emission(
    text: &mut String,
    module: &Module,
    function: &Function,
    block_id: BlockId,
    instruction: &Instruction,
    analysis: &LoweringAnalysis<'_>,
    symbols: &HashMap<SymbolId, usize>,
    block_state: &mut BlockState,
    state: &mut EmissionState,
) -> bool {
    match instruction {
        Instruction::Member {
            destination,
            object,
            name,
            owner,
            ty,
            ..
        } => {
            block_state.constants.remove(destination);
            if owner == "Error" && name == "Message" {
                let aggregate = analysis
                    .values
                    .get(object)
                    .and_then(llvm_type)
                    .expect("validated error aggregate");
                let _ = writeln!(
                    text,
                    "  %v{} = extractvalue {aggregate} %v{}, 1",
                    destination.0, object.0
                );
            } else if owner == "Error" && name == "Code" {
                let _ = writeln!(
                    text,
                    "  %errorcode{} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                    destination.0, object.0
                );
                let _ = writeln!(
                    text,
                    "  %v{} = trunc i64 %errorcode{} to i32",
                    destination.0, destination.0
                );
            } else if owner == "HOST.Exec.Result"
                && matches!(name.as_str(), "ReturnCode" | "Stdout" | "Stderr")
            {
                let suffix = match name.as_str() {
                    "ReturnCode" => "return_code",
                    "Stdout" => "stdout",
                    "Stderr" => "stderr",
                    _ => unreachable!(),
                };
                let _ = writeln!(
                    text,
                    "  %execmemberhandle{} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                    destination.0, object.0
                );
                let _ = writeln!(
                    text,
                    "  %v{} = call {} @bn_rt_exec_result_{}(i64 %execmemberhandle{})",
                    destination.0,
                    if suffix == "return_code" {
                        "i64"
                    } else {
                        "ptr"
                    },
                    suffix,
                    destination.0
                );
            } else {
                let offset = field_byte_offset(module, owner, name);
                emit_member(text, *destination, *object, offset, ty);
            }
        }
        Instruction::SetIndex {
            symbol,
            indices,
            value,
            ty,
            ..
        } => {
            let value_ty = analysis
                .values
                .get(value)
                .expect("validated setindex value type");
            if indices.len() == 1 {
                let index = indices[0];
                let index_ty = analysis
                    .values
                    .get(&index)
                    .expect("validated setindex index type");
                let transfers_object =
                    is_class_type(module, ty) && analysis.owned_object_results.contains_key(value);
                if transfers_object {
                    let _ = writeln!(text, "  store ptr null, ptr %objectowned{}", value.0);
                }
                emit_pointer_set_index(
                    text,
                    module,
                    function,
                    symbols,
                    block_id,
                    symbols[symbol],
                    index,
                    index_ty,
                    *value,
                    value_ty,
                    ty,
                    transfers_object,
                    state,
                );
            } else {
                emit_vector_set_indices(
                    text,
                    block_id,
                    symbols[symbol],
                    indices,
                    *value,
                    value_ty,
                    ty,
                    analysis,
                    state,
                );
            }
        }
        Instruction::Index {
            destination,
            object,
            index,
            ty,
            ..
        } if analysis
            .values
            .get(object)
            .is_some_and(|ty| is_native_vector(ty) || is_native_pointer(ty)) =>
        {
            block_state.constants.remove(destination);
            let index_ty = analysis
                .values
                .get(index)
                .expect("validated vector index type");
            emit_vector_index(
                text,
                block_id,
                *destination,
                *object,
                *index,
                index_ty,
                ty,
                state,
            );
        }
        Instruction::Index {
            destination,
            object,
            index,
            ..
        } if analysis.values.get(object) == Some(&Type::String) => {
            block_state.constants.remove(destination);
            let idx = extend_to_i32_index(
                text,
                *index,
                analysis.values.get(index).expect("validated index type"),
            );
            let dest = destination.0;
            let _ = writeln!(
                text,
                "  %strindexpacked{dest} = call i64 @bn_rt_str_index_utf8(ptr %v{}, i32 {idx})",
                object.0
            );
            let _ = writeln!(text, "  %strindexbuffer{dest} = alloca i64");
            let _ = writeln!(
                text,
                "  store i64 %strindexpacked{dest}, ptr %strindexbuffer{dest}"
            );
            let _ = writeln!(
                text,
                "  %v{dest} = getelementptr i8, ptr %strindexbuffer{dest}, i64 0"
            );
        }
        Instruction::Index {
            destination,
            object,
            index,
            ..
        } if analysis.values.get(object) == Some(&Type::HostArgs) => {
            block_state.constants.remove(destination);
            let index_type = analysis.values.get(index).expect("validated index type");
            let index =
                coerce_to_type(text, *index, index_type, &Type::Integer(IntegerType::Int32));
            let _ = writeln!(
                text,
                "  %argptr{} = getelementptr ptr, ptr %argv, i32 {index}",
                destination.0
            );
            let _ = writeln!(
                text,
                "  %v{} = load ptr, ptr %argptr{}",
                destination.0, destination.0
            );
        }
        _ => return false,
    }
    true
}

fn extend_to_i32_index(text: &mut String, value: ValueId, ty: &Type) -> String {
    match llvm_type(ty).expect("validated index type") {
        "i32" => format!("%v{}", value.0),
        "i64" => {
            let temp = format!("stridx{}", value.0);
            let _ = writeln!(text, "  %{temp} = trunc i64 %v{} to i32", value.0);
            format!("%{temp}")
        }
        llvm_ty => {
            let opcode = if is_unsigned(ty) { "zext" } else { "sext" };
            let temp = format!("stridx{}", value.0);
            let _ = writeln!(text, "  %{temp} = {opcode} {llvm_ty} %v{} to i32", value.0);
            format!("%{temp}")
        }
    }
}
