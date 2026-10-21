// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// The counted `FOR` continuation test. A step of 0 stops with
// `INVALID_FOR_STEP` (0.6.md, "Control flow"). Otherwise the loop continues
// while `current <= end` for a positive step and `current >= end` for a
// negative one.
#![allow(clippy::wildcard_imports)]
use super::*;
use crate::ir::{
    ICmpCond, InstSink, LlvmInst, LlvmOperand,
    LlvmType::{I1, I64},
};

pub(crate) fn lower_for_condition(
    text: &mut crate::ir::LlvmModule,
    block_id: BlockId,
    destination: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
    state: &mut EmissionState,
) {
    let current = arguments[0];
    let end = arguments[1];
    let step = arguments[2];
    let current_type = analysis.values.get(&current).unwrap_or_else(|| {
        panic!(
            "{}",
            bn_catalog::text("llvm", "msg.validated_for_current_type")
        )
    });
    let step_type = analysis.values.get(&step).unwrap_or_else(|| {
        panic!(
            "{}",
            bn_catalog::text("llvm", "msg.validated_for_step_type")
        )
    });
    let current_i64 = extend_to_i64(text, current, current_type);
    let end_i64 = extend_to_i64(
        text,
        end,
        analysis.values.get(&end).expect("validated FOR end type"),
    );
    let dest = destination.0;
    let reg = LlvmOperand::reg;
    let step_ty = llvm_type(step_type).unwrap_or_else(|| {
        panic!(
            "{}",
            bn_catalog::text("llvm", "msg.validated_for_step_type")
        )
    });
    text.assign(
        format!("for_step_zero{dest}"),
        LlvmInst::icmp(ICmpCond::Eq, step_ty, value_reg(step), LlvmOperand::int(0)),
    );
    let continues = take_continuation(block_id, state);
    emit_trap(
        text,
        block_id,
        state,
        &reg(format!("for_step_zero{dest}")),
        continues,
        bn_diag::DiagId::INVALID_FOR_STEP,
        vec![(
            "message",
            Fact::Text(bn_diag::trap_texts::FOR_STEP_ZERO.into()),
        )],
    );
    text.assign(
        format!("for_step_positive{dest}"),
        LlvmInst::icmp(
            integer_compare_cond("Greater", step_type),
            llvm_type(step_type).unwrap_or_else(|| {
                panic!(
                    "{}",
                    bn_catalog::text("llvm", "msg.validated_for_step_type")
                )
            }),
            value_reg(step),
            LlvmOperand::int(0),
        ),
    );
    text.assign(
        format!("for_ascending{dest}"),
        LlvmInst::icmp(
            integer_compare_cond("LessEqual", current_type),
            I64,
            current_i64.clone(),
            end_i64.clone(),
        ),
    );
    text.assign(
        format!("for_descending{dest}"),
        LlvmInst::icmp(
            integer_compare_cond("GreaterEqual", current_type),
            I64,
            current_i64,
            end_i64,
        ),
    );
    text.assign(
        format!("v{dest}"),
        LlvmInst::select(
            reg(format!("for_step_positive{dest}")),
            I1,
            reg(format!("for_ascending{dest}")),
            reg(format!("for_descending{dest}")),
        ),
    );
}

/// `$for_fits(current, step)`: `current + step` stays inside `current`'s type.
/// A counted `FOR` ends instead of raising `NUMERIC_OVERFLOW` (C3).
pub(crate) fn lower_for_fits(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    use crate::ir::{BinaryOp, ICmpCond, LlvmInst, LlvmType::I128};
    let current = arguments[0];
    let step = arguments[1];
    let current_type = analysis.values.get(&current).unwrap_or_else(|| {
        panic!(
            "{}",
            bn_catalog::text("llvm", "msg.validated_for_current_type")
        )
    });
    let step_type = analysis.values.get(&step).unwrap_or_else(|| {
        panic!(
            "{}",
            bn_catalog::text("llvm", "msg.validated_for_step_type")
        )
    });
    let dest = destination.0;
    let current_wide = extend_to_i128(text, current, current_type);
    let step_wide = extend_to_i128(text, step, step_type);
    text.assign(
        format!("for_sum{dest}"),
        LlvmInst::binary(BinaryOp::Add, I128, current_wide, step_wide),
    );
    let kind = integer_kind(current_type);
    let (minimum, maximum) = super::helpers::integer_range(kind);
    let sum = LlvmOperand::reg(format!("for_sum{dest}"));
    text.assign(
        format!("for_below{dest}"),
        LlvmInst::icmp(
            ICmpCond::Slt,
            I128,
            sum.clone(),
            LlvmOperand::int128(minimum),
        ),
    );
    text.assign(
        format!("for_above{dest}"),
        LlvmInst::icmp(ICmpCond::Sgt, I128, sum, LlvmOperand::int128(maximum)),
    );
    text.assign(
        format!("for_outside{dest}"),
        LlvmInst::binary(
            BinaryOp::Or,
            I1,
            LlvmOperand::reg(format!("for_below{dest}")),
            LlvmOperand::reg(format!("for_above{dest}")),
        ),
    );
    text.assign(
        format!("v{dest}"),
        LlvmInst::binary(
            BinaryOp::Xor,
            I1,
            LlvmOperand::reg(format!("for_outside{dest}")),
            LlvmOperand::bool(true),
        ),
    );
}

fn extend_to_i128(
    text: &mut crate::ir::LlvmModule,
    value: ValueId,
    ty: &Type,
) -> crate::ir::LlvmOperand {
    use crate::ir::{CastOp, LlvmInst, LlvmType::I128};
    let llvm_ty = llvm_type(ty)
        .unwrap_or_else(|| panic!("{}", bn_catalog::text("llvm", "msg.validated_for_integer")));
    if llvm_ty == I128 {
        return crate::ir::LlvmOperand::reg(format!("v{}", value.0));
    }
    let opcode = if is_unsigned(ty) {
        CastOp::ZExt
    } else {
        CastOp::SExt
    };
    let temp = format!("forwide{}_{}", value.0, text.fresh_tag());
    text.assign(
        temp.clone(),
        LlvmInst::cast(
            opcode,
            llvm_ty,
            crate::ir::LlvmOperand::reg(format!("v{}", value.0)),
            I128,
        ),
    );
    crate::ir::LlvmOperand::reg(temp)
}
