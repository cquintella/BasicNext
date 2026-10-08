#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;
use crate::ir::{
    BinaryOp, CastOp, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T,
};

fn v(id: ValueId) -> O {
    O::reg(format!("v{}", id.0))
}

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
                let args = vec![(T::Ptr, O::global(".bn_fmt_str")), (T::Ptr, v(*prompt))];
                text.emit(I::call_variadic(T::I32, vec![T::Ptr], "printf", args));
            }
            let symbol = analysis
                .input_targets
                .get(destination)
                .expect("validated INPUT owner");
            let slot = symbols[symbol];
            let dest = destination.0;
            let r = |name: &str| O::reg(format!("input{name}{dest}"));
            text.assign(
                format!("inputold{dest}"),
                I::load(T::Ptr, O::reg(format!("s{slot}"))),
            );
            let owned = I::load(T::I1, O::reg(format!("inputowned{slot}")));
            text.assign(format!("inputwasowned{dest}"), owned);
            let reuse = I::select(r("wasowned"), T::Ptr, r("old"), O::null());
            text.assign(format!("inputreuse{dest}"), reuse);
            let read = I::call(T::Ptr, "bn_input", vec![(T::Ptr, r("reuse"))]);
            text.assign(format!("v{dest}"), read);
        }
        Instruction::Length {
            destination,
            vector,
            ..
        } if analysis.values.get(vector) == Some(&Type::HostArgs) => {
            block_state.constants.remove(destination);
            let count = I::binary(BinaryOp::Add, T::I32, O::int(0), O::reg("argc"));
            text.assign(format!("v{}", destination.0), count);
        }
        Instruction::Length {
            destination,
            vector,
            ..
        } if analysis.values.get(vector) == Some(&Type::String) => {
            block_state.constants.remove(destination);
            let length = I::call(T::I32, "bn_rt_str_len", vec![(T::Ptr, v(*vector))]);
            text.assign(format!("v{}", destination.0), length);
        }
        Instruction::SizeOf {
            destination, value, ..
        } => {
            block_state.constants.remove(destination);
            let continuation = take_continuation(block_id, state);
            let dest = destination.0;
            let bytes = O::reg(format!("sizeofbytes{dest}"));
            let length = I::call(T::I64, "bn_string_byte_length", vec![(T::Ptr, v(*value))]);
            text.assign(format!("sizeofbytes{dest}"), length);
            let limit = O::int(i64::from(i32::MAX));
            let big = I::icmp(ICmpCond::Ugt, T::I64, bytes.clone(), limit);
            text.assign(format!("sizeofbig{dest}"), big);
            emit_trap(
                text,
                block_id,
                state,
                &format!("%sizeofbig{dest}"),
                continuation,
                bn_diag::DiagId::NUMERIC_OVERFLOW,
                vec![(
                    "operation",
                    Fact::Text("converting a value to INTEGER".into()),
                )],
            );
            text.assign(
                format!("v{dest}"),
                I::cast(CastOp::Trunc, T::I64, bytes, T::I32),
            );
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
            emit_vector_length(text, *destination, *vector, analysis);
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
            // A `FS.File` is a runtime handle, not an object: no class
            // header, no ARC ownership.
            let file_handle = matches!(ty, Type::Named(name) if name == "FS.File");
            if !matches!(ty, Type::Pointer { .. })
                && !is_bndata_dataframe_type(module, ty)
                && bnlog_resource_kind(module, ty).is_none()
                && !file_handle
            {
                let class_global = format!("@.bn_cls_{}", sanitize_symbol(type_name));
                emit_store_object_class(text, *destination, &class_global);
            }
            // The ARC core counts the new object or region from one strong
            // reference, the one `NEW` yields; its id goes to the header.
            if is_region_type(ty) {
                arc_ops::register(
                    text,
                    *destination,
                    None,
                    &format!("%allocbase{}", destination.0),
                    state,
                );
            } else if is_class_type(module, ty) {
                arc_ops::register(
                    text,
                    *destination,
                    Some(&format!(".bn_cls_{}", sanitize_symbol(type_name))),
                    &format!("%v{}", destination.0),
                    state,
                );
            }
        }
        _ => return false,
    }
    true
}
