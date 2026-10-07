// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Representation and management of LLVM basic blocks.

use std::fmt;

use super::instructions::{InstructionEntry, LlvmInst};

/// An LLVM basic block comprising a label, sequential instructions, and a terminator.
#[derive(Clone, Debug, PartialEq)]
pub struct BasicBlock {
    pub label: String,
    pub instructions: Vec<InstructionEntry>,
}

impl BasicBlock {
    /// Creates a new basic block with the specified label name.
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        let raw = label.into();
        let cleaned = raw.strip_prefix('%').unwrap_or(&raw).to_string();
        Self {
            label: cleaned,
            instructions: Vec::new(),
        }
    }

    /// Pushes an instruction that does not produce a value.
    pub fn push(&mut self, inst: LlvmInst) {
        self.instructions.push(InstructionEntry::void(inst));
    }

    /// Pushes an instruction producing a value into the destination SSA register.
    pub fn push_assign(&mut self, dest: impl Into<String>, inst: LlvmInst) {
        self.instructions.push(InstructionEntry::assign(dest, inst));
    }

    /// Pushes an arbitrary instruction entry.
    pub fn push_entry(&mut self, entry: InstructionEntry) {
        self.instructions.push(entry);
    }

    /// Returns true if this basic block ends with a terminator instruction.
    #[must_use]
    pub fn is_terminated(&self) -> bool {
        self.instructions
            .last()
            .is_some_and(|e| e.inst.is_terminator())
    }

    /// Terminates the block with the specified instruction if not already terminated.
    pub fn terminate(&mut self, inst: LlvmInst) {
        if !self.is_terminated() {
            self.push(inst);
        }
    }
}

impl fmt::Display for BasicBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}:", self.label)?;
        for inst in &self.instructions {
            writeln!(f, "{inst}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{operands::LlvmOperand, types::LlvmType};

    #[test]
    fn basic_block_construction_and_display() {
        let mut bb = BasicBlock::new("entry");
        assert!(!bb.is_terminated());

        bb.push_assign(
            "ptr",
            LlvmInst::Alloca {
                ty: LlvmType::I32,
                align: Some(4),
            },
        );
        bb.push(LlvmInst::Store {
            ty: LlvmType::I32,
            val: LlvmOperand::int(10),
            ptr: LlvmOperand::reg("ptr"),
            align: Some(4),
        });
        bb.terminate(LlvmInst::Ret {
            val: Some((LlvmType::I32, LlvmOperand::int(0))),
        });

        assert!(bb.is_terminated());
        let expected = "\
entry:
  %ptr = alloca i32, align 4
  store i32 10, ptr %ptr, align 4
  ret i32 0
";
        assert_eq!(bb.to_string(), expected);
    }

    #[test]
    fn duplicate_terminate_ignored() {
        let mut bb = BasicBlock::new("block");
        bb.terminate(LlvmInst::Unreachable);
        bb.terminate(LlvmInst::Ret { val: None });
        assert_eq!(bb.instructions.len(), 1);
        assert_eq!(bb.instructions[0].inst, LlvmInst::Unreachable);
    }
}
