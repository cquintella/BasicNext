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
            asynchronous: false,
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
fn float32_special_constants_use_float_width_hex_encoding() {
    let module = start_module(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: ValueId(0),
                value: Constant::Float("NAN".into()),
                ty: Type::Float(FloatType::Float32),
                span: span(),
            },
            Instruction::Constant {
                destination: ValueId(1),
                value: Constant::Float("INF".into()),
                ty: Type::Float(FloatType::Float32),
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let llvm = lower_module(&module).expect("lower FLOAT32 special constants");
    assert!(llvm.contains("fadd float 0.0, 0x7FC00000"), "{llvm}");
    assert!(llvm.contains("fadd float 0.0, 0x7F800000"), "{llvm}");
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
fn lower_module_folds_euclidean_div_and_remainder() {
    let module = start_module(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: ValueId(0),
                value: Constant::Integer("-5".into()),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
            Instruction::Constant {
                destination: ValueId(1),
                value: Constant::Integer("3".into()),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
            Instruction::Binary {
                destination: ValueId(2),
                operator: "DIV".into(),
                left: ValueId(0),
                right: ValueId(1),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
            Instruction::Constant {
                destination: ValueId(3),
                value: Constant::Integer("-5".into()),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
            Instruction::Constant {
                destination: ValueId(4),
                value: Constant::Integer("3".into()),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
            Instruction::Binary {
                destination: ValueId(5),
                operator: "Percent".into(),
                left: ValueId(3),
                right: ValueId(4),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
        ],
        terminator: Terminator::Stop { code: ValueId(2) },
    }]);
    let llvm = lower_module(&module).expect("fold euclidean DIV/%");
    assert!(llvm.contains("add i32 0, -2"), "{llvm}");
    assert!(llvm.contains("add i32 0, 1"), "{llvm}");
    assert!(!llvm.contains("sdiv"), "{llvm}");
}

#[test]
fn lower_module_emits_euclidean_div_for_non_constant_i32() {
    let module = start_module(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Default {
                destination: ValueId(0),
                ty: Type::Integer(IntegerType::Int32),
                dimensions: Vec::new(),
                dynamic_dimensions: Vec::new(),
                span: span(),
            },
            Instruction::Constant {
                destination: ValueId(1),
                value: Constant::Integer("3".into()),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
            Instruction::Binary {
                destination: ValueId(2),
                operator: "DIV".into(),
                left: ValueId(0),
                right: ValueId(1),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
        ],
        terminator: Terminator::Stop { code: ValueId(2) },
    }]);
    let llvm = lower_module(&module).expect("lower runtime DIV");
    assert!(llvm.contains("sdiv i32"), "{llvm}");
    assert!(llvm.contains("icmp eq i32 %v1, 0"), "{llvm}");
    assert!(llvm.contains("trap_numeric_overflow"), "{llvm}");
}

#[test]
fn lower_module_emits_unsigned_div_and_traps_min_div_neg_one() {
    let unsigned = start_module(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Default {
                destination: ValueId(0),
                ty: Type::Integer(IntegerType::Byte),
                dimensions: Vec::new(),
                dynamic_dimensions: Vec::new(),
                span: span(),
            },
            Instruction::Constant {
                destination: ValueId(1),
                value: Constant::Integer("3".into()),
                ty: Type::Integer(IntegerType::Byte),
                span: span(),
            },
            Instruction::Binary {
                destination: ValueId(2),
                operator: "Percent".into(),
                left: ValueId(0),
                right: ValueId(1),
                ty: Type::Integer(IntegerType::Byte),
                span: span(),
            },
        ],
        terminator: Terminator::Stop { code: ValueId(2) },
    }]);
    let unsigned_llvm = lower_module(&unsigned).expect("lower BYTE remainder");
    assert!(unsigned_llvm.contains("urem i8"), "{unsigned_llvm}");
    assert!(!unsigned_llvm.contains("srem"), "{unsigned_llvm}");

    let overflow = start_module(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: ValueId(0),
                value: Constant::Integer(i32::MIN.to_string()),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
            Instruction::Constant {
                destination: ValueId(1),
                value: Constant::Integer("-1".into()),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
            Instruction::Binary {
                destination: ValueId(2),
                operator: "DIV".into(),
                left: ValueId(0),
                right: ValueId(1),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
        ],
        terminator: Terminator::Stop { code: ValueId(2) },
    }]);
    let overflow_llvm = lower_module(&overflow).expect("lower INT32_MIN DIV -1");
    assert!(
        overflow_llvm.contains("trap_numeric_overflow"),
        "{overflow_llvm}"
    );
    assert!(
        overflow_llvm.contains("icmp eq i32 %v0, -2147483648"),
        "{overflow_llvm}"
    );
}

#[test]
fn lower_module_folds_int32_min_euclidean_remainder() {
    let module = start_module(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: ValueId(0),
                value: Constant::Integer(i32::MIN.to_string()),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
            Instruction::Constant {
                destination: ValueId(1),
                value: Constant::Integer("-1".into()),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
            Instruction::Binary {
                destination: ValueId(2),
                operator: "Percent".into(),
                left: ValueId(0),
                right: ValueId(1),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
        ],
        terminator: Terminator::Stop { code: ValueId(2) },
    }]);
    let llvm = lower_module(&module).expect("fold INT32_MIN % -1");
    assert!(llvm.contains("add i32 0, 0"), "{llvm}");
    assert!(!llvm.contains("srem"), "{llvm}");
}

#[test]
fn lower_module_folds_power_shift_and_integer_not() {
    let module = start_module(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: ValueId(0),
                value: Constant::Integer("2".into()),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
            Instruction::Constant {
                destination: ValueId(1),
                value: Constant::Integer("3".into()),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
            Instruction::Binary {
                destination: ValueId(2),
                operator: "Power".into(),
                left: ValueId(0),
                right: ValueId(1),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
            Instruction::Constant {
                destination: ValueId(3),
                value: Constant::Integer("1".into()),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
            Instruction::Constant {
                destination: ValueId(4),
                value: Constant::Integer("3".into()),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
            Instruction::Binary {
                destination: ValueId(5),
                operator: "SHL".into(),
                left: ValueId(3),
                right: ValueId(4),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
            Instruction::Constant {
                destination: ValueId(6),
                value: Constant::Integer("0".into()),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
            Instruction::Unary {
                destination: ValueId(7),
                operator: "NOT".into(),
                operand: ValueId(6),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
        ],
        terminator: Terminator::Stop { code: ValueId(2) },
    }]);
    let llvm = lower_module(&module).expect("fold power/shift/not");
    assert!(llvm.contains("add i32 0, 8"), "{llvm}");
    assert!(llvm.contains("add i32 0, -1"), "{llvm}");
}

#[test]
fn lower_module_casts_integer_to_boolean_and_traps_narrowing_overflow() {
    let boolean = start_module(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: ValueId(0),
                value: Constant::Integer("1".into()),
                ty: Type::Integer(IntegerType::Int32),
                span: span(),
            },
            Instruction::Cast {
                destination: ValueId(1),
                value: ValueId(0),
                ty: Type::Boolean,
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let llvm = lower_module(&boolean).expect("cast to BOOLEAN");
    assert!(
        llvm.contains("or i1 0, 1") || llvm.contains("icmp ne i32"),
        "{llvm}"
    );

    let overflow = start_module(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Default {
                destination: ValueId(0),
                ty: Type::Integer(IntegerType::Int32),
                dimensions: Vec::new(),
                dynamic_dimensions: Vec::new(),
                span: span(),
            },
            Instruction::Cast {
                destination: ValueId(1),
                value: ValueId(0),
                ty: Type::Integer(IntegerType::Byte),
                span: span(),
            },
        ],
        terminator: Terminator::Stop { code: ValueId(1) },
    }]);
    let overflow_llvm = lower_module(&overflow).expect("narrowing overflow check");
    assert!(
        overflow_llvm.contains("trap_numeric_overflow"),
        "{overflow_llvm}"
    );
}

#[test]
fn lower_module_emits_dead_vector_path() {
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
    let llvm = lower_module(&module).expect("vector lowering");
    assert!(llvm.contains("alloca [1 x i32]"));
    assert!(llvm.contains("insertvalue { ptr, i32 }"));
}

#[test]
fn lower_module_emits_bn_rt_clock_and_console_calls() {
    let clock = start_module(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: ValueId(0),
                value: Constant::Function("HOST.Clock.Now".into()),
                ty: Type::Function {
                    parameters: Vec::new(),
                    return_type: Box::new(Type::Integer(IntegerType::Int64)),
                },
                span: span(),
            },
            Instruction::Call {
                destination: ValueId(1),
                callee: ValueId(0),
                arguments: Vec::new(),
                ty: Type::Integer(IntegerType::Int64),
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let llvm = lower_module(&clock).expect("lower HOST.Clock.Now");
    assert!(llvm.contains("declare i64 @bn_rt_clock_now()"), "{llvm}");
    assert!(llvm.contains("call i64 @bn_rt_clock_now()"), "{llvm}");

    let cls = start_module(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: ValueId(0),
                value: Constant::HostConsole,
                ty: Type::HostConsole,
                span: span(),
            },
            Instruction::Constant {
                destination: ValueId(1),
                value: Constant::Function("HOST.Console.Cls".into()),
                ty: Type::Function {
                    parameters: Vec::new(),
                    return_type: Box::new(Type::Named("VOID".into())),
                },
                span: span(),
            },
            Instruction::Call {
                destination: ValueId(2),
                callee: ValueId(1),
                arguments: Vec::new(),
                ty: Type::Named("VOID".into()),
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let llvm = lower_module(&cls).expect("lower HOST.Console.Cls");
    assert!(llvm.contains("declare i32 @bn_rt_console_cls()"), "{llvm}");
    assert!(llvm.contains("call i32 @bn_rt_console_cls()"), "{llvm}");
    assert!(llvm.contains("trap_bn_rt"), "{llvm}");
}
