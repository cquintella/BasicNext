use std::{fs, path::Path};

use bn::{
    ast::{ExpressionKind, Item, Statement},
    lexer::lex,
    parser::{parse, parse_expression},
    source::SourceFile,
};

fn parse_path(path: &str) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let source = SourceFile::new(path, text);
    let tokens = lex(&source).map_err(|diagnostic| diagnostic.message)?;
    parse(&tokens)
        .map(|_| ())
        .map_err(|diagnostic| diagnostic.message)
}

#[test]
fn valid_grammar_fixtures_have_valid_top_level_structure() {
    for entry in fs::read_dir(Path::new("tests/grammar/valid")).expect("valid fixture directory") {
        let path = entry.expect("fixture entry").path();
        if path.extension().is_some_and(|extension| extension == "bn") {
            parse_path(path.to_str().expect("UTF-8 path")).expect("valid structural fixture");
        }
    }
}

#[test]
fn mismatched_block_terminator_is_rejected() {
    assert!(parse_path("tests/grammar/invalid/mismatched-end.bn").is_err());
}

#[test]
fn syntax_error_fixtures_are_rejected_by_the_parser() {
    for path in [
        "tests/grammar/invalid/import-after-declaration.bn",
        "tests/grammar/invalid/mismatched-end.bn",
        "tests/grammar/invalid/untyped-let.bn",
        "tests/grammar/invalid/invalid-lvalue.bn",
        "tests/grammar/invalid/bare-stop.bn",
        "tests/grammar/invalid/bare-continue.bn",
        "tests/grammar/invalid/void-vector.bn",
        "tests/grammar/invalid/bad-input-call.bn",
    ] {
        assert!(parse_path(path).is_err(), "{path} must fail parsing");
    }
}

#[test]
fn power_is_right_associative() {
    let source = SourceFile::new("expression.bn", "2 ** 3 ** 4\n");
    let tokens = lex(&source).expect("lex expression");
    let expression = parse_expression(&tokens).expect("parse expression");
    let ExpressionKind::Binary {
        operator, right, ..
    } = expression.kind
    else {
        panic!("expected binary expression");
    };
    assert_eq!(operator, "Power");
    assert!(matches!(
        right.kind,
        ExpressionKind::Binary { ref operator, .. } if operator == "Power"
    ));
}

#[test]
fn binary_ast_preserves_the_exact_operator() {
    let source = SourceFile::new("expression.bn", "1 + 2 / 3 <> 4\n");
    let tokens = lex(&source).expect("lex expression");
    let expression = parse_expression(&tokens).expect("parse expression");
    let ExpressionKind::Binary {
        operator,
        left,
        right,
    } = expression.kind
    else {
        panic!("expected inequality expression");
    };
    assert_eq!(operator, "NotEqual");
    assert!(matches!(left.kind, ExpressionKind::Binary { ref operator, .. } if operator == "Plus"));
    assert!(matches!(right.kind, ExpressionKind::Literal(_)));
}

#[test]
fn loop_body_is_nested_in_the_ast() {
    let text = fs::read_to_string("examples/hello.bn").expect("read example");
    let source = SourceFile::new("examples/hello.bn", text);
    let tokens = lex(&source).expect("lex example");
    let program = parse(&tokens).expect("parse example");
    let Item::Declaration { statements, .. } = &program.items[1] else {
        panic!("expected function declaration");
    };
    let Some(Statement::While { body, .. }) = statements
        .iter()
        .find(|statement| matches!(statement, Statement::While { .. }))
    else {
        panic!("expected while statement");
    };
    assert_eq!(body.statements.len(), 2);
}

#[test]
fn function_signature_preserves_parameters_and_return_type() {
    let source = SourceFile::new(
        "signature.bn",
        "FUNCTION IsPositive(value AS FLOAT, label AS STRING) AS BOOLEAN\nRETURN TRUE\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let Item::Declaration {
        signature: Some(signature),
        ..
    } = &program.items[0]
    else {
        panic!("expected function signature");
    };
    assert_eq!(signature.parameters.len(), 2);
    assert_eq!(
        signature.parameters[0].type_ref.alternatives[0].name,
        "FLOAT"
    );
    assert_eq!(
        signature.parameters[1].type_ref.alternatives[0].name,
        "STRING"
    );
    assert_eq!(signature.return_type.alternatives[0].name, "BOOLEAN");
}

#[test]
fn binding_preserves_its_initializer_expression() {
    let source = SourceFile::new(
        "binding.bn",
        "FUNCTION Start() AS VOID\nLET value AS INTEGER = 2 ** 3\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let Item::Declaration { statements, .. } = &program.items[0] else {
        panic!("expected function");
    };
    let Statement::Binding {
        initializer: Some(expression),
        ..
    } = &statements[0]
    else {
        panic!("expected initializer");
    };
    assert!(matches!(expression.kind, ExpressionKind::Binary { .. }));
}

#[test]
fn vector_and_new_initializers_preserve_their_ast_nodes() {
    let source = SourceFile::new(
        "allocations.bn",
        "FUNCTION Start() AS VOID\nLET values AS INTEGER[2] = [1, 2]\nLET pointer AS POINTER TO INTEGER[2] = NEW INTEGER[2]\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let Item::Declaration { statements, .. } = &program.items[0] else {
        panic!("expected function");
    };
    let Statement::Binding {
        initializer: Some(vector),
        ..
    } = &statements[0]
    else {
        panic!("expected vector initializer");
    };
    assert!(matches!(vector.kind, ExpressionKind::Vector { .. }));
    let Statement::Binding {
        initializer: Some(allocation),
        ..
    } = &statements[1]
    else {
        panic!("expected allocation initializer");
    };
    assert!(matches!(allocation.kind, ExpressionKind::New { .. }));
}

#[test]
fn declaration_preserves_export_and_vector_dimensions() {
    let source = SourceFile::new(
        "export.bn",
        "EXPORT STRUCT Matrix\nvalues AS INTEGER[2][3]\nEND STRUCT\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let Item::Declaration {
        exported,
        statements,
        ..
    } = &program.items[0]
    else {
        panic!("expected declaration");
    };
    assert!(exported);
    let Statement::Binding { type_ref, .. } = &statements[0] else {
        panic!("expected field");
    };
    assert_eq!(
        type_ref.alternatives[0].parts,
        [
            "LeftBracket",
            "2",
            "RightBracket",
            "LeftBracket",
            "3",
            "RightBracket"
        ]
    );
}

#[test]
fn language_tour_preserves_start_return() {
    let text = fs::read_to_string("examples/language-tour.bn").expect("read language tour");
    let source = SourceFile::new("examples/language-tour.bn", text);
    let tokens = lex(&source).expect("lex language tour");
    let program = parse(&tokens).expect("parse language tour");
    let Item::Declaration {
        name, statements, ..
    } = program
        .items
        .iter()
        .find(|item| matches!(item, Item::Declaration { name, .. } if name == "Start"))
        .expect("Start declaration")
    else {
        panic!("expected Start declaration");
    };
    assert_eq!(name, "Start");
    assert!(matches!(statements.last(), Some(Statement::Return { .. })));
}

#[test]
fn if_branches_are_separate_blocks() {
    let source = SourceFile::new(
        "if.bn",
        "FUNCTION Start() AS VOID\nIF TRUE THEN\nPRINT \"yes\"\nELSE IF FALSE THEN\nPRINT \"maybe\"\nELSE\nPRINT \"no\"\nEND IF\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let Item::Declaration { statements, .. } = &program.items[0] else {
        panic!("expected function");
    };
    let Statement::If {
        branches,
        otherwise: Some(otherwise),
        ..
    } = &statements[0]
    else {
        panic!("expected if");
    };
    assert_eq!(branches.len(), 2);
    assert_eq!(branches[0].body.statements.len(), 1);
    assert_eq!(otherwise.statements.len(), 1);
}

#[test]
fn new_expression_enforces_numeric_and_class_forms() {
    for invalid in ["NEW INTEGER()\n", "NEW Box\n", "NEW STRING\n", "NEW Box[2]\n"] {
        let source = SourceFile::new("invalid-new.bn", invalid);
        let tokens = lex(&source).expect("lex NEW expression");
        assert!(parse_expression(&tokens).is_err(), "{invalid:?} must fail");
    }

    let source = SourceFile::new("qualified-new.bn", "NEW Models.Box(1)\n");
    let tokens = lex(&source).expect("lex qualified NEW");
    let expression = parse_expression(&tokens).expect("parse qualified NEW");
    assert!(matches!(
        expression.kind,
        ExpressionKind::New { ref type_name, .. } if type_name == "Models.Box"
    ));
}
