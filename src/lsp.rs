use std::{collections::HashMap, fs, path::PathBuf};

use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::{
    CompletionItem, CompletionOptions, CompletionParams, CompletionResponse,
    Diagnostic as LspDiagnostic, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, GotoDefinitionParams,
    GotoDefinitionResponse, InitializeParams, Location, NumberOrString, Position,
    PublishDiagnosticsParams, ReferenceParams, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, Uri,
};

use crate::{lexer::lex, parser::parse_named, semantic::analyze, source::SourceFile};

#[path = "lsp/completion.rs"]
mod completion;
use completion::completion_items;

const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;

/// Runs the bounded LSP service over standard input/output.
///
/// # Errors
///
/// Returns a protocol or I/O error when framing, initialization, or message
/// processing fails.
pub fn run_stdio() -> Result<(), String> {
    let (connection, io_threads) = Connection::stdio();
    let (initialize_id, initialize_value) = connection
        .initialize_start()
        .map_err(|error| format!("LSP initialize read failed: {error}"))?;
    let initialize: InitializeParams = serde_json::from_value(initialize_value)
        .map_err(|error| format!("invalid initialize params: {error}"))?;
    let workspace_root = workspace_root(&initialize);
    let capabilities = ServerCapabilities {
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![".".into()]),
            ..CompletionOptions::default()
        }),
        definition_provider: Some(lsp_types::OneOf::Left(true)),
        references_provider: Some(lsp_types::OneOf::Left(true)),
        hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
        document_symbol_provider: Some(lsp_types::OneOf::Left(true)),
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        ..ServerCapabilities::default()
    };
    connection
        .initialize_finish(
            initialize_id,
            serde_json::to_value(capabilities).map_err(|error| error.to_string())?,
        )
        .map_err(|error| format!("LSP initialize response failed: {error}"))?;

    let mut documents = HashMap::<String, SourceFile>::new();
    let mut shutdown = false;
    for message in &connection.receiver {
        match message {
            Message::Request(request) if request.method == "shutdown" => {
                shutdown = true;
                connection
                    .sender
                    .send(Message::Response(Response::new_ok(
                        request.id,
                        serde_json::Value::Null,
                    )))
                    .map_err(|error| error.to_string())?;
            }
            Message::Request(request) if request.method == "textDocument/completion" => {
                respond_completion(&connection, request, &documents, workspace_root.as_ref())?;
            }
            Message::Request(request) if request.method == "textDocument/definition" => {
                respond_definition(&connection, request, &documents)?;
            }
            Message::Request(request) if request.method == "textDocument/references" => {
                respond_references(&connection, request, &documents)?;
            }
            Message::Request(request) if request.method == "textDocument/hover" => {
                respond_hover(&connection, request, &documents)?;
            }
            Message::Request(request) if request.method == "textDocument/documentSymbol" => {
                respond_document_symbols(&connection, request, &documents)?;
            }
            Message::Request(request) => respond_unsupported(&connection, request)?,
            Message::Notification(notification) if notification.method == "exit" => break,
            Message::Notification(notification) if notification.method == "initialized" => {}
            Message::Notification(notification)
                if notification.method == "textDocument/didOpen" =>
            {
                let params: DidOpenTextDocumentParams = decode(notification)?;
                publish(
                    &connection,
                    &mut documents,
                    params.text_document.uri,
                    params.text_document.text,
                )?;
            }
            Message::Notification(notification)
                if notification.method == "textDocument/didChange" =>
            {
                let params: DidChangeTextDocumentParams = decode(notification)?;
                let Some(change) = params.content_changes.into_iter().next() else {
                    continue;
                };
                publish(
                    &connection,
                    &mut documents,
                    params.text_document.uri,
                    change.text,
                )?;
            }
            Message::Notification(notification)
                if notification.method == "textDocument/didClose" =>
            {
                let params: DidCloseTextDocumentParams = decode(notification)?;
                documents.remove(&params.text_document.uri.to_string());
                publish_diagnostics(&connection, params.text_document.uri, Vec::new())?;
            }
            Message::Notification(_) | Message::Response(_) => {}
        }
        if shutdown {
            // LSP requires exit after shutdown; continue consuming until the client sends it.
        }
    }
    io_threads
        .join()
        .map_err(|error| format!("LSP I/O thread failed: {error:?}"))
}

fn decode<T: serde::de::DeserializeOwned>(notification: Notification) -> Result<T, String> {
    serde_json::from_value(notification.params)
        .map_err(|error| format!("invalid LSP notification: {error}"))
}

fn respond_unsupported(connection: &Connection, request: Request) -> Result<(), String> {
    connection
        .sender
        .send(Message::Response(Response::new_err(
            request.id,
            -32601,
            "method not implemented".to_string(),
        )))
        .map_err(|error| error.to_string())
}

#[allow(deprecated)]
fn workspace_root(params: &InitializeParams) -> Option<PathBuf> {
    let uri = params
        .workspace_folders
        .as_ref()
        .and_then(|folders| folders.first())
        .map(|folder| folder.uri.as_str())
        .or_else(|| params.root_uri.as_ref().map(|uri| uri.as_str()))?;
    uri.strip_prefix("file://").map(PathBuf::from)
}

fn respond_completion(
    connection: &Connection,
    request: Request,
    documents: &HashMap<String, SourceFile>,
    workspace_root: Option<&PathBuf>,
) -> Result<(), String> {
    let params: CompletionParams = serde_json::from_value(request.params)
        .map_err(|error| format!("invalid completion params: {error}"))?;
    let items = documents
        .get(&params.text_document_position.text_document.uri.to_string())
        .map(|source| {
            completion_items(
                source,
                params.text_document_position.position,
                documents,
                workspace_root.map(PathBuf::as_path),
            )
        })
        .unwrap_or_default();
    let result = serde_json::to_value(CompletionResponse::Array(items))
        .map_err(|error| error.to_string())?;
    connection
        .sender
        .send(Message::Response(Response::new_ok(request.id, result)))
        .map_err(|error| error.to_string())
}

fn respond_hover(
    connection: &Connection,
    request: Request,
    documents: &HashMap<String, SourceFile>,
) -> Result<(), String> {
    let params: lsp_types::HoverParams = serde_json::from_value(request.params)
        .map_err(|error| format!("invalid hover params: {error}"))?;
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    let result = documents.get(&uri.to_string()).and_then(|source| {
        let word = word_prefix(source, position);
        (!word.is_empty()).then(|| serde_json::json!({
            "contents": {"kind": "markdown", "value": format!("**{word}** — Basic Next symbol")}
        }))
    });
    connection.sender.send(Message::Response(Response::new_ok(
        request.id,
        serde_json::to_value(result).map_err(|error| error.to_string())?,
    ))).map_err(|error| error.to_string())
}

fn respond_document_symbols(
    connection: &Connection,
    request: Request,
    documents: &HashMap<String, SourceFile>,
) -> Result<(), String> {
    let params: lsp_types::DocumentSymbolParams = serde_json::from_value(request.params)
        .map_err(|error| format!("invalid document symbol params: {error}"))?;
    let symbols = if let Some(source) = documents.get(&params.text_document.uri.to_string()) {
            let tokens = lex(source).map_err(|error| error.message)?;
            let program = parse_named(&tokens, source.name.clone()).map_err(|error| error.message)?;
            program.items.into_iter().filter_map(|item| match item {
                crate::ast::Item::Declaration { kind, name, span, .. } => Some(serde_json::json!({
                    "name": name,
                    "kind": match kind {
                        crate::ast::DeclarationKind::Function => 12,
                        crate::ast::DeclarationKind::Class => 5,
                        crate::ast::DeclarationKind::Struct => 23,
                        crate::ast::DeclarationKind::Interface => 11,
                    },
                    "range": lsp_range(span.start.line, span.start.column, span.end.line, span.end.column),
                    "selectionRange": lsp_range(span.start.line, span.start.column, span.end.line, span.end.column)
                })),
                crate::ast::Item::Import { .. } => None,
            }).collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    connection.sender.send(Message::Response(Response::new_ok(
        request.id,
        serde_json::Value::Array(symbols),
    ))).map_err(|error| error.to_string())
}

fn respond_definition(
    connection: &Connection,
    request: Request,
    documents: &HashMap<String, SourceFile>,
) -> Result<(), String> {
    let params: GotoDefinitionParams = serde_json::from_value(request.params)
        .map_err(|error| format!("invalid definition params: {error}"))?;
    let locations = find_definition(
        documents,
        &params.text_document_position_params.text_document.uri,
        params.text_document_position_params.position,
    )?;
    let result = GotoDefinitionResponse::Array(locations.into_iter().take(1).collect());
    let value = serde_json::to_value(result).map_err(|error| error.to_string())?;
    connection
        .sender
        .send(Message::Response(Response::new_ok(request.id, value)))
        .map_err(|error| error.to_string())
}

fn find_definition(
    documents: &HashMap<String, SourceFile>,
    uri: &Uri,
    position: Position,
) -> Result<Vec<Location>, String> {
    let Some(source) = documents.get(&uri.to_string()) else {
        return Ok(Vec::new());
    };
    let prefix = word_prefix(source, position);
    if prefix.is_empty() {
        return Ok(Vec::new());
    }
    let tokens = lex(source).map_err(|error| error.message)?;
    let program = parse_named(&tokens, source.name.clone()).map_err(|error| error.message)?;
    let imports = program
        .items
        .iter()
        .filter_map(|item| match item {
            crate::ast::Item::Import { path, alias, .. } => Some((path.clone(), alias.clone())),
            crate::ast::Item::Declaration { .. } => None,
        })
        .collect::<Vec<_>>();
    let local = program
        .items
        .into_iter()
        .filter_map(|item| match item {
            crate::ast::Item::Declaration { name, span, .. } if name == prefix => {
                Some(Location::new(
                    uri.clone(),
                    lsp_range(
                        span.start.line,
                        span.start.column,
                        span.end.line,
                        span.end.column,
                    ),
                ))
            }
            _ => None,
        })
        .take(1)
        .collect::<Vec<_>>();
    if !local.is_empty() {
        return Ok(local);
    }
    for (document_uri, document) in documents {
        if document_uri == &uri.to_string() {
            continue;
        }
        if !imports
            .iter()
            .any(|(path, alias)| imported_document_matches(document_uri, path, alias))
        {
            continue;
        }
        let Ok(tokens) = lex(document) else { continue };
        let Ok(other_program) = parse_named(&tokens, document.name.clone()) else {
            continue;
        };
        if let Some(location) = other_program.items.into_iter().find_map(|item| match item {
            crate::ast::Item::Declaration { name, span, .. } if name == prefix => {
                Some(Location::new(
                    document_uri.parse().unwrap_or_else(|_| uri.clone()),
                    lsp_range(
                        span.start.line,
                        span.start.column,
                        span.end.line,
                        span.end.column,
                    ),
                ))
            }
            _ => None,
        }) {
            return Ok(vec![location]);
        }
    }
    for (path, alias) in imports {
        let Some((document_uri, document)) = load_imported_document(uri, &path, &alias) else {
            continue;
        };
        let Ok(tokens) = lex(&document) else { continue };
        let Ok(other_program) = parse_named(&tokens, document.name.clone()) else {
            continue;
        };
        if let Some(location) = other_program.items.into_iter().find_map(|item| match item {
            crate::ast::Item::Declaration { name, span, .. } if name == prefix => {
                Some(Location::new(
                    document_uri.parse().unwrap_or_else(|_| uri.clone()),
                    lsp_range(
                        span.start.line,
                        span.start.column,
                        span.end.line,
                        span.end.column,
                    ),
                ))
            }
            _ => None,
        }) {
            return Ok(vec![location]);
        }
    }
    Ok(Vec::new())
}

fn load_imported_document(
    current_uri: &Uri,
    path: &[String],
    alias: &str,
) -> Option<(String, SourceFile)> {
    let raw_uri = current_uri.to_string();
    let base = raw_uri.strip_prefix("file://")?;
    if path.is_empty()
        || path
            .iter()
            .any(|part| part.is_empty() || part == "." || part == ".." || !valid_module_name(part))
    {
        return None;
    }
    let module = path.last().map_or(alias, String::as_str);
    if !valid_module_name(module) {
        return None;
    }
    let current_path = PathBuf::from(base);
    let candidate = current_path.parent()?.join(format!("{module}.bn"));
    let metadata = fs::metadata(&candidate).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_DOCUMENT_BYTES as u64 {
        return None;
    }
    let text = fs::read_to_string(&candidate).ok()?;
    let document_uri = format!("file://{}", candidate.display());
    Some((document_uri.clone(), SourceFile::new(document_uri, text)))
}

fn valid_module_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn imported_document_matches(uri: &str, path: &[String], alias: &str) -> bool {
    let name = uri
        .rsplit_once('/')
        .map_or(uri, |(_, name)| name)
        .strip_suffix(".bn")
        .unwrap_or(uri);
    let imported_name = path.last().map_or(alias, String::as_str);
    name == imported_name || name == alias
}

fn respond_references(
    connection: &Connection,
    request: Request,
    documents: &HashMap<String, SourceFile>,
) -> Result<(), String> {
    let params: ReferenceParams = serde_json::from_value(request.params)
        .map_err(|error| format!("invalid reference params: {error}"))?;
    let locations = find_locations(
        documents,
        &params.text_document_position.text_document.uri,
        params.text_document_position.position,
    )?;
    let value = serde_json::to_value(locations).map_err(|error| error.to_string())?;
    connection
        .sender
        .send(Message::Response(Response::new_ok(request.id, value)))
        .map_err(|error| error.to_string())
}

fn find_locations(
    documents: &HashMap<String, SourceFile>,
    uri: &Uri,
    position: Position,
) -> Result<Vec<Location>, String> {
    let Some(source) = documents.get(&uri.to_string()) else {
        return Ok(Vec::new());
    };
    let prefix = word_prefix(source, position);
    if prefix.is_empty() {
        return Ok(Vec::new());
    }
    let tokens = lex(source).map_err(|error| error.message)?;
    Ok(tokens
        .into_iter()
        .filter_map(|token| {
            let crate::token::TokenKind::Identifier(name) = token.kind else {
                return None;
            };
            (name == prefix).then(|| {
                Location::new(
                    uri.clone(),
                    lsp_range(
                        token.span.start.line,
                        token.span.start.column,
                        token.span.end.line,
                        token.span.end.column,
                    ),
                )
            })
        })
        .collect())
}

fn word_prefix(source: &SourceFile, position: Position) -> String {
    source
        .text
        .lines()
        .nth(position.line as usize)
        .map(|line| {
            let mut units = 0;
            let prefix = line
                .chars()
                .take_while(|character| {
                    let width = if character.len_utf16() == 1 { 1 } else { 2 };
                    if units + width > position.character {
                        return false;
                    }
                    units += width;
                    true
                })
                .collect::<String>();
            prefix
                .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
                .next_back()
                .unwrap_or_default()
                .to_string()
        })
        .unwrap_or_default()
}

fn publish(
    connection: &Connection,
    documents: &mut HashMap<String, SourceFile>,
    uri: Uri,
    text: String,
) -> Result<(), String> {
    if text.len() > MAX_DOCUMENT_BYTES {
        return publish_diagnostics(
            connection,
            uri,
            vec![LspDiagnostic {
                range: lsp_range(1, 1, 1, 1),
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                code_description: None,
                source: Some("bn".into()),
                message: "document exceeds 8 MiB".into(),
                related_information: None,
                tags: None,
                data: None,
            }],
        );
    }
    let source = SourceFile::new(uri.to_string(), text);
    let diagnostics = match lex(&source) {
        Ok(tokens) => match parse_named(&tokens, source.name.clone()) {
            Ok(program) => analyze(&program).err().into_iter().map(to_lsp).collect(),
            Err(error) => vec![to_lsp(error)],
        },
        Err(error) => vec![to_lsp(error)],
    };
    documents.insert(uri.to_string(), source);
    publish_diagnostics(connection, uri, diagnostics)
}

fn to_lsp(error: crate::diagnostic::Diagnostic) -> LspDiagnostic {
    LspDiagnostic {
        range: lsp_range(
            error.span.start.line,
            error.span.start.column,
            error.span.end.line,
            error.span.end.column,
        ),
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(NumberOrString::String(error.code.into())),
        code_description: None,
        source: Some("bn".into()),
        message: error.message,
        related_information: None,
        tags: None,
        data: None,
    }
}

fn lsp_range(
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

fn publish_diagnostics(
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

#[cfg(test)]
mod tests;
