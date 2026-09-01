// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one using the MPL-2.0 license.

#[allow(clippy::wildcard_imports)]
use super::*;

impl Builder<'_> {
    pub(crate) fn symbol(&self, span: Span) -> Result<SymbolId, Diagnostic> {
        self.model
            .symbol_at(span)
            .map(|symbol| symbol.id)
            .ok_or_else(|| ir_error("declaration has no resolved SymbolId", span))
    }

    pub(crate) fn block(&mut self) -> BlockId {
        let id = BlockId(u32::try_from(self.blocks.len()).expect("IR block count fits u32"));
        self.blocks.push(OpenBlock {
            instructions: Vec::new(),
            terminator: None,
        });
        id
    }

    pub(crate) fn value(&mut self) -> ValueId {
        let id = ValueId(self.next_value);
        self.next_value += 1;
        id
    }

    pub(crate) fn emit(&mut self, instruction: Instruction) {
        self.blocks[self.current.0 as usize]
            .instructions
            .push(instruction);
    }

    pub(crate) fn terminate(&mut self, terminator: Terminator) {
        self.blocks[self.current.0 as usize].terminator = Some(terminator);
    }

    pub(crate) fn terminated(&self) -> bool {
        self.blocks[self.current.0 as usize].terminator.is_some()
    }

    pub(crate) fn jump_if_open(&mut self, target: BlockId) {
        if !self.terminated() {
            self.terminate(Terminator::Jump { target });
        }
    }

    pub(crate) fn finish(self) -> Result<Vec<BasicBlock>, Diagnostic> {
        self.blocks
            .into_iter()
            .enumerate()
            .map(|(index, block)| {
                Ok(BasicBlock {
                    id: BlockId(u32::try_from(index).expect("IR block count fits u32")),
                    instructions: block.instructions,
                    terminator: block.terminator.ok_or_else(|| {
                        ir_error("generated basic block has no terminator", default_span())
                    })?,
                })
            })
            .collect()
    }
}
