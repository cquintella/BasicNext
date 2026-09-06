use std::io::Cursor;

use bn::{
    ir::{
        BasicBlock, BlockId, Constant, Function, Instruction, Module, Terminator,
        lower_graph_validated, validate_module,
    },
    llvm::lower_validated_module_for_target,
    module_graph::load,
    runtime::{HostEnv, execute_validated_with_host},
    semantic::analyze_modules,
};

fn span() -> bn::source::Span {
    let position = bn::source::Position {
        offset: 0,
        line: 1,
        column: 1,
    };
    bn::source::Span {
        start: position,
        end: position,
    }
}

fn malformed_module() -> Module {
    Module {
        functions: vec![Function {
            name: "Start".into(),
            asynchronous: false,
            parameters: Vec::new(),
            return_type: bn::semantic::Type::Named("VOID".into()),
            entry: BlockId(0),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: Vec::new(),
                terminator: Terminator::Jump { target: BlockId(9) },
            }],
            span: span(),
        }],
        ..Module::default()
    }
}

#[test]
fn invalid_module_cannot_become_validated_ir() {
    let error = validate_module(malformed_module()).expect_err("invalid target");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
fn both_backends_accept_the_same_validated_artifact() {
    let graph = load(std::path::Path::new("examples/hello.bn")).expect("load example");
    let models = analyze_modules(&graph).expect("analyze example");
    let validated = lower_graph_validated(&graph, &models).expect("validate example");

    let llvm = lower_validated_module_for_target(&validated, true).expect("emit LLVM");
    assert!(llvm.contains("define"));

    let mut input = Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();
    let code = execute_validated_with_host(
        &validated,
        &mut input,
        &mut output,
        &HostEnv::system(vec!["hello.bn".into()]),
    )
    .expect("interpret validated IR");
    assert_eq!(code, 0);
    assert!(!output.is_empty());
}

fn function_with_blocks(blocks: Vec<BasicBlock>) -> Module {
    Module {
        functions: vec![Function {
            name: "Start".into(),
            asynchronous: false,
            parameters: Vec::new(),
            return_type: bn::semantic::Type::Named("VOID".into()),
            entry: BlockId(0),
            blocks,
            span: span(),
        }],
        ..Module::default()
    }
}

#[test]
fn validator_rejects_value_defined_on_only_one_branch() {
    let module = function_with_blocks(vec![
        BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::Boolean(true),
                ty: bn::semantic::Type::Boolean,
                span: span(),
            }],
            terminator: Terminator::Branch {
                condition: bn::ir::ValueId(0),
                then_block: BlockId(1),
                else_block: BlockId(2),
            },
        },
        BasicBlock {
            id: BlockId(1),
            instructions: vec![Instruction::Constant {
                destination: bn::ir::ValueId(1),
                value: Constant::Integer("1".into()),
                ty: bn::semantic::Type::Integer(bn::semantic::IntegerType::Int32),
                span: span(),
            }],
            terminator: Terminator::Jump { target: BlockId(3) },
        },
        BasicBlock {
            id: BlockId(2),
            instructions: Vec::new(),
            terminator: Terminator::Jump { target: BlockId(3) },
        },
        BasicBlock {
            id: BlockId(3),
            instructions: vec![Instruction::Print {
                values: vec![bn::ir::ValueId(1)],
                span: span(),
            }],
            terminator: Terminator::Return { value: None },
        },
    ]);
    let error = validate_module(module).expect_err("join must require all-path definition");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
fn validator_rejects_undefined_input_prompt() {
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![Instruction::Input {
            destination: bn::ir::ValueId(0),
            prompt: Some(bn::ir::ValueId(9)),
            ty: bn::semantic::Type::String,
            span: span(),
        }],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("prompt must be defined");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
fn validator_rejects_undefined_dynamic_dimension() {
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![Instruction::Default {
            destination: bn::ir::ValueId(0),
            ty: bn::semantic::Type::String,
            dimensions: Vec::new(),
            dynamic_dimensions: vec![bn::ir::ValueId(9)],
            span: span(),
        }],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("dynamic dimension must be defined");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
fn validator_accepts_all_path_definitions_and_loop_reuse() {
    let module = function_with_blocks(vec![
        BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::Boolean(true),
                ty: bn::semantic::Type::Boolean,
                span: span(),
            }],
            terminator: Terminator::Branch {
                condition: bn::ir::ValueId(0),
                then_block: BlockId(1),
                else_block: BlockId(2),
            },
        },
        BasicBlock {
            id: BlockId(1),
            instructions: vec![Instruction::Constant {
                destination: bn::ir::ValueId(1),
                value: Constant::Integer("1".into()),
                ty: bn::semantic::Type::Integer(bn::semantic::IntegerType::Int32),
                span: span(),
            }],
            terminator: Terminator::Jump { target: BlockId(3) },
        },
        BasicBlock {
            id: BlockId(2),
            instructions: vec![Instruction::Constant {
                destination: bn::ir::ValueId(1),
                value: Constant::Integer("2".into()),
                ty: bn::semantic::Type::Integer(bn::semantic::IntegerType::Int32),
                span: span(),
            }],
            terminator: Terminator::Jump { target: BlockId(3) },
        },
        BasicBlock {
            id: BlockId(3),
            instructions: vec![Instruction::Print {
                values: vec![bn::ir::ValueId(1)],
                span: span(),
            }],
            terminator: Terminator::Branch {
                condition: bn::ir::ValueId(0),
                then_block: BlockId(3),
                else_block: BlockId(4),
            },
        },
        BasicBlock {
            id: BlockId(4),
            instructions: Vec::new(),
            terminator: Terminator::Return { value: None },
        },
    ]);
    validate_module(module).expect("all incoming paths define the reused value");
}
