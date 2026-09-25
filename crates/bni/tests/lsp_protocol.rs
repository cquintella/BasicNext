use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};

use serde_json::{Value, json};

const MESSAGE_TIMEOUT: Duration = Duration::from_secs(5);
static UNIQUE_ID: AtomicU64 = AtomicU64::new(0);

struct LspClient {
    child: Child,
    stdin: Option<ChildStdin>,
    messages: Receiver<Result<Value, String>>,
    reader: Option<thread::JoinHandle<()>>,
}

impl LspClient {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_bni"))
            .arg("lsp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("start bni lsp");
        let stdout = child.stdout.take().expect("LSP stdout pipe");
        let stdin = child.stdin.take().expect("LSP stdin pipe");
        let (sender, messages) = mpsc::channel();
        let reader = thread::spawn(move || {
            let mut stdout = BufReader::new(stdout);
            loop {
                match read_message(&mut stdout) {
                    Ok(Some(message)) => {
                        if sender.send(Ok(message)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        break;
                    }
                }
            }
        });
        Self {
            child,
            stdin: Some(stdin),
            messages,
            reader: Some(reader),
        }
    }

    fn send(&mut self, message: &Value) {
        let payload = serde_json::to_vec(&message).expect("serialize LSP message");
        let stdin = self.stdin.as_mut().expect("open LSP stdin");
        write!(stdin, "Content-Length: {}\r\n\r\n", payload.len()).expect("write LSP header");
        stdin.write_all(&payload).expect("write LSP payload");
        stdin.flush().expect("flush LSP payload");
    }

    fn receive_until(&self, predicate: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + MESSAGE_TIMEOUT;
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .expect("timed out waiting for matching LSP message");
            let message = self
                .messages
                .recv_timeout(remaining)
                .expect("timed out waiting for LSP output")
                .expect("read framed LSP message");
            if predicate(&message) {
                return message;
            }
        }
    }

    fn receive_id(&self, request_id: i64) -> Value {
        self.receive_until(|message| message.get("id").and_then(Value::as_i64) == Some(request_id))
    }

    fn receive_diagnostics(&self, uri: &str, code: Option<&str>) -> Value {
        self.receive_until(|message| {
            if message.get("method").and_then(Value::as_str)
                != Some("textDocument/publishDiagnostics")
            {
                return false;
            }
            let Some(params) = message.get("params") else {
                return false;
            };
            if params.get("uri").and_then(Value::as_str) != Some(uri) {
                return false;
            }
            code.is_none_or(|expected| {
                params["diagnostics"].as_array().is_some_and(|diagnostics| {
                    diagnostics
                        .iter()
                        .any(|item| item.get("code").and_then(Value::as_str) == Some(expected))
                })
            })
        })
    }

    fn shutdown(mut self, request_id: i64) {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "shutdown",
            "params": null
        }));
        assert!(self.receive_id(request_id)["result"].is_null());
        self.send(&json!({"jsonrpc": "2.0", "method": "exit", "params": null}));
        self.stdin.take();
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if self.child.try_wait().expect("poll LSP exit").is_some() {
                break;
            }
            if Instant::now() >= deadline {
                self.child.kill().expect("terminate LSP after exit timeout");
                self.child.wait().expect("reap terminated LSP");
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        if let Some(reader) = self.reader.take() {
            reader.join().expect("join LSP reader");
        }
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        self.stdin.take();
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn read_message(reader: &mut impl BufRead) -> Result<Option<Value>, String> {
    let mut content_length = None;
    loop {
        let mut header = String::new();
        let bytes = reader
            .read_line(&mut header)
            .map_err(|error| error.to_string())?;
        if bytes == 0 {
            return Ok(None);
        }
        let header = header.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("Content-Length")
        {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|error| error.to_string())?,
            );
        }
    }
    let length = content_length.ok_or_else(|| "LSP message lacks Content-Length".to_owned())?;
    let mut payload = vec![0; length];
    reader
        .read_exact(&mut payload)
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&payload)
        .map(Some)
        .map_err(|error| error.to_string())
}

fn initialize(client: &mut LspClient, request_id: i64) -> Value {
    client.send(&json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "method": "initialize",
        "params": {"capabilities": {}}
    }));
    let initialized = client.receive_id(request_id);
    client.send(&json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    initialized
}

fn test_directory() -> PathBuf {
    let id = UNIQUE_ID.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("basic-next-lsp-{}-{id}", std::process::id()));
    fs::create_dir(&path).expect("create LSP test directory");
    path
}

#[test]
fn advertised_requests_and_full_sync_change() {
    let mut client = LspClient::spawn();
    let initialized = initialize(&mut client, 1);
    assert!(initialized["result"].get("completionProvider").is_some());

    let uri = "file:///tmp/basic-next-lsp.bn";
    let text = "FUNCTION Start() AS VOID\n    PRINT \"ok\"\nEND FUNCTION\n";
    client.send(&json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {"textDocument": {"uri": uri, "languageId": "basicnext", "version": 1, "text": text}}
    }));
    client.send(&json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "textDocument/documentSymbol",
        "params": {"textDocument": {"uri": uri}}
    }));
    assert!(client.receive_id(2)["result"].is_array());
    client.send(&json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "textDocument/completion",
        "params": {"textDocument": {"uri": uri}, "position": {"line": 1, "character": 4}}
    }));
    assert!(client.receive_id(3).get("result").is_some());
    client.send(&json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didChange",
        "params": {
            "textDocument": {"uri": uri, "version": 2},
            "contentChanges": [{"text": text.replace("ok", "changed")}]
        }
    }));
    client.send(&json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "textDocument/hover",
        "params": {"textDocument": {"uri": uri}, "position": {"line": 0, "character": 9}}
    }));
    assert!(client.receive_id(4).get("result").is_some());
    client.send(&json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didClose",
        "params": {"textDocument": {"uri": uri}}
    }));
    let closed = client.receive_diagnostics(uri, None);
    assert_eq!(closed["params"]["diagnostics"], json!([]));

    let reopened = "FUNCTION Start() AS INTEGER\n    RETURN \"bad\"\nEND FUNCTION\n";
    client.send(&json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {"textDocument": {"uri": uri, "languageId": "basicnext", "version": 3, "text": reopened}}
    }));
    let diagnostics = client.receive_diagnostics(uri, Some("TYPE_MISMATCH"));
    assert!(
        diagnostics["params"]["diagnostics"]
            .as_array()
            .is_some_and(|items| {
                items
                    .iter()
                    .any(|item| item.get("code").and_then(Value::as_str) == Some("TYPE_MISMATCH"))
            })
    );
    client.shutdown(5);
}

#[test]
fn unsaved_import_change_republishes_dependent_diagnostics() {
    let root = test_directory();
    let main_path = root.join("main.bn");
    let module_path = root.join("Module.bn");
    let main_uri = format!("file://{}", main_path.display());
    let module_uri = format!("file://{}", module_path.display());
    let main_text = "IMPORT Module AS Module\nFUNCTION Start() AS INTEGER\nRETURN Module.Value()\nEND FUNCTION\n";
    let module_text = "EXPORT FUNCTION Value() AS INTEGER\nRETURN 1\nEND FUNCTION\n";
    fs::write(&main_path, main_text).expect("write main LSP fixture");
    fs::write(&module_path, module_text).expect("write module LSP fixture");

    let mut client = LspClient::spawn();
    initialize(&mut client, 1);
    for (uri, text) in [(&main_uri, main_text), (&module_uri, module_text)] {
        client.send(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {"textDocument": {"uri": uri, "languageId": "basicnext", "version": 1, "text": text}}
        }));
    }
    client.send(&json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didChange",
        "params": {
            "textDocument": {"uri": module_uri, "version": 2},
            "contentChanges": [{"text": module_text.replace("RETURN 1", "RETURN \"bad\"")}]
        }
    }));
    let diagnostics = client.receive_diagnostics(&module_uri, Some("TYPE_MISMATCH"));
    let first = &diagnostics["params"]["diagnostics"][0];
    assert_eq!(first["code"], "TYPE_MISMATCH");
    assert_eq!(first["range"]["start"]["line"], 1);
    client.shutdown(2);
    fs::remove_dir_all(root).expect("remove LSP test directory");
}
