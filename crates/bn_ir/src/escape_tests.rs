// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::HashSet;

use bn_source::{Position, Revision, SourceId, Span};

use super::escaping_symbols;
use crate::{
    BasicBlock, BlockId, Constant, Function, FunctionKind, Instruction, Module, SymbolId,
    Terminator, Type, ValueId,
};

fn span() -> Span {
    let position = Position {
        source_id: SourceId(1),
        revision: Revision(1),
        offset: 0,
        line: 1,
        column: 1,
    };
    Span {
        start: position,
        end: position,
    }
}

const LINE: SymbolId = SymbolId(0);
const OTHER: SymbolId = SymbolId(1);

fn load(destination: u32, symbol: SymbolId) -> Instruction {
    Instruction::Load {
        destination: ValueId(destination),
        symbol,
        ty: Type::String,
        span: span(),
    }
}

fn function(name: &str, instructions: Vec<Instruction>, returned: Option<u32>) -> Function {
    Function {
        name: name.into(),
        kind: FunctionKind::Entry,
        owner: None,
        asynchronous: false,
        parameters: Vec::new(),
        weak_symbols: HashSet::new(),
        return_type: Type::String,
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions,
            terminator: Terminator::Return {
                value: returned.map(ValueId),
            },
        }],
        span: span(),
    }
}

fn escaping(module_functions: Vec<Function>, function: &Function) -> bool {
    let module = Module {
        functions: module_functions,
        ..Module::default()
    };
    escaping_symbols(&module, function, &HashSet::from([LINE])).contains(&LINE)
}

fn call(callee: &str) -> Vec<Instruction> {
    vec![
        load(0, LINE),
        Instruction::Constant {
            destination: ValueId(1),
            value: Constant::Function(callee.into()),
            ty: Type::Unknown,
            span: span(),
        },
        Instruction::Call {
            destination: ValueId(2),
            callee: ValueId(1),
            arguments: vec![ValueId(0)],
            ty: Type::Named("VOID".into()),
            span: span(),
        },
    ]
}

#[test]
fn borrowing_uses_do_not_escape() {
    let print = function(
        "Start",
        vec![
            load(0, LINE),
            Instruction::Print {
                values: vec![ValueId(0)],
                span: span(),
            },
        ],
        None,
    );
    assert!(!escaping(Vec::new(), &print));
    // A runtime call (no body in the module) only borrows.
    assert!(!escaping(
        Vec::new(),
        &function("Start", call("HOST.Console.Write"), None)
    ));
}

#[test]
fn stores_returns_and_program_calls_escape() {
    let store = function(
        "Start",
        vec![
            load(0, LINE),
            Instruction::Store {
                symbol: OTHER,
                value: ValueId(0),
                previous: None,
                ty: Type::String,
                span: span(),
            },
        ],
        None,
    );
    assert!(escaping(Vec::new(), &store));
    assert!(escaping(
        Vec::new(),
        &function("Start", vec![load(0, LINE)], Some(0))
    ));
    let keep = function("Keep", vec![load(5, OTHER)], None);
    assert!(escaping(vec![keep], &function("Start", call("Keep"), None)));
}

#[test]
fn copies_carry_the_origin_to_their_uses() {
    let copied = function(
        "Start",
        vec![
            load(0, LINE),
            Instruction::Copy {
                destination: ValueId(1),
                source: ValueId(0),
                ty: Type::String,
                span: span(),
            },
        ],
        Some(1),
    );
    assert!(escaping(Vec::new(), &copied));
}
