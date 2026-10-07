// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The ownership balance rule (proposal `arc-shared-core-0.6.5`, `GC-IR`),
//! checked when a module uses explicit ownership. A value of an ARC kind is
//! *owned* when it carries one strong reference: `Allocate`, a call
//! returning an ARC value, `Retain`, `Take`, and a write's `previous` produce
//! one. Every owned value is consumed exactly once on every path — by
//! `Release`, by an ARC write (which takes the value it stores), or by
//! `RETURN` — and nothing reads it after that. A borrowed value (a `Load`,
//! a field, a parameter) is never consumed. `STOP` ends the process, so
//! values still owned there are not a leak.

use std::collections::{BTreeSet, HashMap, VecDeque};

use bn_types::Type;

use super::operands::{instruction_defines, instruction_type, instruction_uses};
use crate::{
    Diagnostic, Function, FunctionKind, Instruction, Module, Terminator, ValueId, invalid_ir,
};

/// Whether values of `ty` carry a strong reference: a class instance, a
/// pointer region, or an alternative with one of those members.
pub(super) fn is_arc_type(module: &Module, ty: &Type) -> bool {
    match ty {
        Type::Named(name) => is_declared(module, name),
        // A natively implemented library's type is a handle the library
        // manages (the same rule the lowering applies).
        Type::ImportedNamed { module: id, .. }
            if module.standard_library_of((*id).into()).is_some() =>
        {
            false
        }
        Type::ImportedNamed { module: id, name } => {
            is_declared(module, &format!("#{}.{name}", id.0))
        }
        Type::Pointer { .. } => true,
        Type::Vector { element, .. } => is_arc_type(module, element),
        Type::Alternative(members) => members.iter().any(|member| is_arc_type(module, member)),
        _ => false,
    }
}

/// A class (its `FieldInit`), an `INTERFACE` (it holds an object), or a
/// `STRUCT` (its `Default`). Every `STRUCT` counts, as in the lowering: one
/// without strong fields makes `Retain` and `Release` do nothing.
fn is_declared(module: &Module, name: &str) -> bool {
    module.interfaces.contains(name)
        || module
            .function_of_kind(FunctionKind::FieldInit, name)
            .is_some()
        || module
            .function_of_kind(FunctionKind::Default, name)
            .is_some()
}

/// The values an instruction consumes: ARC values (by their own type:
/// storing `NULL` moves no reference) that it releases, stores, or puts in a
/// vector. A store into a weak binding or a weak field consumes nothing.
fn consumed(
    module: &Module,
    function: &Function,
    instruction: &Instruction,
    value_types: &HashMap<ValueId, Type>,
) -> Vec<ValueId> {
    let candidates = match instruction {
        Instruction::Store { symbol, .. } if function.weak_symbols.contains(symbol) => {
            return Vec::new();
        }
        Instruction::SetMember {
            field: Some(field), ..
        } if module.field_is_weak(field) => return Vec::new(),
        Instruction::SetField {
            fields: Some(fields),
            ..
        } if fields
            .last()
            .is_some_and(|field| module.field_is_weak(field)) =>
        {
            return Vec::new();
        }
        Instruction::Vector { values, .. } => values.clone(),
        Instruction::Release { value, .. }
        | Instruction::Store { value, .. }
        | Instruction::SetIndex { value, .. }
        | Instruction::SetMemberIndex { value, .. }
        | Instruction::SetFieldIndex { value, .. }
        | Instruction::SetStaticIndex { value, .. }
        | Instruction::SetMember { value, .. }
        | Instruction::SetField { value, .. }
        | Instruction::StoreStatic { value, .. } => vec![*value],
        _ => return Vec::new(),
    };
    candidates
        .into_iter()
        .filter(|value| {
            value_types
                .get(value)
                .is_some_and(|ty| is_arc_type(module, ty))
        })
        .collect()
}

/// Whether the value an instruction defines is owned.
fn produces_owned(module: &Module, instruction: &Instruction) -> bool {
    match instruction {
        // `RELEASE` of a native handle moves it out with `Take` too, owning
        // nothing.
        Instruction::Allocate { ty, .. }
        | Instruction::Call { ty, .. }
        | Instruction::Vector { ty, .. }
        | Instruction::Default { ty, .. }
        | Instruction::Take { ty, .. }
        | Instruction::TakeMember { ty, .. } => is_arc_type(module, ty),
        Instruction::Retain { .. } => true,
        _ => {
            instruction_defines(instruction).is_some()
                && matches!(
                    instruction,
                    Instruction::Store { .. }
                        | Instruction::SetIndex { .. }
                        | Instruction::SetMemberIndex { .. }
                        | Instruction::SetFieldIndex { .. }
                        | Instruction::SetStaticIndex { .. }
                        | Instruction::SetMember { .. }
                        | Instruction::SetField { .. }
                        | Instruction::StoreStatic { .. }
                )
        }
    }
}

/// Checks every function of `module`.
pub(super) fn validate_ownership(module: &Module) -> Result<(), Diagnostic> {
    for function in &module.functions {
        validate_function(module, function)?;
    }
    Ok(())
}

/// Owned values alive at a program point, by id (ordered, so two states
/// compare deterministically).
type Owned = BTreeSet<u32>;

/// The state on entry to a block: the owned values alive, and the values
/// already consumed (none may be read again).
#[derive(Clone, Default)]
struct State {
    owned: Owned,
    consumed: BTreeSet<u32>,
}

fn validate_function(module: &Module, function: &Function) -> Result<(), Diagnostic> {
    let mut value_types = HashMap::new();
    for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
        if let (Some(value), Some(ty)) = (
            instruction_defines(instruction),
            instruction_type(instruction),
        ) {
            value_types.insert(value, ty.clone());
        }
    }
    let index_of = |id: u32| function.blocks.iter().position(|block| block.id.0 == id);
    let mut entry_state: HashMap<usize, State> = HashMap::new();
    let entry = index_of(function.entry.0)
        .ok_or_else(|| invalid_ir("entry block is missing", function.span))?;
    entry_state.insert(entry, State::default());
    // Breadth first, so every path reaches a join before the paths after it.
    let mut pending = VecDeque::from([entry]);
    let mut done = BTreeSet::new();
    while let Some(index) = pending.pop_front() {
        if !done.insert(index) {
            continue;
        }
        let block = &function.blocks[index];
        let State {
            mut owned,
            consumed: consumed_values,
        } = check_instructions(
            module,
            function,
            &block.instructions,
            &value_types,
            entry_state[&index].clone(),
        )?;
        match &block.terminator {
            Terminator::Return { value } => {
                if let Some(value) = value
                    && value_types
                        .get(value)
                        .is_some_and(|ty| is_arc_type(module, ty))
                    && !owned.remove(&value.0)
                {
                    return Err(invalid_ir(
                        "a returned ARC value must be owned (retain a borrowed value first)",
                        function.span,
                    ));
                }
                if !owned.is_empty() {
                    return Err(invalid_ir(
                        "an owned value is never consumed on a path that returns",
                        function.span,
                    ));
                }
            }
            Terminator::Stop { .. } => {}
            Terminator::Jump { target } => {
                let state = State {
                    owned,
                    consumed: consumed_values,
                };
                merge(
                    &mut entry_state,
                    &mut pending,
                    index_of(target.0),
                    &state,
                    function,
                )?;
            }
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                let state = State {
                    owned,
                    consumed: consumed_values,
                };
                merge(
                    &mut entry_state,
                    &mut pending,
                    index_of(then_block.0),
                    &state,
                    function,
                )?;
                merge(
                    &mut entry_state,
                    &mut pending,
                    index_of(else_block.0),
                    &state,
                    function,
                )?;
            }
        }
    }
    Ok(())
}

/// Applies one block's instructions to the ownership `state`.
fn check_instructions(
    module: &Module,
    function: &Function,
    instructions: &[Instruction],
    value_types: &HashMap<ValueId, Type>,
    state: State,
) -> Result<State, Diagnostic> {
    let State {
        mut owned,
        consumed: mut consumed_values,
    } = state;
    for instruction in instructions {
        let span = instruction.span();
        if let Instruction::Phi { ty, .. } = instruction
            && is_arc_type(module, ty)
        {
            return Err(invalid_ir(
                "a phi of ARC values is not supported under explicit ownership",
                span,
            ));
        }
        for used in instruction_uses(instruction) {
            if consumed_values.contains(&used.0) {
                return Err(invalid_ir(
                    "an owned value is used after it was consumed",
                    span,
                ));
            }
        }
        if let Instruction::Retain { value, ty, .. } = instruction
            && value_types.get(value).is_some_and(|actual| actual != ty)
        {
            return Err(invalid_ir(
                "retain type must match the retained value",
                span,
            ));
        }
        for value in consumed(module, function, instruction, value_types) {
            if !owned.remove(&value.0) {
                return Err(invalid_ir(
                    "a consumed value must be owned and not yet consumed (retain a borrowed value first)",
                    span,
                ));
            }
            consumed_values.insert(value.0);
        }
        if let Some(value) = instruction_defines(instruction)
            && produces_owned(module, instruction)
        {
            owned.insert(value.0);
        }
    }
    Ok(State {
        owned,
        consumed: consumed_values,
    })
}

/// Every path into a block must carry the same owned values. A value
/// consumed on any path stays unreadable after the join.
fn merge(
    entry_state: &mut HashMap<usize, State>,
    pending: &mut VecDeque<usize>,
    target: Option<usize>,
    state: &State,
    function: &Function,
) -> Result<(), Diagnostic> {
    let target = target.ok_or_else(|| invalid_ir("branch target is missing", function.span))?;
    match entry_state.get_mut(&target) {
        Some(existing) if existing.owned != state.owned => Err(invalid_ir(
            "paths that join hold different owned values",
            function.span,
        )),
        Some(existing) => {
            existing.consumed.extend(state.consumed.iter().copied());
            Ok(())
        }
        None => {
            entry_state.insert(target, state.clone());
            pending.push_back(target);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use bn_source::{Position, Span};
    use bn_types::Type;

    use super::validate_ownership;
    use crate::{
        BasicBlock, BlockId, Function, FunctionKind, Instruction, Module, SymbolId, Terminator,
        ValueId,
    };

    fn span() -> Span {
        let position = Position {
            source_id: Position::UNKNOWN_SOURCE,
            revision: Position::UNKNOWN_REVISION,
            offset: 0,
            line: 1,
            column: 1,
        };
        Span {
            start: position,
            end: position,
        }
    }

    fn function(kind: FunctionKind, owner: Option<&str>, blocks: Vec<BasicBlock>) -> Function {
        Function {
            name: owner.map_or_else(|| "Start".into(), |owner| format!("{owner}.$fields")),
            kind,
            owner: owner.map(str::to_owned),
            asynchronous: false,
            parameters: Vec::new(),
            weak_symbols: std::collections::HashSet::default(),
            return_type: Type::Named("VOID".into()),
            entry: BlockId(0),
            blocks,
            span: span(),
        }
    }

    fn block(id: u32, instructions: Vec<Instruction>, terminator: Terminator) -> BasicBlock {
        BasicBlock {
            id: BlockId(id),
            instructions,
            terminator,
        }
    }

    /// A module with class `Box` and an entry made of `blocks`.
    fn module(blocks: Vec<BasicBlock>) -> Module {
        let fields = block(0, Vec::new(), Terminator::Return { value: None });
        Module {
            functions: vec![
                function(FunctionKind::FieldInit, Some("Box"), vec![fields]),
                function(FunctionKind::Entry, None, blocks),
            ],
            ..Module::default()
        }
    }

    fn boxed() -> Type {
        Type::Named("Box".into())
    }

    fn allocate(destination: u32) -> Instruction {
        Instruction::Allocate {
            destination: ValueId(destination),
            type_name: "Box".into(),
            arguments: Vec::new(),
            ty: boxed(),
            span: span(),
        }
    }

    fn release(value: u32) -> Instruction {
        Instruction::Release {
            value: ValueId(value),
            destructor: None,
            span: span(),
        }
    }

    fn store(value: u32, previous: Option<u32>) -> Instruction {
        Instruction::Store {
            symbol: SymbolId(0),
            value: ValueId(value),
            previous: previous.map(ValueId),
            ty: boxed(),
            span: span(),
        }
    }

    fn take(destination: u32) -> Instruction {
        Instruction::Take {
            destination: ValueId(destination),
            symbol: SymbolId(0),
            ty: boxed(),
            span: span(),
        }
    }

    fn load(destination: u32) -> Instruction {
        Instruction::Load {
            destination: ValueId(destination),
            symbol: SymbolId(0),
            ty: boxed(),
            span: span(),
        }
    }

    fn returns() -> Terminator {
        Terminator::Return { value: None }
    }

    fn rejected(instructions: Vec<Instruction>) -> String {
        validate_ownership(&module(vec![block(0, instructions, returns())]))
            .expect_err("unbalanced ownership is invalid IR")
            .message
            .to_string()
    }

    #[test]
    fn a_new_object_stored_replaced_and_taken_is_balanced() {
        let entry = vec![
            allocate(0),
            store(0, None),
            allocate(1),
            store(1, Some(2)),
            release(2),
            take(3),
            release(3),
        ];
        validate_ownership(&module(vec![block(0, entry, returns())])).expect("balanced");
    }

    #[test]
    fn a_retained_borrow_may_be_stored() {
        let entry = vec![
            allocate(0),
            store(0, None),
            load(1),
            Instruction::Retain {
                destination: ValueId(2),
                value: ValueId(1),
                ty: boxed(),
                span: span(),
            },
            store(2, Some(3)),
            release(3),
            take(4),
            release(4),
        ];
        validate_ownership(&module(vec![block(0, entry, returns())])).expect("balanced");
    }

    #[test]
    fn an_owned_value_that_is_never_consumed_is_a_leak() {
        assert!(rejected(vec![allocate(0)]).contains("never consumed"));
    }

    #[test]
    fn an_owned_value_cannot_be_released_twice() {
        assert!(
            rejected(vec![allocate(0), release(0), release(0)])
                .contains("used after it was consumed")
        );
    }

    #[test]
    fn a_borrowed_value_is_never_consumed() {
        let message = rejected(vec![allocate(0), store(0, None), load(1), release(1)]);
        assert!(message.contains("must be owned"), "{message}");
    }

    #[test]
    fn a_borrowed_value_must_be_retained_before_it_is_stored() {
        let message = rejected(vec![
            allocate(0),
            store(0, None),
            load(1),
            store(1, Some(2)),
            release(2),
        ]);
        assert!(message.contains("must be owned"), "{message}");
    }

    #[test]
    fn nothing_reads_a_consumed_value() {
        let print = Instruction::Print {
            values: vec![ValueId(0)],
            span: span(),
        };
        assert!(
            rejected(vec![allocate(0), release(0), print]).contains("used after it was consumed")
        );
    }

    #[test]
    fn paths_that_join_must_hold_the_same_owned_values() {
        let condition = Instruction::Constant {
            destination: ValueId(1),
            value: crate::Constant::Boolean(true),
            ty: Type::Boolean,
            span: span(),
        };
        let blocks = vec![
            block(
                0,
                vec![allocate(0), condition],
                Terminator::Branch {
                    condition: ValueId(1),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                },
            ),
            block(1, vec![release(0)], Terminator::Jump { target: BlockId(3) }),
            block(2, Vec::new(), Terminator::Jump { target: BlockId(3) }),
            block(3, Vec::new(), returns()),
        ];
        let error = validate_ownership(&module(blocks)).expect_err("one path leaks");
        assert!(
            error.message.contains("different owned values"),
            "{}",
            error.message
        );
    }

    #[test]
    fn a_stop_may_end_the_process_with_owned_values() {
        let code = Instruction::Constant {
            destination: ValueId(1),
            value: crate::Constant::Integer("0".into()),
            ty: Type::Integer(bn_types::IntegerType::Int32),
            span: span(),
        };
        let blocks = vec![block(
            0,
            vec![allocate(0), code],
            Terminator::Stop { code: ValueId(1) },
        )];
        validate_ownership(&module(blocks)).expect("STOP ends the process");
    }
}
