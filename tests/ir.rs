use std::fs;

use bn::{
    ir::{Instruction, Terminator, lower},
    lexer::lex,
    parser::parse_named,
    semantic::analyze,
    source::SourceFile,
};

fn lower_path(path: &str) -> bn::ir::Module {
    let source = SourceFile::new(path, fs::read_to_string(path).expect("read source"));
    let tokens = lex(&source).expect("lex source");
    let program = parse_named(&tokens, path).expect("parse source");
    let model = analyze(&program).expect("analyze source");
    lower(&program, &model).expect("lower source")
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
