// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Pure-Rust typed LLVM IR builder: types, operands, instructions with short
//! constructors and a buffer sink, typed C-ABI signatures, debug-information
//! metadata, and the canonical textual rendering.

pub(crate) mod abi;
pub mod block;
pub mod debug;
pub mod emit;
pub mod function;
pub(crate) mod global;
pub mod instructions;
pub mod module;
pub(crate) mod operands;
pub mod types;

pub(crate) use abi::{AbiType, Intrinsic, RuntimeFn, VariadicFn, write_declarations};
pub use abi::{Declaration, Signature};
pub use block::BasicBlock;
pub(crate) use debug::{DebugFormat, DebugMetadata, MdRef};
pub(crate) use emit::InstSink;
pub(crate) use function::LlvmFunction;
pub(crate) use global::{Linkage, LlvmGlobal};
pub(crate) use instructions::{BinaryOp, CastOp, FCmpCond, ICmpCond, LlvmInst};
pub(crate) use module::{DebugScopes, LlvmModule};
pub use operands::LlvmOperand;
pub(crate) use types::LlvmType;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_function_emission_pipeline() {
        let pair = LlvmType::struct_of([LlvmType::I32, LlvmType::I32]);
        let params = vec![
            (LlvmType::I32, "x".to_string()),
            (LlvmType::I32, "y".to_string()),
        ];
        let mut func = LlvmFunction::new_definition("min_max", pair.clone(), params);
        let entry = func.add_block("entry");
        let (x, y) = (LlvmOperand::reg("x"), LlvmOperand::reg("y"));
        let less = LlvmInst::icmp(ICmpCond::Slt, LlvmType::I32, x.clone(), y.clone());
        entry.push_assign("is_less", less);
        let branch = LlvmInst::cond_br(LlvmOperand::reg("is_less"), "x_smaller", "y_smaller");
        entry.terminate(branch);
        for (label, first, second, names) in [
            ("x_smaller", &x, &y, ["t1", "t2"]),
            ("y_smaller", &y, &x, ["t3", "t4"]),
        ] {
            let block = func.add_block(label);
            let insert = |agg, value: &LlvmOperand, index| {
                LlvmInst::insert(pair.clone(), agg, LlvmType::I32, value.clone(), index)
            };
            block.push_assign(names[0], insert(LlvmOperand::undef(), first, 0));
            block.push_assign(names[1], insert(LlvmOperand::reg(names[0]), second, 1));
            let result = Some((pair.clone(), LlvmOperand::reg(names[1])));
            block.terminate(LlvmInst::Ret { val: result });
        }

        let output = func.to_string();
        assert!(output.starts_with("define { i32, i32 } @min_max(i32 %x, i32 %y) {"));
        assert!(output.contains("entry:\n  %is_less = icmp slt i32 %x, %y\n  br i1 %is_less, label %x_smaller, label %y_smaller\n"));
        assert!(output.contains("x_smaller:\n  %t1 = insertvalue { i32, i32 } undef, i32 %x, 0\n  %t2 = insertvalue { i32, i32 } %t1, i32 %y, 1\n  ret { i32, i32 } %t2\n"));
        assert!(output.contains("y_smaller:\n  %t3 = insertvalue { i32, i32 } undef, i32 %y, 0\n  %t4 = insertvalue { i32, i32 } %t3, i32 %x, 1\n  ret { i32, i32 } %t4\n"));
        assert!(output.ends_with("}\n"));
    }
}
