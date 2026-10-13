// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! LLVM function definitions and their basic blocks; a declaration is a
//! `Signature` (`abi.rs`).

use std::fmt;

use super::{block::BasicBlock, types::LlvmType};

/// An LLVM function definition.
#[derive(Clone, Debug, PartialEq)]
pub struct LlvmFunction {
    pub name: String,
    pub ret_ty: LlvmType,
    pub params: Vec<(LlvmType, String)>,
    pub blocks: Vec<BasicBlock>,
    pub linkage: Option<String>,
    pub attributes: Vec<String>,
}

impl LlvmFunction {
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
            linkage: None,
            attributes: Vec::new(),
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
        format!("{} {{", self.signature_line())
    }

    /// The header of a function described by the debug subprogram `dbg`:
    /// `define ... !dbg !N {`.
    #[must_use]
    pub fn header_with_dbg(&self, dbg: super::debug::MdRef) -> String {
        format!("{} !dbg {dbg} {{", self.signature_line())
    }

    /// `define <linkage><ret> @<name>(<params>) <attributes>`, without the brace.
    fn signature_line(&self) -> String {
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
        header
    }
}

impl fmt::Display for LlvmFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.header())?;
        for block in &self.blocks {
            write!(f, "{block}")?;
        }
        writeln!(f, "}}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{instructions::LlvmInst, operands::LlvmOperand};

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
}
