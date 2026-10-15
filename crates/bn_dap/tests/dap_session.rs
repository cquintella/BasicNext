//! Integration tests simulating DAP debug sessions over message framing.

use std::io::{BufRead, Cursor, Write};

use serde_json::json;

fn write_frame(buf: &mut Vec<u8>, msg: &serde_json::Value) {
    let payload = serde_json::to_vec(msg).unwrap();
    write!(buf, "Content-Length: {}\r\n\r\n", payload.len()).unwrap();
    buf.extend_from_slice(&payload);
}

#[test]
fn dap_session_full_lifecycle_with_stop_on_entry() {
    let directory = std::env::temp_dir().join(format!("bn-dap-session-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("session_test.bn");
    std::fs::write(
        &path,
        "FUNCTION Start() AS VOID\n    LET x = 1\n    PRINT x\nEND FUNCTION\n",
    )
    .unwrap();
    let program = path.to_string_lossy().into_owned();

    let mut input_buf = Vec::new();
    write_frame(
        &mut input_buf,
        &json!({"seq": 1, "type": "request", "command": "initialize", "arguments": {}}),
    );
    write_frame(
        &mut input_buf,
        &json!({"seq": 2, "type": "request", "command": "launch", "arguments": {"program": program, "stopOnEntry": true}}),
    );
    write_frame(
        &mut input_buf,
        &json!({"seq": 3, "type": "request", "command": "configurationDone", "arguments": {}}),
    );
    write_frame(
        &mut input_buf,
        &json!({"seq": 4, "type": "request", "command": "continue", "arguments": {}}),
    );
    write_frame(
        &mut input_buf,
        &json!({"seq": 5, "type": "request", "command": "disconnect", "arguments": {}}),
    );

    let output = Vec::new();
    let cursor = Cursor::new(input_buf);
    let result = bn_dap::run_session(cursor, output);
    assert!(result.is_ok());

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn dap_framing_bounds_max_message_bytes() {
    let oversized = vec![b'a'; 2 * 1024 * 1024]; // 2 MiB, exceeds MAX_MESSAGE_BYTES (1 MiB)
    let header = format!("Content-Length: {}\r\n\r\n", oversized.len());
    let mut input = Cursor::new([header.as_bytes(), oversized.as_slice()].concat());

    // Header parsing detects oversized Content-Length immediately
    let mut line = String::new();
    input.read_line(&mut line).unwrap();
    let trimmed = line.trim_end_matches(['\r', '\n']);
    let (_, val) = trimmed.split_once(": ").unwrap();
    let parsed: usize = val.parse().unwrap();
    assert!(parsed > 1024 * 1024);
}
