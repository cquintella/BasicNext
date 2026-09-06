use std::collections::HashSet;

use super::{Diagnostic, Module, Terminator, instruction_defines, instruction_uses, invalid_ir};

#[allow(clippy::too_many_lines)]
/// Enforces language-level IR well-formedness. Backend capability gaps are
/// target-support concerns and must be checked separately by `validate_for`;
/// they are not validation failures.
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
            validate_successor_bounds(block, block_count, function.span)?;
        }

        let entry = usize::try_from(function.entry.0)
            .map_err(|_| invalid_ir("function entry block does not fit", function.span))?;
        let mut successors = vec![Vec::new(); function.blocks.len()];
        let mut predecessors = vec![Vec::new(); function.blocks.len()];
        for (index, block) in function.blocks.iter().enumerate() {
            successors[index] = block_successors(&block.terminator);
            for &target in &successors[index] {
                let target = usize::try_from(target)
                    .map_err(|_| invalid_ir("block target does not fit", function.span))?;
                predecessors[target].push(index);
            }
        }

        let reachable = reachable_blocks(entry, &successors);
        let all_values = function
            .blocks
            .iter()
            .flat_map(|block| block.instructions.iter().filter_map(instruction_defines))
            .collect::<HashSet<_>>();
        let mut incoming = vec![HashSet::new(); function.blocks.len()];
        let mut outgoing = vec![HashSet::new(); function.blocks.len()];
        for index in 0..function.blocks.len() {
            if reachable[index] && index != entry {
                incoming[index].clone_from(&all_values);
                outgoing[index].clone_from(&all_values);
            }
        }

        loop {
            let mut changed = false;
            for index in 0..function.blocks.len() {
                if !reachable[index] {
                    continue;
                }
                let mut next_incoming = if index == entry {
                    HashSet::new()
                } else {
                    let mut paths = predecessors[index]
                        .iter()
                        .copied()
                        .filter(|predecessor| reachable[*predecessor]);
                    match paths.next() {
                        Some(first) => {
                            let mut intersection = outgoing[first].clone();
                            for predecessor in paths {
                                intersection.retain(|value| outgoing[predecessor].contains(value));
                            }
                            intersection
                        }
                        None => HashSet::new(),
                    }
                };
                if next_incoming != incoming[index] {
                    incoming[index].clone_from(&next_incoming);
                    changed = true;
                }
                for instruction in &function.blocks[index].instructions {
                    if let Some(destination) = instruction_defines(instruction) {
                        next_incoming.insert(destination);
                    }
                }
                if next_incoming != outgoing[index] {
                    outgoing[index] = next_incoming;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        for (index, block) in function.blocks.iter().enumerate() {
            if !reachable[index] {
                continue;
            }
            let mut defined = incoming[index].clone();
            for instruction in &block.instructions {
                for used in instruction_uses(instruction) {
                    if !defined.contains(&used)
                        && !instruction_defines(instruction)
                            .is_some_and(|defined_id| defined_id == used)
                    {
                        return Err(invalid_ir(
                            "instruction uses a value that is not defined on every executable path",
                            function.span,
                        ));
                    }
                }
                if let Some(destination) = instruction_defines(instruction) {
                    defined.insert(destination);
                }
            }
            match &block.terminator {
                Terminator::Branch { condition, .. } if !defined.contains(condition) => {
                    return Err(invalid_ir("branch condition is not defined", function.span));
                }
                Terminator::Return { value: Some(value) } | Terminator::Stop { code: value }
                    if !defined.contains(value) =>
                {
                    return Err(invalid_ir("terminator value is not defined", function.span));
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn validate_successor_bounds(
    block: &super::BasicBlock,
    block_count: u32,
    span: super::Span,
) -> Result<(), Diagnostic> {
    let targets = block_successors(&block.terminator);
    if targets.iter().any(|target| *target >= block_count) {
        return Err(invalid_ir(
            "terminator references a basic block that does not exist",
            span,
        ));
    }
    Ok(())
}

fn block_successors(terminator: &Terminator) -> Vec<u32> {
    match terminator {
        Terminator::Jump { target } => vec![target.0],
        Terminator::Branch {
            then_block,
            else_block,
            ..
        } => vec![then_block.0, else_block.0],
        Terminator::Return { .. } | Terminator::Stop { .. } => Vec::new(),
    }
}

fn reachable_blocks(entry: usize, successors: &[Vec<u32>]) -> Vec<bool> {
    let mut reachable = vec![false; successors.len()];
    let mut pending = vec![entry];
    while let Some(index) = pending.pop() {
        if reachable[index] {
            continue;
        }
        reachable[index] = true;
        pending.extend(
            successors[index]
                .iter()
                .filter_map(|target| usize::try_from(*target).ok()),
        );
    }
    reachable
}
