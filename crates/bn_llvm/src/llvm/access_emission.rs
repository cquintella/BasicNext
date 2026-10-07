#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;
use crate::ir::{CastOp, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};
use crate::layout::{handle_result_ty, typed_llvm};

fn v(id: ValueId) -> O {
    O::reg(format!("v{}", id.0))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_access_emission(
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
                let record = I::extract(typed_llvm(aggregate), v(*object), 1);
                text.assign(format!("errorptr{dest}"), record);
                let args = vec![
                    (T::Ptr, O::reg(format!("errorptr{dest}"))),
                    (T::I32, O::int(field)),
                ];
                text.assign(
                    format!("v{dest}"),
                    I::call(T::Ptr, "bn_rt_error_field", args),
                );
            } else if owner == "Error" && name == "Code" {
                let dest = destination.0;
                let aggregate = analysis
                    .values
                    .get(object)
                    .and_then(llvm_type)
                    .expect("validated error aggregate");
                let code = O::reg(format!("errorcode{dest}"));
                if aggregate == "{ i1, ptr, i64 }" {
                    let field = I::extract(handle_result_ty(), v(*object), 2);
                    text.assign(format!("errorcode{dest}"), field);
                } else {
                    // No code field: the runtime record carries the code.
                    let record = I::extract(typed_llvm(aggregate), v(*object), 1);
                    text.assign(format!("errorrecord{dest}"), record);
                    let args = vec![(T::Ptr, O::reg(format!("errorrecord{dest}")))];
                    text.assign(
                        format!("errorcode{dest}"),
                        I::call(T::I64, "bn_rt_error_code", args),
                    );
                }
                text.assign(
                    format!("v{dest}"),
                    I::cast(CastOp::Trunc, T::I64, code, T::I32),
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
                let dest = destination.0;
                let handle = I::extract(handle_result_ty(), v(*object), 2);
                text.assign(format!("execmemberhandle{dest}"), handle);
                let ret = if suffix == "return_code" {
                    T::I64
                } else {
                    T::Ptr
                };
                let args = vec![(T::I64, O::reg(format!("execmemberhandle{dest}")))];
                let symbol = format!("bn_rt_exec_result_{suffix}");
                text.assign(format!("v{dest}"), I::call(ret, &symbol, args));
            } else {
                let field = field.as_ref().expect("validated member field reference");
                let offset = field_byte_offset(module, field).expect("validated member field slot");
                let weak = module.field_is_weak(field);
                emit_member(
                    text,
                    block_id,
                    *destination,
                    *object,
                    offset,
                    ty,
                    weak,
                    state,
                );
            }
        }
        Instruction::SetIndex {
            symbol,
            indices,
            value,
            previous,
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
                emit_pointer_set_index(
                    text,
                    block_id,
                    symbols[symbol],
                    index,
                    index_ty,
                    *value,
                    value_ty,
                    ty,
                    *previous,
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
                    *previous,
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
            let r = |name: &str| O::reg(format!("strindex{name}{dest}"));
            let args = vec![
                (T::Ptr, v(*object)),
                (T::I32, O::raw(idx)),
                (T::Ptr, O::raw(trap)),
            ];
            let packed = I::call(T::I64, "bn_rt_str_index_utf8", args);
            text.assign(format!("strindexpacked{dest}"), packed);
            text.assign(format!("strindexbuffer{dest}"), I::alloca(T::I64));
            text.emit(I::store(T::I64, r("packed"), r("buffer")));
            let start = vec![(T::I64, O::int(0))];
            text.assign(format!("v{dest}"), I::gep(T::I8, r("buffer"), start));
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
            let dest = destination.0;
            let at = vec![(T::I32, O::raw(index))];
            text.assign(format!("argptr{dest}"), I::gep(T::Ptr, O::reg("argv"), at));
            text.assign(
                format!("v{dest}"),
                I::load(T::Ptr, O::reg(format!("argptr{dest}"))),
            );
        }
        _ => return false,
    }
    true
}

fn extend_to_i32_index(text: &mut String, value: ValueId, ty: &Type) -> String {
    let llvm_ty = llvm_type(ty).expect("validated index type");
    let op = match llvm_ty {
        "i32" => return format!("%v{}", value.0),
        "i64" => CastOp::Trunc,
        _ if is_unsigned(ty) => CastOp::ZExt,
        _ => CastOp::SExt,
    };
    let temp = format!("stridx{}", value.0);
    text.assign(&temp, I::cast(op, typed_llvm(llvm_ty), v(value), T::I32));
    format!("%{temp}")
}
