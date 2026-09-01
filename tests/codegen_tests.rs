// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::fs;

use bn::{
    ir::{BasicBlock, BlockId, Constant, Function, Instruction, Module, Terminator, ValueId},
    llvm::lower_module,
    semantic::{FloatType, IntegerType, Type},
    source::{Position, Span},
};

fn span() -> Span {
    Span {
        start: Position {
            offset: 0,
            line: 1,
            column: 1,
        },
        end: Position {
            offset: 0,
            line: 1,
            column: 1,
        },
    }
}

fn start_module(blocks: Vec<BasicBlock>) -> Module {
    Module {
        source_name: Some("tests/codegen/manual.bn".into()),
        functions: vec![Function {
            name: "Start".into(),
            parameters: Vec::new(),
            return_type: Type::Named("VOID".into()),
            entry: BlockId(0),
            blocks,
            span: span(),
        }],
        ..Module::default()
    }
}

fn read_gold(path: &str) -> String {
    fs::read_to_string(path).expect("read gold fixture")
}

#[test]
fn lower_module_emits_contract_types_and_cfg() {
    let module = start_module(vec![
        BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Constant {
                    destination: ValueId(0),
                    value: Constant::Integer("1".into()),
                    ty: Type::Integer(IntegerType::Int32),
                    span: span(),
                },
                Instruction::Constant {
                    destination: ValueId(1),
                    value: Constant::Integer("2".into()),
                    ty: Type::Integer(IntegerType::Byte),
                    span: span(),
                },
                Instruction::Constant {
                    destination: ValueId(2),
                    value: Constant::Integer("3".into()),
                    ty: Type::Integer(IntegerType::UInt64),
                    span: span(),
                },
                Instruction::Constant {
                    destination: ValueId(3),
                    value: Constant::Float("1.5".into()),
                    ty: Type::Float(FloatType::Float32),
                    span: span(),
                },
                Instruction::Constant {
                    destination: ValueId(4),
                    value: Constant::Float("2.5".into()),
                    ty: Type::Float(FloatType::Float64),
                    span: span(),
                },
                Instruction::Constant {
                    destination: ValueId(5),
                    value: Constant::Boolean(true),
                    ty: Type::Boolean,
                    span: span(),
                },
            ],
            terminator: Terminator::Branch {
                condition: ValueId(5),
                then_block: BlockId(1),
                else_block: BlockId(2),
            },
        },
        BasicBlock {
            id: BlockId(1),
            instructions: Vec::new(),
            terminator: Terminator::Jump { target: BlockId(3) },
        },
        BasicBlock {
            id: BlockId(2),
            instructions: Vec::new(),
            terminator: Terminator::Jump { target: BlockId(3) },
        },
        BasicBlock {
            id: BlockId(3),
            instructions: Vec::new(),
            terminator: Terminator::Return { value: None },
        },
    ]);
    let llvm = lower_module(&module).expect("lower typed CFG");
    assert_eq!(llvm, read_gold("tests/gold/llvm_typed_cfg.ll"));
}

#[test]
fn lower_module_emits_i32_overflow_trap() {
    let module = start_module(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: ValueId(0),
                value: Constant::Integer(i32::MAX.to_string()),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
            Instruction::Constant {
                destination: ValueId(1),
                value: Constant::Integer("1".into()),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
            Instruction::Binary {
                destination: ValueId(2),
                operator: "Plus".into(),
                left: ValueId(0),
                right: ValueId(1),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
        ],
        terminator: Terminator::Stop { code: ValueId(2) },
    }]);
    let llvm = lower_module(&module).expect("lower i32 overflow");
    assert_eq!(llvm, read_gold("tests/gold/llvm_i32_overflow.ll"));
}

#[test]
fn lower_module_rejects_unsupported_dead_vector_path() {
    let module = start_module(vec![
        BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Constant {
                    destination: ValueId(0),
                    value: Constant::Boolean(true),
                    ty: Type::Boolean,
                    span: span(),
                },
                Instruction::Constant {
                    destination: ValueId(1),
                    value: Constant::Integer("7".into()),
                    ty: Type::Integer(IntegerType::Int32),
                    span: span(),
                },
            ],
            terminator: Terminator::Branch {
                condition: ValueId(0),
                then_block: BlockId(1),
                else_block: BlockId(2),
            },
        },
        BasicBlock {
            id: BlockId(1),
            instructions: vec![Instruction::Print {
                values: vec![ValueId(1)],
                span: span(),
            }],
            terminator: Terminator::Return { value: None },
        },
        BasicBlock {
            id: BlockId(2),
            instructions: vec![Instruction::Vector {
                destination: ValueId(2),
                values: vec![ValueId(1)],
                ty: Type::Vector {
                    element: Box::new(Type::Integer(IntegerType::Int32)),
                    dimensions: vec![1],
                },
                span: span(),
            }],
            terminator: Terminator::Return { value: None },
        },
    ]);
    let error = lower_module(&module).expect_err("vector lowering must be rejected");
    assert!(error.contains("BUILD_LOWERING_UNAVAILABLE"));
    assert!(error.contains("vectors"));
    assert!(error.contains("Start"));
    assert!(error.contains("1:1"));
}
