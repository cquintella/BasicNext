//! Diagnostics mapping and multi-file validation for LSP.
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
};

use lsp_server::{Connection, Message, Notification};
use lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, Location,
    NumberOrString, Position, PublishDiagnosticsParams, Uri,
};

use bn_diag::{Catalog, Diagnostic, Severity};
use bn_frontend::{
    lowering::lower_graph_validated,
    module_graph::{load_with_overlays, spell_diagnostic_text},
};
use bn_source::{SourceFile, SourceId};

use crate::file_uri::file_uri_to_path;
use crate::navigation::NavigationSnapshot;
use crate::position::span_to_lsp_range;

/// Collects diagnostics for an open multi-file snapshot set.
///
/// This is the same load → semantic analysis → lowering → language validation
/// pipeline used by the CLI. The returned map is keyed by the URI of the open
/// document that owns each diagnostic, so clients can publish one coherent
/// baseline for every affected buffer.
pub fn diagnostics_for_documents<S: std::hash::BuildHasher>(
    entry: &PathBuf,
    documents: &HashMap<String, SourceFile, S>,
    session: &mut bn_frontend::frontend_session::FrontendSession,
) -> HashMap<String, Vec<LspDiagnostic>> {
    analyze_documents(entry, documents, session).diagnostics
}

pub(crate) struct OpenAnalysis {
    pub diagnostics: HashMap<String, Vec<LspDiagnostic>>,
    pub navigation: NavigationSnapshot,
}

fn without_model(diagnostics: HashMap<String, Vec<LspDiagnostic>>) -> OpenAnalysis {
    OpenAnalysis {
        diagnostics,
        navigation: NavigationSnapshot::default(),
    }
}

pub(crate) fn analyze_documents<S: std::hash::BuildHasher>(
    entry: &PathBuf,
    documents: &HashMap<String, SourceFile, S>,
    session: &mut bn_frontend::frontend_session::FrontendSession,
) -> OpenAnalysis {
    let overlays = documents
        .values()
        .filter_map(|source| {
            uri_to_path_string(&source.name).map(|path| {
                (
                    std::fs::canonicalize(&path).unwrap_or(path),
                    source.text.clone(),
                )
            })
        })
        .collect::<BTreeMap<_, _>>();
    let target = std::fs::canonicalize(entry).unwrap_or_else(|_| entry.clone());
    let target_source = SourceFile::new(target.display().to_string(), "").source_id;
    let mut result = documents
        .keys()
        .filter_map(|key| {
            key.parse::<Uri>()
                .ok()
                .map(|uri| (uri.to_string(), Vec::new()))
        })
        .collect::<HashMap<_, Vec<_>>>();
    let source_uris = documents
        .keys()
        .filter_map(|key| {
            key.parse::<Uri>().ok().and_then(|uri| {
                uri_to_path(&uri).map(|path| {
                    let path = path.canonicalize().unwrap_or(path);
                    (
                        SourceFile::new(path.display().to_string(), "").source_id,
                        uri.to_string(),
                    )
                })
            })
        })
        .collect::<HashMap<_, _>>();
    let add = |diagnostic: &Diagnostic,
               result: &mut HashMap<String, Vec<LspDiagnostic>>,
               graph: Option<&bn_frontend::module_graph::ModuleGraph>| {
        let source = diagnostic_source(diagnostic);
        let uri = source_uris.get(&source).cloned().or_else(|| {
            if source == target_source {
                documents
                    .keys()
                    .find(|key| {
                        uri_to_path_string(key)
                            .is_some_and(|path| path.canonicalize().unwrap_or(path) == target)
                    })
                    .cloned()
            } else {
                None
            }
        });
        if let Some(uri) = uri
            && let Ok(parsed) = uri.parse::<Uri>()
        {
            let doc = documents.get(&uri);
            let mut published = to_lsp_with_source(diagnostic, &parsed, doc);
            if let Some(graph) = graph {
                published.message = spell_diagnostic_text(
                    &published.message,
                    diagnostic.span.start.source_id,
                    graph,
                );
            }
            result.entry(uri).or_default().push(published);
        }
    };
    let graph = match load_with_overlays(entry, session, &overlays) {
        Ok(graph) => graph,
        Err(error) => {
            add(&error.diagnostic, &mut result, None);
            return without_model(result);
        }
    };
    let analysis = match bn_frontend::semantic::analyze_modules_with_warnings(&graph) {
        Ok(analysis) => analysis,
        Err(error) => {
            add(&error.diagnostic, &mut result, Some(&graph));
            return without_model(result);
        }
    };
    if let Err(error) = lower_graph_validated(&graph, &analysis.models) {
        add(&error, &mut result, Some(&graph));
        return OpenAnalysis {
            navigation: NavigationSnapshot::from_graph(&graph, analysis.models, documents),
            diagnostics: result,
        };
    }
    for warning in &analysis.warnings {
        add(&warning.diagnostic, &mut result, Some(&graph));
    }
    OpenAnalysis {
        navigation: NavigationSnapshot::from_graph(&graph, analysis.models, documents),
        diagnostics: result,
    }
}

pub(crate) fn diagnostic_source(diagnostic: &Diagnostic) -> SourceId {
    diagnostic.span.start.source_id
}

pub(crate) fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    uri_to_path_string(uri.as_str())
}

pub(crate) fn uri_to_path_string(uri: &str) -> Option<PathBuf> {
    file_uri_to_path(uri)
}

#[cfg(test)]
pub(crate) fn graph_diagnostics(
    uri: &Uri,
    documents: &HashMap<String, SourceFile>,
    session: &mut bn_frontend::frontend_session::FrontendSession,
) -> Vec<LspDiagnostic> {
    let Some(entry) = uri_to_path(uri) else {
        return Vec::new();
    };
    diagnostics_for_documents(&entry, documents, session)
        .remove(&uri.to_string())
        .unwrap_or_default()
}

#[cfg(test)]
pub(crate) fn to_lsp(error: &Diagnostic, uri: &Uri) -> LspDiagnostic {
    to_lsp_with_source(error, uri, None)
}

pub(crate) fn to_lsp_with_source(
    error: &Diagnostic,
    uri: &Uri,
    source: Option<&SourceFile>,
) -> LspDiagnostic {
    let structured = error
        .spec()
        .and_then(|spec| Catalog::global_for_environment().ok()?.render(&spec).ok());
    let related_information = structured.as_ref().map(|rendered| {
        rendered
            .labels
            .iter()
            .filter(|label| label.style == bn_diag::LabelStyle::Secondary)
            .map(|label| DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range: source.map_or_else(
                        || {
                            lsp_range(
                                label.span.start.line,
                                label.span.start.column,
                                label.span.end.line,
                                label.span.end.column,
                            )
                        },
                        |src| span_to_lsp_range(src, label.span),
                    ),
                },
                message: label.text.clone().unwrap_or_default(),
            })
            .collect::<Vec<_>>()
    });
    let (severity, message) = structured.map_or(
        (DiagnosticSeverity::ERROR, error.message.clone()),
        |rendered| {
            // Legacy diagnostics still carry an intentional compatibility
            // message; catalog rendering supplies the stable title while the
            // original facts remain verbatim until that producer is migrated.
            let rendered_message = if error.structured.is_none() {
                error.message.to_string()
            } else {
                rendered.message.clone()
            };
            let mut message = format!("{}: {}", rendered.title, rendered_message);
            for cause in rendered.causes {
                message.push_str("\n= cause: ");
                message.push_str(&cause);
            }
            if let Some(help) = rendered.help {
                message.push_str("\n= help: ");
                message.push_str(&help);
            }
            let severity = match rendered.severity {
                Severity::Error => DiagnosticSeverity::ERROR,
                Severity::Warning => DiagnosticSeverity::WARNING,
            };
            (severity, message.into())
        },
    );
    let range = source.map_or_else(
        || {
            lsp_range(
                error.span.start.line,
                error.span.start.column,
                error.span.end.line,
                error.span.end.column,
            )
        },
        |src| span_to_lsp_range(src, error.span),
    );
    LspDiagnostic {
        range,
        severity: Some(severity),
        code: Some(NumberOrString::String(error.code.into())),
        code_description: None,
        source: Some("bn".into()),
        message: message.to_string(),
        related_information,
        tags: None,
        data: None,
    }
}

pub(crate) fn lsp_range(
    start_line: usize,
    start_column: usize,
    end_line: usize,
    end_column: usize,
) -> lsp_types::Range {
    lsp_types::Range {
        start: Position::new(
            u32::try_from(start_line.saturating_sub(1)).unwrap_or(u32::MAX),
            u32::try_from(start_column.saturating_sub(1)).unwrap_or(u32::MAX),
        ),
        end: Position::new(
            u32::try_from(end_line.saturating_sub(1)).unwrap_or(u32::MAX),
            u32::try_from(end_column.saturating_sub(1)).unwrap_or(u32::MAX),
        ),
    }
}

pub(crate) fn publish_diagnostics(
    connection: &Connection,
    uri: Uri,
    diagnostics: Vec<LspDiagnostic>,
) -> Result<(), String> {
    let params = PublishDiagnosticsParams {
        uri,
        diagnostics,
        version: None,
    };
    connection
        .sender
        .send(Message::Notification(Notification::new(
            "textDocument/publishDiagnostics".to_string(),
            params,
        )))
        .map_err(|error| error.to_string())
}
