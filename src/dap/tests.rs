use super::{
    SessionState, breakpoint_response, executable_lines_from_source, execute_program, read_message,
    resume_session, validate_launch, write_message,
};
use serde_json::json;
use std::{
    collections::HashMap,
    io::Cursor,
    sync::{Arc, Condvar, Mutex},
};

#[test]
fn framing_round_trips_bounded_json() {
    let payload = serde_json::to_vec(&json!({"seq": 1, "command": "initialize"})).unwrap();
    let framed = format!("Content-Length: {}\r\n\r\n", payload.len());
    let mut input = Cursor::new([framed.as_bytes(), payload.as_slice()].concat());
    assert_eq!(read_message(&mut input).unwrap().unwrap()["seq"], 1);
    let mut output = Vec::new();
    write_message(&mut output, &json!({"ok": true})).unwrap();
    assert!(
        String::from_utf8(output)
            .unwrap()
            .starts_with("Content-Length:")
    );
}

#[test]
fn breakpoint_registry_deduplicates_and_bounds_lines() {
    let mut registry = HashMap::new();
    let response = breakpoint_response(
        &json!({"arguments": {"source": {"path": "main.bn"}, "breakpoints": [
            {"line": 4}, {"line": 4}, {"line": 0}
        ]}}),
        &mut registry,
    );
    assert_eq!(response["breakpoints"].as_array().unwrap().len(), 1);
    assert_eq!(registry["main.bn"].len(), 1);
}

#[test]
fn executable_line_mapping_uses_statement_spans() {
    let lines = executable_lines_from_source(
        "FUNCTION Start() AS VOID\nPRINT \"ok\"\nEND FUNCTION\n",
        "main.bn".into(),
    )
    .unwrap();
    assert!(lines.contains(&2));
    assert!(!lines.contains(&1));
}

#[test]
fn launch_requires_a_bounded_bn_file() {
    assert!(validate_launch(&json!({"arguments": {"program": "missing.txt"}})).is_err());
    assert!(validate_launch(&json!({"arguments": {"program": "missing.bn"}})).is_err());
}

#[test]
fn launch_accepts_a_valid_frontend_program() {
    assert!(validate_launch(&json!({"arguments": {"program": "examples/hello.bn"}})).is_ok());
}

#[test]
fn execution_session_pauses_then_resumes() {
    let session = Arc::new((Mutex::new(SessionState::default()), Condvar::new()));
    let breakpoints = Arc::new(Mutex::new(HashMap::new()));
    let worker_session = Arc::clone(&session);
    let worker_breakpoints = Arc::clone(&breakpoints);
    let worker = std::thread::spawn(move || {
        execute_program("examples/hello.bn", &worker_session, &worker_breakpoints)
    });
    let (lock, condvar) = &*session;
    let mut state = lock.lock().unwrap();
    while !state.paused {
        state = condvar.wait(state).unwrap();
    }
    drop(state);
    resume_session(&session, None);
    assert_eq!(worker.join().unwrap().unwrap(), 0);
}
