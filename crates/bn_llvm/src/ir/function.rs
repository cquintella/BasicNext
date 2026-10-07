// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Management of LLVM functions, basic blocks, and scoped SSA generation.

use std::fmt;

use super::{block::BasicBlock, operands::LlvmOperand, types::LlvmType};

/// An LLVM function representing either an external declaration or a full definition.
#[derive(Clone, Debug, PartialEq)]
pub struct LlvmFunction {
    pub name: String,
    pub ret_ty: LlvmType,
    pub params: Vec<(LlvmType, String)>,
    pub blocks: Vec<BasicBlock>,
    pub is_declaration: bool,
    pub linkage: Option<String>,
    pub attributes: Vec<String>,
    reg_counter: usize,
    block_counter: usize,
}

impl LlvmFunction {
    /// Creates a declaration prototype (`declare ...`).
    #[must_use]
    pub fn new_declaration(
        name: impl Into<String>,
        ret_ty: LlvmType,
        params: Vec<(LlvmType, String)>,
    ) -> Self {
        Self {
            name: name.into(),
            ret_ty,
            params,
            blocks: Vec::new(),
            is_declaration: true,
            linkage: None,
            attributes: Vec::new(),
            reg_counter: 0,
            block_counter: 0,
        }
    }

    /// Creates a function definition (`define ... { ... }`).
    #[must_use]
    pub fn new_definition(
        name: impl Into<String>,
        ret_ty: LlvmType,
        params: Vec<(LlvmType, String)>,
    ) -> Self {
        Self {
            name: name.into(),
            ret_ty,
            params,
            blocks: Vec::new(),
            is_declaration: false,
            linkage: None,
            attributes: Vec::new(),
            reg_counter: 0,
            block_counter: 0,
        }
    }

    /// Sets the linkage type (e.g. "internal", "private").
    pub fn set_linkage(&mut self, linkage: impl Into<String>) {
        self.linkage = Some(linkage.into());
    }

    /// Appends a function attribute (e.g. "nounwind").
    pub fn add_attribute(&mut self, attr: impl Into<String>) {
        self.attributes.push(attr.into());
    }

    /// Generates a unique SSA register operand scoped to this function.
    pub fn fresh_reg(&mut self) -> LlvmOperand {
        self.reg_counter += 1;
        LlvmOperand::reg(format!("t{}", self.reg_counter))
    }

    /// Generates a unique SSA register with a custom prefix.
    pub fn fresh_named_reg(&mut self, prefix: &str) -> LlvmOperand {
        self.reg_counter += 1;
        LlvmOperand::reg(format!("{prefix}{}", self.reg_counter))
    }

    /// Generates a unique block label name.
    pub fn fresh_block_label(&mut self, prefix: &str) -> String {
        self.block_counter += 1;
        format!("{prefix}.{}", self.block_counter)
    }

    /// Adds a new basic block with the specified label and returns a mutable reference to it.
    pub fn add_block(&mut self, label: impl Into<String>) -> &mut BasicBlock {
        let idx = self.blocks.len();
        self.blocks.push(BasicBlock::new(label));
        &mut self.blocks[idx]
    }

    /// Returns a mutable reference to the last basic block, if any.
    pub fn current_block_mut(&mut self) -> Option<&mut BasicBlock> {
        self.blocks.last_mut()
    }

    /// The `define ... {` line of a definition, without a newline, so a
    /// caller that streams the body can emit the header alone.
    #[must_use]
    pub fn header(&self) -> String {
        let sigil = if self.name.starts_with('@') { "" } else { "@" };
        let linkage = self
            .linkage
            .as_ref()
            .map_or_else(String::new, |link| format!("{link} "));
        let params = self
            .params
            .iter()
            .map(|(ty, name)| {
                let reg = if name.starts_with('%') { "" } else { "%" };
                format!("{ty} {reg}{name}")
            })
            .collect::<Vec<_>>()
            .join(", ");
        let mut header = format!(
            "define {linkage}{} {sigil}{}({params})",
            self.ret_ty, self.name
        );
        for attr in &self.attributes {
            header.push(' ');
            header.push_str(attr);
        }
        header.push_str(" {");
        header
    }
}

impl fmt::Display for LlvmFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let fn_name = if self.name.starts_with('@') {
            self.name.clone()
        } else {
            format!("@{}", self.name)
        };

        if self.is_declaration {
            write!(f, "declare {} {fn_name}(", self.ret_ty)?;
            for (i, (ty, _)) in self.params.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{ty}")?;
            }
            writeln!(f, ")")
        } else {
            writeln!(f, "{}", self.header())?;
            for block in &self.blocks {
                write!(f, "{block}")?;
            }
            writeln!(f, "}}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::instructions::LlvmInst;

    #[test]
    fn function_declaration_rendering() {
        let decl = LlvmFunction::new_declaration(
            "malloc",
            LlvmType::Ptr,
            vec![(LlvmType::I64, "size".to_string())],
        );
        assert_eq!(decl.to_string(), "declare ptr @malloc(i64)\n");
    }

    #[test]
    fn function_definition_rendering() {
        let mut def = LlvmFunction::new_definition(
            "add",
            LlvmType::I32,
            vec![
                (LlvmType::I32, "a".to_string()),
                (LlvmType::I32, "b".to_string()),
            ],
        );
        def.add_attribute("nounwind");

        let entry = def.add_block("entry");
        entry.push_assign(
            "res",
            LlvmInst::Binary {
                op: crate::ir::instructions::BinaryOp::Add,
                ty: LlvmType::I32,
                lhs: LlvmOperand::reg("a"),
                rhs: LlvmOperand::reg("b"),
            },
        );
        entry.terminate(LlvmInst::Ret {
            val: Some((LlvmType::I32, LlvmOperand::reg("res"))),
        });

        let expected = "\
define i32 @add(i32 %a, i32 %b) nounwind {
entry:
  %res = add i32 %a, %b
  ret i32 %res
}
";
        assert_eq!(def.to_string(), expected);
    }

    #[test]
    fn scoped_register_and_block_counter() {
        let mut f = LlvmFunction::new_definition("test", LlvmType::Void, vec![]);
        let r1 = f.fresh_reg();
        let r2 = f.fresh_reg();
        let named = f.fresh_named_reg("call");
        let b1 = f.fresh_block_label("then");
        let b2 = f.fresh_block_label("else");

        assert_eq!(r1.to_string(), "%t1");
        assert_eq!(r2.to_string(), "%t2");
        assert_eq!(named.to_string(), "%call3");
        assert_eq!(b1, "then.1");
        assert_eq!(b2, "else.2");
    }
}
