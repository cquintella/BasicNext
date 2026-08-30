// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use bn::{
    diagnostic::Diagnostic,
    lexer::lex,
    parser::parse,
    semantic::{IntegerType, PointerLength, SemanticModel, Type, analyze},
    source::SourceFile,
};

fn analyze_source(text: impl Into<String>) -> Result<SemanticModel, Diagnostic> {
    let source = SourceFile::new("sprint7-closure.bn", text);
    let tokens = lex(&source).expect("test source must lex");
    let program = parse(&tokens).expect("test source must parse");
    analyze(&program)
}

fn symbol_type<'a>(model: &'a SemanticModel, name: &str) -> &'a Type {
    &model
        .symbols
        .iter()
        .find(|symbol| symbol.name == name)
        .unwrap_or_else(|| panic!("symbol '{name}' must be resolved"))
        .ty
}

#[test]
fn normalized_aliases_cannot_repeat_in_an_alternative() {
    for (left, right) in [("INTEGER", "INT32"), ("FLOAT", "FLOAT64")] {
        let source =
            format!("FUNCTION Start() AS VOID\nLET value AS {left} OR {right} = 1\nEND FUNCTION\n");
        let diagnostic = analyze_source(source).expect_err("aliases denote one alternative type");
        assert_eq!(diagnostic.code, "TYPE_MISMATCH");
    }
}

#[test]
fn function_type_preserves_composed_pointer_types() {
    let model = analyze_source(
        "FUNCTION Identity(value AS POINTER TO INTEGER) AS POINTER TO INTEGER\n\
             RETURN value\n\
         END FUNCTION\n\
         FUNCTION Start() AS VOID\n\
             LET identity AS FUNCTION(POINTER TO INTEGER) AS POINTER TO INTEGER = Identity\n\
             LET pointer AS POINTER TO INTEGER = NEW INTEGER\n\
             LET same AS POINTER TO INTEGER = identity(pointer)\n\
         END FUNCTION\n",
    )
    .expect("FUNCTION types must retain their POINTER parameter and result");

    let pointer = Type::Pointer {
        element: Box::new(Type::Integer(IntegerType::Int32)),
        length: PointerLength::One,
    };
    assert_eq!(
        symbol_type(&model, "identity"),
        &Type::Function {
            parameters: vec![pointer.clone()],
            return_type: Box::new(pointer),
        }
    );
}

#[test]
fn vector_dimensions_are_exact_and_empty_literals_use_context() {
    let model = analyze_source(
        "FUNCTION Start() AS VOID\n\
             LET matrix AS INTEGER[2][3] = [[1, 2, 3], [4, 5, 6]]\n\
             LET empty AS INTEGER[0] = []\n\
         END FUNCTION\n",
    )
    .expect("matching dimensions and a context-typed empty vector must be accepted");

    assert_eq!(
        symbol_type(&model, "matrix"),
        &Type::Vector {
            element: Box::new(Type::Integer(IntegerType::Int32)),
            dimensions: vec![2, 3],
        }
    );
    assert_eq!(
        symbol_type(&model, "empty"),
        &Type::Vector {
            element: Box::new(Type::Integer(IntegerType::Int32)),
            dimensions: vec![0],
        }
    );
    assert!(
        model
            .expressions
            .iter()
            .all(|expression| expression.ty != Type::Unknown)
    );

    let diagnostic = analyze_source(
        "FUNCTION Start() AS VOID\n\
             LET matrix AS INTEGER[2][2] = [[1, 2]]\n\
         END FUNCTION\n",
    )
    .expect_err("a vector literal must match every declared dimension");
    assert_eq!(diagnostic.code, "TYPE_MISMATCH");
}

#[test]
fn calling_a_non_executable_value_is_rejected() {
    let diagnostic = analyze_source(
        "FUNCTION Start() AS VOID\n\
             LET value AS INTEGER = 1\n\
             value()\n\
         END FUNCTION\n",
    )
    .expect_err("an INTEGER value is not callable");
    assert_eq!(diagnostic.code, "NOT_CALLABLE");
}

#[test]
fn hexadecimal_pointer_length_is_a_fixed_length() {
    let model = analyze_source(
        "FUNCTION Start() AS VOID\n\
             LET memory AS POINTER TO INTEGER[0x10] = NEW INTEGER[0x10]\n\
         END FUNCTION\n",
    )
    .expect("hexadecimal integer literals are valid pointer lengths");

    assert_eq!(
        symbol_type(&model, "memory"),
        &Type::Pointer {
            element: Box::new(Type::Integer(IntegerType::Int32)),
            length: PointerLength::Fixed(16),
        }
    );
}
