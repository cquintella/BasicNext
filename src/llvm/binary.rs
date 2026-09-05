#![allow(clippy::wildcard_imports, clippy::match_same_arms)]
use super::*;

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn emit_runtime_binary(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    operator: &str,
    left: ValueId,
    right: ValueId,
    left_ty: &Type,
    right_ty: &Type,
    ty: &Type,
    state: &mut EmissionState,
) {
    let left_llvm = llvm_type(left_ty).expect("validated binary LLVM type");
    let right_llvm = llvm_type(right_ty).expect("validated binary LLVM type");
    let result_llvm = llvm_type(ty).expect("validated binary result LLVM type");
    if matches!(operator, "Slash" | "Divide")
        && integer_llvm(left_llvm)
        && integer_llvm(right_llvm)
        && float_llvm(result_llvm)
    {
        emit_integer_float_div(
            text,
            destination,
            left,
            right,
            left_ty,
            right_ty,
            result_llvm,
        );
        return;
    }
    // Match on result width for integer ops so IntegerLiteral (i64) can feed INTEGER.
    let int_op_llvm = if integer_llvm(result_llvm) {
        result_llvm
    } else {
        left_llvm
    };
    match (operator, int_op_llvm) {
        ("Plus" | "Minus" | "Star" | "Multiply", "i8" | "i16" | "i32" | "i64")
            if integer_llvm(left_llvm) && integer_llvm(right_llvm) =>
        {
            emit_checked_integer_op(
                text,
                block_id,
                destination,
                operator,
                left,
                Some(right),
                left_ty,
                right_ty,
                ty,
                state,
            );
        }
        ("DIV" | "Percent", "i8" | "i16" | "i32" | "i64")
            if integer_llvm(left_llvm) && integer_llvm(right_llvm) =>
        {
            emit_euclidean_integer_op(
                text,
                block_id,
                destination,
                operator,
                left,
                right,
                left_ty,
                right_ty,
                ty,
                state,
            );
        }
        ("SHL" | "SHR", "i8" | "i16" | "i32" | "i64")
            if integer_llvm(left_llvm) && integer_llvm(right_llvm) =>
        {
            emit_shift(
                text,
                block_id,
                destination,
                operator,
                left,
                right,
                left_ty,
                right_ty,
                ty,
                state,
            );
        }
        ("Power", "i8" | "i16" | "i32" | "i64")
            if integer_llvm(left_llvm) && integer_llvm(right_llvm) =>
        {
            emit_integer_power(
                text,
                block_id,
                destination,
                left,
                right,
                left_ty,
                right_ty,
                ty,
                state,
            );
        }
        ("Power", "float" | "double") => {
            emit_float_power(text, destination, left, right, ty);
        }
        ("Plus", "ptr") => {
            emit_string_concat(text, destination, left, right);
        }
        ("AND" | "OR" | "XOR", "i8" | "i16" | "i32" | "i64")
            if integer_llvm(left_llvm) && integer_llvm(right_llvm) =>
        {
            let op = match operator {
                "AND" => "and",
                "OR" => "or",
                "XOR" => "xor",
                _ => unreachable!(),
            };
            let left_op = coerce_to_type(text, left, left_ty, ty);
            let right_op = coerce_to_type(text, right, right_ty, ty);
            let _ = writeln!(
                text,
                "  %v{} = {op} {result_llvm} {left_op}, {right_op}",
                destination.0
            );
        }
        ("Equal" | "Assign" | "NotEqual", "ptr") => {
            let _ = writeln!(
                text,
                "  %streq{} = call i32 @bn_rt_str_eq(ptr %v{}, ptr %v{})",
                destination.0, left.0, right.0
            );
            let predicate = if matches!(operator, "Equal" | "Assign") {
                "eq"
            } else {
                "ne"
            };
            let _ = writeln!(
                text,
                "  %v{} = icmp {predicate} i32 %streq{}, 1",
                destination.0, destination.0
            );
        }
        ("Plus", "float") => {
            let _ = writeln!(
                text,
                "  %v{} = fadd float %v{}, %v{}",
                destination.0, left.0, right.0
            );
        }
        ("Minus", "float") => {
            let _ = writeln!(
                text,
                "  %v{} = fsub float %v{}, %v{}",
                destination.0, left.0, right.0
            );
        }
        ("Star" | "Multiply", "float") => {
            let _ = writeln!(
                text,
                "  %v{} = fmul float %v{}, %v{}",
                destination.0, left.0, right.0
            );
        }
        ("Slash" | "Divide", "float") => {
            let _ = writeln!(
                text,
                "  %v{} = fdiv float %v{}, %v{}",
                destination.0, left.0, right.0
            );
        }
        ("Plus", "double") => {
            let _ = writeln!(
                text,
                "  %v{} = fadd double %v{}, %v{}",
                destination.0, left.0, right.0
            );
        }
        ("Minus", "double") => {
            let _ = writeln!(
                text,
                "  %v{} = fsub double %v{}, %v{}",
                destination.0, left.0, right.0
            );
        }
        ("Star" | "Multiply", "double") => {
            let _ = writeln!(
                text,
                "  %v{} = fmul double %v{}, %v{}",
                destination.0, left.0, right.0
            );
        }
        ("Slash" | "Divide", "double") => {
            let _ = writeln!(
                text,
                "  %v{} = fdiv double %v{}, %v{}",
                destination.0, left.0, right.0
            );
        }
        ("AND", "i1") => {
            let _ = writeln!(
                text,
                "  %v{} = and i1 %v{}, %v{}",
                destination.0, left.0, right.0
            );
        }
        ("OR", "i1") => {
            let _ = writeln!(
                text,
                "  %v{} = or i1 %v{}, %v{}",
                destination.0, left.0, right.0
            );
        }
        ("XOR", "i1") => {
            let _ = writeln!(
                text,
                "  %v{} = xor i1 %v{}, %v{}",
                destination.0, left.0, right.0
            );
        }
        (
            "Less" | "LessEqual" | "Greater" | "GreaterEqual" | "Equal" | "Assign" | "NotEqual",
            "i8" | "i16" | "i32" | "i64",
        ) if integer_llvm(left_llvm) && integer_llvm(right_llvm) => {
            let cmp_ty = wider_integer_type(left_ty, right_ty);
            let left_op = coerce_to_type(text, left, left_ty, cmp_ty);
            let right_op = coerce_to_type(text, right, right_ty, cmp_ty);
            let _ = writeln!(
                text,
                "  %v{} = {} {} {left_op}, {right_op}",
                destination.0,
                integer_compare_opcode(operator, cmp_ty),
                llvm_type(cmp_ty).expect("validated compare type")
            );
        }
        (
            "Less" | "LessEqual" | "Greater" | "GreaterEqual" | "Equal" | "Assign" | "NotEqual",
            "float" | "double",
        ) => {
            let _ = writeln!(
                text,
                "  %v{} = {} {} %v{}, %v{}",
                destination.0,
                float_compare_opcode(operator),
                llvm_type(left_ty).expect("validated float compare type"),
                left.0,
                right.0
            );
        }
        ("Equal" | "Assign", "i1") => {
            let _ = writeln!(
                text,
                "  %v{} = icmp eq i1 %v{}, %v{}",
                destination.0, left.0, right.0
            );
        }
        ("NotEqual", "i1") => {
            let _ = writeln!(
                text,
                "  %v{} = icmp ne i1 %v{}, %v{}",
                destination.0, left.0, right.0
            );
        }
        _ => unreachable!("validated binary operator"),
    }
}

fn wider_integer_type<'a>(left: &'a Type, right: &'a Type) -> &'a Type {
    let left_w = integer_llvm_bitwidth(left);
    let right_w = integer_llvm_bitwidth(right);
    if right_w > left_w { right } else { left }
}

fn integer_llvm_bitwidth(ty: &Type) -> u8 {
    match llvm_type(ty) {
        Some("i8") => 8,
        Some("i16") => 16,
        Some("i32") => 32,
        Some("i64") => 64,
        _ => 32,
    }
}

fn emit_integer_float_div(
    text: &mut String,
    destination: ValueId,
    left: ValueId,
    right: ValueId,
    left_ty: &Type,
    right_ty: &Type,
    result_llvm: &str,
) {
    let left_op = int_to_float(text, left, left_ty, result_llvm, "divl");
    let right_op = int_to_float(text, right, right_ty, result_llvm, "divr");
    let _ = writeln!(
        text,
        "  %v{} = fdiv {result_llvm} {left_op}, {right_op}",
        destination.0
    );
}

fn int_to_float(
    text: &mut String,
    value: ValueId,
    ty: &Type,
    result_llvm: &str,
    tag: &str,
) -> String {
    let llvm_ty = llvm_type(ty).expect("validated integer dividend type");
    let opcode = if is_unsigned(ty) { "uitofp" } else { "sitofp" };
    let temp = format!("{tag}{}", value.0);
    let _ = writeln!(
        text,
        "  %{temp} = {opcode} {llvm_ty} %v{} to {result_llvm}",
        value.0
    );
    format!("%{temp}")
}
