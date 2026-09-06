use bn_frontend::{lexer::lex, parser::parse, semantic::analyze, source::SourceFile};

#[test]
fn frontend_exposes_the_shared_lowering_entrypoint() {
    let source = SourceFile::new("lowering.bn", "FUNCTION Start() AS VOID\nEND FUNCTION\n");
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let model = analyze(&program).expect("analyze source");
    let module = bn_frontend::lowering::lower_validated(&program, &model)
        .expect("lower and validate source");
    assert!(
        module
            .as_module()
            .functions
            .iter()
            .any(|function| function.name == "Start")
    );
}
