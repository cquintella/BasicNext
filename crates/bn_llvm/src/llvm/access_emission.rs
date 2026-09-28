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
            field,
            name,
            owner,
            ty,
            ..
        } => {
            block_state.constants.remove(destination);
            if owner == "Error" && matches!(name.as_str(), "Message" | "Operation" | "Cause") {
                let aggregate = analysis
                    .values
                    .get(object)
                    .and_then(llvm_type)
                    .expect("validated error aggregate");
                // The pointer is an error record (or a plain message); the
                // runtime reads the field (bn_rt error_abi).
                let field = match name.as_str() {
                    "Message" => 0,
                    "Operation" => 1,
                    _ => 2,
                };
                let dest = destination.0;
                let _ = writeln!(
                    text,
                    "  %errorptr{dest} = extractvalue {aggregate} %v{}, 1",
                    object.0
                );
                let _ = writeln!(
                    text,
                    "  %v{dest} = call ptr @bn_rt_error_field(ptr %errorptr{dest}, i32 {field})"
                );
            } else if owner == "Error" && name == "Code" {
                let dest = destination.0;
                let aggregate = analysis
                    .values
                    .get(object)
                    .and_then(llvm_type)
                    .expect("validated error aggregate");
                if aggregate == "{ i1, ptr, i64 }" {
                    let _ = writeln!(
                        text,
                        "  %errorcode{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                        object.0
                    );
                } else {
                    // No code field: the runtime record carries the code.
                    let _ = writeln!(
                        text,
                        "  %errorrecord{dest} = extractvalue {aggregate} %v{}, 1\n  %errorcode{dest} = call i64 @bn_rt_error_code(ptr %errorrecord{dest})",
                        object.0
                    );
                }
                let _ = writeln!(text, "  %v{dest} = trunc i64 %errorcode{dest} to i32");
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
                let offset = field_byte_offset(
                    module,
                    field.as_ref().expect("validated member field reference"),
                )
                .expect("validated member field slot");
                emit_member(text, block_id, *destination, *object, offset, ty, state);
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
                    if analysis.symbols.get(symbol).is_some_and(is_native_pointer) {
                        "region"
                    } else {
                        "vector"
                    },
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
            let context = if analysis.values.get(object).is_some_and(is_native_pointer) {
                "region"
            } else {
                "vector"
            };
            emit_vector_index(
                text,
                block_id,
                *destination,
                *object,
                *index,
                index_ty,
                ty,
                context,
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
            // `bn_rt` checks the index against the character count and, when
            // it is outside, prints this site's diagnostic with both facts.
            let (trap, _) = trap_symbol(
                state,
                bn_diag::DiagId::INDEX_OUT_OF_BOUNDS,
                vec![
                    ("index", Fact::Runtime("{}", String::new())),
                    ("bound", Fact::Runtime("{}", String::new())),
                    ("context", Fact::Text("string".into())),
                ],
            );
            let _ = writeln!(
                text,
                "  %strindexpacked{dest} = call i64 @bn_rt_str_index_utf8(ptr %v{}, i32 {idx}, ptr {trap})",
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
