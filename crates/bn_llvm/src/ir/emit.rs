// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Short constructors for `LlvmInst` and the `InstSink` trait that writes
//! instructions straight into an emission buffer, so a typed instruction
//! takes no more lines than the `writeln!` it replaces.

use std::fmt::Write as _;

use super::{
    instructions::{BinaryOp, CastOp, FCmpCond, ICmpCond, InstructionEntry, LlvmInst},
    operands::LlvmOperand,
    types::LlvmType,
};

impl LlvmType {
    /// A literal struct type: `{ ptr, i32 }` is `struct_of([Ptr, I32])`.
    #[must_use]
    pub fn struct_of<const N: usize>(fields: [Self; N]) -> Self {
        Self::Struct(fields.into())
    }
}

impl LlvmInst {
    #[must_use]
    pub const fn alloca(ty: LlvmType) -> Self {
        Self::Alloca { ty, align: None }
    }

    #[must_use]
    pub const fn load(ty: LlvmType, ptr: LlvmOperand) -> Self {
        Self::Load {
            ty,
            ptr,
            align: None,
        }
    }

    #[must_use]
    pub const fn store(ty: LlvmType, val: LlvmOperand, ptr: LlvmOperand) -> Self {
        Self::Store {
            ty,
            val,
            ptr,
            align: None,
        }
    }

    /// A call of a variadic global (`printf`) with its `fixed` parameters.
    #[must_use]
    pub fn call_variadic(
        ret_ty: LlvmType,
        fixed: Vec<LlvmType>,
        func: &str,
        args: Vec<(LlvmType, LlvmOperand)>,
    ) -> Self {
        Self::Call {
            ret_ty,
            func: LlvmOperand::global(func),
            args,
            tail: false,
            variadic: Some(fixed),
        }
    }

    /// A direct call of global `@func`.
    #[must_use]
    pub fn call(ret_ty: LlvmType, func: &str, args: Vec<(LlvmType, LlvmOperand)>) -> Self {
        Self::Call {
            ret_ty,
            func: LlvmOperand::global(func),
            args,
            tail: false,
            variadic: None,
        }
    }

    /// A call through a function pointer or operand.
    #[must_use]
    pub fn call_operand(
        ret_ty: LlvmType,
        func: LlvmOperand,
        args: Vec<(LlvmType, LlvmOperand)>,
    ) -> Self {
        Self::Call {
            ret_ty,
            func,
            args,
            tail: false,
            variadic: None,
        }
    }

    #[must_use]
    pub const fn cast(op: CastOp, from_ty: LlvmType, val: LlvmOperand, to_ty: LlvmType) -> Self {
        Self::Cast {
            op,
            from_ty,
            val,
            to_ty,
        }
    }

    #[must_use]
    pub const fn icmp(cond: ICmpCond, ty: LlvmType, lhs: LlvmOperand, rhs: LlvmOperand) -> Self {
        Self::ICmp { cond, ty, lhs, rhs }
    }

    #[must_use]
    pub const fn fcmp(cond: FCmpCond, ty: LlvmType, lhs: LlvmOperand, rhs: LlvmOperand) -> Self {
        Self::FCmp { cond, ty, lhs, rhs }
    }

    #[must_use]
    pub const fn binary(op: BinaryOp, ty: LlvmType, lhs: LlvmOperand, rhs: LlvmOperand) -> Self {
        Self::Binary { op, ty, lhs, rhs }
    }

    /// `extractvalue` of one top-level field.
    #[must_use]
    pub fn extract(agg_ty: LlvmType, agg: LlvmOperand, index: usize) -> Self {
        Self::ExtractValue {
            agg_ty,
            agg,
            indices: vec![index],
        }
    }

    /// `insertvalue` of one top-level field.
    #[must_use]
    pub fn insert(
        agg_ty: LlvmType,
        agg: LlvmOperand,
        elem_ty: LlvmType,
        elem: LlvmOperand,
        index: usize,
    ) -> Self {
        Self::InsertValue {
            agg_ty,
            agg,
            elem_ty,
            elem,
            indices: vec![index],
        }
    }

    /// `getelementptr` (not `inbounds`) over `elem_ty` from `ptr`.
    #[must_use]
    pub const fn gep(
        elem_ty: LlvmType,
        ptr: LlvmOperand,
        indices: Vec<(LlvmType, LlvmOperand)>,
    ) -> Self {
        Self::GetElementPtr {
            inbounds: false,
            elem_ty,
            ptr,
            indices,
        }
    }

    /// `select i1 cond, ty a, ty b`: both arms share one type.
    #[must_use]
    pub fn select(cond: LlvmOperand, ty: LlvmType, a: LlvmOperand, b: LlvmOperand) -> Self {
        Self::Select {
            cond,
            true_val: (ty.clone(), a),
            false_val: (ty, b),
        }
    }

    /// `br label %dest`
    #[must_use]
    pub fn br(dest: impl Into<String>) -> Self {
        Self::Br { dest: dest.into() }
    }

    /// `br i1 cond, label %true_dest, label %false_dest`
    #[must_use]
    pub fn cond_br(
        cond: LlvmOperand,
        true_dest: impl Into<String>,
        false_dest: impl Into<String>,
    ) -> Self {
        Self::CondBr {
            cond,
            true_dest: true_dest.into(),
            false_dest: false_dest.into(),
        }
    }
}

/// A buffer that receives one rendered instruction per line.
pub trait InstSink {
    /// Writes `%dest = inst`.
    fn assign(&mut self, dest: impl Into<String>, inst: LlvmInst);
    /// Writes an instruction without a result.
    fn emit(&mut self, inst: LlvmInst);
    /// Starts the basic block `name`.
    fn label(&mut self, name: &str);
}

impl InstSink for String {
    fn assign(&mut self, dest: impl Into<String>, inst: LlvmInst) {
        let _ = writeln!(self, "{}", InstructionEntry::assign(dest, inst));
    }

    fn emit(&mut self, inst: LlvmInst) {
        let _ = writeln!(self, "{}", InstructionEntry::void(inst));
    }

    fn label(&mut self, name: &str) {
        let _ = writeln!(self, "{name}:");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use LlvmType::{I32, I64, Ptr};

    #[test]
    fn sink_renders_one_canonical_line_per_instruction() {
        let mut text = String::new();
        text.assign("out1", LlvmInst::alloca(I64));
        text.emit(LlvmInst::store(
            I64,
            LlvmOperand::int(0),
            LlvmOperand::reg("out1"),
        ));
        text.assign(
            "rc1",
            LlvmInst::call(
                I32,
                "bn_rt_f",
                vec![(Ptr, LlvmOperand::raw("%v3")), (I32, LlvmOperand::raw("7"))],
            ),
        );
        text.assign(
            "w1",
            LlvmInst::cast(CastOp::SExt, I32, LlvmOperand::reg("rc1"), I64),
        );
        text.assign(
            "p1",
            LlvmInst::extract(LlvmType::struct_of([Ptr, I32]), LlvmOperand::reg("v2"), 0),
        );
        text.assign(
            "s1",
            LlvmInst::select(
                LlvmOperand::reg("e1"),
                I64,
                LlvmOperand::reg("w1"),
                LlvmOperand::int(0),
            ),
        );
        assert_eq!(
            text,
            "  %out1 = alloca i64\n  \
             store i64 0, ptr %out1\n  \
             %rc1 = call i32 @bn_rt_f(ptr %v3, i32 7)\n  \
             %w1 = sext i32 %rc1 to i64\n  \
             %p1 = extractvalue { ptr, i32 } %v2, 0\n  \
             %s1 = select i1 %e1, i64 %w1, i64 0\n"
        );
    }
}
