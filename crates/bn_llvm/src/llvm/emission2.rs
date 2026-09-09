#![allow(
    clippy::wildcard_imports,
    clippy::match_same_arms,
    clippy::too_many_lines
)]
use super::*;

pub(crate) fn lower_terminator(
    text: &mut String,
    terminator: &Terminator,
    analysis: &LoweringAnalysis<'_>,
    symbols: &HashMap<SymbolId, usize>,
    _block_state: &mut BlockState,
    state: &mut EmissionState,
) {
    match terminator {
        Terminator::Jump { target } => {
            let _ = writeln!(text, "  br label %b{}", target.0);
        }
        Terminator::Branch {
            condition,
            then_block,
            else_block,
        } => {
            let operand = i1_operand(text, analysis, state, *condition);
            let _ = writeln!(
                text,
                "  br i1 {operand}, label %b{}, label %b{}",
                then_block.0, else_block.0
            );
        }
        Terminator::Return { value: None } if state.is_start => {
            cleanup_owned_memory(text, analysis, symbols, state);
            text.push_str("  ret i32 0\n");
        }
        Terminator::Return { value: None } if state.return_llvm == "void" => {
            cleanup_owned_memory(text, analysis, symbols, state);
            text.push_str("  ret void\n");
        }
        Terminator::Return { value: None } => {
            text.push_str("  unreachable\n");
        }
        Terminator::Stop { code: value } if !state.is_start => {
            let operand = coerce_return_operand(
                text,
                *value,
                analysis.values.get(value).expect("validated stop type"),
            );
            cleanup_owned_memory(text, analysis, symbols, state);
            let _ = writeln!(text, "  call void @exit(i32 {operand})");
            text.push_str("  unreachable\n");
        }
        Terminator::Return { value: Some(value) } if !state.is_start => {
            if state.return_llvm == "void" {
                cleanup_owned_memory(text, analysis, symbols, state);
                text.push_str("  ret void\n");
            } else {
                let value_ty = analysis.values.get(value).expect("validated return type");
                let operand = if state.return_llvm == "{ i1, ptr, i64 }"
                    && matches!(value_ty, Type::Integer(_) | Type::IntegerLiteral(_))
                {
                    integer_error_union_return_operand(text, *value, value_ty)
                } else if state.return_llvm == "{ i1, i32 }" {
                    optional_integer_return_operand(text, *value, value_ty)
                } else if llvm_type(value_ty) == Some(state.return_llvm) {
                    format!("%v{}", value.0)
                } else if matches!(state.return_llvm, "i8" | "i16" | "i32" | "i64")
                    && matches!(llvm_type(value_ty), Some("i8" | "i16" | "i32" | "i64"))
                {
                    coerce_to_type(
                        text,
                        *value,
                        value_ty,
                        match state.return_llvm {
                            "i8" => &Type::Integer(IntegerType::Int8),
                            "i16" => &Type::Integer(IntegerType::Int16),
                            "i32" => &Type::Integer(IntegerType::Int32),
                            _ => &Type::Integer(IntegerType::Int64),
                        },
                    )
                } else {
                    format!("%v{}", value.0)
                };
                cleanup_owned_memory(text, analysis, symbols, state);
                let _ = writeln!(text, "  ret {} {operand}", state.return_llvm);
            }
        }
        Terminator::Return { value: Some(value) } | Terminator::Stop { code: value } => {
            let operand = coerce_return_operand(
                text,
                *value,
                analysis.values.get(value).expect("validated return type"),
            );
            cleanup_owned_memory(text, analysis, symbols, state);
            let _ = writeln!(text, "  ret i32 {operand}");
        }
    }
}

fn integer_error_union_return_operand(text: &mut String, value: ValueId, ty: &Type) -> String {
    let payload = coerce_to_type(text, value, ty, &Type::Integer(IntegerType::Int64));
    let _ = writeln!(
        text,
        "  %retuniontag{} = insertvalue {{ i1, ptr, i64 }} undef, i1 false, 0",
        value.0
    );
    let _ = writeln!(
        text,
        "  %retunionmessage{} = insertvalue {{ i1, ptr, i64 }} %retuniontag{}, ptr null, 1",
        value.0, value.0
    );
    let _ = writeln!(
        text,
        "  %retunion{} = insertvalue {{ i1, ptr, i64 }} %retunionmessage{}, i64 {payload}, 2",
        value.0, value.0
    );
    format!("%retunion{}", value.0)
}

fn lower_print_language_error_union(
    text: &mut String,
    value: ValueId,
    integer_value: bool,
    void_value: bool,
    scalar: Option<&Type>,
    state: &mut EmissionState,
) {
    let count = state.print_count;
    let _ = writeln!(
        text,
        "  %unionerror{count} = extractvalue {{ i1, ptr, i64 }} %v{}, 0",
        value.0
    );
    let _ = writeln!(
        text,
        "  %unionmessage{count} = extractvalue {{ i1, ptr, i64 }} %v{}, 1",
        value.0
    );
    let _ = writeln!(
        text,
        "  %unionpayload{count} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
        value.0
    );
    let _ = writeln!(
        text,
        "  br i1 %unionerror{count}, label %unionerr{count}, label %unionvalue{count}"
    );
    state.control_flow.label(text, format!("unionerr{count}"));
    let _ = writeln!(
        text,
        "  %unionerrprint{count} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_error, i64 %unionpayload{count}, ptr %unionmessage{count})"
    );
    let _ = writeln!(text, "  br label %unionjoin{count}");
    state.control_flow.label(text, format!("unionvalue{count}"));
    if scalar.is_some() {
        let _ = writeln!(
            text,
            "  %unionnaptr{count} = getelementptr [3 x i8], ptr @.bn_na, i64 0, i64 0"
        );
        let _ = writeln!(
            text,
            "  %unionisna{count} = icmp eq ptr %unionmessage{count}, %unionnaptr{count}"
        );
        let _ = writeln!(
            text,
            "  br i1 %unionisna{count}, label %unionna{count}, label %unionpresent{count}"
        );
        state.control_flow.label(text, format!("unionna{count}"));
        let _ = writeln!(
            text,
            "  call i32 (ptr, ...) @printf(ptr @.bn_fmt_str, ptr %unionnaptr{count})"
        );
        let _ = writeln!(text, "  br label %unionjoin{count}");
        state
            .control_flow
            .label(text, format!("unionpresent{count}"));
    }
    if matches!(scalar, Some(Type::Float(_))) {
        let _ = writeln!(
            text,
            "  %unionfloat{count} = bitcast i64 %unionpayload{count} to double"
        );
        let _ = writeln!(
            text,
            "  call void @bn_rt_print_float(double %unionfloat{count})"
        );
    } else if matches!(scalar, Some(Type::Boolean)) {
        let _ = writeln!(
            text,
            "  %unionbool{count} = icmp ne i64 %unionpayload{count}, 0"
        );
        let _ = writeln!(
            text,
            "  %unionboolstr{count} = select i1 %unionbool{count}, ptr @.bn_true, ptr @.bn_false"
        );
        let _ = writeln!(
            text,
            "  call i32 (ptr, ...) @printf(ptr @.bn_fmt_str, ptr %unionboolstr{count})"
        );
    } else if integer_value || matches!(scalar, Some(Type::Integer(_))) {
        let _ = writeln!(
            text,
            "  %unionintprint{count} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_int, i64 %unionpayload{count})"
        );
    } else if void_value {
        let _ = writeln!(
            text,
            "  %unionnullprint{count} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_str, ptr @.bn_null)"
        );
    } else {
        let _ = writeln!(
            text,
            "  %unionstrprint{count} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_str, ptr %unionmessage{count})"
        );
    }
    let _ = writeln!(text, "  br label %unionjoin{count}");
    state.control_flow.label(text, format!("unionjoin{count}"));
    state.print_count += 1;
}

pub(crate) fn cleanup_owned_memory(
    text: &mut String,
    analysis: &LoweringAnalysis<'_>,
    symbols: &HashMap<SymbolId, usize>,
    state: &mut EmissionState,
) {
    let cleanup = state.input_cleanup_count;
    state.input_cleanup_count += 1;
    let mut input_symbols = analysis.input_symbols.iter().collect::<Vec<_>>();
    input_symbols.sort_by_key(|symbol| symbol.0);
    for symbol in input_symbols {
        let slot = symbols[symbol];
        let _ = writeln!(
            text,
            "  %inputfree{cleanup}_{slot} = load ptr, ptr %s{slot}"
        );
        let _ = writeln!(
            text,
            "  %inputfreeowned{cleanup}_{slot} = load i1, ptr %inputowned{slot}"
        );
        let _ = writeln!(
            text,
            "  %inputfreenull{cleanup}_{slot} = select i1 %inputfreeowned{cleanup}_{slot}, ptr %inputfree{cleanup}_{slot}, ptr null"
        );
        let _ = writeln!(
            text,
            "  call void @free(ptr %inputfreenull{cleanup}_{slot})"
        );
    }
    let mut struct_results = analysis
        .owned_struct_results
        .iter()
        .copied()
        .collect::<Vec<_>>();
    struct_results.sort_by_key(|value| value.0);
    for value in struct_results {
        let _ = writeln!(
            text,
            "  %structfree{cleanup}_{} = load ptr, ptr %structowned{}",
            value.0, value.0
        );
        let _ = writeln!(
            text,
            "  call void @free(ptr %structfree{cleanup}_{})",
            value.0
        );
    }
    let mut log_results = analysis.owned_log_results.iter().collect::<Vec<_>>();
    log_results.sort_by_key(|(value, _)| value.0);
    for (value, kind) in log_results {
        let symbol = if *kind == "Fields" {
            "bn_rt_log_fields_close"
        } else {
            "bn_rt_log_logger_delete"
        };
        let _ = writeln!(
            text,
            "  %logfree{cleanup}_{} = load i64, ptr %logowned{}",
            value.0, value.0
        );
        let _ = writeln!(
            text,
            "  %logfreerc{cleanup}_{} = call i32 @{symbol}(i64 %logfree{cleanup}_{})",
            value.0, value.0
        );
    }
}

pub(crate) fn lower_print_value(
    text: &mut String,
    value: ValueId,
    ty: &Type,
    state: &mut EmissionState,
) {
    if let Type::Alternative(alternatives) = ty
        && (integer_or_error(alternatives)
            || string_or_error(alternatives)
            || void_or_error(alternatives)
            || string_na_or_error(alternatives)
            || scalar_na_or_error(alternatives))
    {
        lower_print_language_error_union(
            text,
            value,
            integer_or_error(alternatives),
            void_or_error(alternatives),
            if string_na_or_error(alternatives) || scalar_na_or_error(alternatives) {
                alternatives.iter().find(|ty| {
                    matches!(
                        ty,
                        Type::Integer(_) | Type::Float(_) | Type::Boolean | Type::String
                    )
                })
            } else {
                None
            },
            state,
        );
        return;
    }
    if let Type::Vector {
        element,
        dimensions,
    } = ty
        && dimensions.len() == 1
        && matches!(element.as_ref(), Type::Integer(IntegerType::Int32))
    {
        lower_print_int32_vector(text, value, dimensions[0], state);
        return;
    }
    if matches!(ty, Type::Named(name) if name == "DATE") {
        let _ = writeln!(text, "  call void @bn_rt_print_date(i32 %v{})", value.0);
        return;
    }
    if matches!(ty, Type::Named(name) if name == "TIME") {
        let _ = writeln!(text, "  call void @bn_rt_print_time(i32 %v{})", value.0);
        return;
    }
    if llvm_type(ty) == Some("{ i1, ptr, i64 }") {
        let count = state.print_count;
        let _ = writeln!(
            text,
            "  %netprint{count} = extractvalue {{ i1, ptr, i64 }} %v{}, 1",
            value.0
        );
        let _ = writeln!(
            text,
            "  %print{count} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_str, ptr %netprint{count})"
        );
        state.print_count += 1;
        return;
    }
    if llvm_type(ty) == Some("{ i1, i32 }") {
        lower_print_optional_integer(text, value, state);
        return;
    }
    if llvm_type(ty) == Some("{ i1, double }") {
        let count = state.print_count;
        let _ = writeln!(
            text,
            "  %optisna{count} = extractvalue {{ i1, double }} %v{}, 0",
            value.0
        );
        let _ = writeln!(
            text,
            "  %optval{count} = extractvalue {{ i1, double }} %v{}, 1",
            value.0
        );
        let _ = writeln!(
            text,
            "  br i1 %optisna{count}, label %optna{count}, label %optnum{count}"
        );
        state.control_flow.label(text, format!("optna{count}"));
        let _ = writeln!(
            text,
            "  %optnaprint{count} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_str, ptr @.bn_na)"
        );
        let _ = writeln!(text, "  br label %optjoin{count}");
        state.control_flow.label(text, format!("optnum{count}"));
        let _ = writeln!(
            text,
            "  call void @bn_rt_print_float(double %optval{count})"
        );
        let _ = writeln!(text, "  br label %optjoin{count}");
        state.control_flow.label(text, format!("optjoin{count}"));
        return;
    }
    match llvm_type(ty).expect("validated print type") {
        "i1" => {
            let _ = writeln!(
                text,
                "  %bool{} = select i1 %v{}, ptr @.bn_true, ptr @.bn_false",
                state.print_count, value.0
            );
            let _ = writeln!(
                text,
                "  %print{} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_str, ptr %bool{})",
                state.print_count, state.print_count
            );
        }
        "i8" | "i16" | "i32" => {
            let opcode = if is_unsigned(ty) { "zext" } else { "sext" };
            let _ = writeln!(
                text,
                "  %printint{} = {opcode} {} %v{} to i64",
                state.print_count,
                llvm_type(ty).expect("validated integer print type"),
                value.0
            );
            let _ = writeln!(
                text,
                "  %print{} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_int, i64 %printint{})",
                state.print_count, state.print_count
            );
        }
        "i64" => {
            let fmt = if is_unsigned(ty) {
                "@.bn_fmt_uint"
            } else {
                "@.bn_fmt_int"
            };
            let _ = writeln!(
                text,
                "  %print{} = call i32 (ptr, ...) @printf(ptr {fmt}, i64 %v{})",
                state.print_count, value.0
            );
        }
        "float" => {
            let _ = writeln!(
                text,
                "  %printfloat{} = fpext float %v{} to double",
                state.print_count, value.0
            );
            let _ = writeln!(
                text,
                "  call void @bn_rt_print_float(double %printfloat{})",
                state.print_count
            );
        }
        "double" => {
            let _ = writeln!(text, "  call void @bn_rt_print_float(double %v{})", value.0);
        }
        "ptr" => {
            let _ = writeln!(
                text,
                "  %print{} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_str, ptr %v{})",
                state.print_count, value.0
            );
        }
        _ => unreachable!("validated printable LLVM type"),
    }
    state.print_count += 1;
}

fn lower_print_int32_vector(
    text: &mut String,
    value: ValueId,
    length: u64,
    state: &mut EmissionState,
) {
    let vector = state.print_count;
    let _ = writeln!(text, "  %vecopen{vector} = call i32 @putchar(i32 91)");
    let _ = writeln!(
        text,
        "  %vecprintdata{vector} = extractvalue {{ ptr, i32 }} %v{}, 0",
        value.0
    );
    state.print_count += 1;
    for index in 0..length {
        let item = state.print_count;
        if index > 0 {
            let _ = writeln!(text, "  %veccomma{item} = call i32 @putchar(i32 44)");
            let _ = writeln!(text, "  %vecspace{item} = call i32 @putchar(i32 32)");
        }
        let _ = writeln!(
            text,
            "  %vecitemptr{item} = getelementptr i32, ptr %vecprintdata{vector}, i64 {index}"
        );
        let _ = writeln!(text, "  %vecitem{item} = load i32, ptr %vecitemptr{item}");
        let _ = writeln!(text, "  %vecitem64_{item} = sext i32 %vecitem{item} to i64");
        let _ = writeln!(
            text,
            "  %vecprint{item} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_int, i64 %vecitem64_{item})"
        );
        state.print_count += 1;
    }
    let close = state.print_count;
    let _ = writeln!(text, "  %vecclose{close} = call i32 @putchar(i32 93)");
    state.print_count += 1;
}

fn optional_integer_return_operand(text: &mut String, value: ValueId, ty: &Type) -> String {
    if llvm_type(ty) == Some("{ i1, i32 }") {
        return format!("%v{}", value.0);
    }

    let is_null = matches!(ty, Type::Null);
    let payload = if is_null {
        "0".to_string()
    } else {
        coerce_to_type(text, value, ty, &Type::Integer(IntegerType::Int32))
    };
    let tag = u8::from(is_null);
    let _ = writeln!(
        text,
        "  %retopttag{} = insertvalue {{ i1, i32 }} undef, i1 {tag}, 0",
        value.0
    );
    let _ = writeln!(
        text,
        "  %retopt{} = insertvalue {{ i1, i32 }} %retopttag{}, i32 {payload}, 1",
        value.0, value.0
    );
    format!("%retopt{}", value.0)
}

fn lower_print_optional_integer(text: &mut String, value: ValueId, state: &mut EmissionState) {
    let count = state.print_count;
    let _ = writeln!(
        text,
        "  %optisnull{count} = extractvalue {{ i1, i32 }} %v{}, 0",
        value.0
    );
    let _ = writeln!(
        text,
        "  %optintval{count} = extractvalue {{ i1, i32 }} %v{}, 1",
        value.0
    );
    let _ = writeln!(
        text,
        "  br i1 %optisnull{count}, label %optnull{count}, label %optint{count}"
    );
    state.control_flow.label(text, format!("optnull{count}"));
    let _ = writeln!(
        text,
        "  %optnullprint{count} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_str, ptr @.bn_null)"
    );
    let _ = writeln!(text, "  br label %optintjoin{count}");
    state.control_flow.label(text, format!("optint{count}"));
    let _ = writeln!(
        text,
        "  %optintwide{count} = sext i32 %optintval{count} to i64"
    );
    let _ = writeln!(
        text,
        "  %optintprint{count} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_int, i64 %optintwide{count})"
    );
    let _ = writeln!(text, "  br label %optintjoin{count}");
    state.control_flow.label(text, format!("optintjoin{count}"));
    state.print_count += 1;
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_checked_integer_op(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    operator: &str,
    left: ValueId,
    right: Option<ValueId>,
    left_ty: &Type,
    right_ty: &Type,
    ty: &Type,
    state: &mut EmissionState,
) {
    let intrinsic = checked_intrinsic_name(ty, operator).expect("validated checked intrinsic");
    let llvm_ty = llvm_type(ty).expect("validated integer type");
    let (left_operand, right_operand) = if operator == "Minus" && right.is_none() {
        ("0".into(), coerce_to_type(text, left, left_ty, ty))
    } else {
        (
            coerce_to_type(text, left, left_ty, ty),
            coerce_to_type(text, right.expect("binary op right"), right_ty, ty),
        )
    };
    let _ = writeln!(
        text,
        "  %ov{} = call {{ {llvm_ty}, i1 }} @{intrinsic}({llvm_ty} {left_operand}, {llvm_ty} {right_operand})",
        destination.0
    );
    let _ = writeln!(
        text,
        "  %v{} = extractvalue {{ {llvm_ty}, i1 }} %ov{}, 0",
        destination.0, destination.0
    );
    let _ = writeln!(
        text,
        "  %ovf{} = extractvalue {{ {llvm_ty}, i1 }} %ov{}, 1",
        destination.0, destination.0
    );
    let continuation = format!("b{}.cont{}", block_id.0, state.continuation_count);
    state.continuation_count += 1;
    let _ = writeln!(
        text,
        "  br i1 %ovf{}, label %trap_numeric_overflow, label %{continuation}",
        destination.0
    );
    state.control_flow.label(text, continuation.clone());
    state.needs_numeric_overflow_trap = true;
}

pub(crate) fn checked_intrinsic_name(ty: &Type, operator: &str) -> Option<&'static str> {
    let width = match llvm_type(ty)? {
        "i8" => "i8",
        "i16" => "i16",
        "i32" => "i32",
        "i64" => "i64",
        _ => return None,
    };
    let signed = !is_unsigned(ty);
    match operator {
        "Plus" => Some(match (signed, width) {
            (true, "i8") => "llvm.sadd.with.overflow.i8",
            (true, "i16") => "llvm.sadd.with.overflow.i16",
            (true, "i32") => "llvm.sadd.with.overflow.i32",
            (true, "i64") => "llvm.sadd.with.overflow.i64",
            (false, "i8") => "llvm.uadd.with.overflow.i8",
            (false, "i16") => "llvm.uadd.with.overflow.i16",
            (false, "i32") => "llvm.uadd.with.overflow.i32",
            (false, "i64") => "llvm.uadd.with.overflow.i64",
            _ => unreachable!(),
        }),
        "Minus" => Some(match (signed, width) {
            (true, "i8") => "llvm.ssub.with.overflow.i8",
            (true, "i16") => "llvm.ssub.with.overflow.i16",
            (true, "i32") => "llvm.ssub.with.overflow.i32",
            (true, "i64") => "llvm.ssub.with.overflow.i64",
            (false, "i8") => "llvm.usub.with.overflow.i8",
            (false, "i16") => "llvm.usub.with.overflow.i16",
            (false, "i32") => "llvm.usub.with.overflow.i32",
            (false, "i64") => "llvm.usub.with.overflow.i64",
            _ => unreachable!(),
        }),
        "Star" | "Multiply" => Some(match (signed, width) {
            (true, "i8") => "llvm.smul.with.overflow.i8",
            (true, "i16") => "llvm.smul.with.overflow.i16",
            (true, "i32") => "llvm.smul.with.overflow.i32",
            (true, "i64") => "llvm.smul.with.overflow.i64",
            (false, "i8") => "llvm.umul.with.overflow.i8",
            (false, "i16") => "llvm.umul.with.overflow.i16",
            (false, "i32") => "llvm.umul.with.overflow.i32",
            (false, "i64") => "llvm.umul.with.overflow.i64",
            _ => unreachable!(),
        }),
        _ => None,
    }
}

pub(crate) fn checked_intrinsic_declaration(ty: &Type, operator: &str) -> Option<&'static str> {
    let llvm_ty = llvm_type(ty)?;
    let name = checked_intrinsic_name(ty, operator)?;
    Some(match (llvm_ty, name) {
        ("i8", "llvm.sadd.with.overflow.i8") => "{ i8, i1 } @llvm.sadd.with.overflow.i8(i8, i8)",
        ("i16", "llvm.sadd.with.overflow.i16") => {
            "{ i16, i1 } @llvm.sadd.with.overflow.i16(i16, i16)"
        }
        ("i32", "llvm.sadd.with.overflow.i32") => {
            "{ i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)"
        }
        ("i64", "llvm.sadd.with.overflow.i64") => {
            "{ i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)"
        }
        ("i8", "llvm.uadd.with.overflow.i8") => "{ i8, i1 } @llvm.uadd.with.overflow.i8(i8, i8)",
        ("i16", "llvm.uadd.with.overflow.i16") => {
            "{ i16, i1 } @llvm.uadd.with.overflow.i16(i16, i16)"
        }
        ("i32", "llvm.uadd.with.overflow.i32") => {
            "{ i32, i1 } @llvm.uadd.with.overflow.i32(i32, i32)"
        }
        ("i64", "llvm.uadd.with.overflow.i64") => {
            "{ i64, i1 } @llvm.uadd.with.overflow.i64(i64, i64)"
        }
        ("i8", "llvm.ssub.with.overflow.i8") => "{ i8, i1 } @llvm.ssub.with.overflow.i8(i8, i8)",
        ("i16", "llvm.ssub.with.overflow.i16") => {
            "{ i16, i1 } @llvm.ssub.with.overflow.i16(i16, i16)"
        }
        ("i32", "llvm.ssub.with.overflow.i32") => {
            "{ i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)"
        }
        ("i64", "llvm.ssub.with.overflow.i64") => {
            "{ i64, i1 } @llvm.ssub.with.overflow.i64(i64, i64)"
        }
        ("i8", "llvm.usub.with.overflow.i8") => "{ i8, i1 } @llvm.usub.with.overflow.i8(i8, i8)",
        ("i16", "llvm.usub.with.overflow.i16") => {
            "{ i16, i1 } @llvm.usub.with.overflow.i16(i16, i16)"
        }
        ("i32", "llvm.usub.with.overflow.i32") => {
            "{ i32, i1 } @llvm.usub.with.overflow.i32(i32, i32)"
        }
        ("i64", "llvm.usub.with.overflow.i64") => {
            "{ i64, i1 } @llvm.usub.with.overflow.i64(i64, i64)"
        }
        ("i8", "llvm.smul.with.overflow.i8") => "{ i8, i1 } @llvm.smul.with.overflow.i8(i8, i8)",
        ("i16", "llvm.smul.with.overflow.i16") => {
            "{ i16, i1 } @llvm.smul.with.overflow.i16(i16, i16)"
        }
        ("i32", "llvm.smul.with.overflow.i32") => {
            "{ i32, i1 } @llvm.smul.with.overflow.i32(i32, i32)"
        }
        ("i64", "llvm.smul.with.overflow.i64") => {
            "{ i64, i1 } @llvm.smul.with.overflow.i64(i64, i64)"
        }
        ("i8", "llvm.umul.with.overflow.i8") => "{ i8, i1 } @llvm.umul.with.overflow.i8(i8, i8)",
        ("i16", "llvm.umul.with.overflow.i16") => {
            "{ i16, i1 } @llvm.umul.with.overflow.i16(i16, i16)"
        }
        ("i32", "llvm.umul.with.overflow.i32") => {
            "{ i32, i1 } @llvm.umul.with.overflow.i32(i32, i32)"
        }
        ("i64", "llvm.umul.with.overflow.i64") => {
            "{ i64, i1 } @llvm.umul.with.overflow.i64(i64, i64)"
        }
        _ => return None,
    })
}

pub(crate) fn integer_compare_opcode(operator: &str, ty: &Type) -> &'static str {
    match operator {
        "Less" => {
            if is_unsigned(ty) {
                "icmp ult"
            } else {
                "icmp slt"
            }
        }
        "LessEqual" => {
            if is_unsigned(ty) {
                "icmp ule"
            } else {
                "icmp sle"
            }
        }
        "Greater" => {
            if is_unsigned(ty) {
                "icmp ugt"
            } else {
                "icmp sgt"
            }
        }
        "GreaterEqual" => {
            if is_unsigned(ty) {
                "icmp uge"
            } else {
                "icmp sge"
            }
        }
        "Equal" | "Assign" => "icmp eq",
        "NotEqual" => "icmp ne",
        _ => unreachable!("validated integer comparison"),
    }
}

pub(crate) fn float_compare_opcode(operator: &str) -> &'static str {
    match operator {
        "Less" => "fcmp olt",
        "LessEqual" => "fcmp ole",
        "Greater" => "fcmp ogt",
        "GreaterEqual" => "fcmp oge",
        "Equal" | "Assign" => "fcmp oeq",
        "NotEqual" => "fcmp one",
        _ => unreachable!("validated float comparison"),
    }
}
