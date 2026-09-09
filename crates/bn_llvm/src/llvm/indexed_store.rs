use super::*;

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
    state: &mut EmissionState,
) {
    let tag = state.continuation_count;
    let index_op = coerce_to_type(text, index, index_ty, &Type::Integer(IntegerType::Int32));
    let value_op = coerce_to_type(text, value, value_ty, elem_ty);
    let ok = take_continuation(block_id, state);
    let llvm_elem = llvm_type(elem_ty).expect("validated indexed-store element");
    let _ = writeln!(
        text,
        "  %setfat{tag} = load {{ ptr, i32 }}, ptr %s{symbol_slot}"
    );
    emit_fat_pointer_store(
        text,
        block_id,
        tag,
        &format!("%setfat{tag}"),
        &index_op,
        &value_op,
        llvm_elem,
        ok,
        state,
    );
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
    analysis: &LoweringAnalysis<'_>,
    state: &mut EmissionState,
) {
    let base_tag = state.continuation_count;
    let value_op = coerce_to_type(text, value, value_ty, elem_ty);
    let llvm_elem = llvm_type(elem_ty).expect("validated indexed-store element");
    let mut fat_pointer = format!("%mdsetfat{base_tag}_0");
    let _ = writeln!(
        text,
        "  {fat_pointer} = load {{ ptr, i32 }}, ptr %s{symbol_slot}"
    );

    for (depth, index) in indices.iter().enumerate() {
        let tag = state.continuation_count;
        let index_ty = analysis
            .values
            .get(index)
            .expect("validated multidimensional index type");
        let index_op = coerce_to_type(text, *index, index_ty, &Type::Integer(IntegerType::Int32));
        let continuation = take_continuation(block_id, state);
        let _ = writeln!(
            text,
            "  %mdsetptr{tag} = extractvalue {{ ptr, i32 }} {fat_pointer}, 0"
        );
        let _ = writeln!(
            text,
            "  %mdsetlen{tag} = extractvalue {{ ptr, i32 }} {fat_pointer}, 1"
        );
        let _ = writeln!(text, "  %mdsetneg{tag} = icmp slt i32 {index_op}, 0");
        let _ = writeln!(
            text,
            "  %mdsetoob{tag} = icmp uge i32 {index_op}, %mdsetlen{tag}"
        );
        let _ = writeln!(
            text,
            "  %mdsetbad{tag} = or i1 %mdsetneg{tag}, %mdsetoob{tag}"
        );
        let _ = writeln!(
            text,
            "  br i1 %mdsetbad{tag}, label %trap_numeric_overflow, label %{continuation}"
        );
        state.control_flow.label(text, continuation);

        if depth + 1 == indices.len() {
            let _ = writeln!(
                text,
                "  %mdsetslot{tag} = getelementptr {llvm_elem}, ptr %mdsetptr{tag}, i32 {index_op}"
            );
            let _ = writeln!(text, "  store {llvm_elem} {value_op}, ptr %mdsetslot{tag}");
        } else {
            let next = format!("%mdsetfat{base_tag}_{}", depth + 1);
            let _ = writeln!(
                text,
                "  %mdsetslot{tag} = getelementptr {{ ptr, i32 }}, ptr %mdsetptr{tag}, i32 {index_op}"
            );
            let _ = writeln!(text, "  {next} = load {{ ptr, i32 }}, ptr %mdsetslot{tag}");
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
    state: &mut EmissionState,
) {
    let tag = state.continuation_count;
    let index_op = coerce_to_type(text, index, index_ty, &Type::Integer(IntegerType::Int32));
    let value_op = coerce_to_type(text, value, value_ty, elem_ty);
    let ok = take_continuation(block_id, state);
    let llvm_elem = llvm_type(elem_ty).expect("validated indexed-field element");
    let _ = writeln!(text, "  %fieldsetobj{tag} = load ptr, ptr %s{symbol_slot}");
    let _ = writeln!(
        text,
        "  %fieldsetptr{tag} = getelementptr i8, ptr %fieldsetobj{tag}, i32 {field_offset}"
    );
    let _ = writeln!(
        text,
        "  %fieldsetfat{tag} = load {{ ptr, i32 }}, ptr %fieldsetptr{tag}"
    );
    emit_fat_pointer_store(
        text,
        block_id,
        tag,
        &format!("%fieldsetfat{tag}"),
        &index_op,
        &value_op,
        llvm_elem,
        ok,
        state,
    );
}

#[allow(clippy::too_many_arguments)]
fn emit_fat_pointer_store(
    text: &mut String,
    _block_id: BlockId,
    tag: usize,
    fat_pointer: &str,
    index: &str,
    value: &str,
    llvm_elem: &str,
    continuation: String,
    state: &mut EmissionState,
) {
    let _ = writeln!(
        text,
        "  %setptr{tag} = extractvalue {{ ptr, i32 }} {fat_pointer}, 0"
    );
    let _ = writeln!(
        text,
        "  %setlen{tag} = extractvalue {{ ptr, i32 }} {fat_pointer}, 1"
    );
    let _ = writeln!(text, "  %setneg{tag} = icmp slt i32 {index}, 0");
    let _ = writeln!(text, "  %setoob{tag} = icmp uge i32 {index}, %setlen{tag}");
    let _ = writeln!(text, "  %setbad{tag} = or i1 %setneg{tag}, %setoob{tag}");
    let _ = writeln!(
        text,
        "  br i1 %setbad{tag}, label %trap_numeric_overflow, label %{continuation}"
    );
    state.control_flow.label(text, continuation);
    let _ = writeln!(
        text,
        "  %setslot{tag} = getelementptr {llvm_elem}, ptr %setptr{tag}, i32 {index}"
    );
    let _ = writeln!(text, "  store {llvm_elem} {value}, ptr %setslot{tag}");
    state.needs_numeric_overflow_trap = true;
}
