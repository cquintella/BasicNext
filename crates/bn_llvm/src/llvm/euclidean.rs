#![allow(clippy::wildcard_imports)]
use super::*;
use crate::ir::{
    BinaryOp, CastOp, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T,
};
use crate::layout::typed_llvm;

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_euclidean_integer_op(
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
    let llvm_str = llvm_type(ty).expect("validated integer type");
    let llvm_ty = typed_llvm(llvm_str);
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("div{name}{dest}"));
    let left_op = O::raw(coerce_to_type(text, left, left_ty, ty));
    let right_op = O::raw(coerce_to_type(text, right, right_ty, ty));
    let compare = |text: &mut String, name: &str, value: &O, constant: &str| {
        let inst = I::icmp(
            ICmpCond::Eq,
            llvm_ty.clone(),
            value.clone(),
            O::raw(constant),
        );
        text.assign(format!("div{name}{dest}"), inst);
    };
    let zero_ok = take_continuation(block_id, state);
    compare(text, "z", &right_op, "0");
    emit_trap(
        text,
        block_id,
        state,
        &format!("%divz{dest}"),
        zero_ok,
        bn_diag::DiagId::DIVISION_BY_ZERO,
        vec![(
            "operation",
            Fact::Text(bn_types::operator_spelling(operator).into()),
        )],
    );
    if is_unsigned(ty) {
        let op = match operator {
            "DIV" => BinaryOp::UDiv,
            "Percent" => BinaryOp::URem,
            _ => unreachable!("validated euclidean operator"),
        };
        text.assign(
            format!("v{dest}"),
            I::binary(op, llvm_ty, left_op, right_op),
        );
        return;
    }
    compare(text, "min", &left_op, signed_minimum(llvm_str));
    compare(text, "neg", &right_op, "-1");
    text.assign(
        format!("divovf{dest}"),
        I::binary(BinaryOp::And, T::I1, r("min"), r("neg")),
    );
    match operator {
        "DIV" => {
            // MIN DIV -1: the exact quotient is -MIN, as the interpreter reports.
            let ok = take_continuation(block_id, state);
            let exact = I::cast(CastOp::SExt, llvm_ty.clone(), left_op.clone(), T::I128);
            text.assign(format!("divexact{dest}"), exact);
            let negated = I::binary(BinaryOp::Sub, T::I128, O::int(0), r("exact"));
            text.assign(format!("divneg128{dest}"), negated);
            emit_overflow_trap(
                text,
                block_id,
                state,
                &format!("%divovf{dest}"),
                ok,
                &format!("%divneg128{dest}"),
                ty,
            );
            emit_signed_div(text, dest, &llvm_ty, left_op, right_op);
        }
        "Percent" => {
            let min_case = take_continuation(block_id, state);
            let ok = take_continuation(block_id, state);
            let join = take_continuation(block_id, state);
            text.emit(I::CondBr {
                cond: r("ovf"),
                true_dest: min_case.clone(),
                false_dest: ok.clone(),
            });
            state.control_flow.label(text, min_case.clone());
            text.emit(I::Br { dest: join.clone() });
            state.control_flow.label(text, ok.clone());
            emit_signed_rem(text, dest, &llvm_ty, left_op, right_op);
            text.emit(I::Br { dest: join.clone() });
            state.control_flow.label(text, join);
            let incoming = vec![(O::int(0), min_case), (O::reg(format!("eucl{dest}")), ok)];
            text.assign(
                format!("v{dest}"),
                I::Phi {
                    ty: llvm_ty,
                    incoming,
                },
            );
        }
        _ => unreachable!("validated euclidean operator"),
    }
}

fn emit_signed_div(text: &mut String, dest: u32, ty: &T, left: O, right: O) {
    let r = |name: &str| O::reg(format!("{name}{dest}"));
    let op = |op, a, b| I::binary(op, ty.clone(), a, b);
    text.assign(
        format!("qtrunc{dest}"),
        op(BinaryOp::SDiv, left.clone(), right.clone()),
    );
    text.assign(
        format!("rtrunc{dest}"),
        op(BinaryOp::SRem, left, right.clone()),
    );
    let negative = I::icmp(ICmpCond::Slt, ty.clone(), r("rtrunc"), O::int(0));
    text.assign(format!("rneg{dest}"), negative);
    text.assign(
        format!("rhspos{dest}"),
        I::icmp(ICmpCond::Sgt, ty.clone(), right, O::int(0)),
    );
    let adjust = I::select(r("rhspos"), ty.clone(), O::int(-1), O::int(1));
    text.assign(format!("qadj{dest}"), adjust);
    text.assign(
        format!("qfix{dest}"),
        op(BinaryOp::Add, r("qtrunc"), r("qadj")),
    );
    let quotient = I::select(r("rneg"), ty.clone(), r("qfix"), r("qtrunc"));
    text.assign(format!("v{dest}"), quotient);
}

fn emit_signed_rem(text: &mut String, dest: u32, ty: &T, left: O, right: O) {
    let r = |name: &str| O::reg(format!("{name}{dest}"));
    let op = |op, a, b| I::binary(op, ty.clone(), a, b);
    text.assign(
        format!("rtrunc{dest}"),
        op(BinaryOp::SRem, left, right.clone()),
    );
    let negative = I::icmp(ICmpCond::Slt, ty.clone(), r("rtrunc"), O::int(0));
    text.assign(format!("rneg{dest}"), negative);
    let divisor_negative = I::icmp(ICmpCond::Slt, ty.clone(), right.clone(), O::int(0));
    text.assign(format!("rhsneg{dest}"), divisor_negative);
    text.assign(
        format!("rsub{dest}"),
        op(BinaryOp::Sub, r("rtrunc"), right.clone()),
    );
    text.assign(format!("radd{dest}"), op(BinaryOp::Add, r("rtrunc"), right));
    let adjust = I::select(r("rhsneg"), ty.clone(), r("rsub"), r("radd"));
    text.assign(format!("radj{dest}"), adjust);
    let remainder = I::select(r("rneg"), ty.clone(), r("radj"), r("rtrunc"));
    text.assign(format!("eucl{dest}"), remainder);
}

fn signed_minimum(llvm_ty: &str) -> &'static str {
    match llvm_ty {
        "i8" => "-128",
        "i16" => "-32768",
        "i32" => "-2147483648",
        "i64" => "-9223372036854775808",
        _ => unreachable!("signed integer LLVM type"),
    }
}
