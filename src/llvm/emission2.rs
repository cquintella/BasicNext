#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn lower_terminator(
    text: &mut String,
    terminator: &Terminator,
    analysis: &LoweringAnalysis<'_>,
    _block_state: &mut BlockState,
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
            let _ = writeln!(
                text,
                "  br i1 %v{}, label %b{}, label %b{}",
                condition.0, then_block.0, else_block.0
            );
        }
        Terminator::Return { value: None } => {
            text.push_str("  ret i32 0\n");
        }
        Terminator::Return { value: Some(value) } | Terminator::Stop { code: value } => {
            let operand = coerce_return_operand(
                text,
                *value,
                analysis.values.get(value).expect("validated return type"),
            );
            let _ = writeln!(text, "  ret i32 {operand}");
        }
    }
}

pub(crate) fn lower_cast(
    text: &mut String,
    destination: ValueId,
    value: ValueId,
    source_ty: &Type,
    target_ty: &Type,
) {
    let source = llvm_type(source_ty).expect("validated cast source type");
    let target = llvm_type(target_ty).expect("validated cast target type");
    match (source, target) {
        ("i8" | "i16" | "i32" | "i64", "i8" | "i16" | "i32" | "i64") => {
            if source == target {
                let _ = writeln!(
                    text,
                    "  %v{} = add {target} 0, %v{}",
                    destination.0, value.0
                );
            } else if integer_width_from_llvm(source) < integer_width_from_llvm(target) {
                let opcode = if is_unsigned(source_ty) {
                    "zext"
                } else {
                    "sext"
                };
                let _ = writeln!(
                    text,
                    "  %v{} = {opcode} {source} %v{} to {target}",
                    destination.0, value.0
                );
            } else {
                let _ = writeln!(
                    text,
                    "  %v{} = trunc {source} %v{} to {target}",
                    destination.0, value.0
                );
            }
        }
        ("i8" | "i16" | "i32" | "i64", "float" | "double") => {
            let opcode = if is_unsigned(source_ty) {
                "uitofp"
            } else {
                "sitofp"
            };
            let _ = writeln!(
                text,
                "  %v{} = {opcode} {source} %v{} to {target}",
                destination.0, value.0
            );
        }
        ("float" | "double", "i8" | "i16" | "i32" | "i64") => {
            let opcode = if is_unsigned(target_ty) {
                "fptoui"
            } else {
                "fptosi"
            };
            let _ = writeln!(
                text,
                "  %v{} = {opcode} {source} %v{} to {target}",
                destination.0, value.0
            );
        }
        ("float", "double") => {
            let _ = writeln!(
                text,
                "  %v{} = fpext float %v{} to double",
                destination.0, value.0
            );
        }
        ("double", "float") => {
            let _ = writeln!(
                text,
                "  %v{} = fptrunc double %v{} to float",
                destination.0, value.0
            );
        }
        ("i1", "i1") => {
            let _ = writeln!(text, "  %v{} = or i1 false, %v{}", destination.0, value.0);
        }
        ("ptr", "ptr") => {
            let _ = writeln!(
                text,
                "  %v{} = getelementptr i8, ptr %v{}, i64 0",
                destination.0, value.0
            );
        }
        ("float", "float") => {
            let _ = writeln!(
                text,
                "  %v{} = fadd float 0.0, %v{}",
                destination.0, value.0
            );
        }
        ("double", "double") => {
            let _ = writeln!(
                text,
                "  %v{} = fadd double 0.0, %v{}",
                destination.0, value.0
            );
        }
        _ => unreachable!("validated cast shape"),
    }
}

pub(crate) fn lower_print_value(
    text: &mut String,
    value: ValueId,
    ty: &Type,
    state: &mut EmissionState,
) {
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
            let _ = writeln!(
                text,
                "  %print{} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_int, i64 %v{})",
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
                "  %print{} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_float, double %printfloat{})",
                state.print_count, state.print_count
            );
        }
        "double" => {
            let _ = writeln!(
                text,
                "  %print{} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_float, double %v{})",
                state.print_count, value.0
            );
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_checked_integer_op(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    operator: &str,
    left: ValueId,
    right: Option<ValueId>,
    ty: &Type,
    state: &mut EmissionState,
) {
    let intrinsic = checked_intrinsic_name(ty, operator).expect("validated checked intrinsic");
    let llvm_ty = llvm_type(ty).expect("validated integer type");
    let (left_operand, right_operand) = if operator == "Minus" && right.is_none() {
        ("0".into(), format!("%v{}", left.0))
    } else {
        (
            format!("%v{}", left.0),
            format!("%v{}", right.expect("binary op right").0),
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
    let _ = writeln!(text, "{continuation}:");
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
