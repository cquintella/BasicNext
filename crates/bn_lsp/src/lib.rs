//! Language Server Protocol adapter over the shared frontend: document
//! snapshots, diagnostics (≡ `check`), completion, definition and references.
//! Frontend-only; no backend crate is reachable from here.
use std::{collections::HashMap, fs, path::PathBuf};

use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::{
    CompletionItem, CompletionOptions, CompletionParams, CompletionResponse,
    Diagnostic as LspDiagnostic, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, GotoDefinitionParams,
    GotoDefinitionResponse, InitializeParams, Position, ReferenceParams, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};

use bn_frontend::{
    frontend_session::FrontendSession, lexer::lex, parser::parse_named, semantic::analyze,
};
use bn_source::{SourceFile, SourceId};

#[path = "lsp/completion.rs"]
mod completion;
use completion::completion_items;

#[path = "lsp/file_uri.rs"]
mod file_uri;
use file_uri::file_uri_to_path;

#[path = "lsp/position.rs"]
mod position;
pub use position::lsp_pos_to_scalar;

#[path = "lsp/folding.rs"]
mod folding;
pub use folding::compute_folding_ranges;

#[path = "lsp/navigation.rs"]
mod navigation;
use navigation::NavigationSnapshot;

#[path = "lsp/diagnostics.rs"]
mod diagnostics;
use diagnostics::analyze_documents;
pub use diagnostics::diagnostics_for_documents;
#[cfg(test)]
pub(crate) use diagnostics::{graph_diagnostics, to_lsp};
use diagnostics::{lsp_range, publish_diagnostics, uri_to_path};
#[cfg(test)]
pub(crate) use lsp_types::NumberOrString;

const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;

fn bounded_transport() -> (
    Connection,
    std::thread::JoinHandle<std::io::Result<()>>,
    std::thread::JoinHandle<std::io::Result<()>>,
) {
    let (client, server) = Connection::memory();
    let reader = std::thread::Builder::new()
        .spawn(move || {
            let stdin = std::io::stdin();
            let mut input = stdin.lock();
            while let Some(msg) = read_bounded_message(&mut input)? {
                let is_exit = matches!(&msg, Message::Notification(n) if n.method == "exit");
                if client.sender.send(msg).is_err() {
                    break;
                }
                if is_exit {
                    break;
                }
            }
            Ok(())
        })
        .expect("reader");
    let writer = std::thread::Builder::new()
        .spawn(move || {
            let stdout = std::io::stdout();
            let mut output = stdout.lock();
            for msg in client.receiver {
                write_bounded_message(&mut output, &msg)?;
            }
            Ok(())
        })
        .expect("writer");
    (server, reader, writer)
}

fn read_bounded_message(input: &mut impl std::io::BufRead) -> std::io::Result<Option<Message>> {
    let mut length = None;
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        if !line.ends_with("\r\n") {
            return Err(std::io::Error::from(std::io::ErrorKind::InvalidData));
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, val)) = trimmed.split_once(": ")
            && name.eq_ignore_ascii_case("Content-Length")
        {
            let parsed = val
                .trim()
                .parse::<usize>()
                .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidData))?;
            if parsed > MAX_DOCUMENT_BYTES {
                return Err(std::io::Error::from(std::io::ErrorKind::InvalidData));
            }
            length = Some(parsed);
        }
    }
    let Some(length) = length else {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidData));
    };
    let mut payload = vec![0u8; length];
    input.read_exact(&mut payload)?;
    serde_json::from_slice(&payload)
        .map(Some)
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidData))
}

fn write_bounded_message(output: &mut impl std::io::Write, msg: &Message) -> std::io::Result<()> {
    let payload = serde_json::to_vec(msg)
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidData))?;
    write!(output, "Content-Length: {}\r\n\r\n", payload.len())?;
    output.write_all(&payload)?;
    output.flush()
}

/// Runs the bounded LSP service over standard input/output.
///
/// # Errors
///
/// Returns a protocol or I/O error when framing, initialization, or message
/// processing fails.
#[allow(clippy::too_many_lines)]
pub fn run_stdio() -> Result<(), String> {
    let (connection, reader_thread, writer_thread) = bounded_transport();
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
        folding_range_provider: Some(lsp_types::FoldingRangeProviderCapability::Simple(true)),
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
    let mut source_ids = HashMap::<String, SourceId>::new();
    let mut session = FrontendSession::default();
    let mut navigation = NavigationSnapshot::default();
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
                respond_definition(&connection, request, &navigation)?;
            }
            Message::Request(request) if request.method == "textDocument/references" => {
                respond_references(&connection, request, &navigation)?;
            }
            Message::Request(request) if request.method == "textDocument/hover" => {
                respond_hover(&connection, request, &navigation)?;
            }
            Message::Request(request) if request.method == "textDocument/documentSymbol" => {
                respond_document_symbols(&connection, request, &documents)?;
            }
            Message::Request(request) if request.method == "textDocument/foldingRange" => {
                respond_folding_range(&connection, request, &documents)?;
            }
            Message::Request(request) => respond_unsupported(&connection, request)?,
            Message::Notification(notification) if notification.method == "exit" => break,
            Message::Notification(notification) if notification.method == "initialized" => {}
            Message::Notification(notification)
                if matches!(
                    notification.method.as_str(),
                    "textDocument/didOpen" | "textDocument/didChange"
                ) =>
            {
                publish_document_update(
                    &connection,
                    &mut documents,
                    &mut source_ids,
                    &mut session,
                    &mut navigation,
                    notification,
                )?;
            }
            Message::Notification(notification)
                if notification.method == "textDocument/didClose" =>
            {
                let params: DidCloseTextDocumentParams = decode(notification)?;
                let key = params.text_document.uri.to_string();
                documents.remove(&key);
                if let Some(source) = source_ids.remove(&key) {
                    let _ = session.remove(source);
                }
                publish_diagnostics(&connection, params.text_document.uri, Vec::new())?;
            }
            Message::Notification(_) | Message::Response(_) => {}
        }
        if shutdown {
            // LSP requires exit after shutdown; continue consuming until the client sends it.
        }
    }
    drop(connection);
    let _ = reader_thread
        .join()
        .map_err(|error| format!("LSP I/O thread failed: {error:?}"))?;
    let _ = writer_thread.join().map_err(|error| format!("{error:?}"))?;
    Ok(())
}

fn decode<T: serde::de::DeserializeOwned>(notification: Notification) -> Result<T, String> {
    serde_json::from_value(notification.params)
        .map_err(|error| format!("invalid LSP notification: {error}"))
}

fn publish_document_update(
    connection: &Connection,
    documents: &mut HashMap<String, SourceFile>,
    source_ids: &mut HashMap<String, SourceId>,
    session: &mut FrontendSession,
    navigation: &mut NavigationSnapshot,
    notification: Notification,
) -> Result<(), String> {
    if notification.method == "textDocument/didOpen" {
        let params: DidOpenTextDocumentParams = decode(notification)?;
        return publish(
            connection,
            documents,
            source_ids,
            session,
            navigation,
            params.text_document.uri,
            params.text_document.text,
        );
    }
    let params: DidChangeTextDocumentParams = decode(notification)?;
    let Some(change) = params.content_changes.into_iter().last() else {
        return Ok(());
    };
    publish(
        connection,
        documents,
        source_ids,
        session,
        navigation,
        params.text_document.uri,
        change.text,
    )
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
    file_uri_to_path(uri)
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
    navigation: &NavigationSnapshot,
) -> Result<(), String> {
    let params: lsp_types::HoverParams = serde_json::from_value(request.params)
        .map_err(|error| format!("invalid hover params: {error}"))?;
    let uri = params
        .text_document_position_params
        .text_document
        .uri
        .to_string();
    let position = params.text_document_position_params.position;
    let result = navigation.hover(&uri, position).map(|text| {
        serde_json::json!({
            "contents": {"kind": "markdown", "value": text}
        })
    });
    connection
        .sender
        .send(Message::Response(Response::new_ok(
            request.id,
            serde_json::to_value(result).map_err(|error| error.to_string())?,
        )))
        .map_err(|error| error.to_string())
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
                bn_frontend::ast::Item::Declaration { kind, name, span, .. } => Some(serde_json::json!({
                    "name": name,
                    "kind": match kind {
                        bn_frontend::ast::DeclarationKind::Function => 12,
                        bn_frontend::ast::DeclarationKind::Class => 5,
                        bn_frontend::ast::DeclarationKind::Struct => 23,
                        bn_frontend::ast::DeclarationKind::Interface => 11,
                    },
                    "range": lsp_range(span.start.line, span.start.column, span.end.line, span.end.column),
                    "selectionRange": lsp_range(span.start.line, span.start.column, span.end.line, span.end.column)
                })),
                bn_frontend::ast::Item::Import { .. } | bn_frontend::ast::Item::Constant { .. } => None,
            }).collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    connection
        .sender
        .send(Message::Response(Response::new_ok(
            request.id,
            serde_json::Value::Array(symbols),
        )))
        .map_err(|error| error.to_string())
}

fn respond_definition(
    connection: &Connection,
    request: Request,
    navigation: &NavigationSnapshot,
) -> Result<(), String> {
    let params: GotoDefinitionParams = serde_json::from_value(request.params)
        .map_err(|error| format!("invalid definition params: {error}"))?;
    let uri = params
        .text_document_position_params
        .text_document
        .uri
        .to_string();
    let locations = navigation.definition(&uri, params.text_document_position_params.position);
    let result = GotoDefinitionResponse::Array(locations);
    let value = serde_json::to_value(result).map_err(|error| error.to_string())?;
    connection
        .sender
        .send(Message::Response(Response::new_ok(request.id, value)))
        .map_err(|error| error.to_string())
}

fn respond_folding_range(
    connection: &Connection,
    request: Request,
    documents: &HashMap<String, SourceFile>,
) -> Result<(), String> {
    let params: lsp_types::FoldingRangeParams =
        serde_json::from_value(request.params).map_err(|error| error.to_string())?;
    let ranges = if let Some(source) = documents.get(&params.text_document.uri.to_string()) {
        compute_folding_ranges(source)
    } else {
        Vec::new()
    };
    let value = serde_json::to_value(ranges).map_err(|error| error.to_string())?;
    connection
        .sender
        .send(Message::Response(Response::new_ok(request.id, value)))
        .map_err(|error| error.to_string())
}

fn respond_references(
    connection: &Connection,
    request: Request,
    navigation: &NavigationSnapshot,
) -> Result<(), String> {
    let params: ReferenceParams = serde_json::from_value(request.params)
        .map_err(|error| format!("invalid reference params: {error}"))?;
    let uri = params.text_document_position.text_document.uri.to_string();
    let locations = navigation.references(
        &uri,
        params.text_document_position.position,
        params.context.include_declaration,
    );
    let value = serde_json::to_value(locations).map_err(|error| error.to_string())?;
    connection
        .sender
        .send(Message::Response(Response::new_ok(request.id, value)))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
pub(crate) fn word_prefix(source: &SourceFile, position: Position) -> String {
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
    source_ids: &mut HashMap<String, SourceId>,
    session: &mut FrontendSession,
    navigation: &mut NavigationSnapshot,
    uri: Uri,
    text: String,
) -> Result<(), String> {
    let key = uri.to_string();
    let source_id = source_ids.get(&key).copied();
    let snapshot = session.upsert(source_id, text.clone());
    source_ids.insert(key.clone(), snapshot.source);
    let request = session
        .request(snapshot.source)
        .ok_or_else(|| "LSP snapshot became stale before analysis".to_string())?;
    if text.len() > MAX_DOCUMENT_BYTES {
        let diagnostics = vec![LspDiagnostic {
            range: lsp_range(1, 1, 1, 1),
            severity: Some(DiagnosticSeverity::ERROR),
            code: None,
            code_description: None,
            source: Some("bn".into()),
            message: "document exceeds 8 MiB".into(),
            related_information: None,
            tags: None,
            data: None,
        }];
        *navigation = NavigationSnapshot::default();
        if session.accept(request).is_none() {
            return Ok(());
        }
        return publish_diagnostics(connection, uri, diagnostics);
    }
    let source = SourceFile::new(uri.to_string(), text);
    documents.insert(key, source);
    if session.accept(request).is_none() {
        return Ok(());
    }
    let Some(entry) = uri_to_path(&uri) else {
        return Ok(());
    };
    let analyzed = analyze_documents(&entry, documents, session);
    *navigation = analyzed.navigation;
    for (open_uri, diagnostics) in analyzed.diagnostics {
        let open_uri = open_uri
            .parse::<Uri>()
            .map_err(|error| format!("invalid open document URI: {error}"))?;
        publish_diagnostics(connection, open_uri, diagnostics)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "lsp/tests.rs"]
mod tests;
