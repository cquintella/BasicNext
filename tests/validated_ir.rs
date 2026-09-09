use std::io::Cursor;

use bn::{
    ir::{
        BasicBlock, BlockId, Constant, Function, Instruction, Module, Terminator,
        lower_graph_validated, validate_module,
    },
    llvm::{Target, lower_validated_module_for_target, validate_for},
    module_graph::load,
    runtime::{HostEnv, execute_validated_with_host},
    semantic::analyze_modules,
};

fn span() -> bn::source::Span {
    let position = bn::source::Position {
        source_id: bn::source::Position::UNKNOWN_SOURCE,
        revision: bn::source::Position::UNKNOWN_REVISION,
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

#[test]
fn target_support_is_checked_after_language_validation() {
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![Instruction::Default {
            destination: bn::ir::ValueId(0),
            ty: bn::semantic::Type::Vector {
                element: Box::new(bn::semantic::Type::Integer(
                    bn::semantic::IntegerType::Int32,
                )),
                dimensions: vec![2, 3],
            },
            dimensions: vec![2, 3],
            dynamic_dimensions: Vec::new(),
            span: span(),
        }],
        terminator: Terminator::Return { value: None },
    }]);
    let validated = validate_module(module).expect("multidimensional vector is valid IR");
    let error = validate_for(&validated, Target::Native).expect_err("LLVM lacks vector support");
    assert_eq!(error.code, "TARGET_UNSUPPORTED_TYPE");
}

#[test]
fn vector_default_with_non_scalar_element_is_rejected_before_emit() {
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![Instruction::Default {
            destination: bn::ir::ValueId(0),
            ty: bn::semantic::Type::Vector {
                element: Box::new(bn::semantic::Type::String),
                dimensions: vec![2],
            },
            dimensions: vec![2],
            dynamic_dimensions: Vec::new(),
            span: span(),
        }],
        terminator: Terminator::Return { value: None },
    }]);
    let validated = validate_module(module).expect("vector default is valid language IR");
    let error = validate_for(&validated, Target::Native)
        .expect_err("LLVM must reject a non-scalar vector default before emission");
    assert_eq!(error.code, "TARGET_UNSUPPORTED_TYPE");
}

#[test]
fn vector_default_with_oversized_dimension_is_rejected_before_emit() {
    let oversized = usize::try_from(u32::MAX).expect("test platform has 32-bit usize") + 1;
    let type_dimension = u64::try_from(oversized).expect("test dimension fits in u64");
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![Instruction::Default {
            destination: bn::ir::ValueId(0),
            ty: bn::semantic::Type::Vector {
                element: Box::new(bn::semantic::Type::Integer(
                    bn::semantic::IntegerType::Int32,
                )),
                dimensions: vec![type_dimension],
            },
            dimensions: vec![oversized],
            dynamic_dimensions: Vec::new(),
            span: span(),
        }],
        terminator: Terminator::Return { value: None },
    }]);
    let validated = validate_module(module).expect("oversized vector default is valid IR");
    let error = validate_for(&validated, Target::Native)
        .expect_err("LLVM index operands cannot represent this vector dimension");
    assert_eq!(error.code, "TARGET_UNSUPPORTED_TYPE");
}

#[test]
fn index_of_non_indexable_value_is_rejected_by_language_validation() {
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::Boolean(true),
                ty: bn::semantic::Type::Boolean,
                span: span(),
            },
            Instruction::Constant {
                destination: bn::ir::ValueId(1),
                value: Constant::Integer("0".into()),
                ty: bn::semantic::Type::Integer(bn::semantic::IntegerType::Int32),
                span: span(),
            },
            Instruction::Index {
                destination: bn::ir::ValueId(2),
                object: bn::ir::ValueId(0),
                index: bn::ir::ValueId(1),
                ty: bn::semantic::Type::String,
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("boolean is not indexable");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
fn index_result_must_match_the_indexed_element_type() {
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::Integer("1".into()),
                ty: bn::semantic::Type::Integer(bn::semantic::IntegerType::Int32),
                span: span(),
            },
            Instruction::Vector {
                destination: bn::ir::ValueId(1),
                values: vec![bn::ir::ValueId(0)],
                ty: bn::semantic::Type::Vector {
                    element: Box::new(bn::semantic::Type::Integer(
                        bn::semantic::IntegerType::Int32,
                    )),
                    dimensions: vec![1],
                },
                span: span(),
            },
            Instruction::Index {
                destination: bn::ir::ValueId(2),
                object: bn::ir::ValueId(1),
                index: bn::ir::ValueId(0),
                ty: bn::semantic::Type::String,
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("index result must be integer");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
#[allow(clippy::too_many_lines)]
fn member_and_class_identity_names_cannot_be_empty() {
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![Instruction::EnsureClass {
            class: String::new(),
            span: span(),
        }],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("empty class identity");
    assert_eq!(error.code, "INVALID_IR");

    // Member with empty owner
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::Integer("0".to_string()),
                ty: bn::ir::Type::Integer(bn::ir::IntegerType::Int32),
                span: span(),
            },
            Instruction::Member {
                destination: bn::ir::ValueId(1),
                object: bn::ir::ValueId(0),
                name: "field".to_string(),
                owner: String::new(),
                ty: bn::ir::Type::Integer(bn::ir::IntegerType::Int32),
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("empty member owner");
    assert_eq!(error.code, "INVALID_IR");

    // SetMember with empty owner
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::Integer("0".to_string()),
                ty: bn::ir::Type::Integer(bn::ir::IntegerType::Int32),
                span: span(),
            },
            Instruction::SetMember {
                object: bn::ir::ValueId(0),
                name: "field".to_string(),
                owner: String::new(),
                value: bn::ir::ValueId(0),
                ty: bn::ir::Type::Integer(bn::ir::IntegerType::Int32),
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("empty set_member owner");
    assert_eq!(error.code, "INVALID_IR");

    // StoreStatic with empty class
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::Integer("0".to_string()),
                ty: bn::ir::Type::Integer(bn::ir::IntegerType::Int32),
                span: span(),
            },
            Instruction::StoreStatic {
                class: String::new(),
                field: "f".to_string(),
                value: bn::ir::ValueId(0),
                ty: bn::ir::Type::Integer(bn::ir::IntegerType::Int32),
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("empty store_static class");
    assert_eq!(error.code, "INVALID_IR");

    // Allocate with empty type_name
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![Instruction::Allocate {
            destination: bn::ir::ValueId(0),
            type_name: String::new(),
            arguments: Vec::new(),
            ty: bn::ir::Type::Unknown,
            span: span(),
        }],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("empty allocate type_name");
    assert_eq!(error.code, "INVALID_IR");

    // Delete with empty destructor
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::Integer("0".to_string()),
                ty: bn::ir::Type::Integer(bn::ir::IntegerType::Int32),
                span: span(),
            },
            Instruction::Delete {
                value: bn::ir::ValueId(0),
                destructor: Some(String::new()),
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("empty delete destructor");
    assert_eq!(error.code, "INVALID_IR");

    // SetField with empty path
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::Integer("0".to_string()),
                ty: bn::ir::Type::Integer(bn::ir::IntegerType::Int32),
                span: span(),
            },
            Instruction::SetField {
                symbol: bn::ir::SymbolId(0),
                path: vec![],
                value: bn::ir::ValueId(0),
                ty: bn::ir::Type::Integer(bn::ir::IntegerType::Int32),
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("empty set_field path");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
fn dispatch_submit_requires_a_queue_and_function_task() {
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::Boolean(true),
                ty: bn::semantic::Type::Boolean,
                span: span(),
            },
            Instruction::Constant {
                destination: bn::ir::ValueId(1),
                value: Constant::Boolean(false),
                ty: bn::semantic::Type::Boolean,
                span: span(),
            },
            Instruction::DispatchSubmit {
                destination: bn::ir::ValueId(2),
                callee: bn::ir::ValueId(0),
                queue: bn::ir::ValueId(0),
                task: bn::ir::ValueId(1),
                arguments: Vec::new(),
                ty: bn::semantic::Type::Unknown,
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("invalid dispatch operands");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
fn dispatch_await_requires_a_ticket_and_integer_timeout() {
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::Boolean(true),
                ty: bn::semantic::Type::Boolean,
                span: span(),
            },
            Instruction::Constant {
                destination: bn::ir::ValueId(1),
                value: Constant::Boolean(false),
                ty: bn::semantic::Type::Boolean,
                span: span(),
            },
            Instruction::DispatchAwait {
                destination: bn::ir::ValueId(2),
                callee: bn::ir::ValueId(0),
                ticket: bn::ir::ValueId(0),
                timeout: bn::ir::ValueId(1),
                ty: bn::semantic::Type::Unknown,
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("invalid await operands");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
fn valid_eof_constant_is_rejected_as_target_support_not_language_error() {
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![Instruction::Constant {
            destination: bn::ir::ValueId(0),
            value: Constant::EndOfFile,
            ty: bn::semantic::Type::EndOfFile,
            span: span(),
        }],
        terminator: Terminator::Return { value: None },
    }]);
    let validated = validate_module(module).expect("EOF is valid language IR");
    let error = validate_for(&validated, Target::Native).expect_err("LLVM lacks EOF constants");
    assert_eq!(error.code, "TARGET_UNSUPPORTED_OP");
}

#[test]
fn valid_numeric_constant_with_invalid_llvm_literal_is_rejected_before_emit() {
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![Instruction::Constant {
            destination: bn::ir::ValueId(0),
            value: Constant::Integer("not-an-integer".into()),
            ty: bn::semantic::Type::Integer(bn::semantic::IntegerType::Int32),
            span: span(),
        }],
        terminator: Terminator::Return { value: None },
    }]);
    let validated = validate_module(module).expect("literal shape is valid language IR");
    let error = validate_for(&validated, Target::Native)
        .expect_err("LLVM must reject an unparsable numeric literal before emission");
    assert_eq!(error.code, "TARGET_UNSUPPORTED_OP");
}

#[test]
fn missing_start_is_a_target_entrypoint_rejection() {
    let module = Module::default();
    let validated = validate_module(module).expect("empty module is valid language IR");
    let error = validate_for(&validated, Target::Native).expect_err("LLVM needs Start");
    assert_eq!(error.code, "TARGET_UNSUPPORTED_ENTRYPOINT");
}

#[test]
fn validator_rejects_indexed_member_store_without_an_object_receiver() {
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::Boolean(true),
                ty: bn::semantic::Type::Boolean,
                span: span(),
            },
            Instruction::Constant {
                destination: bn::ir::ValueId(1),
                value: Constant::Integer("0".into()),
                ty: bn::semantic::Type::Integer(bn::semantic::IntegerType::Int32),
                span: span(),
            },
            Instruction::SetMemberIndex {
                object: bn::ir::ValueId(0),
                name: "data".into(),
                owner: "Fake".into(),
                indices: vec![bn::ir::ValueId(1)],
                value: bn::ir::ValueId(0),
                ty: bn::semantic::Type::Boolean,
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("receiver must have object identity");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
fn validator_rejects_an_indexed_store_without_indices() {
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Load {
                destination: bn::ir::ValueId(0),
                symbol: bn::ir::SymbolId::from_raw(0),
                ty: bn::semantic::Type::Named("Box".into()),
                span: span(),
            },
            Instruction::Constant {
                destination: bn::ir::ValueId(1),
                value: Constant::Integer("1".into()),
                ty: bn::semantic::Type::Integer(bn::semantic::IntegerType::Int32),
                span: span(),
            },
            Instruction::SetMemberIndex {
                object: bn::ir::ValueId(0),
                name: "data".into(),
                owner: "Box".into(),
                indices: Vec::new(),
                value: bn::ir::ValueId(1),
                ty: bn::semantic::Type::Integer(bn::semantic::IntegerType::Int32),
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("indexed store needs an index");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
fn validator_rejects_cyclic_class_layout_metadata() {
    let mut module = Module::default();
    module.class_bases.insert("A".into(), "B".into());
    module.class_bases.insert("B".into(), "A".into());
    let error = validate_module(module).expect_err("layout inheritance must be acyclic");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
fn validator_rejects_dangling_class_layout_metadata() {
    let mut module = Module::default();
    module.class_bases.insert("Child".into(), "Parent".into());
    let error = validate_module(module).expect_err("layout classes must exist in the module");
    assert_eq!(error.code, "INVALID_IR");
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
fn validator_rejects_phi_without_every_reachable_predecessor() {
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
                destination: bn::ir::ValueId(2),
                value: Constant::Integer("2".into()),
                ty: bn::semantic::Type::Integer(bn::semantic::IntegerType::Int32),
                span: span(),
            }],
            terminator: Terminator::Jump { target: BlockId(3) },
        },
        BasicBlock {
            id: BlockId(3),
            instructions: vec![Instruction::Phi {
                destination: bn::ir::ValueId(3),
                incoming: vec![(BlockId(1), bn::ir::ValueId(1))],
                ty: bn::semantic::Type::Integer(bn::semantic::IntegerType::Int32),
                span: span(),
            }],
            terminator: Terminator::Return { value: None },
        },
    ]);
    let error = validate_module(module).expect_err("Phi must cover every predecessor");
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
fn validator_rejects_a_copy_with_an_incompatible_value_type() {
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::Boolean(true),
                ty: bn::semantic::Type::Boolean,
                span: span(),
            },
            Instruction::Copy {
                destination: bn::ir::ValueId(1),
                source: bn::ir::ValueId(0),
                ty: bn::semantic::Type::Integer(bn::semantic::IntegerType::Int32),
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("copy type must match the value");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
fn validator_rejects_an_operator_with_incompatible_types() {
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::Boolean(true),
                ty: bn::semantic::Type::Boolean,
                span: span(),
            },
            Instruction::Constant {
                destination: bn::ir::ValueId(1),
                value: Constant::Boolean(false),
                ty: bn::semantic::Type::Boolean,
                span: span(),
            },
            Instruction::Binary {
                destination: bn::ir::ValueId(2),
                operator: "Plus".into(),
                left: bn::ir::ValueId(0),
                right: bn::ir::ValueId(1),
                ty: bn::semantic::Type::Boolean,
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("boolean addition is invalid IR");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
fn validator_rejects_an_invalid_cast() {
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::Boolean(true),
                ty: bn::semantic::Type::Boolean,
                span: span(),
            },
            Instruction::Cast {
                destination: bn::ir::ValueId(1),
                value: bn::ir::ValueId(0),
                ty: bn::semantic::Type::String,
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("boolean-to-string is invalid IR");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
fn validator_rejects_a_call_through_a_non_function_value() {
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::Boolean(true),
                ty: bn::semantic::Type::Boolean,
                span: span(),
            },
            Instruction::Call {
                destination: bn::ir::ValueId(1),
                callee: bn::ir::ValueId(0),
                arguments: Vec::new(),
                ty: bn::semantic::Type::Named("VOID".into()),
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("boolean callee is invalid IR");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
fn validator_rejects_console_control_with_a_non_console_value() {
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::Boolean(true),
                ty: bn::semantic::Type::Boolean,
                span: span(),
            },
            Instruction::Beep {
                console: bn::ir::ValueId(0),
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("console control needs HOST.Console");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
fn validator_rejects_a_constant_with_an_incompatible_value_type() {
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![Instruction::Constant {
            destination: bn::ir::ValueId(0),
            value: Constant::Boolean(true),
            ty: bn::semantic::Type::Integer(bn::semantic::IntegerType::Int32),
            span: span(),
        }],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("constant type mismatch must be rejected");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
fn validator_rejects_incompatible_store_values() {
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::Boolean(true),
                ty: bn::semantic::Type::Boolean,
                span: span(),
            },
            Instruction::Store {
                symbol: bn::ir::SymbolId::from_raw(0),
                value: bn::ir::ValueId(0),
                ty: bn::semantic::Type::Integer(bn::semantic::IntegerType::Int32),
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("store value must match its declared type");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
fn validator_rejects_incompatible_member_and_field_values() {
    let boolean_constant = || Instruction::Constant {
        destination: bn::ir::ValueId(0),
        value: Constant::Boolean(true),
        ty: bn::semantic::Type::Boolean,
        span: span(),
    };
    let cases = [
        Instruction::SetMember {
            object: bn::ir::ValueId(0),
            name: "value".into(),
            owner: "Example".into(),
            value: bn::ir::ValueId(0),
            ty: bn::semantic::Type::Integer(bn::semantic::IntegerType::Int32),
            span: span(),
        },
        Instruction::SetField {
            symbol: bn::ir::SymbolId::from_raw(0),
            path: vec!["value".into()],
            value: bn::ir::ValueId(0),
            ty: bn::semantic::Type::Integer(bn::semantic::IntegerType::Int32),
            span: span(),
        },
        Instruction::StoreStatic {
            class: "Example".into(),
            field: "value".into(),
            value: bn::ir::ValueId(0),
            ty: bn::semantic::Type::Integer(bn::semantic::IntegerType::Int32),
            span: span(),
        },
    ];
    for instruction in cases {
        let module = function_with_blocks(vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![boolean_constant(), instruction],
            terminator: Terminator::Return { value: None },
        }]);
        let error = validate_module(module).expect_err("assignment value must match its type");
        assert_eq!(error.code, "INVALID_IR");
    }
}

#[test]
fn validator_rejects_non_integer_indices_and_mismatched_vector_elements() {
    let index_module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::String("text".into()),
                ty: bn::semantic::Type::String,
                span: span(),
            },
            Instruction::Constant {
                destination: bn::ir::ValueId(1),
                value: Constant::Boolean(false),
                ty: bn::semantic::Type::Boolean,
                span: span(),
            },
            Instruction::Index {
                destination: bn::ir::ValueId(2),
                object: bn::ir::ValueId(0),
                index: bn::ir::ValueId(1),
                ty: bn::semantic::Type::String,
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(index_module).expect_err("index must be integer");
    assert_eq!(error.code, "INVALID_IR");

    let vector_module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::Boolean(true),
                ty: bn::semantic::Type::Boolean,
                span: span(),
            },
            Instruction::Vector {
                destination: bn::ir::ValueId(1),
                values: vec![bn::ir::ValueId(0)],
                ty: bn::semantic::Type::Vector {
                    element: Box::new(bn::semantic::Type::Integer(
                        bn::semantic::IntegerType::Int32,
                    )),
                    dimensions: vec![1],
                },
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(vector_module).expect_err("vector element type must match");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
fn validator_rejects_a_vector_with_the_wrong_shape() {
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::Integer("1".into()),
                ty: bn::semantic::Type::Integer(bn::semantic::IntegerType::Int32),
                span: span(),
            },
            Instruction::Vector {
                destination: bn::ir::ValueId(1),
                values: vec![bn::ir::ValueId(0)],
                ty: bn::semantic::Type::Vector {
                    element: Box::new(bn::semantic::Type::Integer(
                        bn::semantic::IntegerType::Int32,
                    )),
                    dimensions: vec![2],
                },
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("vector shape mismatch must be rejected");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
fn validator_rejects_non_boolean_branch_condition() {
    let module = function_with_blocks(vec![
        BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::Integer("1".into()),
                ty: bn::semantic::Type::Integer(bn::semantic::IntegerType::Int32),
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
            instructions: Vec::new(),
            terminator: Terminator::Return { value: None },
        },
        BasicBlock {
            id: BlockId(2),
            instructions: Vec::new(),
            terminator: Terminator::Return { value: None },
        },
    ]);
    let error = validate_module(module).expect_err("integer branch condition must be rejected");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
fn validator_rejects_return_value_for_void_function() {
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![Instruction::Constant {
            destination: bn::ir::ValueId(0),
            value: Constant::Integer("1".into()),
            ty: bn::semantic::Type::Integer(bn::semantic::IntegerType::Int32),
            span: span(),
        }],
        terminator: Terminator::Return {
            value: Some(bn::ir::ValueId(0)),
        },
    }]);
    let error = validate_module(module).expect_err("VOID function cannot return a value");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
fn validator_rejects_duplicate_value_definitions() {
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::Integer("1".into()),
                ty: bn::semantic::Type::Integer(bn::semantic::IntegerType::Int32),
                span: span(),
            },
            Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::Integer("2".into()),
                ty: bn::semantic::Type::Integer(bn::semantic::IntegerType::Int32),
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("SSA values must have one definition");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
fn validator_rejects_phi_after_a_non_phi_instruction() {
    let module = function_with_blocks(vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            Instruction::Constant {
                destination: bn::ir::ValueId(0),
                value: Constant::Boolean(true),
                ty: bn::semantic::Type::Boolean,
                span: span(),
            },
            Instruction::Phi {
                destination: bn::ir::ValueId(1),
                incoming: Vec::new(),
                ty: bn::semantic::Type::Boolean,
                span: span(),
            },
        ],
        terminator: Terminator::Return { value: None },
    }]);
    let error = validate_module(module).expect_err("Phi must be at block start");
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
                destination: bn::ir::ValueId(2),
                value: Constant::Integer("2".into()),
                ty: bn::semantic::Type::Integer(bn::semantic::IntegerType::Int32),
                span: span(),
            }],
            terminator: Terminator::Jump { target: BlockId(3) },
        },
        BasicBlock {
            id: BlockId(3),
            instructions: vec![
                Instruction::Phi {
                    destination: bn::ir::ValueId(3),
                    incoming: vec![
                        (BlockId(1), bn::ir::ValueId(1)),
                        (BlockId(2), bn::ir::ValueId(2)),
                        (BlockId(3), bn::ir::ValueId(3)),
                    ],
                    ty: bn::semantic::Type::Integer(bn::semantic::IntegerType::Int32),
                    span: span(),
                },
                Instruction::Print {
                    values: vec![bn::ir::ValueId(3)],
                    span: span(),
                },
            ],
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
