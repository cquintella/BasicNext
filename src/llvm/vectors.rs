#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn emit_vector(
    text: &mut String,
    destination: ValueId,
    elements: &[ValueId],
    ty: &Type,
    analysis: &LoweringAnalysis<'_>,
) {
    let Type::Vector {
        element,
        dimensions,
    } = ty
    else {
        unreachable!("validated vector type");
    };
    let elem_ty = llvm_type(element).expect("validated vector element");
    let len = u32::try_from(dimensions[0]).unwrap_or(0);
    let dest = destination.0;
    let _ = writeln!(text, "  %vecdata{dest} = alloca [{len} x {elem_ty}]");
    for (index, element_id) in elements.iter().enumerate() {
        let _ = writeln!(
            text,
            "  %vecslot{dest}_{index} = getelementptr [{len} x {elem_ty}], ptr %vecdata{dest}, i32 0, i32 {index}"
        );
        let source_ty = analysis
            .values
            .get(element_id)
            .expect("validated vector element value");
        let operand = coerce_to_type(text, *element_id, source_ty, element);
        let _ = writeln!(
            text,
            "  store {elem_ty} {operand}, ptr %vecslot{dest}_{index}"
        );
    }
    let _ = writeln!(
        text,
        "  %vecptr{dest} = getelementptr [{len} x {elem_ty}], ptr %vecdata{dest}, i32 0, i32 0"
    );
    let _ = writeln!(
        text,
        "  %vecfat{dest} = insertvalue {{ ptr, i32 }} undef, ptr %vecptr{dest}, 0"
    );
    let _ = writeln!(
        text,
        "  %v{dest} = insertvalue {{ ptr, i32 }} %vecfat{dest}, i32 {len}, 1"
    );
}

pub(crate) fn emit_vector_length(text: &mut String, destination: ValueId, vector: ValueId) {
    let _ = writeln!(
        text,
        "  %v{} = extractvalue {{ ptr, i32 }} %v{}, 1",
        destination.0, vector.0
    );
}

pub(crate) fn emit_allocate(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    ty: &Type,
    analysis: &LoweringAnalysis<'_>,
    object_bytes: u64,
) {
    let dest = destination.0;
    if matches!(ty, Type::Pointer { .. }) {
        let element = match ty {
            Type::Pointer { element, .. } => element.as_ref(),
            _ => unreachable!(),
        };
        let elem_ty = llvm_type(element).expect("validated allocate element");
        let elem_bytes: u64 = match elem_ty {
            "i8" => 1,
            "i16" => 2,
            "i32" | "float" => 4,
            "i64" | "double" | "ptr" => 8,
            _ => 4,
        };
        let len_op = if let Some(len_value) = arguments.first().copied() {
            let len_ty = analysis
                .values
                .get(&len_value)
                .expect("validated allocate length type");
            coerce_to_type(text, len_value, len_ty, &Type::Integer(IntegerType::Int32))
        } else {
            "1".to_string()
        };
        let _ = writeln!(text, "  %alloclen{dest} = zext i32 {len_op} to i64");
        let _ = writeln!(
            text,
            "  %allocbytes{dest} = mul i64 %alloclen{dest}, {elem_bytes}"
        );
        let _ = writeln!(
            text,
            "  %allocptr{dest} = call ptr @malloc(i64 %allocbytes{dest})"
        );
        let _ = writeln!(
            text,
            "  %allocfat{dest} = insertvalue {{ ptr, i32 }} undef, ptr %allocptr{dest}, 0"
        );
        let _ = writeln!(
            text,
            "  %v{dest} = insertvalue {{ ptr, i32 }} %allocfat{dest}, i32 {len_op}, 1"
        );
        return;
    }
    let bytes = object_bytes.max(u64::from(OBJECT_HEADER_BYTES) + 4);
    let _ = writeln!(text, "  %v{dest} = call ptr @malloc(i64 {bytes})");
}

pub(crate) fn emit_store_object_class(text: &mut String, object: ValueId, class_global: &str) {
    let _ = writeln!(text, "  store ptr {class_global}, ptr %v{}", object.0);
}

pub(crate) fn emit_set_member(
    text: &mut String,
    object: ValueId,
    field_offset: u32,
    value: ValueId,
    value_ty: &Type,
    field_ty: &Type,
) {
    let llvm_ty = llvm_type(field_ty).expect("validated member type");
    let value_op = coerce_to_type(text, value, value_ty, field_ty);
    let _ = writeln!(
        text,
        "  %mbrptr{} = getelementptr i8, ptr %v{}, i32 {field_offset}",
        value.0, object.0
    );
    let _ = writeln!(text, "  store {llvm_ty} {value_op}, ptr %mbrptr{}", value.0);
}

pub(crate) fn emit_member(
    text: &mut String,
    destination: ValueId,
    object: ValueId,
    field_offset: u32,
    field_ty: &Type,
) {
    let llvm_ty = llvm_type(field_ty).expect("validated member type");
    let dest = destination.0;
    let _ = writeln!(
        text,
        "  %mbrptr{dest} = getelementptr i8, ptr %v{}, i32 {field_offset}",
        object.0
    );
    let _ = writeln!(text, "  %v{dest} = load {llvm_ty}, ptr %mbrptr{dest}");
}

pub(crate) fn emit_delete(text: &mut String, value: ValueId, ty: &Type) {
    match llvm_type(ty) {
        Some("{ ptr, i32 }") => {
            let _ = writeln!(
                text,
                "  %delptr{} = extractvalue {{ ptr, i32 }} %v{}, 0",
                value.0, value.0
            );
            let _ = writeln!(text, "  call void @free(ptr %delptr{})", value.0);
        }
        _ => {
            let _ = writeln!(text, "  call void @free(ptr %v{})", value.0);
        }
    }
}

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
    let dest = value.0;
    let llvm_elem = llvm_type(elem_ty).expect("validated setindex element");
    let index_op = coerce_to_type(text, index, index_ty, &Type::Integer(IntegerType::Int32));
    let value_op = coerce_to_type(text, value, value_ty, elem_ty);
    let ok = take_continuation(block_id, state);
    let _ = writeln!(
        text,
        "  %setfat{dest} = load {{ ptr, i32 }}, ptr %s{symbol_slot}"
    );
    let _ = writeln!(
        text,
        "  %setptr{dest} = extractvalue {{ ptr, i32 }} %setfat{dest}, 0"
    );
    let _ = writeln!(
        text,
        "  %setlen{dest} = extractvalue {{ ptr, i32 }} %setfat{dest}, 1"
    );
    let _ = writeln!(text, "  %setneg{dest} = icmp slt i32 {index_op}, 0");
    let _ = writeln!(
        text,
        "  %setoob{dest} = icmp uge i32 {index_op}, %setlen{dest}"
    );
    let _ = writeln!(text, "  %setbad{dest} = or i1 %setneg{dest}, %setoob{dest}");
    let _ = writeln!(
        text,
        "  br i1 %setbad{dest}, label %trap_numeric_overflow, label %{ok}"
    );
    let _ = writeln!(text, "{ok}:");
    let _ = writeln!(
        text,
        "  %setslot{dest} = getelementptr {llvm_elem}, ptr %setptr{dest}, i32 {index_op}"
    );
    let _ = writeln!(text, "  store {llvm_elem} {value_op}, ptr %setslot{dest}");
    state.needs_numeric_overflow_trap = true;
}

pub(crate) fn emit_vector_index(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    object: ValueId,
    index: ValueId,
    index_ty: &Type,
    ty: &Type,
    state: &mut EmissionState,
) {
    let elem_ty = llvm_type(ty).expect("validated index element");
    let dest = destination.0;
    let ok = take_continuation(block_id, state);
    let index_op = coerce_to_type(text, index, index_ty, &Type::Integer(IntegerType::Int32));
    let _ = writeln!(
        text,
        "  %vecptr{dest} = extractvalue {{ ptr, i32 }} %v{}, 0",
        object.0
    );
    let _ = writeln!(
        text,
        "  %veclen{dest} = extractvalue {{ ptr, i32 }} %v{}, 1",
        object.0
    );
    let _ = writeln!(text, "  %vecneg{dest} = icmp slt i32 {index_op}, 0");
    let _ = writeln!(
        text,
        "  %vecoob{dest} = icmp uge i32 {index_op}, %veclen{dest}"
    );
    let _ = writeln!(text, "  %vecbad{dest} = or i1 %vecneg{dest}, %vecoob{dest}");
    let _ = writeln!(
        text,
        "  br i1 %vecbad{dest}, label %trap_numeric_overflow, label %{ok}"
    );
    let _ = writeln!(text, "{ok}:");
    let _ = writeln!(
        text,
        "  %vecslot{dest} = getelementptr {elem_ty}, ptr %vecptr{dest}, i32 {index_op}"
    );
    let _ = writeln!(text, "  %v{dest} = load {elem_ty}, ptr %vecslot{dest}");
    state.needs_numeric_overflow_trap = true;
}

pub(crate) fn emit_is(
    text: &mut String,
    destination: ValueId,
    left: ValueId,
    left_ty: &Type,
    right_ty: &Type,
) {
    let test_name = match right_ty {
        Type::TypeName(name) | Type::Named(name) => name.as_str(),
        Type::NotAvailable => "NA",
        _ => "",
    };
    if llvm_type(left_ty) == Some("{ i1, ptr, i64 }") && test_name == "Error" {
        let _ = writeln!(
            text,
            "  %v{} = extractvalue {{ i1, ptr, i64 }} %v{}, 0",
            destination.0, left.0
        );
        return;
    }
    if llvm_type(left_ty) == Some("{ i1, ptr, i64 }")
        && (test_name == "EOF" || matches!(right_ty, Type::EndOfFile))
    {
        let _ = writeln!(
            text,
            "  %eofptr{} = getelementptr [4 x i8], ptr @.bn_eof, i64 0, i64 0",
            destination.0
        );
        let _ = writeln!(
            text,
            "  %eofvalue{} = extractvalue {{ i1, ptr, i64 }} %v{}, 1",
            destination.0, left.0
        );
        let _ = writeln!(
            text,
            "  %v{} = icmp eq ptr %eofvalue{}, %eofptr{}",
            destination.0, destination.0, destination.0
        );
        return;
    }
    if llvm_type(left_ty) == Some("{ i1, double }") && test_name == "NA" {
        let _ = writeln!(
            text,
            "  %v{} = extractvalue {{ i1, double }} %v{}, 0",
            destination.0, left.0
        );
        return;
    }
    if llvm_type(left_ty) == Some("{ i1, double }") && matches!(test_name, "FLOAT" | "FLOAT64") {
        let _ = writeln!(
            text,
            "  %isna{} = extractvalue {{ i1, double }} %v{}, 0",
            destination.0, left.0
        );
        let _ = writeln!(
            text,
            "  %v{} = xor i1 %isna{}, true",
            destination.0, destination.0
        );
        return;
    }
    let float_llvm = match left_ty {
        Type::Float(FloatType::Float32) => Some("float"),
        Type::Float(FloatType::Float64) | Type::FloatLiteral => Some("double"),
        _ => None,
    };
    if let Some(llvm_ty) = float_llvm {
        match test_name {
            "NAN" => {
                let _ = writeln!(
                    text,
                    "  %v{} = fcmp uno {llvm_ty} %v{}, 0.0",
                    destination.0, left.0
                );
                return;
            }
            "INF" => {
                let _ = writeln!(
                    text,
                    "  %v{} = fcmp oeq {llvm_ty} %v{}, 0x7FF0000000000000",
                    destination.0, left.0
                );
                return;
            }
            "-INF" => {
                let _ = writeln!(
                    text,
                    "  %v{} = fcmp oeq {llvm_ty} %v{}, 0xFFF0000000000000",
                    destination.0, left.0
                );
                return;
            }
            _ => {}
        }
    }
    let is_na = test_name == "NA";
    let _ = writeln!(
        text,
        "  %v{} = or i1 false, {}",
        destination.0,
        u8::from(is_na && matches!(left_ty, Type::NotAvailable))
    );
}

pub(crate) fn emit_optional_float_default(text: &mut String, destination: ValueId) {
    let dest = destination.0;
    let _ = writeln!(
        text,
        "  %na{dest} = insertvalue {{ i1, double }} undef, i1 false, 0"
    );
    let _ = writeln!(
        text,
        "  %v{dest} = insertvalue {{ i1, double }} %na{dest}, double 0.0, 1"
    );
}

pub(crate) fn extract_optional_float(text: &mut String, destination: ValueId, value: ValueId) {
    let _ = writeln!(
        text,
        "  %v{} = extractvalue {{ i1, double }} %v{}, 1",
        destination.0, value.0
    );
}

fn take_continuation(block_id: BlockId, state: &mut EmissionState) -> String {
    let name = format!("b{}.cont{}", block_id.0, state.continuation_count);
    state.continuation_count += 1;
    name
}

#[cfg(test)]
mod tests {
    use super::emit_is;
    use crate::{
        ir::ValueId,
        semantic::{IntegerType, Type},
    };

    #[test]
    fn eof_type_name_uses_the_eof_marker_for_alternative_values() {
        let mut llvm = String::new();
        emit_is(
            &mut llvm,
            ValueId(2),
            ValueId(1),
            &Type::Alternative(vec![
                Type::Integer(IntegerType::Int32),
                Type::EndOfFile,
                Type::Named("Error".into()),
            ]),
            &Type::TypeName("EOF".into()),
        );
        assert!(llvm.contains("icmp eq ptr"));
        assert!(llvm.contains("@.bn_eof"));
    }
}
