// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::HashSet;

use super::{Diagnostic, Module, Terminator, instruction_defines, instruction_uses, invalid_ir};

pub(super) fn validate(module: &Module) -> Result<(), Diagnostic> {
    for function in &module.functions {
        let block_count = u32::try_from(function.blocks.len())
            .map_err(|_| invalid_ir("function has too many basic blocks", function.span))?;
        if function.entry.0 >= block_count {
            return Err(invalid_ir(
                "function entry block does not exist",
                function.span,
            ));
        }
        let mut defined = HashSet::new();
        for (index, block) in function.blocks.iter().enumerate() {
            if block.id.0
                != u32::try_from(index)
                    .map_err(|_| invalid_ir("function has too many basic blocks", function.span))?
            {
                return Err(invalid_ir(
                    "basic block IDs must be dense and ordered",
                    function.span,
                ));
            }
            for instruction in &block.instructions {
                for used in instruction_uses(instruction) {
                    if !defined.contains(&used)
                        && !instruction_defines(instruction)
                            .is_some_and(|defined_id| defined_id == used)
                    {
                        return Err(invalid_ir(
                            "instruction uses a value that is not defined",
                            function.span,
                        ));
                    }
                }
                if let Some(destination) = instruction_defines(instruction) {
                    defined.insert(destination);
                }
            }
            match &block.terminator {
                Terminator::Jump { target } if target.0 >= block_count => {
                    return Err(invalid_ir(
                        "terminator references a basic block that does not exist",
                        function.span,
                    ));
                }
                Terminator::Branch {
                    then_block,
                    else_block,
                    condition,
                } => {
                    if then_block.0 >= block_count || else_block.0 >= block_count {
                        return Err(invalid_ir(
                            "terminator references a basic block that does not exist",
                            function.span,
                        ));
                    }
                    if !defined.contains(condition) {
                        return Err(invalid_ir("branch condition is not defined", function.span));
                    }
                }
                Terminator::Return { value: Some(value) } | Terminator::Stop { code: value }
                    if !defined.contains(value) =>
                {
                    return Err(invalid_ir("terminator value is not defined", function.span));
                }
                Terminator::Jump { .. } | Terminator::Return { .. } | Terminator::Stop { .. } => {}
            }
        }
    }
    Ok(())
}
