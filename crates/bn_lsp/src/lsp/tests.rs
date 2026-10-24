use std::collections::HashMap;

use super::{completion::completion_items, lsp_range, word_prefix};
use bn_diag::Diagnostic;
use bn_source::{Position, Revision, SourceFile, SourceId, Span};

/// Unit tests run with the crate directory as cwd; fixtures and the stdlib
/// live at the workspace root.
const WORKSPACE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn cursor(text: &str, needle: &str, nth: usize) -> super::Position {
    let mut seen = 0usize;
    for (line_index, line) in text.lines().enumerate() {
        if let Some(column) = line.find(needle) {
            if seen == nth {
                return super::Position::new(
                    u32::try_from(line_index).unwrap_or(u32::MAX),
                    u32::try_from(column).unwrap_or(u32::MAX),
                );
            }
            seen += 1;
        }
    }
    panic!("missing {needle} occurrence {nth}");
}

fn line_of(text: &str, needle: &str, nth: usize) -> u32 {
    text.lines()
        .enumerate()
        .filter(|(_, line)| line.contains(needle))
        .nth(nth)
        .map_or_else(
            || panic!("missing {needle}"),
            |(index, _)| u32::try_from(index).unwrap_or(u32::MAX),
        )
}

struct Fixture {
    directory: std::path::PathBuf,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let directory =
            std::env::temp_dir().join(format!("basicnext-lsp-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("create fixture directory");
        Self { directory }
    }

    fn write(&self, relative: &str, text: &str) -> std::path::PathBuf {
        let path = self.directory.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create fixture parent");
        }
        std::fs::write(&path, text).expect("write fixture");
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn documents_for(files: &[(&std::path::PathBuf, &str)]) -> HashMap<String, SourceFile> {
    files
        .iter()
        .map(|(path, text)| {
            let uri = super::file_uri::path_to_file_uri(path);
            (uri.clone(), SourceFile::new(uri, (*text).to_string()))
        })
        .collect()
}

fn analyze(
    entry: &std::path::Path,
    documents: &HashMap<String, SourceFile>,
) -> super::diagnostics::OpenAnalysis {
    let mut session = bn_frontend::frontend_session::FrontendSession::default();
    super::diagnostics::analyze_documents(&entry.to_path_buf(), documents, &mut session)
}

fn same_file(uri: &super::Uri, path: &std::path::Path) -> bool {
    let Some(from_uri) = super::file_uri::file_uri_to_path(uri.as_str()) else {
        return false;
    };
    from_uri.canonicalize().ok().as_deref() == path.canonicalize().ok().as_deref()
}

#[test]
fn spans_are_zero_based_at_the_protocol_boundary() {
    let range = lsp_range(3, 4, 3, 9);
    assert_eq!(range.start.line, 2);
    assert_eq!(range.start.character, 3);
    assert_eq!(range.end.character, 8);
}

#[test]
fn registered_diagnostics_reach_lsp_with_catalog_prose() {
    let diagnostic = Diagnostic {
        code: "E0100",
        message: "expected AS".into(),
        span: Span {
            start: Position {
                source_id: SourceId(1),
                revision: Revision(1),
                offset: 0,
                line: 1,
                column: 1,
            },
            end: Position {
                source_id: SourceId(1),
                revision: Revision(1),
                offset: 3,
                line: 1,
                column: 4,
            },
        },
        structured: None,
    };
    let uri: super::Uri = "file:///main.bn".parse().expect("URI");
    let rendered = super::to_lsp(&diagnostic, &uri);
    assert_eq!(rendered.severity, Some(super::DiagnosticSeverity::ERROR));
    assert_eq!(
        rendered.code,
        Some(super::NumberOrString::String("E0100".into()))
    );
    assert!(rendered.message.starts_with("Syntax error: expected AS"));
}

#[test]
fn warning_diagnostics_reach_lsp_with_warning_severity() {
    let span = Span {
        start: Position {
            source_id: SourceId(1),
            revision: Revision(1),
            offset: 0,
            line: 2,
            column: 5,
        },
        end: Position {
            source_id: SourceId(1),
            revision: Revision(1),
            offset: 5,
            line: 2,
            column: 10,
        },
    };
    let diagnostic = Diagnostic::structured(
        bn_diag::DiagId::UNREACHABLE_CODE,
        vec![(
            "context".into(),
            "statement follows a terminating path".into(),
        )],
        vec![bn_diag::Label {
            span,
            style: bn_diag::LabelStyle::Primary,
            text: None,
        }],
    )
    .expect("structured warning");
    let uri: super::Uri = "file:///main.bn".parse().expect("URI");
    let rendered = super::to_lsp(&diagnostic, &uri);
    assert_eq!(rendered.severity, Some(super::DiagnosticSeverity::WARNING));
    assert_eq!(
        rendered.code,
        Some(super::NumberOrString::String("UNREACHABLE_CODE".into()))
    );
    assert!(
        rendered
            .message
            .starts_with("Unreachable code: This code is unreachable")
    );
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

/// Member completion comes from the frontend's HOST catalog, so it lists what
/// type checking accepts (`TCPStream` members were missing from the old
/// hand-written table).
#[test]
fn completion_lists_host_members_from_the_frontend_catalog() {
    let source =
        "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nFS.File.Read\nEND FUNCTION\n";
    let items = labels(source, 2, 12);
    for member in ["ReadAll", "ReadBytes", "ReadLine"] {
        assert!(items.contains(&member.into()), "{items:?}");
    }
    assert!(!items.contains(&"Write".into()), "{items:?}");
    let source =
        "IMPORT HOST.Net AS Net\nFUNCTION Start() AS VOID\nNet.TCPStream.Set\nEND FUNCTION\n";
    let items = labels(source, 2, 17);
    assert!(items.contains(&"SetTimeouts".into()), "{items:?}");
}

#[test]
fn completion_lists_local_functions() {
    let source = "FUNCTION Add(a AS INTEGER, b AS INTEGER) AS INTEGER\nRETURN a + b\nEND FUNCTION\nFUNCTION Start() AS VOID\nAd\nEND FUNCTION\n";
    let items = labels(source, 4, 2);
    assert!(items.contains(&"Add".into()), "{items:?}");
}

#[test]
fn completion_lists_bnmath_exports_on_alias() {
    // The stdlib is found by walking the document's ancestors for
    // `modules/bn`; place the document inside the workspace.
    let uri = super::file_uri::path_to_file_uri(&std::path::Path::new(WORKSPACE).join("test.bn"));
    let source = "IMPORT BNMath AS Math\nFUNCTION Start() AS VOID\nMath.AB\nEND FUNCTION\n";
    let file = SourceFile::new(uri.clone(), source);
    let documents = HashMap::from([(uri.clone(), file)]);
    let items = completion_items(
        documents.get(&uri).expect("fixture"),
        super::Position::new(2, 7),
        &documents,
        None,
    )
    .into_iter()
    .map(|item| item.label)
    .collect::<Vec<_>>();
    assert!(items.contains(&"ABS".into()), "{items:?}");
}

#[test]
fn references_of_one_binding_include_the_declaration_and_can_omit_it() {
    let source = "\
FUNCTION Start() AS INTEGER\n\
    LET value AS INTEGER = 1\n\
    RETURN value\n\
END FUNCTION\n";
    let fixture = Fixture::new("references");
    let path = fixture.write("main.bn", source);
    let documents = documents_for(&[(&path, source)]);
    let uri = super::file_uri::path_to_file_uri(&path);
    let analysis = analyze(&path, &documents);
    let at = cursor(source, "value", 1);
    let locations = analysis.navigation.references(&uri, at, true);
    assert!(
        locations.len() >= 2,
        "{locations:?} diagnostics {:?}",
        analysis.diagnostics
    );
    assert!(
        locations
            .iter()
            .any(|location| location.range.start.line == line_of(source, "LET value", 0))
    );
    assert!(
        locations
            .iter()
            .any(|location| location.range.start.line == line_of(source, "RETURN value", 0))
    );
    let uses = analysis.navigation.references(&uri, at, false);
    assert!(!uses.is_empty(), "{uses:?}");
    assert!(
        uses.iter()
            .all(|location| location.range.start.line == line_of(source, "RETURN value", 0))
    );
}

#[test]
fn definition_of_a_function_name_uses_its_declaration_span() {
    let source = "\
FUNCTION Value() AS INTEGER\n\
    RETURN 1\n\
END FUNCTION\n";
    let fixture = Fixture::new("function-name");
    let path = fixture.write("main.bn", source);
    let documents = documents_for(&[(&path, source)]);
    let uri = super::file_uri::path_to_file_uri(&path);
    let analysis = analyze(&path, &documents);
    let locations = analysis
        .navigation
        .definition(&uri, cursor(source, "Value", 0));
    assert_eq!(
        locations.len(),
        1,
        "{locations:?} {:?}",
        analysis.diagnostics
    );
    assert_eq!(locations[0].range.start.line, 0);
}

#[test]
fn definition_of_an_imported_function_follows_that_module() {
    let main = "\
IMPORT Module AS Module\n\
FUNCTION Start() AS INTEGER\n\
    RETURN Module.Value()\n\
END FUNCTION\n";
    let module = "\
EXPORT FUNCTION Value() AS INTEGER\n\
    RETURN 1\n\
END FUNCTION\n";
    let decoy = "\
EXPORT FUNCTION Value() AS INTEGER\n\
    RETURN 0\n\
END FUNCTION\n";
    let fixture = Fixture::new("import-open");
    let main_path = fixture.write("main.bn", main);
    let module_path = fixture.write("Module.bn", module);
    let decoy_path = fixture.write("Decoy.bn", decoy);
    let documents = documents_for(&[
        (&main_path, main),
        (&module_path, module),
        (&decoy_path, decoy),
    ]);
    let uri = super::file_uri::path_to_file_uri(&main_path);
    let analysis = analyze(&main_path, &documents);
    let locations = analysis
        .navigation
        .definition(&uri, cursor(main, "Value()", 0));
    assert_eq!(
        locations.len(),
        1,
        "{locations:?} diagnostics {:?}",
        analysis.diagnostics
    );
    assert!(
        same_file(&locations[0].uri, &module_path),
        "{:?}",
        locations[0].uri
    );
}

#[test]
fn definition_loads_an_imported_module_that_is_not_open() {
    let main = "\
IMPORT Module AS Module\n\
FUNCTION Start() AS INTEGER\n\
    RETURN Module.Value()\n\
END FUNCTION\n";
    let module = "\
EXPORT FUNCTION Value() AS INTEGER\n\
    RETURN 1\n\
END FUNCTION\n";
    let fixture = Fixture::new("import-disk");
    let main_path = fixture.write("main.bn", main);
    fixture.write("Module.bn", module);
    let module_path = fixture.directory.join("Module.bn");
    let documents = documents_for(&[(&main_path, main)]);
    let uri = super::file_uri::path_to_file_uri(&main_path);
    let analysis = analyze(&main_path, &documents);
    let locations = analysis
        .navigation
        .definition(&uri, cursor(main, "Value()", 0));
    assert_eq!(
        locations.len(),
        1,
        "{locations:?} {:?}",
        analysis.diagnostics
    );
    assert!(
        same_file(&locations[0].uri, &module_path),
        "{:?}",
        locations[0].uri
    );
}

#[test]
fn diagnostics_use_unsaved_imported_sources_and_the_shared_validated_pipeline() {
    let suffix = std::process::id();
    let directory = std::path::PathBuf::from(format!("/tmp/basicnext-lsp-graph-{suffix}"));
    std::fs::create_dir_all(&directory).expect("create fixture directory");
    let main_path = directory.join("main.bn");
    let module_path = directory.join("Module.bn");
    std::fs::write(
        &main_path,
        "IMPORT Module AS Module\nFUNCTION Start() AS INTEGER\nRETURN Module.Value()\nEND FUNCTION\n",
    )
    .expect("write main fixture");
    std::fs::write(
        &module_path,
        "FUNCTION Value() AS INTEGER\nRETURN 1\nEND FUNCTION\n",
    )
    .expect("write module fixture");
    let main_uri: super::Uri = super::file_uri::path_to_file_uri(&main_path)
        .parse()
        .expect("main URI");
    let module_uri: super::Uri = super::file_uri::path_to_file_uri(&module_path)
        .parse()
        .expect("module URI");
    let mut documents = HashMap::from([
        (
            main_uri.to_string(),
            SourceFile::new(
                main_uri.to_string(),
                "IMPORT Module AS Module\nFUNCTION Start() AS INTEGER\nRETURN Module.Value()\nEND FUNCTION\n",
            ),
        ),
        (
            module_uri.to_string(),
            SourceFile::new(
                module_uri.to_string(),
                "EXPORT FUNCTION Value() AS STRING\nRETURN \"changed\"\nEND FUNCTION\n",
            ),
        ),
    ]);
    let mut session = bn_frontend::frontend_session::FrontendSession::default();
    let diagnostics = super::graph_diagnostics(&main_uri, &documents, &mut session);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].code,
        Some(super::NumberOrString::String("TYPE_MISMATCH".into()))
    );
    documents.insert(
        module_uri.to_string(),
        SourceFile::new(
            module_uri.to_string(),
            "EXPORT FUNCTION Value() AS INTEGER\nRETURN \"bad\"\nEND FUNCTION\n",
        ),
    );
    let module_diagnostics = super::graph_diagnostics(&module_uri, &documents, &mut session);
    assert_eq!(module_diagnostics.len(), 1);
    assert_eq!(
        module_diagnostics[0].code,
        Some(super::NumberOrString::String("TYPE_MISMATCH".into()))
    );
    documents.insert(
        module_uri.to_string(),
        SourceFile::new(
            module_uri.to_string(),
            "EXPORT FUNCTION Value() AS INTEGER\nRETURN 1\nEND FUNCTION\n",
        ),
    );
    assert!(super::graph_diagnostics(&main_uri, &documents, &mut session).is_empty());
}

#[test]
fn lsp_problems_equivalent_to_cli_check_on_shared_fixture() {
    let path = std::path::PathBuf::from(WORKSPACE)
        .join("tests/grammar/invalid/cross-type-equality.bn")
        .canonicalize()
        .expect("canonical path for fixture");
    let text = std::fs::read_to_string(&path).expect("read fixture text");
    let uri: super::Uri = super::file_uri::path_to_file_uri(&path)
        .parse()
        .expect("fixture URI");
    let documents = HashMap::from([(
        uri.to_string(),
        SourceFile::new(uri.to_string(), text.clone()),
    )]);
    let mut session = bn_frontend::frontend_session::FrontendSession::default();
    let lsp_diags = super::graph_diagnostics(&uri, &documents, &mut session);
    assert_eq!(
        lsp_diags.len(),
        1,
        "LSP should produce exactly 1 diagnostic"
    );
    let lsp_code = match &lsp_diags[0].code {
        Some(super::NumberOrString::String(s)) => s.as_str(),
        _ => "",
    };
    assert_eq!(lsp_code, "TYPE_MISMATCH");

    // Verify against the CLI pipeline (load_with_session -> analyze_modules_with_warnings -> lower_graph_validated)
    let mut cli_session = bn_frontend::frontend_session::FrontendSession::default();
    let graph = bn_frontend::module_graph::load_with_session(&path, &mut cli_session)
        .expect("module graph");
    let analysis_err = bn_frontend::semantic::analyze_modules_with_warnings(&graph)
        .expect_err("semantic analysis must fail");
    assert_eq!(analysis_err.diagnostic.code, "TYPE_MISMATCH");
    assert_eq!(
        analysis_err.diagnostic.span.start.line,
        lsp_diags[0].range.start.line as usize + 1
    );
    assert_eq!(
        analysis_err.diagnostic.span.start.column,
        lsp_diags[0].range.start.character as usize + 1
    );
}

#[test]
fn same_name_bindings_in_two_functions_do_not_share_a_definition() {
    let source = "\
FUNCTION Start() AS INTEGER\n\
    LET value AS INTEGER = 1\n\
    RETURN value\n\
END FUNCTION\n\
FUNCTION Other() AS INTEGER\n\
    LET value AS INTEGER = 2\n\
    RETURN value\n\
END FUNCTION\n";
    let fixture = Fixture::new("homonym");
    let path = fixture.write("main.bn", source);
    let documents = documents_for(&[(&path, source)]);
    let uri = super::file_uri::path_to_file_uri(&path);
    let analysis = analyze(&path, &documents);
    let locations = analysis
        .navigation
        .definition(&uri, cursor(source, "value", 3));
    assert_eq!(
        locations.len(),
        1,
        "{locations:?} {:?}",
        analysis.diagnostics
    );
    assert_eq!(
        locations[0].range.start.line,
        line_of(source, "LET value", 1)
    );
    let references = analysis
        .navigation
        .references(&uri, cursor(source, "value", 3), true);
    assert!(references.len() >= 2, "{references:?}");
    assert!(references.iter().all(|location| {
        let line = location.range.start.line;
        line == line_of(source, "LET value", 1) || line == line_of(source, "RETURN value", 1)
    }));
}

#[test]
fn hover_reports_the_resolved_type_name() {
    let source = "\
FUNCTION Start() AS INTEGER\n\
    LET value AS INTEGER = 1\n\
    RETURN value\n\
END FUNCTION\n";
    let fixture = Fixture::new("hover");
    let path = fixture.write("main.bn", source);
    let documents = documents_for(&[(&path, source)]);
    let uri = super::file_uri::path_to_file_uri(&path);
    let analysis = analyze(&path, &documents);
    let hover = analysis
        .navigation
        .hover(&uri, cursor(source, "value", 1))
        .expect("hover");
    assert!(hover.contains("value"), "{hover}");
    assert!(
        hover.contains("INTEGER") || hover.contains("INT32"),
        "{hover}"
    );
    assert!(!hover.contains("Basic Next"), "{hover}");
}

#[test]
fn navigation_is_empty_when_analysis_has_no_model() {
    let source = "\
FUNCTION Start() AS INTEGER\n\
    RETURN missing\n\
END FUNCTION\n";
    let fixture = Fixture::new("unresolved");
    let path = fixture.write("main.bn", source);
    let documents = documents_for(&[(&path, source)]);
    let uri = super::file_uri::path_to_file_uri(&path);
    let analysis = analyze(&path, &documents);
    assert!(
        analysis.diagnostics.values().any(|items| !items.is_empty()),
        "{:?}",
        analysis.diagnostics
    );
    assert!(
        analysis
            .navigation
            .definition(&uri, cursor(source, "missing", 0))
            .is_empty()
    );
    assert!(
        analysis
            .navigation
            .hover(&uri, cursor(source, "missing", 0))
            .is_none()
    );
}

#[test]
fn definition_resolves_nested_module_and_relative_paths() {
    let main = "\
IMPORT nested.Child AS Child\n\
FUNCTION Start() AS INTEGER\n\
    RETURN Child.Compute()\n\
END FUNCTION\n";
    let child = "\
EXPORT FUNCTION Compute() AS INTEGER\n\
    RETURN 42\n\
END FUNCTION\n";
    let fixture = Fixture::new("nested");
    let main_path = fixture.write("main.bn", main);
    let child_path = fixture.write("nested/Child.bn", child);
    let documents = documents_for(&[(&main_path, main)]);
    let uri = super::file_uri::path_to_file_uri(&main_path);
    let analysis = analyze(&main_path, &documents);
    let locations = analysis
        .navigation
        .definition(&uri, cursor(main, "Compute()", 0));
    assert_eq!(
        locations.len(),
        1,
        "{locations:?} {:?}",
        analysis.diagnostics
    );
    assert!(
        same_file(&locations[0].uri, &child_path),
        "{:?}",
        locations[0].uri
    );
}

#[test]
fn lsp_folding_uses_ast_statement_and_declaration_spans() {
    let source_text = "FUNCTION Calc(x AS INTEGER) AS INTEGER\n    IF x > 0 THEN\n        RETURN x\n    END IF\n    RETURN 0\nEND FUNCTION\n";
    let source = SourceFile::new("calc.bn", source_text);
    let ranges = super::folding::compute_folding_ranges(&source);
    assert_eq!(ranges.len(), 2);
    assert_eq!(ranges[0].start_line, 0);
    assert_eq!(ranges[0].end_line, 5);
    assert_eq!(ranges[1].start_line, 1);
    assert_eq!(ranges[1].end_line, 3);
}

#[test]
fn unsupported_methods_respond_with_protocol_not_implemented() {
    let (client, server) = lsp_server::Connection::memory();
    let request = lsp_server::Request::new(
        1.into(),
        "textDocument/formatting".into(),
        serde_json::Value::Null,
    );
    super::respond_unsupported(&server, request).unwrap();
    let response = client.receiver.recv().unwrap();
    let lsp_server::Message::Response(res) = response else {
        panic!()
    };
    assert_eq!(res.id, 1.into());
    assert_eq!(res.response_result.unwrap_err().code, -32601);
}

#[test]
fn diagnostics_and_spans_with_emojis_use_utf16_offsets() {
    let source = SourceFile::new("emoji.bn", "LET 🚀 = 1\n");
    let span = bn_source::Span {
        start: bn_source::Position {
            source_id: source.source_id,
            revision: source.revision,
            offset: 0,
            line: 1,
            column: 5, // on '🚀'
        },
        end: bn_source::Position {
            source_id: source.source_id,
            revision: source.revision,
            offset: 8,
            line: 1,
            column: 6, // after '🚀'
        },
    };
    let range = super::position::span_to_lsp_range(&source, span);
    // In UTF-16, 'LET ' is 4 units; '🚀' is 2 units (surrogate pair)
    assert_eq!(range.start.character, 4);
    assert_eq!(range.end.character, 6);
}
