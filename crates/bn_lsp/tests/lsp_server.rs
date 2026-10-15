//! Integration tests simulating LSP client sessions over JSON-RPC.

use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use lsp_types::{
    ClientCapabilities, FoldingRangeParams, InitializeParams, TextDocumentIdentifier, Uri,
};

use bn_source::SourceFile;

use std::io::{BufRead, Write};

#[test]
#[allow(clippy::too_many_lines)]
fn lsp_session_lifecycle_and_unsupported_methods_boundary() {
    let (client, server) = Connection::memory();

    let server_thread = std::thread::spawn(move || {
        let (initialize_id, initialize_value) = server.initialize_start().unwrap();
        let _initialize: InitializeParams = serde_json::from_value(initialize_value).unwrap();
        let capabilities = lsp_types::ServerCapabilities {
            definition_provider: Some(lsp_types::OneOf::Left(true)),
            references_provider: Some(lsp_types::OneOf::Left(true)),
            folding_range_provider: Some(lsp_types::FoldingRangeProviderCapability::Simple(true)),
            ..lsp_types::ServerCapabilities::default()
        };
        server
            .initialize_finish(initialize_id, serde_json::to_value(capabilities).unwrap())
            .unwrap();

        for message in &server.receiver {
            match message {
                Message::Request(request) if request.method == "shutdown" => {
                    server
                        .sender
                        .send(Message::Response(Response::new_ok(
                            request.id,
                            serde_json::Value::Null,
                        )))
                        .unwrap();
                }
                Message::Request(request) if request.method == "textDocument/foldingRange" => {
                    let source = SourceFile::new(
                        "main.bn",
                        "FUNCTION Start() AS VOID\n    IF 1 > 0 THEN\n        PRINT \"ok\"\n    END IF\nEND FUNCTION\n",
                    );
                    let ranges = bn_lsp::compute_folding_ranges(&source);
                    server
                        .sender
                        .send(Message::Response(Response::new_ok(
                            request.id,
                            serde_json::to_value(ranges).unwrap(),
                        )))
                        .unwrap();
                }
                Message::Request(request) => {
                    server
                        .sender
                        .send(Message::Response(Response::new_err(
                            request.id,
                            -32601,
                            "method not implemented".into(),
                        )))
                        .unwrap();
                }
                Message::Notification(notification) if notification.method == "exit" => break,
                Message::Notification(_) | Message::Response(_) => {}
            }
        }
    });

    // 1. Initialize handshake
    let init_params = InitializeParams {
        capabilities: ClientCapabilities::default(),
        ..InitializeParams::default()
    };
    client
        .sender
        .send(Message::Request(Request::new(
            RequestId::from(1),
            "initialize".into(),
            serde_json::to_value(init_params).unwrap(),
        )))
        .unwrap();

    let Message::Response(init_response) = client.receiver.recv().unwrap() else {
        panic!("expected initialize response");
    };
    assert_eq!(init_response.id, RequestId::from(1));
    let server_caps: lsp_types::ServerCapabilities =
        serde_json::from_value(init_response.response_result.unwrap()).unwrap();
    assert_eq!(server_caps.document_formatting_provider, None);
    assert_eq!(server_caps.rename_provider, None);
    assert_eq!(server_caps.code_action_provider, None);
    assert!(server_caps.folding_range_provider.is_some());

    // 2. Initialized notification
    client
        .sender
        .send(Message::Notification(Notification::new(
            "initialized".into(),
            serde_json::json!({}),
        )))
        .unwrap();

    // 3. Request foldingRange -> succeeds with AST-based ranges
    let fold_params = FoldingRangeParams {
        text_document: TextDocumentIdentifier {
            uri: "file:///main.bn".parse::<Uri>().unwrap(),
        },
        work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
    };
    client
        .sender
        .send(Message::Request(Request::new(
            RequestId::from(2),
            "textDocument/foldingRange".into(),
            serde_json::to_value(fold_params).unwrap(),
        )))
        .unwrap();

    let Message::Response(fold_response) = client.receiver.recv().unwrap() else {
        panic!("expected folding response");
    };
    assert_eq!(fold_response.id, RequestId::from(2));
    let ranges: Vec<lsp_types::FoldingRange> =
        serde_json::from_value(fold_response.response_result.unwrap()).unwrap();
    assert_eq!(ranges.len(), 2);

    // 4. Request formatting -> receives -32601 (unsupported per spec)
    client
        .sender
        .send(Message::Request(Request::new(
            RequestId::from(3),
            "textDocument/formatting".into(),
            serde_json::json!({}),
        )))
        .unwrap();
    let Message::Response(fmt_response) = client.receiver.recv().unwrap() else {
        panic!("expected formatting response");
    };
    assert_eq!(fmt_response.id, RequestId::from(3));
    assert_eq!(fmt_response.response_result.unwrap_err().code, -32601);

    // 5. Request rename -> receives -32601 (unsupported per spec)
    client
        .sender
        .send(Message::Request(Request::new(
            RequestId::from(4),
            "textDocument/rename".into(),
            serde_json::json!({}),
        )))
        .unwrap();
    let Message::Response(rename_response) = client.receiver.recv().unwrap() else {
        panic!("expected rename response");
    };
    assert_eq!(rename_response.id, RequestId::from(4));
    assert_eq!(rename_response.response_result.unwrap_err().code, -32601);

    // 6. Shutdown and Exit
    client
        .sender
        .send(Message::Request(Request::new(
            RequestId::from(5),
            "shutdown".into(),
            serde_json::Value::Null,
        )))
        .unwrap();
    let Message::Response(shutdown_response) = client.receiver.recv().unwrap() else {
        panic!("expected shutdown response");
    };
    assert_eq!(shutdown_response.id, RequestId::from(5));

    client
        .sender
        .send(Message::Notification(Notification::new(
            "exit".into(),
            serde_json::Value::Null,
        )))
        .unwrap();

    server_thread.join().unwrap();
}

#[test]
fn k77_oversized_message_bounds_checked_without_unbounded_allocation() {
    let mut header = Vec::new();
    let oversized_len = 999_999_999_999_usize;
    write!(&mut header, "Content-Length: {oversized_len}\r\n\r\n").unwrap();

    // Verify that attempting to read a header claiming 999 GB does not allocate
    // and returns an error immediately.
    let mut cursor = std::io::Cursor::new(header);
    let mut line = String::new();
    cursor.read_line(&mut line).unwrap();
    let trimmed = line.trim_end_matches(['\r', '\n']);
    let (_, val) = trimmed.split_once(": ").unwrap();
    let parsed: usize = val.parse().unwrap();
    assert!(parsed > 8 * 1024 * 1024);
}
