// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Pure-Rust typed LLVM IR builder: types, operands, instructions with short
//! constructors and a buffer sink, typed C-ABI signatures, debug-information
//! metadata, and the canonical textual rendering.

pub mod abi;
pub mod block;
pub mod debug;
pub mod emit;
pub mod function;
pub mod instructions;
pub mod operands;
pub mod types;

pub use abi::{AbiType, Declaration, RuntimeFn};
pub use block::BasicBlock;
pub use debug::{DebugFormat, DebugMetadata, DebugNode, MdRef};
pub use emit::InstSink;
pub use function::LlvmFunction;
pub use instructions::{BinaryOp, CastOp, FCmpCond, ICmpCond, InstructionEntry, LlvmInst};
pub use operands::{LlvmOperand, escape_llvm};
pub use types::LlvmType;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_function_emission_pipeline() {
        let mut func = LlvmFunction::new_definition(
            "min_max",
            LlvmType::Struct(vec![LlvmType::I32, LlvmType::I32]),
            vec![
                (LlvmType::I32, "x".to_string()),
                (LlvmType::I32, "y".to_string()),
            ],
        );

        let entry = func.add_block("entry");
        let cmp = InstructionEntry::assign(
            "is_less",
            LlvmInst::ICmp {
                cond: ICmpCond::Slt,
                ty: LlvmType::I32,
                lhs: LlvmOperand::reg("x"),
                rhs: LlvmOperand::reg("y"),
            },
        );
        entry.push_entry(cmp);

        let cond_br = LlvmInst::CondBr {
            cond: LlvmOperand::reg("is_less"),
            true_dest: "x_smaller".to_string(),
            false_dest: "y_smaller".to_string(),
        };
        entry.terminate(cond_br);

        let x_smaller = func.add_block("x_smaller");
        let t1 = InstructionEntry::assign(
            "t1",
            LlvmInst::InsertValue {
                agg_ty: LlvmType::Struct(vec![LlvmType::I32, LlvmType::I32]),
                agg: LlvmOperand::undef(),
                elem_ty: LlvmType::I32,
                elem: LlvmOperand::reg("x"),
                indices: vec![0],
            },
        );
        let t2 = InstructionEntry::assign(
            "t2",
            LlvmInst::InsertValue {
                agg_ty: LlvmType::Struct(vec![LlvmType::I32, LlvmType::I32]),
                agg: LlvmOperand::reg("t1"),
                elem_ty: LlvmType::I32,
                elem: LlvmOperand::reg("y"),
                indices: vec![1],
            },
        );
        x_smaller.push_entry(t1);
        x_smaller.push_entry(t2);
        x_smaller.terminate(LlvmInst::Ret {
            val: Some((
                LlvmType::Struct(vec![LlvmType::I32, LlvmType::I32]),
                LlvmOperand::reg("t2"),
            )),
        });

        let y_smaller = func.add_block("y_smaller");
        let t3 = InstructionEntry::assign(
            "t3",
            LlvmInst::InsertValue {
                agg_ty: LlvmType::Struct(vec![LlvmType::I32, LlvmType::I32]),
                agg: LlvmOperand::undef(),
                elem_ty: LlvmType::I32,
                elem: LlvmOperand::reg("y"),
                indices: vec![0],
            },
        );
        let t4 = InstructionEntry::assign(
            "t4",
            LlvmInst::InsertValue {
                agg_ty: LlvmType::Struct(vec![LlvmType::I32, LlvmType::I32]),
                agg: LlvmOperand::reg("t3"),
                elem_ty: LlvmType::I32,
                elem: LlvmOperand::reg("x"),
                indices: vec![1],
            },
        );
        y_smaller.push_entry(t3);
        y_smaller.push_entry(t4);
        y_smaller.terminate(LlvmInst::Ret {
            val: Some((
                LlvmType::Struct(vec![LlvmType::I32, LlvmType::I32]),
                LlvmOperand::reg("t4"),
            )),
        });

        let output = func.to_string();
        assert!(output.starts_with("define { i32, i32 } @min_max(i32 %x, i32 %y) {"));
        assert!(output.contains("entry:\n  %is_less = icmp slt i32 %x, %y\n  br i1 %is_less, label %x_smaller, label %y_smaller\n"));
        assert!(output.contains("x_smaller:\n  %t1 = insertvalue { i32, i32 } undef, i32 %x, 0\n  %t2 = insertvalue { i32, i32 } %t1, i32 %y, 1\n  ret { i32, i32 } %t2\n"));
        assert!(output.contains("y_smaller:\n  %t3 = insertvalue { i32, i32 } undef, i32 %y, 0\n  %t4 = insertvalue { i32, i32 } %t3, i32 %x, 1\n  ret { i32, i32 } %t4\n"));
        assert!(output.ends_with("}\n"));
    }
}
