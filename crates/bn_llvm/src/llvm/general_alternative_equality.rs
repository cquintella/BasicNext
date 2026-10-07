// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Equality comparison for general alternatives and their members.
#![allow(clippy::wildcard_imports)]
use super::*;
use crate::ir::{
    CastOp, FCmpCond, ICmpCond, InstSink, LlvmInst, LlvmOperand,
    LlvmType::{Double, I1, I32, I64, Ptr},
};
use bn_types::Type;
use general_alternative::{general_alternative, layout, pointer_member, stored_member, tag, wrap};

#[derive(Clone, Copy)]
pub(crate) struct EqualsOperands<'a> {
    pub(crate) destination: ValueId,
    pub(crate) left: ValueId,
    pub(crate) left_ty: &'a Type,
    pub(crate) members: &'a [Type],
    pub(crate) right: ValueId,
    pub(crate) right_ty: &'a Type,
    pub(crate) not_equal: bool,
}

/// `%v{destination} = left = right` (or `<>`), `left` a general alternative
/// and `right` a value of one of its members (or a literal): equal when the
/// alternative holds that member with an equal value.
pub(crate) fn emit_equals(text: &mut String, operands: EqualsOperands<'_>) {
    let dest = operands.destination.0;
    let reg = |suffix: &str| format!("geneq{dest}_{suffix}");
    let left_op = if general_alternative(operands.left_ty).is_some() {
        value_reg(operands.left)
    } else {
        wrap(
            text,
            &reg("left"),
            value_reg(operands.left),
            operands.left_ty,
            operands.members,
        )
    };
    let other = wrap(
        text,
        &reg("right"),
        value_reg(operands.right),
        operands.right_ty,
        operands.members,
    );
    let member = stored_member(operands.right_ty, operands.members);
    text.assign(
        reg("lefttag"),
        LlvmInst::extract(layout(), left_op.clone(), 0),
    );
    text.assign(
        reg("same"),
        LlvmInst::icmp(
            ICmpCond::Eq,
            I32,
            LlvmOperand::reg(reg("lefttag")),
            tag(member),
        ),
    );
    let values_equal = match member {
        // An object compares by identity: the same instance.
        _ if pointer_member(member) => {
            text.assign(reg("leftobj"), LlvmInst::extract(layout(), left_op, 1));
            text.assign(
                reg("rightobj"),
                LlvmInst::extract(layout(), other.clone(), 1),
            );
            LlvmInst::icmp(
                ICmpCond::Eq,
                Ptr,
                LlvmOperand::reg(reg("leftobj")),
                LlvmOperand::reg(reg("rightobj")),
            )
        }
        Type::String => {
            // Compare text only when the left side holds a STRING: otherwise
            // its pointer is not text, so compare the right side with itself
            // and let the tag decide.
            text.assign(reg("leftptr"), LlvmInst::extract(layout(), left_op, 1));
            text.assign(
                reg("rightptr"),
                LlvmInst::extract(layout(), other.clone(), 1),
            );
            text.assign(
                reg("text"),
                LlvmInst::select(
                    LlvmOperand::reg(reg("same")),
                    Ptr,
                    LlvmOperand::reg(reg("leftptr")),
                    LlvmOperand::reg(reg("rightptr")),
                ),
            );
            text.assign(
                reg("cmp"),
                runtime_abi::STR_EQ.call([
                    LlvmOperand::reg(reg("text")),
                    LlvmOperand::reg(reg("rightptr")),
                ]),
            );
            LlvmInst::icmp(
                ICmpCond::Ne,
                I32,
                LlvmOperand::reg(reg("cmp")),
                LlvmOperand::int(0),
            )
        }
        Type::Float(_) => {
            for (side, operand) in [("left", left_op), ("other", other.clone())] {
                text.assign(
                    reg(&format!("{side}bits")),
                    LlvmInst::extract(layout(), operand, 2),
                );
                text.assign(
                    reg(&format!("{side}double")),
                    LlvmInst::cast(
                        CastOp::BitCast,
                        I64,
                        LlvmOperand::reg(reg(&format!("{side}bits"))),
                        Double,
                    ),
                );
            }
            LlvmInst::fcmp(
                FCmpCond::Oeq,
                Double,
                LlvmOperand::reg(reg("leftdouble")),
                LlvmOperand::reg(reg("otherdouble")),
            )
        }
        // NULL, NA, EOF: the tag is the value.
        Type::Null | Type::NotAvailable | Type::EndOfFile => LlvmInst::icmp(
            ICmpCond::Eq,
            I32,
            LlvmOperand::reg(reg("lefttag")),
            tag(member),
        ),
        _ => {
            text.assign(reg("leftbits"), LlvmInst::extract(layout(), left_op, 2));
            text.assign(reg("otherbits"), LlvmInst::extract(layout(), other, 2));
            LlvmInst::icmp(
                ICmpCond::Eq,
                I64,
                LlvmOperand::reg(reg("leftbits")),
                LlvmOperand::reg(reg("otherbits")),
            )
        }
    };
    text.assign(reg("value"), values_equal);
    text.assign(
        reg("equal"),
        LlvmInst::binary(
            crate::ir::BinaryOp::And,
            I1,
            LlvmOperand::reg(reg("same")),
            LlvmOperand::reg(reg("value")),
        ),
    );
    // `<>` flips the result; `=` keeps it (`xor` with false).
    text.assign(
        format!("v{dest}"),
        LlvmInst::binary(
            crate::ir::BinaryOp::Xor,
            I1,
            LlvmOperand::reg(reg("equal")),
            LlvmOperand::bool(operands.not_equal),
        ),
    );
}
