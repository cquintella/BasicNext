use bn::{
    lexer::lex,
    parser::parse,
    semantic::{PointerLength, Type, analyze},
    source::SourceFile,
};
use std::fs;

#[test]
fn duplicate_top_level_declaration_is_rejected() {
    let source = SourceFile::new(
        "duplicate.bn",
        "FUNCTION Start() AS VOID\nEND FUNCTION\nFUNCTION Start() AS VOID\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    assert!(analyze(&program).is_err());
}

#[test]
fn duplicate_local_binding_is_rejected() {
    let source = SourceFile::new(
        "duplicate-local.bn",
        "FUNCTION Start() AS VOID\nLET x AS INTEGER = 1\nLET x AS INTEGER = 2\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    assert!(analyze(&program).is_err());
}

#[test]
fn pointer_binding_requires_initializer() {
    let source = SourceFile::new(
        "pointer.bn",
        "FUNCTION Start() AS VOID\nLET address AS POINTER TO INTEGER\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    assert!(analyze(&program).is_err());
}

#[test]
fn function_value_binding_requires_initializer() {
    let source = SourceFile::new(
        "function-value.bn",
        "FUNCTION Start() AS VOID\nLET transform AS FUNCTION(INTEGER) AS INTEGER\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    assert!(analyze(&program).is_err());
}

#[test]
fn unknown_name_is_rejected() {
    let source = SourceFile::new(
        "unknown.bn",
        "FUNCTION Start() AS VOID\nPRINT missing\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let diagnostic = analyze(&program).expect_err("unknown name must fail");
    assert_eq!(diagnostic.code, "NAME_NOT_FOUND");
}

#[test]
fn accepted_host_members_have_exact_types() {
    let path = "tests/grammar/valid/host-capabilities.bn";
    let source = SourceFile::new(path, fs::read_to_string(path).expect("read fixture"));
    let tokens = lex(&source).expect("lex fixture");
    let program = parse(&tokens).expect("parse fixture");
    analyze(&program).expect("accepted HOST members must type-check");
}

#[test]
fn host_argument_index_must_be_integer() {
    let source = SourceFile::new(
        "host-index.bn",
        "IMPORT HOST.main AS main\nFUNCTION Start() AS VOID\nPRINT main.Argument(\"0\")\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let diagnostic = analyze(&program).expect_err("STRING host index must fail");
    assert_eq!(diagnostic.code, "TYPE_MISMATCH");
}

#[test]
fn temporal_math_conversions_have_exact_types() {
    let source = SourceFile::new(
        "temporal.bn",
        "FUNCTION Start() AS VOID\nLET timestamp AS TIMESTAMP = 0\nLET date AS DATE = Math.TODATE(timestamp)\nLET time AS TIME = Math.TOTIME(timestamp)\nLET roundTrip AS TIMESTAMP = Math.TOTIMESTAMP(date, time)\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    analyze(&program).expect("temporal Math conversions must type-check");
}

#[test]
fn const_cannot_be_reassigned() {
    let source = SourceFile::new(
        "const.bn",
        "FUNCTION Start() AS VOID\nCONST value AS INTEGER = 1\nvalue = 2\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let diagnostic = analyze(&program).expect_err("CONST assignment must fail");
    assert_eq!(diagnostic.code, "TYPE_MISMATCH");
}

#[test]
fn loop_control_requires_the_named_enclosing_loop() {
    let source = SourceFile::new(
        "control.bn",
        "FUNCTION Start() AS VOID\nCONTINUE FOR\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let diagnostic = analyze(&program).expect_err("orphan CONTINUE must fail");
    assert_eq!(diagnostic.code, "INVALID_LOOP_CONTROL");
}

#[test]
fn semantic_fixtures_are_rejected() {
    for path in [
        "tests/grammar/invalid/continue-outside-loop.bn",
        "tests/grammar/invalid/cross-type-equality.bn",
        "tests/grammar/invalid/invalid-stop-code.bn",
        "tests/grammar/invalid/nonvoid-bare-return.bn",
        "tests/grammar/invalid/out-of-range-exit-code.bn",
        "tests/grammar/invalid/uninitialized-pointer.bn",
        "tests/grammar/invalid/void-return-value.bn",
        "tests/grammar/invalid/zero-for-step.bn",
        "tests/grammar/invalid/ragged-vector-literal.bn",
        "tests/grammar/invalid/removed-host-memory.bn",
    ] {
        let source = SourceFile::new(path, fs::read_to_string(path).expect("read fixture"));
        let tokens = lex(&source).expect("lex fixture");
        let program = parse(&tokens).expect("parse fixture");
        assert!(
            analyze(&program).is_err(),
            "{path} must fail semantic analysis"
        );
    }
}

#[test]
fn alternative_type_accepts_declared_singleton_values() {
    let source = SourceFile::new(
        "alternative.bn",
        "FUNCTION Start() AS VOID\nLET value AS STRING OR NULL = NULL\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    analyze(&program).expect("declared NULL alternative must be accepted");
}

#[test]
fn duplicate_alternative_type_is_rejected() {
    let source = SourceFile::new(
        "duplicate-alternative.bn",
        "FUNCTION Start() AS VOID\nLET value AS INTEGER OR INTEGER = 1\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    assert!(analyze(&program).is_err());
}

#[test]
fn is_test_requires_a_declared_alternative() {
    let source = SourceFile::new(
        "invalid-is.bn",
        "FUNCTION Start() AS VOID\nLET value AS STRING = \"text\"\nIF value IS NULL THEN\nPRINT value\nEND IF\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let diagnostic = analyze(&program).expect_err("undeclared NULL alternative must fail");
    assert_eq!(diagnostic.code, "INVALID_ALTERNATIVE_USE");
}

#[test]
fn direct_function_calls_validate_argument_count_and_type() {
    let source = SourceFile::new(
        "call.bn",
        "FUNCTION Double(value AS INTEGER) AS INTEGER\nRETURN value\nEND FUNCTION\nFUNCTION Start() AS VOID\nLET result AS INTEGER = Double(\"wrong\")\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let diagnostic = analyze(&program).expect_err("wrong function argument type must fail");
    assert_eq!(diagnostic.code, "TYPE_MISMATCH");
}

#[test]
fn statement_after_return_is_rejected_as_unreachable() {
    let source = SourceFile::new(
        "unreachable.bn",
        "FUNCTION Start() AS VOID\nRETURN\nPRINT \"never\"\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let diagnostic = analyze(&program).expect_err("unreachable statement must fail");
    assert_eq!(diagnostic.code, "UNREACHABLE_CODE");
}

#[test]
fn return_value_must_match_function_type() {
    let source = SourceFile::new(
        "return-type.bn",
        "FUNCTION Start() AS INTEGER\nRETURN \"wrong\"\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let diagnostic = analyze(&program).expect_err("wrong return type must fail");
    assert_eq!(diagnostic.code, "TYPE_MISMATCH");
}

#[test]
fn declared_numeric_values_do_not_convert_implicitly() {
    let source = SourceFile::new(
        "numeric.bn",
        "FUNCTION Start() AS VOID\nLET narrow AS INT16 = 1\nLET wide AS INT32 = narrow\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let diagnostic = analyze(&program).expect_err("implicit numeric conversion must fail");
    assert_eq!(diagnostic.code, "TYPE_MISMATCH");
}

#[test]
fn static_field_requires_an_initializer() {
    let source = SourceFile::new(
        "static.bn",
        "CLASS Counter\nSTATIC value AS INTEGER\nEND CLASS\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let diagnostic = analyze(&program).expect_err("static field without initializer must fail");
    assert_eq!(diagnostic.code, "TYPE_MISMATCH");
}

#[test]
fn class_member_names_are_unique() {
    let source = SourceFile::new(
        "members.bn",
        "CLASS Counter\nPUBLIC value AS INTEGER = 0\nPRIVATE value AS INTEGER = 1\nEND CLASS\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let diagnostic = analyze(&program).expect_err("duplicate member must fail");
    assert_eq!(diagnostic.code, "DUPLICATE_NAME");
}

#[test]
fn class_must_implement_a_declared_interface() {
    let source = SourceFile::new("interface.bn", "CLASS Box IMPLEMENTS Missing\nEND CLASS\n");
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let diagnostic = analyze(&program).expect_err("unknown interface must fail");
    assert_eq!(diagnostic.code, "NAME_NOT_FOUND");
}

#[test]
fn class_implementation_requires_public_interface_method() {
    let source = SourceFile::new(
        "implementation.bn",
        "INTERFACE Named\nFUNCTION Name() AS STRING\nEND INTERFACE\nCLASS Box IMPLEMENTS Named\nPRIVATE FUNCTION Name() AS STRING\nRETURN \"box\"\nEND FUNCTION\nEND CLASS\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let diagnostic = analyze(&program).expect_err("private implementation must fail");
    assert_eq!(diagnostic.code, "TYPE_MISMATCH");
}

#[test]
fn nonvoid_function_may_return_after_an_intermediate_if() {
    let source = SourceFile::new(
        "return-after-if.bn",
        "FUNCTION Value(flag AS BOOLEAN) AS INTEGER\nIF flag THEN\nPRINT \"yes\"\nEND IF\nRETURN 1\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    analyze(&program).expect("the final RETURN covers every path");
}

#[test]
fn arithmetic_operator_rules_are_checked() {
    let valid = SourceFile::new(
        "numeric-operators.bn",
        "FUNCTION Start() AS VOID\nLET quotient AS FLOAT = 5 / 2\nLET flags AS BYTE = 1\nLET mask AS BYTE = 2\nLET combined AS BYTE = flags OR mask\nEND FUNCTION\n",
    );
    let tokens = lex(&valid).expect("lex valid source");
    let program = parse(&tokens).expect("parse valid source");
    analyze(&program).expect("numeric operators must have their documented types");

    let invalid = SourceFile::new(
        "invalid-operator.bn",
        "FUNCTION Start() AS VOID\nLET result AS BOOLEAN = TRUE + FALSE\nEND FUNCTION\n",
    );
    let tokens = lex(&invalid).expect("lex invalid source");
    let program = parse(&tokens).expect("parse invalid source");
    assert_eq!(
        analyze(&program)
            .expect_err("BOOLEAN addition must fail")
            .code,
        "TYPE_MISMATCH"
    );
}

fn analyze_text(text: &str) -> Result<bn::semantic::SemanticModel, bn::diagnostic::Diagnostic> {
    let source = SourceFile::new("semantic-test.bn", text);
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    analyze(&program)
}

#[test]
fn constructors_private_members_and_interface_upcasts_are_resolved() {
    analyze_text(
        "INTERFACE Valued\nFUNCTION Value() AS INTEGER\nEND INTERFACE\nCLASS Counter IMPLEMENTS Valued\nPRIVATE value AS INTEGER = 0\nPUBLIC FUNCTION CONSTRUCTOR(initial AS INTEGER)\nSELF.value = initial\nEND FUNCTION\nPUBLIC FUNCTION Value() AS INTEGER\nRETURN SELF.value\nEND FUNCTION\nEND CLASS\nFUNCTION Start() AS VOID\nLET counter AS Counter = NEW Counter(1)\nLET valued AS Valued = counter\nPRINT valued.Value()\nEND FUNCTION\n",
    )
    .expect("object semantics must resolve");

    let error = analyze_text(
        "CLASS Box\nPRIVATE value AS INTEGER = 0\nPUBLIC FUNCTION CONSTRUCTOR()\nEND FUNCTION\nEND CLASS\nFUNCTION Start() AS VOID\nLET box AS Box = NEW Box()\nPRINT box.value\nEND FUNCTION\n",
    )
    .expect_err("private field access must fail");
    assert_eq!(error.code, "PRIVATE_ACCESS");
}

#[test]
fn constructor_targets_and_arguments_are_checked() {
    for (text, code) in [
        (
            "STRUCT Point\nX AS INTEGER\nEND STRUCT\nFUNCTION Start() AS VOID\nLET point AS Point = NEW Point()\nEND FUNCTION\n",
            "INVALID_CONSTRUCTOR",
        ),
        (
            "CLASS Box\nPUBLIC FUNCTION CONSTRUCTOR(value AS INTEGER)\nEND FUNCTION\nEND CLASS\nFUNCTION Start() AS VOID\nLET box AS Box = NEW Box(\"bad\")\nEND FUNCTION\n",
            "INVALID_CONSTRUCTOR",
        ),
        (
            "CLASS Box\nEND CLASS\nFUNCTION Start() AS VOID\nLET box AS Box = NEW Box()\nEND FUNCTION\n",
            "PRIVATE_ACCESS",
        ),
    ] {
        assert_eq!(analyze_text(text).expect_err("invalid NEW").code, code);
    }
}

#[test]
fn pointer_shapes_allocation_sizes_and_delete_targets_are_checked() {
    let model = analyze_text(
        "CLASS Box\nPUBLIC FUNCTION CONSTRUCTOR()\nEND FUNCTION\nEND CLASS\nFUNCTION Start() AS VOID\nLET one AS POINTER TO INTEGER = NEW INTEGER\nLET fixed AS POINTER TO INTEGER[2] = NEW INTEGER[2]\nLET dynamic AS POINTER TO INTEGER[] = NEW INTEGER[2]\nLET count AS INTEGER = 2\nLET checked AS POINTER TO INTEGER[2] = NEW INTEGER[count]\nLET optional AS POINTER TO INTEGER OR NULL = NULL\nLET box AS Box = NEW Box()\nIF optional IS POINTER TO INTEGER THEN\nPRINT optional[0]\nEND IF\nDELETE one\nDELETE fixed\nDELETE dynamic\nDELETE checked\nDELETE optional\nDELETE box\nEND FUNCTION\n",
    )
    .expect("valid pointer shapes");
    assert!(model.symbols.iter().any(|symbol| matches!(
        symbol.ty,
        Type::Pointer {
            length: PointerLength::Fixed(2),
            ..
        }
    )));

    for (text, code) in [
        (
            "FUNCTION Start() AS VOID\nLET bad AS POINTER TO STRING = NEW INTEGER\nEND FUNCTION\n",
            "INVALID_POINTER_TYPE",
        ),
        (
            "FUNCTION Start() AS VOID\nLET size AS FLOAT = 2.0\nLET bad AS POINTER TO INTEGER[] = NEW INTEGER[size]\nEND FUNCTION\n",
            "ALLOCATION_SIZE_INVALID",
        ),
        (
            "FUNCTION Start() AS VOID\nLET value AS INTEGER = 1\nDELETE value\nEND FUNCTION\n",
            "INVALID_DELETE_TARGET",
        ),
        (
            "FUNCTION Start() AS VOID\nLET bad AS POINTER TO INTEGER[2] = NEW INTEGER[3]\nEND FUNCTION\n",
            "POINTER_LENGTH_MISMATCH",
        ),
    ] {
        assert_eq!(
            analyze_text(text).expect_err("invalid pointer use").code,
            code
        );
    }
}

#[test]
fn exact_temporal_types_and_host_clock_survive_name_collisions() {
    analyze_text(
        "IMPORT HOST.clock AS HostClock\nCLASS Clock\nEND CLASS\nFUNCTION Start() AS VOID\nLET timestamp AS TIMESTAMP = HostClock.Timestamp()\nLET date AS DATE = Date.Parse(\"2026-08-22\")\nLET time AS TIME = Time.Parse(\"10:20:30.000\")\nLET zone AS TIMEZONE = TimeZone.Parse(\"America/Sao_Paulo\")\nLET parsed AS TIMESTAMP = Timestamp.Parse(\"2026-08-22T10:20:30.000Z\")\nLET rendered AS STRING = Timestamp.Format(parsed)\nEND FUNCTION\n",
    )
    .expect("temporal namespaces and HOST.clock must retain exact types");
}

#[test]
fn accepted_frontend_model_contains_no_unknown_type() {
    let path = "examples/language-tour.bn";
    let source = SourceFile::new(path, fs::read_to_string(path).expect("read fixture"));
    let tokens = lex(&source).expect("lex fixture");
    let program = parse(&tokens).expect("parse fixture");
    let model = analyze(&program).expect("analyze full frontend fixture");
    assert!(
        model
            .symbols
            .iter()
            .all(|symbol| !contains_unknown(&symbol.ty))
    );
    assert!(
        model
            .expressions
            .iter()
            .all(|expression| !contains_unknown(&expression.ty))
    );
}

fn contains_unknown(ty: &Type) -> bool {
    match ty {
        Type::Unknown => true,
        Type::Alternative(types) => types.iter().any(contains_unknown),
        Type::Function {
            parameters,
            return_type,
        } => parameters.iter().any(contains_unknown) || contains_unknown(return_type),
        Type::Vector { element, .. } | Type::Pointer { element, .. } => contains_unknown(element),
        _ => false,
    }
}

#[test]
fn compound_assignment_cannot_hide_a_result_conversion() {
    let source = SourceFile::new(
        "compound.bn",
        "FUNCTION Start() AS VOID\nLET value AS INTEGER = 4\nvalue /= 2\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    assert_eq!(
        analyze(&program)
            .expect_err("INTEGER /= produces FLOAT and must fail")
            .code,
        "TYPE_MISMATCH"
    );
}

#[test]
fn interface_implementation_requires_the_exact_signature() {
    let source = SourceFile::new(
        "interface-signature.bn",
        "INTERFACE Named\nFUNCTION Name(prefix AS STRING) AS STRING\nEND INTERFACE\nCLASS Box IMPLEMENTS Named\nPUBLIC FUNCTION Name(prefix AS INTEGER) AS STRING\nRETURN \"box\"\nEND FUNCTION\nEND CLASS\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    assert_eq!(
        analyze(&program)
            .expect_err("mismatched interface signature must fail")
            .code,
        "TYPE_MISMATCH"
    );
}
