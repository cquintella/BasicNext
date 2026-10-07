// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Escape of a binding's content. A value loaded or moved out of a binding
//! that reaches a store to another binding, a field, a vector, a `RETURN`, a
//! call of a program function, or any instruction not known to only borrow
//! it, is a copy the binding cannot see: the binding must not reuse or free
//! storage it holds (the native `INPUT` line buffer, bucket
//! typed-llvm-emitter, defect of 2026-10-07).

use std::collections::{HashMap, HashSet};

use crate::{Constant, Function, Instruction, Module, SymbolId, Terminator, Type, ValueId};

/// The members of `symbols` whose content can outlive the binding.
#[must_use]
pub fn escaping_symbols<S: std::hash::BuildHasher>(
    module: &Module,
    function: &Function,
    symbols: &HashSet<SymbolId, S>,
) -> HashSet<SymbolId> {
    let instructions = || function.blocks.iter().flat_map(|block| &block.instructions);
    let callees = instructions()
        .filter_map(|instruction| match instruction {
            Instruction::Constant {
                destination,
                value: Constant::Function(name),
                ..
            } => Some((*destination, name.as_str())),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    // A call of a function with a body may keep its argument; a runtime or
    // HOST call only borrows it.
    let borrowing_call = |callee: &ValueId| {
        callees.get(callee).is_some_and(|name| {
            !module
                .functions
                .iter()
                .any(|function| function.name == *name && !function.blocks.is_empty())
        })
    };
    let mut origin = HashMap::<ValueId, HashSet<SymbolId>>::new();
    for instruction in instructions() {
        if let Instruction::Load {
            destination,
            symbol,
            ..
        }
        | Instruction::Take {
            destination,
            symbol,
            ..
        } = instruction
            && symbols.contains(symbol)
        {
            origin.entry(*destination).or_default().insert(*symbol);
        }
    }
    let size = |origin: &HashMap<ValueId, HashSet<SymbolId>>| {
        origin.values().map(HashSet::len).sum::<usize>()
    };
    let mut escaping = HashSet::new();
    loop {
        let known = (size(&origin), escaping.len());
        for instruction in instructions() {
            let sources = crate::instruction_uses(instruction)
                .iter()
                .filter_map(|value| origin.get(value))
                .flatten()
                .copied()
                .collect::<HashSet<_>>();
            if sources.is_empty() {
                continue;
            }
            let carried = match instruction {
                Instruction::Copy { destination, .. }
                | Instruction::Phi { destination, .. }
                | Instruction::Retain { destination, .. } => Some(*destination),
                Instruction::Cast {
                    destination, ty, ..
                } if carries_text(ty) => Some(*destination),
                _ => None,
            };
            let borrows = match instruction {
                Instruction::Store { symbol, .. } => sources.iter().all(|source| source == symbol),
                Instruction::Print { .. }
                | Instruction::Binary { .. }
                | Instruction::Unary { .. }
                | Instruction::Cast { .. }
                | Instruction::Length { .. }
                | Instruction::Index { .. }
                | Instruction::Release { .. } => true,
                Instruction::Call { callee, .. } => borrowing_call(callee),
                _ => false,
            };
            if let Some(destination) = carried {
                origin.entry(destination).or_default().extend(sources);
            } else if !borrows {
                escaping.extend(sources);
            }
        }
        for block in &function.blocks {
            if let Terminator::Return { value: Some(value) } = &block.terminator
                && let Some(symbols) = origin.get(value)
            {
                escaping.extend(symbols.iter().copied());
            }
        }
        if (size(&origin), escaping.len()) == known {
            return escaping;
        }
    }
}

fn carries_text(ty: &Type) -> bool {
    match ty {
        Type::String => true,
        Type::Alternative(members) => members.contains(&Type::String),
        _ => false,
    }
}

#[cfg(test)]
#[path = "escape_tests.rs"]
mod tests;
