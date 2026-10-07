// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Typed LLVM instruction representations and rendering.

use std::fmt;

use super::{operands::LlvmOperand, types::LlvmType};

/// Binary arithmetic, bitwise and logical operators.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    UDiv,
    SDiv,
    URem,
    SRem,
    FAdd,
    FSub,
    FMul,
    FDiv,
    FRem,
    Shl,
    LShr,
    AShr,
    And,
    Or,
    Xor,
}

impl fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let op = match self {
            Self::Add => "add",
            Self::Sub => "sub",
            Self::Mul => "mul",
            Self::UDiv => "udiv",
            Self::SDiv => "sdiv",
            Self::URem => "urem",
            Self::SRem => "srem",
            Self::FAdd => "fadd",
            Self::FSub => "fsub",
            Self::FMul => "fmul",
            Self::FDiv => "fdiv",
            Self::FRem => "frem",
            Self::Shl => "shl",
            Self::LShr => "lshr",
            Self::AShr => "ashr",
            Self::And => "and",
            Self::Or => "or",
            Self::Xor => "xor",
        };
        write!(f, "{op}")
    }
}

/// Integer comparison predicates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ICmpCond {
    Eq,
    Ne,
    Ugt,
    Uge,
    Ult,
    Ule,
    Sgt,
    Sge,
    Slt,
    Sle,
}

impl fmt::Display for ICmpCond {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cond = match self {
            Self::Eq => "eq",
            Self::Ne => "ne",
            Self::Ugt => "ugt",
            Self::Uge => "uge",
            Self::Ult => "ult",
            Self::Ule => "ule",
            Self::Sgt => "sgt",
            Self::Sge => "sge",
            Self::Slt => "slt",
            Self::Sle => "sle",
        };
        write!(f, "{cond}")
    }
}

/// Floating point comparison predicates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FCmpCond {
    Oeq,
    Ogt,
    Oge,
    Olt,
    Ole,
    One,
    Ord,
    Ueq,
    Ugt,
    Uge,
    Ult,
    Ule,
    Une,
    Uno,
}

impl fmt::Display for FCmpCond {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cond = match self {
            Self::Oeq => "oeq",
            Self::Ogt => "ogt",
            Self::Oge => "oge",
            Self::Olt => "olt",
            Self::Ole => "ole",
            Self::One => "one",
            Self::Ord => "ord",
            Self::Ueq => "ueq",
            Self::Ugt => "ugt",
            Self::Uge => "uge",
            Self::Ult => "ult",
            Self::Ule => "ule",
            Self::Une => "une",
            Self::Uno => "uno",
        };
        write!(f, "{cond}")
    }
}

/// Type conversion and cast operators.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CastOp {
    Trunc,
    ZExt,
    SExt,
    FPToUI,
    FPToSI,
    UIToFP,
    SIToFP,
    FPTrunc,
    FPExt,
    PtrToInt,
    IntToPtr,
    BitCast,
}

impl fmt::Display for CastOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let op = match self {
            Self::Trunc => "trunc",
            Self::ZExt => "zext",
            Self::SExt => "sext",
            Self::FPToUI => "fptoui",
            Self::FPToSI => "fptosi",
            Self::UIToFP => "uitofp",
            Self::SIToFP => "sitofp",
            Self::FPTrunc => "fptrunc",
            Self::FPExt => "fpext",
            Self::PtrToInt => "ptrtoint",
            Self::IntToPtr => "inttoptr",
            Self::BitCast => "bitcast",
        };
        write!(f, "{op}")
    }
}

/// Strongly typed LLVM instructions.
#[derive(Clone, Debug, PartialEq)]
pub enum LlvmInst {
    Alloca {
        ty: LlvmType,
        align: Option<usize>,
    },
    Load {
        ty: LlvmType,
        ptr: LlvmOperand,
        align: Option<usize>,
    },
    Store {
        ty: LlvmType,
        val: LlvmOperand,
        ptr: LlvmOperand,
        align: Option<usize>,
    },
    Binary {
        op: BinaryOp,
        ty: LlvmType,
        lhs: LlvmOperand,
        rhs: LlvmOperand,
    },
    ICmp {
        cond: ICmpCond,
        ty: LlvmType,
        lhs: LlvmOperand,
        rhs: LlvmOperand,
    },
    FCmp {
        cond: FCmpCond,
        ty: LlvmType,
        lhs: LlvmOperand,
        rhs: LlvmOperand,
    },
    Cast {
        op: CastOp,
        from_ty: LlvmType,
        val: LlvmOperand,
        to_ty: LlvmType,
    },
    GetElementPtr {
        inbounds: bool,
        elem_ty: LlvmType,
        ptr: LlvmOperand,
        indices: Vec<(LlvmType, LlvmOperand)>,
    },
    ExtractValue {
        agg_ty: LlvmType,
        agg: LlvmOperand,
        indices: Vec<usize>,
    },
    InsertValue {
        agg_ty: LlvmType,
        agg: LlvmOperand,
        elem_ty: LlvmType,
        elem: LlvmOperand,
        indices: Vec<usize>,
    },
    Call {
        ret_ty: LlvmType,
        func: LlvmOperand,
        args: Vec<(LlvmType, LlvmOperand)>,
        tail: bool,
        /// The fixed parameters of a variadic callee (`printf`: `[ptr]`), which
        /// LLVM requires in the call as `call i32 (ptr, ...) @printf(...)`.
        variadic: Option<Vec<LlvmType>>,
    },
    Select {
        cond: LlvmOperand,
        true_val: (LlvmType, LlvmOperand),
        false_val: (LlvmType, LlvmOperand),
    },
    Br {
        dest: String,
    },
    CondBr {
        cond: LlvmOperand,
        true_dest: String,
        false_dest: String,
    },
    /// `phi ty [ value, %label ], ...`.
    Phi {
        ty: LlvmType,
        incoming: Vec<(LlvmOperand, String)>,
    },
    Ret {
        val: Option<(LlvmType, LlvmOperand)>,
    },
    Unreachable,
    Comment(String),
    Raw(String),
}

impl LlvmInst {
    /// Returns true if this is a block terminator.
    #[must_use]
    pub fn is_terminator(&self) -> bool {
        matches!(
            self,
            Self::Br { .. } | Self::CondBr { .. } | Self::Ret { .. } | Self::Unreachable
        )
    }
}

impl fmt::Display for LlvmInst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Alloca { ty, align } => {
                write!(f, "alloca {ty}")?;
                if let Some(a) = align {
                    write!(f, ", align {a}")?;
                }
                Ok(())
            }
            Self::Load { ty, ptr, align } => {
                write!(f, "load {ty}, ptr {ptr}")?;
                if let Some(a) = align {
                    write!(f, ", align {a}")?;
                }
                Ok(())
            }
            Self::Store {
                ty,
                val,
                ptr,
                align,
            } => {
                write!(f, "store {ty} {val}, ptr {ptr}")?;
                if let Some(a) = align {
                    write!(f, ", align {a}")?;
                }
                Ok(())
            }
            Self::Binary { op, ty, lhs, rhs } => {
                write!(f, "{op} {ty} {lhs}, {rhs}")
            }
            Self::ICmp { cond, ty, lhs, rhs } => {
                write!(f, "icmp {cond} {ty} {lhs}, {rhs}")
            }
            Self::FCmp { cond, ty, lhs, rhs } => {
                write!(f, "fcmp {cond} {ty} {lhs}, {rhs}")
            }
            Self::Cast {
                op,
                from_ty,
                val,
                to_ty,
            } => {
                write!(f, "{op} {from_ty} {val} to {to_ty}")
            }
            Self::GetElementPtr {
                inbounds,
                elem_ty,
                ptr,
                indices,
            } => {
                write!(
                    f,
                    "getelementptr {}{elem_ty}, ptr {ptr}",
                    if *inbounds { "inbounds " } else { "" }
                )?;
                for (idx_ty, idx) in indices {
                    write!(f, ", {idx_ty} {idx}")?;
                }
                Ok(())
            }
            Self::ExtractValue {
                agg_ty,
                agg,
                indices,
            } => {
                write!(f, "extractvalue {agg_ty} {agg}")?;
                for idx in indices {
                    write!(f, ", {idx}")?;
                }
                Ok(())
            }
            Self::InsertValue {
                agg_ty,
                agg,
                elem_ty,
                elem,
                indices,
            } => {
                write!(f, "insertvalue {agg_ty} {agg}, {elem_ty} {elem}")?;
                for idx in indices {
                    write!(f, ", {idx}")?;
                }
                Ok(())
            }
            Self::Call {
                ret_ty,
                func,
                args,
                tail,
                variadic,
            } => {
                if *tail {
                    write!(f, "tail ")?;
                }
                write!(f, "call {ret_ty} ")?;
                if let Some(fixed) = variadic {
                    write!(f, "(")?;
                    for parameter in fixed {
                        write!(f, "{parameter}, ")?;
                    }
                    write!(f, "...) ")?;
                }
                write!(f, "{func}(")?;
                for (i, (arg_ty, arg_val)) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{arg_ty} {arg_val}")?;
                }
                write!(f, ")")
            }
            Self::Select {
                cond,
                true_val,
                false_val,
            } => {
                write!(
                    f,
                    "select i1 {cond}, {} {}, {} {}",
                    true_val.0, true_val.1, false_val.0, false_val.1
                )
            }
            Self::Br { dest } => {
                let label = dest.strip_prefix('%').unwrap_or(dest);
                write!(f, "br label %{label}")
            }
            Self::CondBr {
                cond,
                true_dest,
                false_dest,
            } => {
                let t = true_dest.strip_prefix('%').unwrap_or(true_dest);
                let fl = false_dest.strip_prefix('%').unwrap_or(false_dest);
                write!(f, "br i1 {cond}, label %{t}, label %{fl}")
            }
            Self::Ret { val } => match val {
                Some((ty, v)) => write!(f, "ret {ty} {v}"),
                None => write!(f, "ret void"),
            },
            Self::Phi { ty, incoming } => {
                write!(f, "phi {ty} ")?;
                for (index, (value, label)) in incoming.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    let label = label.strip_prefix('%').unwrap_or(label);
                    write!(f, "[ {value}, %{label} ]")?;
                }
                Ok(())
            }
            Self::Unreachable => write!(f, "unreachable"),
            Self::Comment(c) => write!(f, "; {c}"),
            Self::Raw(r) => write!(f, "{r}"),
        }
    }
}

/// An instruction paired with an optional SSA destination register and debug metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct InstructionEntry {
    pub dest: Option<String>,
    pub inst: LlvmInst,
    pub dbg_loc: Option<u32>,
}

impl InstructionEntry {
    /// Creates an instruction entry without destination register.
    #[must_use]
    pub fn void(inst: LlvmInst) -> Self {
        Self {
            dest: None,
            inst,
            dbg_loc: None,
        }
    }

    /// Creates an instruction entry with destination SSA register.
    #[must_use]
    pub fn assign(dest: impl Into<String>, inst: LlvmInst) -> Self {
        Self {
            dest: Some(dest.into()),
            inst,
            dbg_loc: None,
        }
    }

    /// Attaches a DWARF debug location ID (`!dbg !{id}`).
    #[must_use]
    pub fn with_dbg(mut self, dbg_id: u32) -> Self {
        self.dbg_loc = Some(dbg_id);
        self
    }
}

impl fmt::Display for InstructionEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if matches!(self.inst, LlvmInst::Comment(_)) {
            return write!(f, "  {}", self.inst);
        }
        write!(f, "  ")?;
        if let Some(dest) = &self.dest {
            let reg = if dest.starts_with('%') {
                dest.as_str()
            } else {
                &format!("%{dest}")
            };
            write!(f, "{reg} = ")?;
        }
        write!(f, "{}", self.inst)?;
        if let Some(dbg_id) = self.dbg_loc {
            write!(f, ", !dbg !{dbg_id}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloca_and_load_store() {
        let a = InstructionEntry::assign(
            "x",
            LlvmInst::Alloca {
                ty: LlvmType::I32,
                align: Some(4),
            },
        );
        assert_eq!(a.to_string(), "  %x = alloca i32, align 4");

        let ld = InstructionEntry::assign(
            "val",
            LlvmInst::Load {
                ty: LlvmType::I32,
                ptr: LlvmOperand::reg("x"),
                align: Some(4),
            },
        );
        assert_eq!(ld.to_string(), "  %val = load i32, ptr %x, align 4");

        let st = InstructionEntry::void(LlvmInst::Store {
            ty: LlvmType::I32,
            val: LlvmOperand::int(42),
            ptr: LlvmOperand::reg("x"),
            align: Some(4),
        });
        assert_eq!(st.to_string(), "  store i32 42, ptr %x, align 4");
    }

    #[test]
    fn binary_and_compare() {
        let add = InstructionEntry::assign(
            "sum",
            LlvmInst::Binary {
                op: BinaryOp::Add,
                ty: LlvmType::I64,
                lhs: LlvmOperand::reg("a"),
                rhs: LlvmOperand::int(1),
            },
        );
        assert_eq!(add.to_string(), "  %sum = add i64 %a, 1");

        let cmp = InstructionEntry::assign(
            "cond",
            LlvmInst::ICmp {
                cond: ICmpCond::Sgt,
                ty: LlvmType::I32,
                lhs: LlvmOperand::reg("count"),
                rhs: LlvmOperand::int(0),
            },
        );
        assert_eq!(cmp.to_string(), "  %cond = icmp sgt i32 %count, 0");
    }

    #[test]
    fn call_instruction() {
        let call = InstructionEntry::assign(
            "res",
            LlvmInst::Call {
                ret_ty: LlvmType::I32,
                func: LlvmOperand::global("bn_rt_exit"),
                args: vec![(LlvmType::I32, LlvmOperand::int(0))],
                tail: false,
                variadic: None,
            },
        );
        assert_eq!(call.to_string(), "  %res = call i32 @bn_rt_exit(i32 0)");
    }

    #[test]
    fn aggregates_and_gep() {
        let extract = InstructionEntry::assign(
            "item",
            LlvmInst::ExtractValue {
                agg_ty: LlvmType::Struct(vec![LlvmType::I1, LlvmType::Double]),
                agg: LlvmOperand::reg("tuple"),
                indices: vec![1],
            },
        );
        assert_eq!(
            extract.to_string(),
            "  %item = extractvalue { i1, double } %tuple, 1"
        );

        let insert = InstructionEntry::assign(
            "tuple1",
            LlvmInst::InsertValue {
                agg_ty: LlvmType::Struct(vec![LlvmType::I1, LlvmType::Double]),
                agg: LlvmOperand::reg("tuple0"),
                elem_ty: LlvmType::I1,
                elem: LlvmOperand::bool(true),
                indices: vec![0],
            },
        );
        assert_eq!(
            insert.to_string(),
            "  %tuple1 = insertvalue { i1, double } %tuple0, i1 true, 0"
        );

        let gep = InstructionEntry::assign(
            "slot",
            LlvmInst::GetElementPtr {
                inbounds: true,
                elem_ty: LlvmType::I32,
                ptr: LlvmOperand::reg("base"),
                indices: vec![(LlvmType::I32, LlvmOperand::int(2))],
            },
        );
        assert_eq!(
            gep.to_string(),
            "  %slot = getelementptr inbounds i32, ptr %base, i32 2"
        );
    }

    #[test]
    fn control_flow_instructions() {
        let br = InstructionEntry::void(LlvmInst::Br {
            dest: "loop.exit".to_string(),
        });
        assert_eq!(br.to_string(), "  br label %loop.exit");

        let cond_br = InstructionEntry::void(LlvmInst::CondBr {
            cond: LlvmOperand::reg("c"),
            true_dest: "then".to_string(),
            false_dest: "else".to_string(),
        });
        assert_eq!(cond_br.to_string(), "  br i1 %c, label %then, label %else");

        let ret = InstructionEntry::void(LlvmInst::Ret {
            val: Some((LlvmType::I32, LlvmOperand::int(0))),
        });
        assert_eq!(ret.to_string(), "  ret i32 0");
    }

    #[test]
    fn debug_metadata_annotation() {
        let ret = InstructionEntry::void(LlvmInst::Ret {
            val: Some((LlvmType::I32, LlvmOperand::int(0))),
        })
        .with_dbg(42);
        assert_eq!(ret.to_string(), "  ret i32 0, !dbg !42");
    }
}
