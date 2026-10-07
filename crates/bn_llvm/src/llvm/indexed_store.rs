#![allow(clippy::wildcard_imports)]
use super::*;
use crate::ir::{
    BinaryOp, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T,
};
use crate::layout::{typed_llvm, vector_ty};

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_pointer_set_index(
    text: &mut String,
    block_id: BlockId,
    symbol_slot: usize,
    index: ValueId,
    index_ty: &Type,
    value: ValueId,
    value_ty: &Type,
    elem_ty: &Type,
    previous: Option<ValueId>,
    context: &'static str,
    state: &mut EmissionState,
) {
    let tag = state.continuation_count;
    let index_op = coerce_to_type(text, index, index_ty, &Type::Integer(IntegerType::Int32));
    let value_op = coerce_to_type(text, value, value_ty, elem_ty);
    let ok = take_continuation(block_id, state);
    let llvm_elem = llvm_type(elem_ty).expect("validated indexed-store element");
    let slot = O::reg(format!("s{symbol_slot}"));
    text.assign(format!("setfat{tag}"), I::load(vector_ty(), slot));
    emit_fat_pointer_store(
        text,
        block_id,
        context,
        tag,
        &format!("%setfat{tag}"),
        &index_op,
        &value_op,
        llvm_elem,
        previous,
        ok,
        state,
    );
}

/// Bounds-checks `index` against the vector `fat_pointer`: writes
/// `%{prefix}<ptr|len|neg|oob|bad>{tag}` and traps through `continuation`.
#[allow(clippy::too_many_arguments)]
fn emit_bounds_check(
    text: &mut String,
    block_id: BlockId,
    state: &mut EmissionState,
    prefix: &str,
    tag: usize,
    fat_pointer: &str,
    index: &str,
    continuation: String,
    context: &'static str,
) {
    let r = |name: &str| O::reg(format!("{prefix}{name}{tag}"));
    let fat = O::raw(fat_pointer);
    let at = O::raw(index);
    text.assign(
        format!("{prefix}ptr{tag}"),
        I::extract(vector_ty(), fat.clone(), 0),
    );
    text.assign(format!("{prefix}len{tag}"), I::extract(vector_ty(), fat, 1));
    let negative = I::icmp(ICmpCond::Slt, T::I32, at.clone(), O::int(0));
    text.assign(format!("{prefix}neg{tag}"), negative);
    text.assign(
        format!("{prefix}oob{tag}"),
        I::icmp(ICmpCond::Uge, T::I32, at, r("len")),
    );
    text.assign(
        format!("{prefix}bad{tag}"),
        I::binary(BinaryOp::Or, T::I1, r("neg"), r("oob")),
    );
    emit_index_trap(
        text,
        block_id,
        state,
        &format!("%{prefix}bad{tag}"),
        continuation,
        index,
        &format!("%{prefix}len{tag}"),
        context,
    );
}

/// Writes `%{prefix}slot{tag}`, the element of type `llvm_elem` at `index`;
/// the element replaced goes to `previous` (explicit ownership).
fn emit_element_store(
    text: &mut String,
    prefix: &str,
    tag: usize,
    (index, value, llvm_elem): (&str, &str, &str),
    previous: Option<ValueId>,
) {
    let elem = typed_llvm(llvm_elem);
    let slot = O::reg(format!("{prefix}slot{tag}"));
    let base = O::reg(format!("{prefix}ptr{tag}"));
    let at = vec![(T::I32, O::raw(index))];
    text.assign(format!("{prefix}slot{tag}"), I::gep(elem.clone(), base, at));
    if let Some(previous) = previous {
        text.assign(
            format!("v{}", previous.0),
            I::load(elem.clone(), slot.clone()),
        );
    }
    text.emit(I::store(elem, O::raw(value), slot));
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_vector_set_indices(
    text: &mut String,
    block_id: BlockId,
    symbol_slot: usize,
    indices: &[ValueId],
    value: ValueId,
    value_ty: &Type,
    elem_ty: &Type,
    previous: Option<ValueId>,
    analysis: &LoweringAnalysis<'_>,
    state: &mut EmissionState,
) {
    let base_tag = state.continuation_count;
    let value_op = coerce_to_type(text, value, value_ty, elem_ty);
    let llvm_elem = llvm_type(elem_ty).expect("validated indexed-store element");
    let mut fat_pointer = format!("%mdsetfat{base_tag}_0");
    text.assign(
        &fat_pointer,
        I::load(vector_ty(), O::reg(format!("s{symbol_slot}"))),
    );

    for (depth, index) in indices.iter().enumerate() {
        let tag = state.continuation_count;
        let index_ty = analysis
            .values
            .get(index)
            .expect("validated multidimensional index type");
        let index_op = coerce_to_type(text, *index, index_ty, &Type::Integer(IntegerType::Int32));
        let continuation = take_continuation(block_id, state);
        emit_bounds_check(
            text,
            block_id,
            state,
            "mdset",
            tag,
            &fat_pointer,
            &index_op,
            continuation,
            "vector",
        );
        if depth + 1 == indices.len() {
            let store = (index_op.as_str(), value_op.as_str(), llvm_elem);
            emit_element_store(text, "mdset", tag, store, previous);
        } else {
            let next = format!("%mdsetfat{base_tag}_{}", depth + 1);
            let at = vec![(T::I32, O::raw(index_op))];
            let row = I::gep(vector_ty(), O::reg(format!("mdsetptr{tag}")), at);
            text.assign(format!("mdsetslot{tag}"), row);
            text.assign(
                &next,
                I::load(vector_ty(), O::reg(format!("mdsetslot{tag}"))),
            );
            fat_pointer = next;
        }
    }
    state.needs_numeric_overflow_trap = true;
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_field_set_index(
    text: &mut String,
    block_id: BlockId,
    symbol_slot: usize,
    field_offset: u32,
    index: ValueId,
    index_ty: &Type,
    value: ValueId,
    value_ty: &Type,
    elem_ty: &Type,
    previous: Option<ValueId>,
    state: &mut EmissionState,
) {
    let tag = state.continuation_count;
    let index_op = coerce_to_type(text, index, index_ty, &Type::Integer(IntegerType::Int32));
    let value_op = coerce_to_type(text, value, value_ty, elem_ty);
    let ok = take_continuation(block_id, state);
    let llvm_elem = llvm_type(elem_ty).expect("validated indexed-field element");
    let r = |name: &str| O::reg(format!("fieldset{name}{tag}"));
    text.assign(
        format!("fieldsetobj{tag}"),
        I::load(T::Ptr, O::reg(format!("s{symbol_slot}"))),
    );
    let offset = vec![(T::I32, O::uint(u64::from(field_offset)))];
    text.assign(format!("fieldsetptr{tag}"), I::gep(T::I8, r("obj"), offset));
    text.assign(format!("fieldsetfat{tag}"), I::load(vector_ty(), r("ptr")));
    emit_fat_pointer_store(
        text,
        block_id,
        "vector",
        tag,
        &format!("%fieldsetfat{tag}"),
        &index_op,
        &value_op,
        llvm_elem,
        previous,
        ok,
        state,
    );
}

#[allow(clippy::too_many_arguments)]
fn emit_fat_pointer_store(
    text: &mut String,
    block_id: BlockId,
    context: &'static str,
    tag: usize,
    fat_pointer: &str,
    index: &str,
    value: &str,
    llvm_elem: &str,
    previous: Option<ValueId>,
    continuation: String,
    state: &mut EmissionState,
) {
    emit_bounds_check(
        text,
        block_id,
        state,
        "set",
        tag,
        fat_pointer,
        index,
        continuation,
        context,
    );
    emit_element_store(text, "set", tag, (index, value, llvm_elem), previous);
    state.needs_numeric_overflow_trap = true;
}
