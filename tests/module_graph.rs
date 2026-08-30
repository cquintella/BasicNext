// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::path::Path;

use bn::{module_graph::load, semantic::analyze_modules};

#[test]
fn loads_dependencies_before_the_executable_module() {
    let graph = load(Path::new("tests/modules/graph/main.bn")).expect("load module graph");
    assert_eq!(graph.modules.len(), 2);
    assert!(
        graph.modules[usize::try_from(graph.root.0).expect("root index")]
            .program
            .source_name
            .as_deref()
            .is_some_and(|name| name.ends_with("tests/modules/graph/main.bn"))
    );
    assert_eq!(graph.modules[1].imports, vec![graph.modules[0].id]);
}

#[test]
fn user_modules_resolve_beneath_modules_directory() {
    let graph = load(Path::new("tests/modules/user-modules/main.bn"))
        .expect("load user module through modules directory");
    assert!(
        graph
            .modules
            .iter()
            .any(|module| module.path.ends_with("MeuMod.bn"))
    );
}

#[test]
fn import_cycles_are_source_spanned() {
    let error = load(Path::new("tests/modules/cycle/main.bn")).expect_err("cycle must fail");
    assert_eq!(error.diagnostic.code, "IMPORT_CYCLE");
    assert!(error.source.name.ends_with("tests/modules/cycle/A/C.bn"));
}

#[test]
fn missing_module_reports_the_importing_source() {
    let error =
        load(Path::new("tests/modules/missing/main.bn")).expect_err("missing module must fail");
    assert_eq!(error.diagnostic.code, "MODULE_NOT_FOUND");
    assert!(error.source.name.ends_with("tests/modules/missing/main.bn"));
}

#[test]
fn exported_functions_are_available_through_the_import_alias() {
    let graph = load(Path::new("tests/modules/graph/main.bn")).expect("load module graph");
    analyze_modules(&graph).expect("exported function must be callable through its alias");
}

#[test]
fn single_identifier_module_is_called_through_its_alias() {
    let graph = load(Path::new("tests/modules/user-alias/main.bn")).expect("load user module");
    analyze_modules(&graph).expect("Meu.Soma must resolve through the import alias");
}

#[test]
fn official_bn_namespace_resolves_under_bn_directory() {
    let graph = load(Path::new("tests/modules/bn-namespace/main.bn")).expect("load BN module");
    assert!(graph.modules.iter().any(|module| {
        module
            .path
            .file_name()
            .is_some_and(|name| name == "Demo.bn")
            && module
                .path
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|name| name == "BN")
    }));
    analyze_modules(&graph).expect("BN.Demo must resolve");
}

#[test]
fn bnmath_exports_come_from_the_module_file() {
    let graph = load(Path::new("tests/grammar/valid/bnmath-02.bn")).expect("load BNMath");
    let models = analyze_modules(&graph).expect("analyze BNMath import");
    assert!(
        graph
            .modules
            .iter()
            .any(|module| module.standard_module == Some(bn::module_graph::StandardModule::BNMath))
    );
    let _ = models;
}

#[test]
fn bnmath_unknown_export_is_rejected() {
    let graph = load(Path::new("tests/grammar/invalid/bnmath-unknown-export.bn"))
        .expect("load unknown BNMath export");
    let error = analyze_modules(&graph).expect_err("unknown BNMath export must fail");
    assert_eq!(error.diagnostic.code, "NAME_NOT_FOUND");
}

#[test]
fn bnmath_api_is_defined_by_its_module_exports() {
    let graph = load(Path::new("tests/modules/bnmath-missing/main.bn"))
        .expect("load reduced BNMath module");
    let error = analyze_modules(&graph).expect_err("missing SQRT export must fail");
    assert_eq!(error.diagnostic.code, "NAME_NOT_FOUND");
    assert!(error.diagnostic.message.contains("SQRT"));
}

#[test]
fn standard_bn_modules_resolve_without_a_hard_coded_name_list() {
    let graph = load(Path::new("tests/modules/bn-standard/main.bn"))
        .expect("load generic standard BN module");
    assert_eq!(graph.modules.len(), 2);
}

#[test]
fn imported_function_is_not_visible_without_its_alias() {
    let graph = load(Path::new("tests/modules/unqualified/main.bn")).expect("load unqualified");
    let error = analyze_modules(&graph).expect_err("unqualified Soma must fail");
    assert_eq!(error.diagnostic.code, "NAME_NOT_FOUND");
    assert_eq!(error.module, graph.root);
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
fn qualified_imported_interface_is_implemented() {
    let graph = load(Path::new("tests/modules/qualified-interface/main.bn"))
        .expect("load qualified interface graph");
    analyze_modules(&graph).expect("qualified interface implementation must resolve");
}

#[test]
fn imported_interfaces_do_not_match_by_last_name_segment() {
    let graph = load(Path::new(
        "tests/modules/imported-interface-collision/main.bn",
    ))
    .expect("load colliding imported interfaces");
    let error = analyze_modules(&graph).expect_err("different imported interfaces must not upcast");
    assert_eq!(error.diagnostic.code, "TYPE_MISMATCH");
}

#[test]
fn host_main_is_restricted_to_the_executable_module() {
    let graph = load(Path::new("tests/modules/host-scope/main.bn")).expect("load host graph");
    let error = analyze_modules(&graph).expect_err("imported HOST.Main must fail");
    assert_eq!(error.diagnostic.code, "HOST_IMPORT_SCOPE");
    assert_ne!(error.module, graph.root);
}

#[test]
fn host_args_is_restricted_to_the_executable_module() {
    let graph =
        load(Path::new("tests/modules/host-args-scope/main.bn")).expect("load host args graph");
    let error = analyze_modules(&graph).expect_err("imported HOST.Args must fail");
    assert_eq!(error.diagnostic.code, "HOST_ARGS_SCOPE");
    assert_ne!(error.module, graph.root);
}
