#![allow(
    clippy::wildcard_imports,
    clippy::match_same_arms,
    clippy::too_many_arguments
)]
use super::*;

pub(crate) fn emit_vector(
    text: &mut String,
    module: &Module,
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
    let stored_type = if dimensions.len() == 1 {
        element.as_ref().clone()
    } else {
        Type::Vector {
            element: element.clone(),
            dimensions: dimensions[1..].to_vec(),
        }
    };
    let elem_ty = llvm_type(&stored_type).expect("validated vector element");
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
        let operand = if is_struct_type(module, &stored_type) {
            let Type::Named(owner) = &stored_type else {
                unreachable!("validated struct vector element");
            };
            let bytes = class_instance_bytes(module, owner);
            let _ = writeln!(
                text,
                "  %vecstructcopy{dest}_{index} = alloca [{bytes} x i8]"
            );
            let _ = writeln!(
                text,
                "  call void @llvm.memcpy.p0.p0.i64(ptr %vecstructcopy{dest}_{index}, ptr %v{}, i64 {bytes}, i1 false)",
                element_id.0
            );
            format!("%vecstructcopy{dest}_{index}")
        } else {
            coerce_to_type(text, *element_id, source_ty, &stored_type)
        };
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
    module: &Module,
    destination: ValueId,
    arguments: &[ValueId],
    ty: &Type,
    analysis: &LoweringAnalysis<'_>,
    object_bytes: u64,
) {
    let dest = destination.0;
    if is_bndata_dataframe_type(module, ty) {
        let _ = writeln!(text, "  %dfout{dest} = alloca i64");
        let _ = writeln!(
            text,
            "  %dfrc{dest} = call i32 @bn_rt_dataframe_create(ptr null, i32 0, ptr %dfout{dest})"
        );
        let _ = writeln!(text, "  %dfhandle{dest} = load i64, ptr %dfout{dest}");
        let _ = writeln!(text, "  %v{dest} = inttoptr i64 %dfhandle{dest} to ptr");
        return;
    }
    if let Some(kind) = bnlog_resource_kind(module, ty) {
        let symbol = if kind == "Fields" {
            "bn_rt_log_fields_create"
        } else {
            "bn_rt_log_logger_create"
        };
        let _ = writeln!(text, "  %loghandle{dest} = call i64 @{symbol}()");
        let _ = writeln!(text, "  %v{dest} = inttoptr i64 %loghandle{dest} to ptr");
        let _ = writeln!(text, "  store i64 %loghandle{dest}, ptr %logowned{dest}");
        return;
    }
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
    let _ = writeln!(text, "  %v{dest} = call ptr @calloc(i64 1, i64 {bytes})");
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
    state: &mut EmissionState,
) {
    let llvm_ty = llvm_type(field_ty).expect("validated member type");
    let mut value_op = coerce_to_type(text, value, value_ty, field_ty);
    if let Type::Vector { element, .. } = field_ty {
        let tag = state.continuation_count;
        state.continuation_count += 1;
        let element_llvm = llvm_type(element).expect("validated vector field element");
        let _ = writeln!(
            text,
            "  %fieldsrc{tag} = extractvalue {{ ptr, i32 }} {value_op}, 0"
        );
        let _ = writeln!(
            text,
            "  %fieldlen{tag} = extractvalue {{ ptr, i32 }} {value_op}, 1"
        );
        let _ = writeln!(text, "  %fieldlen64_{tag} = zext i32 %fieldlen{tag} to i64");
        let element_bytes = match element_llvm {
            "i1" | "i8" => 1,
            "i16" => 2,
            "i32" | "float" => 4,
            "i64" | "double" | "ptr" => 8,
            _ => unreachable!("validated scalar vector field element"),
        };
        let _ = writeln!(
            text,
            "  %fieldbytes{tag} = mul i64 %fieldlen64_{tag}, {element_bytes}"
        );
        let _ = writeln!(
            text,
            "  %fieldcopy{tag} = call ptr @malloc(i64 %fieldbytes{tag})"
        );
        let _ = writeln!(
            text,
            "  call void @llvm.memcpy.p0.p0.i64(ptr %fieldcopy{tag}, ptr %fieldsrc{tag}, i64 %fieldbytes{tag}, i1 false)"
        );
        let _ = writeln!(
            text,
            "  %fieldfat0_{tag} = insertvalue {{ ptr, i32 }} undef, ptr %fieldcopy{tag}, 0"
        );
        let _ = writeln!(
            text,
            "  %fieldfat{tag} = insertvalue {{ ptr, i32 }} %fieldfat0_{tag}, i32 %fieldlen{tag}, 1"
        );
        value_op = format!("%fieldfat{tag}");
    }
    let _ = writeln!(
        text,
        "  %mbrptr{} = getelementptr i8, ptr %v{}, i32 {field_offset}",
        value.0, object.0
    );
    if matches!(field_ty, Type::Vector { .. }) {
        let tag = state.continuation_count - 1;
        let _ = writeln!(
            text,
            "  %fieldoldfat{tag} = load {{ ptr, i32 }}, ptr %mbrptr{}",
            value.0
        );
        let _ = writeln!(
            text,
            "  %fieldoldptr{tag} = extractvalue {{ ptr, i32 }} %fieldoldfat{tag}, 0"
        );
        let _ = writeln!(text, "  call void @free(ptr %fieldoldptr{tag})");
    }
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

pub(crate) fn emit_delete(text: &mut String, module: &Module, value: ValueId, ty: &Type) {
    match llvm_type(ty) {
        Some("{ ptr, i32 }") => {
            let _ = writeln!(
                text,
                "  %delptr{} = extractvalue {{ ptr, i32 }} %v{}, 0",
                value.0, value.0
            );
            let _ = writeln!(text, "  call void @free(ptr %delptr{})", value.0);
        }
        Some("ptr") => {
            let owner = match ty {
                Type::Named(name) | Type::ImportedNamed { name, .. } => Some(name.as_str()),
                _ => None,
            };
            if let Some(owner) = owner {
                for (index, offset) in vector_field_offsets(module, owner).into_iter().enumerate() {
                    let _ = writeln!(
                        text,
                        "  %delfieldptr{}_{index} = getelementptr i8, ptr %v{}, i32 {offset}",
                        value.0, value.0
                    );
                    let _ = writeln!(
                        text,
                        "  %delfield{}_{index} = load {{ ptr, i32 }}, ptr %delfieldptr{}_{index}",
                        value.0, value.0
                    );
                    let _ = writeln!(
                        text,
                        "  %delfielddata{}_{index} = extractvalue {{ ptr, i32 }} %delfield{}_{index}, 0",
                        value.0, value.0
                    );
                    let _ = writeln!(
                        text,
                        "  call void @free(ptr %delfielddata{}_{index})",
                        value.0
                    );
                }
            }
            let _ = writeln!(text, "  call void @free(ptr %v{})", value.0);
        }
        _ => {
            let _ = writeln!(text, "  call void @free(ptr %v{})", value.0);
        }
    }
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
    state.control_flow.label(text, ok.clone());
    let _ = writeln!(
        text,
        "  %vecslot{dest} = getelementptr {elem_ty}, ptr %vecptr{dest}, i32 {index_op}"
    );
    let _ = writeln!(text, "  %v{dest} = load {elem_ty}, ptr %vecslot{dest}");
    state.needs_numeric_overflow_trap = true;
}

#[allow(clippy::too_many_lines)]
pub(crate) fn emit_is(
    text: &mut String,
    destination: ValueId,
    left: ValueId,
    left_ty: &Type,
    right_ty: &Type,
) {
    let test_name = is_test_name(right_ty);
    if let Type::Alternative(alternatives) = left_ty
        && (string_na_or_error(alternatives) || scalar_na_or_error(alternatives))
    {
        let id = destination.0;
        let _ = writeln!(
            text,
            "  %cellerror{id} = extractvalue {{ i1, ptr, i64 }} %v{}, 0",
            left.0
        );
        let _ = writeln!(
            text,
            "  %cellptr{id} = extractvalue {{ i1, ptr, i64 }} %v{}, 1",
            left.0
        );
        let _ = writeln!(
            text,
            "  %cellnaptr{id} = getelementptr [3 x i8], ptr @.bn_na, i64 0, i64 0"
        );
        let _ = writeln!(
            text,
            "  %cellna{id} = icmp eq ptr %cellptr{id}, %cellnaptr{id}"
        );
        if test_name == "Error" {
            let _ = writeln!(text, "  %v{id} = or i1 false, %cellerror{id}");
        } else if test_name == "NA" {
            let _ = writeln!(text, "  %cellok{id} = xor i1 %cellerror{id}, true");
            let _ = writeln!(text, "  %v{id} = and i1 %cellok{id}, %cellna{id}");
        } else {
            let matches = alternatives.iter().any(|ty| ty == right_ty);
            let _ = writeln!(
                text,
                "  %cellabsent{id} = or i1 %cellerror{id}, %cellna{id}"
            );
            let _ = writeln!(text, "  %cellpresent{id} = xor i1 %cellabsent{id}, true");
            let _ = writeln!(
                text,
                "  %v{id} = and i1 %cellpresent{id}, {}",
                u8::from(matches)
            );
        }
        return;
    }
    if emit_string_or_eof_is(text, destination, left, left_ty, test_name) {
        return;
    }
    if emit_optional_integer_is(text, destination, left, left_ty, right_ty, test_name) {
        return;
    }
    if emit_integer_error_union_is(text, destination, left, left_ty, test_name) {
        return;
    }
    if llvm_type(left_ty) == Some("ptr") && (test_name == "NULL" || matches!(right_ty, Type::Null))
    {
        let _ = writeln!(
            text,
            "  %v{} = icmp eq ptr %v{}, null",
            destination.0, left.0
        );
        return;
    }
    if matches!(
        llvm_type(left_ty),
        Some("{ i1, ptr }" | "{ i1, ptr, i32 }" | "{ i1, ptr, i64 }")
    ) && test_name == "Error"
    {
        let _ = writeln!(
            text,
            "  %v{} = extractvalue {} %v{}, 0",
            destination.0,
            llvm_type(left_ty).expect("validated error aggregate"),
            left.0
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

fn emit_integer_error_union_is(
    text: &mut String,
    destination: ValueId,
    left: ValueId,
    left_ty: &Type,
    test_name: &str,
) -> bool {
    if integer_union_payload(left_ty).is_none()
        || !matches!(
            test_name,
            "INTEGER"
                | "INT8"
                | "INT16"
                | "INT32"
                | "INT64"
                | "BYTE"
                | "UINT16"
                | "UINT32"
                | "UINT64"
        )
    {
        return false;
    }
    let dest = destination.0;
    let _ = writeln!(
        text,
        "  %unioniserror{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 0",
        left.0
    );
    let _ = writeln!(
        text,
        "  %unionnoterror{dest} = xor i1 %unioniserror{dest}, true"
    );
    if matches!(left_ty, Type::Alternative(alternatives) if alternatives.iter().any(|ty| matches!(ty, Type::EndOfFile)))
    {
        let _ = writeln!(
            text,
            "  %unionisptr{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 1",
            left.0
        );
        let _ = writeln!(
            text,
            "  %unionnoteof{dest} = icmp ne ptr %unionisptr{dest}, @.bn_eof"
        );
        let _ = writeln!(
            text,
            "  %v{dest} = and i1 %unionnoterror{dest}, %unionnoteof{dest}"
        );
    } else {
        let _ = writeln!(text, "  %v{dest} = or i1 false, %unionnoterror{dest}");
    }
    true
}

fn is_test_name(ty: &Type) -> &str {
    match ty {
        Type::TypeName(name) | Type::Named(name) => name,
        Type::NotAvailable => "NA",
        Type::Null => "NULL",
        _ => "",
    }
}

/// `INTEGER OR NULL` uses field zero as its null tag. Type tests must inspect
/// that tag instead of treating the statically alternative-typed value as a
/// failed scalar test.
fn emit_optional_integer_is(
    text: &mut String,
    destination: ValueId,
    left: ValueId,
    left_ty: &Type,
    right_ty: &Type,
    test_name: &str,
) -> bool {
    if llvm_type(left_ty) != Some("{ i1, i32 }") {
        return false;
    }
    let tests_null = test_name == "NULL" || matches!(right_ty, Type::Null);
    let tests_integer = matches!(test_name, "INTEGER" | "INT32")
        || matches!(right_ty, Type::Integer(IntegerType::Int32));
    if !tests_null && !tests_integer {
        return false;
    }
    if tests_null {
        let _ = writeln!(
            text,
            "  %v{} = extractvalue {{ i1, i32 }} %v{}, 0",
            destination.0, left.0
        );
    } else {
        let _ = writeln!(
            text,
            "  %optisnull{} = extractvalue {{ i1, i32 }} %v{}, 0",
            destination.0, left.0
        );
        let _ = writeln!(
            text,
            "  %v{} = xor i1 %optisnull{}, true",
            destination.0, destination.0
        );
    }
    true
}

/// `INPUT` uses a string pointer or the unique EOF sentinel. Its erased
/// pointer representation still requires a dynamic tag test. A STRING whose
/// contents are `EOF` remains distinct because emitted string globals retain
/// address significance.
fn emit_string_or_eof_is(
    text: &mut String,
    destination: ValueId,
    left: ValueId,
    left_ty: &Type,
    test_name: &str,
) -> bool {
    if *left_ty != Type::String || !matches!(test_name, "EOF" | "STRING") {
        return false;
    }
    let comparison = if test_name == "EOF" { "eq" } else { "ne" };
    let _ = writeln!(
        text,
        "  %v{} = icmp {comparison} ptr %v{}, @.bn_eof",
        destination.0, left.0
    );
    true
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
    use bn_ir::ValueId;
    use bn_types::{IntegerType, Type};

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
