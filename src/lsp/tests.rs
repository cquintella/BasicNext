use std::collections::HashMap;

use super::{find_definition, find_locations, lsp_range, word_prefix};
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

#[test]
fn navigation_returns_all_matching_identifier_spans() {
    let uri: super::Uri = "file:///test.bn".parse().unwrap();
    let source = SourceFile::new(uri.to_string(), "LET value AS INTEGER\nPRINT value\n");
    let documents = HashMap::from([(uri.to_string(), source)]);
    let locations = find_locations(&documents, &uri, super::Position::new(1, 11)).unwrap();
    assert_eq!(locations.len(), 2);
    assert_eq!(locations[0].range, lsp_range(1, 5, 1, 10));
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
