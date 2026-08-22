use std::path::Path;

use bn::{module_graph::load, semantic::analyze_modules};

#[test]
fn loads_dependencies_before_the_executable_module() {
    let graph = load(Path::new("tests/modules/graph/main.bn")).expect("load module graph");
    assert_eq!(graph.modules.len(), 2);
    assert_eq!(
        graph.modules[usize::try_from(graph.root.0).expect("root index")]
            .program
            .source_name
            .as_deref(),
        Some("tests/modules/graph/main.bn")
    );
    assert_eq!(graph.modules[1].imports, vec![graph.modules[0].id]);
}

#[test]
fn import_cycles_are_source_spanned() {
    let error = load(Path::new("tests/modules/cycle/main.bn")).expect_err("cycle must fail");
    assert_eq!(error.diagnostic.code, "IMPORT_CYCLE");
    assert_eq!(error.source.name, "tests/modules/cycle/A/C.bn");
}

#[test]
fn missing_module_reports_the_importing_source() {
    let error =
        load(Path::new("tests/modules/missing/main.bn")).expect_err("missing module must fail");
    assert_eq!(error.diagnostic.code, "MODULE_NOT_FOUND");
    assert_eq!(error.source.name, "tests/modules/missing/main.bn");
}

#[test]
fn exported_functions_are_available_through_the_import_alias() {
    let graph = load(Path::new("tests/modules/graph/main.bn")).expect("load module graph");
    analyze_modules(&graph).expect("exported function must be callable through its alias");
}

#[test]
fn private_declarations_are_not_visible_through_an_import_alias() {
    let graph = load(Path::new("tests/modules/private/main.bn")).expect("load module graph");
    let error = analyze_modules(&graph).expect_err("private declaration must stay private");
    assert_eq!(error.diagnostic.code, "NAME_NOT_FOUND");
    assert_eq!(error.module, graph.root);
}

#[test]
fn imported_modules_cannot_declare_start() {
    let graph = load(Path::new("tests/modules/imported-start/main.bn")).expect("load graph");
    let error = analyze_modules(&graph).expect_err("imported Start must fail");
    assert_eq!(error.diagnostic.code, "IMPORTED_START");
    assert_ne!(error.module, graph.root);
}

#[test]
fn imported_class_identity_constructor_and_members_are_resolved() {
    let graph = load(Path::new("tests/modules/objects/main.bn")).expect("load object graph");
    let models = analyze_modules(&graph).expect("analyze imported class");
    let root = &models[usize::try_from(graph.root.0).expect("root index")];
    assert!(root.symbols.iter().any(|symbol| matches!(
        symbol.ty,
        bn::semantic::Type::ImportedNamed { ref name, .. } if name == "Box"
    )));
    assert!(root.expressions.iter().any(|expression| {
        expression
            .member_target
            .as_ref()
            .is_some_and(|target| target.module.is_some() && target.name == "CONSTRUCTOR")
    }));
    assert!(root.expressions.iter().any(|expression| {
        expression
            .member_target
            .as_ref()
            .is_some_and(|target| target.module.is_some() && target.name == "Value")
    }));
}

#[test]
fn host_main_is_restricted_to_the_executable_module() {
    let graph = load(Path::new("tests/modules/host-scope/main.bn")).expect("load host graph");
    let error = analyze_modules(&graph).expect_err("imported HOST.main must fail");
    assert_eq!(error.diagnostic.code, "HOST_IMPORT_SCOPE");
    assert_ne!(error.module, graph.root);
}
