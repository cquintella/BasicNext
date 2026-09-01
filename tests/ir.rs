// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use bn::{
    ir::{BlockId, Constant, Function, Instruction, Module, Terminator, lower_graph, validate},
    module_graph::load,
    semantic::analyze_modules,
};

fn lower_path(path: &str) -> bn::ir::Module {
    let graph = load(std::path::Path::new(path)).expect("load source");
    let models = analyze_modules(&graph).expect("analyze source");
    lower_graph(&graph, &models).expect("lower source")
}

#[test]
fn factorial_lowers_to_typed_control_flow() {
    let module = lower_path("examples/factorial.bn");
    assert_eq!(module.functions.len(), 2);
    let factorial = &module.functions[0];
    assert_eq!(factorial.name, "Factorial");
    assert_eq!(factorial.parameters.len(), 1);
    assert!(factorial.blocks.len() > 1);
    assert!(factorial.blocks.iter().all(|block| matches!(
        block.terminator,
        Terminator::Jump { .. }
            | Terminator::Branch { .. }
            | Terminator::Return { .. }
            | Terminator::Stop { .. }
    )));
    assert!(factorial.blocks.iter().any(|block| block.instructions.iter().any(
        |instruction| matches!(instruction, Instruction::Binary { operator, .. } if operator == "Assign")
    )));
}

#[test]
fn len_and_sizeof_lower_static_values_to_constants() {
    let module = lower_path("tests/grammar/valid/len-and-sizeof.bn");
    let start = module
        .functions
        .iter()
        .find(|function| function.name == "Start")
        .expect("Start");
    let instructions: Vec<_> = start
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .collect();
    assert!(instructions.iter().any(|instruction| matches!(
        instruction,
        Instruction::Constant {
            value: Constant::Integer(value),
            ..
        } if value == "1"
    )));
    assert!(instructions.iter().any(|instruction| matches!(
        instruction,
        Instruction::Constant {
            value: Constant::Integer(value),
            ..
        } if value == "4"
    )));
    assert!(instructions.iter().any(|instruction| matches!(
        instruction,
        Instruction::Constant {
            value: Constant::Integer(value),
            ..
        } if value == "24"
    )));
    assert!(
        instructions
            .iter()
            .any(|instruction| matches!(instruction, Instruction::Length { .. }))
    );
    assert!(
        instructions
            .iter()
            .any(|instruction| matches!(instruction, Instruction::SizeOf { .. }))
    );
}

#[test]
fn language_tour_lowers_to_extended_ir() {
    let module = lower_path("examples/language-tour.bn");
    assert!(
        module
            .functions
            .iter()
            .any(|function| function.name == "Start")
    );
    assert!(
        module
            .functions
            .iter()
            .any(|function| function.name == "Counter.CONSTRUCTOR")
    );
    assert!(
        module
            .functions
            .iter()
            .any(|function| function.name == "Counter.DESTRUCTOR")
    );
    assert!(
        module
            .functions
            .iter()
            .any(|function| function.name == "Counter.Increment")
    );
    assert!(
        module
            .functions
            .iter()
            .any(|function| function.name == "Counter.$fields")
    );
    assert!(
        module
            .functions
            .iter()
            .any(|function| function.name == "Point.$default")
    );
    let instructions: Vec<_> = module
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .collect();
    assert!(
        instructions
            .iter()
            .any(|instruction| matches!(instruction, Instruction::Allocate { .. }))
    );
    assert!(
        instructions
            .iter()
            .any(|instruction| matches!(instruction, Instruction::Delete { .. }))
    );
    assert!(
        instructions
            .iter()
            .any(|instruction| matches!(instruction, Instruction::SetMember { .. }))
    );
    assert!(
        instructions
            .iter()
            .any(|instruction| matches!(instruction, Instruction::SetField { .. }))
    );
}

#[test]
fn imported_class_constructor_uses_the_module_id() {
    let graph = load(std::path::Path::new("tests/modules/objects/main.bn")).expect("load objects");
    let models = analyze_modules(&graph).expect("analyze objects");
    let module = lower_graph(&graph, &models).expect("lower objects");
    assert!(module.functions.iter().any(|function| {
        function.name.ends_with("Box.CONSTRUCTOR") && function.name.starts_with('#')
    }));
    assert!(module.functions.iter().any(|function| {
        function.name.ends_with("Box.$fields") && function.name.starts_with('#')
    }));
}

#[test]
fn bndata_provider_is_not_lowered_as_executable_bn() {
    let graph =
        load(std::path::Path::new("tests/grammar/valid/bndata-import.bn")).expect("load BNData");
    let provider = graph
        .modules
        .iter()
        .find(|module| module.standard_module.is_some())
        .expect("BNData provider");
    let models = analyze_modules(&graph).expect("analyze BNData");
    let module = lower_graph(&graph, &models).expect("lower BNData user program");
    assert!(module.bndata_providers.contains(&provider.id));
    assert!(
        !module
            .functions
            .iter()
            .any(|function| function.name.starts_with(&format!("#{}.", provider.id.0)))
    );
}

#[test]
fn validate_rejects_a_dangling_block_target() {
    let module = Module {
        source_name: None,
        functions: vec![Function {
            name: "Broken".into(),
            parameters: Vec::new(),
            return_type: bn::semantic::Type::Named("VOID".into()),
            entry: BlockId(0),
            blocks: vec![bn::ir::BasicBlock {
                id: BlockId(0),
                instructions: Vec::new(),
                terminator: Terminator::Jump { target: BlockId(9) },
            }],
            span: bn::source::Span {
                start: bn::source::Position {
                    offset: 0,
                    line: 1,
                    column: 1,
                },
                end: bn::source::Position {
                    offset: 0,
                    line: 1,
                    column: 1,
                },
            },
        }],
        ..Module::default()
    };
    let error = validate(&module).expect_err("dangling terminator must fail");
    assert_eq!(error.code, "INVALID_IR");
}

#[test]
fn loops_and_function_values_lower_without_ast_names() {
    let module = lower_path("tests/grammar/valid/function-values-and-control-flow.bn");
    assert!(
        module
            .functions
            .iter()
            .any(|function| function.name == "Start")
    );
    assert!(
        module
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .any(|block| matches!(block.terminator, Terminator::Branch { .. }))
    );
}
