// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Binary operators that are not constant-folded: integer, floating-point,
// boolean and string arithmetic and comparisons, including comparisons
// against an `INTEGER OR Error` payload.
#![allow(clippy::wildcard_imports, clippy::match_same_arms)]
use super::*;
use crate::{
    ir::{
        BinaryOp, CastOp, ICmpCond, InstSink, LlvmInst, LlvmOperand,
        LlvmType::{I1, I32, I64},
    },
    layout::{handle_result_ty, typed_llvm},
};
use runtime_abi::STR_EQ;

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
    if matches!(
        operator,
        "Less" | "LessEqual" | "Greater" | "GreaterEqual" | "Equal" | "Assign" | "NotEqual"
    ) && let Some(payload_ty) = integer_union_payload(left_ty)
        && integer_llvm(right_llvm)
    {
        emit_integer_union_compare(
            text,
            destination,
            operator,
            left,
            right,
            right_ty,
            payload_ty,
            true,
        );
        return;
    } else if matches!(
        operator,
        "Less" | "LessEqual" | "Greater" | "GreaterEqual" | "Equal" | "Assign" | "NotEqual"
    ) && let Some(payload_ty) = integer_union_payload(right_ty)
        && integer_llvm(left_llvm)
    {
        emit_integer_union_compare(
            text,
            destination,
            operator,
            right,
            left,
            left_ty,
            payload_ty,
            false,
        );
        return;
    }
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
    let int_op_llvm = if integer_llvm(result_llvm) || float_llvm(result_llvm) {
        result_llvm
    } else {
        left_llvm
    };
    match (operator, int_op_llvm) {
        ("Plus" | "Minus" | "Star" | "Multiply", "i8" | "i16" | "i32" | "i64")
            if (integer_llvm(left_llvm) || integer_union_payload(left_ty).is_some())
                && (integer_llvm(right_llvm) || integer_union_payload(right_ty).is_some()) =>
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
            if (integer_llvm(left_llvm) || integer_union_payload(left_ty).is_some())
                && (integer_llvm(right_llvm) || integer_union_payload(right_ty).is_some()) =>
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
            let left_op = LlvmOperand::raw(coerce_to_type(text, left, left_ty, ty));
            let right_op = LlvmOperand::raw(coerce_to_type(text, right, right_ty, ty));
            text.assign(
                format!("v{}", destination.0),
                LlvmInst::binary(
                    logic_op(operator),
                    typed_llvm(result_llvm),
                    left_op,
                    right_op,
                ),
            );
        }
        ("Equal" | "Assign" | "NotEqual", "ptr") => {
            text.assign(
                format!("streq{}", destination.0),
                STR_EQ.call([value_reg(left), value_reg(right)]),
            );
            let predicate = if matches!(operator, "Equal" | "Assign") {
                ICmpCond::Eq
            } else {
                ICmpCond::Ne
            };
            text.assign(
                format!("v{}", destination.0),
                LlvmInst::icmp(
                    predicate,
                    I32,
                    LlvmOperand::reg(format!("streq{}", destination.0)),
                    LlvmOperand::int(1),
                ),
            );
        }
        ("Plus" | "Minus" | "Star" | "Multiply" | "Slash" | "Divide", "float" | "double") => {
            let op = match operator {
                "Plus" => BinaryOp::FAdd,
                "Minus" => BinaryOp::FSub,
                "Star" | "Multiply" => BinaryOp::FMul,
                _ => BinaryOp::FDiv,
            };
            let left = LlvmOperand::raw(coerce_to_type(text, left, left_ty, ty));
            let right = LlvmOperand::raw(coerce_to_type(text, right, right_ty, ty));
            text.assign(
                format!("v{}", destination.0),
                LlvmInst::binary(op, typed_llvm(result_llvm), left, right),
            );
        }
        ("AND" | "OR" | "XOR", "i1") => text.assign(
            format!("v{}", destination.0),
            LlvmInst::binary(logic_op(operator), I1, value_reg(left), value_reg(right)),
        ),
        (
            "Less" | "LessEqual" | "Greater" | "GreaterEqual" | "Equal" | "Assign" | "NotEqual",
            "i8" | "i16" | "i32" | "i64",
        ) if integer_llvm(left_llvm) && integer_llvm(right_llvm) => {
            let cmp_ty = wider_integer_type(left_ty, right_ty);
            let left_op = LlvmOperand::raw(coerce_to_type(text, left, left_ty, cmp_ty));
            let right_op = LlvmOperand::raw(coerce_to_type(text, right, right_ty, cmp_ty));
            text.assign(
                format!("v{}", destination.0),
                LlvmInst::icmp(
                    integer_compare_cond(operator, cmp_ty),
                    typed_llvm(llvm_type(cmp_ty).expect("validated compare type")),
                    left_op,
                    right_op,
                ),
            );
        }
        (
            "Less" | "LessEqual" | "Greater" | "GreaterEqual" | "Equal" | "Assign" | "NotEqual",
            "float" | "double",
        ) => {
            text.assign(
                format!("v{}", destination.0),
                LlvmInst::fcmp(
                    float_compare_cond(operator),
                    typed_llvm(llvm_type(left_ty).expect("validated float compare type")),
                    value_reg(left),
                    value_reg(right),
                ),
            );
        }
        ("Equal" | "Assign" | "NotEqual", "i1") => text.assign(
            format!("v{}", destination.0),
            LlvmInst::icmp(
                integer_compare_cond(operator, &Type::Boolean),
                I1,
                value_reg(left),
                value_reg(right),
            ),
        ),
        _ => unreachable!("validated binary operator"),
    }
}

/// `and`, `or` or `xor` for the BN logical and bitwise operators.
fn logic_op(operator: &str) -> BinaryOp {
    match operator {
        "AND" => BinaryOp::And,
        "OR" => BinaryOp::Or,
        _ => BinaryOp::Xor,
    }
}

/// Compares the `i64` payload of an `INTEGER OR Error` with a scalar.
#[allow(clippy::too_many_arguments)]
fn emit_integer_union_compare(
    text: &mut String,
    destination: ValueId,
    operator: &str,
    union: ValueId,
    scalar: ValueId,
    scalar_ty: &Type,
    payload_ty: &Type,
    union_on_left: bool,
) {
    let dest = destination.0;
    text.assign(
        format!("unioncmp{dest}"),
        LlvmInst::extract(handle_result_ty(), value_reg(union), 2),
    );
    let payload = LlvmOperand::reg(format!("unioncmp{dest}"));
    let scalar_i64 = LlvmOperand::raw(coerce_to_type(
        text,
        scalar,
        scalar_ty,
        &Type::Integer(IntegerType::Int64),
    ));
    let (left, right) = if union_on_left {
        (payload, scalar_i64)
    } else {
        (scalar_i64, payload)
    };
    text.assign(
        format!("v{dest}"),
        LlvmInst::icmp(integer_compare_cond(operator, payload_ty), I64, left, right),
    );
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

/// Integer `/` with a floating-point result: both operands convert first.
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
    text.assign(
        format!("v{}", destination.0),
        LlvmInst::binary(BinaryOp::FDiv, typed_llvm(result_llvm), left_op, right_op),
    );
}

fn int_to_float(
    text: &mut String,
    value: ValueId,
    ty: &Type,
    result_llvm: &str,
    tag: &str,
) -> LlvmOperand {
    let llvm_ty = llvm_type(ty).expect("validated integer dividend type");
    let op = if is_unsigned(ty) {
        CastOp::UIToFP
    } else {
        CastOp::SIToFP
    };
    let temp = format!("{tag}{}", value.0);
    text.assign(
        temp.clone(),
        LlvmInst::cast(
            op,
            typed_llvm(llvm_ty),
            value_reg(value),
            typed_llvm(result_llvm),
        ),
    );
    LlvmOperand::reg(temp)
}
