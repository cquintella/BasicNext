use std::collections::HashMap;

use super::{
    completion::completion_items, find_definition, find_locations, lsp_range, word_prefix,
};
use crate::source::SourceFile;

#[test]
fn spans_are_zero_based_at_the_protocol_boundary() {
    let range = lsp_range(3, 4, 3, 9);
    assert_eq!(range.start.line, 2);
    assert_eq!(range.start.character, 3);
    assert_eq!(range.end.character, 8);
}

#[test]
fn completion_prefix_uses_the_requested_line_and_utf8_chars() {
    let source = SourceFile::new("test.bn", "LET valor AS INTEGER\nPRI");
    assert_eq!(word_prefix(&source, super::Position::new(1, 3)), "PRI");
}

fn labels(source: &str, line: u32, character: u32) -> Vec<String> {
    let uri = "file:///test.bn";
    let file = SourceFile::new(uri, source);
    let documents = HashMap::from([(uri.into(), file)]);
    completion_items(
        documents.get(uri).expect("fixture"),
        super::Position::new(line, character),
        &documents,
        None,
    )
    .into_iter()
    .map(|item| item.label)
    .collect()
}

#[test]
fn completion_filters_keywords_by_prefix() {
    let items = labels("PRI", 0, 3);
    assert!(items.contains(&"PRINT".into()), "{items:?}");
    assert!(items.contains(&"PRIVATE".into()), "{items:?}");
    assert!(!items.contains(&"LET".into()), "{items:?}");
}

#[test]
fn completion_lists_host_capabilities_after_a_dot() {
    let items = labels("PRINT HOST.", 0, 11);
    assert!(items.contains(&"Clock".into()), "{items:?}");
    assert!(items.contains(&"Console".into()), "{items:?}");
    assert!(!items.contains(&"PRINT".into()), "{items:?}");
}

#[test]
fn completion_lists_console_members_on_an_import_alias() {
    let source = "IMPORT HOST.Console AS CON\nFUNCTION Start() AS VOID\nCON.Cl\nEND FUNCTION\n";
    let items = labels(source, 2, 6);
    assert!(items.contains(&"Cls".into()), "{items:?}");
    assert!(!items.contains(&"Beep".into()), "{items:?}");
}

#[test]
fn completion_lists_local_functions() {
    let source = "FUNCTION Add(a AS INTEGER, b AS INTEGER) AS INTEGER\nRETURN a + b\nEND FUNCTION\nFUNCTION Start() AS VOID\nAd\nEND FUNCTION\n";
    let items = labels(source, 4, 2);
    assert!(items.contains(&"Add".into()), "{items:?}");
}

#[test]
fn completion_lists_bnmath_exports_on_alias() {
    let source = "IMPORT BNMath AS Math\nFUNCTION Start() AS VOID\nMath.AB\nEND FUNCTION\n";
    let items = labels(source, 2, 7);
    assert!(items.contains(&"ABS".into()), "{items:?}");
}

#[test]
fn navigation_returns_all_matching_identifier_spans() {
    let uri: super::Uri = "file:///test.bn".parse().unwrap();
    let source = SourceFile::new(uri.to_string(), "LET value AS INTEGER\nPRINT value\n");
    let documents = HashMap::from([(uri.to_string(), source)]);
    let locations = find_locations(&documents, &uri, super::Position::new(1, 11), true).unwrap();
    assert_eq!(locations.len(), 2);
    assert_eq!(locations[0].range, lsp_range(1, 5, 1, 10));
}

#[test]
fn references_can_exclude_the_declaration_span() {
    let uri: super::Uri = "file:///test.bn".parse().unwrap();
    let source = SourceFile::new(uri.to_string(), "LET value AS INTEGER\nPRINT value\n");
    let documents = HashMap::from([(uri.to_string(), source)]);
    let locations = find_locations(&documents, &uri, super::Position::new(1, 11), false).unwrap();
    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].range, lsp_range(2, 7, 2, 12));
}

#[test]
fn definition_uses_ast_declaration_span() {
    let uri: super::Uri = "file:///test.bn".parse().unwrap();
    let source = SourceFile::new(
        uri.to_string(),
        "FUNCTION Value() AS INTEGER\nRETURN 1\nEND FUNCTION\n",
    );
    let documents = HashMap::from([(uri.to_string(), source)]);
    let locations = find_definition(&documents, &uri, super::Position::new(0, 14)).unwrap();
    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].range.start.line, 0);
}

#[test]
fn definition_searches_other_open_documents_when_not_local() {
    let main_uri: super::Uri = "file:///main.bn".parse().unwrap();
    let module_uri: super::Uri = "file:///Module.bn".parse().unwrap();
    let documents = HashMap::from([
        (
            main_uri.to_string(),
            SourceFile::new(
                main_uri.to_string(),
                "IMPORT Module AS Module\nFUNCTION Main() AS INTEGER\nRETURN Value()\nEND FUNCTION\n",
            ),
        ),
        (
            module_uri.to_string(),
            SourceFile::new(
                module_uri.to_string(),
                "FUNCTION Value() AS INTEGER\nRETURN 1\nEND FUNCTION\n",
            ),
        ),
    ]);
    let locations = find_definition(&documents, &main_uri, super::Position::new(2, 12)).unwrap();
    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].uri, module_uri);
}

#[test]
fn definition_loads_sibling_module_from_filesystem() {
    let suffix = std::process::id();
    let main_path = format!("/tmp/basicnext-lsp-main-{suffix}.bn");
    let module_path = "/tmp/Module.bn";
    let main_uri: super::Uri = format!("file://{main_path}").parse().unwrap();
    let module_uri: super::Uri = format!("file://{module_path}").parse().unwrap();
    std::fs::write(
        module_path,
        "FUNCTION Value() AS INTEGER\nRETURN 1\nEND FUNCTION\n",
    )
    .unwrap();
    let documents = HashMap::from([(
        main_uri.to_string(),
        SourceFile::new(
            main_uri.to_string(),
            "IMPORT Module AS Module\nFUNCTION Main() AS INTEGER\nRETURN Value()\nEND FUNCTION\n",
        ),
    )]);
    let locations = find_definition(&documents, &main_uri, super::Position::new(2, 12)).unwrap();
    let _ = std::fs::remove_file(main_path);
    let _ = std::fs::remove_file(module_path);
    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].uri, module_uri);
}
