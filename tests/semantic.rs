// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use bn::{
    lexer::lex,
    module_graph::load,
    parser::parse,
    semantic::{PointerLength, Type, analyze, analyze_modules},
    source::SourceFile,
};
use std::fs;
use std::path::Path;

fn analyze_path(path: &str) -> bn::semantic::SemanticModel {
    let graph = load(Path::new(path)).expect("load source");
    let models = analyze_modules(&graph).expect("analyze source");
    let index = usize::try_from(graph.root.0).expect("root index");
    models.into_iter().nth(index).expect("root model")
}

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
fn multi_binding_initializers_cannot_see_declared_names() {
    let source = SourceFile::new(
        "multi-scope.bn",
        "FUNCTION Start() AS VOID\nLET first, second AS INTEGER = second, 2\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let diagnostic = analyze(&program).expect_err("binding names must be out of scope");
    assert_eq!(diagnostic.code, "NAME_NOT_FOUND");
}

#[test]
fn multi_binding_initializer_count_must_match() {
    let source = SourceFile::new(
        "multi-count.bn",
        "FUNCTION Start() AS VOID\nLET first, second AS INTEGER = 1\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    assert!(parse(&tokens).is_err());
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
fn bnmath_requires_an_explicit_import() {
    let source = SourceFile::new(
        "bnmath-import.bn",
        "FUNCTION Start() AS VOID\nPRINT BNMath.SQRT(1.0)\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let diagnostic = analyze(&program).expect_err("BNMath must require IMPORT");
    assert_eq!(diagnostic.code, "NAME_NOT_FOUND");
}

#[test]
fn accepted_host_members_have_exact_types() {
    analyze_path("tests/grammar/valid/host-capabilities.bn");
}

#[test]
fn host_net_identity_exposes_typed_address_namespace() {
    let source = SourceFile::new(
        "host-net.bn",
        "IMPORT HOST.Net AS Net\nFUNCTION Start() AS VOID\nLET address AS Net.Address\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    analyze(&program).expect("HOST.Net.Address must resolve as a host type");
}

#[test]
fn host_argument_index_must_be_integer() {
    let source = SourceFile::new(
        "host-index.bn",
        "FUNCTION Start() AS VOID\nPRINT HOST.Args[\"0\"]\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let diagnostic = analyze(&program).expect_err("STRING host index must fail");
    assert_eq!(diagnostic.code, "TYPE_MISMATCH");
}

#[test]
fn withdrawn_host_main_is_rejected() {
    let source = SourceFile::new(
        "withdrawn.bn",
        "IMPORT HOST.Main AS main\nFUNCTION Start() AS VOID\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let diagnostic = analyze(&program).expect_err("HOST.Main must be withdrawn");
    assert_eq!(diagnostic.code, "NAME_NOT_FOUND");
}

#[test]
fn string_indices_are_read_only() {
    let source = SourceFile::new(
        "string-index.bn",
        "FUNCTION Start() AS VOID\nLET text AS STRING = \"a\"\ntext[0] = \"b\"\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let diagnostic = analyze(&program).expect_err("STRING index assignment must fail");
    assert_eq!(diagnostic.code, "TYPE_MISMATCH");
}

#[test]
fn temporal_math_conversions_have_exact_types() {
    analyze_path("tests/grammar/valid/bnmath-temporal.bn");
}

#[test]
fn former_math_namespace_is_rejected() {
    let source = SourceFile::new(
        "old-math.bn",
        "FUNCTION Start() AS VOID\nPRINT Math.SQRT(9.0)\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let diagnostic = analyze(&program).expect_err("Math is not the 0.1 numeric namespace");
    assert_eq!(diagnostic.code, "NAME_NOT_FOUND");
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
        "tests/grammar/invalid/duplicate-interface.bn",
        "tests/grammar/invalid/incompatible-override.bn",
        "tests/grammar/invalid/inheritance-cycle.bn",
        "tests/grammar/invalid/host-args-assignment.bn",
        "tests/grammar/invalid/host-args-index-assignment.bn",
        "tests/grammar/invalid/host-args-value.bn",
        "tests/grammar/invalid/invalid-stop-code.bn",
        "tests/grammar/invalid/nonvoid-bare-return.bn",
        "tests/grammar/invalid/out-of-range-exit-code.bn",
        "tests/grammar/invalid/private-base-member.bn",
        "tests/grammar/invalid/string-index-assignment.bn",
        "tests/grammar/invalid/super-as-value.bn",
        "tests/grammar/invalid/super-in-destructor.bn",
        "tests/grammar/invalid/super-not-first.bn",
        "tests/grammar/invalid/super-member-as-value.bn",
        "tests/grammar/invalid/uninitialized-pointer.bn",
        "tests/grammar/invalid/static-pointer-uninitialized.bn",
        "tests/grammar/invalid/void-return-value.bn",
        "tests/grammar/invalid/zero-for-step.bn",
        "tests/grammar/invalid/ragged-vector-literal.bn",
        "tests/grammar/invalid/removed-host-memory.bn",
        "tests/grammar/invalid/withdrawn-system-type.bn",
        "tests/grammar/invalid/withdrawn-argument-count.bn",
        "tests/grammar/invalid/withdrawn-argument.bn",
        "tests/grammar/invalid/withdrawn-console-statements.bn",
        "tests/grammar/invalid/float-try-parse.bn",
        "tests/grammar/invalid/len-on-boolean.bn",
        "tests/grammar/invalid/len-on-single-pointer.bn",
        "tests/grammar/invalid/sizeof-function-value.bn",
        "tests/grammar/invalid/beep-on-integer.bn",
        "tests/grammar/invalid/filesystem-unknown-mode.bn",
        "tests/grammar/invalid/filesystem-seek.bn",
        "tests/grammar/invalid/filesystem-directory-api.bn",
        "tests/grammar/invalid/local-vector-negative-dimension.bn",
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
fn host_args_index_assignment_reports_immutability() {
    let error =
        analyze_text("FUNCTION Start() AS VOID\nHOST.Args[0] = \"changed\"\nEND FUNCTION\n")
            .expect_err("HOST.Args entries are immutable");
    assert_eq!(error.code, "TYPE_MISMATCH");
    assert_eq!(error.message, "HOST.Args entries are immutable");
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
fn error_return_narrows_the_rest_of_the_block() {
    let graph = load(Path::new("tests/grammar/valid/error-return-narrow.bn")).expect("load source");
    analyze_modules(&graph).expect("IF IS Error THEN RETURN must narrow");
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
fn static_field_uses_type_default_when_initializer_is_omitted() {
    let source = SourceFile::new(
        "static.bn",
        "CLASS Counter\nPUBLIC STATIC value AS INTEGER\nEND CLASS\nFUNCTION Start() AS VOID\nPRINT Counter.value\nEND FUNCTION\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    analyze(&program).expect("defaultable STATIC INTEGER may omit initializer");
}

#[test]
fn static_pointer_field_still_requires_an_initializer() {
    let source = SourceFile::new(
        "static-pointer.bn",
        "CLASS Holder\nSTATIC slot AS POINTER TO INTEGER\nEND CLASS\n",
    );
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let diagnostic = analyze(&program).expect_err("STATIC POINTER requires an initializer");
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
fn local_vector_dimensions_require_non_negative_integer_values() {
    analyze_text(
        "FUNCTION Start() AS VOID\nLET count AS INTEGER = 0\nLET values AS INTEGER[count]\nEND FUNCTION\n",
    )
    .expect("zero-length local vector is valid");

    for (text, code) in [
        (
            "FUNCTION Start() AS VOID\nLET count AS FLOAT = 2.0\nLET values AS INTEGER[count]\nEND FUNCTION\n",
            "INVALID_VECTOR_DIMENSION",
        ),
        (
            "FUNCTION Start() AS VOID\nLET values AS INTEGER[-1]\nEND FUNCTION\n",
            "INVALID_VECTOR_DIMENSION",
        ),
        (
            "FUNCTION Start() AS VOID\nLET values AS INTEGER[2147483648]\nEND FUNCTION\n",
            "NUMERIC_OVERFLOW",
        ),
    ] {
        assert_eq!(
            analyze_text(text)
                .expect_err("invalid vector dimension must fail")
                .code,
            code
        );
    }
}

#[test]
fn pointer_types_accept_declared_named_elements() {
    analyze_text(
        "CLASS Box\nPUBLIC FUNCTION CONSTRUCTOR()\nEND FUNCTION\nEND CLASS\nFUNCTION Keep(value AS POINTER TO Box) AS POINTER TO Box\nRETURN value\nEND FUNCTION\nFUNCTION Start() AS VOID\nLET value AS POINTER TO Box OR NULL = NULL\nIF value IS POINTER TO Box THEN\nDELETE value\nEND IF\nEND FUNCTION\n",
    )
    .expect("declared named pointer element");

    let error = analyze_text(
        "FUNCTION Start() AS VOID\nLET value AS POINTER TO Missing OR NULL = NULL\nEND FUNCTION\n",
    )
    .expect_err("unknown named pointer element");
    assert_eq!(error.code, "UNKNOWN_TYPE");
}

#[test]
fn void_pointer_has_c_style_conversion_and_requires_a_typed_dereference() {
    analyze_text(
        "FUNCTION Start() AS VOID\nLET typed AS POINTER TO INTEGER = NEW INTEGER\nLET opaque AS POINTER TO VOID = typed\nLET restored AS POINTER TO INTEGER = opaque\nIF opaque IS POINTER TO VOID THEN\nrestored[0] = 1\nEND IF\nDELETE opaque\nEND FUNCTION\n",
    )
    .expect("C-style void pointer conversions");

    let error = analyze_text(
        "FUNCTION Start() AS VOID\nLET typed AS POINTER TO INTEGER = NEW INTEGER\nLET opaque AS POINTER TO VOID = typed\nPRINT opaque[0]\nEND FUNCTION\n",
    )
    .expect_err("opaque pointer indexing");
    assert_eq!(error.code, "TYPE_MISMATCH");
}

#[test]
fn exact_temporal_types_and_host_clock_survive_name_collisions() {
    analyze_text(
        "IMPORT HOST.Clock AS HostClock\nCLASS Clock\nEND CLASS\nFUNCTION Start() AS VOID\nLET timestamp AS TIMESTAMP = HostClock.Now()\nLET date AS DATE = Date.Parse(\"2026-08-22\")\nLET time AS TIME = Time.Parse(\"10:20:30.000\")\nLET zone AS TIMEZONE = TimeZone.Parse(\"America/Sao_Paulo\")\nLET parsed AS TIMESTAMP = Timestamp.Parse(\"2026-08-22T10:20:30.000Z\")\nLET rendered AS STRING = Timestamp.Format(parsed)\nLET elapsed AS INT64 = HostClock.Timer()\nEND FUNCTION\n",
    )
    .expect("temporal namespaces and HOST.Clock must retain exact types");
}

#[test]
fn host_clock_old_member_names_are_rejected() {
    for member in ["Timestamp", "Monotonic"] {
        let source = format!(
            "IMPORT HOST.Clock AS Clock\nFUNCTION Start() AS VOID\nPRINT Clock.{member}()\nEND FUNCTION\n"
        );
        let error = analyze_text(&source).expect_err("old HOST.Clock member must be rejected");
        assert_eq!(error.code, "NAME_NOT_FOUND");
    }
}

#[test]
fn len_and_sizeof_fixture_type_checks() {
    let path = "tests/grammar/valid/len-and-sizeof.bn";
    let source = SourceFile::new(path, fs::read_to_string(path).expect("read fixture"));
    let tokens = lex(&source).expect("lex fixture");
    let program = parse(&tokens).expect("parse fixture");
    analyze(&program).expect("LEN and SIZEOF core fixture must type-check");
}

#[test]
fn len_rejects_a_single_value_pointer() {
    let error = analyze_text(
        "FUNCTION Start() AS VOID\nLET value AS POINTER TO INTEGER = NEW INTEGER\nPRINT LEN(value)\nEND FUNCTION\n",
    )
    .expect_err("LEN requires a pointer region");
    assert_eq!(error.code, "TYPE_MISMATCH");
}

#[test]
fn struct_sizeof_uses_static_instance_layout() {
    let model = analyze_path("tests/grammar/valid/all-constructs.bn");
    assert_eq!(model.layouts.get("Point"), Some(&16));
}

#[test]
fn accepted_frontend_model_contains_no_unknown_type() {
    let model = analyze_path("examples/language-tour.bn");
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
